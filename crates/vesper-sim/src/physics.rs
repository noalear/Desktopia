//! Bodies as physical things. Each creature's segments are rigid capsules
//! hinged together in the creature's own upright plane (forward and up), and
//! simulated with small-step position-based dynamics (XPBD). Muscles pull each
//! hinge toward a target angle that swings on the body's rhythm: evolution
//! sets each joint's swing and timing, the brain sets how hard and which way.
//! Whether a body then walks, swims, flaps, inches along or just flails is
//! whatever gravity, ground friction, drag and buoyancy make of it.
//!
//! Units are world units (about ten metres each) and seconds.

use crate::body::Body;
use crate::genome::Tissue;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

/// Physics steps per second.
pub const HZ: f32 = 30.0;
pub const STEP: f32 = 1.0 / HZ;
/// Physics steps in one tick of the simulation.
pub const STEPS_PER_TICK: usize = 6;
/// Energy spent per unit of work the muscles do.
pub const MOTOR_COST: f32 = 250.0;
const SUBSTEPS: usize = 6;
/// World units per second squared.
pub const GRAVITY: f32 = 0.98;
/// Density of the thick air of the twilight band, relative to flesh.
pub const AIR: f32 = 0.2;
/// Density of river water, relative to flesh.
pub const WATER: f32 = 1.0;
/// How fast a bladder can carry a body up or down, body units per second.
const CLIMB: f32 = 2.0;
/// Lift of gas-float tissue per unit of its area, against flesh's weight per
/// unit area, matching the floatiness the creature's abilities assume.
const FLOAT_LIFT: f32 = 3.5;

/// Segments present in a body at this maturity and these losses, as a bit mask.
pub fn present(body: &Body, maturity: f32, lost: u64) -> u64 {
    let mut gone = lost;
    let mut mask = 0u64;
    for (i, s) in body.segments.iter().enumerate() {
        if s.parent >= 0 && gone & (1 << s.parent) != 0 {
            gone |= 1 << i;
        }
        if gone & (1 << i) == 0 && s.onset <= maturity {
            mask |= 1 << i;
        }
    }
    mask
}

/// One rigid segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    /// Which body segment this is.
    pub seg: u16,
    /// Centre: forward along the creature's plane, and altitude.
    pub pos: [f32; 2],
    /// Direction of the segment from base to tip, radians, and as a unit
    /// vector, which the solver turns along with it to spare itself trig.
    pub angle: f32,
    rot: [f32; 2],
    pub vel: [f32; 2],
    pub spin: f32,
    pub half: f32,
    pub radius: f32,
    inv_mass: f32,
    inv_inertia: f32,
    /// Upward force from gas floats.
    lift: f32,
    /// Drag across and along the segment.
    drag_n: f32,
    drag_t: f32,
    /// Friction on the ground; spines grip backward and slide forward.
    grip: f32,
    spiny: bool,
    #[serde(skip)]
    prev: [f32; 3],
    #[serde(skip)]
    prev_rot: [f32; 2],
}

impl Part {
    pub fn axis(&self) -> [f32; 2] {
        self.rot
    }

    fn set_angle(&mut self, a: f32) {
        self.angle = a;
        let (s, c) = sin_cos(a);
        self.rot = [c, s];
    }

    /// Turns by `da`, rotating the axis by series for the small turns the
    /// solver makes.
    fn turn(&mut self, da: f32) {
        self.angle += da;
        let d2 = da * da;
        let [c, s] = if d2 < 0.09 {
            [1.0 - d2 * (0.5 - d2 * (1.0 / 24.0)), da * (1.0 - d2 * (1.0 / 6.0 - d2 * (1.0 / 120.0)))]
        } else {
            let (s, c) = sin_cos(da);
            [c, s]
        };
        let [x, y] = self.rot;
        self.rot = [x * c - y * s, x * s + y * c];
    }

    pub fn base(&self) -> [f32; 2] {
        let t = self.axis();
        [self.pos[0] - t[0] * self.half, self.pos[1] - t[1] * self.half]
    }

    pub fn tip(&self) -> [f32; 2] {
        let t = self.axis();
        [self.pos[0] + t[0] * self.half, self.pos[1] + t[1] * self.half]
    }

    pub fn mass(&self) -> f32 {
        1.0 / self.inv_mass
    }
}

/// A hinge between a segment and the one it grows from, with its muscle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Joint {
    parent: u16,
    child: u16,
    /// Where on the parent, along its axis from its centre.
    anchor: f32,
    /// Angle of the child relative to the parent at rest.
    rest: f32,
    amp: f32,
    phase: f32,
    lift: f32,
    /// How far the joint yields to load (inverse stiffness), and how much of
    /// its wobble dies away each substep.
    compliance: f32,
    damping: f32,
    /// How far either side of rest the joint can bend at all: well past
    /// what its muscle swings it through, so a knee can be forced backward,
    /// but not folded flat through its own limb.
    range: f32,
    #[serde(skip)]
    target: f32,
}

/// What the brain asks of the body this step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Drive {
    /// -1 full reverse .. 1 full ahead; 0 holds the rest pose.
    pub power: f32,
    /// -1 sink .. 1 rise: bends each joint by its lift gene.
    pub rise: f32,
    /// The body's rhythm at full drive, cycles per second.
    pub hz: f32,
    /// Dead or held: muscles go slack.
    pub limp: bool,
}

/// What the body moves through.
#[derive(Clone, Copy, Debug)]
pub struct Medium {
    /// Relative to flesh: AIR or WATER.
    pub density: f32,
    /// Velocity of the air or water in the creature's plane.
    pub flow: [f32; 2],
    /// Highest altitude anything can reach.
    pub ceiling: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    pub parts: Vec<Part>,
    joints: Vec<Joint>,
    /// Position in the body's rhythm, cycles.
    pub clock: f32,
    /// The segments this rig was built with, and at what size.
    pub mask: u64,
    pub scale: f32,
    /// Something touched the ground during the last step.
    pub grounded: bool,
    /// Work done by muscles since last taken, in energy-like units.
    pub work: f32,
    /// Seconds it has been almost still; a still corpse stops being simulated.
    pub still: f32,
    /// Which way it is turned as we see it, -1 (toward the day) .. 1 (toward
    /// the night). It swings over smoothly and only once a turn is well
    /// under way, so a body heading into the distance doesn't flicker.
    #[serde(default = "one")]
    pub face: f32,
    /// Pieces that bump into each other rather than pass through: those on
    /// the same side of the body (or the midline), not jointed close
    /// together, and apart at rest. Worked out again from the body after a load.
    #[serde(skip)]
    pairs: Option<Vec<(u16, u16)>>,
}

fn one() -> f32 {
    1.0
}

fn tissue_strength(t: Tissue) -> f32 {
    match t {
        Tissue::Muscle => 1.0,
        Tissue::Frame | Tissue::Armor | Tissue::Spine => 0.5,
        Tissue::Mouth | Tissue::Sense | Tissue::Gut | Tissue::Store | Tissue::Brood => 0.3,
        Tissue::Float | Tissue::LightFilm | Tissue::Glow => 0.12,
    }
}

/// Sine and cosine to about 1e-7, several times faster than the library's;
/// the solver needs a great many of them.
fn sin_cos(x: f32) -> (f32, f32) {
    let k = ((x * std::f32::consts::FRAC_2_PI) + 0.5f32.copysign(x)) as i32 as f32;
    let r = x - k * 1.570_796_4 + k * 4.371_139e-8;
    let r2 = r * r;
    let s = r * (1.0 + r2 * (-1.0 / 6.0 + r2 * (1.0 / 120.0 + r2 * (-1.0 / 5040.0))));
    let c = 1.0 + r2 * (-0.5 + r2 * (1.0 / 24.0 + r2 * (-1.0 / 720.0 + r2 * (1.0 / 40320.0))));
    match (k as i64) & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

fn wrap(a: f32) -> f32 {
    a - TAU * ((a * (1.0 / TAU)) + 0.5f32.copysign(a)) as i32 as f32
}

fn cross(r: [f32; 2], v: [f32; 2]) -> f32 {
    r[0] * v[1] - r[1] * v[0]
}

impl Rig {
    /// A rig with nothing in it, until the body is known.
    pub fn empty() -> Self {
        Self {
            parts: Vec::new(),
            joints: Vec::new(),
            clock: 0.0,
            mask: 0,
            scale: 1.0,
            grounded: false,
            work: 0.0,
            still: 0.0,
            face: 1.0,
            pairs: Some(Vec::new()),
        }
    }

    /// Builds the rig for the segments in `mask` at rest, with its lowest
    /// point at altitude `alt`.
    pub fn new(body: &Body, mask: u64, scale: f32, alt: f32) -> Self {
        let mut rig = Self::build(body, mask, scale, alt);
        rig.pairs = Some(rig.rest_pairs(body));
        rig
    }

    /// Finds which pieces collide, if a load left that unknown.
    pub fn ensure_pairs(&mut self, body: &Body) {
        if self.pairs.is_none() {
            self.pairs = Some(Self::build(body, self.mask, self.scale, 0.0).rest_pairs(body));
        }
    }

    /// The colliding pairs, from this rig in its rest pose.
    fn rest_pairs(&self, body: &Body) -> Vec<(u16, u16)> {
        let n = self.parts.len();
        let mut parent = vec![u16::MAX; n];
        for j in &self.joints {
            parent[j.child as usize] = j.parent;
        }
        let up = |k: u16| if k == u16::MAX { u16::MAX } else { parent[k as usize] };
        let side = |k: usize| body.segments.get(self.parts[k].seg as usize).map_or(0, |s| s.side);
        let mut pairs = Vec::new();
        for a in 0..n {
            for b in a + 1..n {
                let (sa, sb) = (side(a), side(b));
                // The near and far legs of a pair swing past each other.
                if sa != 0 && sb != 0 && sa != sb {
                    continue;
                }
                // Jointed together or nearly: they share a hinge.
                let (a16, b16) = (a as u16, b as u16);
                if up(a16) == b16 || up(b16) == a16 || up(a16) == up(b16) || up(up(a16)) == b16 || up(up(b16)) == a16 {
                    continue;
                }
                let (pa, pb) = (&self.parts[a], &self.parts[b]);
                let (_, _, d) = closest(pa.base(), pa.tip(), pb.base(), pb.tip());
                // Grown overlapping, like a gut inside a frame: leave them be.
                if d < (pa.radius + pb.radius) * 1.05 {
                    continue;
                }
                pairs.push((a16, b16));
            }
        }
        pairs
    }

    fn build(body: &Body, mask: u64, scale: f32, alt: f32) -> Self {
        let mut index = vec![u16::MAX; body.segments.len()];
        let mut parts = Vec::new();
        for (i, s) in body.segments.iter().enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            index[i] = parts.len() as u16;
            let area = s.length * s.width * scale * scale;
            let density = s.tissue.density().max(0.05);
            let m = (area * density).max(1e-9);
            let len = s.length * scale;
            let w = s.width * scale;
            let mut drag = len * (w + if s.membrane { s.length * scale * 0.8 } else { 0.0 });
            if s.tissue == Tissue::LightFilm {
                drag += len * len * 0.3;
            }
            parts.push(Part {
                seg: i as u16,
                pos: [(s.base[0] + s.tip[0]) * 0.5 * scale, (s.base[1] + s.tip[1]) * 0.5 * scale],
                angle: s.dir,
                rot: [s.dir.cos(), s.dir.sin()],
                vel: [0.0; 2],
                spin: 0.0,
                half: len * 0.5,
                radius: w * 0.5,
                inv_mass: 1.0 / m,
                inv_inertia: 12.0 / (m * (len * len + w * w)).max(1e-15),
                lift: if s.tissue == Tissue::Float { FLOAT_LIFT * area * GRAVITY } else { 0.0 },
                drag_n: drag * 40.0,
                drag_t: w * len * 4.0,
                grip: if s.tissue == Tissue::Spine { 1.4 } else { 0.7 },
                spiny: s.tissue == Tissue::Spine,
                prev: [0.0; 3],
                prev_rot: [1.0, 0.0],
            });
        }
        // Joints, with each muscle as stiff as it needs to be to hold up
        // everything that hangs from it.
        let mut joints = Vec::new();
        for (i, s) in body.segments.iter().enumerate() {
            if index[i] == u16::MAX || s.parent < 0 || index[s.parent as usize] == u16::MAX {
                continue;
            }
            let p = s.parent as usize;
            let parent = &body.segments[p];
            let hinge = [s.base[0] * scale, s.base[1] * scale];
            let sub = body.subtree(i) & mask;
            let mut inertia = 0.0;
            for (k, part) in parts.iter().enumerate() {
                let _ = k;
                if sub & (1 << part.seg) != 0 {
                    let d = [part.pos[0] - hinge[0], part.pos[1] - hinge[1]];
                    inertia += part.mass() * (d[0] * d[0] + d[1] * d[1]) + 1.0 / part.inv_inertia;
                }
            }
            let strength = tissue_strength(s.tissue).max(tissue_strength(parent.tissue));
            let omega = TAU * (1.5 + 14.0 * strength);
            let anchor = (s.along - 0.5) * parent.length * scale;
            joints.push(Joint {
                parent: index[p],
                child: index[i],
                anchor,
                rest: s.angle,
                amp: if strength >= 0.3 { s.amp } else { s.amp * 0.3 },
                phase: s.phase,
                lift: s.lift,
                compliance: 1.0 / (inertia * omega * omega).max(1e-18),
                damping: 1.0 - (-omega * 0.8 * STEP / SUBSTEPS as f32).exp(),
                range: (s.amp + s.lift.abs() + 1.0).min(2.5),
                target: s.angle,
            });
        }
        let mut rig = Self {
            parts,
            joints,
            clock: 0.0,
            mask,
            scale,
            grounded: false,
            work: 0.0,
            still: 0.0,
            face: 1.0,
            pairs: None,
        };
        let low = rig.lowest();
        for p in &mut rig.parts {
            p.pos[1] += alt - low;
        }
        rig
    }

    /// A new rig for a body that has grown, shrunk or lost pieces, keeping
    /// the posture and motion of every piece both share.
    pub fn rebuild(&self, body: &Body, mask: u64, scale: f32) -> Self {
        let mut new = Self::new(body, mask, scale, 0.0);
        new.clock = self.clock;
        let old: std::collections::HashMap<u16, &Part> = self.parts.iter().map(|p| (p.seg, p)).collect();
        // Angles carry over; positions follow from the hinges, root first.
        let root_old = self.parts.first();
        for k in 0..new.parts.len() {
            let seg = new.parts[k].seg;
            if let Some(o) = old.get(&seg) {
                new.parts[k].set_angle(o.angle);
                new.parts[k].vel = o.vel;
                new.parts[k].spin = o.spin;
            } else if let Some(j) = new.joints.iter().find(|j| j.child as usize == k) {
                let a = new.parts[j.parent as usize].angle + j.rest;
                new.parts[k].set_angle(a);
            }
            if k == 0 {
                if let Some(r) = root_old {
                    new.parts[0].pos = r.pos;
                }
                continue;
            }
            if let Some(j) = new.joints.iter().find(|j| j.child as usize == k).cloned() {
                let p = &new.parts[j.parent as usize];
                let t = p.axis();
                let hinge = [p.pos[0] + t[0] * j.anchor, p.pos[1] + t[1] * j.anchor];
                let c = &mut new.parts[k];
                let tc = c.axis();
                c.pos = [hinge[0] + tc[0] * c.half, hinge[1] + tc[1] * c.half];
            }
        }
        new.grounded = self.grounded;
        new.still = self.still;
        new.face = self.face;
        new
    }

    /// The pieces in `sub` torn away as a body of their own, moving as they were.
    pub fn piece(&self, body: &Body, sub: u64) -> Self {
        let mut new = Self::new(body, sub & self.mask, self.scale, 0.0);
        new.face = self.face;
        for p in &mut new.parts {
            if let Some(o) = self.parts.iter().find(|o| o.seg == p.seg) {
                p.pos = o.pos;
                p.set_angle(o.angle);
                p.vel = o.vel;
                p.spin = o.spin;
            }
        }
        new
    }

    /// The lowest point of the body, altitude.
    pub fn lowest(&self) -> f32 {
        let mut low = f32::MAX;
        for p in &self.parts {
            low = low.min(p.base()[1] - p.radius).min(p.tip()[1] - p.radius);
        }
        if low == f32::MAX { 0.0 } else { low }
    }

    pub fn centre(&self) -> [f32; 2] {
        let (mut x, mut y, mut m) = (0.0, 0.0, 0.0);
        for p in &self.parts {
            let pm = p.mass();
            x += p.pos[0] * pm;
            y += p.pos[1] * pm;
            m += pm;
        }
        if m > 0.0 { [x / m, y / m] } else { [0.0, 0.0] }
    }

    pub fn momentum(&self) -> [f32; 2] {
        let mut v = [0.0, 0.0];
        for p in &self.parts {
            v[0] += p.vel[0] * p.mass();
            v[1] += p.vel[1] * p.mass();
        }
        v
    }

    pub fn mass(&self) -> f32 {
        self.parts.iter().map(Part::mass).sum()
    }

    /// Swings the facing toward the side `heading` points to, in seconds `dt`.
    pub fn turn_to(&mut self, heading: f32, dt: f32) {
        let c = heading.cos();
        let goal = if c > 0.3 {
            1.0
        } else if c < -0.3 {
            -1.0
        } else {
            self.face.signum()
        };
        self.face += (goal - self.face).clamp(-dt / 0.4, dt / 0.4);
    }

    /// Moves the whole body up or down.
    pub fn raise(&mut self, dy: f32) {
        for p in &mut self.parts {
            p.pos[1] += dy;
        }
    }

    /// Moves the whole body along its plane, e.g. to keep it centred on the origin.
    pub fn shift(&mut self, du: f32) {
        for p in &mut self.parts {
            p.pos[0] += du;
        }
    }

    /// Mirrors the body front to back, for turning round on the spot.
    pub fn mirror(&mut self) {
        for p in &mut self.parts {
            p.pos[0] = -p.pos[0];
            p.vel[0] = -p.vel[0];
            p.set_angle(PI - p.angle);
            p.spin = -p.spin;
        }
    }

    /// Advances one physics step.
    pub fn step(&mut self, drive: Drive, medium: Medium) {
        if self.parts.is_empty() {
            return;
        }
        let dt = STEP;
        let power = drive.power.clamp(-1.0, 1.0);
        let rate = drive.hz * (0.35 + 0.65 * power.abs());
        self.clock = (self.clock + rate * power.signum() * dt).rem_euclid(1.0);
        let slack = if drive.limp { 200.0 } else { 1.0 };
        let mut moved = 0.0f32;
        for j in &mut self.joints {
            let swing =
                if drive.limp { 0.0 } else { j.amp * power.abs().sqrt() * sin_cos(TAU * (self.clock + j.phase)).0 };
            let rise = if drive.limp { 0.0 } else { j.lift * drive.rise.clamp(-1.0, 1.0) };
            let target = j.rest + swing + rise;
            moved = moved.max((target - j.target).abs());
            j.target = target;
        }
        // A body resting on dry ground with nothing changing stays as it is.
        if !drive.limp && self.grounded && self.still > 1.0 && moved < 1e-3 && medium.density < WATER * 0.5 {
            self.still += dt;
            return;
        }
        // Gas floats are regulated like swim bladders: enough float buoys the
        // whole body evenly so it hovers, and the bladder lets it climb or
        // sink at a gentle pace as the brain asks. Too little float only
        // lightens it.
        let lift: f32 = self.parts.iter().map(|p| p.lift).sum();
        let weight = self.mass() * GRAVITY;
        let buoyed = if lift > 0.0 {
            let ctrl = if drive.limp {
                0.0
            } else {
                let want = drive.rise.clamp(-1.0, 1.0) * CLIMB * self.scale;
                let vy = self.momentum()[1] / self.mass();
                ((want - vy) / (GRAVITY * 0.5)).clamp(-0.5, 0.5)
            };
            (lift / weight).min(1.0) * (1.0 + ctrl)
        } else {
            0.0
        };
        // Balance in open air or water: the body keeps itself level (or
        // pitched as it means to climb), as fins and a sense of up would. On
        // the ground only its legs and joints hold it up. Corpses have none.
        // Thrown onto its back, it struggles over again rather than lie there.
        let upended = self.grounded && wrap(self.parts[0].angle).abs() > 1.75;
        let (bal_target, bal_compliance) = if drive.limp || (self.grounded && !upended) {
            (0.0, f32::INFINITY)
        } else {
            let c = self.centre();
            let inertia: f32 = self
                .parts
                .iter()
                .map(|p| p.mass() * ((p.pos[0] - c[0]).powi(2) + (p.pos[1] - c[1]).powi(2)) + 1.0 / p.inv_inertia)
                .sum();
            let omega = TAU * 1.2;
            (drive.rise.clamp(-1.0, 1.0) * 0.4, 1.0 / (inertia * omega * omega).max(1e-18))
        };
        // The turned axes drift a hair from the angles; square them up.
        for p in &mut self.parts {
            p.set_angle(p.angle);
        }
        let h = dt / SUBSTEPS as f32;
        self.grounded = false;
        let mut pressed: Vec<f32> = Vec::with_capacity(self.parts.len() * 2);
        let mut energy = 0.0;
        for sub in 0..SUBSTEPS {
            for p in &mut self.parts {
                // Drag across and along the segment, solved implicitly so thin
                // light pieces can't be flung by it.
                let vr = [p.vel[0] - medium.flow[0], p.vel[1] - medium.flow[1]];
                let t = p.axis();
                let n = [-t[1], t[0]];
                let vt = vr[0] * t[0] + vr[1] * t[1];
                let vn = vr[0] * n[0] + vr[1] * n[1];
                let kn = p.drag_n * medium.density;
                let kt = p.drag_t * medium.density;
                let vn = vn / (1.0 + h * p.inv_mass * kn * (vn.abs() + 0.05));
                let vt = vt / (1.0 + h * p.inv_mass * kt * (vt.abs() + 0.05));
                p.vel = [medium.flow[0] + t[0] * vt + n[0] * vn, medium.flow[1] + t[1] * vt + n[1] * vn];
                let kr = kn * p.half * p.half * 0.3;
                p.spin /= 1.0 + h * p.inv_inertia * kr * (p.spin.abs() * p.half + 0.05);
                p.vel[1] += h * GRAVITY * (buoyed - 1.0);
                p.prev = [p.pos[0], p.pos[1], p.angle];
                p.prev_rot = p.rot;
                p.pos[0] += p.vel[0] * h;
                p.pos[1] += p.vel[1] * h;
                p.turn(p.spin * h);
            }
            if bal_compliance.is_finite() {
                let root = &mut self.parts[0];
                let c = wrap(root.angle - bal_target);
                let da = root.inv_inertia * (-c / (root.inv_inertia + bal_compliance / (h * h)));
                root.turn(da);
            }
            let forward = sub % 2 == 0;
            pressed.clear();
            pressed.resize(self.parts.len() * 2, 0.0);
            // Two passes, so hinges pulled apart by the ground come back
            // together and friction still sees every bit of sliding.
            for pass in 0..2 {
                for k in 0..self.joints.len() {
                    let k = if forward == (pass == 0) { k } else { self.joints.len() - 1 - k };
                    let j = &self.joints[k];
                    let (anchor, target, compliance) = (j.anchor, j.target, j.compliance);
                    let (rest, range) = (j.rest, j.range);
                    let (a, b) = pair_mut(&mut self.parts, j.parent as usize, j.child as usize);
                    let w = a.inv_inertia + b.inv_inertia;
                    if pass == 0 {
                        // Muscle: pull toward the target angle, yielding under load.
                        let c = wrap(b.angle - a.angle - target);
                        let dl = -c / (w + compliance * slack / (h * h));
                        b.turn(b.inv_inertia * dl);
                        a.turn(-a.inv_inertia * dl);
                        let rel = (b.angle - b.prev[2]) - (a.angle - a.prev[2]);
                        energy += (dl / (h * h) * rel).abs();
                    }
                    // The joint's stops, firm, and a little looser in the dead.
                    let lim = range + if drive.limp { 0.4 } else { 0.0 };
                    let bend = wrap(b.angle - a.angle - rest);
                    if bend.abs() > lim {
                        let dl = -(bend - lim.copysign(bend)) / w;
                        b.turn(b.inv_inertia * dl);
                        a.turn(-a.inv_inertia * dl);
                    }
                    // Last, so the turning above doesn't leave it out of joint.
                    hinge(a, b, anchor);
                }
                for (k, p) in self.parts.iter_mut().enumerate() {
                    // Nowhere near the ground or ceiling, and not touching yet.
                    let reach = p.half + p.radius;
                    if p.pos[1] > reach
                        && p.pos[1] + reach < medium.ceiling
                        && pressed[k * 2] <= 0.0
                        && pressed[k * 2 + 1] <= 0.0
                    {
                        continue;
                    }
                    for (e, end) in [-1.0f32, 1.0].into_iter().enumerate() {
                        if ground(p, end, medium.ceiling, &mut pressed[k * 2 + e]) {
                            self.grounded = true;
                        }
                    }
                }
            }
            // Limbs bump rather than pass through each other. Last, and twice
            // over, so the muscles and hinges can't force them through.
            if let Some(pairs) = &self.pairs {
                for _ in 0..2 {
                    for &(a, b) in pairs {
                        let (a, b) = pair_mut(&mut self.parts, a as usize, b as usize);
                        collide(a, b);
                    }
                }
            }
            for p in &mut self.parts {
                p.vel = [(p.pos[0] - p.prev[0]) / h, (p.pos[1] - p.prev[1]) / h];
                p.spin = (p.angle - p.prev[2]) / h;
            }
            for j in &self.joints {
                let (a, b) = pair_mut(&mut self.parts, j.parent as usize, j.child as usize);
                let rel = b.spin - a.spin;
                let k = j.damping / (a.inv_inertia + b.inv_inertia);
                let imp = rel * k;
                b.spin -= imp * b.inv_inertia;
                a.spin += imp * a.inv_inertia;
            }
        }
        self.work += energy;
        let speed: f32 =
            self.parts.iter().map(|p| p.vel[0].abs() + p.vel[1].abs()).sum::<f32>() / self.parts.len() as f32;
        if speed < self.scale * 0.02 {
            self.still += dt;
        } else {
            self.still = 0.0;
        }
    }
}

/// Keeps a child's base on its hinge on the parent.
fn hinge(a: &mut Part, b: &mut Part, anchor: f32) {
    let ta = a.axis();
    let pa = [a.pos[0] + ta[0] * anchor, a.pos[1] + ta[1] * anchor];
    let pb = b.base();
    let d = [pb[0] - pa[0], pb[1] - pa[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1e-12 {
        return;
    }
    let n = [d[0] / len, d[1] / len];
    let ra = [pa[0] - a.pos[0], pa[1] - a.pos[1]];
    let rb = [pb[0] - b.pos[0], pb[1] - b.pos[1]];
    let ca = cross(ra, n);
    let cb = cross(rb, n);
    let w = a.inv_mass + a.inv_inertia * ca * ca + b.inv_mass + b.inv_inertia * cb * cb;
    let l = -len / w;
    let p = [n[0] * l, n[1] * l];
    b.pos[0] += p[0] * b.inv_mass;
    b.pos[1] += p[1] * b.inv_mass;
    b.turn(b.inv_inertia * cross(rb, p));
    a.pos[0] -= p[0] * a.inv_mass;
    a.pos[1] -= p[1] * a.inv_mass;
    a.turn(-a.inv_inertia * cross(ra, p));
}

/// Pushes one end of a part out of the ground (or down from the ceiling) and
/// applies friction where it touches, as much as the deepest press this
/// substep allows. Returns whether it touched the ground.
fn ground(p: &mut Part, end: f32, ceiling: f32, pressed: &mut f32) -> bool {
    let t = p.axis();
    let r = [t[0] * p.half * end, t[1] * p.half * end];
    let e = [p.pos[0] + r[0], p.pos[1] + r[1]];
    if e[1] + p.radius > ceiling {
        p.pos[1] -= e[1] + p.radius - ceiling;
    }
    let pen = p.radius - e[1];
    if pen <= 0.0 && (*pressed <= 0.0 || pen < -p.radius * 0.1) {
        return false;
    }
    if pen > 0.0 {
        // Normal: straight up.
        let w = p.inv_mass + p.inv_inertia * r[0] * r[0];
        let l = pen / w;
        p.pos[1] += l * p.inv_mass;
        p.turn(p.inv_inertia * r[0] * l);
        *pressed += pen;
    }
    // Friction: undo sliding of the contact point, up to what the grip allows.
    let prev_e = p.prev[0] + p.prev_rot[0] * p.half * end;
    let t = p.axis();
    let now_e = p.pos[0] + t[0] * p.half * end;
    let dx = now_e - prev_e;
    let mu = if p.spiny { if dx * t[0] < 0.0 { p.grip } else { 0.15 } } else { p.grip };
    let limit = mu * *pressed;
    let fix = if dx.abs() <= limit { -dx } else { -dx.signum() * limit };
    let r = [t[0] * p.half * end, t[1] * p.half * end];
    let wt = p.inv_mass + p.inv_inertia * r[1] * r[1];
    let lt = fix / wt;
    p.pos[0] += lt * p.inv_mass;
    p.turn(p.inv_inertia * (-r[1] * lt));
    true
}

/// The closest points of segments `a0`-`a1` and `b0`-`b1`, as fractions
/// along each, and the distance between them.
fn closest(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> (f32, f32, f32) {
    let d1 = [a1[0] - a0[0], a1[1] - a0[1]];
    let d2 = [b1[0] - b0[0], b1[1] - b0[1]];
    let r = [a0[0] - b0[0], a0[1] - b0[1]];
    let a = d1[0] * d1[0] + d1[1] * d1[1];
    let e = d2[0] * d2[0] + d2[1] * d2[1];
    let f = d2[0] * r[0] + d2[1] * r[1];
    let c = d1[0] * r[0] + d1[1] * r[1];
    let b = d1[0] * d2[0] + d1[1] * d2[1];
    let denom = a * e - b * b;
    let mut s = if denom > 1e-20 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
    let mut t = if e > 1e-20 { (b * s + f) / e } else { 0.0 };
    if t < 0.0 {
        t = 0.0;
        s = if a > 1e-20 { (-c / a).clamp(0.0, 1.0) } else { 0.0 };
    } else if t > 1.0 {
        t = 1.0;
        s = if a > 1e-20 { ((b - c) / a).clamp(0.0, 1.0) } else { 0.0 };
    }
    let pa = [a0[0] + d1[0] * s, a0[1] + d1[1] * s];
    let pb = [b0[0] + d2[0] * t, b0[1] + d2[1] * t];
    (s, t, ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt())
}

/// Pushes two pieces of one body apart where they overlap.
fn collide(a: &mut Part, b: &mut Part) {
    let reach = a.half + b.half + a.radius + b.radius;
    let gap = [b.pos[0] - a.pos[0], b.pos[1] - a.pos[1]];
    if gap[0] * gap[0] + gap[1] * gap[1] > reach * reach {
        return;
    }
    let (a0, a1, b0, b1) = (a.base(), a.tip(), b.base(), b.tip());
    let (s, t, d) = closest(a0, a1, b0, b1);
    let sum = a.radius + b.radius;
    if d >= sum {
        return;
    }
    let at = |s: f32, p: [f32; 2], q: [f32; 2]| [p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s];
    let (pa, pb, n, pen) = if d > 1e-6 {
        let (pa, pb) = (at(s, a0, a1), at(t, b0, b1));
        (pa, pb, [(pb[0] - pa[0]) / d, (pb[1] - pa[1]) / d], sum - d)
    } else {
        // Crossed: the shortest way out is to carry one piece's nearer end
        // back over the other's axis.
        let out = |p0: [f32; 2], axis: [f32; 2], q0: [f32; 2], q1: [f32; 2]| {
            let n = [-axis[1], axis[0]];
            let d0 = (q0[0] - p0[0]) * n[0] + (q0[1] - p0[1]) * n[1];
            let d1 = (q1[0] - p0[0]) * n[0] + (q1[1] - p0[1]) * n[1];
            let (end, near, far) = if d0.abs() < d1.abs() { (q0, d0, d1) } else { (q1, d1, d0) };
            let n = [n[0] * far.signum(), n[1] * far.signum()];
            let foot = [end[0] - n[0] * near.abs(), end[1] - n[1] * near.abs()];
            (foot, end, n, near.abs() + sum)
        };
        let (fb, eb, nb, pb_) = out(a0, a.axis(), b0, b1);
        let (fa, ea, na, pa_) = out(b0, b.axis(), a0, a1);
        if pb_ <= pa_ { (fb, eb, nb, pb_) } else { (ea, fa, [-na[0], -na[1]], pa_) }
    };
    let ra = [pa[0] - a.pos[0], pa[1] - a.pos[1]];
    let rb = [pb[0] - b.pos[0], pb[1] - b.pos[1]];
    let ca = cross(ra, n);
    let cb = cross(rb, n);
    let w = a.inv_mass + a.inv_inertia * ca * ca + b.inv_mass + b.inv_inertia * cb * cb;
    let l = pen / w;
    b.pos[0] += n[0] * l * b.inv_mass;
    b.pos[1] += n[1] * l * b.inv_mass;
    b.turn(b.inv_inertia * cb * l);
    a.pos[0] -= n[0] * l * a.inv_mass;
    a.pos[1] -= n[1] * l * a.inv_mass;
    a.turn(-a.inv_inertia * ca * l);
}

fn pair_mut(v: &mut [Part], a: usize, b: usize) -> (&mut Part, &mut Part) {
    assert!(a != b);
    if a < b {
        let (lo, hi) = v.split_at_mut(b);
        (&mut lo[a], &mut hi[0])
    } else {
        let (lo, hi) = v.split_at_mut(a);
        (&mut hi[0], &mut lo[b])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::{Founder, Genome};
    use crate::rng::Rng;

    fn rig_of(kind: Founder, seed: u64) -> (Genome, Rig) {
        let mut rng = Rng::new(seed);
        let g = Genome::founder(kind, &mut rng);
        let body = Body::develop(&g);
        let mask = present(&body, 1.0, 0);
        let scale = g.adult_scale;
        let alt = if matches!(kind, Founder::Swimmer | Founder::Drifter) { 0.1 } else { 0.0 };
        (g, Rig::new(&body, mask, scale, alt))
    }

    fn run(rig: &mut Rig, g: &Genome, power: f32, density: f32, secs: f32) -> f32 {
        let start = rig.centre()[0];
        let medium = Medium { density, flow: [0.0; 2], ceiling: 0.68 };
        for _ in 0..(secs * HZ) as usize {
            rig.step(Drive { power, rise: 0.0, hz: g.gait_hz, limp: false }, medium);
        }
        rig.centre()[0] - start
    }

    #[test]
    fn bodies_hold_together_and_settle() {
        for seed in 0..20 {
            for kind in [Founder::Grazer, Founder::Crawler, Founder::Swimmer, Founder::Drifter] {
                let (g, mut rig) = rig_of(kind, seed);
                run(&mut rig, &g, 0.0, AIR, 4.0);
                for p in &rig.parts {
                    assert!(p.pos[0].is_finite() && p.pos[1].is_finite(), "{kind:?} {seed} blew up");
                    assert!(p.pos[1] > -p.half * 2.0 - p.radius * 2.0, "{kind:?} {seed} fell through the ground");
                }
                // Every hinge still joined.
                for j in &rig.joints {
                    let a = &rig.parts[j.parent as usize];
                    let b = &rig.parts[j.child as usize];
                    let t = a.axis();
                    let pa = [a.pos[0] + t[0] * j.anchor, a.pos[1] + t[1] * j.anchor];
                    let pb = b.base();
                    let gap = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt();
                    // Within the thickness of a limb, where it doesn't show.
                    assert!(gap < rig.scale * 0.15, "{kind:?} {seed}: joint pulled apart by {gap}");
                }
            }
        }
    }

    #[test]
    fn limbs_on_one_side_keep_out_of_each_other() {
        let (mut samples, mut crossed, mut deep) = (0u32, 0u32, 0.0f32);
        for seed in 0..10 {
            for kind in [Founder::Grazer, Founder::Crawler, Founder::Swimmer] {
                let (g, mut rig) = rig_of(kind, seed);
                let pairs = rig.pairs.clone().unwrap();
                let medium = Medium { density: AIR, flow: [0.0; 2], ceiling: 0.68 };
                for _ in 0..(6.0 * HZ) as usize {
                    rig.step(Drive { power: 1.0, rise: 0.0, hz: g.gait_hz, limp: false }, medium);
                    for &(a, b) in &pairs {
                        let (pa, pb) = (&rig.parts[a as usize], &rig.parts[b as usize]);
                        let (_, _, d) = closest(pa.base(), pa.tip(), pb.base(), pb.tip());
                        let sank = ((pa.radius + pb.radius - d) / (pa.radius + pb.radius)).max(0.0);
                        samples += 1;
                        deep += sank;
                        if sank > 0.9 {
                            crossed += 1;
                        }
                    }
                }
            }
        }
        let rate = crossed as f32 / samples as f32;
        let mean = deep / samples as f32;
        assert!(rate < 0.002, "limbs passed through each other in {:.2}% of looks", rate * 100.0);
        assert!(mean < 0.01, "limbs sank {mean:.3} of their thickness into each other on average");
    }

    #[test]
    fn heavy_bodies_fall_and_floats_rise() {
        let (g, mut rig) = rig_of(Founder::Grazer, 3);
        for p in &mut rig.parts {
            p.pos[1] += 0.2;
        }
        run(&mut rig, &g, 0.0, AIR, 6.0);
        assert!(rig.lowest() < rig.scale * 0.2, "a grazer should come down: {}", rig.lowest());
        assert!(rig.grounded);
    }

    #[test]
    #[ignore]
    fn report_gaits() {
        for kind in [Founder::Grazer, Founder::Crawler, Founder::Swimmer, Founder::Drifter] {
            for seed in 0..4 {
                let (g, mut rig) = rig_of(kind, seed);
                run(&mut rig, &g, 0.0, AIR, 2.0);
                let rest = rig.lowest();
                let mut wet = rig.clone();
                let dw = run(&mut wet, &g, 1.0, WATER, 10.0) / rig.scale / 10.0;
                let back = run(&mut rig.clone(), &g, -1.0, AIR, 10.0) / rig.scale / 10.0;
                let idle = run(&mut rig.clone(), &g, 0.0, AIR, 10.0) / rig.scale / 10.0;
                print!("idle {idle:.3} water {dw:.2} reverse {back:.2} | ");
                let d = run(&mut rig, &g, 1.0, AIR, 10.0);
                let mut worst = 0.0f32;
                for j in &rig.joints {
                    let a = &rig.parts[j.parent as usize];
                    let b = &rig.parts[j.child as usize];
                    let t = a.axis();
                    let pa = [a.pos[0] + t[0] * j.anchor, a.pos[1] + t[1] * j.anchor];
                    let pb = b.base();
                    worst = worst.max(((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt() / rig.scale);
                }
                println!(
                    "{kind:?} {seed}: {} parts, moved {:.2} body units/s, low {:.2}, centre y {:.2}, gap {:.3}, work {:.2e}, root angle {:.2}",
                    rig.parts.len(),
                    d / rig.scale / 10.0,
                    rest / rig.scale,
                    rig.centre()[1] / rig.scale,
                    worst,
                    rig.work,
                    rig.parts[0].angle
                );
            }
        }
    }

    #[test]
    fn founders_can_move_under_their_own_power() {
        let mut moved = 0;
        for seed in 0..10 {
            let (g, mut rig) = rig_of(Founder::Grazer, seed);
            run(&mut rig, &g, 0.0, AIR, 2.0);
            let d = run(&mut rig, &g, 1.0, AIR, 10.0).abs();
            if d > rig.scale * 0.5 {
                moved += 1;
            }
        }
        assert!(moved >= 5, "only {moved} of 10 grazers got anywhere");
        let mut swam = 0;
        for seed in 0..10 {
            let (g, mut rig) = rig_of(Founder::Swimmer, seed);
            let d = run(&mut rig, &g, 1.0, AIR, 10.0).abs();
            if d > rig.scale * 0.5 {
                swam += 1;
            }
        }
        assert!(swam >= 5, "only {swam} of 10 swimmers got anywhere");
    }
}

#[cfg(test)]
mod explore {
    use super::*;
    use crate::genome::{Founder, Genome};
    use crate::rng::Rng;

    #[test]
    #[ignore]
    fn crawler_variants() {
        for (amp_body, amp_leg, step) in
            [(0.2, 0.6, 0.8), (0.5, 0.6, 0.8), (0.5, 0.6, 0.2), (0.8, 0.3, 0.8), (0.8, 0.3, 0.75), (0.5, 1.0, 0.85)]
        {
            let mut total = 0.0;
            let mut back = 0.0;
            for seed in 0..6 {
                let mut rng = Rng::new(seed);
                let mut g = Genome::founder(Founder::Crawler, &mut rng);
                for p in &mut g.parts {
                    for l in &mut p.links {
                        if l.to == 1 {
                            l.motor.amp = amp_body;
                            if p.tissue == Tissue::Armor {
                                l.motor.phase = step;
                            }
                        }
                        if l.to == 3 {
                            l.motor.amp = amp_leg;
                        }
                    }
                }
                let body = Body::develop(&g);
                let mut rig = Rig::new(&body, present(&body, 1.0, 0), g.adult_scale, 0.0);
                let m = Medium { density: AIR, flow: [0.0; 2], ceiling: 0.68 };
                let x0 = rig.centre()[0];
                let mut r2 = rig.clone();
                for _ in 0..300 {
                    rig.step(Drive { power: 1.0, rise: 0.0, hz: g.gait_hz, limp: false }, m);
                    r2.step(Drive { power: -1.0, rise: 0.0, hz: g.gait_hz, limp: false }, m);
                }
                total += (rig.centre()[0] - x0) / rig.scale / 10.0;
                back += (r2.centre()[0] - x0) / rig.scale / 10.0;
            }
            println!("body {amp_body} leg {amp_leg} step {step}: fwd {:.3} back {:.3}", total / 6.0, back / 6.0);
        }
    }

    /// Searches the founders' motor genes for gaits that get somewhere, to
    /// seed the world with bodies that can already move.
    #[test]
    #[ignore]
    fn tune_founders() {
        fn speed(g: &Genome) -> f32 {
            let mut total = 0.0;
            for k in [0.85f32, 1.0, 1.2] {
                let body = Body::develop(g);
                let scale = g.adult_scale * k;
                let mut rig = Rig::new(&body, present(&body, 1.0, 0), scale, 0.0);
                let medium = Medium { density: AIR, flow: [0.0; 2], ceiling: 0.68 };
                for _ in 0..60 {
                    rig.step(Drive { power: 0.0, rise: 0.0, hz: g.gait_hz, limp: false }, medium);
                }
                let start = rig.centre()[0];
                let level = rig.parts[0].angle;
                let mut pitch = 0.0;
                for _ in 0..240 {
                    rig.step(Drive { power: 1.0, rise: 0.0, hz: g.gait_hz, limp: false }, medium);
                    pitch += wrap(rig.parts[0].angle - level).abs() / 240.0;
                }
                // Getting somewhere, but not by bucking and rearing.
                total += ((rig.centre()[0] - start) / scale / 8.0).abs() - 3.0 * pitch;
            }
            total / 3.0
        }
        for kind in [Founder::Grazer, Founder::Crawler, Founder::Swimmer] {
            let mut rng = Rng::new(7);
            let mut best = Genome::founder_plan(kind, &mut rng);
            let mut score = speed(&best);
            println!("{kind:?} start {score:.3}");
            for round in 0..400 {
                let mut g = best.clone();
                let r = 0.3 * (1.0 - round as f32 / 400.0) + 0.05;
                if rng.chance(0.2) {
                    g.gait_hz = (g.gait_hz + rng.signed() * r * 2.0).clamp(0.5, 4.0);
                }
                for p in &mut g.parts {
                    for l in &mut p.links {
                        if l.motor.amp > 0.0 && rng.chance(0.3) {
                            l.motor.amp = (l.motor.amp + rng.signed() * r).clamp(0.05, 1.2);
                            l.motor.phase = (l.motor.phase + rng.signed() * r).rem_euclid(1.0);
                            if l.pair {
                                l.motor.pair = (l.motor.pair + rng.signed() * r).rem_euclid(1.0);
                            }
                        }
                        if l.motor.amp > 0.0 && rng.chance(0.1) {
                            l.angle += rng.signed() * r;
                        }
                    }
                }
                let s = speed(&g);
                if s > score {
                    score = s;
                    best = g;
                }
            }
            println!("{kind:?} best {score:.3} hz {:.2}", best.gait_hz);
            for (i, p) in best.parts.iter().enumerate() {
                for l in &p.links {
                    if l.motor.amp > 0.0 {
                        println!(
                            "  part {i} -> {}: angle {:.2} amp {:.2} phase {:.2} pair {:.2}",
                            l.to, l.angle, l.motor.amp, l.motor.phase, l.motor.pair
                        );
                    }
                }
            }
        }
    }
}
