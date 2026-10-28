//! What a species looks like: one member drawn as it was seen in the world,
//! kept as the pieces the wallpaper's renderer draws, so the chronicle window
//! can paint a portrait even after the species is gone.

use crate::facts::SpeciesId;
use crate::history::History;
use serde::{Deserialize, Serialize};
use vesper_sim::Tissue;
use vesper_sim::frame::{CreatureView, Frame, PlantView};
use vesper_sim::genome::LOOK_GENES;
use vesper_sim::pose::{self, Pose};

/// One tapered tube of tissue, in world units, as `vesper_sim::pose::Seg`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub ra: f32,
    pub rb: f32,
    pub tissue: Tissue,
    pub damage: f32,
    pub side: i8,
}

/// A thin sheet between three points, as `vesper_sim::pose::Membrane`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub c: [f32; 2],
    pub tissue: Tissue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Likeness {
    /// Sim seconds and wall clock when it was seen.
    pub tick: f64,
    pub wall: f64,
    pub plant: bool,
    pub parts: Vec<Part>,
    pub sheets: Vec<Sheet>,
    pub look: [f32; LOOK_GENES],
    /// Draw depth, 0 nearest .. 1 farthest, and a per-individual pattern seed.
    pub z: f32,
    pub seed: u32,
}

impl Likeness {
    fn from_pose(pose: &Pose, tick: f64, plant: bool, look: [f32; LOOK_GENES], seed: u64) -> Self {
        Self {
            tick,
            wall: 0.0,
            plant,
            parts: pose
                .segs
                .iter()
                .map(|s| Part { a: s.a, b: s.b, ra: s.ra, rb: s.rb, tissue: s.tissue, damage: s.damage, side: s.side })
                .collect(),
            sheets: pose.membranes.iter().map(|m| Sheet { a: m.a, b: m.b, c: m.c, tissue: m.tissue }).collect(),
            look,
            z: pose.z,
            seed: seed as u32,
        }
    }

    pub fn creature(v: &CreatureView, tick: f64) -> Self {
        let mut pose = Pose::default();
        pose::creature(v, 1.0, &mut pose);
        Self::from_pose(&pose, tick, false, v.look, v.id)
    }

    pub fn plant(p: &PlantView, tick: f64) -> Self {
        let mut pose = Pose::default();
        pose::plant(p, &mut pose);
        Self::from_pose(&pose, tick, true, p.look, p.id)
    }

    /// Left, bottom, right, top, in world units.
    pub fn bounds(&self) -> [f32; 4] {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        let mut add = |p: [f32; 2], r: f32| {
            b = [b[0].min(p[0] - r), b[1].min(p[1] - r), b[2].max(p[0] + r), b[3].max(p[1] + r)];
        };
        for s in &self.parts {
            add(s.a, s.ra);
            add(s.b, s.rb);
        }
        for m in &self.sheets {
            add(m.a, 0.0);
            add(m.b, 0.0);
            add(m.c, 0.0);
        }
        b
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty() && self.sheets.is_empty()
    }
}

/// For each species in `frame` that `wanted` says needs a portrait, the
/// member that shows it best: a grown, whole and healthy one.
pub fn sitters(frame: &Frame, mut wanted: impl FnMut(SpeciesId) -> bool) -> Vec<(SpeciesId, Likeness)> {
    let mut best: std::collections::BTreeMap<u64, (f32, &CreatureView)> = Default::default();
    for c in &frame.creatures {
        if !wanted(SpeciesId(c.species)) && !best.contains_key(&c.species) {
            continue;
        }
        let score = c.maturity.min(1.0) * 2.0 + c.health - c.bleeding - c.lost.count_ones() as f32 * 0.5
            + if c.infection.is_some() { -0.5 } else { 0.0 };
        if best.get(&c.species).is_none_or(|(s, _)| score > *s) {
            best.insert(c.species, (score, c));
        }
    }
    let mut out: Vec<(SpeciesId, Likeness)> =
        best.into_values().map(|(_, c)| (SpeciesId(c.species), Likeness::creature(c, frame.time))).collect();
    let mut plants: std::collections::BTreeMap<u64, (f32, &PlantView)> = Default::default();
    for p in &frame.plants {
        if !plants.contains_key(&p.species) && !wanted(SpeciesId(p.species)) {
            continue;
        }
        let score = p.growth.min(1.0) * 2.0 + p.health + p.vigor * 0.5;
        if plants.get(&p.species).is_none_or(|(s, _)| score > *s) {
            plants.insert(p.species, (score, p));
        }
    }
    out.extend(plants.into_values().map(|(_, p)| (SpeciesId(p.species), Likeness::plant(p, frame.time))));
    out.retain(|(_, l)| !l.is_empty());
    out
}

/// How often the sketchbook looks at the world, how old a species must be
/// before it is drawn (most branches vanish within a minute), and how often
/// each is drawn again as it changes, all in sim seconds.
const LOOK_EVERY: f64 = 60.0;
const SETTLED: f64 = 300.0;
const REDRAW: f64 = 1800.0;

/// Decides when to draw each species, from what the chronicle already holds,
/// so species that were drawn before a restart aren't drawn again at once and
/// settled ones without a drawing are drawn at the first look.
#[derive(Default)]
pub struct Sketchbook {
    last: Option<f64>,
}

impl Sketchbook {
    /// Portraits due from this frame, usually none.
    pub fn sketch(&mut self, frame: &Frame, history: &History) -> Vec<(SpeciesId, Likeness)> {
        let now = frame.time;
        if self.last.is_some_and(|t| now - t < LOOK_EVERY && now >= t) {
            return Vec::new();
        }
        self.last = Some(now);
        sitters(frame, |s| {
            history.species.get(&s).is_some_and(|sp| now - sp.arose >= SETTLED)
                && history.likenesses.get(&s).is_none_or(|l| now - l.tick >= REDRAW || l.tick > now)
        })
    }
}
