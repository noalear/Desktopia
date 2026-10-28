//! Stand-in bodies for Vesper's creatures and plants.
//!
//! The simulation owns real development (genome to body) and real motion. This
//! crate exists so the renderer can be judged before that lands: it grows a
//! varied cast of body plans from random form genes, moves them with simple
//! procedural gaits, and emits them in exactly the draw-list shape the
//! simulation's pose step will.

pub mod creature;
pub mod plant;
mod rope;

use vesper_render::{LifeBatch, Look, Organism};
use vesper_terrain as terrain;

pub use creature::{Creature, Plan};
pub use plant::{Plant, PlantKind};

/// A tiny deterministic generator, so a seed always grows the same world.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in 0..1.
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }

    pub fn int(&mut self, lo: usize, hi_inclusive: usize) -> usize {
        lo + (self.next_u64() % (hi_inclusive - lo + 1) as u64) as usize
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }

    /// Roughly normal, mean 0, deviation 1.
    pub fn normal(&mut self) -> f32 {
        (self.f() + self.f() + self.f() + self.f() - 2.0) * 1.732
    }

    pub fn look(&mut self) -> Look {
        let mut genes = [0.0; 16];
        for g in &mut genes {
            *g = self.f();
        }
        Look { genes }
    }

    /// A relative's look: close to `base`, as inheritance with mutation would give.
    pub fn kin(&mut self, base: &Look, spread: f32) -> Look {
        let mut genes = base.genes;
        for g in &mut genes {
            *g = (*g + self.normal() * spread).clamp(0.0, 1.0);
        }
        Look { genes }
    }
}

/// How much smaller things are toward the back of the ground plane.
pub fn perspective(depth: f32) -> f32 {
    1.0 - 0.42 * depth
}

/// Draw order for something at a depth on the ground plane.
pub fn order(depth: f32) -> f32 {
    0.08 + 0.55 * depth
}

/// A small world of stand-in life for previews.
pub struct Menagerie {
    pub creatures: Vec<Creature>,
    pub plants: Vec<Plant>,
    pub river: terrain::RiverState,
}

impl Menagerie {
    pub fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let river = terrain::RiverState::default();
        let plants = plant::populate(&mut rng, river);
        let creatures = creature::populate(&mut rng);
        Self { creatures, plants, river }
    }

    /// Advances every creature by `dt` seconds of world time.
    pub fn step(&mut self, dt: f32) {
        for c in &mut self.creatures {
            c.step(dt);
        }
    }

    /// Runs `secs` of motion in small steps, so trailing parts settle.
    pub fn warm_up(&mut self, secs: f32) {
        let dt = 1.0 / 30.0;
        for _ in 0..(secs / dt) as usize {
            self.step(dt);
        }
    }

    pub fn plants_batch(&self, batch: &mut LifeBatch) {
        batch.clear();
        for p in &self.plants {
            batch.add(&Organism {
                segs: &p.segs,
                membranes: &p.membranes,
                look: &p.look,
                z: p.z,
                alive: p.alive,
                signal: 0.0,
                symbol: 0,
                bleeding: 0.0,
                infection: None,
                clip_y: None,
                ground: p.segs.first().map(|s| s.a),
                seed: p.seed,
            });
        }
    }

    pub fn creatures_batch(&self, batch: &mut LifeBatch) {
        batch.clear();
        let (mut segs, mut mems) = (Vec::new(), Vec::new());
        for c in &self.creatures {
            segs.clear();
            mems.clear();
            c.emit(&mut segs, &mut mems);
            batch.add(&Organism {
                segs: &segs,
                membranes: &mems,
                look: &c.look,
                z: c.z(),
                alive: c.alive,
                signal: c.signal(),
                symbol: c.symbol,
                bleeding: c.bleeding,
                infection: c.infection,
                clip_y: c.clip_y(),
                ground: Some(c.ground()),
                seed: c.seed,
            });
        }
    }
}

/// Gives the lineup's `i`th creature one condition each, so they can be
/// compared side by side: healthy, a microbe, a parasite, a symbiont,
/// bleeding, and the rest flashing different signals.
fn show_state(c: &mut Creature, i: usize) {
    use vesper_render::Infection;
    c.infection = None;
    c.bleeding = 0.0;
    c.signal_hold = None;
    match i {
        1 => c.infection = Some(Infection { load: 0.9, visible: 0.0, hue: 0.2, symbiont: false }),
        2 => c.infection = Some(Infection { load: 0.9, visible: 1.0, hue: 0.1, symbiont: false }),
        3 => c.infection = Some(Infection { load: 0.9, visible: 0.0, hue: 0.45, symbiont: true }),
        4 => c.bleeding = 1.0,
        0 => {}
        _ => {
            c.signal_hold = Some(1.0);
            c.symbol = (i * 5 % 16) as u8;
        }
    }
}

pub(crate) fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

pub(crate) fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn mul(a: [f32; 2], s: f32) -> [f32; 2] {
    [a[0] * s, a[1] * s]
}

pub(crate) fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

pub(crate) fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

pub(crate) fn polar(angle: f32, r: f32) -> [f32; 2] {
    [angle.cos() * r, angle.sin() * r]
}

pub(crate) fn rot(v: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

impl Menagerie {
    /// One of each body plan lined up for inspection: ground forms in a row
    /// across the floor from `x0`, swimmers and fliers in rows above.
    pub fn lineup(seed: u64, x0: f32, width: f32) -> Self {
        let mut rng = Rng::new(seed);
        let ground =
            [Plan::Walker, Plan::Walker, Plan::Walker, Plan::Wader, Plan::Walker, Plan::Burrower, Plan::Walker];
        let air = [Plan::Swimmer, Plan::Floater, Plan::Glider, Plan::Swimmer, Plan::Floater, Plan::Swimmer];
        let mut creatures = Vec::new();
        for (i, &plan) in ground.iter().enumerate() {
            let form = creature::Form::random(&mut rng, plan);
            let look = rng.look();
            let x = x0 + width * (i as f32 + 0.5) / ground.len() as f32;
            let mut c = Creature::new(form, look, &mut rng, x, 0.12);
            c.hold();
            show_state(&mut c, i);
            creatures.push(c);
        }
        for (i, &plan) in air.iter().enumerate() {
            let form = creature::Form::random(&mut rng, plan);
            let look = rng.look();
            let x = x0 + width * (i as f32 + 0.5) / air.len() as f32;
            let mut c = Creature::new(form, look, &mut rng, x, 0.12);
            c.alt = 0.17 + 0.1 * (i % 2) as f32;
            c.hold();
            show_state(&mut c, i + 3);
            creatures.push(c);
        }
        Self { creatures, plants: Vec::new(), river: terrain::RiverState::default() }
    }
}
