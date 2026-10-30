//! Creature genomes: a body plan (see `plan`) that development unrolls
//! into a body, the starting wiring of a brain, and the genes for life
//! history, looks and immunity.

use crate::brain::{NI, NO};
use crate::genetics::{self, COMPAT};
pub use crate::plan::{BodyPlan, Kind, Module, Trunk};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const LOOK_GENES: usize = 16;
pub const MAX_HIDDEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tissue {
    /// Shape and attachment points. Cheap.
    Frame,
    /// Movement, grip and strikes.
    Muscle,
    /// Feeds on sunlight. Fragile.
    LightFilm,
    /// Digests food.
    Gut,
    /// Buoyancy in the thick air.
    Float,
    /// Eyes, pits and feelers.
    Sense,
    /// Signals and lures.
    Glow,
    /// Shell and plates.
    Armor,
    /// Spines, stingers and horns.
    Spine,
    /// Fat for famine.
    Store,
    /// Eggs, buds or spores.
    Brood,
    /// Jaws and beaks.
    Mouth,
}

impl Tissue {
    pub const ALL: [Tissue; 12] = [
        Tissue::Frame,
        Tissue::Muscle,
        Tissue::LightFilm,
        Tissue::Gut,
        Tissue::Float,
        Tissue::Sense,
        Tissue::Glow,
        Tissue::Armor,
        Tissue::Spine,
        Tissue::Store,
        Tissue::Brood,
        Tissue::Mouth,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Matter per unit of area, relative to frame.
    pub fn density(self) -> f32 {
        match self {
            Tissue::Float => 0.25,
            Tissue::LightFilm => 0.35,
            Tissue::Armor => 2.2,
            Tissue::Spine => 1.4,
            Tissue::Store => 1.3,
            Tissue::Mouth => 1.4,
            _ => 1.0,
        }
    }

    /// How a part of this tissue narrows to its tip before genes reshape it:
    /// tip thickness over base thickness.
    pub fn taper(self) -> f32 {
        match self {
            Tissue::Spine | Tissue::Mouth => 0.25,
            Tissue::Muscle => 0.7,
            Tissue::Float => 0.85,
            _ => 0.8,
        }
    }

    /// Energy per second to keep a unit of matter of this tissue alive, relative to frame.
    pub fn upkeep(self) -> f32 {
        match self {
            Tissue::Frame => 0.5,
            Tissue::Armor | Tissue::Spine => 0.3,
            Tissue::Store => 0.15,
            Tissue::Float => 0.6,
            Tissue::Sense => 2.0,
            Tissue::Glow => 1.2,
            Tissue::Gut => 1.4,
            Tissue::Muscle => 1.2,
            _ => 1.0,
        }
    }
}

/// The shape a part grows into, whatever it is made of. The same tissue
/// makes very different organs in different forms: muscle as a plain limb,
/// a hooked talon or a pincer; sense tissue as an eye on a stalk or a
/// feathery antenna; armour as a smooth shell, overlapping plates or a coil.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Form {
    /// A plain tapered tube.
    #[default]
    Tube,
    /// Ringed into beads, like a worm, a grub or an antenna.
    Beads,
    /// Swollen into a ball on a stalk: an eye stalk, a sac, a club.
    Bulb,
    /// Ends in a hook that curls back: talons, claws, hooked beaks and horns.
    Hook,
    /// Ends in two jaws that close on each other.
    Pincer,
    /// Splits at the tip into prongs, like antlers or a forked tongue.
    Fork,
    /// A shaft fringed with barbs down both sides: plumes, gills, feathers.
    Frond,
    /// Rays fanned out at the tip with webbing between: fins, flukes, frills.
    Fan,
    /// Flattened into a broad blade: paddles, flippers, ears.
    Paddle,
    /// Overlapping plates along its length, like a pill bug's back.
    Plates,
    /// Bristling with hairs: fur, whiskers, cilia.
    Tuft,
    /// Curled into a spiral toward the tip: tentacles, proboscises, shells.
    Coil,
}

impl Form {
    pub const ALL: [Form; 12] = [
        Form::Tube,
        Form::Beads,
        Form::Bulb,
        Form::Hook,
        Form::Pincer,
        Form::Fork,
        Form::Frond,
        Form::Fan,
        Form::Paddle,
        Form::Plates,
        Form::Tuft,
        Form::Coil,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// Matter in a part of this form, relative to a plain tube of the same
    /// length and thickness.
    pub fn bulk(self) -> f32 {
        match self {
            Form::Bulb => 1.5,
            Form::Plates => 1.3,
            Form::Pincer | Form::Paddle => 1.2,
            Form::Hook | Form::Fork | Form::Fan | Form::Frond | Form::Tuft => 1.1,
            Form::Beads => 0.9,
            Form::Tube | Form::Coil => 1.0,
        }
    }

    /// Whether it pushes on the air like a web or a fin.
    pub fn flat(self) -> bool {
        matches!(self, Form::Fan | Form::Paddle)
    }

    /// A form for a new or changed part: plain tubes are commonest.
    pub fn random(rng: &mut Rng) -> Self {
        if rng.chance(0.35) { Form::Tube } else { *rng.pick(&Form::ALL) }
    }
}

/// The starting wiring of a brain and its learning rule. Weights are stored as
/// small integers (value / 32) so genomes stay compact.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrainGenes {
    pub hidden: usize,
    /// Input to hidden, NI x hidden.
    pub w_ih: Vec<i8>,
    /// Hidden to hidden (recurrent), hidden x hidden.
    pub w_hh: Vec<i8>,
    /// Hidden to output, hidden x NO.
    pub w_ho: Vec<i8>,
    /// Input straight to output (reflexes), NI x NO.
    pub w_io: Vec<i8>,
    pub b_h: Vec<i8>,
    pub b_o: Vec<i8>,
    /// Neuromodulated Hebbian rule: dw = eta * m * trace, where trace follows
    /// a * pre * post + b * pre + c * post + d. [a, b, c, d, eta].
    pub plasticity: [f32; 5],
    /// How quickly the eligibility trace fades, 0..1 per brain step.
    pub trace_keep: f32,
    /// The same rule for the input-to-hidden weights, so what a brain
    /// notices can change with experience as well as what it does.
    pub plasticity_h: [f32; 5],
    /// The critic's inborn expectations: weights on hidden neurons and on inputs.
    pub critic_h: Vec<i8>,
    pub critic_i: Vec<i8>,
    /// How far ahead the critic looks, per brain step (0.2 s), 0..1.
    pub gamma: f32,
    /// How fast the critic corrects its expectations.
    pub critic_rate: f32,
    /// How fast fresh learning is made lasting, per second at rest.
    pub consolidate: f32,
    /// How much the outputs wander on their own, which is how a brain tries things.
    pub explore: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Genome {
    pub plan: BodyPlan,
    pub brain: BrainGenes,
    /// World units per body unit when fully grown.
    pub adult_scale: f32,
    /// How fast it lives and moves: stride and stroke rate over what its
    /// build would set, 0.6 slow and thrifty .. 1.6 quick and hungry.
    pub tempo: f32,
    /// Fastest possible time from hatching to adulthood, in seconds. Food usually limits it more.
    pub growth_time: f32,
    /// How fast damage builds with age; higher lives faster and dies sooner.
    pub aging: f32,
    /// 0 eats plants, 1 eats meat.
    pub diet: f32,
    /// Above 0.5 needs a mate.
    pub sexual: f32,
    /// Offspring per breeding.
    pub clutch: f32,
    /// Matter given to each offspring, as a fraction of the parent's adult matter.
    pub invest: f32,
    /// Above 0.5 young are born live (or budded); below, eggs are laid.
    pub live_birth: f32,
    /// Fraction of young born as sterile helpers.
    pub helpers: f32,
    /// Willingness to give the body to the young at death.
    pub matriphagy: f32,
    pub heat_tolerance: f32,
    pub cold_tolerance: f32,
    /// Poison in the flesh, 0..1.
    pub toxin: f32,
    /// How picky in mates, 0..1.
    pub choosiness: f32,
    /// Look a mate is preferred to have (genes 0..3).
    pub mate_pref: [f32; 4],
    /// Immune signature; strains adapted to one signature struggle in another.
    pub immune: [f32; 4],
    /// Weight given to watching others when learning, 0..1.
    pub imitation: f32,
    /// Chance per signal learned of inventing a new one instead.
    pub innovation: f32,
    /// Temperament, 0..1. Curiosity: how rewarding new ground and new things
    /// are, and how much it tries.
    pub curiosity: f32,
    /// Boldness: how slowly fear builds and how quickly it passes.
    pub boldness: f32,
    /// Sociality: how much it needs company, and how closely it keeps to it.
    pub sociality: f32,
    /// How strongly the genome mutates in offspring.
    pub mutation: f32,
    /// Opaque appearance genes, drawn by the renderer. 0 hue, 1 value, 2 pattern, 3 glow hue.
    pub look: [f32; LOOK_GENES],
    /// Neutral genes passed down the mother's line only, like mitochondria,
    /// that drift fast; shared markers mean a shared mother line (kin scent).
    pub marker: [u8; 8],
    /// The two sets of genes it carries, one from each parent, that the
    /// fields above express together. Missing in species records.
    pub haplotypes: Option<Arc<[Genome; 2]>>,
    /// In a haplotype: how strongly each of its alleles is expressed over the
    /// other copy's, in `loci` order; 128 is equal.
    pub dominance: Vec<u8>,
    /// In a haplotype: genes broken by harmful mutations, one bit each. They
    /// are mostly recessive, so they hurt when both copies are broken, which
    /// inbreeding makes likely.
    pub load: u64,
    /// In a haplotype: alleles of genes that must work with each other.
    /// Lineages apart for long fix alleles that clash in their hybrids.
    pub compat: [u8; COMPAT],
    /// The other parent's immune allele. Both are expressed, so a host with
    /// two different ones resists more strains.
    pub immune_b: [f32; 4],
    /// How fit its genes leave it, 1 sound .. 0, from its broken genes.
    pub vigor: f32,
    /// Leaning toward mates whose immune genes differ from its own (+) or match them (-).
    pub mhc_pref: f32,
    /// Chance a hermaphrodite with no mate fertilises its own eggs.
    pub selfing: f32,
    /// How closely its body follows its genes, 0 loosely .. 1 exactly. No two
    /// bodies grow quite alike; poorly buffered ones, and inbred ones, vary most.
    pub canalization: f32,
    /// Poison it can break down, 0..1, at a cost to keep up. Eaters and the
    /// poisonous things they eat push each other up, round after round.
    pub detox: f32,
    /// The plant chemistry its gut is tuned to, on the same wheel as plant
    /// hues (pigments are where plants keep their chemistry).
    pub palate: f32,
    /// How broad its gut's tuning is, 0 a specialist that thrives on one kind
    /// of plant .. 1 a generalist that gets by on any.
    pub breadth: f32,
}

fn w(rng: &mut Rng, spread: f32) -> i8 {
    (rng.normal() * spread * 32.0).clamp(-127.0, 127.0) as i8
}

impl BrainGenes {
    pub fn random(rng: &mut Rng, hidden: usize) -> Self {
        let mut b = Self {
            hidden,
            w_ih: (0..NI * hidden).map(|_| w(rng, 0.5)).collect(),
            w_hh: (0..hidden * hidden).map(|_| w(rng, 0.2)).collect(),
            w_ho: (0..hidden * NO).map(|_| w(rng, 0.5)).collect(),
            w_io: (0..NI * NO).map(|_| w(rng, 0.15)).collect(),
            b_h: (0..hidden).map(|_| w(rng, 0.2)).collect(),
            b_o: (0..NO).map(|_| w(rng, 0.2)).collect(),
            plasticity: [rng.signed() * 0.5, rng.signed() * 0.1, rng.signed() * 0.1, 0.0, rng.range(0.0, 0.05)],
            trace_keep: rng.range(0.5, 0.95),
            plasticity_h: [rng.signed() * 0.3, rng.signed() * 0.05, rng.signed() * 0.05, 0.0, rng.range(0.0, 0.01)],
            critic_h: (0..hidden).map(|_| w(rng, 0.2)).collect(),
            critic_i: (0..NI).map(|_| w(rng, 0.1)).collect(),
            gamma: rng.range(0.93, 0.99),
            critic_rate: rng.range(0.02, 0.1),
            consolidate: rng.range(0.0, 0.03),
            explore: rng.range(0.1, 0.3),
        };
        // A few reflexes to start from: seek food when hungry, flee pain.
        crate::brain::seed_reflexes(&mut b);
        b
    }

    fn mutate(&mut self, rng: &mut Rng, rate: f32) {
        let p = 0.04 * rate;
        for m in [
            &mut self.w_ih,
            &mut self.w_hh,
            &mut self.w_ho,
            &mut self.w_io,
            &mut self.b_h,
            &mut self.b_o,
            &mut self.critic_h,
            &mut self.critic_i,
        ] {
            for v in m.iter_mut() {
                if rng.chance(p) {
                    *v = (*v as f32 + rng.normal() * 10.0).clamp(-127.0, 127.0) as i8;
                }
            }
        }
        for v in self.plasticity.iter_mut() {
            if rng.chance(0.1 * rate) {
                *v += rng.normal() * 0.03;
            }
        }
        self.plasticity[4] = self.plasticity[4].clamp(0.0, 0.3);
        for v in self.plasticity_h.iter_mut() {
            if rng.chance(0.1 * rate) {
                *v += rng.normal() * 0.02;
            }
        }
        self.plasticity_h[4] = self.plasticity_h[4].clamp(0.0, 0.1);
        if rng.chance(0.1 * rate) {
            self.trace_keep = (self.trace_keep + rng.normal() * 0.05).clamp(0.0, 0.99);
        }
        let mut nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32| {
            if rng.chance(0.1 * rate) {
                *v = (*v + rng.normal() * sd).clamp(lo, hi);
            }
        };
        nudge(&mut self.gamma, 0.01, 0.5, 0.995);
        nudge(&mut self.critic_rate, 0.01, 0.0, 0.3);
        nudge(&mut self.consolidate, 0.004, 0.0, 0.1);
        nudge(&mut self.explore, 0.03, 0.0, 0.8);
        if rng.chance(0.03 * rate) {
            let h = if rng.chance(0.5) { self.hidden + 1 } else { self.hidden.saturating_sub(1) };
            self.resize(h.clamp(2, MAX_HIDDEN), rng);
        }
    }

    /// Adds or removes hidden neurons, keeping the others' wiring.
    pub fn resize(&mut self, hidden: usize, rng: &mut Rng) {
        let old = self.hidden;
        if hidden == old {
            return;
        }
        let keep = old.min(hidden);
        let mut w_ih = vec![0i8; NI * hidden];
        for i in 0..NI {
            for h in 0..hidden {
                w_ih[i * hidden + h] = if h < keep { self.w_ih[i * old + h] } else { w(rng, 0.3) };
            }
        }
        let mut w_hh = vec![0i8; hidden * hidden];
        for a in 0..hidden {
            for b in 0..hidden {
                w_hh[a * hidden + b] = if a < keep && b < keep { self.w_hh[a * old + b] } else { w(rng, 0.1) };
            }
        }
        let mut w_ho = vec![0i8; hidden * NO];
        for h in 0..hidden {
            for o in 0..NO {
                w_ho[h * NO + o] = if h < keep { self.w_ho[h * NO + o] } else { w(rng, 0.1) };
            }
        }
        let mut b_h: Vec<i8> = self.b_h.iter().copied().take(keep).collect();
        b_h.resize_with(hidden, || 0);
        self.critic_h.truncate(keep);
        self.critic_h.resize_with(hidden, || 0);
        self.w_ih = w_ih;
        self.w_hh = w_hh;
        self.w_ho = w_ho;
        self.b_h = b_h;
        self.hidden = hidden;
    }

    fn cross(&self, other: &Self, rng: &mut Rng) -> Self {
        if self.hidden != other.hidden {
            return if rng.chance(0.5) { self.clone() } else { other.clone() };
        }
        // Swap whole neurons so that co-adapted wiring stays together.
        let mut c = self.clone();
        let h = self.hidden;
        for n in 0..h {
            if rng.chance(0.5) {
                for i in 0..NI {
                    c.w_ih[i * h + n] = other.w_ih[i * h + n];
                }
                for o in 0..NO {
                    c.w_ho[n * NO + o] = other.w_ho[n * NO + o];
                }
                c.b_h[n] = other.b_h[n];
                c.critic_h[n] = other.critic_h[n];
            }
        }
        for i in 0..NI {
            if rng.chance(0.5) {
                c.critic_i[i] = other.critic_i[i];
            }
        }
        for o in 0..NO {
            if rng.chance(0.5) {
                for i in 0..NI {
                    c.w_io[i * NO + o] = other.w_io[i * NO + o];
                }
                c.b_o[o] = other.b_o[o];
            }
        }
        if rng.chance(0.5) {
            c.plasticity = other.plasticity;
            c.trace_keep = other.trace_keep;
        }
        if rng.chance(0.5) {
            c.plasticity_h = other.plasticity_h;
        }
        if rng.chance(0.5) {
            c.gamma = other.gamma;
            c.critic_rate = other.critic_rate;
        }
        if rng.chance(0.5) {
            c.consolidate = other.consolidate;
        }
        if rng.chance(0.5) {
            c.explore = other.explore;
        }
        c
    }
}

/// A body plan the founders start from, so the first creatures can feed and move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Founder {
    /// A gas-filled bag with sails and a sieve, feeding on light and specks.
    Drifter,
    /// A four-legged ground grazer with grinding teeth, eyes and ears.
    Grazer,
    /// A finned, bladdered swimmer through the thick air with a fluked tail.
    Swimmer,
    /// A short-legged, plated scavenger that scoops rot out of the soil.
    Crawler,
    /// A long-legged hunter with fangs, eyes and taloned arms.
    Stalker,
    /// A winged fruit-and-egg thief with a beak and perching claws.
    Flier,
}

impl Founder {
    pub const ALL: [Founder; 6] =
        [Founder::Drifter, Founder::Grazer, Founder::Swimmer, Founder::Crawler, Founder::Stalker, Founder::Flier];
}

impl Genome {
    /// A founder exactly as designed, before the variety every founder gets.
    pub fn founder_plan(kind: Founder, rng: &mut Rng) -> Self {
        use Form::*;
        use Kind::*;
        let module =
            |kind: Kind, form: Form, zone: f32, size: f32, rng: &mut Rng| Module::new(kind, form, zone, size, rng);
        let mut trunk = Trunk::plain();
        let mut modules: Vec<Module> = Vec::new();
        let (scale, diet, hidden) = match kind {
            Founder::Drifter => {
                trunk.segments = 2;
                trunk.length = 0.8;
                trunk.girth = 0.6;
                trunk.profile = [0.8, 1.0, 0.8];
                trunk.tissue = Tissue::Float;
                trunk.form = Bulb;
                trunk.head = 0.12;
                trunk.head_width = 0.8;
                trunk.head_form = Bulb;
                trunk.gut = 0.25;
                trunk.brood = 0.1;
                let mut sieve = module(Mouth, Frond, 1.0, 0.35, rng);
                sieve.width = 0.5;
                sieve.angle = -0.6;
                modules.push(sieve);
                let mut film = module(Film, Frond, 0.3, 0.55, rng);
                film.count = 2;
                film.spacing = 0.3;
                film.paired = true;
                modules.push(film);
                let mut sail = module(Float, Fan, 0.5, 0.45, rng);
                sail.width = 0.35;
                modules.push(sail);
                let mut fins = module(Fin, Paddle, 0.6, 0.25, rng);
                fins.angle = -0.3;
                modules.push(fins);
                let mut eye = module(Sense, Tube, 0.7, 0.07, rng);
                eye.width = 0.8;
                modules.push(eye);
                (0.0045, 0.05, 4)
            }
            Founder::Grazer => {
                trunk.segments = 3;
                trunk.girth = 0.32;
                trunk.head = 0.3;
                trunk.head_width = 0.6;
                trunk.neck = 0.25;
                trunk.neck_angle = 0.45;
                trunk.tail = 0.4;
                trunk.tail_width = 0.25;
                trunk.tail_form = Tuft;
                trunk.gut = 0.45;
                let mut legs = module(Leg, Tube, 0.2, 0.6, rng);
                legs.count = 2;
                legs.spacing = 0.5;
                modules.push(legs);
                let mut mouth = module(Mouth, Plates, 1.0, 0.26, rng);
                mouth.width = 0.6;
                modules.push(mouth);
                let mut eyes = module(Sense, Bulb, 0.6, 0.1, rng);
                eyes.width = 0.8;
                modules.push(eyes);
                let mut ears = module(Sense, Paddle, 0.3, 0.14, rng);
                ears.width = 0.5;
                modules.push(ears);
                (0.009, 0.15, 8)
            }
            Founder::Stalker => {
                trunk.segments = 3;
                trunk.girth = 0.24;
                trunk.profile = [1.0, 0.9, 0.8];
                trunk.head = 0.3;
                trunk.head_width = 0.55;
                trunk.neck = 0.3;
                trunk.neck_angle = 0.35;
                trunk.tail = 0.7;
                trunk.tail_width = 0.2;
                trunk.tail_segments = 3;
                trunk.gut = 0.3;
                let mut legs = module(Leg, Tube, 0.25, 0.7, rng);
                legs.count = 2;
                legs.spacing = 0.5;
                legs.width = 0.18;
                modules.push(legs);
                let mut arms = module(Arm, Hook, 0.05, 0.35, rng);
                arms.curl = 0.8;
                modules.push(arms);
                let mut mouth = module(Mouth, Fork, 1.0, 0.28, rng);
                mouth.width = 0.55;
                modules.push(mouth);
                let mut eyes = module(Sense, Bulb, 0.6, 0.12, rng);
                eyes.width = 0.8;
                modules.push(eyes);
                let mut nose = module(Sense, Beads, 0.85, 0.2, rng);
                nose.width = 0.3;
                modules.push(nose);
                (0.0105, 0.85, 10)
            }
            Founder::Swimmer => {
                trunk.segments = 4;
                trunk.girth = 0.28;
                trunk.profile = [0.9, 1.0, 0.7];
                trunk.head = 0.25;
                trunk.head_width = 0.7;
                trunk.tail = 0.9;
                trunk.tail_width = 0.3;
                trunk.tail_segments = 3;
                trunk.tail_form = Fan;
                trunk.gut = 0.45;
                let mut fins = module(Fin, Paddle, 0.3, 0.3, rng);
                fins.width = 0.4;
                modules.push(fins);
                let mut floats = module(Float, Bulb, 0.3, 0.4, rng);
                floats.count = 2;
                floats.spacing = 0.4;
                floats.width = 0.9;
                modules.push(floats);
                let mut mouth = module(Mouth, Tube, 1.0, 0.28, rng);
                mouth.width = 0.6;
                modules.push(mouth);
                let mut eyes = module(Sense, Bulb, 0.6, 0.1, rng);
                eyes.width = 0.8;
                modules.push(eyes);
                let mut gills = module(Gill, Frond, 0.1, 0.15, rng);
                gills.width = 0.5;
                modules.push(gills);
                (0.006, 0.25, 6)
            }
            Founder::Crawler => {
                trunk.segments = 6;
                trunk.length = 1.2;
                trunk.girth = 0.33;
                trunk.profile = [0.8, 1.0, 0.8];
                trunk.tissue = Tissue::Armor;
                trunk.form = Plates;
                trunk.head = 0.15;
                trunk.head_width = 0.9;
                trunk.head_form = Plates;
                trunk.tail = 0.2;
                trunk.tail_tissue = Tissue::Armor;
                trunk.tail_width = 0.5;
                trunk.tail_segments = 1;
                trunk.gut = 0.5;
                trunk.fat = 0.1;
                let mut legs = module(Leg, Tube, 0.15, 0.22, rng);
                legs.count = 3;
                legs.spacing = 0.3;
                legs.width = 0.25;
                modules.push(legs);
                let mut mouth = module(Mouth, Paddle, 1.0, 0.3, rng);
                mouth.angle = -0.6;
                mouth.width = 0.6;
                modules.push(mouth);
                let mut feelers = module(Sense, Beads, 0.8, 0.22, rng);
                feelers.width = 0.3;
                modules.push(feelers);
                let mut spines = module(Spine, Tube, 0.4, 0.18, rng);
                spines.count = 2;
                spines.spacing = 0.3;
                spines.paired = false;
                modules.push(spines);
                (0.006, 0.2, 4)
            }
            Founder::Flier => {
                trunk.segments = 2;
                trunk.length = 0.8;
                trunk.girth = 0.26;
                trunk.form = Frond;
                trunk.head = 0.3;
                trunk.head_width = 0.7;
                trunk.head_form = Bulb;
                trunk.neck = 0.2;
                trunk.neck_angle = 0.5;
                trunk.tail = 0.5;
                trunk.tail_width = 0.25;
                trunk.tail_segments = 2;
                trunk.tail_form = Fan;
                trunk.gut = 0.3;
                let mut wings = module(Wing, Fan, 0.35, 1.2, rng);
                wings.width = 0.08;
                wings.detail = 0.7;
                modules.push(wings);
                let mut legs = module(Leg, Hook, 0.75, 0.4, rng);
                legs.width = 0.15;
                legs.curl = 0.9;
                modules.push(legs);
                let mut mouth = module(Mouth, Hook, 1.0, 0.26, rng);
                mouth.width = 0.55;
                modules.push(mouth);
                let mut eyes = module(Sense, Bulb, 0.6, 0.12, rng);
                eyes.width = 0.8;
                modules.push(eyes);
                let mut float = module(Float, Bulb, 0.6, 0.22, rng);
                float.width = 0.8;
                float.paired = false;
                modules.push(float);
                (0.007, 0.45, 8)
            }
        };
        let mut look = [0.0; LOOK_GENES];
        for v in look.iter_mut() {
            *v = rng.f32();
        }
        let mut marker = [0u8; 8];
        for m in marker.iter_mut() {
            *m = rng.below(256) as u8;
        }
        let mut g = Self {
            plan: BodyPlan { trunk, modules },
            brain: BrainGenes::random(rng, hidden),
            adult_scale: scale * rng.range(0.85, 1.2),
            tempo: rng.range(0.85, 1.15),
            growth_time: rng.range(60.0, 180.0),
            aging: rng.range(0.8, 1.2),
            diet: diet + rng.signed() * 0.05,
            sexual: rng.range(0.0, 0.6),
            clutch: rng.range(2.0, 6.0),
            invest: rng.range(0.08, 0.18),
            live_birth: rng.f32(),
            helpers: 0.0,
            matriphagy: rng.range(0.0, 0.2),
            heat_tolerance: rng.range(0.3, 0.7),
            cold_tolerance: rng.range(0.3, 0.7),
            toxin: rng.range(0.0, 0.1),
            choosiness: rng.range(0.0, 0.3),
            mate_pref: [rng.f32(), rng.f32(), rng.f32(), rng.f32()],
            immune: [rng.f32(), rng.f32(), rng.f32(), rng.f32()],
            imitation: rng.range(0.0, 0.5),
            innovation: rng.range(0.005, 0.03),
            curiosity: rng.range(0.2, 0.6),
            boldness: rng.range(0.3, 0.7),
            sociality: rng.range(0.2, 0.6),
            mutation: rng.range(0.8, 1.2),
            look,
            marker,
            haplotypes: None,
            dominance: Vec::new(),
            load: 0,
            compat: [0; COMPAT],
            immune_b: [0.0; 4],
            vigor: 1.0,
            mhc_pref: rng.range(0.0, 0.3),
            selfing: rng.range(0.0, 0.3),
            canalization: rng.range(0.4, 0.8),
            detox: rng.range(0.0, 0.1),
            palate: rng.f32(),
            breadth: rng.range(0.8, 1.0),
        };
        g.mate_pref.copy_from_slice(&g.look[..4]);
        g.dress(kind);
        g
    }

    /// A founder: a working body plan with random details, carrying two
    /// slightly different sets of genes, and a brain to learn with.
    pub fn founder(kind: Founder, rng: &mut Rng) -> Self {
        Self::founder_diverged(kind, 0, rng)
    }

    /// A founding lineage that has drifted `rounds` further from its kind's
    /// plan, as if it had evolved apart somewhere else.
    pub fn founder_diverged(kind: Founder, rounds: u32, rng: &mut Rng) -> Self {
        let plan = Self::founder_plan(kind, rng);
        // Founders are already a little varied, but still have the organs
        // their way of life needs: a mouth, and legs or wings or fins.
        let mut g = plan.clone();
        for attempt in 0..8 {
            g = plan.clone();
            for _ in 0..1 + rounds {
                g.mutate(rng, 1.0);
            }
            let whole = g.plan.has(Kind::Mouth)
                && match kind {
                    Founder::Drifter => g.plan.has(Kind::Film),
                    Founder::Flier => g.plan.has(Kind::Wing),
                    Founder::Swimmer => g.plan.has(Kind::Float),
                    _ => g.plan.has(Kind::Leg),
                };
            if attempt == 7 || whole {
                break;
            }
        }
        genetics::found(g, rng)
    }

    /// Varies a founder's details within what its way of life allows, so
    /// the first lineages differ in shape as well as in size and colour.
    /// The choice is drawn from the founder's own mother line rather than
    /// the world's random stream.
    fn dress(&mut self, kind: Founder) {
        let rng = &mut Rng::new(u64::from_le_bytes(self.marker) ^ 0xD8E5_5F0E);
        let t = &mut self.plan.trunk;
        t.length *= rng.range(0.85, 1.15);
        t.girth *= rng.range(0.85, 1.2);
        t.arch = rng.signed() * 0.3;
        t.detail = rng.f32();
        t.head *= rng.range(0.8, 1.25);
        t.tail *= rng.range(0.7, 1.4);
        t.tail_curl = rng.signed() * 0.8;
        if !matches!(kind, Founder::Drifter | Founder::Crawler) && rng.chance(0.5) {
            t.form = *rng.pick(&[Form::Tube, Form::Beads, Form::Plates, Form::Tuft, Form::Frond]);
        }
        if rng.chance(0.4) {
            t.head_form = *rng.pick(&Trunk::HEAD_FORMS);
        }
        if t.tail > 0.0 && rng.chance(0.5) {
            t.tail_form = *rng.pick(&Trunk::TAIL_FORMS);
        }
        for m in self.plan.modules.iter_mut() {
            m.size *= rng.range(0.8, 1.25);
            m.width *= rng.range(0.85, 1.2);
            m.detail = rng.f32();
            m.curl += rng.signed() * 0.5;
            m.angle += rng.signed() * 0.2;
            // Mouths and feet take the forms their work allows.
            if rng.chance(0.35) {
                m.form = match (kind, m.kind) {
                    (Founder::Grazer, Kind::Mouth) => *rng.pick(&[Form::Plates, Form::Pincer, Form::Tube, Form::Beads]),
                    (Founder::Stalker, Kind::Mouth) => *rng.pick(&[Form::Fork, Form::Pincer, Form::Hook, Form::Tube]),
                    (Founder::Swimmer, Kind::Mouth) => *rng.pick(&[Form::Tube, Form::Fork, Form::Bulb, Form::Frond]),
                    (Founder::Crawler, Kind::Mouth) => *rng.pick(&[Form::Paddle, Form::Beads, Form::Plates]),
                    (Founder::Flier, Kind::Mouth) => *rng.pick(&[Form::Hook, Form::Coil, Form::Tube]),
                    (Founder::Drifter, Kind::Mouth) => *rng.pick(&[Form::Frond, Form::Tuft]),
                    (_, Kind::Leg) => *rng.pick(&[Form::Tube, Form::Tube, Form::Hook, Form::Fork, Form::Beads]),
                    (_, Kind::Sense) if m.form == Form::Bulb => Form::Bulb,
                    (_, Kind::Sense) => *rng.pick(&[Form::Beads, Form::Fork, Form::Frond, Form::Tuft, Form::Paddle]),
                    (_, Kind::Fin) => *rng.pick(&[Form::Paddle, Form::Fan]),
                    _ => m.form,
                };
            }
        }
    }

    pub fn wild(&self, rng: &mut Rng) -> Self {
        let [a, b] = genetics::haplotypes(self);
        let mut gametes = [genetics::recombine(&a, &b, rng), genetics::recombine(&a, &b, rng)];
        for h in gametes.iter_mut() {
            for _ in 0..2 {
                h.mutate_genes(rng, self.mutation);
            }
            genetics::mutate_haplotype(h, rng, self.mutation);
        }
        let mut brain = self.brain.clone();
        brain.mutate(rng, self.mutation);
        genetics::express(gametes, brain, genetics::mother_line(&self.marker, rng), rng)
    }

    /// Offspring of one parent without sex: both of its gene sets are copied
    /// (with copying errors), so nothing is shuffled and harmful mutations
    /// can only pile up.
    pub fn bud(&self, rng: &mut Rng) -> Self {
        let rate = self.mutation;
        let mut hs = genetics::clone_haplotypes(self, rng);
        for h in hs.iter_mut() {
            genetics::mutate_haplotype(h, rng, rate);
        }
        let mut brain = self.brain.clone();
        brain.mutate(rng, rate);
        genetics::express(hs, brain, genetics::mother_line(&self.marker, rng), rng)
    }

    /// Offspring of two parents (or one fertilising itself): each gives one
    /// gamete, a set of genes shuffled from its own two by crossing over.
    pub fn mate(&self, other: &Self, rng: &mut Rng) -> Self {
        let mut eggs = genetics::gamete(self, rng);
        let mut sperm = genetics::gamete(other, rng);
        genetics::mutate_haplotype(&mut eggs, rng, self.mutation);
        genetics::mutate_haplotype(&mut sperm, rng, other.mutation);
        // The brain's wiring is inherited as one linked block from each parent.
        let mut brain = self.brain.cross(&other.brain, rng);
        brain.mutate(rng, (self.mutation + other.mutation) * 0.5);
        genetics::express([eggs, sperm], brain, genetics::mother_line(&self.marker, rng), rng)
    }

    /// Mutates the expressed genes directly, as tools and tests do; living
    /// lineages mutate through their gametes instead.
    pub fn mutate(&mut self, rng: &mut Rng, rate: f32) {
        self.brain.mutate(rng, rate.clamp(0.2, 3.0));
        self.mutate_genes(rng, rate);
    }

    /// Copying errors in everything but the brain's wiring and the mother
    /// line, as when a gamete is made. Most changes are small; a few are large.
    pub(crate) fn mutate_genes(&mut self, rng: &mut Rng, rate: f32) {
        let r = rate.clamp(0.2, 3.0);
        let nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32, rng: &mut Rng| {
            if rng.chance(0.15 * r) {
                *v = (*v + genetics::effect(rng) * sd).clamp(lo, hi);
            }
        };
        // Hues go round the colour wheel rather than stopping at an end.
        let turn = |v: &mut f32, sd: f32, rng: &mut Rng| {
            if rng.chance(0.15 * r) {
                *v = (*v + genetics::effect(rng) * sd).rem_euclid(1.0);
            }
        };
        nudge(&mut self.adult_scale, 0.0006, 0.002, 0.035, rng);
        nudge(&mut self.tempo, 0.05, 0.6, 1.6, rng);
        if rng.chance(0.15 * r) {
            self.growth_time = (self.growth_time * (1.0 + genetics::effect(rng) * 0.1)).clamp(20.0, 30000.0);
        }
        nudge(&mut self.aging, 0.06, 0.2, 3.0, rng);
        nudge(&mut self.diet, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.sexual, 0.05, 0.0, 1.0, rng);
        nudge(&mut self.clutch, 0.6, 1.0, 40.0, rng);
        nudge(&mut self.invest, 0.015, 0.01, 0.5, rng);
        nudge(&mut self.live_birth, 0.06, 0.0, 1.0, rng);
        nudge(&mut self.helpers, 0.03, 0.0, 0.95, rng);
        nudge(&mut self.matriphagy, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.heat_tolerance, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.cold_tolerance, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.toxin, 0.03, 0.0, 1.0, rng);
        nudge(&mut self.choosiness, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.imitation, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.innovation, 0.004, 0.0, 0.2, rng);
        nudge(&mut self.curiosity, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.boldness, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.sociality, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.mutation, 0.05, 0.3, 2.5, rng);
        nudge(&mut self.mhc_pref, 0.05, -1.0, 1.0, rng);
        nudge(&mut self.selfing, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.canalization, 0.04, 0.0, 1.0, rng);
        nudge(&mut self.detox, 0.03, 0.0, 1.0, rng);
        turn(&mut self.palate, 0.03, rng);
        nudge(&mut self.breadth, 0.04, 0.0, 1.0, rng);
        for i in 0..4 {
            if i == 0 || i == 3 {
                turn(&mut self.mate_pref[i], 0.05, rng);
            } else {
                nudge(&mut self.mate_pref[i], 0.05, 0.0, 1.0, rng);
            }
            nudge(&mut self.immune[i], 0.05, 0.0, 1.0, rng);
        }
        for (i, v) in self.look.iter_mut().enumerate() {
            if i == 0 || i == 3 {
                turn(v, 0.04, rng);
            } else {
                nudge(v, 0.04, 0.0, 1.0, rng);
            }
        }
        self.plan.mutate(rng, r);
    }

    /// the worse elsewhere.
    pub fn digests(&self, chem: f32) -> f32 {
        let d = (self.palate - chem).rem_euclid(1.0);
        let d = d.min(1.0 - d);
        let width = 0.06 + 0.44 * self.breadth;
        (1.0 + 0.4 * (1.0 - self.breadth)) * (-0.5 * (d / width).powi(2)).exp()
    }

    /// Tunes a founding lineage's gut, in both its gene sets, to a plant it
    /// arrives among.
    pub fn tune(&mut self, palate: f32) {
        self.palate = palate;
        if let Some(h) = self.haplotypes.as_mut() {
            for g in Arc::make_mut(h).iter_mut() {
                g.palate = palate;
            }
        }
    }

    /// How much of a dose of poison still harms it.
    pub fn felt(&self, toxin: f32) -> f32 {
        (toxin - self.detox * 0.8).max(0.0)
    }

    /// How much two genomes share their neutral markers, 0..1.
    pub fn kinship(a: &[u8; 8], b: &[u8; 8]) -> f32 {
        a.iter().zip(b).filter(|(x, y)| x == y).count() as f32 / 8.0
    }

    /// How alike two genomes look, 0 same .. 1 opposite (genes 0..3).
    pub fn look_distance(a: &[f32], b: &[f32]) -> f32 {
        let mut d = 0.0;
        for i in 0..4 {
            // Hue wraps around.
            let x = (a[i] - b[i]).abs();
            d += if i == 0 || i == 3 { x.min(1.0 - x) * 2.0 } else { x };
        }
        d / 4.0
    }
}
