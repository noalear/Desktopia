//! Creature genomes. A genome is a growth program, not a blueprint: a small
//! graph of part genes that development unrolls into a body, plus the starting
//! wiring of a brain and the genes for life history, looks and immunity.

use crate::brain::{NI, NO};
use crate::genetics::{self, COMPAT};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const LOOK_GENES: usize = 16;
pub const MAX_PARTS: usize = 10;
pub const MAX_LINKS: usize = 4;
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkGene {
    /// Which part gene grows here.
    pub to: u8,
    /// Angle from the parent's direction, radians.
    pub angle: f32,
    /// Where along the parent it attaches, 0 base .. 1 tip.
    pub along: f32,
    /// Size relative to the parent.
    pub scale: f32,
    /// Grows as a near and far pair, like legs seen from the side.
    pub pair: bool,
    /// Copies fanned out from the same spot (1..=3).
    pub repeat: u8,
    /// Maturity 0..1 at which it grows, so young can look unlike adults.
    pub onset: f32,
    /// Only grows from the last repetition of a recursive part.
    pub tip_only: bool,
    /// A web of membrane spans between this part and its parent.
    pub membrane: bool,
    /// How the joint here is driven.
    pub motor: Motor,
}

/// A joint's drive: each joint swings on the body's rhythm, at its own
/// timing, so gaits, strokes and waves are all heritable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Motor {
    /// Swing either side of rest at full drive, radians.
    pub amp: f32,
    /// Timing after the joint it grows from, cycles. Repeated down a chain,
    /// this makes a travelling wave.
    pub phase: f32,
    /// Timing added for the far one of a pair, cycles.
    pub pair: f32,
    /// How far it bends when the brain wants to rise, radians.
    pub lift: f32,
}

impl Motor {
    fn random(rng: &mut Rng) -> Self {
        Self {
            amp: if rng.chance(0.5) { rng.range(0.0, 0.6) } else { 0.0 },
            phase: rng.f32(),
            pair: if rng.chance(0.6) { 0.5 } else { rng.f32() },
            lift: rng.signed() * 0.3,
        }
    }

    fn mutate(&mut self, rng: &mut Rng, r: f32) {
        if rng.chance(0.08 * r) {
            self.amp = (self.amp + rng.normal() * 0.1).clamp(0.0, 1.2);
        }
        if rng.chance(0.08 * r) {
            self.phase = (self.phase + rng.normal() * 0.08).rem_euclid(1.0);
        }
        if rng.chance(0.04 * r) {
            self.pair = (self.pair + rng.normal() * 0.08).rem_euclid(1.0);
        }
        if rng.chance(0.05 * r) {
            self.lift = (self.lift + rng.normal() * 0.08).clamp(-1.0, 1.0);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartGene {
    pub tissue: Tissue,
    /// Length in body units.
    pub length: f32,
    /// Thickness relative to length.
    pub width: f32,
    /// How many times it may repeat into itself, making chains like tails and trunks.
    pub recursion: u8,
    pub links: Vec<LinkGene>,
    /// Which ancestral gene this one descends from. Copies of a part made by
    /// duplication get a new origin, so two genomes can tell a shared
    /// (homologous) gene from one that only sits in the same place.
    pub origin: u32,
    /// How strongly this copy is expressed over the other parent's copy,
    /// 0..255; 128 is equal (codominant).
    pub dom: u8,
    /// Tip thickness over base thickness: below 1 it narrows to a point or
    /// a tail, above 1 it swells into a club or bulb.
    pub taper: f32,
    /// The shape it grows into.
    pub form: Form,
    /// How it bows along its length, radians end to end; for hooks and
    /// coils, which way and how hard the tip curls.
    pub curl: f32,
    /// How elaborate its form is, 0..1: how many beads, barbs, rays, plates
    /// or hairs, how wide a blade, how big a bulb.
    pub detail: f32,
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
    pub parts: Vec<PartGene>,
    pub brain: BrainGenes,
    /// World units per body unit when fully grown.
    pub adult_scale: f32,
    /// The body's rhythm at full drive, cycles per second.
    pub gait_hz: f32,
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

impl LinkGene {
    fn random(rng: &mut Rng, parts: usize) -> Self {
        Self {
            to: rng.below(parts) as u8,
            angle: rng.signed() * 2.4,
            along: rng.range(0.3, 1.0),
            scale: rng.range(0.4, 1.0),
            pair: rng.chance(0.5),
            repeat: if rng.chance(0.8) { 1 } else { 2 + rng.below(2) as u8 },
            onset: if rng.chance(0.7) { 0.0 } else { rng.range(0.0, 0.8) },
            tip_only: rng.chance(0.2),
            membrane: rng.chance(0.15),
            motor: Motor::random(rng),
        }
    }
}

impl PartGene {
    fn random(rng: &mut Rng, parts: usize) -> Self {
        let tissue = *rng.pick(&Tissue::ALL);
        let mut p = Self {
            tissue,
            length: rng.range(0.4, 1.6),
            width: rng.range(0.15, 0.7),
            recursion: if rng.chance(0.7) { 0 } else { 1 + rng.below(4) as u8 },
            links: Vec::new(),
            origin: rng.next_u64() as u32,
            dom: 128,
            taper: tissue.taper(),
            form: Form::random(rng),
            curl: rng.signed() * 0.6,
            detail: rng.f32(),
        };
        for _ in 0..rng.below(3) {
            p.links.push(LinkGene::random(rng, parts));
        }
        p
    }
}

/// A body plan the founders start from, so the first creatures can feed and move.
#[derive(Clone, Copy, Debug)]
pub enum Founder {
    /// Small floating light-feeder.
    Drifter,
    /// Ground grazer with legs and a mouth.
    Grazer,
    /// Swimming filter-feeder with a tail.
    Swimmer,
    /// Slow crawling detritus feeder.
    Crawler,
}

impl Genome {
    /// A founder genome: a working body plan with random details and a random brain.
    /// A founder exactly as designed, before the variety every founder gets.
    pub fn founder_plan(kind: Founder, rng: &mut Rng) -> Self {
        use Tissue::*;
        let link = |to: u8, angle: f32, along: f32, scale: f32, pair: bool| LinkGene {
            to,
            angle,
            along,
            scale,
            pair,
            repeat: 1,
            onset: 0.0,
            tip_only: false,
            membrane: false,
            motor: Motor::default(),
        };
        // A driven joint: swing, timing, and the far one of a pair's offset.
        let drive = |l: LinkGene, amp: f32, phase: f32, pair: f32| LinkGene {
            motor: Motor { amp, phase, pair, lift: 0.0 },
            ..l
        };
        let part = |tissue, length, width, recursion, links| PartGene {
            tissue,
            length,
            width,
            recursion,
            links,
            origin: 0,
            dom: 128,
            taper: Tissue::taper(tissue),
            form: Form::Tube,
            curl: 0.0,
            detail: 0.5,
        };
        let (parts, scale, diet, hidden) = match kind {
            Founder::Drifter => (
                vec![
                    part(
                        Float,
                        1.0,
                        0.8,
                        0,
                        vec![link(1, 0.0, 1.0, 0.6, false), drive(link(2, 2.3, 0.6, 0.7, true), 0.5, 0.0, 0.0)],
                    ),
                    part(Gut, 0.8, 0.6, 0, vec![link(3, 0.3, 1.0, 0.5, false)]),
                    part(LightFilm, 1.2, 0.2, 0, vec![]),
                    part(Sense, 0.5, 0.4, 0, vec![]),
                ],
                0.0045,
                0.05,
                4,
            ),
            Founder::Grazer => (
                vec![
                    part(
                        Frame,
                        1.4,
                        0.45,
                        0,
                        vec![
                            link(1, 0.0, 1.0, 0.5, false),
                            drive(link(2, -1.7, 0.25, 0.75, true), 1.2, 0.83, 0.14),
                            drive(link(2, -1.25, 0.8, 0.75, true), 0.5, 0.35, 0.5),
                            link(3, 3.1, 0.0, 0.7, false),
                        ],
                    ),
                    part(Mouth, 0.6, 0.5, 0, vec![link(4, 0.6, 0.3, 0.6, false)]),
                    part(Muscle, 1.0, 0.2, 1, vec![drive(link(2, 0.85, 1.0, 0.8, false), 0.7, 0.26, 0.5)]),
                    part(Gut, 0.9, 0.7, 0, vec![]),
                    part(Sense, 0.5, 0.4, 0, vec![]),
                ],
                0.009,
                0.15,
                8,
            ),
            Founder::Swimmer => (
                vec![
                    part(
                        Float,
                        1.1,
                        0.5,
                        0,
                        vec![
                            drive(link(1, 3.05, 0.0, 0.8, false), 0.9, 0.5, 0.0),
                            link(2, 0.0, 1.0, 0.6, false),
                            link(4, -2.2, 0.4, 0.6, true),
                        ],
                    ),
                    part(Muscle, 0.8, 0.35, 3, vec![drive(link(1, -0.1, 1.0, 0.8, false), 1.0, 0.8, 0.0)]),
                    part(Mouth, 0.5, 0.5, 0, vec![link(3, 0.4, 0.2, 0.7, false)]),
                    part(Sense, 0.5, 0.4, 0, vec![]),
                    part(Gut, 0.7, 0.5, 0, vec![]),
                ],
                0.006,
                0.25,
                6,
            ),
            Founder::Crawler => (
                vec![
                    part(
                        Gut,
                        1.0,
                        0.6,
                        0,
                        vec![
                            drive(link(1, 2.8, 0.0, 0.9, false), 0.3, 0.06, 0.0),
                            link(2, 0.0, 1.0, 0.6, false),
                            drive(link(3, -1.3, 0.5, 0.5, true), 1.05, 0.68, 0.75),
                        ],
                    ),
                    part(
                        Armor,
                        0.9,
                        0.55,
                        3,
                        vec![
                            drive(link(1, 0.2, 1.0, 0.9, false), 1.2, 0.18, 0.0),
                            drive(link(3, -1.55, 0.5, 0.5, true), 0.8, 0.37, 0.9),
                        ],
                    ),
                    part(Mouth, 0.5, 0.6, 0, vec![link(4, 0.0, 1.0, 0.6, false)]),
                    part(Muscle, 0.5, 0.25, 1, vec![drive(link(3, 0.45, 1.0, 0.8, false), 0.35, 0.6, 0.0)]),
                    part(Sense, 0.4, 0.3, 0, vec![]),
                ],
                0.006,
                0.2,
                4,
            ),
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
            parts,
            brain: BrainGenes::random(rng, hidden),
            adult_scale: scale * rng.range(0.85, 1.2),
            gait_hz: rng.range(1.2, 2.5),
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
        for p in g.parts.iter_mut() {
            p.origin = rng.next_u64() as u32;
        }
        g.mate_pref.copy_from_slice(&g.look[..4]);
        // Each plan's gait was tuned at about this rhythm.
        g.gait_hz = match kind {
            Founder::Drifter => g.gait_hz,
            Founder::Grazer => rng.range(3.2, 4.0),
            Founder::Crawler => rng.range(2.3, 2.9),
            Founder::Swimmer => rng.range(2.6, 3.2),
        };
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
        let mut g = Self::founder_plan(kind, rng);
        g.dress(kind);
        // Founders are already a little varied.
        for _ in 0..3 + rounds {
            g.mutate(rng, 1.0);
        }
        genetics::found(g, rng)
    }

    /// Gives a founder's parts forms of their own, from those that suit each
    /// part's job in its plan, so the first lineages differ in shape as well
    /// as in size and colour. The choice is drawn from the founder's own
    /// mother line rather than the world's random stream, so every founder
    /// is otherwise built exactly as before.
    fn dress(&mut self, kind: Founder) {
        let rng = &mut Rng::new(u64::from_le_bytes(self.marker) ^ 0xD8E5_5F0E);
        use Form::*;
        let choices: &[&[Form]] = match kind {
            Founder::Drifter => {
                &[&[Tube, Bulb, Beads], &[Tube], &[Tube, Fan, Frond, Paddle], &[Tube, Bulb, Frond, Fork]]
            }
            Founder::Grazer => &[
                &[Tube, Plates, Tuft],
                &[Tube, Hook, Pincer],
                &[Tube, Tube, Beads, Hook],
                &[Tube, Bulb, Tuft],
                &[Tube, Bulb, Fork, Frond, Coil],
            ],
            Founder::Swimmer => &[
                &[Tube, Plates, Beads],
                &[Tube, Beads, Plates],
                &[Tube, Hook, Pincer, Coil],
                &[Bulb, Frond, Fork, Tube],
                &[Tube, Bulb, Frond],
            ],
            Founder::Crawler => &[
                &[Tube, Beads, Tuft],
                &[Tube, Plates, Plates, Beads],
                &[Tube, Pincer, Hook],
                &[Tube, Hook, Beads],
                &[Tube, Fork, Frond, Bulb, Coil],
            ],
        };
        for (p, forms) in self.parts.iter_mut().zip(choices) {
            p.form = *rng.pick(forms);
            p.detail = rng.f32();
            if p.form != Tube || rng.chance(0.3) {
                p.curl = rng.signed() * 0.8;
            }
        }
    }

    /// One member of a founding population: drawn from the founder's genes
    /// with the variety a wild population carries, so the first generations
    /// have something for selection to work on.
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
        nudge(&mut self.gait_hz, 0.15, 0.2, 6.0, rng);
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
        self.mutate_body(rng, r);
    }

    fn mutate_body(&mut self, rng: &mut Rng, r: f32) {
        let n = self.parts.len();
        for p in &mut self.parts {
            if rng.chance(0.08 * r) {
                p.length = (p.length * (1.0 + genetics::effect(rng) * 0.12)).clamp(0.2, 3.0);
            }
            if rng.chance(0.08 * r) {
                p.width = (p.width * (1.0 + genetics::effect(rng) * 0.12)).clamp(0.08, 1.5);
            }
            if rng.chance(0.03 * r) {
                p.dom = (p.dom as f32 + rng.normal() * 24.0).clamp(1.0, 255.0) as u8;
            }
            if rng.chance(0.05 * r) {
                p.taper = (p.taper + genetics::effect(rng) * 0.06).clamp(0.12, 1.5);
            }
            if rng.chance(0.01 * r) {
                p.tissue = *rng.pick(&Tissue::ALL);
            }
            if rng.chance(0.006 * r) {
                p.form = Form::random(rng);
            }
            if rng.chance(0.06 * r) {
                p.curl = (p.curl + genetics::effect(rng) * 0.12).clamp(-2.5, 2.5);
            }
            if rng.chance(0.06 * r) {
                p.detail = (p.detail + genetics::effect(rng) * 0.08).clamp(0.0, 1.0);
            }
            if rng.chance(0.01 * r) {
                p.recursion = (p.recursion as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(0, 5) as u8;
            }
            for l in &mut p.links {
                if rng.chance(0.06 * r) {
                    l.angle += genetics::effect(rng) * 0.2;
                }
                if rng.chance(0.04 * r) {
                    l.along = (l.along + rng.normal() * 0.1).clamp(0.0, 1.0);
                }
                if rng.chance(0.04 * r) {
                    l.scale = (l.scale + rng.normal() * 0.08).clamp(0.2, 1.3);
                }
                if rng.chance(0.01 * r) {
                    l.pair = !l.pair;
                }
                if rng.chance(0.01 * r) {
                    l.repeat = (l.repeat as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(1, 3) as u8;
                }
                if rng.chance(0.02 * r) {
                    l.onset = (l.onset + rng.normal() * 0.15).clamp(0.0, 0.9);
                }
                if rng.chance(0.01 * r) {
                    l.membrane = !l.membrane;
                }
                if rng.chance(0.01 * r) {
                    l.tip_only = !l.tip_only;
                }
                if rng.chance(0.008 * r) {
                    l.to = rng.below(n) as u8;
                }
                l.motor.mutate(rng, r);
            }
            // Losing a link is as likely as gaining one, so bodies don't
            // grow ever more complex for no reason but chance.
            if rng.chance(0.012 * r) && !p.links.is_empty() {
                let i = rng.below(p.links.len());
                p.links.remove(i);
            }
            if rng.chance(0.012 * r) && p.links.len() < MAX_LINKS {
                p.links.push(LinkGene::random(rng, n));
            }
        }
        // Duplicate a part: the classic route to new organs. The copy starts
        // out as a twin of its source, links and all (serial homology), and
        // drifts or takes on a new tissue from there. When the genome is full,
        // it lands on a gene nothing grows any more.
        if rng.chance(0.01 * r) {
            let slot = if n < MAX_PARTS { Some(n) } else { (1..n).find(|&i| !self.grows(i)) };
            if let Some(slot) = slot {
                let src = rng.below(n);
                let mut copy = self.parts[src].clone();
                copy.origin = rng.next_u64() as u32;
                if rng.chance(0.25) {
                    copy.tissue = *rng.pick(&Tissue::ALL);
                }
                if slot == n {
                    self.parts.push(copy);
                } else {
                    self.parts[slot] = copy;
                }
                let host = rng.below(n);
                if self.parts[host].links.len() < MAX_LINKS {
                    let mut l = LinkGene::random(rng, self.parts.len());
                    l.to = slot as u8;
                    self.parts[host].links.push(l);
                }
            }
        }
        // Brand new part.
        if rng.chance(0.004 * r) && self.parts.len() < MAX_PARTS {
            let len = self.parts.len();
            self.parts.push(PartGene::random(rng, len + 1));
            let host = rng.below(len);
            if self.parts[host].links.len() < MAX_LINKS {
                let mut l = LinkGene::random(rng, len + 1);
                l.to = len as u8;
                self.parts[host].links.push(l);
            }
        }
    }

    /// Whether any link still grows part `i`; the others are silent genes
    /// that drift freely until a new link wakes them.
    pub fn grows(&self, i: usize) -> bool {
        i == 0 || self.parts.iter().any(|p| p.links.iter().any(|l| l.to as usize % self.parts.len() == i))
    }

    /// How well its gut gets the good out of a plant of hue `chem`: best for
    /// what it is tuned to, and the narrower the tuning the better there and
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
