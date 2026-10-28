//! The rooted kingdom. Plants evolve under the same rules as creatures: a
//! genome grows a branching body, and light, vents, water and soil decide
//! whether it pays. Defences (spines, poison, traps) and ways of spreading
//! (wind, water, fruit eaten by animals) are all heritable.

use crate::body::{Body, Segment};
use crate::genetics::{self, COMPAT};
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
    /// Passed down the seed parent's line only, like chloroplasts.
    pub marker: [u8; 8],
    /// How strongly being eaten raises its spines and poison for a while.
    pub induce: f32,
    /// How much it gives off a warning scent when eaten.
    pub warn: f32,
    /// How readily it takes up a neighbour's warning and arms itself in turn.
    pub listen: f32,
    /// How readily it shuts down to wait out heat, cold, drought or darkness.
    pub dormancy: f32,
    /// How far it can harden itself to the heat or cold it keeps meeting.
    pub hardening: f32,
    /// How much it holds its seed back to release all at once after rain.
    pub mast: f32,
    /// The two sets of genes it carries, from its seed parent and its pollen
    /// parent, that the fields above express. Missing in species records.
    pub haplotypes: Option<Arc<[PlantGenome; 2]>>,
    /// In a haplotype: each allele's strength of expression, in `loci` order.
    pub dominance: Vec<u8>,
    /// In a haplotype: genes broken by harmful mutations, one bit each.
    pub load: u64,
    /// In a haplotype: alleles of genes that must work with each other.
    pub compat: [u8; COMPAT],
    /// How fit its genes leave it, 1 sound .. 0.
    pub vigor: f32,
    /// Share of its seeds it fertilises with its own pollen.
    pub selfing: f32,
    /// Sweet lures that bring animals, which carry its pollen far.
    pub nectar: f32,
}

/// Quantitative plant loci, in chromosome order.
const PLANT_LOCI: usize = 47;
const PLANT_CHROMS: [usize; 5] = [0, 11, 21, 31, PLANT_LOCI];
/// Hue loci, which blend round the colour wheel.
const PLANT_HUES: [usize; 2] = [31, 34];
/// How far pollen carries on the wind, at the wind gene's full reach, and on animals.
pub const POLLEN_WIND: f32 = 0.25;
pub const POLLEN_ANIMAL: f32 = 0.6;

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
            induce: rng.range(0.0, 0.4),
            warn: rng.range(0.0, 0.3),
            listen: rng.range(0.0, 0.4),
            dormancy: rng.range(0.1, 0.5),
            hardening: rng.range(0.1, 0.5),
            mast: rng.range(0.0, 0.3),
            haplotypes: None,
            dominance: Vec::new(),
            load: 0,
            compat: [0; COMPAT],
            vigor: 1.0,
            selfing: rng.range(0.05, 0.5),
            nectar: rng.range(0.0, 0.2),
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
        // Two slightly different sets of genes, whose interacting genes all
        // work together.
        let mut a = g.haplotype_of();
        a.compat = genetics::compatible_set(rng);
        for d in a.dominance.iter_mut() {
            *d = (128.0 + rng.normal() * 20.0).clamp(1.0, 255.0) as u8;
        }
        let mut b = a.clone();
        b.mutate(rng);
        for d in b.dominance.iter_mut() {
            *d = (128.0 + rng.normal() * 20.0).clamp(1.0, 255.0) as u8;
        }
        let marker = g.marker;
        Self::express([a, b], marker)
    }

    /// One member of a founding stand, with the variety a wild population carries.
    pub fn wild(&self, rng: &mut Rng) -> Self {
        let [a, b] = self.haplotypes();
        let mut gametes = [Self::recombine(&a, &b, rng), Self::recombine(&a, &b, rng)];
        for h in gametes.iter_mut() {
            h.mutate(rng);
            h.mutate_haplotype(rng);
        }
        Self::express(gametes, genetics::mother_line(&self.marker, rng))
    }

    /// A seed: one gamete from this plant and one from the pollen parent,
    /// which may be itself.
    pub fn cross(&self, pollen: &Self, rng: &mut Rng) -> Self {
        let mut ovule = self.gamete(rng);
        let mut grain = pollen.gamete(rng);
        ovule.mutate_haplotype(rng);
        grain.mutate_haplotype(rng);
        Self::express([ovule, grain], genetics::mother_line(&self.marker, rng))
    }

    /// Whether a seed with these genes sprouts at all.
    pub fn viability(&self) -> f32 {
        match &self.haplotypes {
            Some(h) => genetics::viability_of(h[0].load, h[1].load, self.clashes()),
            None => 1.0,
        }
    }

    /// Whether it can set seed once grown.
    pub fn fertility(&self) -> f32 {
        genetics::fertility_of(self.clashes())
    }

    pub fn clashes(&self) -> u32 {
        self.haplotypes.as_ref().map_or(0, |h| genetics::clash_count(&h[0].compat, &h[1].compat))
    }

    /// Fraction of its genes at which its two copies differ, 0..1.
    pub fn heterozygosity(&self) -> f32 {
        let Some(h) = &self.haplotypes else { return 0.0 };
        let (a, b) = (h[0].loci(), h[1].loci());
        let mut differ = a.iter().zip(&b).filter(|(x, y)| (*x - *y).abs() > 1e-6).count();
        differ += h[0].compat.iter().zip(&h[1].compat).filter(|(x, y)| x != y).count();
        differ as f32 / (PLANT_LOCI + COMPAT) as f32
    }

    /// How far apart two plants' interacting genes are, 0 same .. 1 none shared.
    pub fn compat_divergence(&self, o: &Self) -> f32 {
        let (Some(a), Some(b)) = (&self.haplotypes, &o.haplotypes) else { return 0.0 };
        let apart = (0..COMPAT)
            .filter(|&k| {
                let x = [a[0].compat[k], a[1].compat[k]];
                !x.contains(&b[0].compat[k]) && !x.contains(&b[1].compat[k])
            })
            .count();
        apart as f32 / COMPAT as f32
    }

    /// A copy without the haplotypes, for records.
    pub fn clone_shallow(&self) -> Self {
        let mut g = self.clone();
        g.haplotypes = None;
        g
    }

    fn haplotype_of(&self) -> Self {
        let mut h = self.clone_shallow();
        if h.dominance.len() != PLANT_LOCI {
            h.dominance = vec![128; PLANT_LOCI];
        }
        h
    }

    fn haplotypes(&self) -> [Self; 2] {
        match &self.haplotypes {
            Some(h) => [h[0].clone(), h[1].clone()],
            None => [self.haplotype_of(), self.haplotype_of()],
        }
    }

    fn gamete(&self, rng: &mut Rng) -> Self {
        let [a, b] = self.haplotypes();
        Self::recombine(&a, &b, rng)
    }

    fn mutate_haplotype(&mut self, rng: &mut Rng) {
        let rate = self.mutation;
        genetics::mutate_shared(&mut self.dominance, PLANT_LOCI, &mut self.load, &mut self.compat, rng, rate);
    }

    fn place(i: usize) -> (usize, f32) {
        let c = PLANT_CHROMS.iter().rposition(|&s| s <= i).unwrap_or(0).min(PLANT_CHROMS.len() - 2);
        let n = (PLANT_CHROMS[c + 1] - PLANT_CHROMS[c]) as f32;
        (c, ((i - PLANT_CHROMS[c]) as f32 + 0.5) / n)
    }

    /// One gamete from two haplotypes, crossed over along four chromosomes.
    fn recombine(a: &Self, b: &Self, rng: &mut Rng) -> Self {
        let m = genetics::Meiosis::new(rng);
        let src = [a, b];
        let mut out = src[m.strand(2, 0.5)].clone_shallow();
        let (la, lb) = (a.loci(), b.loci());
        for (i, slot) in out.loci_mut().into_iter().enumerate() {
            let (c, pos) = Self::place(i);
            *slot = if m.strand(c, pos) == 0 { la[i] } else { lb[i] };
        }
        out.dominance = (0..PLANT_LOCI)
            .map(|i| {
                let (c, pos) = Self::place(i);
                src[m.strand(c, pos)].dominance.get(i).copied().unwrap_or(128)
            })
            .collect();
        // The counted parts of the body plan ride on the form chromosome.
        let s = m.strand(0, 0.95);
        (out.stem_segments, out.branches, out.leaves) = (src[s].stem_segments, src[s].branches, src[s].leaves);
        for k in 0..COMPAT {
            out.compat[k] = src[m.strand(k % 4, ((k / 4) as f32 + 0.5) / 4.0)].compat[k];
        }
        out.load = 0;
        for bit in 0..64 {
            out.load |= src[m.strand(bit % 4, ((bit / 4) as f32 + 0.5) / 16.0)].load & (1 << bit);
        }
        out
    }

    /// What two haplotypes make together; anything not listed follows the seed parent.
    fn express(h: [Self; 2], marker: [u8; 8]) -> Self {
        let mut g = h[0].clone_shallow();
        let (a, b) = (h[0].loci(), h[1].loci());
        for (i, slot) in g.loci_mut().into_iter().enumerate() {
            let t = {
                let (wa, wb) = (genetics::weight(h[0].dominance.get(i)), genetics::weight(h[1].dominance.get(i)));
                wb / (wa + wb)
            };
            *slot = if PLANT_HUES.contains(&i) {
                let d = (b[i] - a[i] + 0.5).rem_euclid(1.0) - 0.5;
                (a[i] + d * t).rem_euclid(1.0)
            } else {
                a[i] + (b[i] - a[i]) * t
            };
        }
        // Counted parts blend to the nearer whole number, the seed parent's on a tie.
        let count = |x: u8, y: u8| ((x as f32 + y as f32) * 0.5 + if x > y { 0.01 } else { -0.01 }).round() as u8;
        g.stem_segments = count(h[0].stem_segments, h[1].stem_segments);
        g.branches = count(h[0].branches, h[1].branches);
        g.leaves = count(h[0].leaves, h[1].leaves);
        g.marker = marker;
        g.dominance = Vec::new();
        g.load = h[0].load & h[1].load;
        g.vigor = genetics::vigor_of(h[0].load, h[1].load);
        // Inbred plants grow slower and die younger.
        g.growth *= g.vigor;
        g.lifespan *= g.vigor.sqrt();
        g.haplotypes = Some(Arc::new(h));
        g
    }

    fn loci(&self) -> [f32; PLANT_LOCI] {
        let l = &self.look;
        [
            self.adult_scale,
            self.stem_length,
            self.stem_width,
            self.curl,
            self.branch_angle,
            self.branch_scale,
            self.leaf,
            self.root,
            self.floating,
            self.thirst,
            self.vent,
            self.heat_tolerance,
            self.cold_tolerance,
            self.growth,
            self.lifespan,
            self.mutation,
            self.seed_size,
            self.dormancy,
            self.hardening,
            self.mast,
            self.selfing,
            self.spines,
            self.toxin,
            self.trap,
            self.fruit,
            self.wind,
            self.glow,
            self.nectar,
            self.induce,
            self.warn,
            self.listen,
            l[0],
            l[1],
            l[2],
            l[3],
            l[4],
            l[5],
            l[6],
            l[7],
            l[8],
            l[9],
            l[10],
            l[11],
            l[12],
            l[13],
            l[14],
            l[15],
        ]
    }

    /// The named loci, along four chromosomes: form and habit; warmth,
    /// growth and breeding; defence and spread; looks.
    fn loci_mut(&mut self) -> [&mut f32; PLANT_LOCI] {
        let [l0, l1, l2, l3, l4, l5, l6, l7, l8, l9, l10, l11, l12, l13, l14, l15] = &mut self.look;
        [
            &mut self.adult_scale,
            &mut self.stem_length,
            &mut self.stem_width,
            &mut self.curl,
            &mut self.branch_angle,
            &mut self.branch_scale,
            &mut self.leaf,
            &mut self.root,
            &mut self.floating,
            &mut self.thirst,
            &mut self.vent,
            &mut self.heat_tolerance,
            &mut self.cold_tolerance,
            &mut self.growth,
            &mut self.lifespan,
            &mut self.mutation,
            &mut self.seed_size,
            &mut self.dormancy,
            &mut self.hardening,
            &mut self.mast,
            &mut self.selfing,
            &mut self.spines,
            &mut self.toxin,
            &mut self.trap,
            &mut self.fruit,
            &mut self.wind,
            &mut self.glow,
            &mut self.nectar,
            &mut self.induce,
            &mut self.warn,
            &mut self.listen,
            l0,
            l1,
            l2,
            l3,
            l4,
            l5,
            l6,
            l7,
            l8,
            l9,
            l10,
            l11,
            l12,
            l13,
            l14,
            l15,
        ]
    }

    pub fn mutate(&mut self, rng: &mut Rng) {
        let r = self.mutation.clamp(0.3, 2.5);
        let nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32, rng: &mut Rng| {
            if rng.chance(0.15 * r) {
                *v = (*v + genetics::effect(rng) * sd).clamp(lo, hi);
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
        nudge(&mut self.induce, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.warn, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.listen, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.dormancy, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.hardening, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.mast, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.selfing, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.nectar, 0.03, 0.0, 1.0, rng);
        for (i, v) in self.look.iter_mut().enumerate() {
            if i == 0 || i == 3 {
                if rng.chance(0.15 * r) {
                    *v = (*v + genetics::effect(rng) * 0.04).rem_euclid(1.0);
                }
            } else {
                nudge(v, 0.04, 0.0, 1.0, rng);
            }
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
                    taper: tissue.taper(),
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
            (self.induce * 0.5, o.induce * 0.5),
            (self.mast * 0.5, o.mast * 0.5),
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
    /// Defences raised against grazing, 0..1, fading over minutes.
    pub alarm: f32,
    /// Memory of having been attacked, 0..1, fading over an hour: a primed
    /// plant arms itself faster.
    pub primed: f32,
    /// How deeply it has shut down, 0 awake .. 1 dormant.
    pub dormant: f32,
    /// How bad things have been lately, smoothed, for deciding to shut down.
    pub hardship: f32,
    /// Tolerance it has hardened itself to: + more heat, - more cold.
    pub acclim: f32,
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

    /// Spines with any raised against grazing.
    pub fn spines(&self) -> f32 {
        (self.genome.spines + self.alarm * self.genome.induce * 0.6).min(1.0)
    }

    /// Poison with any raised against grazing.
    pub fn toxin(&self) -> f32 {
        (self.genome.toxin + self.alarm * self.genome.induce * 0.8).min(1.0)
    }

    pub fn leaf_area(&self) -> f32 {
        let s = self.scale();
        let film: f32 = self.body.segments.iter().filter(|s| s.tissue == Tissue::LightFilm).map(|s| s.area()).sum();
        film * s * s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stands_breed_true_and_strangers_dont() {
        let mut rng = Rng::new(8);
        let a = PlantGenome::founder(Seedling::Fan, &mut rng);
        let b = PlantGenome::founder(Seedling::Fan, &mut rng);
        let (a1, a2) = (a.wild(&mut rng), a.wild(&mut rng));
        let n = 300;
        let within: f32 = (0..n).map(|_| a1.cross(&a2, &mut rng).viability()).sum::<f32>() / n as f32;
        let across: f32 = (0..n).map(|_| a1.cross(&b.wild(&mut rng), &mut rng).viability()).sum::<f32>() / n as f32;
        assert!(within > 0.7, "{within}");
        assert!(across < 0.2, "{across}");
    }

    #[test]
    fn plants_stay_growable_over_many_generations() {
        let mut rng = Rng::new(9);
        for kind in [Seedling::Fan, Seedling::VentCrust, Seedling::Thicket, Seedling::Mat] {
            let f = PlantGenome::founder(kind, &mut rng);
            let mut pop: Vec<PlantGenome> = (0..8).map(|_| f.wild(&mut rng)).collect();
            for _ in 0..400 {
                let (i, j) = (rng.below(8), rng.below(8));
                let child = pop[i].cross(&pop[j], &mut rng);
                let body = child.develop();
                assert!(!body.segments.is_empty());
                assert!(child.stem_segments >= 1 && child.leaves >= 1);
                pop[rng.below(8)] = child;
            }
        }
    }
}
