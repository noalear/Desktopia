//! The world: its land, weather and fields, everything living and dead in
//! it, and the order things happen in each tick.

use crate::body::Body;
use crate::creature::Creature;
use crate::genome::{Genome, LOOK_GENES};
use crate::grid::Grid;
use crate::history::{Behavior, CreatureId, Fact, NatureKind, SpeciesId, StrainId};
use crate::plant::Plant;
use crate::rng::Rng;
use crate::society::Society;
use crate::species::Registry;
use crate::strain::Strain;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use vesper_terrain::{self as terrain, RiverState, WORLD_W};

/// Sim seconds per tick.
pub const TICK: f32 = 0.2;
/// Depth of the ground plane, world units, for distances.
pub const DEPTH: f32 = 0.35;
/// Highest a floating creature goes above the ground plane.
pub const ALT_MAX: f32 = 0.68;
/// Matter per world unit squared of body at frame density.
pub const MATTER_DENSITY: f32 = 1000.0;
/// Soil columns across the world.
pub const COLS: usize = 256;
/// Soil rows from the front of the ground plane to the back.
pub const ROWS: usize = 4;
/// Energy a unit of matter can hold as reserves.
pub const E_PER_MATTER: f32 = 60.0;
/// Energy from a unit of matter eaten, by kind of food.
pub const E_PLANT: f32 = 70.0;
pub const E_MEAT: f32 = 140.0;
pub const E_DETRITUS: f32 = 35.0;
/// Upkeep per unit of matter^0.75 per second.
pub const BASAL: f32 = 0.06;
pub const BRAIN_COST: f32 = 0.0006;
pub const MAX_CREATURES: usize = 150;
pub const MAX_PLANTS: usize = 2600;
/// Ticks between plant updates; plants grow on a one-second step.
pub const PLANT_EVERY: u64 = 5;

pub fn col_of(x: f32) -> usize {
    ((x / WORLD_W) * COLS as f32).clamp(0.0, COLS as f32 - 1.0) as usize
}

pub fn row_of(depth: f32) -> usize {
    ((depth / DEPTH) * ROWS as f32).clamp(0.0, ROWS as f32 - 1.0) as usize
}

pub fn cell_of(x: f32, depth: f32) -> usize {
    col_of(x) * ROWS + row_of(depth)
}

pub fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

pub fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Env {
    pub band_shift: f32,
    pub storm_x: f32,
    pub storm_v: f32,
    /// 0..1.
    pub storm: f32,
    pub storm_goal: f32,
    pub lightning: f32,
    pub lightning_x: f32,
    /// 0..1 how hard the volcano is erupting.
    pub eruption: f32,
    pub next_eruption: f64,
    /// How far lava has run from the volcano, world units.
    pub lava: f32,
    pub ash: f32,
    pub river: RiverState,
    pub river_v: f32,
    pub river_reported: f32,
    pub cold: f32,
    pub cold_x: f32,
    pub dust: f32,
    pub dust_x: f32,
}

impl Env {
    pub fn rain(&self, x: f32) -> f32 {
        self.storm * (-((x - self.storm_x) / 0.35).powi(2)).exp()
    }

    pub fn dust_at(&self, x: f32) -> f32 {
        self.dust * (-((x - self.dust_x) / 0.4).powi(2)).exp()
    }

    pub fn cold_at(&self, x: f32) -> f32 {
        self.cold * (-((x - self.cold_x) / 0.45).powi(2)).exp()
    }

    /// Sunlight at a point, 0..1.
    pub fn light(&self, x: f32, alt: f32) -> f32 {
        let sun = terrain::daylight(x, self.band_shift);
        let haze = (1.0 - 0.5 * self.rain(x)) * (1.0 - 0.6 * self.ash) * (1.0 - 0.6 * self.dust_at(x));
        (sun * haze * (0.85 + 0.15 * (alt / ALT_MAX).min(1.0))).max(0.0)
    }

    /// Heat at a point: positive is hot, negative cold; about +1.3 at the day
    /// edge and -1.3 at the night edge.
    pub fn temperature(&self, x: f32, alt: f32) -> f32 {
        let base = 1.3 - 2.6 * terrain::nightness(x, self.band_shift);
        let vents = terrain::vent_heat(x) * 0.9 * (1.0 - (alt / 0.15).min(1.0));
        let lava = if self.lava > 0.0 && (x - terrain::VOLCANO_X).abs() < self.lava {
            2.0 * (1.0 - alt * 4.0).max(0.0)
        } else {
            0.0
        };
        base + vents + lava - self.cold_at(x) - 0.25 * self.rain(x) - alt * 0.5
    }

    /// Wind along x at an altitude: day-ward near the ground, night-ward up high.
    pub fn wind(&self, alt: f32) -> f32 {
        let t = (alt / 0.35).clamp(0.0, 1.0);
        (-0.004 + 0.009 * t) * (1.0 + self.storm)
    }

    pub fn lava_at(&self, x: f32) -> bool {
        self.eruption > 0.05 && (x - terrain::VOLCANO_X).abs() < self.lava
    }
}

/// A body or a torn-off piece of one, breaking down.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Remains {
    pub id: u64,
    /// The creature these remains were.
    pub of: CreatureId,
    pub killer: Option<CreatureId>,
    pub species: SpeciesId,
    pub body: Arc<Body>,
    /// Segments of the body that are not part of these remains.
    pub missing: u64,
    pub scale: f32,
    pub look: [f32; LOOK_GENES],
    pub pos: [f32; 3],
    pub heading: f32,
    pub matter: f32,
    pub initial: f32,
    pub age: f32,
    pub toxin: f32,
    /// A strain it carries, caught by whatever eats it.
    pub strain: Option<StrainId>,
    /// A parent's body, given as food to its own young (the parent's id).
    pub offered_to: Option<CreatureId>,
    /// The body as a physical thing while it falls and settles.
    #[serde(default)]
    pub rig: Option<crate::physics::Rig>,
    #[serde(skip)]
    pub moments: Vec<crate::frame::Moment>,
}

impl Remains {
    pub fn decay01(&self) -> f32 {
        (1.0 - self.matter / self.initial.max(1e-6)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Egg {
    pub id: u64,
    pub species: SpeciesId,
    pub genome: Arc<Genome>,
    pub mother: Option<CreatureId>,
    pub voice: [u8; 4],
    pub home: [f32; 4],
    pub route_from: Option<CreatureId>,
    pub generation: u32,
    pub pos: [f32; 3],
    pub matter: f32,
    pub hatch_in: f32,
    pub sterile: bool,
    /// Tiny wind-borne spores rather than laid eggs.
    pub spore: bool,
}

/// A cloud of a strain's spores, left when a parasite bursts out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spores {
    pub strain: StrainId,
    pub pos: [f32; 3],
    pub strength: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Counters {
    pub births: u64,
    pub deaths: u64,
    pub kills: u64,
    pub starved: u64,
    pub aged: u64,
    pub disease: u64,
    pub exposure: u64,
    pub plant_births: u64,
    pub plant_deaths: u64,
}

#[derive(Serialize, Deserialize)]
pub struct World {
    /// Sim seconds since the world was born.
    pub time: f64,
    pub tick: u64,
    /// Unix seconds when the world was born; seasons follow epoch + time.
    pub epoch: f64,
    pub rng: Rng,
    pub env: Env,
    /// Nutrient in the soil, COLS x ROWS.
    pub soil: Vec<f32>,
    /// Detritus drifting in the air, per column.
    pub air: Vec<f32>,
    /// Matter deep in the planet, raised by vents and eruptions.
    pub deep: f32,
    pub creatures: Vec<Creature>,
    pub plants: Vec<Plant>,
    pub remains: Vec<Remains>,
    pub eggs: Vec<Egg>,
    pub spores: Vec<Spores>,
    pub strains: Vec<Strain>,
    pub species: Registry,
    pub society: Society,
    pub next_id: u64,
    pub firsts: Vec<Behavior>,
    /// Behaviors on their way to becoming habits, by species.
    pub habits: std::collections::BTreeMap<(u64, Behavior), Habit>,
    /// All matter in the world; fixed at birth.
    pub matter_total: f64,
    pub counters: Counters,
    #[serde(skip)]
    pub history: Vec<(f64, Fact)>,
    #[serde(skip)]
    pub index: HashMap<CreatureId, usize>,
    #[serde(skip)]
    pub cgrid: Grid,
    #[serde(skip)]
    pub pgrid: Grid,
    #[serde(skip)]
    pub rgrid: Grid,
    #[serde(skip)]
    pub egrid: Grid,
    #[serde(skip)]
    pub canopy: Vec<f32>,
    #[serde(skip)]
    pub ground: crate::ground::Ground,
    /// Fast-forwarding: bodies move by the gaits they have learned rather
    /// than being simulated limb by limb.
    #[serde(skip)]
    pub coarse: bool,
}

impl World {
    pub fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn record(&mut self, fact: Fact) {
        self.history.push((self.time, fact));
    }

    /// Notes a behavior. It is reported as a first once a species has made a
    /// habit of it (several individuals, repeatedly), not when one random
    /// act happens to look like it.
    pub fn first(&mut self, behavior: Behavior, species: SpeciesId, who: Option<CreatureId>) {
        if self.firsts.contains(&behavior) {
            return;
        }
        let (times, individuals) = habit_threshold(behavior);
        let now = self.time;
        let h = self.habits.entry((species.0, behavior)).or_insert(Habit { count: 0, who: Vec::new(), since: now });
        if now - h.since > 1200.0 {
            *h = Habit { count: 0, who: Vec::new(), since: now };
        }
        h.count += 1;
        if let Some(id) = who
            && !h.who.contains(&id.0)
            && h.who.len() < 8
        {
            h.who.push(id.0);
        }
        if h.count >= times && (who.is_none() || h.who.len() >= individuals) {
            self.firsts.push(behavior);
            self.habits.retain(|(_, b), _| *b != behavior);
            self.record(Fact::First { behavior, species, who });
        }
    }

    pub fn nature(&mut self, kind: NatureKind, x: f32, strength: f32) {
        self.record(Fact::Nature { kind, x, strength });
    }

    pub fn soil_at(&self, x: f32, depth: f32) -> f32 {
        self.soil[cell_of(x, depth)]
    }

    /// Returns matter to the ground at a spot.
    pub fn to_soil(&mut self, x: f32, depth: f32, m: f32) {
        if m > 0.0 {
            self.soil[cell_of(x, depth)] += m;
        }
    }

    /// Returns matter to the air or, near the ground, to the soil.
    pub fn shed(&mut self, pos: [f32; 3], m: f32) {
        if m <= 0.0 {
            return;
        }
        if pos[2] > 0.05 {
            self.air[col_of(pos[0])] += m;
        } else {
            self.to_soil(pos[0], pos[1], m);
        }
    }

    /// Takes up to `want` matter from the soil around a spot.
    pub fn take_soil(&mut self, x: f32, depth: f32, want: f32) -> f32 {
        let c = cell_of(x, depth);
        let got = want.min(self.soil[c] * 0.5).max(0.0);
        self.soil[c] -= got;
        got
    }

    pub fn rebuild_index(&mut self) {
        self.index.clear();
        for (i, c) in self.creatures.iter().enumerate() {
            self.index.insert(c.id, i);
        }
    }

    pub fn find(&self, id: CreatureId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    pub fn strain_slot(&self, id: StrainId) -> Option<usize> {
        self.strains.iter().position(|s| s.id == id)
    }

    /// Every unit of matter, wherever it is.
    pub fn matter_now(&self) -> f64 {
        let mut m = self.deep as f64;
        m += self.soil.iter().map(|&v| v as f64).sum::<f64>();
        m += self.air.iter().map(|&v| v as f64).sum::<f64>();
        m += self.creatures.iter().map(|c| c.matter as f64).sum::<f64>();
        m += self.plants.iter().map(|p| (p.matter + p.fruit + p.seeds) as f64).sum::<f64>();
        m += self.remains.iter().map(|r| r.matter as f64).sum::<f64>();
        m += self.eggs.iter().map(|e| e.matter as f64).sum::<f64>();
        m
    }

    /// Puts rounding drift back into the soil so matter stays exactly fixed.
    pub fn rebalance(&mut self) {
        let drift = self.matter_total - self.matter_now();
        if drift.abs() < 1e-9 {
            return;
        }
        let per = (drift / self.soil.len() as f64) as f32;
        for s in &mut self.soil {
            *s = (*s + per).max(0.0);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Habit {
    pub count: u32,
    pub who: Vec<u64>,
    pub since: f64,
}

/// How often, and by how many individuals, a behavior must happen within
/// twenty minutes before it counts as part of a species' life.
fn habit_threshold(b: Behavior) -> (u32, usize) {
    use Behavior::*;
    match b {
        FoodSharing | Grooming | Signaling | Scavenging | Guarding => (40, 6),
        Hunting | Cannibalism | Infanticide | SexualReproduction | Deception => (20, 4),
        Parasitism | Symbiosis => (5, 3),
        Sacrifice | Matriphagy | War | Trapping => (4, 2),
        Dialect | Tradition | Migration | Eusociality | Overthrow => (2, 1),
    }
}

/// Splits two distinct elements of a slice into mutable references.
pub fn pair_mut<T>(v: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert!(a != b);
    if a < b {
        let (l, r) = v.split_at_mut(b);
        (&mut l[a], &mut r[0])
    } else {
        let (l, r) = v.split_at_mut(a);
        (&mut r[0], &mut l[b])
    }
}

pub fn wrap_angle(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let mut a = a % t;
    if a > std::f32::consts::PI {
        a -= t;
    } else if a < -std::f32::consts::PI {
        a += t;
    }
    a
}

/// Weather, the volcano, the river and the season, stepped once per second.
pub fn weather(w: &mut World, dt: f32) {
    let rng = &mut w.rng;
    let e = &mut w.env;
    e.band_shift = terrain::band_shift(w.epoch + w.time);

    // Storms wander the twilight and build and fade over tens of minutes.
    if rng.chance(dt / 900.0) {
        e.storm_goal = if rng.chance(0.4) { rng.range(0.5, 1.0) } else { rng.range(0.0, 0.3) };
    }
    let was = e.storm;
    e.storm += (e.storm_goal - e.storm) * (dt / 300.0).min(1.0);
    e.storm_v += rng.normal() * 0.00005 * dt;
    e.storm_v = e.storm_v.clamp(-0.003, 0.003);
    e.storm_x += e.storm_v * dt;
    if e.storm_x < 0.9 || e.storm_x > 3.1 {
        e.storm_v = -e.storm_v;
        e.storm_x = e.storm_x.clamp(0.9, 3.1);
    }
    let storm_report = was < 0.7 && e.storm >= 0.7;
    e.lightning *= (-dt * 3.0).exp();
    let strike = e.storm > 0.55 && rng.chance(dt * 0.03 * e.storm);
    if strike {
        e.lightning = 1.0;
        e.lightning_x = e.storm_x + rng.normal() * 0.2;
    }

    // Floods follow heavy rain on the night side, where the river is fed.
    let upstream = e.rain(2.8);
    e.river.flood += (upstream * 0.8 - e.river.flood) * (dt / 1200.0).min(1.0);
    let flood_report = e.river.flood > 0.5 && rng.chance(dt / 3600.0);
    // The river slowly wanders its plain over days.
    e.river_v += rng.normal() * 2e-8 * dt;
    e.river_v = e.river_v.clamp(-2e-5, 2e-5);
    e.river.phase += e.river_v * dt;
    let shifted = (e.river.phase - e.river_reported).abs() > 0.4;
    if shifted {
        e.river_reported = e.river.phase;
    }

    // Cold falls off the glacier and creeps day-ward.
    if e.cold <= 0.01 && rng.chance(dt / 5400.0) {
        e.cold = rng.range(0.4, 1.0);
        e.cold_x = 3.3;
        w.history.push((w.time, Fact::Nature { kind: NatureKind::ColdFall, x: 3.3, strength: e.cold }));
    }
    if e.cold > 0.0 {
        e.cold_x -= 0.0012 * dt;
        if e.cold_x < 1.2 {
            e.cold *= (-dt / 120.0).exp();
        }
        if e.cold < 0.01 {
            e.cold = 0.0;
        }
    }
    // Dust blows off the scorched day side.
    if e.dust <= 0.01 && rng.chance(dt / 7200.0) {
        e.dust = rng.range(0.4, 1.0);
        e.dust_x = 0.2;
        w.history.push((w.time, Fact::Nature { kind: NatureKind::DustStorm, x: 0.2, strength: e.dust }));
    }
    if e.dust > 0.0 {
        e.dust_x += 0.0015 * dt;
        if e.dust_x > 1.8 {
            e.dust *= (-dt / 120.0).exp();
        }
        if e.dust < 0.01 {
            e.dust = 0.0;
        }
    }

    // The volcano.
    let mut erupted = None;
    if w.time >= e.next_eruption {
        let strength = rng.range(0.4, 1.0);
        e.eruption = strength;
        e.next_eruption = w.time + rng.range(3.0, 12.0) as f64 * 3600.0;
        erupted = Some(strength);
    }
    if e.eruption > 0.0 {
        let reach = 0.06 + 0.3 * e.eruption;
        e.lava = (e.lava + 0.002 * dt).min(reach);
        e.ash = (e.ash + e.eruption * 0.004 * dt).min(0.8);
        e.eruption *= (-dt / 900.0).exp();
        if e.eruption < 0.03 {
            e.eruption = 0.0;
        }
    } else {
        e.lava = (e.lava - 0.0005 * dt).max(0.0);
    }
    e.ash *= (-dt / 1800.0).exp();
    let (storm_x, lightning_x) = (e.storm_x, e.lightning_x);

    if storm_report {
        w.nature(NatureKind::Storm, storm_x, 0.7);
    }
    if flood_report {
        let f = w.env.river.flood;
        w.nature(NatureKind::Flood, 2.0, f);
    }
    if shifted {
        w.nature(NatureKind::RiverShift, 2.0, w.env.river.phase);
    }
    if strike {
        w.nature(NatureKind::Lightning, lightning_x, 1.0);
    }
    if let Some(s) = erupted {
        w.nature(NatureKind::Eruption, terrain::VOLCANO_X, s);
    }
    // Eruptions and vents raise matter from deep in the planet; ash and lava
    // spread it over the ground near the volcano.
    let lift = (0.00002 + w.env.eruption * 0.0003) * w.deep * dt;
    if lift > 0.0 {
        w.deep -= lift;
        let n = 12;
        for k in 0..n {
            let x = terrain::VOLCANO_X + (k as f32 / (n - 1) as f32 - 0.5) * 0.6;
            let d = w.rng.range(0.0, DEPTH);
            w.to_soil(x, d, lift / n as f32);
        }
    }
    // The glacier buries a little matter back into the deep.
    let mut buried = 0.0;
    for c in col_of(3.25)..COLS {
        for r in 0..ROWS {
            let v = &mut w.soil[c * ROWS + r];
            let b = *v * 0.0002 * dt;
            *v -= b;
            buried += b;
        }
    }
    w.deep += buried;
}

/// Soil, air and canopy fields, stepped once per second.
pub fn fields(w: &mut World, dt: f32) {
    // Detritus in the air drifts with the wind and settles.
    let wind = w.env.wind(0.2);
    let shift = wind * dt / (WORLD_W / COLS as f32);
    let mut next = vec![0.0f32; COLS];
    for c in 0..COLS {
        let v = w.air[c];
        let settle = v * 0.004 * dt;
        let stay = v - settle;
        let to = (c as f32 + shift).clamp(0.0, COLS as f32 - 1.0);
        let lo = to.floor() as usize;
        let hi = (lo + 1).min(COLS - 1);
        let f = to - lo as f32;
        next[lo] += stay * (1.0 - f);
        next[hi] += stay * f;
        let r = w.rng.below(ROWS);
        w.soil[c * ROWS + r] += settle;
    }
    w.air = next;
    // Wind lifts fine soil into the air, most of all in dust storms.
    for c in 0..COLS {
        let x = (c as f32 + 0.5) / COLS as f32 * WORLD_W;
        let k = 0.00005 * (1.0 + w.env.dust_at(x) * 20.0 + w.env.storm * 2.0) * dt;
        for r in 0..ROWS {
            let lift = w.soil[c * ROWS + r] * k;
            w.soil[c * ROWS + r] -= lift;
            w.air[c] += lift;
        }
    }
    // Soil spreads slowly; the river carries some downstream (day-ward).
    let k = 0.002 * dt;
    let mut flow = vec![0.0f32; COLS * ROWS];
    for c in 0..COLS {
        for r in 0..ROWS {
            let i = c * ROWS + r;
            let v = w.soil[i];
            if c + 1 < COLS {
                let d = (v - w.soil[i + ROWS]) * k;
                flow[i] -= d;
                flow[i + ROWS] += d;
            }
            if r + 1 < ROWS {
                let d = (v - w.soil[i + 1]) * k;
                flow[i] -= d;
                flow[i + 1] += d;
            }
            let x = (c as f32 + 0.5) / COLS as f32 * WORLD_W;
            let depth = (r as f32 + 0.5) / ROWS as f32 * DEPTH;
            if c > 0 && w.ground.in_river(x, depth / DEPTH) {
                let d = v * 0.0008 * dt * (1.0 + w.env.river.flood * 3.0);
                flow[i] -= d;
                flow[i - ROWS] += d;
            }
        }
    }
    for (s, f) in w.soil.iter_mut().zip(&flow) {
        *s = (*s + f).max(0.0);
    }
    // The canopy: leaf area per column, for shading.
    w.canopy.clear();
    w.canopy.resize(COLS, 0.0);
    for p in &w.plants {
        if p.alt < 0.05 {
            w.canopy[col_of(p.x)] += p.leaf_area();
        }
    }
}
