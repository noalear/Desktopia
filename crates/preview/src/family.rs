//! A family portrait: grown members of a real world's commonest species,
//! side by side, one row per species, to show how far no two bodies of one
//! kind are alike and how far the kinds have come apart.

use crate::acts::{add, bounds};
use crate::living;
use crate::{Args, read_rgba8, target, view, view_at};
use vesper_render::{LifeBatch, OutputConfig, OutputMode, Renderer};
use vesper_sim::creature::Action;
use vesper_sim::frame::CreatureView;
use vesper_sim::pose::{self, Pose};
use vesper_sim::world::DEPTH;

const COLS: usize = 6;
const ROWS: usize = 3;
const CELL: [u32; 2] = [256, 192];
const STAGE: [f32; 2] = [1.6, 0.08];

pub fn render(renderer: &mut Renderer, args: &Args, minutes: f64) {
    let mut sim = vesper_sim::Sim::new(args.seed, 1_800_000_000.0);
    let ticks = (minutes * 60.0 / 0.2) as u64;
    for t in 0..ticks {
        sim.step();
        if t % 3000 == 0 {
            eprintln!("sim {:.0} min: {} creatures", t as f64 * 0.2 / 60.0, sim.world().creatures.len());
        }
    }
    let frame = sim.frame();
    let mut by: std::collections::BTreeMap<u64, Vec<&CreatureView>> = Default::default();
    for c in frame.creatures.iter().filter(|c| c.maturity > 0.85 && c.lost == 0) {
        by.entry(c.species).or_default().push(c);
    }
    let mut kinds: Vec<Vec<&CreatureView>> = by.into_values().collect();
    kinds.sort_by_key(|k| std::cmp::Reverse(k.len()));
    kinds.truncate(ROWS);
    let world_w = 2.0 * crate::MONITOR[0] as f32 / crate::MONITOR[1] as f32;
    let mut sheet = image::RgbaImage::new(CELL[0] * COLS as u32, CELL[1] * kinds.len().max(1) as u32);
    let tex = target(renderer.device(), CELL[0], CELL[1], wgpu::TextureFormat::Rgba8Unorm);
    let (mut batch, mut pose) = (LifeBatch::new(), Pose::default());
    let (mut segs, mut membranes) = (Vec::new(), Vec::new());
    for (row, kind) in kinds.iter().enumerate() {
        eprintln!("species {}: {} grown members", kind[0].species, kind.len());
        // All of one kind at the same scale, so their sizes compare.
        let staged: Vec<CreatureView> = kind.iter().take(COLS).map(|c| stage(c)).collect();
        let mut span = 0.0f32;
        for c in &staged {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            pose::creature(c, 0.0, &mut pose);
            bounds(&pose, &mut lo, &mut hi);
            span = span.max((hi[0] - lo[0]).max((hi[1] - lo[1]) * CELL[0] as f32 / CELL[1] as f32));
        }
        let w = span * 1.3;
        let h = w * CELL[1] as f32 / CELL[0] as f32;
        let ppu = CELL[0] as f32 / w;
        for (col, c) in staged.iter().enumerate() {
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            pose::creature(c, 0.0, &mut pose);
            bounds(&pose, &mut lo, &mut hi);
            let (cx, base) = ((lo[0] + hi[0]) * 0.5, lo[1]);
            let mut out = renderer.create_output(OutputConfig {
                origin_px: [(cx - w * 0.5) * ppu, (1.0 - (base - h * 0.2) - h) * ppu],
                size_px: CELL,
                format: wgpu::TextureFormat::Rgba8Unorm,
                mode: OutputMode::Sdr,
            });
            batch.clear();
            living::convert(&pose, |_| 0.0, &mut segs, &mut membranes);
            add(&mut batch, &segs, &membranes, c.look, pose.z, 1.0, 0.0, c, c.id);
            renderer.set_creatures(&mut batch);
            let world = view_at(args, [world_w * ppu, ppu], args.time);
            renderer.render(&mut out, &world, &view(&tex));
            let px = read_rgba8(renderer, &tex, CELL[0], CELL[1]);
            let (ox, oy) = (col as u32 * CELL[0], row as u32 * CELL[1]);
            for y in 0..CELL[1] {
                for x in 0..CELL[0] {
                    let i = ((y * CELL[0] + x) * 4) as usize;
                    sheet.put_pixel(ox + x, oy + y, image::Rgba([px[i], px[i + 1], px[i + 2], 255]));
                }
            }
        }
    }
    let path = args.out.join(format!("family-{}.png", args.seed));
    sheet.save(&path).unwrap();
    eprintln!("{}", path.display());
}

/// A creature at rest on the staging ground, side on.
fn stage(c: &CreatureView) -> CreatureView {
    let mut c = c.clone();
    let alt = c.pos[2];
    let mut m = c.motion.last().cloned().unwrap_or_default();
    m.face = if m.face < 0.0 { -1.0 } else { 1.0 };
    m.heading = if m.face < 0.0 { std::f32::consts::PI } else { 0.0 };
    m.pos = [STAGE[0], STAGE[1] * DEPTH];
    c.heading = m.heading;
    c.motion = vec![m];
    c.pos = [STAGE[0], STAGE[1], alt];
    c.action = Action::Idle;
    c.target = None;
    c.focus = None;
    c.infection = None;
    c.bleeding = 0.0;
    c.flash = 0.0;
    c
}
