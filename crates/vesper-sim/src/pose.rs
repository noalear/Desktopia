//! Turns bodies into drawable pieces in screen-world coordinates: capsules
//! for segments and triangles for membranes, posed by gait, effort, injury
//! and decay. The host calls this at render rate, interpolating between the
//! last two sim frames, so the simulation itself never animates anything.
//!
//! Coordinates are the shaders': x along the band, y screen height (0 bottom
//! .. 1 top), both in monitor heights.

use crate::body::{Body, Segment};
use crate::creature::Action;
use crate::frame::{CreatureView, PlantView, RemainsView};
use crate::genome::Tissue;
use crate::history::Locomotion;
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
    let shrink = 1.0 - 0.35 * pos[1];
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

fn gone(body: &Body, lost: u64) -> u64 {
    let mut g = lost;
    for (i, s) in body.segments.iter().enumerate() {
        if s.parent >= 0 && g & (1 << s.parent) != 0 {
            g |= 1 << i;
        }
    }
    g
}

struct Place {
    origin: [f32; 2],
    /// World units per body unit, after perspective.
    scale: f32,
    /// +1 facing right, -1 facing left.
    face: f32,
    /// How much of its length is seen (it's turned toward or away from us).
    fore: f32,
    tilt: f32,
}

impl Place {
    fn map(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.tilt.sin_cos();
        let x = p[0] * c - p[1] * s;
        let y = p[0] * s + p[1] * c;
        [self.origin[0] + self.face * x * self.scale * self.fore, self.origin[1] + y * self.scale]
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
    for (i, s) in body.segments.iter().enumerate() {
        if skip & (1 << i) != 0 || !keep(i, s) {
            continue;
        }
        let r = s.width * 0.5 * place.scale;
        let taper = match s.tissue {
            Tissue::Spine | Tissue::Mouth => 0.25,
            Tissue::Muscle => 0.7,
            Tissue::Float => 0.85,
            _ => 0.8,
        };
        let dmg = damage(i, s);
        let sw = sway(s);
        out.segs.push(Seg {
            a: place.map(base[i]),
            b: place.map(tip[i]),
            ra: r,
            rb: r * taper,
            tissue: s.tissue,
            damage: dmg,
            side: s.side,
            sway: sw,
        });
        if s.membrane && s.parent >= 0 {
            let p = s.parent as usize;
            let far = if s.along < 0.5 { tip[p] } else { base[p] };
            out.membranes.push(Membrane {
                a: place.map(base[i]),
                b: place.map(tip[i]),
                c: place.map([(far[0] + tip[i][0]) * 0.5, (far[1] + tip[i][1]) * 0.5]),
                tissue: s.tissue,
                sway: sw,
            });
        }
    }
}

/// Poses a living creature. `time` is any running clock in seconds, for idle motion.
pub fn creature(c: &CreatureView, time: f32, out: &mut Pose) {
    out.clear();
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

fn body_has_lost_child(body: &Body, lost: u64, i: usize) -> bool {
    lost != 0 && body.segments.iter().enumerate().any(|(j, s)| s.parent == i as i16 && lost & (1 << j) != 0)
}

/// Poses a plant. Sway is left to the renderer: each piece says how much it moves.
pub fn plant(p: &PlantView, out: &mut Pose) {
    out.clear();
    let (at, shrink, z) = project(p.pos);
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
pub fn remains(r: &RemainsView, out: &mut Pose) {
    out.clear();
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
    let keep = |_: usize, s: &Segment| {
        let lasts = match s.tissue {
            Tissue::LightFilm | Tissue::Float => 0.3,
            Tissue::Gut | Tissue::Brood | Tissue::Glow | Tissue::Sense => 0.45,
            Tissue::Muscle | Tissue::Store => 0.6,
            Tissue::Mouth => 0.85,
            Tissue::Frame | Tissue::Armor | Tissue::Spine => 0.97,
        };
        d < lasts
    };
    let damage = |_: usize, _: &Segment| (0.3 + d).min(1.0);
    emit(&r.body, skip, bend, keep, damage, |_| 0.0, &place, out);
}
