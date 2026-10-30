//! Body plans. A genome does not say where every part goes; it says what
//! kind of animal to build. There is one axis, a trunk of a few segments
//! with an optional neck, head and tail, and a list of modules hung on it:
//! legs, fins, wings, mouths, eyes, horns, spines, floats and the rest. Each
//! module knows where on the axis its kind belongs and how it is attached,
//! so a mutation moves an eye along the head or lengthens a leg, but never
//! sprouts a leg from an eye. This is the Hox-like logic that gives Earth's
//! animals their legible bodies: everything is a variation on a plan, and
//! yet the plans run from worms to birds.
//!
//! Lengths are in body units: the trunk is about one unit long.

use crate::genetics;
use crate::genome::{Form, Tissue};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

pub const MAX_MODULES: usize = 12;
pub const MAX_TRUNK: u8 = 8;

/// What a module is, which fixes what it is made of, where it may sit and
/// how it hangs. Its form (see `Form`) then decides which organ it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    /// Eyes, antennae, ears and feelers on the head.
    Sense,
    /// Jaws, beaks, mandibles, fangs, grinders, sieves... at the head's tip.
    Mouth,
    /// A walking leg in two joints that reaches the ground.
    Leg,
    /// A fore limb that grips, digs or strikes rather than bears weight.
    Arm,
    /// A fin or flipper on the flank.
    Fin,
    /// A wing: a long membranous limb on the back.
    Wing,
    /// Gas bladders and sails on the back.
    Float,
    /// Spines, barbs and stingers in a row down the back.
    Spine,
    /// Horns, frills and clubs on the head.
    Horn,
    /// Light-feeding fronds on the back.
    Film,
    /// Light organs, lures and crests.
    Glow,
    /// A hump or roll of fat on the back.
    Store,
    /// Brood sacs or a pouch under the rump.
    Brood,
    /// Gills on the flank just behind the head.
    Gill,
}

impl Kind {
    pub const ALL: [Kind; 14] = [
        Kind::Sense,
        Kind::Mouth,
        Kind::Leg,
        Kind::Arm,
        Kind::Fin,
        Kind::Wing,
        Kind::Float,
        Kind::Spine,
        Kind::Horn,
        Kind::Film,
        Kind::Glow,
        Kind::Store,
        Kind::Brood,
        Kind::Gill,
    ];

    pub fn tissue(self) -> Tissue {
        match self {
            Kind::Sense => Tissue::Sense,
            Kind::Mouth => Tissue::Mouth,
            Kind::Leg | Kind::Arm | Kind::Fin | Kind::Wing => Tissue::Muscle,
            Kind::Float => Tissue::Float,
            Kind::Spine => Tissue::Spine,
            Kind::Horn => Tissue::Armor,
            Kind::Film => Tissue::LightFilm,
            Kind::Glow => Tissue::Glow,
            Kind::Store => Tissue::Store,
            Kind::Brood => Tissue::Brood,
            Kind::Gill => Tissue::Gut,
        }
    }

    /// The forms a module of this kind can take; the first is the plainest.
    pub fn forms(self) -> &'static [Form] {
        use Form::*;
        match self {
            Kind::Sense => &[Bulb, Beads, Fork, Frond, Coil, Paddle, Fan, Tuft, Tube],
            Kind::Mouth => &[Tube, Hook, Pincer, Fork, Plates, Beads, Coil, Frond, Tuft, Bulb, Paddle],
            Kind::Leg => &[Tube, Hook, Fork, Paddle, Pincer, Beads],
            Kind::Arm => &[Tube, Hook, Pincer, Fork, Coil, Paddle],
            Kind::Fin => &[Paddle, Fan],
            Kind::Wing => &[Fan, Paddle, Frond],
            Kind::Float => &[Bulb, Fan, Frond, Paddle, Tube],
            Kind::Spine => &[Tube, Hook, Fork, Coil, Frond, Tuft, Pincer],
            Kind::Horn => &[Hook, Fork, Bulb, Paddle, Fan, Tube],
            Kind::Film => &[Frond, Fan, Paddle, Tube],
            Kind::Glow => &[Bulb, Tube, Fan, Paddle, Frond],
            Kind::Store => &[Bulb, Tube],
            Kind::Brood => &[Tube, Beads, Bulb, Paddle],
            Kind::Gill => &[Frond, Tuft],
        }
    }

    /// Whether a module of this kind sits on the head rather than the trunk.
    pub fn on_head(self) -> bool {
        matches!(self, Kind::Sense | Kind::Mouth | Kind::Horn)
    }

    /// Whether it normally comes as a left-and-right pair.
    pub fn pairs(self) -> bool {
        matches!(self, Kind::Leg | Kind::Arm | Kind::Fin | Kind::Wing | Kind::Gill | Kind::Sense)
    }

    /// The stretch of the axis (0 head .. 1 rump) it may grow on.
    pub fn zone(self) -> (f32, f32) {
        match self {
            Kind::Sense | Kind::Mouth | Kind::Horn => (0.0, 1.0),
            Kind::Leg => (0.05, 0.95),
            Kind::Arm => (0.0, 0.35),
            Kind::Gill => (0.0, 0.4),
            Kind::Brood => (0.5, 1.0),
            Kind::Wing => (0.1, 0.7),
            _ => (0.0, 1.0),
        }
    }

    /// Body units of length a new module of this kind starts at.
    fn size(self) -> f32 {
        match self {
            Kind::Sense => 0.12,
            Kind::Mouth => 0.28,
            Kind::Leg => 0.6,
            Kind::Arm => 0.4,
            Kind::Fin => 0.3,
            Kind::Wing => 0.9,
            Kind::Float => 0.35,
            Kind::Spine => 0.2,
            Kind::Horn => 0.25,
            Kind::Film => 0.5,
            Kind::Glow => 0.12,
            Kind::Store => 0.3,
            Kind::Brood => 0.2,
            Kind::Gill => 0.2,
        }
    }

    /// Kinds a module of this kind can turn into: limbs into other limbs,
    /// weapons into shows.
    fn shifts(self) -> &'static [Kind] {
        match self {
            Kind::Leg => &[Kind::Fin, Kind::Arm],
            Kind::Arm => &[Kind::Leg, Kind::Fin],
            Kind::Fin => &[Kind::Leg, Kind::Wing, Kind::Arm],
            Kind::Wing => &[Kind::Fin],
            Kind::Spine => &[Kind::Horn, Kind::Glow],
            Kind::Horn => &[Kind::Spine, Kind::Glow],
            Kind::Glow => &[Kind::Spine],
            Kind::Float => &[Kind::Film, Kind::Store],
            Kind::Film => &[Kind::Float],
            Kind::Store => &[Kind::Float, Kind::Brood],
            Kind::Brood => &[Kind::Store],
            Kind::Gill => &[Kind::Fin],
            _ => &[],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Sense => "sense organ",
            Kind::Mouth => "mouth",
            Kind::Leg => "leg",
            Kind::Arm => "arm",
            Kind::Fin => "fin",
            Kind::Wing => "wing",
            Kind::Float => "float",
            Kind::Spine => "spine",
            Kind::Horn => "horn",
            Kind::Film => "light frond",
            Kind::Glow => "light organ",
            Kind::Store => "hump",
            Kind::Brood => "brood sac",
            Kind::Gill => "gill",
        }
    }
}

/// One kind of part, and where and how it grows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub kind: Kind,
    pub form: Form,
    /// Where along the axis it sits, 0 head .. 1 rump (along the head for
    /// head modules).
    pub zone: f32,
    /// Copies in a row down the axis, 1..=4, and the gap between them as a
    /// share of the trunk.
    pub count: u8,
    pub spacing: f32,
    /// Grows on both sides, seen as a near and a far one.
    pub paired: bool,
    /// Length in body units, and thickness over length.
    pub size: f32,
    pub width: f32,
    /// Set against the angle its kind hangs at, radians.
    pub angle: f32,
    /// How it bows or curls, how elaborate it is, how it narrows (see `PartGene`).
    pub curl: f32,
    pub detail: f32,
    pub taper: f32,
    /// Maturity it grows at.
    pub onset: f32,
    /// Which ancestral module it descends from, so two genomes can tell a
    /// shared gene from one that only sits in the same place.
    pub origin: u32,
    /// How strongly this copy is expressed over the other parent's, 0..255.
    pub dom: u8,
}

impl Module {
    pub fn new(kind: Kind, form: Form, zone: f32, size: f32, rng: &mut Rng) -> Self {
        Self {
            kind,
            form,
            zone,
            count: 1,
            spacing: 0.15,
            paired: kind.pairs(),
            size,
            width: match kind {
                Kind::Sense | Kind::Glow | Kind::Float | Kind::Store => 0.7,
                Kind::Mouth => 0.6,
                Kind::Brood => 0.5,
                Kind::Fin | Kind::Film | Kind::Gill => 0.3,
                Kind::Wing => 0.1,
                _ => 0.22,
            },
            angle: 0.0,
            curl: 0.0,
            detail: 0.5,
            taper: kind.tissue().taper(),
            onset: 0.0,
            origin: rng.next_u64() as u32,
            dom: 128,
        }
    }

    /// A module of a random kind in a plain form, for a new gene.
    pub fn random(rng: &mut Rng) -> Self {
        let kind = *rng.pick(&Kind::ALL);
        let form = *rng.pick(kind.forms());
        let (lo, hi) = kind.zone();
        let mut m = Self::new(kind, form, rng.range(lo, hi), kind.size() * rng.range(0.6, 1.2), rng);
        m.curl = rng.signed() * 0.4;
        m.detail = rng.f32();
        m
    }

    /// The angle its kind hangs at from its attachment, before its own.
    pub fn hang(&self) -> f32 {
        use std::f32::consts::FRAC_PI_2;
        let a = match self.kind {
            Kind::Mouth => 0.0,
            Kind::Sense => match crate::organ::Organ::of(Tissue::Sense, self.form) {
                crate::organ::Organ::Eye | crate::organ::Organ::Ocellus => 0.9,
                crate::organ::Organ::Antenna => 0.6,
                crate::organ::Organ::Ear => 1.4,
                _ => -0.4,
            },
            Kind::Horn => 1.0,
            Kind::Leg => -FRAC_PI_2 - 0.35,
            Kind::Arm => -0.9,
            Kind::Fin => -0.35,
            Kind::Gill => -0.25,
            Kind::Wing => 0.7,
            Kind::Float | Kind::Film | Kind::Spine | Kind::Store | Kind::Glow => FRAC_PI_2,
            Kind::Brood => -FRAC_PI_2,
        };
        a + self.angle
    }

    pub fn mutate(&mut self, rng: &mut Rng, r: f32) {
        let nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32, rng: &mut Rng| {
            if rng.chance(0.07 * r) {
                *v = (*v + genetics::effect(rng) * sd).clamp(lo, hi);
            }
        };
        let (lo, hi) = self.kind.zone();
        nudge(&mut self.zone, 0.05, lo, hi, rng);
        if rng.chance(0.08 * r) {
            self.size = (self.size * (1.0 + genetics::effect(rng) * 0.12)).clamp(0.03, 2.0);
        }
        if rng.chance(0.08 * r) {
            self.width = (self.width * (1.0 + genetics::effect(rng) * 0.12)).clamp(0.08, 1.4);
        }
        nudge(&mut self.angle, 0.12, -1.2, 1.2, rng);
        nudge(&mut self.curl, 0.12, -2.5, 2.5, rng);
        nudge(&mut self.detail, 0.08, 0.0, 1.0, rng);
        nudge(&mut self.taper, 0.06, 0.12, 1.5, rng);
        nudge(&mut self.spacing, 0.03, 0.05, 0.35, rng);
        if rng.chance(0.02 * r) {
            self.onset = (self.onset + rng.normal() * 0.15).clamp(0.0, 0.9);
        }
        if rng.chance(0.03 * r) {
            self.dom = (self.dom as f32 + rng.normal() * 24.0).clamp(1.0, 255.0) as u8;
        }
        if rng.chance(0.006 * r) {
            self.form = *rng.pick(self.kind.forms());
        }
        if rng.chance(0.008 * r) {
            self.count = (self.count as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(1, 4) as u8;
        }
        if rng.chance(0.004 * r) {
            self.paired = !self.paired;
        }
        if rng.chance(0.002 * r) && !self.kind.shifts().is_empty() {
            let kind = *rng.pick(self.kind.shifts());
            self.kind = kind;
            if !kind.forms().contains(&self.form) {
                self.form = kind.forms()[0];
            }
            let (lo, hi) = kind.zone();
            self.zone = self.zone.clamp(lo, hi);
        }
    }

    /// How far apart two modules of the same descent have drifted.
    pub fn distance(&self, o: &Self) -> f32 {
        let mut d = 0.0;
        if self.kind != o.kind {
            d += 1.5;
        }
        if self.form != o.form {
            d += 0.8;
        }
        d += (self.zone - o.zone).abs() * 1.0
            + (self.size - o.size).abs() * 0.8
            + (self.width - o.width).abs() * 0.5
            + (self.angle - o.angle).abs() * 0.2
            + (self.curl - o.curl).abs() * 0.15
            + (self.detail - o.detail).abs() * 0.3
            + (self.count as f32 - o.count as f32).abs() * 0.4
            + (self.paired != o.paired) as u8 as f32 * 0.5;
        d
    }
}

/// The axis everything hangs on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trunk {
    /// Segments along it, 1..=MAX_TRUNK.
    pub segments: u8,
    /// Length in body units (about 1), and thickness over length.
    pub length: f32,
    pub girth: f32,
    /// Thickness at the front, middle and rear, relative.
    pub profile: [f32; 3],
    /// How the back arches, radians end to end.
    pub arch: f32,
    /// Its skin: what it is made of and how it is textured. Frame is plain
    /// hide; with plates it is scaled, with tufts furred; armour is plating
    /// or a shell; float is a gas-filled body; gut a soft body.
    pub tissue: Tissue,
    pub form: Form,
    pub detail: f32,
    /// Head length over trunk length (0 is none) and its shape.
    pub head: f32,
    pub head_width: f32,
    pub head_form: Form,
    /// Neck length over trunk length (0 is none), in how many pieces, and
    /// how high it is carried.
    pub neck: f32,
    pub neck_segments: u8,
    pub neck_angle: f32,
    /// Tail length over trunk length (0 is none), its thickness, pieces,
    /// what it is made of and how its tip is shaped.
    pub tail: f32,
    pub tail_width: f32,
    pub tail_segments: u8,
    pub tail_tissue: Tissue,
    pub tail_form: Form,
    pub tail_curl: f32,
    /// What fills the trunk: gut, fat and brood, each as a share of its bulk.
    pub gut: f32,
    pub fat: f32,
    pub brood: f32,
    pub dom: u8,
}

impl Trunk {
    pub const TISSUES: [Tissue; 4] = [Tissue::Frame, Tissue::Armor, Tissue::Float, Tissue::Gut];
    pub const FORMS: [Form; 6] = [Form::Tube, Form::Beads, Form::Plates, Form::Tuft, Form::Frond, Form::Paddle];
    pub const ARMOR_FORMS: [Form; 4] = [Form::Tube, Form::Plates, Form::Coil, Form::Beads];
    pub const HEAD_FORMS: [Form; 5] = [Form::Tube, Form::Bulb, Form::Paddle, Form::Plates, Form::Beads];
    pub const TAIL_TISSUES: [Tissue; 5] = [Tissue::Muscle, Tissue::Frame, Tissue::Armor, Tissue::Spine, Tissue::Store];
    pub const TAIL_FORMS: [Form; 7] =
        [Form::Tube, Form::Fan, Form::Paddle, Form::Bulb, Form::Tuft, Form::Coil, Form::Beads];

    pub fn plain() -> Self {
        Self {
            segments: 3,
            length: 1.0,
            girth: 0.3,
            profile: [0.9, 1.0, 0.85],
            arch: 0.0,
            tissue: Tissue::Frame,
            form: Form::Tube,
            detail: 0.5,
            head: 0.25,
            head_width: 0.7,
            head_form: Form::Tube,
            neck: 0.0,
            neck_segments: 1,
            neck_angle: 0.3,
            tail: 0.0,
            tail_width: 0.3,
            tail_segments: 2,
            tail_tissue: Tissue::Muscle,
            tail_form: Form::Tube,
            tail_curl: 0.0,
            gut: 0.3,
            fat: 0.05,
            brood: 0.08,
            dom: 128,
        }
    }

    fn skin_forms(tissue: Tissue) -> &'static [Form] {
        match tissue {
            Tissue::Armor => &Self::ARMOR_FORMS,
            Tissue::Float | Tissue::Gut => &[Form::Tube, Form::Beads, Form::Bulb],
            _ => &Self::FORMS,
        }
    }

    pub fn mutate(&mut self, rng: &mut Rng, r: f32) {
        let nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32, rng: &mut Rng| {
            if rng.chance(0.07 * r) {
                *v = (*v + genetics::effect(rng) * sd).clamp(lo, hi);
            }
        };
        if rng.chance(0.08 * r) {
            self.length = (self.length * (1.0 + genetics::effect(rng) * 0.1)).clamp(0.4, 2.5);
        }
        nudge(&mut self.girth, 0.03, 0.08, 0.9, rng);
        for p in self.profile.iter_mut() {
            nudge(p, 0.08, 0.3, 1.8, rng);
        }
        nudge(&mut self.arch, 0.1, -1.2, 1.2, rng);
        nudge(&mut self.detail, 0.08, 0.0, 1.0, rng);
        nudge(&mut self.head, 0.03, 0.0, 0.8, rng);
        nudge(&mut self.head_width, 0.06, 0.2, 1.5, rng);
        nudge(&mut self.neck, 0.04, 0.0, 1.2, rng);
        nudge(&mut self.neck_angle, 0.1, -0.6, 1.3, rng);
        nudge(&mut self.tail, 0.06, 0.0, 2.5, rng);
        nudge(&mut self.tail_width, 0.04, 0.08, 0.9, rng);
        nudge(&mut self.tail_curl, 0.12, -2.0, 2.0, rng);
        nudge(&mut self.gut, 0.04, 0.02, 0.9, rng);
        nudge(&mut self.fat, 0.03, 0.0, 0.8, rng);
        nudge(&mut self.brood, 0.03, 0.0, 0.5, rng);
        if rng.chance(0.03 * r) {
            self.dom = (self.dom as f32 + rng.normal() * 24.0).clamp(1.0, 255.0) as u8;
        }
        let step = |v: &mut u8, lo: u8, hi: u8, rng: &mut Rng| {
            if rng.chance(0.01 * r) {
                *v = (*v as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(lo as i32, hi as i32) as u8;
            }
        };
        step(&mut self.segments, 1, MAX_TRUNK, rng);
        step(&mut self.neck_segments, 1, 3, rng);
        step(&mut self.tail_segments, 1, 4, rng);
        if rng.chance(0.004 * r) {
            self.tissue = *rng.pick(&Self::TISSUES);
            if !Self::skin_forms(self.tissue).contains(&self.form) {
                self.form = Form::Tube;
            }
        }
        if rng.chance(0.005 * r) {
            self.form = *rng.pick(Self::skin_forms(self.tissue));
        }
        if rng.chance(0.004 * r) {
            self.head_form = *rng.pick(&Self::HEAD_FORMS);
        }
        if rng.chance(0.004 * r) {
            self.tail_tissue = *rng.pick(&Self::TAIL_TISSUES);
        }
        if rng.chance(0.005 * r) {
            self.tail_form = *rng.pick(&Self::TAIL_FORMS);
        }
    }

    pub fn distance(&self, o: &Self) -> f32 {
        let mut d = 0.0;
        if self.tissue != o.tissue {
            d += 1.5;
        }
        if self.form != o.form {
            d += 0.8;
        }
        if self.head_form != o.head_form {
            d += 0.5;
        }
        if self.tail_tissue != o.tail_tissue {
            d += 0.6;
        }
        if self.tail_form != o.tail_form {
            d += 0.5;
        }
        d += (self.segments as f32 - o.segments as f32).abs() * 0.3
            + (self.length - o.length).abs() * 0.8
            + (self.girth - o.girth).abs() * 2.0
            + (self.head - o.head).abs() * 1.5
            + (self.neck - o.neck).abs() * 1.2
            + (self.tail - o.tail).abs() * 0.8
            + (self.arch - o.arch).abs() * 0.3
            + (self.gut - o.gut).abs() * 0.8
            + (self.fat - o.fat).abs() * 0.8;
        d
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyPlan {
    pub trunk: Trunk,
    pub modules: Vec<Module>,
}

impl BodyPlan {
    pub fn mutate(&mut self, rng: &mut Rng, r: f32) {
        self.trunk.mutate(rng, r);
        for m in self.modules.iter_mut() {
            m.mutate(rng, r);
        }
        // Duplication is the classic road to new organs: the copy starts as
        // a twin of its source a little further down the axis, and drifts
        // from there. Loss is as likely, so bodies don't pile up parts for
        // no reason but chance.
        if rng.chance(0.008 * r) && !self.modules.is_empty() && self.modules.len() < MAX_MODULES {
            let src = rng.below(self.modules.len());
            let mut copy = self.modules[src].clone();
            copy.origin = rng.next_u64() as u32;
            let (lo, hi) = copy.kind.zone();
            copy.zone = (copy.zone + rng.signed() * 0.25).clamp(lo, hi);
            self.modules.push(copy);
        }
        if rng.chance(0.008 * r) && self.modules.len() > 1 {
            let i = rng.below(self.modules.len());
            self.modules.remove(i);
        }
        if rng.chance(0.004 * r) && self.modules.len() < MAX_MODULES {
            self.modules.push(Module::random(rng));
        }
    }

    /// Modules of a kind, in order.
    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Module> {
        self.modules.iter().filter(move |m| m.kind == kind)
    }

    pub fn has(&self, kind: Kind) -> bool {
        self.modules.iter().any(|m| m.kind == kind)
    }

    /// How far apart two plans are as bodies.
    pub fn distance(&self, o: &Self) -> f32 {
        let mut d = self.trunk.distance(&o.trunk);
        let mut matched = vec![false; o.modules.len()];
        for m in &self.modules {
            match o.modules.iter().position(|q| q.origin == m.origin) {
                Some(j) => {
                    matched[j] = true;
                    d += m.distance(&o.modules[j]);
                }
                None => d += 1.5,
            }
        }
        d += matched.iter().filter(|&&m| !m).count() as f32 * 1.5;
        d
    }
}
