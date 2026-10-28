//! The simulation's front door: make a world, step it, read it, save it.

use crate::act;
use crate::ecology;
use crate::genome::{Founder, Genome};
use crate::history::{CreatureId, Fact};
use crate::life;
use crate::mind::Mind;
use crate::plant::{PlantGenome, Seed, Seedling};
use crate::rng::Rng;
use crate::society;
use crate::species::Registry;
use crate::think::{self, Percept};
use crate::world::{self, COLS, DEPTH, Env, PLANT_EVERY, ROWS, TICK, World};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vesper_terrain::{self as terrain, WORLD_W};

/// Soil nutrient per cell at the world's birth.
const SOIL_START: f32 = 3.0;
const DEEP_START: f32 = 1200.0;

pub struct Sim {
    pub(crate) w: World,
    percepts: Vec<Percept>,
    profile: Option<[f64; PHASES]>,
}

impl Sim {
    /// A new world. `epoch` is the Unix time it is born at, which sets where
    /// in the season it starts.
    pub fn new(seed: u64, epoch: f64) -> Self {
        let mut w = World {
            time: 0.0,
            tick: 0,
            epoch,
            rng: Rng::new(seed),
            env: Env::default(),
            soil: vec![SOIL_START; COLS * ROWS],
            air: vec![0.0; COLS],
            deep: DEEP_START,
            creatures: Vec::new(),
            plants: Vec::new(),
            remains: Vec::new(),
            eggs: Vec::new(),
            spores: Vec::new(),
            strains: Vec::new(),
            species: Registry::default(),
            society: Default::default(),
            next_id: 0,
            firsts: Vec::new(),
            habits: Default::default(),
            matter_total: 0.0,
            counters: Default::default(),
            history: Vec::new(),
            index: Default::default(),
            cgrid: Default::default(),
            pgrid: Default::default(),
            rgrid: Default::default(),
            egrid: Default::default(),
            canopy: vec![0.0; COLS],
            ground: Default::default(),
            coarse: false,
        };
        w.env.storm_x = 2.2;
        w.env.next_eruption = w.rng.range(1.0, 4.0) as f64 * 3600.0;
        w.env.band_shift = terrain::band_shift(epoch);
        // Richer ground near the vents and along the river.
        for c in 0..COLS {
            let x = (c as f32 + 0.5) / COLS as f32 * WORLD_W;
            for r in 0..ROWS {
                let d = (r as f32 + 0.5) / ROWS as f32;
                let rich = 1.0 + terrain::vent_heat(x) + terrain::wetness(x, d, Default::default()) * 0.8;
                w.soil[c * ROWS + r] *= rich;
            }
        }
        w.ground.refresh(w.env.river);
        let mut sim = Self { w, percepts: Vec::new(), profile: None };
        sim.genesis();
        sim.w.matter_total = sim.w.matter_now();
        sim
    }

    fn genesis(&mut self) {
        let w = &mut self.w;
        ecology::rebuild_plant_grid(w);
        let kinds = [
            (Seedling::Fan, 360, 0.5, 2.7),
            (Seedling::Thicket, 220, 0.8, 2.9),
            (Seedling::VentCrust, 150, 0.0, 0.0),
            (Seedling::Mat, 90, 0.8, 2.6),
        ];
        for (kind, n, lo, hi) in kinds {
            let g = Arc::new(PlantGenome::founder(kind, &mut w.rng));
            let species = w.species.found_plant(&g, 0.0, &mut w.history);
            for _ in 0..n {
                let mut child = (*g).clone();
                child.mutate(&mut w.rng);
                let x = match kind {
                    Seedling::VentCrust => {
                        let v = terrain::VENTS[w.rng.below(terrain::VENTS.len())];
                        v.0 + w.rng.normal() * 0.04
                    }
                    Seedling::Thicket => loop {
                        let x = w.rng.range(lo, hi);
                        if terrain::river(x, Default::default())[1] > 0.0 {
                            break x;
                        }
                    },
                    _ => w.rng.range(lo, hi),
                };
                let depth = match kind {
                    Seedling::Thicket => {
                        let [c, wd] = terrain::river(x, Default::default());
                        let h = c + (wd * 1.6 + 0.004) * if w.rng.chance(0.5) { 1.0 } else { -1.0 };
                        terrain::depth_of(x, h) * DEPTH
                    }
                    _ => w.rng.range(0.0, DEPTH),
                };
                let seed = Seed { genome: Arc::new(child), species, generation: 0 };
                let body = seed.genome.develop();
                let m = seed.genome.adult_matter(&body) * w.rng.range(0.2, 0.7);
                let m = w.take_soil(x, depth, m);
                let alt = if seed.genome.floats() { w.rng.range(0.1, 0.4) } else { 0.0 };
                ecology::sow(w, &seed, x, depth, alt, m);
                if let Some(p) = w.plants.last_mut() {
                    p.age = w.rng.range(0.0, p.genome.lifespan * 0.6);
                }
            }
            // The founders are not "born" as a fact of history beyond SpeciesArose.
        }
        let founders = [
            (Founder::Drifter, 30, 0.15),
            (Founder::Grazer, 25, 0.0),
            (Founder::Swimmer, 25, 0.08),
            (Founder::Crawler, 20, 0.0),
        ];
        for (kind, n, alt) in founders {
            let g = Arc::new(Genome::founder(kind, &mut w.rng));
            let species = w.species.found_creature(&g, 0.0, &mut w.history);
            let voice = [w.rng.below(16) as u8, w.rng.below(16) as u8, w.rng.below(16) as u8, w.rng.below(16) as u8];
            for _ in 0..n {
                let child = Arc::new(g.bud(&mut w.rng));
                let x = w.rng.range(0.9, 2.5);
                let depth = w.rng.range(0.02, DEPTH - 0.02);
                let pos = [x, depth, if alt > 0.0 { alt + w.rng.range(0.0, 0.2) } else { 0.0 }];
                let body = crate::body::Body::develop(&child);
                let adult = crate::creature::adult_matter(&child, &body);
                let share = w.rng.range(0.4, 1.0);
                let m = w.take_soil(x, depth, adult * share);
                let id = CreatureId(w.id());
                let heading = w.rng.signed() * std::f32::consts::PI;
                let mut c =
                    life::new_creature(id, child, species, pos, m, None, Mind::new(voice), 0, false, 0.0, heading);
                c.age = w.rng.range(0.0, 200.0);
                w.species.born(species);
                w.creatures.push(c);
            }
        }
        // A couple of microbes to begin with.
        world::fields(w, 1.0);
        w.rebuild_index();
        for _ in 0..3 {
            life::emerge(w);
        }
    }

    pub(crate) fn from_world(w: World) -> Self {
        Self { w, percepts: Vec::new(), profile: None }
    }

    /// A snapshot for the renderer.
    pub fn frame(&self) -> crate::frame::Frame {
        crate::frame::Frame::of(&self.w)
    }

    /// Sim seconds since the world was born.
    pub fn time(&self) -> f64 {
        self.w.time
    }

    /// Unix time the world was born at.
    pub fn epoch(&self) -> f64 {
        self.w.epoch
    }

    pub fn world(&self) -> &World {
        &self.w
    }

    /// For tools that stage a scene, such as the preview's sheet of behaviours.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.w
    }

    /// Facts reported since the last call, oldest first.
    pub fn drain_history(&mut self) -> Vec<(f64, Fact)> {
        std::mem::take(&mut self.w.history)
    }

    /// Fast-forwarding moves bodies by their learned gaits instead of
    /// simulating every limb, which is far cheaper and not worth watching.
    pub fn set_coarse(&mut self, coarse: bool) {
        self.w.coarse = coarse;
    }

    /// Runs up to `seconds` of sim time, stopping early if `budget` of real
    /// time runs out. Returns the sim seconds actually run.
    pub fn advance(&mut self, seconds: f64, budget: Duration) -> f64 {
        let start = Instant::now();
        let ticks = (seconds / TICK as f64).round() as u64;
        let mut done = 0;
        while done < ticks {
            self.step();
            done += 1;
            if start.elapsed() >= budget {
                break;
            }
        }
        done as f64 * TICK as f64
    }

    /// One tick: TICK sim seconds.
    pub fn step(&mut self) {
        let w = &mut self.w;
        let mut probe = Probe::new(w, &mut self.profile);
        w.tick += 1;
        w.time += TICK as f64;
        index_creatures(w);
        w.ground.refresh(w.env.river);
        let dt_slow = TICK * PLANT_EVERY as f32;
        if w.tick % PLANT_EVERY == 0 {
            world::weather(w, dt_slow);
            probe.at(w, 0);
            world::fields(w, dt_slow);
            probe.at(w, 1);
            ecology::plants(w, dt_slow);
            probe.at(w, 2);
            life::disease(w, dt_slow);
            probe.at(w, 3);
            life::decay(w, dt_slow);
            life::eggs(w, dt_slow);
            index_creatures(w);
            probe.at(w, 4);
        }
        ecology::drift(w, TICK);
        think::think(w, &mut self.percepts);
        probe.at(w, 5);
        act::movement(w, &self.percepts);
        act::physics(w);
        probe.at(w, 6);
        act::actions(w, &self.percepts);
        probe.at(w, 7);
        life::metabolism(w, TICK);
        probe.at(w, 8);
        life::bury(w);
        probe.at(w, 9);
        if w.tick % 150 == 0 {
            w.rebuild_index();
            society::regroup(w);
        }
        if w.tick % 300 == 0 {
            let census = society::census(w);
            w.record(Fact::Census(census));
            sweep_strains(w);
            w.rebalance();
            if w.rng.chance(0.05) {
                life::emerge(w);
            }
            renew(w);
        }
        probe.at(w, 10);
    }

    /// Turns on timing of each phase of a tick, for tuning.
    pub fn enable_profile(&mut self) {
        self.profile = Some(Default::default());
    }

    /// Seconds spent in each phase since the last call: weather, fields,
    /// plants, disease, decay and eggs, thinking, moving, acting, metabolism,
    /// burial, society.
    pub fn take_profile(&mut self) -> Option<[f64; PHASES]> {
        self.profile.as_mut().map(std::mem::take)
    }
}

pub const PHASES: usize = 11;
pub const PHASE_NAMES: [&str; PHASES] =
    ["weather", "fields", "plants", "disease", "decay+eggs", "think", "move", "act", "metabolism", "bury", "society"];

fn index_creatures(w: &mut World) {
    w.rebuild_index();
    w.cgrid.clear();
    for (i, c) in w.creatures.iter().enumerate() {
        w.cgrid.insert(i, c.pos);
    }
}

/// Optional per-phase timing, plus a matter audit when VESPER_AUDIT is set.
struct Probe<'a> {
    profile: Option<&'a mut [f64; PHASES]>,
    last: Instant,
    audit: Option<f64>,
}

impl<'a> Probe<'a> {
    fn new(w: &World, profile: &'a mut Option<[f64; PHASES]>) -> Self {
        let audit = AUDIT.with(|a| *a).then(|| w.matter_now());
        Self { profile: profile.as_mut(), last: Instant::now(), audit }
    }

    fn at(&mut self, w: &World, phase: usize) {
        if let Some(p) = self.profile.as_deref_mut() {
            let now = Instant::now();
            p[phase] += (now - self.last).as_secs_f64();
            self.last = now;
        }
        if let Some(last) = self.audit {
            let m = w.matter_now();
            if (m - last).abs() > 1e-3 {
                eprintln!("tick {} {}: matter {:+.5}", w.tick, PHASE_NAMES[phase], m - last);
            }
            self.audit = Some(m);
        }
    }
}

thread_local! {
    static AUDIT: bool = std::env::var_os("VESPER_AUDIT").is_some();
}

/// Retires strains nobody carries any more.
fn sweep_strains(w: &mut World) {
    let mut gone = Vec::new();
    for s in &mut w.strains {
        if s.gone || s.hosts > 0 {
            continue;
        }
        let id = s.id;
        let held = w.remains.iter().any(|r| r.strain == Some(id)) || w.spores.iter().any(|p| p.strain == id);
        if !held {
            s.gone = true;
            gone.push(id);
        }
    }
    for id in gone {
        w.record(Fact::PathogenGone { id });
    }
}

/// When life is nearly gone, new life seeps up from the vents: a new founder
/// lineage, not a revival of anything that died.
fn renew(w: &mut World) {
    if w.creatures.len() + w.eggs.len() < 12 && w.rng.chance(0.2) {
        let kinds = [Founder::Drifter, Founder::Grazer, Founder::Swimmer, Founder::Crawler];
        let kind = *w.rng.pick(&kinds);
        let g = Arc::new(Genome::founder(kind, &mut w.rng));
        let species = w.species.found_creature(&g, w.time, &mut w.history);
        let voice = [0, 1, 2, 3];
        let vent = terrain::VENTS[w.rng.below(terrain::VENTS.len())].0;
        for _ in 0..20 {
            let child = Arc::new(g.bud(&mut w.rng));
            let pos = [vent + w.rng.normal() * 0.05, w.rng.range(0.02, DEPTH - 0.02), 0.0];
            let body = crate::body::Body::develop(&child);
            let adult = crate::creature::adult_matter(&child, &body);
            let m = w.take_soil(pos[0], pos[1], adult * 0.5);
            let id = CreatureId(w.id());
            let heading = w.rng.signed() * 3.0;
            let c = life::new_creature(id, child, species, pos, m, None, Mind::new(voice), 0, false, w.time, heading);
            w.species.born(species);
            w.creatures.push(c);
        }
    }
    if w.plants.len() < 30 && w.rng.chance(0.2) {
        let kinds = [Seedling::Fan, Seedling::VentCrust, Seedling::Thicket, Seedling::Mat];
        let g = Arc::new(PlantGenome::founder(*w.rng.pick(&kinds), &mut w.rng));
        let species = w.species.found_plant(&g, w.time, &mut w.history);
        for _ in 0..20 {
            let x = w.rng.range(0.6, 3.0);
            let d = w.rng.range(0.0, DEPTH);
            let seed = Seed { genome: g.clone(), species, generation: 0 };
            ecology::sow(w, &seed, x, d, 0.2, 0.0);
        }
    }
}
