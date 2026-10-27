//! Creature bodies and their motion.
//!
//! Each creature is posed fresh every frame in a local frame (x forward, y up,
//! origin on the ground under its middle), then placed in the world. Legs
//! plant their feet with two-bone IK, swimmers send a wave down the spine, and
//! anything that trails is a verlet rope.

use std::f32::consts::{PI, TAU};

use vesper_render::{Look, Membrane, Seg, Tissue};
use vesper_terrain as terrain;

use crate::rope::Rope;
use crate::{Rng, add, len, lerp2, mul, order, perspective, polar, rot, sub};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Plan {
    /// Legged ground forager or hunter: spider to bear to crab.
    Walker,
    /// Small body on very long legs, long neck, works the river edge.
    Wader,
    /// Long body swimming through the thick air on a spine wave.
    Swimmer,
    /// Buoyant bladders trailing a curtain of filaments.
    Floater,
    /// Wide sail wings, rides the high wind.
    Glider,
    /// A worm that rises from its burrow and sinks back.
    Burrower,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mouth {
    Mandibles,
    Proboscis,
    Crown,
    Beak,
    Maw,
}

/// The body plan's numbers. The simulation's genome will grow these for real.
#[derive(Clone, Debug)]
pub struct Form {
    pub plan: Plan,
    /// Trunk length, world units.
    pub size: f32,
    trunk: usize,
    /// Widest radius as a fraction of size.
    girth: f32,
    head: f32,
    widest: f32,
    tail_tip: f32,
    arch: f32,
    tail: usize,
    legs: usize,
    /// Leg reach as a fraction of size.
    leg_len: f32,
    leg_r: f32,
    knee_up: bool,
    tarsus: bool,
    leg_first: f32,
    leg_span: f32,
    neck: usize,
    neck_len: f32,
    armor: f32,
    spines: usize,
    fins: bool,
    tail_fin: f32,
    sail: f32,
    feelers: usize,
    feeler_len: f32,
    tentacles: usize,
    tentacle_len: f32,
    bladders: usize,
    wing: f32,
    mouth: Mouth,
    lure: bool,
    senses: usize,
    gut: bool,
    brood: bool,
    store: bool,
    glow_spots: usize,
    /// Cruising speed in body lengths per second.
    pace: f32,
}

impl Form {
    pub fn random(rng: &mut Rng, plan: Plan) -> Self {
        let mouth = match rng.int(0, 4) {
            0 => Mouth::Mandibles,
            1 => Mouth::Proboscis,
            2 => Mouth::Crown,
            3 => Mouth::Beak,
            _ => Mouth::Maw,
        };
        let mut f = Form {
            plan,
            size: 0.03,
            trunk: rng.int(3, 6),
            girth: rng.range(0.12, 0.24),
            head: rng.range(0.45, 0.9),
            widest: rng.range(0.3, 0.65),
            tail_tip: rng.range(0.15, 0.6),
            arch: rng.range(-0.04, 0.12),
            tail: rng.int(0, 3),
            legs: 0,
            leg_len: 0.6,
            leg_r: rng.range(0.18, 0.4),
            knee_up: rng.chance(0.4),
            tarsus: rng.chance(0.5),
            leg_first: rng.range(0.1, 0.3),
            leg_span: rng.range(0.4, 0.7),
            neck: 0,
            neck_len: 0.0,
            armor: if rng.chance(0.4) { rng.range(0.3, 1.0) } else { 0.0 },
            spines: if rng.chance(0.25) { rng.int(1, 3) } else { 0 },
            fins: false,
            tail_fin: 0.0,
            sail: 0.0,
            feelers: if rng.chance(0.5) { rng.int(1, 2) } else { 0 },
            feeler_len: rng.range(0.3, 0.8),
            tentacles: 0,
            tentacle_len: 0.0,
            bladders: 0,
            wing: 0.0,
            mouth,
            lure: rng.chance(0.15),
            senses: rng.int(1, 5),
            gut: rng.chance(0.8),
            brood: rng.chance(0.4),
            store: rng.chance(0.4),
            glow_spots: if rng.chance(0.4) { rng.int(1, 4) } else { 0 },
            pace: rng.range(0.4, 1.2),
        };
        match plan {
            Plan::Walker => {
                f.size = rng.range(0.025, 0.1);
                f.legs = *[2, 2, 3, 3, 4, 5].get(rng.int(0, 5)).unwrap();
                f.leg_len = if f.knee_up { rng.range(0.7, 1.3) } else { rng.range(0.35, 0.7) };
                if f.knee_up {
                    f.girth *= 0.8;
                    f.leg_r *= 0.6;
                } else if f.size > 0.075 {
                    // Heavy foragers: pillar legs, fewer of them, deep bodies.
                    f.legs = f.legs.min(3);
                    f.leg_len = rng.range(0.6, 0.85);
                    f.leg_r = rng.range(0.28, 0.38);
                    f.girth = rng.range(0.2, 0.28);
                    f.tarsus = false;
                }
            }
            Plan::Wader => {
                f.size = rng.range(0.03, 0.05);
                f.legs = rng.int(1, 2);
                f.leg_len = rng.range(1.6, 2.4);
                f.leg_r = rng.range(0.12, 0.2);
                f.knee_up = false;
                f.neck = rng.int(3, 5);
                f.neck_len = rng.range(0.8, 1.4);
                f.mouth = if rng.chance(0.5) { Mouth::Proboscis } else { Mouth::Beak };
                f.armor *= 0.5;
                f.pace *= 0.5;
            }
            Plan::Swimmer => {
                f.size = rng.range(0.04, 0.11);
                f.trunk = rng.int(6, 11);
                f.girth = rng.range(0.07, 0.15);
                f.legs = if rng.chance(0.4) { rng.int(3, 6) } else { 0 };
                f.leg_len = rng.range(0.12, 0.22);
                f.leg_r = 0.35;
                f.fins = rng.chance(0.7);
                f.tail_fin = rng.range(0.15, 0.4);
                f.sail = if rng.chance(0.4) { rng.range(0.1, 0.3) } else { 0.0 };
                f.feelers = rng.int(0, 3);
                f.feeler_len = rng.range(0.4, 1.0);
                f.tail = 0;
                f.spines = 0;
                f.pace = rng.range(0.5, 1.0);
            }
            Plan::Floater => {
                f.size = rng.range(0.03, 0.07);
                f.bladders = rng.int(2, 6);
                f.tentacles = rng.int(5, 12);
                f.tentacle_len = rng.range(1.0, 3.0);
                f.armor = 0.0;
                f.spines = 0;
                f.glow_spots = rng.int(2, 6);
                f.pace = 0.1;
            }
            Plan::Glider => {
                f.size = rng.range(0.035, 0.07);
                f.trunk = rng.int(3, 5);
                f.girth = rng.range(0.08, 0.14);
                f.wing = rng.range(0.9, 1.6);
                f.feelers = rng.int(1, 3);
                f.feeler_len = rng.range(0.5, 1.2);
                f.spines = 0;
                f.armor *= 0.4;
                f.mouth = if rng.chance(0.5) { Mouth::Beak } else { Mouth::Crown };
                f.pace = rng.range(0.8, 1.5);
            }
            Plan::Burrower => {
                f.size = rng.range(0.05, 0.11);
                f.trunk = rng.int(7, 11);
                f.girth = rng.range(0.1, 0.16);
                f.mouth = Mouth::Crown;
                f.armor = 0.0;
                f.spines = 0;
                f.feelers = 0;
                f.legs = 0;
            }
        }
        f
    }

    fn profile(&self, t: f32) -> f32 {
        // Head radius, swelling to the widest point, tapering to the tail tip.
        if t < self.widest {
            let k = t / self.widest;
            self.head + (1.0 - self.head) * (k * (2.0 - k))
        } else {
            let k = (t - self.widest) / (1.0 - self.widest);
            1.0 + (self.tail_tip - 1.0) * k * k
        }
    }
}

/// Where a body is drawn: origin, facing, scale and tilt, plus the output lists.
struct Pen<'a> {
    o: [f32; 2],
    /// +1 facing the night edge (right), -1 facing the day edge; passes
    /// through 0 while turning, which squeezes the body like a side-on turn.
    fx: f32,
    s: f32,
    tilt: f32,
    segs: &'a mut Vec<Seg>,
    mems: &'a mut Vec<Membrane>,
    /// Rope anchors (world point, world direction) in the order ropes were made.
    anchors: Vec<([f32; 2], [f32; 2])>,
}

impl Pen<'_> {
    fn w(&self, l: [f32; 2]) -> [f32; 2] {
        let r = rot(l, self.tilt);
        [self.o[0] + r[0] * self.fx * self.s, self.o[1] + r[1] * self.s]
    }

    fn dir(&self, l: [f32; 2]) -> [f32; 2] {
        let r = rot(l, self.tilt);
        let v = [r[0] * self.fx.signum(), r[1]];
        let n = len(v).max(1e-6);
        [v[0] / n, v[1] / n]
    }

    fn seg(&mut self, a: [f32; 2], b: [f32; 2], ra: f32, rb: f32, tissue: Tissue, side: f32) {
        self.segs.push(Seg {
            a: self.w(a),
            b: self.w(b),
            ra: ra * self.s,
            rb: rb * self.s,
            tissue,
            damage: 0.0,
            side,
            sway: 0.0,
        });
    }

    /// A chain through local points with radii tapering from r0 to r1.
    fn chain(&mut self, pts: &[[f32; 2]], r0: f32, r1: f32, tissue: Tissue, side: f32) {
        let n = pts.len().saturating_sub(1).max(1) as f32;
        for i in 0..pts.len().saturating_sub(1) {
            let ra = r0 + (r1 - r0) * i as f32 / n;
            let rb = r0 + (r1 - r0) * (i + 1) as f32 / n;
            self.seg(pts[i], pts[i + 1], ra, rb, tissue, side);
        }
    }

    /// A chain already in world space (a rope).
    fn world_chain(&mut self, pts: &[[f32; 2]], r0: f32, r1: f32, tissue: Tissue, side: f32) {
        let n = pts.len().saturating_sub(1).max(1) as f32;
        for i in 0..pts.len().saturating_sub(1) {
            let ra = (r0 + (r1 - r0) * i as f32 / n) * self.s;
            let rb = (r0 + (r1 - r0) * (i + 1) as f32 / n) * self.s;
            self.segs.push(Seg { a: pts[i], b: pts[i + 1], ra, rb, tissue, damage: 0.0, side, sway: 0.0 });
        }
    }

    fn mem(&mut self, a: [f32; 2], b: [f32; 2], c: [f32; 2], tissue: Tissue, side: f32) {
        self.mems.push(Membrane { a: self.w(a), b: self.w(b), c: self.w(c), tissue, side });
    }

    fn anchor(&mut self, l: [f32; 2], d: [f32; 2]) {
        let p = self.w(l);
        let d = self.dir(d);
        self.anchors.push((p, d));
    }
}

/// Two-bone IK: the knee for a limb from `hip` reaching `foot`, bending to
/// the side `bend` (+1 above the line, -1 below).
fn knee(hip: [f32; 2], foot: [f32; 2], l1: f32, l2: f32, bend: f32) -> [f32; 2] {
    let d = sub(foot, hip);
    let dist = len(d).clamp(1e-6, (l1 + l2) * 0.999);
    let cos_a = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
    let base = d[1].atan2(d[0]);
    add(hip, polar(base + bend * cos_a.acos(), l1))
}

pub struct Creature {
    pub form: Form,
    pub look: Look,
    pub seed: u32,
    /// World x of the body's middle.
    pub x: f32,
    /// Height above the floor at its depth (fliers and swimmers), world units.
    pub alt: f32,
    /// 0 front of the ground plane .. 1 back.
    pub depth: f32,
    pub alive: f32,
    facing: f32,
    fx: f32,
    vel: [f32; 2],
    phase: f32,
    time: f32,
    moving: bool,
    timer: f32,
    goal: [f32; 2],
    ropes: Vec<Rope>,
    rng: Rng,
    held: bool,
}

impl Creature {
    pub fn new(form: Form, look: Look, rng: &mut Rng, x: f32, depth: f32) -> Self {
        let alt = match form.plan {
            Plan::Swimmer => rng.range(0.06, 0.4),
            Plan::Floater => rng.range(0.1, 0.45),
            Plan::Glider => rng.range(0.3, 0.6),
            _ => 0.0,
        };
        let facing = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let mut c = Self {
            form,
            look,
            seed: rng.next_u64() as u32,
            x,
            alt,
            depth,
            alive: 1.0,
            facing,
            fx: facing,
            vel: [0.0, 0.0],
            phase: rng.f(),
            time: rng.range(0.0, 100.0),
            moving: rng.chance(0.7),
            timer: rng.range(2.0, 10.0),
            goal: [x, alt],
            ropes: Vec::new(),
            rng: Rng::new(rng.next_u64()),
            held: false,
        };
        c.make_ropes();
        c
    }

    /// Keeps it in place facing right (still animating), for inspection.
    pub fn hold(&mut self) {
        self.held = true;
        self.facing = 1.0;
        self.fx = 1.0;
        self.moving = true;
    }

    /// A corpse: lies where it fell, decaying.
    pub fn dead(mut self, decay: f32) -> Self {
        self.alive = 1.0 - decay;
        self.moving = false;
        self
    }

    fn floor_y(&self) -> f32 {
        terrain::floor_h(self.x, self.depth) + 0.004
    }

    fn origin(&self) -> [f32; 2] {
        [self.x, self.floor_y() + self.alt]
    }

    pub fn z(&self) -> f32 {
        order(self.depth) - 0.002
    }

    pub fn clip_y(&self) -> Option<f32> {
        (self.form.plan == Plan::Burrower).then(|| self.floor_y() - 0.002)
    }

    /// Glow flashes: occasional bursts, like a call.
    pub fn signal(&self) -> f32 {
        if self.alive < 1.0 {
            return 0.0;
        }
        let slot = (self.time / 7.0).floor();
        let h = ((slot * 12.9898 + self.seed as f32 * 0.001).sin() * 43758.5).fract().abs();
        if h < 0.7 {
            return 0.0;
        }
        let local = self.time - slot * 7.0;
        let pulses = (local * 9.0).sin().max(0.0) * (-local * 0.8).exp();
        pulses * (h - 0.7) / 0.3
    }

    fn make_ropes(&mut self) {
        let (mut segs, mut mems) = (Vec::new(), Vec::new());
        let anchors = self.build(&mut segs, &mut mems, true);
        let s = perspective(self.depth);
        self.ropes = anchors
            .iter()
            .enumerate()
            .map(|(i, &(p, d))| {
                let (n, length, gravity, stiff, curl, wave) = self.rope_spec(i);
                let mut r = Rope::new(p, d, n, length * s / n as f32, gravity, stiff);
                r.curl = curl;
                r.wave = wave;
                r
            })
            .collect();
    }

    /// Links, length, gravity, stiffness, curl and ripple of the i-th rope.
    fn rope_spec(&self, i: usize) -> (usize, f32, f32, f32, f32, f32) {
        let f = &self.form;
        let jitter = ((i * 37 + self.seed as usize) % 11) as f32 / 10.0;
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        match f.plan {
            Plan::Floater => {
                (12, f.tentacle_len * f.size * (0.45 + 0.55 * jitter), 0.3, 0.1, side * 0.04 * jitter, 0.3)
            }
            Plan::Glider if i >= f.feelers => (10, f.feeler_len * f.size * 1.2, 0.05, 0.3, 0.06 * side, 0.1),
            _ => (8, f.feeler_len * f.size * (0.7 + 0.3 * jitter), 0.0, 0.35, -0.12 * self.fx.signum(), 0.08),
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        if self.alive < 1.0 {
            self.step_ropes(dt);
            return;
        }
        let (x0, alt0) = (self.x, self.alt);
        let f = &self.form;
        let s = perspective(self.depth);
        let body = f.size * s;
        self.timer -= dt;

        // Turning in place reads as the body narrowing and widening again.
        self.fx += (self.facing - self.fx).clamp(-dt * 1.4, dt * 1.4);
        let turning = (self.fx - self.facing).abs() > 0.01;

        match f.plan {
            Plan::Walker | Plan::Wader => {
                if self.timer <= 0.0 {
                    self.timer = self.rng.range(3.0, 14.0);
                    if self.rng.chance(0.3) {
                        self.facing = -self.facing;
                    } else {
                        self.moving = !self.moving || self.rng.chance(0.6);
                    }
                }
                if self.x < 0.35 {
                    self.facing = 1.0;
                } else if self.x > 3.35 {
                    self.facing = -1.0;
                }
                if self.moving && !turning {
                    let speed = f.pace * body;
                    self.x += self.facing * speed * dt;
                    let stride = self.stride() * s;
                    self.phase = (self.phase + speed * dt / (stride / 0.6)).fract();
                }
                self.vel = [if self.moving { self.facing * f.pace * body } else { 0.0 }, 0.0];
            }
            Plan::Swimmer | Plan::Glider => {
                if self.timer <= 0.0 || len(sub(self.goal, [self.x, self.alt])) < body {
                    self.timer = self.rng.range(4.0, 12.0);
                    let reach = if f.plan == Plan::Glider { 0.7 } else { 0.35 };
                    let lo = if f.plan == Plan::Glider { 0.28 } else { 0.05 };
                    self.goal =
                        [(self.x + self.rng.range(-reach, reach)).clamp(0.4, 3.2), self.rng.range(lo, lo + 0.35)];
                }
                let to = sub(self.goal, [self.x, self.alt]);
                let want = mul(to, f.pace * body * 1.5 / len(to).max(1e-5));
                self.vel = lerp2(self.vel, want, (dt * 0.6).min(1.0));
                if self.vel[0].abs() > body * 0.05 {
                    self.facing = self.vel[0].signum();
                }
                let spd = if turning { 0.3 } else { 1.0 };
                self.x += self.vel[0] * dt * spd;
                self.alt = (self.alt + self.vel[1] * dt * spd).max(0.03);
                let hz = if f.plan == Plan::Glider { 0.35 } else { 0.5 + f.pace * 0.6 };
                self.phase = (self.phase + dt * hz).fract();
            }
            Plan::Floater => {
                // Carried by the wind: day-ward low, night-ward high.
                let wind = if self.alt > 0.3 { 0.004 } else { -0.003 };
                self.vel = [wind, 0.002 * (self.time * 0.2 + self.seed as f32).sin()];
                self.x += self.vel[0] * dt;
                self.alt += self.vel[1] * dt;
                if self.x < 0.4 || self.x > 3.2 {
                    self.alt += 0.01 * dt * if self.x < 0.4 { 1.0 } else { -1.0 };
                    self.x = self.x.clamp(0.4, 3.2);
                }
                self.phase = (self.phase + dt * 0.25).fract();
            }
            Plan::Burrower => {
                // Rise, sway, sink, wait.
                self.phase = (self.phase + dt / 40.0).fract();
            }
        }
        if self.held {
            self.x = x0;
            self.alt = alt0;
            self.facing = 1.0;
            self.fx = 1.0;
            self.moving = true;
            self.timer = 10.0;
            if matches!(self.form.plan, Plan::Swimmer | Plan::Glider) {
                self.vel = [self.form.pace * body, 0.0];
            }
        }
        self.step_ropes(dt);
    }

    fn step_ropes(&mut self, dt: f32) {
        if self.ropes.is_empty() {
            return;
        }
        let (mut segs, mut mems) = (Vec::new(), Vec::new());
        let anchors = self.build(&mut segs, &mut mems, true);
        let drift = mul(self.vel, -1.0);
        for (r, (p, d)) in self.ropes.iter_mut().zip(anchors) {
            r.step(p, d, drift, dt);
        }
    }

    fn stride(&self) -> f32 {
        self.form.size * self.form.leg_len * 0.55
    }

    pub fn emit(&self, segs: &mut Vec<Seg>, mems: &mut Vec<Membrane>) {
        self.build(segs, mems, false);
    }

    /// Poses the body. Returns rope anchors; ropes are drawn unless `anchors_only`.
    fn build(&self, segs: &mut Vec<Seg>, mems: &mut Vec<Membrane>, anchors_only: bool) -> Vec<([f32; 2], [f32; 2])> {
        let s = perspective(self.depth);
        let tilt = match self.form.plan {
            Plan::Swimmer | Plan::Glider => {
                let sp = len(self.vel).max(1e-5);
                (self.vel[1] / sp).clamp(-0.5, 0.5) * 0.7 * self.fx.signum()
            }
            _ => 0.0,
        };
        let mut pen = Pen { o: self.origin(), fx: self.fx, s, tilt, segs, mems, anchors: Vec::new() };
        // Fully side-on turning bodies still need some width.
        if pen.fx.abs() < 0.15 {
            pen.fx = 0.15 * if pen.fx < 0.0 { -1.0 } else { 1.0 };
        }
        match self.form.plan {
            Plan::Walker | Plan::Wader => self.walker(&mut pen),
            Plan::Swimmer => self.swimmer(&mut pen),
            Plan::Floater => self.floater(&mut pen),
            Plan::Glider => self.glider(&mut pen),
            Plan::Burrower => self.burrower(&mut pen),
        }
        if !anchors_only {
            let f = &self.form;
            for (i, r) in self.ropes.iter().enumerate() {
                let (r0, r1, tissue) = match f.plan {
                    Plan::Floater => (f.size * 0.016, f.size * 0.004, Tissue::Frame),
                    _ => (f.size * f.girth * 0.1, f.size * 0.004, Tissue::Frame),
                };
                pen.world_chain(&r.pts, r0, r1, tissue, if i % 2 == 0 { 0.5 } else { -0.5 });
                // Glowing beads at the tips of a floater's longest filaments, and lures.
                if f.plan == Plan::Floater && i % 3 == 0 {
                    let tip = *r.pts.last().unwrap();
                    pen.segs.push(Seg {
                        a: tip,
                        b: tip,
                        ra: f.size * 0.03 * s,
                        rb: f.size * 0.03 * s,
                        tissue: Tissue::Glow,
                        damage: 0.0,
                        side: 0.6,
                        sway: 0.0,
                    });
                }
            }
        }
        pen.anchors
    }

    fn spine(&self, lift: f32, wave: impl Fn(f32) -> f32) -> Vec<[f32; 2]> {
        let f = &self.form;
        (0..=f.trunk)
            .map(|i| {
                let t = i as f32 / f.trunk as f32;
                [f.size * (0.5 - t), lift + f.arch * f.size * (PI * t).sin() + wave(t)]
            })
            .collect()
    }

    fn radius(&self, t: f32) -> f32 {
        self.form.size * self.form.girth * self.form.profile(t)
    }

    fn trunk(&self, pen: &mut Pen, sp: &[[f32; 2]]) {
        let f = &self.form;
        let n = sp.len() - 1;
        for i in 0..n {
            let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            pen.seg(sp[i], sp[i + 1], self.radius(t0), self.radius(t1), Tissue::Muscle, 0.0);
        }
        // Organs, seen through translucent hides.
        let at = |t: f32| {
            let x = t * n as f32;
            let i = (x.floor() as usize).min(n - 1);
            lerp2(sp[i], sp[i + 1], x - i as f32)
        };
        if f.gut {
            pen.seg(at(0.15), at(0.75), self.radius(0.2) * 0.35, self.radius(0.7) * 0.3, Tissue::Gut, 0.0);
        }
        if f.store {
            pen.seg(at(0.35), at(0.5), self.radius(0.4) * 0.45, self.radius(0.5) * 0.4, Tissue::Store, 0.0);
        }
        if f.brood {
            pen.seg(at(0.62), at(0.85), self.radius(0.7) * 0.5, self.radius(0.85) * 0.35, Tissue::Brood, 0.0);
        }
        // Shell plates over the back, or one dome for the heaviest armour.
        if f.armor > 0.75 && f.plan == Plan::Walker {
            let mid = at(f.widest);
            let r = self.radius(f.widest) * 1.15;
            pen.seg(add(at(0.12), [0.0, r * 0.25]), add(mid, [0.0, r * 0.35]), r * 0.8, r, Tissue::Armor, 0.2);
            pen.seg(add(mid, [0.0, r * 0.35]), add(at(0.9), [0.0, r * 0.15]), r, r * 0.65, Tissue::Armor, 0.2);
        } else if f.armor > 0.0 {
            let plates = ((n as f32) * f.armor).ceil() as usize;
            for i in 0..plates.min(n) {
                let (t0, t1) = (i as f32 / n as f32, (i as f32 + 1.1) / n as f32);
                let (r0, r1) = (self.radius(t0), self.radius(t1.min(1.0)));
                pen.seg(
                    add(at(t0), [0.0, r0 * 0.45]),
                    add(at(t1.min(1.0)), [0.0, r1 * 0.45]),
                    r0 * 0.7,
                    r1 * 0.7,
                    Tissue::Armor,
                    0.15,
                );
            }
        }
        for k in 0..f.spines {
            let t = 0.3 + 0.5 * (k as f32 + 0.5) / f.spines as f32;
            let base = add(at(t), [0.0, self.radius(t) * 0.8]);
            let tip = add(base, [-f.size * 0.08, f.size * (0.12 + 0.05 * k as f32)]);
            pen.seg(base, tip, self.radius(t) * 0.22, 0.0, Tissue::Spine, 0.1);
        }
        for k in 0..f.glow_spots {
            let t = 0.2 + 0.6 * (k as f32 + 0.5) / f.glow_spots as f32;
            let p = add(at(t), [0.0, -self.radius(t) * 0.4]);
            pen.seg(p, p, self.radius(t) * 0.16, self.radius(t) * 0.16, Tissue::Glow, 0.4);
        }
        // Tail beyond the trunk.
        if f.tail > 0 {
            let last = sp[n];
            let prev = sp[n - 1];
            let d = sub(last, prev);
            let mut pts = vec![last];
            for k in 1..=f.tail * 2 {
                let bend = (self.time * 1.3 + k as f32 * 0.6).sin() * 0.15 * k as f32;
                let p = add(*pts.last().unwrap(), rot(mul(d, 0.7), -0.15 * k as f32 + bend));
                pts.push(p);
            }
            pen.chain(&pts, self.radius(1.0), self.radius(1.0) * 0.15, Tissue::Muscle, 0.0);
        }
    }

    /// Mouthparts, senses, feelers and lure at the front of the head.
    fn head(&self, pen: &mut Pen, at: [f32; 2], fwd: [f32; 2], r: f32, feelers: bool) {
        let f = &self.form;
        let up = [-fwd[1], fwd[0]];
        let tip = add(at, mul(fwd, r * 0.9));
        let chew = (self.time * 3.0 + self.seed as f32).sin() * 0.5 + 0.5;
        match f.mouth {
            Mouth::Mandibles => {
                for (side, sgn) in [(-0.6, 1.0), (0.6, -1.0)] {
                    let open = 0.25 + 0.35 * chew;
                    let mid = add(tip, rot(mul(fwd, r * 0.9), sgn * open));
                    let end = add(mid, rot(mul(fwd, r * 0.8), -sgn * (open + 0.6)));
                    pen.chain(&[tip, mid, end], r * 0.22, r * 0.04, Tissue::Mouth, side);
                }
            }
            Mouth::Proboscis => {
                let sway = (self.time * 0.9).sin() * 0.15;
                let a = rot(fwd, -0.9 + sway);
                let m = add(tip, mul(a, r * 2.0));
                let e = add(m, mul(rot(a, -0.3), r * 1.6));
                pen.chain(&[tip, m, e], r * 0.25, r * 0.06, Tissue::Mouth, 0.3);
            }
            Mouth::Crown => {
                for k in 0..6 {
                    let a = -0.9 + 1.8 * k as f32 / 5.0 + 0.15 * (self.time * 1.2 + k as f32).sin();
                    let d = rot(fwd, a);
                    let m = add(tip, mul(d, r * 0.7));
                    let e = add(m, mul(rot(d, a * 0.6), r * 0.6));
                    pen.chain(&[tip, m, e], r * 0.2, r * 0.05, Tissue::Muscle, if k % 2 == 0 { 0.3 } else { -0.3 });
                }
            }
            Mouth::Beak => {
                let e = add(tip, add(mul(fwd, r * 1.8), mul(up, -r * 0.5)));
                pen.seg(tip, e, r * 0.45, r * 0.02, Tissue::Armor, 0.3);
            }
            Mouth::Maw => {
                let gape = 0.2 + 0.25 * chew;
                let upper = add(tip, mul(rot(fwd, gape), r * 1.3));
                let lower = add(tip, mul(rot(fwd, -gape - 0.2), r * 1.1));
                pen.seg(tip, upper, r * 0.5, r * 0.12, Tissue::Mouth, 0.2);
                pen.seg(tip, lower, r * 0.42, r * 0.1, Tissue::Mouth, 0.25);
            }
        }
        // Wet sense pits in a cluster: not eyes, nothing that looks back.
        for k in 0..f.senses {
            let a = 0.5 + 0.35 * k as f32;
            let p = add(at, add(mul(fwd, r * 0.55 * a.cos()), mul(up, r * 0.6 * a.sin())));
            pen.seg(p, p, r * 0.09, r * 0.09, Tissue::Sense, 0.6);
        }
        if f.lure {
            let base = add(at, mul(up, r * 0.8));
            let bend = (self.time * 0.7).sin() * 0.2;
            let m = add(base, mul(rot(up, -0.5 + bend), r * 2.2));
            let e = add(m, mul(rot(fwd, -0.8 + bend), r * 1.6));
            pen.chain(&[base, m, e], r * 0.12, r * 0.05, Tissue::Frame, 0.2);
            pen.seg(e, e, r * 0.28, r * 0.28, Tissue::Glow, 0.3);
        }
        if feelers {
            for k in 0..f.feelers {
                let d = rot(fwd, 2.4 + 0.3 * k as f32);
                pen.anchor(add(at, mul(up, r * 0.5)), d);
            }
        }
    }

    fn walker(&self, pen: &mut Pen) {
        let f = &self.form;
        let reach = f.leg_len * f.size;
        let hip_h = if f.knee_up { reach * 0.5 } else { reach * 0.78 };
        let walking = self.moving && self.alive >= 1.0;
        let bob = if walking { (self.phase * TAU * 2.0).sin() * hip_h * 0.03 } else { 0.0 };
        let breathe = (self.time * 0.8).sin() * f.size * 0.004;

        if self.alive < 1.0 {
            return self.carcass(pen);
        }

        let sp = self.spine(hip_h + bob + breathe, |_| 0.0);
        self.trunk(pen, &sp);

        let n = sp.len() - 1;
        let at = |t: f32| {
            let x = t * n as f32;
            let i = (x.floor() as usize).min(n - 1);
            lerp2(sp[i], sp[i + 1], x - i as f32)
        };

        // Legs: far side first so the near side draws over the body.
        let stride = self.stride();
        for side in [-1.0f32, 1.0] {
            for k in 0..f.legs {
                let t = f.leg_first + f.leg_span * (k as f32 + 0.5) / f.legs as f32;
                let hip = add(at(t), [0.0, -self.radius(t) * 0.3]);
                let spread = (t - 0.5) * f.size * 1.1;
                let home = hip[0] + spread;
                let ph = (self.phase + k as f32 * 0.37 + if side > 0.0 { 0.5 } else { 0.0 }).fract();
                let (fx, fy) = if !walking {
                    (home, 0.0)
                } else if ph < 0.6 {
                    (home + stride * (0.5 - ph / 0.6), 0.0)
                } else {
                    let s = (ph - 0.6) / 0.4;
                    (home + stride * (s - 0.5), (s * PI).sin() * hip_h * 0.22)
                };
                let r = self.radius(t) * f.leg_r;
                let tar = if f.tarsus { reach * 0.18 } else { 0.0 };
                let foot = [fx, fy];
                let ankle = [fx - tar * 0.35 * if f.knee_up { -1.0 } else { 1.0 }, fy + tar];
                let l = (reach - tar) * 0.55;
                // Spider-like legs splay, knees high; others fold like elbows in
                // front and knees behind.
                let front = t < f.leg_first + f.leg_span * 0.5;
                let bend = match (f.knee_up, front) {
                    (true, true) => 1.0,
                    (true, false) => -1.0,
                    (false, true) => -1.0,
                    (false, false) => 1.0,
                };
                let kn = knee(hip, ankle, l, l * 1.05, bend);
                let tissue = if f.knee_up || f.armor > 0.5 { Tissue::Frame } else { Tissue::Muscle };
                let sd = side * 0.9;
                pen.seg(hip, kn, r, r * 0.7, tissue, sd);
                pen.seg(kn, ankle, r * 0.7, r * 0.45, tissue, sd);
                if tar > 0.0 {
                    pen.seg(ankle, foot, r * 0.45, r * 0.15, tissue, sd);
                }
            }
        }

        // Neck and head.
        let front = sp[0];
        let fwd0 = {
            let d = sub(sp[0], sp[1]);
            mul(d, 1.0 / len(d).max(1e-6))
        };
        if f.neck > 0 {
            let seg = f.neck_len * f.size / f.neck as f32;
            let mut pts = vec![front];
            let mut a = 1.1f32;
            let peck = if walking { 0.0 } else { (self.time * 0.5).sin().max(0.0) * 1.2 };
            for k in 0..f.neck {
                a -= 0.45 + peck * 0.5 * k as f32 / f.neck as f32;
                pts.push(add(*pts.last().unwrap(), polar(a, seg)));
            }
            let r0 = self.radius(0.0) * 0.6;
            pen.chain(&pts, r0, r0 * 0.6, Tissue::Muscle, 0.1);
            let end = *pts.last().unwrap();
            let d = sub(end, pts[pts.len() - 2]);
            self.head(pen, end, mul(d, 1.0 / len(d).max(1e-6)), r0 * 0.9, true);
        } else {
            self.head(pen, front, fwd0, self.radius(0.0), true);
        }
    }

    /// A body on its side, legs drawn up, sinking into rot.
    fn carcass(&self, pen: &mut Pen) {
        let f = &self.form;
        let r_mid = self.radius(f.widest);
        let sag = 1.0 - self.alive;
        let sp = self.spine(r_mid * (1.0 - sag * 0.5), |t| -(PI * t).sin() * r_mid * 0.2);
        self.trunk(pen, &sp);
        let n = sp.len() - 1;
        for side in [-1.0f32, 1.0] {
            for k in 0..f.legs {
                let t = f.leg_first + f.leg_span * (k as f32 + 0.5) / f.legs as f32;
                let x = t * n as f32;
                let i = (x.floor() as usize).min(n - 1);
                let hip = lerp2(sp[i], sp[i + 1], x - i as f32);
                let l = f.leg_len * f.size * 0.5;
                let kn = add(hip, polar(1.2 + side * 0.4 + k as f32 * 0.2, l));
                let tip = add(kn, polar(-0.6 + k as f32 * 0.3, l * 0.8));
                let r = self.radius(t) * f.leg_r;
                pen.seg(hip, kn, r, r * 0.7, Tissue::Frame, side * 0.9);
                pen.seg(kn, tip, r * 0.7, r * 0.3, Tissue::Frame, side * 0.9);
            }
        }
        let fwd = sub(sp[0], sp[1]);
        self.head(pen, sp[0], mul(fwd, 1.0 / len(fwd).max(1e-6)), self.radius(0.0), false);
    }

    fn swimmer(&self, pen: &mut Pen) {
        let f = &self.form;
        let amp = f.size * 0.08;
        let ph = self.phase * TAU;
        let sp = self.spine(0.0, |t| amp * t * t.sqrt() * (t * 5.0 - ph).sin());
        let n = sp.len() - 1;
        let at = |t: f32| {
            let x = t * n as f32;
            let i = (x.floor() as usize).min(n - 1);
            lerp2(sp[i], sp[i + 1], x - i as f32)
        };
        // Fins behind the body first.
        if f.fins {
            let flap = (ph * 1.5).sin() * 0.5;
            let root = at(0.22);
            let r = self.radius(0.22);
            let b = add(root, rot([-f.size * 0.22, -f.size * 0.18], flap));
            let c = add(root, rot([-f.size * 0.3, -f.size * 0.02], flap * 0.6));
            pen.mem(add(root, [0.0, -r * 0.3]), b, c, Tissue::Muscle, -0.8);
            let b2 = add(root, rot([-f.size * 0.2, -f.size * 0.2], -flap));
            let c2 = add(root, rot([-f.size * 0.28, -f.size * 0.04], -flap * 0.6));
            pen.mem(add(root, [0.0, -r * 0.3]), b2, c2, Tissue::Muscle, 0.8);
        }
        if f.sail > 0.0 {
            let a = add(at(0.2), [0.0, self.radius(0.2) * 0.7]);
            let c = add(at(0.75), [0.0, self.radius(0.75) * 0.7]);
            let top = add(at(0.4), [-f.size * 0.08, f.size * f.sail]);
            pen.mem(c, top, a, Tissue::Muscle, 0.1);
        }
        self.trunk(pen, &sp);
        if f.tail_fin > 0.0 {
            let end = sp[n];
            let d = sub(end, sp[n - 1]);
            let d = mul(d, 1.0 / len(d).max(1e-6));
            let fin = f.size * f.tail_fin;
            let up = [-d[1], d[0]];
            let b = add(end, add(mul(d, fin), mul(up, fin * 0.8)));
            let c = add(end, add(mul(d, fin * 0.9), mul(up, -fin * 0.7)));
            pen.mem(end, b, c, Tissue::Muscle, 0.2);
        }
        // Paddling swimmerets, a metachronal wave front to back.
        for k in 0..f.legs {
            let t = 0.25 + 0.5 * k as f32 / f.legs.max(1) as f32;
            let base = add(at(t), [0.0, -self.radius(t) * 0.7]);
            let a = -PI * 0.5 + 0.6 * (ph * 2.0 - k as f32 * 0.8).sin();
            let l = f.leg_len * f.size;
            let m = add(base, polar(a, l * 0.6));
            let e = add(m, polar(a - 0.5, l * 0.5));
            let r = self.radius(t) * 0.18;
            pen.chain(&[base, m, e], r, r * 0.3, Tissue::Frame, 0.7);
        }
        let fwd = sub(sp[0], sp[1]);
        self.head(pen, sp[0], mul(fwd, 1.0 / len(fwd).max(1e-6)), self.radius(0.0), true);
    }

    fn floater(&self, pen: &mut Pen) {
        let f = &self.form;
        let pulse = (self.phase * TAU).sin();
        let r0 = f.size * 0.32;
        // Body core hangs under a lopsided cluster of gas bladders.
        let core_top = [0.0, 0.0];
        let core_bot = [f.size * 0.05, -f.size * 0.35];
        for k in 0..f.bladders {
            let a = 0.6 + 2.0 * (k as f32 + 0.5) / f.bladders as f32 + 0.3 * ((k * 7 % 5) as f32 - 2.0) * 0.2;
            let dist = f.size * (0.12 + 0.12 * ((k * 3 % 4) as f32 / 3.0));
            let c = add(core_top, polar(a, dist));
            let r = r0 * (0.55 + 0.45 * ((k * 5 % 7) as f32 / 6.0)) * (1.0 + 0.05 * (pulse + k as f32).sin());
            pen.seg(c, add(c, [0.0, r * 0.15]), r, r * 0.92, Tissue::Float, 0.3 - 0.15 * (k % 3) as f32);
        }
        pen.seg(core_top, core_bot, f.size * 0.13, f.size * 0.08, Tissue::Muscle, 0.5);
        pen.seg(
            add(core_top, [0.0, -f.size * 0.05]),
            add(core_bot, [0.0, f.size * 0.06]),
            f.size * 0.06,
            f.size * 0.04,
            Tissue::Gut,
            0.5,
        );
        if f.brood {
            pen.seg(core_top, core_top, f.size * 0.1, f.size * 0.1, Tissue::Brood, 0.5);
        }
        for k in 0..f.glow_spots {
            let a = -0.3 + 3.6 * k as f32 / f.glow_spots as f32;
            let p = add(core_top, polar(a, f.size * 0.2));
            pen.seg(p, p, f.size * 0.025, f.size * 0.025, Tissue::Glow, 0.6);
        }
        for k in 0..f.tentacles {
            let x = f.size * (-0.2 + 0.4 * k as f32 / (f.tentacles - 1).max(1) as f32);
            pen.anchor(add(core_bot, [x, f.size * 0.04]), [0.05 * x.signum(), -1.0]);
        }
    }

    fn glider(&self, pen: &mut Pen) {
        let f = &self.form;
        let ph = self.phase * TAU;
        // Long glides broken by a few deep strokes.
        let stroke = (ph * 3.0).sin() * ((ph).sin().max(0.0)).powf(3.0);
        let sp = self.spine(0.0, |t| f.size * 0.02 * (t * 3.0 - ph).sin());
        let n = sp.len() - 1;
        let at = |t: f32| {
            let x = t * n as f32;
            let i = (x.floor() as usize).min(n - 1);
            lerp2(sp[i], sp[i + 1], x - i as f32)
        };
        let span = f.size * f.wing;
        let shoulder = add(at(0.28), [0.0, self.radius(0.3) * 0.3]);
        // Seen a little from below: the far wing rises above the body, the
        // near one sweeps below it, and a stroke swings both.
        let wing = |up: f32| {
            let tip = add(shoulder, [-span * 0.3, span * up]);
            let elbow = add(shoulder, [-span * 0.05, span * up * 0.45]);
            let trail = add(tip, [-span * 0.35, -span * up * 0.25]);
            (elbow, tip, trail)
        };
        let rear = at(0.85);
        let (e1, t1, r1) = wing(0.75 + 0.35 * stroke);
        pen.mem(shoulder, t1, r1, Tissue::Muscle, -0.9);
        pen.mem(shoulder, r1, rear, Tissue::Muscle, -0.9);
        pen.chain(&[shoulder, e1, t1], self.radius(0.3) * 0.25, self.radius(0.3) * 0.06, Tissue::Frame, -0.9);
        self.trunk(pen, &sp);
        let (e2, t2, r2) = wing(-0.45 + 0.6 * stroke);
        pen.mem(shoulder, t2, r2, Tissue::Muscle, 0.9);
        pen.mem(shoulder, r2, rear, Tissue::Muscle, 0.9);
        pen.chain(&[shoulder, e2, t2], self.radius(0.3) * 0.3, self.radius(0.3) * 0.08, Tissue::Frame, 0.9);
        let fwd = sub(sp[0], sp[1]);
        self.head(pen, sp[0], mul(fwd, 1.0 / len(fwd).max(1e-6)), self.radius(0.0), true);
        // Trailing tail filaments.
        for k in 0..2 {
            pen.anchor(sp[n], [-1.0, -0.2 - 0.3 * k as f32]);
        }
    }

    fn burrower(&self, pen: &mut Pen) {
        let f = &self.form;
        // Emerge over a few seconds, stand and sway, then pull back down.
        let cyc = self.phase;
        let out = ((cyc * 4.0).min(1.0) * ((1.0 - cyc) * 3.0).min(1.0)).clamp(0.0, 1.0);
        let out = out * out * (3.0 - 2.0 * out);
        let total = f.size * (0.15 + 0.85 * out);
        let n = f.trunk;
        let seg = f.size / n as f32;
        let mut pts = vec![[0.0, -f.size * 0.2]];
        let mut a = PI * 0.5;
        let mut grown = -f.size * 0.2;
        for k in 0..n {
            let sway = (self.time * 0.6 + k as f32 * 0.35).sin() * 0.12 + 0.05 * (self.time * 1.7 + k as f32).sin();
            a += sway * (k as f32 / n as f32);
            let l = seg.min((total - grown).max(0.0) + seg * 0.2);
            grown += l;
            pts.push(add(*pts.last().unwrap(), polar(a, l)));
        }
        let r = f.size * f.girth;
        for i in 0..n {
            let t = i as f32 / n as f32;
            // Ribbed: alternating swellings down the body.
            let rib = 1.0 + 0.1 * ((i % 2) as f32);
            pen.seg(
                pts[i],
                pts[i + 1],
                r * rib * (1.0 - 0.25 * t),
                r * (1.0 - 0.25 * (t + 1.0 / n as f32)),
                Tissue::Muscle,
                0.0,
            );
        }
        let end = pts[n];
        let d = sub(end, pts[n - 1]);
        let fwd = mul(d, 1.0 / len(d).max(1e-6));
        self.head(pen, end, fwd, r * 0.85, false);
    }
}

/// Spreads a cast of species over the span, each a handful of relatives.
pub fn populate(rng: &mut Rng) -> Vec<Creature> {
    let mut out = Vec::new();
    let plans = [
        (Plan::Walker, 6, 1..=3),
        (Plan::Wader, 2, 1..=2),
        (Plan::Swimmer, 4, 1..=3),
        (Plan::Floater, 2, 1..=3),
        (Plan::Glider, 2, 1..=2),
        (Plan::Burrower, 2, 1..=3),
    ];
    for (plan, species, herd) in plans {
        for _ in 0..species {
            let form = Form::random(rng, plan);
            let look = rng.look();
            let home = rng.range(0.7, 3.0);
            let count = rng.int(*herd.start(), *herd.end());
            let depth0 = rng.range(0.1, 0.9);
            for _ in 0..count {
                let mut f = form.clone();
                f.size *= rng.range(0.8, 1.15);
                let x = (home + rng.normal() * 0.2).clamp(0.4, 3.2);
                let depth = match plan {
                    Plan::Wader => rng.range(0.25, 0.75),
                    _ => (depth0 + rng.normal() * 0.2).clamp(0.02, 0.98),
                };
                let kin = rng.kin(&look, 0.03);
                out.push(Creature::new(f, kin, rng, x, depth));
            }
        }
    }
    // A fresh kill and an older carcass.
    for decay in [0.3, 0.75] {
        let mut f = Form::random(rng, Plan::Walker);
        f.size = rng.range(0.07, 0.1);
        f.leg_r = 0.45;
        f.girth = 0.28;
        f.knee_up = false;
        let look = rng.look();
        let x = rng.range(1.4, 2.6);
        let depth = rng.range(0.2, 0.6);
        let c = Creature::new(f, look, rng, x, depth).dead(decay);
        out.push(c);
    }
    out
}
