//! The living world: runs the simulation on its own low-priority thread and
//! turns its snapshots into draw lists, sampling the motion recorded
//! through each tick so movement stays smooth at any frame rate.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vesper_chronicle::likeness::Sketchbook;
use vesper_chronicle::Chronicle;
use vesper_render::{Infection, LifeBatch, Look, Membrane, Organism, Seg, Tissue, WorldView};
use vesper_sim::frame::{CreatureView, Frame, PlantView};
use vesper_sim::history::Fact;
use vesper_sim::history::Locomotion;
use vesper_sim::pose::{self, Pose};
use vesper_sim::runner::{Options, Recorder, Runner};
use vesper_sim::{Sim, save};

/// How much time the PC was off to make up for on start, at most.
const CATCH_UP_MAX: f64 = 2.0 * 3600.0;
const SAVE_EVERY: Duration = Duration::from_secs(300);
/// How often the plants are posed and uploaded again; they change slowly.
const PLANTS_EVERY: u64 = 5;

pub struct Living {
    runner: Runner,
    plants_tick: Option<u64>,
    pose: Pose,
    segs: Vec<Seg>,
    membranes: Vec<Membrane>,
    /// How far the high wind has carried the clouds, and when it was last moved.
    cloud: Option<(f32, Instant)>,
    /// Where each floating mat was in the older snapshot, and that snapshot's tick.
    mats_before: (u64, HashMap<u64, [f32; 3]>),
}

impl Living {
    /// Loads the saved world, or starts a new one if there is none, keeping
    /// its chronicle in `chronicle_dir`.
    pub fn start(
        save_path: Option<PathBuf>,
        chronicle_dir: Option<PathBuf>,
        on_start: Option<Box<dyn FnOnce() + Send>>,
    ) -> Self {
        let now = unix_now();
        let loaded = save_path.as_deref().map(save::load);
        let mut chronicle = chronicle_dir.map(Chronicle::open);
        let (sim, catch_up) = match loaded {
            Some(Ok(sim)) => {
                let behind = (now - (sim.epoch() + sim.time())).clamp(0.0, CATCH_UP_MAX);
                log::info!(
                    "world loaded: {:.1} days old, {} creatures, catching up {:.0} s",
                    sim.time() / 86400.0,
                    sim.world().creatures.len(),
                    behind
                );
                // History the world lost with an older save is lost here too.
                if let Some(c) = &mut chronicle {
                    c.rewind_to(sim.time());
                }
                (sim, behind)
            }
            other => {
                if let Some(Err(e)) = other {
                    log::warn!("no saved world ({e}); a new one begins");
                }
                let seed = now as u64 ^ 0x5eed_7e57;
                let sim = Sim::new(seed, now);
                if let Some(c) = &mut chronicle {
                    c.new_world();
                }
                (sim, 0.0)
            }
        };
        let recorder = chronicle.map(|c| Box::new(Chronicler(c, Sketchbook::default())) as Box<dyn Recorder>);
        Self::run(sim, Options { save_path, save_every: SAVE_EVERY, catch_up, on_start, recorder })
    }

    /// Runs a world already in hand, e.g. one the preview tool fast-forwarded.
    pub fn run(sim: Sim, opts: Options) -> Self {
        let runner = Runner::start(sim, opts);
        Self {
            runner,
            plants_tick: None,
            pose: Pose::default(),
            segs: Vec::new(),
            membranes: Vec::new(),
            cloud: None,
            mats_before: (u64::MAX, HashMap::new()),
        }
    }

    /// The landscape's weather and river as the simulation has them now.
    pub fn world_view(&mut self, mut view: WorldView) -> WorldView {
        let f = self.runner.frames();
        let e = &f.next.env;
        view.band_shift = e.band_shift;
        view.storm = e.storm;
        view.storm_x = e.storm_x;
        view.lightning = e.lightning;
        view.lightning_x = e.lightning_x;
        view.volcano = e.eruption;
        view.lava = e.lava;
        view.ash = e.ash;
        view.river_phase = e.river.phase;
        view.river_flood = e.river.flood;
        view.cold = e.cold;
        view.cold_x = e.cold_x;
        view.dust = e.dust;
        view.dust_x = e.dust_x;
        view.wind_low = e.wind_low;
        view.wind_high = e.wind_high;
        let now = Instant::now();
        let (drift, last) = self.cloud.unwrap_or((view.cloud_drift, now));
        let drift = (drift + e.wind_high * 0.5 * (now - last).as_secs_f32().min(1.0)) % 4096.0;
        self.cloud = Some((drift, now));
        view.cloud_drift = drift;
        view
    }

    /// Asks for the plants to be uploaded again, e.g. after a new renderer.
    pub fn replant(&mut self) {
        self.plants_tick = None;
    }

    /// Fills `plants` with the rooted plants when they need uploading again,
    /// returning whether it did. Floating mats go with the creatures.
    pub fn plants(&mut self, batch: &mut LifeBatch) -> bool {
        let f = self.runner.frames();
        let tick = f.next.tick;
        if self.plants_tick.is_some_and(|t| tick < t + PLANTS_EVERY) {
            return false;
        }
        self.plants_tick = Some(tick);
        batch.clear();
        for p in f.next.plants.iter().filter(|p| !p.floating) {
            pose::plant(p, &mut self.pose);
            self.add_plant(p, batch);
        }
        true
    }

    fn add_plant(&mut self, p: &PlantView, batch: &mut LifeBatch) {
        let height = p.scale * p.body.extent * self.pose.shrink;
        convert(&self.pose, |w| 0.1 * height * w * w, &mut self.segs, &mut self.membranes);
        batch.add(&Organism {
            segs: &self.segs,
            membranes: &self.membranes,
            look: &Look { genes: p.look },
            z: self.pose.z,
            alive: 1.0,
            signal: p.glow * p.vigor * 0.3,
            symbol: 0,
            bleeding: 0.0,
            infection: None,
            clip_y: None,
            seed: p.id as u32,
        });
    }

    /// Fills `batch` with every creature, body, egg and floating mat, blended to now.
    pub fn creatures(&mut self, batch: &mut LifeBatch) {
        let f = self.runner.frames();
        let t = f.alpha();
        batch.clear();
        if self.mats_before.0 != f.prev.tick {
            let (tick, before) = &mut self.mats_before;
            *tick = f.prev.tick;
            before.clear();
            before.extend(f.prev.plants.iter().filter(|p| p.floating).map(|p| (p.id, p.pos)));
        }
        for p in f.next.plants.iter().filter(|p| p.floating) {
            let to = p.pos;
            let from = self.mats_before.1.get(&p.id).copied().unwrap_or(to);
            let pos = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
            pose::plant_at(p, pos, &mut self.pose);
            self.add_plant(p, batch);
        }
        for v in &f.next.creatures {
            pose::creature(v, t, &mut self.pose);
            convert(&self.pose, |_| 0.0, &mut self.segs, &mut self.membranes);
            batch.add(&Organism {
                segs: &self.segs,
                membranes: &self.membranes,
                look: &Look { genes: v.look },
                z: self.pose.z,
                alive: 1.0,
                signal: v.flash,
                symbol: v.symbol,
                bleeding: v.bleeding,
                infection: v.infection.as_ref().map(|i| Infection {
                    load: i.load,
                    visible: i.visible,
                    hue: i.hue,
                    symbiont: i.symbiont,
                }),
                clip_y: clip(v),
                seed: v.id as u32,
            });
        }
        for r in &f.next.remains {
            pose::remains(r, t, &mut self.pose);
            convert(&self.pose, |_| 0.0, &mut self.segs, &mut self.membranes);
            batch.add(&Organism {
                segs: &self.segs,
                membranes: &self.membranes,
                look: &Look { genes: r.look },
                z: self.pose.z,
                alive: 1.0 - r.decay,
                signal: 0.0,
                symbol: 0,
                bleeding: 0.0,
                infection: None,
                clip_y: None,
                seed: r.id as u32,
            });
        }
        for e in &f.next.eggs {
            let (at, shrink, z) = pose::project(e.pos);
            let r = (e.size * 0.5 * shrink).max(0.0008);
            let y = at[1] + r;
            self.segs.clear();
            self.membranes.clear();
            self.segs.push(Seg {
                a: [at[0] - r * 0.2, y],
                b: [at[0] + r * 0.2, y],
                ra: r,
                rb: r,
                tissue: Tissue::Brood,
                damage: 0.0,
                side: 0.0,
                sway: 0.0,
            });
            batch.add(&Organism {
                segs: &self.segs,
                membranes: &self.membranes,
                look: &Look { genes: e.look },
                z,
                alive: 1.0,
                signal: e.progress * 0.2,
                symbol: 0,
                bleeding: 0.0,
                infection: None,
                clip_y: None,
                seed: e.id as u32,
            });
        }
    }

    /// Where a creature is drawn now, if it is alive: its floor point on screen
    /// and its length, in world units.
    #[allow(dead_code)] // for the preview tool
    pub fn locate(&self, id: u64) -> Option<([f32; 2], f32)> {
        let f = self.runner.frames();
        let c = f.next.creatures.iter().find(|c| c.id == id)?;
        let (at, shrink, _) = pose::project(c.pos);
        Some((at, c.scale * c.body.extent * shrink))
    }

    /// Saves and stops the simulation.
    pub fn stop(self) {
        self.runner.stop();
    }
}

/// Writes the world's history down for the chronicle window, and draws its
/// species now and then, on the sim thread, whether or not the desktop shows.
struct Chronicler(Chronicle, Sketchbook);

impl Recorder for Chronicler {
    fn record(&mut self, facts: Vec<(f64, Fact)>) {
        self.0.record_all(facts);
    }

    fn saved(&mut self) {
        self.0.checkpoint();
    }

    fn frame(&mut self, frame: &Frame) {
        // Usually nothing: it looks once a minute and draws a species every half hour.
        for (species, likeness) in self.1.sketch(frame, self.0.history()) {
            self.0.record_likeness(species, likeness);
        }
    }
}

/// Burrowers are drawn only above the ground.
fn clip(c: &CreatureView) -> Option<f32> {
    (c.locomotion == Locomotion::Burrow && c.pos[2] < 0.01).then(|| vesper_terrain::floor_h(c.pos[0], c.pos[1]) - 0.002)
}

/// Copies a pose into the renderer's types. `sway` turns the sim's 0..1 wind
/// weight into how far the tip bends, in world units.
fn convert(pose: &Pose, sway: impl Fn(f32) -> f32, segs: &mut Vec<Seg>, membranes: &mut Vec<Membrane>) {
    segs.clear();
    membranes.clear();
    segs.extend(pose.segs.iter().map(|s| Seg {
        a: s.a,
        b: s.b,
        ra: s.ra,
        rb: s.rb,
        tissue: tissue(s.tissue),
        damage: s.damage,
        side: s.side as f32,
        sway: sway(s.sway),
    }));
    membranes.extend(pose.membranes.iter().map(|m| Membrane {
        a: m.a,
        b: m.b,
        c: m.c,
        tissue: tissue(m.tissue),
        side: 0.0,
    }));
}

fn tissue(t: vesper_sim::Tissue) -> Tissue {
    use vesper_sim::Tissue as S;
    match t {
        S::Frame => Tissue::Frame,
        S::Muscle => Tissue::Muscle,
        S::LightFilm => Tissue::LightFilm,
        S::Gut => Tissue::Gut,
        S::Float => Tissue::Float,
        S::Sense => Tissue::Sense,
        S::Glow => Tissue::Glow,
        S::Armor => Tissue::Armor,
        S::Spine => Tissue::Spine,
        S::Store => Tissue::Store,
        S::Brood => Tissue::Brood,
        S::Mouth => Tissue::Mouth,
    }
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
