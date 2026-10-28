//! A creature's state, and what its body lets it do.

use crate::body::{Body, Phenotype};
use crate::brain::{Brain, NO};
use crate::frame::Moment;
use crate::genome::{Genome, Tissue};
use crate::history::{CreatureId, DeathCause, GroupId, Life, Locomotion, MindSketch, SpeciesId};
use crate::mind::Mind;
use crate::physics::Rig;
use crate::plant::Seed;
use crate::strain::Infection;
use crate::world::{BASAL, BRAIN_COST, E_PER_MATTER, MATTER_DENSITY};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What a creature is visibly doing, for the renderer and the chronicle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    #[default]
    Idle,
    Moving,
    Eating,
    Attacking,
    Fleeing,
    Mating,
    Sharing,
    Grooming,
    Guarding,
    Laying,
    /// Caught in a trap plant, or being eaten alive.
    Held,
    /// Settled down: living cheaply, and letting what it learned sink in.
    Resting,
    /// A threat display: rearing up, gaping and bristling at a rival.
    Displaying,
    /// A young one's mock fight with another, which does no harm.
    Playing,
    /// Staying by the body of one it knew.
    Mourning,
}

/// What a body can do right now, in world units.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Abilities {
    /// Nose to tail.
    pub length: f32,
    /// Matter this body holds when whole at its current growth.
    pub body_matter: f32,
    pub locomotion: Option<Locomotion>,
    /// Lift from float bladders against weight; 1 hovers.
    pub buoyancy: f32,
    /// Top speed, world units per second.
    pub speed: f32,
    /// Matter torn off per second of biting.
    pub bite: f32,
    /// Matter digested per second.
    pub gut: f32,
    /// Can strain detritus from the air.
    pub filter: bool,
    /// 0..0.8 fraction of damage turned aside.
    pub armor: f32,
    /// Damage returned to whatever bites it, per unit of bite.
    pub spines: f32,
    /// Energy per second from full sunlight.
    pub film: f32,
    pub sense: f32,
    /// Range of its light signals.
    pub glow: f32,
    /// Energy it can hold.
    pub energy_cap: f32,
    /// Energy per second just to stay alive.
    pub upkeep: f32,
    /// 0..1 brood tissue; more breeds faster.
    pub brood: f32,
    pub legs: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub kills: u32,
    pub offspring: u32,
    pub eaten: f32,
    pub kindness: u32,
    pub scars: u32,
}

/// Weather builds up in a body and only harms it once it has: one cold
/// snap, soaking or dry afternoon is nothing, but hours of them are. Each
/// runs 0 (fine) to about 1 (in danger); harm starts past about 0.6.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Exposure {
    /// Body heat lost below what it can bear.
    pub chill: f32,
    /// Heat taken on above what it can bear.
    pub heat: f32,
    /// Water lost and not drunk or eaten back.
    pub thirst: f32,
    /// How wet it is: rain and river soak in and make cold far worse.
    pub soaked: f32,
    /// Ash and dust clogging its breathing.
    pub grit: f32,
}

impl Exposure {
    /// The worst of it, 0..1, for anything that wants one number.
    pub fn worst(&self) -> f32 {
        self.chill.max(self.heat).max(self.thirst).max(self.grit).min(1.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Creature {
    pub id: CreatureId,
    pub species: SpeciesId,
    pub genome: Arc<Genome>,
    /// Grown again from the genome when a world is loaded.
    #[serde(skip, default = "crate::body::empty")]
    pub body: Arc<Body>,
    /// Segments torn off, as a bit mask over `body.segments`.
    pub lost: u64,
    pub pheno: Phenotype,
    pub abil: Abilities,
    /// 0 newborn .. 1 adult.
    pub maturity: f32,
    /// x along the band, depth on the ground plane (world units), altitude.
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    /// Facing in the x-depth plane, radians; 0 faces the night edge.
    pub heading: f32,
    pub gait: f32,
    pub matter: f32,
    pub energy: f32,
    /// 0..1.
    pub health: f32,
    /// Damage built up by age, 0..1; death at 1.
    pub wear: f32,
    pub age: f32,
    pub pain: f32,
    pub bleeding: f32,
    pub brain: Brain,
    pub mind: Mind,
    pub action: Action,
    pub effort: f32,
    /// Light signal shown now (0..1) and its symbol.
    pub flash: f32,
    pub symbol: u8,
    /// Meaning last signalled and when, for spotting lures.
    pub lure: Option<(usize, f64)>,
    pub infections: Vec<Infection>,
    /// Strains it has beaten and now resists.
    pub immunity: Vec<u64>,
    pub sterile: bool,
    pub parent: Option<CreatureId>,
    pub generation: u32,
    pub born: f64,
    pub tally: Tally,
    pub rank: f32,
    pub group: Option<GroupId>,
    /// Appearance class and how good it was, of the last thing it ate.
    pub last_meal: Option<(u8, f32, f64)>,
    pub last_attacker: Option<(CreatureId, f64)>,
    /// Seconds until it can breed again.
    pub breed_wait: f32,
    pub seed: Option<Seed>,
    pub seed_timer: f32,
    /// Reward gathered since the brain last learned.
    pub reward: f32,
    /// How covered it is from the weather, 0 exposed .. 1 fully: under
    /// plants, or huddled with others.
    pub shelter: f32,
    /// Energy gained recently, smoothed, for remembering good feeding grounds.
    pub intake: f32,
    /// Who or what it's interacting with, for the renderer.
    pub target: Option<CreatureId>,
    /// Where the food it is eating is, for the renderer.
    #[serde(skip)]
    pub focus: Option<[f32; 3]>,
    /// Set when it dies this tick; the body is laid out at the end of the tick.
    #[serde(skip)]
    pub doom: Option<DeathCause>,
    /// The body as a physical thing.
    pub rig: Rig,
    /// What the brain asks of the body: power (-1..1) and rise (-1..1).
    pub drive: [f32; 2],
    /// Body lengths per second along its heading it has been making per unit
    /// of power, running its rhythm forward and in reverse; it uses whichever
    /// has served it better, as it has no other way to know which way its own
    /// body carries it.
    pub stride: f32,
    #[serde(default)]
    pub back: f32,
    /// Muscle work per second at full power, and altitude gained per second
    /// at full rise, as it has found them; for moving it when fast-forwarding.
    #[serde(default)]
    pub toil: f32,
    #[serde(default)]
    pub climb: f32,
    /// What the weather has worn into it.
    #[serde(default)]
    pub exposure: Exposure,
    /// Its pose at each physics step this tick, the first being where the tick began.
    #[serde(skip)]
    pub moments: Vec<Moment>,
}

impl Creature {
    pub fn length(&self) -> f32 {
        self.abil.length
    }

    pub fn scale(&self) -> f32 {
        self.genome.adult_scale * (0.3 + 0.7 * self.maturity)
    }

    pub fn adult_matter(&self) -> f32 {
        adult_matter(&self.genome, &self.body)
    }

    pub fn ready(&self) -> bool {
        !self.sterile
            && self.maturity >= 0.95
            && self.breed_wait <= 0.0
            && self.energy > self.abil.energy_cap * 0.6
            && self.health > 0.6
    }

    /// The coldest and hottest it can stand without harm.
    pub fn bearable(&self) -> (f32, f32) {
        let cold = -(0.35 + self.genome.cold_tolerance * 0.8 + self.pheno.frac(Tissue::Store) * 1.5);
        let hot = 0.35 + self.genome.heat_tolerance * 0.8 + self.abil.armor * 0.5;
        (cold, hot)
    }

    pub fn on_ground(&self) -> bool {
        self.pos[2] <= 0.001
    }

    /// Recomputes the body's abilities after growth or injury.
    pub fn refresh(&mut self) {
        self.pheno = Phenotype::read(&self.body, self.maturity, self.lost);
        self.abil = abilities(&self.genome, &self.body, &self.pheno, self.scale(), self.brain.neurons(), self.matter);
    }

    pub fn life(&self) -> Life {
        Life {
            id: self.id,
            species: self.species,
            parent: self.parent,
            generation: self.generation,
            born: self.born,
            age: self.age,
            kills: self.tally.kills,
            offspring: self.tally.offspring,
            eaten: self.tally.eaten,
            rank: self.rank,
            group: self.group,
            is_leader: false,
            x: self.pos[0],
            kindness: self.tally.kindness,
            scars: self.tally.scars,
            mind: Some(MindSketch {
                curiosity: self.genome.curiosity,
                boldness: self.genome.boldness,
                sociality: self.genome.sociality,
                known: self.mind.known.len() as u32,
                friends: self.mind.known.iter().filter(|a| a.bond > 0.3).count() as u32,
                foes: self.mind.known.iter().filter(|a| a.bond < -0.3).count() as u32,
                places: self.mind.places.len() as u32,
                fears: (0..crate::mind::TASTE_BINS).filter(|&b| self.mind.wary(b) > 0.3).count() as u32,
                learned: (self.brain.learned() / 200.0).min(1.0),
            }),
        }
    }

    pub fn out(&self) -> &[f32; NO] {
        &self.brain.out
    }
}

pub fn adult_matter(g: &Genome, body: &Body) -> f32 {
    let mass: f32 = body.segments.iter().map(|s| s.area() * s.tissue.density()).sum();
    mass * g.adult_scale * g.adult_scale * MATTER_DENSITY
}

pub fn abilities(g: &Genome, body: &Body, p: &Phenotype, scale: f32, neurons: usize, matter: f32) -> Abilities {
    let s2 = scale * scale;
    let mass = p.mass.max(0.05);
    let length = body.extent * scale;
    let muscle = p.frac(Tissue::Muscle);
    let armor = p.frac(Tissue::Armor);
    let buoyancy = p.area[Tissue::Float.index()] * 3.5 / mass;
    let flat = p.membrane / p.total_area.max(0.05);
    let locomotion = if p.segments == 0 {
        None
    } else if buoyancy >= 1.0 {
        Some(if flat > 0.25 {
            Locomotion::Glide
        } else if muscle > 0.12 || p.tail > 0.0 {
            Locomotion::Swim
        } else {
            Locomotion::Drift
        })
    } else if buoyancy >= 0.55 && (p.tail > 0.0 || flat > 0.15) && muscle > 0.15 {
        Some(Locomotion::Swim)
    } else if p.legs >= 2 {
        Some(Locomotion::Walk)
    } else if armor + p.frac(Tissue::Frame) > 0.6 && muscle > 0.05 && p.has(Tissue::Mouth) {
        Some(Locomotion::Burrow)
    } else {
        Some(Locomotion::Crawl)
    };
    // Body lengths per second.
    let mut bls = 0.12 + 2.2 * muscle;
    match locomotion {
        Some(Locomotion::Walk) => bls += 0.25 * (p.legs as f32 / 2.0).min(3.0),
        Some(Locomotion::Swim) => bls += 0.6 * (p.tail / p.total_area).min(0.5) + 0.8 * flat.min(0.4),
        Some(Locomotion::Glide) => bls += 0.4,
        Some(Locomotion::Drift) => bls *= 0.5,
        Some(Locomotion::Crawl) | Some(Locomotion::Burrow) => bls *= 0.5,
        _ => {}
    }
    bls *= 1.0 - armor * 0.6;
    let speed = (bls * length).clamp(0.002, 0.09);
    let mouth = p.area[Tissue::Mouth.index()] * s2 * MATTER_DENSITY;
    let gut = p.area[Tissue::Gut.index()] * s2 * MATTER_DENSITY;
    let film = p.area[Tissue::LightFilm.index()] * s2 * MATTER_DENSITY;
    let upkeep_mix: f32 = Tissue::ALL.iter().map(|t| p.area[t.index()] * t.density() * t.upkeep()).sum::<f32>() / mass;
    let tolerance = 0.3 * (g.heat_tolerance + g.cold_tolerance);
    let body_matter = mass * s2 * MATTER_DENSITY;
    let m = matter.max(body_matter * 0.3).max(1e-4);
    Abilities {
        length,
        body_matter,
        locomotion,
        buoyancy,
        speed,
        bite: mouth * (0.6 + muscle * 2.0) * 0.8,
        gut: (gut * 0.3 + m * 0.002) * (0.5 + p.frac(Tissue::Gut)),
        filter: gut > 0.0 && buoyancy >= 0.55,
        armor: (armor * 1.2).min(0.8),
        spines: p.frac(Tissue::Spine) * 3.0,
        film: film * 4.0,
        sense: (0.04 + p.area[Tissue::Sense.index()].sqrt() * scale * 9.0).clamp(0.03, 0.3),
        glow: (0.03 + p.area[Tissue::Glow.index()].sqrt() * scale * 14.0).min(0.45),
        energy_cap: m * E_PER_MATTER * (1.0 + 5.0 * p.frac(Tissue::Store)),
        upkeep: BASAL * m.powf(0.75) * upkeep_mix * (1.0 + tolerance + g.toxin * 0.3 + g.detox * 0.3)
            + BRAIN_COST * neurons as f32,
        brood: p.frac(Tissue::Brood),
        legs: p.legs,
    }
}
