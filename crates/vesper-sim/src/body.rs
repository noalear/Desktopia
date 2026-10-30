//! Development: unrolling a genome's body plan into a body of linked
//! segments, and reading what that body is made of.

use crate::genome::{Form, Genome, Kind, Module, Tissue, Trunk};
use crate::organ::{Kit, Organ};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const MAX_SEGMENTS: usize = 48;
const MIN_LENGTH: f32 = 0.02;

/// What a segment is for, so the poser and the reader of bodies need not
/// guess from where it points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    #[default]
    Trunk,
    Neck,
    Head,
    Tail,
    Leg,
    Arm,
    Fin,
    Wing,
    Float,
    Spine,
    Horn,
    Film,
    Glow,
    Store,
    Brood,
    Gill,
    Sense,
    Mouth,
}

impl Role {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::Sense => Role::Sense,
            Kind::Mouth => Role::Mouth,
            Kind::Leg => Role::Leg,
            Kind::Arm => Role::Arm,
            Kind::Fin => Role::Fin,
            Kind::Wing => Role::Wing,
            Kind::Float => Role::Float,
            Kind::Spine => Role::Spine,
            Kind::Horn => Role::Horn,
            Kind::Film => Role::Film,
            Kind::Glow => Role::Glow,
            Kind::Store => Role::Store,
            Kind::Brood => Role::Brood,
            Kind::Gill => Role::Gill,
        }
    }

    /// Part of the axis itself rather than hung on it.
    pub fn axial(self) -> bool {
        matches!(self, Role::Trunk | Role::Neck | Role::Head | Role::Tail)
    }
}

/// One segment of a body, in body units. The trunk points forward along
/// +x; +y is up. Bodies are seen from the side, so a pair of limbs becomes a
/// near segment (side +1) and a far one (side -1) at the same place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub parent: i16,
    pub tissue: Tissue,
    pub length: f32,
    /// Thickness, body units.
    pub width: f32,
    /// Rest angle from the parent's direction.
    pub angle: f32,
    /// Where on the parent it attaches, 0 base .. 1 tip.
    pub along: f32,
    pub side: i8,
    /// Maturity at which it has grown.
    pub onset: f32,
    /// A membrane spans between this segment and its parent.
    pub membrane: bool,
    /// Links from the root.
    pub depth: u8,
    /// Rest pose, body units, centred on the body's centre of mass.
    pub base: [f32; 2],
    pub tip: [f32; 2],
    /// Rest direction, radians from +x.
    pub dir: f32,
    /// Tip thickness over base thickness.
    pub taper: f32,
    /// The shape it is drawn and works as, how it bows or curls, and how
    /// elaborate it is (see `Module`).
    pub form: Form,
    pub curl: f32,
    pub detail: f32,
    pub role: Role,
    /// For legs and arms, which joint from the body: 0 upper, 1 lower.
    pub joint: u8,
    /// Which copy down the axis it is (trunk piece, tail piece, leg pair...).
    pub index: u8,
}

impl Segment {
    /// Its size as matter is counted: length by thickness, more for forms
    /// with more to them.
    pub fn area(&self) -> f32 {
        self.length * self.width * self.form.bulk()
    }

    /// Whether it is one of the pieces the axis is made of.
    pub fn axial(&self) -> bool {
        self.role.axial()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub segments: Vec<Segment>,
    /// Nose-to-tail extent of the full-grown body, body units.
    pub extent: f32,
    /// What fills the trunk, as areas in body units squared: gut, fat and
    /// brood. Not drawn, but counted and paid for.
    pub innards: [f32; 3],
    /// How high the trunk's underside stands off the ground at rest when it
    /// is carried on legs, body units; 0 when it lies on the ground.
    pub stance: f32,
}

/// A placeholder body, replaced as soon as a loaded world regrows its bodies.
pub fn empty() -> std::sync::Arc<Body> {
    std::sync::Arc::new(Body { segments: Vec::new(), extent: 0.3, innards: [0.0; 3], stance: 0.0 })
}

fn profile(t: &Trunk, u: f32) -> f32 {
    // u 0 front .. 1 rear, through the three stations.
    let [f, m, r] = t.profile;
    if u < 0.5 { f + (m - f) * (u * 2.0) } else { m + (r - m) * ((u - 0.5) * 2.0) }
}

struct Builder<'a> {
    g: &'a Genome,
    segs: Vec<Segment>,
    /// Trunk segment indices, rear to front.
    trunk: Vec<usize>,
    head: Option<usize>,
}

impl Builder<'_> {
    fn room(&self, n: usize) -> bool {
        self.segs.len() + n <= MAX_SEGMENTS
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        parent: i16,
        tissue: Tissue,
        length: f32,
        width: f32,
        angle: f32,
        along: f32,
        side: i8,
        form: Form,
        role: Role,
    ) -> usize {
        let depth = if parent >= 0 { self.segs[parent as usize].depth + 1 } else { 0 };
        self.segs.push(Segment {
            parent,
            tissue,
            length: length.max(MIN_LENGTH),
            width: width.max(0.02),
            angle,
            along,
            side,
            onset: 0.0,
            membrane: false,
            depth,
            base: [0.0; 2],
            tip: [0.0; 2],
            dir: 0.0,
            taper: tissue.taper(),
            form,
            curl: 0.0,
            detail: 0.5,
            role,
            joint: 0,
            index: 0,
        });
        self.segs.len() - 1
    }

    fn axis(&mut self) {
        let t = &self.g.plan.trunk;
        let n = t.segments.clamp(1, crate::plan::MAX_TRUNK) as usize;
        let seg_len = t.length / n as f32;
        let fat = 1.0 + 0.5 * t.fat;
        let width_at = |i: usize| t.girth * t.length * profile(t, 1.0 - (i as f32 + 0.5) / n as f32) * fat;
        for i in 0..n {
            let parent = if i == 0 { -1 } else { self.trunk[i - 1] as i16 };
            let angle = if i == 0 { -t.arch * 0.5 } else { t.arch / (n - 1).max(1) as f32 };
            let s = self.push(parent, t.tissue, seg_len, width_at(i), angle, 1.0, 0, t.form, Role::Trunk);
            let next = if i + 1 < n { width_at(i + 1) } else { width_at(i) * 0.8 };
            self.segs[s].taper = (next / width_at(i)).clamp(0.4, 1.6);
            self.segs[s].detail = t.detail;
            self.segs[s].index = i as u8;
            self.trunk.push(s);
        }
        let front = *self.trunk.last().unwrap();
        let front_w = self.segs[front].width;
        // Neck, carried up, levelling off toward the head.
        let mut end = front;
        let mut lift = 0.0;
        if t.neck > 0.03 {
            let k = t.neck_segments.clamp(1, 3) as usize;
            if self.room(k) {
                let len = t.neck * t.length / k as f32;
                for j in 0..k {
                    let angle = if j == 0 { t.neck_angle } else { -t.neck_angle * 0.35 / (k - 1) as f32 };
                    lift += angle;
                    let w = front_w * (0.6 - 0.1 * j as f32 / k as f32);
                    let s = self.push(end as i16, Tissue::Frame, len, w, angle, 1.0, 0, Form::Tube, Role::Neck);
                    self.segs[s].taper = 0.85;
                    self.segs[s].index = j as u8;
                    end = s;
                }
            }
        }
        if t.head > 0.03 && self.room(1) {
            let len = t.head * t.length;
            let tissue = if t.tissue == Tissue::Armor { Tissue::Armor } else { Tissue::Frame };
            let s =
                self.push(end as i16, tissue, len, t.head_width * len, -lift * 0.8, 1.0, 0, t.head_form, Role::Head);
            self.segs[s].taper = match t.head_form {
                Form::Bulb => 0.9,
                _ => 0.6,
            };
            self.segs[s].detail = t.detail;
            self.head = Some(s);
        }
        if t.tail > 0.03 {
            let k = t.tail_segments.clamp(1, 4) as usize;
            if self.room(k) {
                let total = t.tail * t.length;
                let rear = self.trunk[0];
                let base_w = self.segs[rear].width * t.tail_width.clamp(0.08, 1.0);
                let mut parent = rear as i16;
                for j in 0..k {
                    let angle = if j == 0 { PI - 0.15 } else { t.tail_curl / k as f32 };
                    let along = if j == 0 { 0.0 } else { 1.0 };
                    let w = base_w * (1.0 - 0.55 * j as f32 / k as f32);
                    let form = if j + 1 == k {
                        t.tail_form
                    } else if t.tail_form == Form::Beads {
                        Form::Beads
                    } else {
                        Form::Tube
                    };
                    let s = self.push(parent, t.tail_tissue, total / k as f32, w, angle, along, 0, form, Role::Tail);
                    self.segs[s].taper = if j + 1 == k { t.tail_tissue.taper() } else { 0.85 };
                    self.segs[s].curl = if j + 1 == k { t.tail_curl * 0.5 } else { 0.0 };
                    self.segs[s].index = j as u8;
                    parent = s as i16;
                }
            }
        }
    }

    /// Where on the axis a module at `zone` attaches: a segment and a point along it.
    fn attach(&self, m: &Module, zone: f32) -> (usize, f32) {
        if m.kind.on_head() || (m.kind == Kind::Glow && m.form == Form::Bulb) {
            if let Some(h) = self.head {
                return (h, zone.clamp(0.05, 1.0));
            }
            let front = *self.trunk.last().unwrap();
            return (front, 0.5 + 0.5 * zone.clamp(0.0, 1.0));
        }
        let n = self.trunk.len();
        let z = zone.clamp(0.0, 0.999) * n as f32;
        let from_front = z.floor() as usize;
        let seg = self.trunk[n - 1 - from_front.min(n - 1)];
        (seg, 1.0 - z.fract())
    }

    fn module(&mut self, m: &Module) {
        let t = &self.g.plan.trunk;
        let unit = t.length;
        let sides: &[i8] = if m.paired { &[-1, 1] } else { &[0] };
        let pieces = match m.kind {
            Kind::Leg | Kind::Arm => 2,
            _ => 1,
        };
        let (lo, hi) = m.kind.zone();
        for k in 0..m.count.clamp(1, 4) {
            let zone = (m.zone + k as f32 * m.spacing).clamp(lo, hi);
            let (at, along) = self.attach(m, zone);
            for &side in sides {
                if !self.room(pieces) {
                    return;
                }
                let tissue = m.kind.tissue();
                let role = Role::of(m.kind);
                let hang = m.hang();
                let len = m.size * unit;
                match m.kind {
                    Kind::Leg | Kind::Arm => {
                        let (upper, lower) = if m.kind == Kind::Leg { (0.55, 0.45) } else { (0.5, 0.5) };
                        let thick = (m.width * len).max(0.02);
                        let upper_form = if m.form == Form::Beads { Form::Beads } else { Form::Tube };
                        let u = self.push(at as i16, tissue, len * upper, thick, hang, along, side, upper_form, role);
                        self.segs[u].taper = 0.85;
                        self.segs[u].onset = m.onset;
                        self.segs[u].index = k;
                        let knee = if m.kind == Kind::Leg { 0.75 } else { -0.45 };
                        let l = self.push(u as i16, tissue, len * lower, thick * 0.85, knee, 1.0, side, m.form, role);
                        self.segs[l].taper = m.taper;
                        self.segs[l].curl = m.curl;
                        self.segs[l].detail = m.detail;
                        self.segs[l].onset = m.onset;
                        self.segs[l].joint = 1;
                        self.segs[l].index = k;
                    }
                    _ => {
                        let width = (m.width * len).max(0.02);
                        let s = self.push(at as i16, tissue, len, width, hang, along, side, m.form, role);
                        let seg = &mut self.segs[s];
                        seg.taper = m.taper;
                        seg.curl = m.curl;
                        seg.detail = m.detail;
                        seg.onset = m.onset;
                        seg.index = k;
                        seg.membrane = m.kind == Kind::Wing;
                        if m.kind == Kind::Sense && m.form == Form::Bulb {
                            // Eyes sit on the surface, not at the end of a stalk.
                            seg.taper = 1.0;
                        }
                    }
                }
            }
        }
    }
}

impl Body {
    pub fn develop(g: &Genome) -> Self {
        let mut b = Builder { g, segs: Vec::new(), trunk: Vec::new(), head: None };
        b.axis();
        // Modules in the order that matters most for a working body, so
        // that when there is no room for everything, feet and mouths come
        // before frills.
        const ORDER: [Kind; 14] = [
            Kind::Mouth,
            Kind::Leg,
            Kind::Sense,
            Kind::Fin,
            Kind::Wing,
            Kind::Arm,
            Kind::Float,
            Kind::Gill,
            Kind::Horn,
            Kind::Spine,
            Kind::Film,
            Kind::Glow,
            Kind::Store,
            Kind::Brood,
        ];
        for kind in ORDER {
            for m in g.plan.of(kind) {
                b.module(m);
            }
        }
        let mut segs = b.segs;
        lay_out(&mut segs);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for s in &segs {
            lo = lo.min(s.base[0].min(s.tip[0]));
            hi = hi.max(s.base[0].max(s.tip[0]));
        }
        let t = &g.plan.trunk;
        let trunk_area: f32 = segs.iter().filter(|s| s.role == Role::Trunk).map(|s| s.area()).sum();
        let innards = [trunk_area * t.gut, trunk_area * t.fat, trunk_area * t.brood];
        // Standing: the trunk's underside sits as high as its legs reach.
        let belly = segs
            .iter()
            .filter(|s| s.role == Role::Trunk)
            .map(|s| s.base[1].min(s.tip[1]) - s.width * 0.5)
            .fold(f32::MAX, f32::min);
        let feet =
            segs.iter().filter(|s| s.role == Role::Leg && s.joint == 1).map(|s| s.tip[1]).fold(f32::MAX, f32::min);
        let stance = if feet < belly { (belly - feet).max(0.0) } else { 0.0 };
        Self { extent: (hi - lo).max(0.3), segments: segs, innards, stance }
    }

    /// Indices of a segment and everything that grows from it.
    pub fn subtree(&self, root: usize) -> u64 {
        let mut mask = 1u64 << root;
        for (i, s) in self.segments.iter().enumerate().skip(root + 1) {
            if s.parent >= 0 && mask & (1 << s.parent) != 0 {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// Segments present at this maturity and these losses, as a bit mask.
    pub fn present(&self, maturity: f32, lost: u64) -> u64 {
        let mut gone = lost;
        let mut mask = 0u64;
        for (i, s) in self.segments.iter().enumerate() {
            if s.parent >= 0 && gone & (1 << s.parent) != 0 {
                gone |= 1 << i;
            }
            if gone & (1 << i) == 0 && s.onset <= maturity {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// The head segment, if the body has one.
    pub fn head(&self) -> Option<usize> {
        self.segments.iter().position(|s| s.role == Role::Head)
    }
}

/// What a segment works as. The axis itself is never a weapon: a bulbous
/// head is a head, not a club.
pub fn organ_of(s: &Segment) -> Organ {
    let o = Organ::of(s.tissue, s.form);
    match s.role {
        Role::Head | Role::Trunk | Role::Neck
            if matches!(o, Organ::Club | Organ::Horn | Organ::Talon | Organ::Tendril) =>
        {
            Organ::Skeleton
        }
        Role::Tail if o == Organ::Limb => Organ::Skeleton,
        _ => o,
    }
}

/// Places every segment at rest and centres the body on its centre of mass.
fn lay_out(segs: &mut [Segment]) {
    for i in 0..segs.len() {
        let (base, dir) = if segs[i].parent < 0 {
            ([0.0, 0.0], segs[i].angle)
        } else {
            let p = &segs[segs[i].parent as usize];
            let a = segs[i].along;
            ([p.base[0] + (p.tip[0] - p.base[0]) * a, p.base[1] + (p.tip[1] - p.base[1]) * a], p.dir + segs[i].angle)
        };
        let s = &mut segs[i];
        s.base = base;
        s.dir = dir;
        s.tip = [base[0] + dir.cos() * s.length, base[1] + dir.sin() * s.length];
    }
    let (mut cx, mut cy, mut m) = (0.0, 0.0, 0.0);
    for s in segs.iter() {
        let w = s.area() * s.tissue.density();
        cx += (s.base[0] + s.tip[0]) * 0.5 * w;
        cy += (s.base[1] + s.tip[1]) * 0.5 * w;
        m += w;
    }
    if m > 0.0 {
        cx /= m;
        cy /= m;
        for s in segs.iter_mut() {
            s.base = [s.base[0] - cx, s.base[1] - cy];
            s.tip = [s.tip[0] - cx, s.tip[1] - cy];
        }
    }
}

/// What a body is made of at its current growth and damage, in body units.
/// The creature multiplies by its scale to get world quantities.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Phenotype {
    /// Area of each tissue, body units squared.
    pub area: [f32; 12],
    /// Area weighted by density (proportional to matter).
    pub mass: f32,
    pub total_area: f32,
    /// Walking legs (both sides counted) and their mean length hip to foot.
    pub legs: u32,
    pub leg: f32,
    pub arms: u32,
    /// Membrane span (fins, webs, wings), body units squared.
    pub membrane: f32,
    /// Area of fins, of wings, and of the tail's muscle and its fluke.
    pub fins: f32,
    pub wings: f32,
    pub tail: f32,
    pub fluke: f32,
    pub segments: u32,
    /// Area grown in each form, body units squared.
    pub form: [f32; 12],
    /// The organs its parts make.
    pub kit: Kit,
    /// Outline length of its parts, body units: thin limbs, fins and ears
    /// have far more skin for their bulk than a round trunk, and lose heat
    /// and water through it.
    pub outline: f32,
    /// Trunk area, and how high it stands.
    pub trunk: f32,
    pub stance: f32,
}

impl Phenotype {
    /// Reads a body, counting only segments grown by `maturity` and not in `lost`.
    pub fn read(body: &Body, maturity: f32, lost: u64) -> Self {
        let mut p = Phenotype::default();
        let mut gone = lost;
        let mut leg_len = 0.0;
        for (i, s) in body.segments.iter().enumerate() {
            if s.parent >= 0 && gone & (1 << s.parent) != 0 {
                gone |= 1 << i;
            }
            if gone & (1 << i) != 0 || s.onset > maturity {
                continue;
            }
            let a = s.area();
            p.area[s.tissue.index()] += a;
            p.kit.add(organ_of(s), a);
            p.outline += 2.0 * s.length + s.width;
            p.mass += a * s.tissue.density();
            p.total_area += a;
            p.form[s.form.index()] += a;
            p.segments += 1;
            // Fins and blades push on the air as webs do.
            match s.form {
                Form::Fan => p.membrane += s.length * s.length * (0.25 + 0.3 * s.detail),
                Form::Paddle => p.membrane += s.length * s.width * (1.5 + 1.5 * s.detail),
                _ => {}
            }
            match s.role {
                Role::Trunk => p.trunk += a,
                Role::Leg => {
                    if s.joint == 0 {
                        p.legs += 1;
                    }
                    leg_len += s.length;
                    if s.form == Form::Paddle {
                        p.fins += a;
                    }
                }
                Role::Arm if s.joint == 0 => p.arms += 1,
                Role::Fin => p.fins += a,
                Role::Wing => {
                    p.wings += a;
                    p.membrane += s.length * s.length * (0.3 + 0.4 * s.detail);
                }
                Role::Tail => {
                    if s.tissue == Tissue::Muscle {
                        p.tail += a;
                    }
                    if s.form.flat() {
                        p.fluke += a;
                    }
                }
                _ => {}
            }
        }
        if p.legs > 0 {
            p.leg = leg_len / p.legs as f32;
        }
        // What the trunk holds, if the trunk is there.
        if p.trunk > 0.0 {
            let whole: f32 = body.segments.iter().filter(|s| s.role == Role::Trunk).map(|s| s.area()).sum();
            let k = p.trunk / whole.max(1e-6);
            for (t, o, a) in [
                (Tissue::Gut, Organ::Stomach, body.innards[0]),
                (Tissue::Store, Organ::Fat, body.innards[1]),
                (Tissue::Brood, Organ::Brood, body.innards[2]),
            ] {
                let a = a * k;
                p.area[t.index()] += a;
                p.kit.add(o, a);
                p.mass += a * t.density();
                p.total_area += a;
            }
        }
        p.stance = body.stance;
        p
    }

    pub fn frac(&self, t: Tissue) -> f32 {
        if self.total_area > 0.0 { self.area[t.index()] / self.total_area } else { 0.0 }
    }

    pub fn has(&self, t: Tissue) -> bool {
        self.area[t.index()] > 0.0
    }

    /// Share of the body an organ takes up, 0..1.
    pub fn organ(&self, o: Organ) -> f32 {
        self.kit.frac(o, self.total_area)
    }

    /// Share of the body grown in a form, 0..1.
    pub fn form_frac(&self, f: Form) -> f32 {
        if self.total_area > 0.0 { self.form[f.index()] / self.total_area } else { 0.0 }
    }
}
