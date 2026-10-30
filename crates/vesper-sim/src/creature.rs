//! A creature's state, and what its body lets it do.

use crate::body::{Body, Phenotype};
use crate::brain::{Brain, NO};
use crate::genome::{Form, Genome, Tissue};
use crate::history::{CreatureId, DeathCause, GroupId, Life, Locomotion, MindSketch, SpeciesId};
use crate::mind::Mind;
use crate::motion::{AFLOAT, GRAVITY, Motion};
use crate::organ::{FARES, Fare, Organ, mouth_takes, strikes};
use crate::plant::Seed;
use crate::strain::Infection;
use crate::world::{BASAL, BRAIN_COST, E_DETRITUS, E_MEAT, E_PER_MATTER, E_PLANT, MATTER_DENSITY};

/// Matter a mouth that is the whole body would take in per second, per
/// unit of body matter, before diminishing returns.
pub const INTAKE: f32 = 0.45;
/// Matter of toughness 1 a gut that is the whole body would deal with per
/// second, per unit of body matter.
pub const DIGEST: f32 = 0.12;
/// Matter torn off per second by striking organs that are the whole body,
/// per unit of body matter.
pub const STRIKE: f32 = 0.35;
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
    /// How it naturally gets about; the medium can change it (see `motion`).
    pub locomotion: Option<Locomotion>,
    /// Lift from float bladders against weight; 1 hovers.
    pub buoyancy: f32,
    /// Top speed its natural way, world units per second; and on the ground
    /// (walking, crawling or burrowing), swimming (0 if it can't) and flying
    /// (0 if its wings can't carry it).
    pub speed: f32,
    pub walk: f32,
    pub swim: f32,
    pub fly: f32,
    /// How fast it can rise or sink when something holds it up.
    pub climb: f32,
    /// Hip to foot, world units.
    pub leg: f32,
    /// Matter torn off per second of fighting, by jaws, claws, horns and clubs.
    pub strike: f32,
    /// Matter of each food (see `organ::Fare`) its mouths can take in per second.
    pub takes: [f32; FARES],
    /// Matter its gut can deal with per second, for food of toughness 1.
    pub gut: f32,
    /// How far grinding teeth and rasps soften tough food before the gut
    /// gets it: 0 not at all .. 0.6.
    pub chew: f32,
    /// 0..0.8 fraction of damage turned aside.
    pub armor: f32,
    /// More armour while pulled into a shell, guarding or resting.
    pub shell: f32,
    /// Damage returned to whatever bites it, per unit of bite.
    pub spines: f32,
    /// Poison it injects with each strike (fangs, stingers), and into
    /// whatever strikes it (stingers, barbs).
    pub venom: f32,
    pub sting: f32,
    /// Grip of talons and claws: how much a hold tires its prey.
    pub grip: f32,
    /// Energy per second from full sunlight.
    pub film: f32,
    /// The farthest it can perceive anything by any sense.
    pub sense: f32,
    /// Range of sight in full light, of smell, of hearing (things that move)
    /// and of touch.
    pub sight: f32,
    pub smell: f32,
    pub hearing: f32,
    pub touch: f32,
    /// How well it sees in dim light, 0..1: big eyes gather more of it.
    pub night_eyes: f32,
    /// How much poison it tastes before swallowing, 0..1.
    pub taste: f32,
    /// How easily others see it, about 0.3 cryptic .. 1.5 gaudy.
    pub conspicuous: f32,
    /// Horns, crests, frills and plumes, to impress rivals and mates.
    pub ornament: f32,
    /// How far it shelters dug in when resting, 0..0.8.
    pub burrow: f32,
    /// How easily it breathes under effort, 0..1; gills clog in ash.
    pub breath: f32,
    /// Water lost through the skin, relative to a plain body, and water
    /// stored against drought, 0..0.6.
    pub leak: f32,
    pub water_store: f32,
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
    /// How its body is carrying itself: stride clock, speed, facing.
    #[serde(default)]
    pub motion: Motion,
    /// What the brain asks of the body: power (-1..1) and rise (-1..1).
    pub drive: [f32; 2],
    /// What the weather has worn into it.
    #[serde(default)]
    pub exposure: Exposure,
    /// Venom working in its body, 0..about 1: it hurts, and it numbs and slows.
    #[serde(default)]
    pub venom: f32,
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

    /// The coldest and hottest it can stand without harm. Its genes set a
    /// little of it; the rest is its body. Fur, plumes and fat keep heat in
    /// and make heat harder to bear; thin fins, sails and big ears shed it.
    /// Big bodies hold their heat (Bergmann's rule) and lanky ones with long
    /// thin limbs lose it (Allen's rule).
    pub fn bearable(&self) -> (f32, f32) {
        comfort(&self.genome, &self.pheno, self.abil.armor)
    }

    pub fn on_ground(&self) -> bool {
        self.pos[2] <= 0.001
    }
}

/// Energy in a unit of matter of each food.
pub fn fare_energy(fare: Fare) -> f32 {
    match fare {
        Fare::Leaf | Fare::Crust => E_PLANT,
        Fare::Fruit => E_PLANT * 1.6,
        Fare::Nectar => E_PLANT * 2.2,
        Fare::Flesh | Fare::Carrion | Fare::Egg => E_MEAT,
        Fare::Plankton => E_DETRITUS,
        Fare::Soil => E_DETRITUS * 0.8,
    }
}

/// The coldest and hottest a body grown from these genes can stand (see
/// `Creature::bearable`).
pub fn comfort(g: &Genome, p: &Phenotype, armor: f32) -> (f32, f32) {
    let o = |x: Organ| p.organ(x);
    let coat = o(Organ::Fur) + o(Organ::Plume) * 0.7;
    let fat = p.frac(Tissue::Store);
    let radiator = o(Organ::Radiator) + o(Organ::Ear) * 0.6 + o(Organ::Sail) * 0.3;
    let bulk = ((g.adult_scale / 0.01).ln() * 0.25).clamp(-0.3, 0.4);
    let lanky = ((p.outline / p.total_area.max(0.05) - 4.0) / 10.0).clamp(0.0, 1.0);
    let cold = -(0.3 + g.cold_tolerance * 0.55 + coat * 1.6 + fat * 1.3 + bulk - lanky * 0.3 - radiator * 0.8);
    let hot = 0.3 + g.heat_tolerance * 0.55 + radiator * 1.6 + lanky * 0.3 + o(Organ::Hump) * 0.6
        - coat * 1.1
        - fat * 0.5
        - bulk * 0.5
        + armor * 0.15;
    (cold, hot)
}

impl Creature {
    /// Matter of a food it can take in per second with its mouths and gut,
    /// for food `tough` times as hard as usual for its kind.
    pub fn intake(&self, fare: Fare, tough: f32) -> f32 {
        let chewed = match fare {
            Fare::Leaf | Fare::Crust | Fare::Soil => 1.0 - self.abil.chew,
            _ => 1.0,
        };
        let t = (fare.toughness() * tough * chewed).max(0.05);
        self.abil.takes[fare.index()].min(self.abil.gut / t)
    }

    /// How much of a food's energy its gut gets out: plant eaters' guts
    /// for plants (tuned to some plant chemistries more than others), meat
    /// eaters' for flesh.
    pub fn efficiency(&self, fare: Fare, chem: f32) -> f32 {
        let diet = self.genome.diet;
        match fare {
            Fare::Leaf | Fare::Crust | Fare::Fruit | Fare::Nectar => {
                (0.15 + 0.85 * (1.0 - diet)) * self.genome.digests(chem)
            }
            Fare::Flesh | Fare::Carrion => 0.1 + 0.9 * diet,
            Fare::Egg => 0.2 + 0.8 * diet,
            Fare::Plankton => 0.6,
            Fare::Soil => 0.5,
        }
    }

    /// Energy per second it could get from a food, all being well.
    pub fn yield_rate(&self, fare: Fare, tough: f32, chem: f32) -> f32 {
        self.intake(fare, tough) * self.efficiency(fare, chem) * fare_energy(fare)
    }

    /// Whether it has any mouth to eat with.
    pub fn can_eat(&self) -> bool {
        self.abil.takes.iter().any(|&t| t > 0.0)
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

/// Diminishing returns on an organ's share of the body: the first of it
/// does the most, so a body does better with several organs than with one
/// enormous one.
fn worth(share: f32) -> f32 {
    share.max(0.0).sqrt()
}

pub fn abilities(g: &Genome, body: &Body, p: &Phenotype, scale: f32, neurons: usize, matter: f32) -> Abilities {
    let s2 = scale * scale;
    let mass = p.mass.max(0.05);
    let length = body.extent * scale;
    let muscle = p.frac(Tissue::Muscle);
    let o = |x: Organ| p.organ(x);
    let buoyancy = p.area[Tissue::Float.index()] * 3.5 / mass;
    let total = p.total_area.max(0.05);
    let armor_cover = o(Organ::Plating) + o(Organ::Shell) + o(Organ::Scales) * 0.5 + o(Organ::Frill) * 0.4;
    let slow = 1.0 - armor_cover.min(1.0) * 0.5;
    let muscle_k = (0.6 + 2.0 * muscle).min(1.4);
    let leg = p.leg * scale;
    let digger = o(Organ::Digger) > 0.03;
    // On the ground: stride length sets speed (Froude scaling), so long legs
    // are fast and short ones scurry; a legless body inches by waves of
    // squeezing, as worms do.
    let walk = if p.legs >= 2 {
        let gait_k = match p.legs {
            2 => 0.85,
            3..=4 => 1.0,
            5..=6 => 0.9,
            _ => 0.8,
        };
        0.6 * (GRAVITY * leg).sqrt() * gait_k * muscle_k * slow
    } else {
        length * (0.1 + 0.4 * muscle + 0.15 * p.form_frac(Form::Beads)) * slow * if digger { 0.6 } else { 1.0 }
    };
    // Through air or water: a fluked tail and fins push, a thick muscular
    // tail most of all.
    let thrust = (p.tail + p.fluke * 1.5 + p.fins) / total;
    let swim = if thrust > 0.0 { length * (0.4 + 2.5 * thrust.min(0.5)) * muscle_k } else { 0.0 };
    // In the air: enough wing for the weight, and muscle to beat it.
    let load = mass * (1.0 - buoyancy).max(0.0) / p.membrane.max(1e-6);
    let fly = if p.wings > 0.0 && load < 3.0 && muscle > 0.08 {
        length * (3.0 + 5.0 * (p.wings / total).min(0.4)) * (1.0 - load / 3.0).powf(0.3) * muscle_k
    } else {
        0.0
    };
    let (walk, swim, fly) = (walk.clamp(0.002, 0.1), swim.min(0.1), fly.min(0.12));
    let climb = (swim.max(fly) * 0.6).max(length * 0.5);
    let locomotion = if p.segments == 0 {
        None
    } else if buoyancy >= AFLOAT {
        Some(if swim > 0.0 { Locomotion::Swim } else { Locomotion::Drift })
    } else if fly > 0.0 {
        Some(Locomotion::Glide)
    } else if p.legs >= 2 {
        Some(Locomotion::Walk)
    } else if digger || (p.frac(Tissue::Armor) > 0.5 && muscle > 0.05) {
        Some(Locomotion::Burrow)
    } else {
        Some(Locomotion::Crawl)
    };
    let speed = match locomotion {
        Some(Locomotion::Swim) => swim,
        Some(Locomotion::Drift) => swim.max(walk * 0.3),
        Some(Locomotion::Glide) => fly,
        _ => walk,
    };
    let body_matter = mass * s2 * MATTER_DENSITY;
    let m = matter.max(body_matter * 0.3).max(1e-4);
    let power = 0.6 + muscle * 2.0;
    // Mouths: each takes its own foods, by its share of the body.
    let mut takes = [0.0f32; FARES];
    let mut strike = 0.0;
    for organ in Organ::ALL {
        let share = o(organ);
        if share <= 0.0 {
            continue;
        }
        let k = worth(share);
        for (t, v) in takes.iter_mut().zip(mouth_takes(organ)) {
            *t += v * k;
        }
        strike += strikes(organ) * k;
    }
    // Digging hands turn up soil to eat, and gills strain specks from the
    // air they breathe, as clams and sea squirts do.
    takes[Fare::Soil.index()] += worth(o(Organ::Digger)) * 0.4;
    takes[Fare::Plankton.index()] += worth(o(Organ::Gill)) * 0.5;
    for t in takes.iter_mut() {
        *t *= body_matter * INTAKE;
    }
    let stomach = o(Organ::Stomach);
    let gut = body_matter * DIGEST * (worth(stomach) + 0.06);
    let chew = (o(Organ::Grinder) * 3.0 + o(Organ::Radula) * 2.0).min(0.6);
    // Senses. Ranges grow with the organ's size, not its share, so a big
    // animal with small eyes still sees far.
    let size = |a: f32| (a.max(0.0)).sqrt() * scale;
    let kit = &p.kit;
    let sight = size(kit.get(Organ::Eye) + kit.get(Organ::Ocellus) * 0.12) * 70.0;
    let smell = size(kit.get(Organ::Antenna)) * 45.0;
    let hearing = size(kit.get(Organ::Ear)) * 55.0;
    let touch = length * 0.6 + 0.008 + size(kit.get(Organ::Feeler)) * 5.0;
    let sense = sight.max(smell).max(hearing).max(touch).clamp(0.02, 0.32);
    let ornament =
        o(Organ::Horn) + o(Organ::Crest) * 1.3 + o(Organ::Frill) + o(Organ::Plume) * 0.8 + o(Organ::Radiator) * 0.4;
    let conspicuous = (0.45 + 0.45 * g.look[2] + ornament * 2.0 + o(Organ::Photophore) * 2.0).clamp(0.3, 1.5);
    let gill = o(Organ::Gill);
    let film = p.area[Tissue::LightFilm.index()] * s2 * MATTER_DENSITY;
    let upkeep_mix: f32 = Tissue::ALL.iter().map(|t| p.area[t.index()] * t.density() * t.upkeep()).sum::<f32>() / mass;
    let tolerance = 0.3 * (g.heat_tolerance + g.cold_tolerance);
    Abilities {
        length,
        body_matter,
        locomotion,
        buoyancy,
        speed,
        walk,
        swim,
        fly,
        climb,
        leg,
        strike: strike * power * body_matter * STRIKE,
        takes,
        gut,
        chew,
        armor: (armor_cover * 1.2).min(0.8),
        shell: (o(Organ::Shell) * 2.5).min(0.5),
        spines: o(Organ::Spine) * 3.0 + o(Organ::Barb) * 4.0 + o(Organ::Horn) + o(Organ::Club) * 2.0,
        venom: (o(Organ::Fang) * 1.5 + o(Organ::Stinger) * 4.0).min(1.5) * (0.15 + g.toxin),
        sting: (o(Organ::Stinger) * 4.0 + o(Organ::Barb) * 1.5).min(1.5) * (0.15 + g.toxin),
        grip: (o(Organ::Talon) * 3.0 + o(Organ::Chela) * 2.0 + o(Organ::Tendril) * 2.0).min(1.0),
        film: film * 4.0,
        sense,
        sight,
        smell,
        hearing,
        touch,
        night_eyes: (o(Organ::Eye) * 4.0).min(1.0),
        taste: (o(Organ::Feeler) * 6.0 + o(Organ::Antenna) * 1.5).min(1.0),
        conspicuous,
        ornament,
        burrow: (o(Organ::Digger) * 5.0 + o(Organ::Shell) * 1.5).min(0.8),
        breath: (gill * 5.0).min(1.0),
        leak: (1.0
            + gill * 1.5
            + p.frac(Tissue::LightFilm) * 0.8
            + (p.outline / p.total_area.max(0.05) - 4.0).max(0.0) * 0.03)
            * (1.0 - 0.6 * ((o(Organ::Scales) + o(Organ::Plating) + o(Organ::Shell)) * 2.5).min(1.0)),
        water_store: (o(Organ::Hump) * 3.0).min(0.6),
        glow: (0.03 + p.area[Tissue::Glow.index()].sqrt() * scale * 14.0).min(0.45),
        // Fat and humps store energy; a big stomach or gullet lets it gorge
        // on a kill and go long between meals, as hunters must.
        energy_cap: m
            * E_PER_MATTER
            * (1.0 + 5.0 * p.frac(Tissue::Store) + 4.0 * (stomach + o(Organ::Gullet)).min(0.4)),
        upkeep: BASAL * m.powf(0.75) * upkeep_mix * (1.0 + tolerance + g.toxin * 0.3 + g.detox * 0.3)
            + BRAIN_COST * neurons as f32,
        brood: p.frac(Tissue::Brood) + o(Organ::Pouch) * 0.5,
        legs: p.legs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::Founder;
    use crate::rng::Rng;

    fn grown(g: &Genome) -> (Phenotype, Abilities) {
        let body = Body::develop(g);
        let p = Phenotype::read(&body, 1.0, 0);
        let a = abilities(g, &body, &p, g.adult_scale, g.brain.hidden, adult_matter(g, &body));
        (p, a)
    }

    /// The founder plan with every part of one tissue given a form; for
    /// frame, the skin of the trunk itself.
    fn with(kind: Founder, tissue: Tissue, form: Form) -> Genome {
        let mut g = Genome::founder_plan(kind, &mut Rng::new(4));
        if tissue == Tissue::Frame {
            g.plan.trunk.form = form;
        }
        for m in g.plan.modules.iter_mut().filter(|m| m.kind.tissue() == tissue) {
            m.form = form;
        }
        g
    }

    #[test]
    fn eyes_see_far_and_feelers_only_close() {
        let (_, eyes) = grown(&with(Founder::Stalker, Tissue::Sense, Form::Bulb));
        let (_, feel) = grown(&with(Founder::Stalker, Tissue::Sense, Form::Tuft));
        assert!(eyes.sight > 0.05, "{}", eyes.sight);
        assert!(feel.sight == 0.0 && feel.touch > eyes.touch);
        assert!(eyes.sense > feel.sense);
    }

    #[test]
    fn fur_keeps_out_cold_and_keeps_in_heat() {
        let bare = with(Founder::Grazer, Tissue::Frame, Form::Tube);
        let furry = with(Founder::Grazer, Tissue::Frame, Form::Tuft);
        let fins = with(Founder::Grazer, Tissue::Frame, Form::Paddle);
        let (pb, ab) = grown(&bare);
        let (pf, af) = grown(&furry);
        let (pr, ar) = grown(&fins);
        let (cold_b, hot_b) = comfort(&bare, &pb, ab.armor);
        let (cold_f, hot_f) = comfort(&furry, &pf, af.armor);
        let (cold_r, hot_r) = comfort(&fins, &pr, ar.armor);
        assert!(cold_f < cold_b - 0.2 && hot_f < hot_b, "{cold_f} {cold_b} {hot_f} {hot_b}");
        assert!(hot_r > hot_b + 0.2 && cold_r > cold_b, "{hot_r} {hot_b}");
    }

    #[test]
    fn teeth_suit_their_food() {
        let grind = with(Founder::Grazer, Tissue::Mouth, Form::Plates);
        let fangs = with(Founder::Grazer, Tissue::Mouth, Form::Fork);
        let (_, ag) = grown(&grind);
        let (_, af) = grown(&fangs);
        assert!(ag.takes[Fare::Leaf.index()] > af.takes[Fare::Leaf.index()] * 4.0);
        assert!(af.takes[Fare::Flesh.index()] > ag.takes[Fare::Flesh.index()] * 4.0);
        assert!(af.strike > ag.strike);
        assert!(ag.chew > 0.0 && af.chew == 0.0);
    }

    #[test]
    fn stingers_and_shells_defend() {
        let mut sting = Genome::founder_plan(Founder::Crawler, &mut Rng::new(4));
        sting.toxin = 0.8;
        for m in sting.plan.modules.iter_mut().filter(|m| m.kind == crate::genome::Kind::Spine) {
            m.form = Form::Coil;
        }
        let mut shell = Genome::founder_plan(Founder::Crawler, &mut Rng::new(4));
        shell.plan.trunk.form = Form::Coil;
        let (_, a) = grown(&sting);
        let (_, b) = grown(&shell);
        assert!(a.sting > 0.0 && a.venom > 0.0);
        assert!(b.shell > 0.0 && b.burrow > 0.0);
    }
}
