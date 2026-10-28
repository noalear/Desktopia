//! A sheet of behaviours: real creatures from a grown world, each acting out
//! everything the animator knows, drawn close up frame by frame.
//!
//! Each creature gets its own sheet of cells, left to right, top to bottom,
//! in the order of `CELLS`. Bodies hold the pose physics last gave them, so
//! all the movement is the animator's, except dying, where the corpse falls
//! as the real simulation drops it.

use crate::living;
use crate::{Args, read_rgba8, target, view, view_at};
use vesper_render::{Infection, LifeBatch, Look, Membrane, Organism, OutputConfig, OutputMode, Renderer, Seg};
use vesper_sim::animate::Animator;
use vesper_sim::creature::Action;
use vesper_sim::frame::{CreatureView, Frame, InfectionView, RemainsView};
use vesper_sim::history::DeathCause;
use vesper_sim::pose::{self, Pose};
use vesper_sim::world::DEPTH;

pub const CELLS: [&str; 15] = [
    "idle",
    "eating",
    "fighting",
    "defending",
    "threat display",
    "fleeing",
    "sharing food",
    "grooming",
    "mating",
    "laying",
    "sick",
    "starving",
    "old",
    "caught",
    "dying",
];
const COLS: u32 = 5;
const CELL: [u32; 2] = [256, 192];
const FPS: f32 = 15.0;
const TICK: f32 = 0.2;
/// Where on the band the scenes are staged, and how far back.
const STAGE_X: f32 = 1.25;
const STAGE_DEPTH: f32 = 0.08;
/// Seconds a body is held, thrashing, before it dies.
const DIES_AT: f32 = 1.2;

struct Actor {
    base: CreatureView,
    /// Which way its head points on screen.
    dir: f32,
    len: f32,
    /// Its own corpse, tick by tick, as the simulation let it fall.
    corpse: Vec<RemainsView>,
}

pub fn render(renderer: &mut Renderer, args: &Args, minutes: f64) {
    let mut sim = vesper_sim::Sim::new(args.seed, 1_800_000_000.0);
    let ticks = (minutes * 60.0 / TICK as f64) as u64;
    for t in 0..ticks {
        sim.step();
        if t % 3000 == 0 {
            eprintln!("sim {:.0} min: {} creatures", t as f64 * 0.2 / 60.0, sim.world().creatures.len());
        }
    }
    let mut actors = cast(&sim, 3);
    eprintln!("{} actors", actors.len());
    die(&mut sim, &mut actors);

    let mut anim = Animator::new();
    let mut batch = LifeBatch::new();
    let mut pose = Pose::default();
    let (mut segs, mut membranes) = (Vec::new(), Vec::new());
    let frames = args.frames.max(2);
    let rows = (CELLS.len() as u32).div_ceil(COLS);
    let world_w = 2.0 * crate::MONITOR[0] as f32 / crate::MONITOR[1] as f32;

    for (ai, actor) in actors.iter().enumerate() {
        // One window per cell, framed on the scene at rest.
        let mut outs = Vec::new();
        let mut ppus = Vec::new();
        for ci in 0..CELLS.len() {
            let (cs, rs) = scene(actor, ci, ai, 0, 0.0);
            let mut lo = [f32::MAX; 2];
            let mut hi = [f32::MIN; 2];
            for c in &cs {
                pose::creature(c, 0.0, &mut pose);
                bounds(&pose, &mut lo, &mut hi);
            }
            for r in &rs {
                pose::remains(r, 0.0, &mut pose);
                bounds(&pose, &mut lo, &mut hi);
            }
            let span = (hi[0] - lo[0]).max((hi[1] - lo[1]) * CELL[0] as f32 / CELL[1] as f32) * 1.25;
            let w = span.max(actor.len * 2.4);
            let h = w * CELL[1] as f32 / CELL[0] as f32;
            let cx = (lo[0] + hi[0]) * 0.5;
            let cy = (lo[1] + hi[1]) * 0.5;
            let ppu = CELL[0] as f32 / w;
            let x0 = cx - w * 0.5;
            let y0 = (cy - h * 0.45).max(lo[1] - h * 0.25);
            outs.push(renderer.create_output(OutputConfig {
                origin_px: [x0 * ppu, (1.0 - y0 - h) * ppu],
                size_px: CELL,
                format: wgpu::TextureFormat::Rgba8Unorm,
                mode: OutputMode::Sdr,
            }));
            ppus.push(ppu);
        }
        let tex = target(renderer.device(), CELL[0], CELL[1], wgpu::TextureFormat::Rgba8Unorm);
        let mut tick = u64::MAX;
        for f in 0..frames {
            let time = f as f32 / FPS;
            let k = (time / TICK) as u64;
            if k != tick {
                tick = k;
                let mut frame = Frame { tick: k + 1, ..Default::default() };
                for ci in 0..CELLS.len() {
                    frame.creatures.extend(scene(actor, ci, ai, k, time).0);
                }
                anim.update(&frame, time as f64);
            } else {
                anim.update(&Frame { tick: k + 1, ..Default::default() }, time as f64);
            }
            let mut sheet = image::RgbaImage::new(CELL[0] * COLS, CELL[1] * rows);
            for ci in 0..CELLS.len() {
                let (cs, rs) = scene(actor, ci, ai, k, time);
                batch.clear();
                for c in &cs {
                    anim.creature(c, 0.0, &mut pose);
                    living::convert(&pose, |_| 0.0, &mut segs, &mut membranes);
                    add(&mut batch, &segs, &membranes, c.look, pose.z, 1.0, anim.signal(c), c, c.id);
                }
                for r in &rs {
                    anim.remains(r, 0.0, &mut pose);
                    living::convert(&pose, |_| 0.0, &mut segs, &mut membranes);
                    batch.add(&Organism {
                        segs: &segs,
                        membranes: &membranes,
                        look: &Look { genes: r.look },
                        z: pose.z,
                        alive: 1.0 - r.decay,
                        signal: 0.0,
                        symbol: 0,
                        bleeding: 0.0,
                        infection: None,
                        clip_y: None,
                        seed: r.id as u32,
                    });
                }
                renderer.set_creatures(&mut batch);
                let ppu = ppus[ci];
                let world = view_at(args, [world_w * ppu, ppu], args.time);
                renderer.render(&mut outs[ci], &world, &view(&tex));
                let px = read_rgba8(renderer, &tex, CELL[0], CELL[1]);
                let (ox, oy) = ((ci as u32 % COLS) * CELL[0], (ci as u32 / COLS) * CELL[1]);
                for y in 0..CELL[1] {
                    for x in 0..CELL[0] {
                        let i = ((y * CELL[0] + x) * 4) as usize;
                        sheet.put_pixel(ox + x, oy + y, image::Rgba([px[i], px[i + 1], px[i + 2], 255]));
                    }
                }
            }
            let path = args.out.join(format!("acts-{ai}-{f:03}.png"));
            sheet.save(&path).unwrap();
            if f % 15 == 0 {
                eprintln!("{}", path.display());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add(
    batch: &mut LifeBatch,
    segs: &[Seg],
    membranes: &[Membrane],
    look: [f32; 16],
    z: f32,
    alive: f32,
    signal: f32,
    c: &CreatureView,
    id: u64,
) {
    batch.add(&Organism {
        segs,
        membranes,
        look: &Look { genes: look },
        z,
        alive,
        signal,
        symbol: c.symbol,
        bleeding: c.bleeding,
        infection: c.infection.as_ref().map(|i| Infection {
            load: i.load,
            visible: i.visible,
            hue: i.hue,
            symbiont: i.symbiont,
        }),
        clip_y: None,
        seed: id as u32,
    });
}

fn bounds(p: &Pose, lo: &mut [f32; 2], hi: &mut [f32; 2]) {
    for s in &p.segs {
        for q in [s.a, s.b] {
            for i in 0..2 {
                lo[i] = lo[i].min(q[i] - s.ra);
                hi[i] = hi[i].max(q[i] + s.ra);
            }
        }
    }
}

/// Picks up to `n` creatures of different species, the most elaborate first.
fn cast(sim: &vesper_sim::Sim, n: usize) -> Vec<Actor> {
    let frame = sim.frame();
    let mut all: Vec<&CreatureView> = frame.creatures.iter().filter(|c| c.maturity > 0.8).collect();
    all.sort_by_key(|c| std::cmp::Reverse(c.body.segments.len() as i64 * 10 - c.lost.count_ones() as i64 * 30));
    let mut picked: Vec<Actor> = Vec::new();
    for c in all {
        if picked.len() >= n || picked.iter().any(|a| a.base.species == c.species) {
            continue;
        }
        let mut base = c.clone();
        let mut m = c.motion.last().cloned().unwrap_or_default();
        // Square on to us, not caught mid-turn.
        m.face = if m.face < 0.0 { -1.0 } else { 1.0 };
        m.heading = if m.face < 0.0 { std::f32::consts::PI } else { 0.0 };
        base.heading = m.heading;
        base.motion = vec![m];
        base.action = Action::Idle;
        base.target = None;
        base.focus = None;
        base.infection = None;
        base.health = 1.0;
        base.bleeding = 0.0;
        base.energy = 0.8;
        base.age = 0.2;
        base.flash = 0.0;
        let mut pose = Pose::default();
        pose::creature(&base, 0.0, &mut pose);
        let head: Vec<f32> = pose
            .segs
            .iter()
            .filter(|s| s.tissue == vesper_sim::Tissue::Mouth || s.tissue == vesper_sim::Tissue::Sense)
            .map(|s| (s.a[0] + s.b[0]) * 0.5)
            .collect();
        let dir = if head.is_empty() {
            base.motion[0].face.signum()
        } else if head.iter().sum::<f32>() / head.len() as f32 >= pose.at[0] {
            1.0
        } else {
            -1.0
        };
        let len = base.scale * base.body.extent;
        eprintln!(
            "actor species {} ({} segments, {:?}), length {len:.4}",
            c.species,
            c.body.segments.len(),
            c.locomotion
        );
        picked.push(Actor { base, dir, len, corpse: Vec::new() });
    }
    picked
}

/// Kills each actor in the simulation and records its body as it falls.
fn die(sim: &mut vesper_sim::Sim, actors: &mut [Actor]) {
    for a in actors.iter() {
        if let Some(c) = sim.world_mut().creatures.iter_mut().find(|c| c.id.0 == a.base.id) {
            c.doom = Some(DeathCause::Starvation);
        }
    }
    let mut ids: Vec<Option<u64>> = vec![None; actors.len()];
    for _ in 0..60 {
        sim.step();
        let w = sim.world();
        for (i, a) in actors.iter().enumerate() {
            if ids[i].is_none() {
                ids[i] = w.remains.iter().find(|r| r.of.0 == a.base.id).map(|r| r.id);
            }
        }
        let frame = sim.frame();
        for (i, a) in actors.iter_mut().enumerate() {
            if let Some(r) = ids[i].and_then(|id| frame.remains.iter().find(|r| r.id == id)) {
                a.corpse.push(r.clone());
            }
        }
    }
}

/// Places a copy of the actor at `x`, turned round if `flip`, grown to `size`.
fn place(a: &Actor, id: u64, x: f32, flip: bool, size: f32) -> CreatureView {
    let mut c = a.base.clone();
    c.id = id;
    let alt = c.pos[2];
    c.pos = [x, STAGE_DEPTH, alt];
    c.scale *= size;
    if flip {
        c.heading = std::f32::consts::PI - c.heading;
    }
    for m in &mut c.motion {
        m.pos = [x, STAGE_DEPTH * DEPTH];
        m.scale *= size;
        if flip {
            m.face = -m.face;
            m.heading = std::f32::consts::PI - m.heading;
        }
        for p in &mut m.parts {
            p.u *= size;
            p.alt = (p.alt - alt) * size + alt;
        }
    }
    c
}

/// Who is in a cell and what they are doing at tick `k`.
fn scene(a: &Actor, cell: usize, row: usize, k: u64, time: f32) -> (Vec<CreatureView>, Vec<RemainsView>) {
    let id = 1000 + row as u64 * 100 + cell as u64 * 4;
    let x = STAGE_X;
    let partner_x = x + a.dir * a.len * 0.95;
    let mut me = place(a, id, x, false, 1.0);
    let mut other = None;
    let mut remains = Vec::new();
    let kf = k as f32;
    match CELLS[cell] {
        "eating" => {
            me.action = Action::Eating;
            me.focus = Some([x + a.dir * a.len * 0.6, STAGE_DEPTH, 0.0]);
        }
        "fighting" | "defending" => {
            me.action = Action::Attacking;
            let mut o = place(a, id + 1, partner_x, true, 1.0);
            me.target = Some(o.id);
            let guard = CELLS[cell] == "defending";
            o.action = if guard { Action::Guarding } else { Action::Idle };
            let toll = if guard { 0.015 } else { 0.03 };
            o.health = (1.0 - kf * toll).max(0.3);
            o.bleeding = (kf * toll * 1.5).min(0.8);
            other = Some(o);
        }
        "threat display" => me.action = Action::Guarding,
        "fleeing" => me.action = Action::Fleeing,
        "sharing food" | "grooming" => {
            let young = CELLS[cell] == "sharing food";
            let o = place(
                a,
                id + 1,
                x + a.dir * a.len * if young { 0.75 } else { 0.95 },
                true,
                if young { 0.6 } else { 1.0 },
            );
            me.action = if young { Action::Sharing } else { Action::Grooming };
            me.target = Some(o.id);
            other = Some(o);
        }
        "mating" => {
            let o = place(a, id + 1, x + a.dir * a.len * 1.1, true, 0.95);
            if k % 30 == 0 {
                me.action = Action::Mating;
                me.target = Some(o.id);
            }
            other = Some(o);
        }
        "laying" => {
            if k % 25 == 0 {
                me.action = Action::Laying;
            }
        }
        "sick" => {
            me.infection = Some(InfectionView { strain: 1, load: 0.85, visible: 0.15, hue: 0.32, symbiont: false });
            me.health = 0.7;
        }
        "starving" => {
            me.energy = 0.02;
            me.health = 0.6;
        }
        "old" => me.age = 0.97,
        "caught" => {
            me.action = Action::Held;
            me.health = (1.0 - kf * 0.02).max(0.2);
            me.bleeding = (kf * 0.02).min(0.6);
        }
        "dying" => {
            if time < DIES_AT || a.corpse.is_empty() {
                me.action = Action::Held;
                me.health = 0.3;
            } else {
                let i = (((time - DIES_AT) / TICK) as usize).min(a.corpse.len() - 1);
                let mut r = a.corpse[i].clone();
                r.id = id + 2;
                r.age = time - DIES_AT;
                let dx = x - a.corpse[0].pos[0];
                r.pos = [r.pos[0] + dx, STAGE_DEPTH, r.pos[2]];
                for m in &mut r.motion {
                    m.pos = [m.pos[0] + dx, STAGE_DEPTH * DEPTH];
                }
                remains.push(r);
                return (Vec::new(), remains);
            }
        }
        _ => {}
    }
    let mut out = vec![me];
    out.extend(other);
    (out, remains)
}
