//! Development: unrolling a genome's part graph into a body of linked
//! segments, and reading what that body can do.

use crate::genome::{Genome, MAX_PARTS, Tissue};
use serde::{Deserialize, Serialize};

pub const MAX_SEGMENTS: usize = 48;
const MAX_DEPTH: u8 = 8;
const MIN_LENGTH: f32 = 0.1;

/// One segment of a body, in body units. The root points forward along +x;
/// +y is up. Bodies are seen from the side, so a pair of limbs becomes a near
/// segment (side +1) and a far one (side -1) at the same place.
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
    /// Links from the root; drives the travelling wave of a gait.
    pub depth: u8,
    /// Rest pose, body units, centred on the body's centre of mass.
    pub base: [f32; 2],
    pub tip: [f32; 2],
    /// Rest direction, radians from +x.
    pub dir: f32,
}

impl Segment {
    pub fn area(&self) -> f32 {
        self.length * self.width
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub segments: Vec<Segment>,
    /// Nose-to-tail extent of the full-grown body, body units.
    pub extent: f32,
}

/// A placeholder body, replaced as soon as a loaded world regrows its bodies.
pub fn empty() -> std::sync::Arc<Body> {
    std::sync::Arc::new(Body { segments: Vec::new(), extent: 0.3 })
}

impl Body {
    pub fn develop(g: &Genome) -> Self {
        let mut segs: Vec<Segment> = Vec::new();
        let mut counts = [0u8; MAX_PARTS];
        grow(g, 0, -1, None, 1.0, 0, &mut counts, 0, 0.0, &mut segs);
        lay_out(&mut segs);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for s in &segs {
            lo = lo.min(s.base[0].min(s.tip[0]));
            hi = hi.max(s.base[0].max(s.tip[0]));
        }
        Self { extent: (hi - lo).max(0.3), segments: segs }
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
}

#[allow(clippy::too_many_arguments)]
fn grow(
    g: &Genome,
    part: usize,
    parent: i16,
    link: Option<(f32, f32, i8, bool, f32)>,
    scale: f32,
    depth: u8,
    counts: &mut [u8; MAX_PARTS],
    side: i8,
    onset: f32,
    segs: &mut Vec<Segment>,
) {
    let p = &g.parts[part];
    let length = p.length * scale;
    if length < MIN_LENGTH || segs.len() >= MAX_SEGMENTS || depth > MAX_DEPTH {
        return;
    }
    let (angle, along, side, membrane, onset) = link.unwrap_or((0.0, 0.0, side, false, onset));
    let me = segs.len() as i16;
    segs.push(Segment {
        parent,
        tissue: p.tissue,
        length,
        width: (p.width * p.length * scale).max(0.04),
        angle,
        along,
        side,
        onset,
        membrane,
        depth,
        base: [0.0; 2],
        tip: [0.0; 2],
        dir: 0.0,
    });
    counts[part] += 1;
    let last = counts[part] > p.recursion;
    for l in &p.links {
        let to = l.to as usize % g.parts.len();
        if l.tip_only && !last {
            continue;
        }
        let limit = g.parts[to].recursion + 1;
        if counts[to] >= limit {
            continue;
        }
        let sides: &[i8] = if l.pair && side == 0 { &[-1, 1] } else { &[0] };
        let reps = l.repeat.max(1);
        for &sd in sides {
            let s = if sd == 0 { side } else { sd };
            for k in 0..reps {
                let fan = (k as f32 - (reps - 1) as f32 * 0.5) * 0.45;
                let child_onset = onset.max(l.onset);
                grow(
                    g,
                    to,
                    me,
                    Some((l.angle + fan, l.along, s, l.membrane, child_onset)),
                    scale * l.scale,
                    depth + 1,
                    counts,
                    s,
                    child_onset,
                    segs,
                );
            }
        }
    }
    counts[part] -= 1;
}

/// Places every segment at rest and centres the body on its centre of mass.
fn lay_out(segs: &mut [Segment]) {
    for i in 0..segs.len() {
        let (base, dir) = if segs[i].parent < 0 {
            ([0.0, 0.0], 0.0)
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

/// What a body can do at its current growth and damage, in body units. The
/// creature multiplies by its scale to get world quantities.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Phenotype {
    /// Area of each tissue, body units squared.
    pub area: [f32; 12],
    /// Area weighted by density (proportional to matter).
    pub mass: f32,
    pub total_area: f32,
    /// Leg-like segments touching down.
    pub legs: u32,
    /// Membrane span (fins, webs, wings), body units squared.
    pub membrane: f32,
    /// Tail-like muscle chains that can drive swimming.
    pub tail: f32,
    pub segments: u32,
}

impl Phenotype {
    /// Reads a body, counting only segments grown by `maturity` and not in `lost`.
    pub fn read(body: &Body, maturity: f32, lost: u64) -> Self {
        let mut p = Phenotype::default();
        let mut gone = lost;
        for (i, s) in body.segments.iter().enumerate() {
            if s.parent >= 0 && gone & (1 << s.parent) != 0 {
                gone |= 1 << i;
            }
            if gone & (1 << i) != 0 || s.onset > maturity {
                continue;
            }
            let a = s.area();
            p.area[s.tissue.index()] += a;
            p.mass += a * s.tissue.density();
            p.total_area += a;
            p.segments += 1;
            let down = s.dir.sin() < -0.45;
            if down && s.side != 0 && matches!(s.tissue, Tissue::Muscle | Tissue::Frame | Tissue::Armor) && s.depth <= 2
            {
                p.legs += 1;
            }
            if s.membrane && s.parent >= 0 {
                let par = &body.segments[s.parent as usize];
                p.membrane += 0.5 * s.length * par.length * (s.angle.sin().abs() + 0.2);
            }
            if s.tissue == Tissue::Muscle && s.dir.cos() < -0.5 {
                p.tail += a;
            }
        }
        p
    }

    pub fn frac(&self, t: Tissue) -> f32 {
        if self.total_area > 0.0 { self.area[t.index()] / self.total_area } else { 0.0 }
    }

    pub fn has(&self, t: Tissue) -> bool {
        self.area[t.index()] > 0.0
    }
}
