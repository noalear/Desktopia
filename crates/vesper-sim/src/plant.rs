//! The rooted kingdom. Plants evolve under the same rules as creatures: a
//! genome grows a branching body, and light, vents, water and soil decide
//! whether it pays. Defences (spines, poison, traps) and ways of spreading
//! (wind, water, fruit eaten by animals) are all heritable.

use crate::body::{Body, Segment};
use crate::genome::{LOOK_GENES, Tissue};
use crate::history::{PlantId, SpeciesId};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlantGenome {
    /// World units per body unit when full grown.
    pub adult_scale: f32,
    /// Stem segments in the trunk (1..=6).
    pub stem_segments: u8,
    pub stem_length: f32,
    pub stem_width: f32,
    /// Bend per stem segment, radians; plants lean and curl.
    pub curl: f32,
    /// Branches per stem node (0..=3).
    pub branches: u8,
    pub branch_angle: f32,
    pub branch_scale: f32,
    /// Light film area per tip, body units.
    pub leaf: f32,
    /// Leaves spread as a fan from each tip (1..=4).
    pub leaves: u8,
    /// Root reach for soil nutrient, 0..1 (hidden, but costs matter).
    pub root: f32,
    /// 0 feeds on light, 1 on vent chemistry.
    pub vent: f32,
    /// How much it depends on wet ground, 0..1.
    pub thirst: f32,
    pub spines: f32,
    pub toxin: f32,
    /// Jaw-like traps at the tips that catch small animals.
    pub trap: f32,
    /// Sweet fruit that animals eat, carrying seeds away.
    pub fruit: f32,
    /// Matter per seed, as a fraction of adult matter.
    pub seed_size: f32,
    /// How far wind carries seeds.
    pub wind: f32,
    /// Above 0.5 the plant lets go of the ground and drifts as a floating mat.
    pub floating: f32,
    pub glow: f32,
    pub heat_tolerance: f32,
    pub cold_tolerance: f32,
    /// Lifespan in seconds.
    pub lifespan: f32,
    pub growth: f32,
    pub mutation: f32,
    pub look: [f32; LOOK_GENES],
    pub marker: [u8; 8],
}

#[derive(Clone, Copy, Debug)]
pub enum Seedling {
    /// Low light-film fans that track the sun.
    Fan,
    /// Crusts that feed on vent chemistry.
    VentCrust,
    /// Thirsty thickets along the river.
    Thicket,
    /// Floating mats on the wind.
    Mat,
}

impl PlantGenome {
    pub fn founder(kind: Seedling, rng: &mut Rng) -> Self {
        let mut look = [0.0; LOOK_GENES];
        for v in look.iter_mut() {
            *v = rng.f32();
        }
        let mut marker = [0u8; 8];
        for m in marker.iter_mut() {
            *m = rng.below(256) as u8;
        }
        let mut g = Self {
            adult_scale: 0.02,
            stem_segments: 2,
            stem_length: 1.0,
            stem_width: 0.12,
            curl: rng.signed() * 0.2,
            branches: 1,
            branch_angle: 0.7,
            branch_scale: 0.7,
            leaf: 0.6,
            leaves: 2,
            root: 0.5,
            vent: 0.0,
            thirst: 0.3,
            spines: rng.range(0.0, 0.1),
            toxin: rng.range(0.0, 0.1),
            trap: 0.0,
            fruit: rng.range(0.0, 0.15),
            seed_size: 0.04,
            wind: 0.4,
            floating: 0.0,
            glow: 0.0,
            heat_tolerance: 0.5,
            cold_tolerance: 0.3,
            lifespan: rng.range(1500.0, 3000.0),
            growth: 1.0,
            mutation: 1.0,
            look,
            marker,
        };
        match kind {
            Seedling::Fan => {
                g.adult_scale = rng.range(0.015, 0.03);
                g.stem_segments = 2;
                g.leaf = 1.1;
                g.leaves = 3;
                g.branches = 0;
            }
            Seedling::VentCrust => {
                g.adult_scale = rng.range(0.006, 0.012);
                g.stem_segments = 1;
                g.stem_length = 0.5;
                g.branches = 2;
                g.branch_angle = 1.2;
                g.leaf = 0.3;
                g.vent = 0.9;
                g.glow = 0.5;
                g.heat_tolerance = 0.8;
                g.cold_tolerance = 0.6;
            }
            Seedling::Thicket => {
                g.adult_scale = rng.range(0.02, 0.035);
                g.stem_segments = 3;
                g.branches = 2;
                g.thirst = 0.9;
                g.leaf = 0.5;
                g.fruit = rng.range(0.2, 0.4);
            }
            Seedling::Mat => {
                g.adult_scale = rng.range(0.01, 0.018);
                g.stem_segments = 1;
                g.stem_length = 0.6;
                g.branches = 3;
                g.branch_angle = 1.5;
                g.leaf = 0.7;
                g.floating = 0.9;
                g.thirst = 0.0;
                g.root = 0.1;
            }
        }
        for _ in 0..2 {
            g.mutate(rng);
        }
        g
    }

    pub fn mutate(&mut self, rng: &mut Rng) {
        let r = self.mutation.clamp(0.3, 2.5);
        let nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32, rng: &mut Rng| {
            if rng.chance(0.15 * r) {
                *v = (*v + rng.normal() * sd).clamp(lo, hi);
            }
        };
        nudge(&mut self.adult_scale, 0.0015, 0.004, 0.07, rng);
        nudge(&mut self.stem_length, 0.08, 0.2, 2.5, rng);
        nudge(&mut self.stem_width, 0.015, 0.04, 0.4, rng);
        nudge(&mut self.curl, 0.05, -0.8, 0.8, rng);
        nudge(&mut self.branch_angle, 0.08, 0.1, 2.2, rng);
        nudge(&mut self.branch_scale, 0.05, 0.3, 1.0, rng);
        nudge(&mut self.leaf, 0.06, 0.05, 2.0, rng);
        nudge(&mut self.root, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.vent, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.thirst, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.spines, 0.03, 0.0, 1.0, rng);
        nudge(&mut self.toxin, 0.03, 0.0, 1.0, rng);
        nudge(&mut self.trap, 0.02, 0.0, 1.0, rng);
        nudge(&mut self.fruit, 0.03, 0.0, 1.0, rng);
        nudge(&mut self.seed_size, 0.005, 0.005, 0.3, rng);
        nudge(&mut self.wind, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.floating, 0.02, 0.0, 1.0, rng);
        nudge(&mut self.glow, 0.03, 0.0, 1.0, rng);
        nudge(&mut self.heat_tolerance, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.cold_tolerance, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.lifespan, 150.0, 300.0, 20000.0, rng);
        nudge(&mut self.growth, 0.05, 0.3, 2.0, rng);
        nudge(&mut self.mutation, 0.05, 0.3, 2.5, rng);
        for v in self.look.iter_mut() {
            nudge(v, 0.04, 0.0, 1.0, rng);
        }
        if rng.chance(0.03 * r) {
            self.stem_segments = (self.stem_segments as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(1, 6) as u8;
        }
        if rng.chance(0.03 * r) {
            self.branches = (self.branches as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(0, 3) as u8;
        }
        if rng.chance(0.03 * r) {
            self.leaves = (self.leaves as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(1, 4) as u8;
        }
        for m in self.marker.iter_mut() {
            if rng.chance(0.03) {
                *m = rng.below(256) as u8;
            }
        }
    }

    pub fn floats(&self) -> bool {
        self.floating > 0.5
    }

    /// Matter of a full-grown plant.
    pub fn adult_matter(&self, body: &Body) -> f32 {
        let area: f32 = body.segments.iter().map(|s| s.area() * s.tissue.density()).sum();
        area * self.adult_scale * self.adult_scale * crate::world::MATTER_DENSITY * (1.0 + self.root * 0.5)
    }

    /// Grows the plant's body: a trunk of stem segments with branches, leaves
    /// as membranes at the tips, and spines, traps, fruit or glow where the
    /// genes put them. Up is +y.
    pub fn develop(&self) -> Body {
        let mut segs: Vec<Segment> = Vec::new();
        let up = std::f32::consts::FRAC_PI_2;
        let push =
            |segs: &mut Vec<Segment>, parent: i16, tissue, length: f32, width: f32, angle: f32, depth: u8, side: i8| {
                segs.push(Segment {
                    parent,
                    tissue,
                    length,
                    width,
                    angle,
                    along: 1.0,
                    side,
                    onset: 0.0,
                    membrane: tissue == Tissue::LightFilm,
                    depth,
                    base: [0.0; 2],
                    tip: [0.0; 2],
                    dir: 0.0,
                    amp: 0.0,
                    phase: 0.0,
                    lift: 0.0,
                });
                (segs.len() - 1) as i16
            };
        let mut tips: Vec<(i16, f32, u8)> = Vec::new();
        let mut parent = -1i16;
        for k in 0..self.stem_segments {
            let angle = if parent < 0 { up } else { self.curl };
            let len = self.stem_length * (1.0 - k as f32 * 0.12);
            parent = push(&mut segs, parent, Tissue::Frame, len, self.stem_width * len, angle, k, 0);
            for b in 0..self.branches {
                let sign = if (b + k) % 2 == 0 { 1.0 } else { -1.0 };
                let blen = len * self.branch_scale;
                let side = if b == 2 { 1 } else { 0 };
                let br = push(
                    &mut segs,
                    parent,
                    Tissue::Frame,
                    blen,
                    self.stem_width * blen * 0.8,
                    sign * self.branch_angle,
                    k + 1,
                    side,
                );
                tips.push((br, blen, k + 2));
            }
        }
        tips.push((parent, self.stem_length, self.stem_segments));
        for &(tip, len, depth) in &tips {
            if segs.len() > 40 {
                break;
            }
            let n = self.leaves.max(1);
            for l in 0..n {
                let a = (l as f32 - (n - 1) as f32 * 0.5) * 0.5;
                push(&mut segs, tip, Tissue::LightFilm, len * self.leaf, len * self.leaf * 0.5, a, depth, 0);
            }
            if self.spines > 0.3 {
                push(&mut segs, tip, Tissue::Spine, len * 0.4 * self.spines, 0.03, 1.2, depth, 0);
            }
            if self.trap > 0.3 {
                push(&mut segs, tip, Tissue::Mouth, len * 0.5 * self.trap, len * 0.3, 0.0, depth, 0);
            }
            if self.fruit > 0.25 {
                push(&mut segs, tip, Tissue::Brood, len * 0.25, len * 0.25, -0.8, depth, 0);
            }
            if self.glow > 0.4 {
                push(&mut segs, tip, Tissue::Glow, len * 0.15, len * 0.15, 0.3, depth, 0);
            }
        }
        // Lay out from the base, without centring: the root sits at the ground.
        for i in 0..segs.len() {
            let (base, dir) = if segs[i].parent < 0 {
                ([0.0, 0.0], 0.0)
            } else {
                let p = &segs[segs[i].parent as usize];
                (p.tip, p.dir)
            };
            let s = &mut segs[i];
            s.base = base;
            s.dir = dir + s.angle;
            s.tip = [base[0] + s.dir.cos() * s.length, base[1] + s.dir.sin() * s.length];
        }
        let extent = segs.iter().map(|s| s.tip[1]).fold(0.3, f32::max);
        Body { segments: segs, extent }
    }

    pub fn distance(&self, o: &Self) -> f32 {
        let mut d = 0.0;
        d += ((self.adult_scale - o.adult_scale) / 0.01).abs();
        d += (self.stem_segments as f32 - o.stem_segments as f32).abs() * 0.5;
        d += (self.branches as f32 - o.branches as f32).abs() * 0.5;
        d += (self.leaves as f32 - o.leaves as f32).abs() * 0.3;
        for (a, b) in [
            (self.vent, o.vent),
            (self.thirst, o.thirst),
            (self.spines, o.spines),
            (self.toxin, o.toxin),
            (self.trap, o.trap),
            (self.fruit, o.fruit),
            (self.floating, o.floating),
            (self.glow, o.glow),
            (self.leaf / 2.0, o.leaf / 2.0),
        ] {
            d += (a - b).abs() * 2.0;
        }
        d += crate::genome::Genome::look_distance(&self.look, &o.look) * 3.0;
        d += (1.0 - crate::genome::Genome::kinship(&self.marker, &o.marker)) * 1.5;
        d
    }
}

/// A seed carried in a gut, waiting to be dropped somewhere new.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Seed {
    pub genome: Arc<PlantGenome>,
    pub species: SpeciesId,
    pub generation: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plant {
    pub id: PlantId,
    pub species: SpeciesId,
    pub genome: Arc<PlantGenome>,
    #[serde(skip, default = "crate::body::empty")]
    pub body: Arc<Body>,
    pub x: f32,
    /// Depth on the ground plane, world units 0..DEPTH.
    pub depth: f32,
    /// Altitude; only floating mats leave the ground.
    pub alt: f32,
    pub matter: f32,
    pub adult_matter: f32,
    pub age: f32,
    /// 0..1, falls with grazing, heat and cold.
    pub health: f32,
    /// Matter held in fruit, eaten by animals.
    pub fruit: f32,
    /// Matter set aside for the next seed.
    pub seeds: f32,
    /// Seconds until a trap can close again; positive while closed.
    pub trap_shut: f32,
    /// How much light it got lately, 0..1, for the renderer to open or close leaves.
    pub vigor: f32,
    pub born: f64,
    pub generation: u32,
    /// Weather worn into it, each 0..1: wilting from thirst, frost-bitten
    /// and sun-scorched leaves, and a coat of ash or dust over them.
    #[serde(default)]
    pub wilt: f32,
    #[serde(default)]
    pub frost: f32,
    #[serde(default)]
    pub burn: f32,
    #[serde(default)]
    pub coat: f32,
}

impl Plant {
    pub fn growth01(&self) -> f32 {
        (self.matter / self.adult_matter).clamp(0.0, 1.0)
    }

    /// Current world units per body unit.
    pub fn scale(&self) -> f32 {
        self.genome.adult_scale * self.growth01().sqrt().max(0.15)
    }

    pub fn height(&self) -> f32 {
        self.body.extent * self.scale()
    }

    pub fn leaf_area(&self) -> f32 {
        let s = self.scale();
        let film: f32 = self.body.segments.iter().filter(|s| s.tissue == Tissue::LightFilm).map(|s| s.area()).sum();
        film * s * s
    }
}
