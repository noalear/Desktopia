//! Turns bodies into drawable pieces in screen-world coordinates: capsules
//! for segments and triangles for membranes, posed by gait, effort, injury
//! and decay. The host calls this at render rate, interpolating between the
//! last two sim frames, so the simulation itself never animates anything.
//!
//! Coordinates are the shaders': x along the band, y screen height (0 bottom
//! .. 1 top), both in monitor heights.

use crate::body::{Body, Segment};
use crate::creature::Action;
use crate::frame::{CreatureView, Moment, PartPose, PlantView, RemainsView};
use crate::genome::Tissue;
use crate::history::Locomotion;
use crate::world::DEPTH;
use vesper_terrain as terrain;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub a: [f32; 2],
    pub b: [f32; 2],
    /// Radius at each end.
    pub ra: f32,
    pub rb: f32,
    pub tissue: Tissue,
    /// 0 whole .. 1 torn, rotten or bleeding.
    pub damage: f32,
    /// -1 far side, 0 middle, +1 near side, for limb pairs seen from the side.
    pub side: i8,
    /// How much the wind moves it (plants), 0 at the root .. 1 at the tips.
    pub sway: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Membrane {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub c: [f32; 2],
    pub tissue: Tissue,
    pub sway: f32,
}

/// One posed organism.
#[derive(Clone, Debug, Default)]
pub struct Pose {
    pub segs: Vec<Seg>,
    pub membranes: Vec<Membrane>,
    /// Draw order: 0 nearest the viewer .. 1 farthest. Draw high z first.
    pub z: f32,
    /// Where it sits on screen and how much perspective shrinks it.
    pub at: [f32; 2],
    pub shrink: f32,
}

impl Pose {
    pub fn clear(&mut self) {
        self.segs.clear();
        self.membranes.clear();
    }
}

/// Screen position, perspective shrink and draw depth of a point given as x,
/// depth on the ground plane 0..1, and altitude above it.
pub fn project(pos: [f32; 3]) -> ([f32; 2], f32, f32) {
    let shrink = terrain::shrink(pos[0], pos[1]);
    let h = terrain::floor_h(pos[0], pos[1]) + pos[2] * shrink;
    ([pos[0], h], shrink, pos[1])
}

/// Linear blend between two snapshots of a creature, for drawing between ticks.
pub fn blend(a: &CreatureView, b: &CreatureView, t: f32) -> CreatureView {
    let mut c = b.clone();
    for i in 0..3 {
        c.pos[i] = a.pos[i] + (b.pos[i] - a.pos[i]) * t;
    }
    let mut dh = b.heading - a.heading;
    if dh > std::f32::consts::PI {
        dh -= std::f32::consts::TAU;
    } else if dh < -std::f32::consts::PI {
        dh += std::f32::consts::TAU;
    }
    c.heading = a.heading + dh * t;
    let mut dg = b.gait - a.gait;
    if dg < -std::f32::consts::PI {
        dg += std::f32::consts::TAU;
    }
    c.gait = a.gait + dg * t;
    c.flash = a.flash + (b.flash - a.flash) * t;
    c.scale = a.scale + (b.scale - a.scale) * t;
    c
}

pub(crate) fn gone(body: &Body, lost: u64) -> u64 {
    let mut g = lost;
    for (i, s) in body.segments.iter().enumerate() {
        if s.parent >= 0 && g & (1 << s.parent) != 0 {
            g |= 1 << i;
        }
    }
    g
}

pub(crate) struct Place {
    pub origin: [f32; 2],
    /// World units per body unit, after perspective.
    pub scale: f32,
    /// +1 facing right, -1 facing left.
    pub face: f32,
    /// How much of its length is seen (it's turned toward or away from us).
    pub fore: f32,
    pub tilt: f32,
}

impl Place {
    pub fn map(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.tilt.sin_cos();
        let x = p[0] * c - p[1] * s;
        let y = p[0] * s + p[1] * c;
        [self.origin[0] + self.face * x * self.scale * self.fore, self.origin[1] + y * self.scale]
    }
}

/// Where each segment's base and tip land on screen, with extra bend per segment.
pub(crate) fn layout(
    body: &Body,
    bend: impl Fn(usize, &Segment) -> f32,
    place: &Place,
    base_out: &mut Vec<[f32; 2]>,
    tip_out: &mut Vec<[f32; 2]>,
) {
    let n = body.segments.len();
    let mut base = vec![[0.0f32; 2]; n];
    let mut tip = vec![[0.0f32; 2]; n];
    let mut dir = vec![0.0f32; n];
    for (i, s) in body.segments.iter().enumerate() {
        let (b, d) = if s.parent < 0 {
            (s.base, s.dir + bend(i, s))
        } else {
            let p = s.parent as usize;
            let a = s.along;
            (
                [base[p][0] + (tip[p][0] - base[p][0]) * a, base[p][1] + (tip[p][1] - base[p][1]) * a],
                dir[p] + s.angle + bend(i, s),
            )
        };
        base[i] = b;
        dir[i] = d;
        // Far-side limbs sit a touch higher and back, so pairs don't overlap exactly.
        let off = if s.side < 0 { [-0.04, 0.03] } else { [0.0, 0.0] };
        tip[i] = [b[0] + d.cos() * s.length + off[0], b[1] + d.sin() * s.length + off[1]];
    }
    base_out.clear();
    tip_out.clear();
    base_out.extend(base.iter().map(|&p| place.map(p)));
    tip_out.extend(tip.iter().map(|&p| place.map(p)));
}

/// Emits segments and membranes laid out at the given screen positions,
/// each segment drawn in its form. `hand` is -1 when the body is seen mirrored.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    body: &Body,
    skip: u64,
    keep: impl Fn(usize, &Segment) -> bool,
    damage: impl Fn(usize, &Segment) -> f32,
    sway: impl Fn(&Segment) -> f32,
    base: &[[f32; 2]],
    tip: &[[f32; 2]],
    scale: f32,
    hand: f32,
    out: &mut Pose,
) {
    // Which segments carry on into another at the tip, or carry limbs along
    // their length, so their forms stay joined to what grows from them.
    let mut continues = 0u64;
    let mut carries = 0u64;
    for (j, s) in body.segments.iter().enumerate() {
        if s.parent >= 0 && skip & (1 << j) == 0 && keep(j, s) {
            let bit = 1u64 << s.parent;
            if s.along > 0.85 {
                continues |= bit;
            } else if s.along > 0.1 {
                carries |= bit;
            }
        }
    }
    for (i, s) in body.segments.iter().enumerate() {
        if skip & (1 << i) != 0 || !keep(i, s) {
            continue;
        }
        let r = s.width * 0.5 * scale;
        let sw = sway(s);
        let sw0 = if s.parent >= 0 { sway(&body.segments[s.parent as usize]) } else { 0.0 };
        crate::forms::draw(
            out,
            s,
            i,
            &crate::forms::Piece {
                a: base[i],
                b: tip[i],
                ra: r,
                rb: r * s.taper,
                damage: damage(i, s),
                sway: [sw0.min(sw), sw],
                hand,
                continues: continues & (1 << i) != 0,
                carries: carries & (1 << i) != 0,
            },
        );
        if s.membrane && s.parent >= 0 {
            let p = s.parent as usize;
            let far = if s.along < 0.5 { tip[p] } else { base[p] };
            out.membranes.push(Membrane {
                a: base[i],
                b: tip[i],
                c: [(far[0] + tip[i][0]) * 0.5, (far[1] + tip[i][1]) * 0.5],
                tissue: s.tissue,
                sway: sw,
            });
        }
    }
}

/// Lays out a body with extra bend per segment and emits it.
#[allow(clippy::too_many_arguments)]
fn emit(
    body: &Body,
    skip: u64,
    bend: impl Fn(usize, &Segment) -> f32,
    keep: impl Fn(usize, &Segment) -> bool,
    damage: impl Fn(usize, &Segment) -> f32,
    sway: impl Fn(&Segment) -> f32,
    place: &Place,
    out: &mut Pose,
) {
    let (mut base, mut tip) = (Vec::new(), Vec::new());
    layout(body, bend, place, &mut base, &mut tip);
    draw(body, skip, keep, damage, sway, &base, &tip, place.scale, place.face.signum(), out);
}

/// A body's pose `t` of the way (0..1) through its tick.
pub fn sample(motion: &[Moment], t: f32) -> Option<Moment> {
    match motion.len() {
        0 => None,
        1 => Some(motion[0].clone()),
        n => {
            let f = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (f as usize).min(n - 2);
            let k = f - i as f32;
            let (a, b) = (&motion[i], &motion[i + 1]);
            if a.parts.len() != b.parts.len() || a.parts.iter().zip(&b.parts).any(|(p, q)| p.seg != q.seg) {
                return Some(if k < 0.5 { a.clone() } else { b.clone() });
            }
            let lerp = |x: f32, y: f32| x + (y - x) * k;
            let turn = |x: f32, y: f32| x + crate::world::wrap_angle(y - x) * k;
            Some(Moment {
                pos: [lerp(a.pos[0], b.pos[0]), lerp(a.pos[1], b.pos[1])],
                heading: turn(a.heading, b.heading),
                face: lerp(a.face, b.face),
                scale: lerp(a.scale, b.scale),
                parts: a
                    .parts
                    .iter()
                    .zip(&b.parts)
                    .map(|(p, q)| PartPose {
                        seg: p.seg,
                        u: lerp(p.u, q.u),
                        alt: lerp(p.alt, q.alt),
                        angle: turn(p.angle, q.angle),
                    })
                    .collect(),
            })
        }
    }
}

/// Lays out a body from its physical pose and emits it.
pub(crate) fn physical(
    body: &Body,
    m: &Moment,
    keep: impl Fn(usize, &Segment) -> bool,
    damage: impl Fn(usize, &Segment) -> f32,
    out: &mut Pose,
) {
    let (at, shrink, z) = project([m.pos[0], m.pos[1] / DEPTH, 0.0]);
    out.z = z;
    out.at = at;
    out.shrink = shrink;
    // Turned toward or away from us, a body is seen shortened, but never so
    // much that it vanishes into a line.
    let fore = m.face * m.heading.cos().abs().max(0.45);
    let n = body.segments.len();
    let mut base = vec![[0.0f32; 2]; n];
    let mut tip = vec![[0.0f32; 2]; n];
    let mut mask = 0u64;
    for p in &m.parts {
        let i = p.seg as usize;
        let Some(s) = body.segments.get(i) else { continue };
        let half = s.length * m.scale * 0.5;
        let (sa, ca) = p.angle.sin_cos();
        let screen = |u: f32, alt: f32| [at[0] + u * fore * shrink, at[1] + alt * shrink];
        base[i] = screen(p.u - ca * half, p.alt - sa * half);
        tip[i] = screen(p.u + ca * half, p.alt + sa * half);
        mask |= 1 << i;
    }
    let hand = if m.face < 0.0 { -1.0 } else { 1.0 };
    draw(body, !mask, keep, damage, |_| 0.0, &base, &tip, m.scale * shrink, hand, out);
}

/// Poses a living creature `t` of the way (0..1) through the tick its view
/// came from, from the physics of its body.
pub fn creature(c: &CreatureView, t: f32, out: &mut Pose) {
    out.clear();
    if let Some(m) = sample(&c.motion, t) {
        physical(&c.body, &m, |_, _| true, wounds(c), out);
        return;
    }
    scripted(c, out);
}

/// How damaged each segment of a living creature looks.
pub(crate) fn wounds(c: &CreatureView) -> impl Fn(usize, &Segment) -> f32 + '_ {
    let wounded = 1.0 - c.health;
    move |i: usize, _: &Segment| {
        // Stumps where a limb was torn off, and a general toll from wounds.
        let stump = body_has_lost_child(&c.body, c.lost, i);
        (wounded * 0.6 + if stump { 0.8 } else { 0.0 } + c.bleeding * 0.3).min(1.0)
    }
}

/// A creature without physics, posed from its gait alone.
fn scripted(c: &CreatureView, out: &mut Pose) {
    let time = 0.0;
    let (at, shrink, z) = project(c.pos);
    let speed = (c.vel[0] * c.vel[0] + c.vel[1] * c.vel[1]).sqrt();
    let climb = if speed > 1e-4 || c.vel[2].abs() > 1e-4 { (c.vel[2]).atan2(speed.max(1e-4)) * 0.4 } else { 0.0 };
    let place = Place {
        origin: at,
        scale: c.scale * shrink,
        face: if c.heading.cos() >= 0.0 { 1.0 } else { -1.0 },
        fore: c.heading.cos().abs().max(0.45),
        tilt: climb.clamp(-0.5, 0.5),
    };
    out.z = z;
    out.at = at;
    out.shrink = shrink;
    let skip = gone(&c.body, c.lost);
    let swim = matches!(c.locomotion, Locomotion::Swim | Locomotion::Crawl | Locomotion::Burrow);
    let effort = c.effort.max(0.15);
    let idle = (time * 0.7 + (c.id % 97) as f32).sin() * 0.06;
    let strike = matches!(c.action, Action::Attacking | Action::Eating) as u8 as f32;
    let wounded = 1.0 - c.health;
    let bend = |_: usize, s: &Segment| -> f32 {
        let phase = c.gait - s.depth as f32 * 0.9;
        match s.tissue {
            Tissue::Muscle => {
                let pair = if s.side > 0 { std::f32::consts::PI } else { 0.0 };
                let amp = if swim { 0.45 } else { 0.35 } * effort;
                (phase + pair).sin() * amp + idle
            }
            Tissue::Mouth => strike * (time * 9.0).sin().abs() * 0.5,
            Tissue::LightFilm | Tissue::Float => (phase * 0.5).sin() * 0.12 * effort + idle,
            _ if s.parent >= 0 && swim => (phase).sin() * 0.12 * effort,
            _ => idle * 0.5,
        }
    };
    let keep = |_: usize, s: &Segment| s.onset <= c.maturity;
    let damage = |i: usize, s: &Segment| {
        // Stumps where a limb was torn off, and a general toll from wounds.
        let stump = body_has_lost_child(&c.body, c.lost, i);
        let _ = s;
        (wounded * 0.6 + if stump { 0.8 } else { 0.0 } + c.bleeding * 0.3).min(1.0)
    };
    emit(&c.body, skip, bend, keep, damage, |_| 0.0, &place, out);
}

pub(crate) fn body_has_lost_child(body: &Body, lost: u64, i: usize) -> bool {
    lost != 0 && body.segments.iter().enumerate().any(|(j, s)| s.parent == i as i16 && lost & (1 << j) != 0)
}

/// Poses a grown body standing still, its origin at screen point `at`,
/// `scale` screen units per body unit, facing right (`face` 1) or left (-1),
/// for galleries and portraits. Plants say how much each piece sways.
pub fn at_rest(body: &Body, at: [f32; 2], scale: f32, face: f32, plant: bool, out: &mut Pose) {
    out.clear();
    out.at = at;
    out.shrink = 1.0;
    let place = Place { origin: at, scale, face, fore: 1.0, tilt: 0.0 };
    let max_depth = body.segments.iter().map(|s| s.depth).max().unwrap_or(1).max(1) as f32;
    let sway = |s: &Segment| if plant { s.depth as f32 / max_depth } else { 0.0 };
    emit(body, 0, |_, _| 0.0, |_, _| true, |_, _| 0.0, sway, &place, out);
}

/// Poses a plant. Sway is left to the renderer: each piece says how much it moves.
pub fn plant(p: &PlantView, out: &mut Pose) {
    plant_at(p, p.pos, out);
}

/// Poses a plant somewhere other than where its snapshot has it, such as a
/// floating mat blended between ticks.
pub fn plant_at(p: &PlantView, pos: [f32; 3], out: &mut Pose) {
    out.clear();
    let (at, shrink, z) = project(pos);
    let place = Place { origin: at, scale: p.scale * shrink, face: 1.0, fore: 1.0, tilt: 0.0 };
    out.z = z;
    out.at = at;
    out.shrink = shrink;
    let max_depth = p.body.segments.iter().map(|s| s.depth).max().unwrap_or(1).max(1) as f32;
    // Leaves droop when the plant is starved of light; traps close.
    let droop = (1.0 - p.vigor) * 0.5;
    let bend = |_: usize, s: &Segment| match s.tissue {
        Tissue::LightFilm => -droop * s.angle.signum(),
        Tissue::Mouth if p.trap_shut => -0.6,
        _ => 0.0,
    };
    let keep = |_: usize, s: &Segment| s.tissue != Tissue::Brood || p.fruit > 0.05;
    let damage = |_: usize, _: &Segment| 1.0 - p.health;
    let sway = |s: &Segment| s.depth as f32 / max_depth;
    emit(&p.body, 0, bend, keep, damage, sway, &place, out);
}

/// Poses a body or piece of one, lying where it fell and falling apart.
pub fn remains(r: &RemainsView, t: f32, out: &mut Pose) {
    out.clear();
    if let Some(m) = sample(&r.motion, t) {
        let d = r.decay;
        let keep = |_: usize, s: &Segment| d < lasts(s.tissue);
        physical(&r.body, &m, keep, |_, _| (0.3 + d).min(1.0), out);
        return;
    }
    let (at, shrink, z) = project(r.pos);
    let settle = (r.age / 6.0).min(1.0);
    let place = Place {
        origin: at,
        scale: r.scale * shrink,
        face: if r.heading.cos() >= 0.0 { 1.0 } else { -1.0 },
        fore: r.heading.cos().abs().max(0.45),
        tilt: -0.25 * settle,
    };
    out.z = z;
    out.at = at;
    out.shrink = shrink;
    let skip = gone(&r.body, r.missing);
    let d = r.decay;
    // Limbs go slack, then soft tissue rots away before shell and frame.
    let bend = |_: usize, s: &Segment| if s.parent >= 0 { -0.35 * settle * s.angle.signum() } else { 0.0 };
    let keep = |_: usize, s: &Segment| d < lasts(s.tissue);
    let damage = |_: usize, _: &Segment| (0.3 + d).min(1.0);
    emit(&r.body, skip, bend, keep, damage, |_| 0.0, &place, out);
}

/// How far into decay each tissue lasts: soft tissue goes first, then shell and frame.
pub(crate) fn lasts(t: Tissue) -> f32 {
    match t {
        Tissue::LightFilm | Tissue::Float => 0.3,
        Tissue::Gut | Tissue::Brood | Tissue::Glow | Tissue::Sense => 0.45,
        Tissue::Muscle | Tissue::Store => 0.6,
        Tissue::Mouth => 0.85,
        Tissue::Frame | Tissue::Armor | Tissue::Spine => 0.97,
    }
}
