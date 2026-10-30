//! Movement as biomechanics. How fast a body can go, and what that costs,
//! follows from what it is built of: legs set stride length and so speed
//! (Froude scaling), fins and a fluked tail push on the thick air or on
//! water, wings carry weight only when there is enough of them for it and
//! enough airspeed, gas floats hold a body up for nothing. The brain says
//! how hard to push and which way; this moves the body through the world
//! and keeps its stride clock, and `posture` poses it for the renderer at
//! any moment of that stride. Nothing here is simulated joint by joint:
//! every body that grows moves like something, and the costs are in the
//! energy budget where selection can see them.
//!
//! Units are world units (about ten metres each) and seconds.

use crate::body::{Body, Role};
use crate::creature::{Abilities, Action, Creature};
use crate::frame::{Moment, PartPose};
use crate::ground::Ground;
use crate::history::Locomotion;
use crate::world::{ALT_MAX, DEPTH, Env, TICK};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;
use vesper_terrain::WORLD_W;

/// World units per second squared.
pub const GRAVITY: f32 = 0.98;
/// Density of the thick air of the twilight band, relative to flesh.
pub const AIR: f32 = 0.2;
/// Density of river water, relative to flesh.
pub const WATER: f32 = 1.0;
/// Energy per second spent moving at full power, in multiples of the
/// body's resting upkeep.
pub const MOVE_K: f32 = 2.0;
/// How fast a body sinks through the thick air when nothing holds it up.
const SINK: f32 = 0.12;
/// A float-borne body: lift at least this share of its weight to hover.
pub const AFLOAT: f32 = 0.9;

/// How a body is carrying itself just now.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// Stride or stroke phase, 0..1.
    pub clock: f32,
    /// Speed along its heading, world units per second; negative is backing.
    pub speed: f32,
    /// Vertical speed.
    pub vz: f32,
    /// Which way it is drawn facing, -1 or 1.
    pub face: f32,
    /// How settled: 0 up and about .. 1 lying down.
    pub settle: f32,
    /// How it is moving right now, which the medium can change.
    pub mode: Locomotion,
}

impl Default for Motion {
    fn default() -> Self {
        Self { clock: 0.0, speed: 0.0, vz: 0.0, face: 1.0, settle: 0.0, mode: Locomotion::Crawl }
    }
}

impl Motion {
    pub fn new(heading: f32) -> Self {
        Self { face: if heading.cos() < 0.0 { -1.0 } else { 1.0 }, ..Default::default() }
    }
}

/// How this body moves in this medium: a finned body that floats in air
/// swims there; one that doesn't, swims in the river and flops ashore.
pub fn mode_in(a: &Abilities, wet: bool, alt: f32) -> Locomotion {
    let natural = a.locomotion.unwrap_or(Locomotion::Crawl);
    let land = if a.legs >= 2 {
        Locomotion::Walk
    } else if a.burrow > 0.2 {
        Locomotion::Burrow
    } else {
        Locomotion::Crawl
    };
    if wet && a.swim > 0.0 {
        return Locomotion::Swim;
    }
    match natural {
        Locomotion::Swim | Locomotion::Drift if a.buoyancy < AFLOAT => {
            if alt > 0.001 {
                natural
            } else {
                land
            }
        }
        Locomotion::Glide if alt <= 0.001 && a.fly <= 0.0 => land,
        _ => natural,
    }
}

/// Top speed in a mode of moving.
pub fn top(a: &Abilities, mode: Locomotion) -> f32 {
    match mode {
        Locomotion::Walk | Locomotion::Crawl | Locomotion::Burrow => a.walk,
        Locomotion::Swim => a.swim,
        Locomotion::Glide => a.fly.max(a.walk),
        Locomotion::Drift => a.swim.max(a.walk * 0.3),
        Locomotion::Rooted | Locomotion::FloatingMat => 0.0,
    }
}

/// What a second of moving at full power costs, in multiples of upkeep.
fn cost(mode: Locomotion) -> f32 {
    match mode {
        Locomotion::Walk => 1.0,
        Locomotion::Crawl => 0.8,
        Locomotion::Burrow => 1.6,
        Locomotion::Swim => 0.8,
        Locomotion::Glide => 2.2,
        Locomotion::Drift => 0.15,
        Locomotion::Rooted | Locomotion::FloatingMat => 0.0,
    }
}

/// Moves every creature one tick by what its body can do and its brain
/// asked. Bodies don't touch each other here, so `helpers` extra threads can
/// share them out with the same result as one thread.
pub fn step(w: &mut crate::world::World, helpers: usize) {
    let crate::world::World { creatures, env, ground, .. } = w;
    let (env, ground) = (&*env, &*ground);
    crate::sim::share(creatures, helpers, |c| body(c, env, ground));
}

fn body(c: &mut Creature, env: &Env, ground: &Ground) {
    let dt = TICK;
    let a = &c.abil;
    let m = &mut c.motion;
    let held = c.action == Action::Held || c.doom.is_some();
    let wet = c.pos[2] < 0.01 && ground.in_river(c.pos[0], c.pos[1] / DEPTH);
    let mode = mode_in(a, wet, c.pos[2]);
    m.mode = mode;
    let [power, rise] = if held { [0.0, 0.0] } else { c.drive };
    let top = top(a, mode) * (0.5 + 0.5 * c.health.clamp(0.0, 1.0)) * c.genome.tempo.sqrt();
    // Wading slows a walker; a swimmer out of its element flops.
    let top = if wet && mode != Locomotion::Swim { top * 0.4 } else { top };
    let want = if power >= 0.0 { power * top } else { power * top * 0.3 };
    let accel = (top * 4.0).max(0.002) * dt;
    m.speed += (want - m.speed).clamp(-accel, accel);
    if held {
        m.speed = 0.0;
    }
    // Up and down. Floats hold a body up; wings only at speed; the rest fall.
    let buoyant = a.buoyancy * if wet { WATER / AIR } else { 1.0 };
    let airborne = c.pos[2] > 0.001;
    let flying = mode == Locomotion::Glide && a.fly > 0.0;
    let lift = if buoyant >= AFLOAT {
        1.0
    } else if flying {
        (m.speed.abs() / a.fly.max(1e-4)).min(1.0) * (0.5 + 0.5 * power.abs()).min(1.0) + buoyant * 0.5
    } else {
        buoyant
    };
    let climb = a.climb * if mode == Locomotion::Glide { 1.0 } else { 0.7 };
    let vz_want = if held || matches!(mode, Locomotion::Walk | Locomotion::Crawl | Locomotion::Burrow) {
        -SINK
    } else {
        rise * climb * lift.min(1.0) - SINK * (1.0 - lift).max(0.0)
    };
    m.vz += (vz_want - m.vz).clamp(-0.3 * dt, 0.3 * dt);
    // Through the world.
    let (s, co) = c.heading.sin_cos();
    let mut dx = m.speed * co * dt;
    let dy = m.speed * s * dt;
    // Wind carries what hangs in the air; the river carries what is in it.
    if airborne || buoyant >= AFLOAT {
        let drift = 1.0 - 0.7 * (power.abs() * (1.0 - 0.5 * (mode == Locomotion::Drift) as u8 as f32));
        dx += env.wind_at(c.pos[0], c.pos[2]) * drift * dt;
    }
    if wet {
        dx -= 0.004 * (1.0 + env.river.flood * 2.0) * dt;
    }
    c.pos[0] = (c.pos[0] + dx).clamp(0.02, WORLD_W - 0.02);
    c.pos[1] = (c.pos[1] + dy).clamp(0.004, DEPTH - 0.004);
    let z = (c.pos[2] + m.vz * dt).clamp(0.0, ALT_MAX);
    if z <= 0.0 && m.vz < 0.0 {
        m.vz = 0.0;
    }
    c.pos[2] = z;
    c.vel = [m.speed * co, m.speed * s, m.vz];
    // What it cost: the muscles' work, more when fighting the air.
    let effort = power.abs().min(1.0);
    let aloft = flying && airborne && buoyant < AFLOAT;
    let hover = if aloft { 0.5 } else { 0.0 };
    // A body the air holds up has no weight to carry as it swims.
    let held_up = if buoyant >= AFLOAT && mode == Locomotion::Swim { 0.6 } else { 1.0 };
    c.energy -= a.upkeep * MOVE_K * (effort * effort * cost(mode) * held_up + hover) * dt;
    // The stride clock runs at the rate its gait sets.
    let hz = stride_hz(a, mode, m.speed, effort, c.genome.tempo, aloft);
    m.clock = (m.clock + hz * dt * if power < 0.0 { -1.0 } else { 1.0 }).rem_euclid(1.0);
    c.gait = m.clock * TAU;
    // Facing follows the heading, with some hysteresis so it doesn't flicker.
    if co > 0.15 {
        m.face = 1.0;
    } else if co < -0.15 {
        m.face = -1.0;
    }
    // Settling down to rest takes a few seconds; getting up is quicker.
    let want_settle = match c.action {
        Action::Resting => 1.0,
        Action::Held => 0.5,
        _ => 0.0,
    };
    let k = if want_settle > m.settle { 0.4 } else { 1.5 };
    m.settle += (want_settle - m.settle).clamp(-k * dt, k * dt);
}

/// Strides, strokes or beats per second.
fn stride_hz(a: &Abilities, mode: Locomotion, speed: f32, effort: f32, tempo: f32, aloft: bool) -> f32 {
    let v = speed.abs();
    match mode {
        Locomotion::Walk => {
            // Each stride covers about one and a half leg lengths.
            let stride = (a.leg * 1.5).max(0.001);
            (v / stride).clamp(0.0, 6.0)
        }
        Locomotion::Crawl | Locomotion::Burrow => {
            (0.3 + 2.0 * v / a.walk.max(1e-4)) * tempo * (effort > 0.02) as u8 as f32
        }
        Locomotion::Swim => (0.4 + 2.6 * (v / a.swim.max(1e-4)).min(1.0)) * tempo,
        Locomotion::Glide => {
            if aloft || effort > 0.05 {
                // Small wings beat fast; big ones slow.
                (1.2 + 2.0 * effort) * tempo * (0.01 / a.length.max(0.002)).powf(0.3).clamp(0.5, 2.5)
            } else {
                0.0
            }
        }
        Locomotion::Drift => 0.3 * tempo * (0.3 + effort),
        _ => 0.0,
    }
}

/// Everything the poser needs to know about a body's moment.
#[derive(Clone, Copy, Debug)]
pub struct Stance {
    pub mode: Locomotion,
    /// Stride phase, radians.
    pub gait: f32,
    /// How hard it is moving, 0..1.
    pub effort: f32,
    /// Speed as a share of its top speed, -1..1.
    pub pace: f32,
    /// Climbing (+) or diving (-), radians of tilt.
    pub tilt: f32,
    /// 0 up and about .. 1 settled down.
    pub settle: f32,
    /// Dead: how far gone, 0 fresh .. 1 rotted, or none.
    pub dead: Option<f32>,
    pub face: f32,
    pub heading: f32,
    pub scale: f32,
    /// Altitude of its lowest point.
    pub alt: f32,
    /// x and depth in world units.
    pub pos: [f32; 2],
    /// A per-body salt for idle fidgets.
    pub salt: f32,
    /// Seconds, for fidgets.
    pub time: f32,
}

/// Poses a body in its own upright plane at one moment of its gait.
/// `present` says which segments are grown and not torn off.
pub fn posture(body: &Body, present: u64, st: &Stance) -> Moment {
    let segs = &body.segments;
    let n = segs.len();
    let mut bend = vec![0.0f32; n];
    let ph = st.gait;
    let e = st.effort.clamp(0.0, 1.0);
    let pace = st.pace.clamp(-1.0, 1.0);
    let speed = pace.abs();
    let idle = |i: usize, k: f32| k * (st.time * 0.9 + st.salt * 7.0 + i as f32 * 1.7).sin();
    let settle = st.settle.clamp(0.0, 1.0);
    let swimming = matches!(st.mode, Locomotion::Swim);
    let flying = matches!(st.mode, Locomotion::Glide);
    let walking = matches!(st.mode, Locomotion::Walk);
    let crawling = matches!(st.mode, Locomotion::Crawl | Locomotion::Burrow);
    let dead = st.dead.unwrap_or(-1.0);
    // How many legs, for the gait pattern.
    let pairs = segs.iter().filter(|s| s.role == Role::Leg && s.joint == 0 && s.side >= 0).count().max(1);
    for (i, s) in segs.iter().enumerate() {
        if present & (1 << i) == 0 {
            continue;
        }
        let far = (s.side < 0) as u8 as f32;
        let b = match s.role {
            Role::Leg => {
                if dead >= 0.0 {
                    // Splayed, then drawn up as it dries.
                    let splay = 0.5 + 0.3 * (i as f32 * 0.7).sin();
                    if s.joint == 0 { splay * (1.0 - dead * 0.5) } else { -0.9 * splay }
                } else {
                    // Diagonal pairs for four legs, alternating waves for more.
                    let off = 0.5 * ((s.index as usize + far as usize) % 2) as f32
                        + if pairs > 2 { 0.08 * s.index as f32 } else { 0.0 };
                    let th = ph + off * TAU;
                    let amp = (0.25 + 0.2 * speed) * speed;
                    let swing = th.sin() * amp * if walking { 1.0 } else { 0.3 };
                    let lift = th.cos().max(0.0) * 0.55 * speed * walking as u8 as f32;
                    let folded = settle * if s.joint == 0 { 0.7 } else { -1.3 };
                    let fly_tuck = if flying && st.alt > 0.002 { if s.joint == 0 { 0.9 } else { -0.8 } } else { 0.0 };
                    if s.joint == 0 {
                        swing + folded + fly_tuck + idle(i, 0.02)
                    } else {
                        lift - swing * 0.5 + folded + fly_tuck
                    }
                }
            }
            Role::Arm => {
                if dead >= 0.0 {
                    if s.joint == 0 { 0.4 } else { -0.6 }
                } else {
                    let th = ph + far * 0.5 * TAU;
                    let reach = if walking { th.sin() * 0.2 * speed } else { 0.0 };
                    let fold = settle * if s.joint == 0 { -0.5 } else { 0.8 };
                    if s.joint == 0 { reach + fold + idle(i, 0.04) } else { -reach * 0.5 + fold }
                }
            }
            Role::Fin => {
                if dead >= 0.0 {
                    -0.4
                } else if swimming || st.mode == Locomotion::Drift {
                    (ph + far * 0.3).sin() * (0.25 + 0.45 * e) + idle(i, 0.05)
                } else {
                    (ph * 0.5).sin() * 0.1 * e + idle(i, 0.05) - 0.3 * settle
                }
            }
            Role::Wing => {
                if dead >= 0.0 {
                    1.3
                } else if flying && (st.alt > 0.002 || e > 0.3) {
                    // Downstroke is quick and strong, upstroke slower.
                    let x = ph.rem_euclid(TAU) / TAU;
                    let s = if x < 0.4 {
                        -(x / 0.4 * std::f32::consts::PI).sin()
                    } else {
                        ((x - 0.4) / 0.6 * std::f32::consts::PI).sin()
                    };
                    s * (0.5 + 0.5 * e) - 0.15
                } else {
                    // Folded flat along the back.
                    (std::f32::consts::PI - 0.25) - s.angle - 0.2 * e + idle(i, 0.03)
                }
            }
            Role::Tail => {
                if dead >= 0.0 {
                    -0.3 * (1.0 - dead) * ((s.index as f32) + 1.0) * 0.4
                } else {
                    let wave = (ph - s.index as f32 * 0.9).sin();
                    let amp = if swimming {
                        0.3 + 0.35 * e
                    } else if walking || crawling {
                        0.12 * speed + 0.05
                    } else {
                        0.08
                    };
                    wave * amp / (1.0 + 0.3 * s.index as f32) + idle(i, 0.04)
                        - 0.15 * settle * (s.index == 0) as u8 as f32
                }
            }
            Role::Trunk => {
                if dead >= 0.0 {
                    0.0
                } else if s.parent < 0 {
                    // The root sets the whole body's pitch.
                    st.tilt + if swimming { (ph).sin() * 0.04 * e } else { 0.0 }
                } else if swimming || crawling {
                    (ph + s.index as f32 * 0.8).sin() * (if crawling { 0.12 } else { 0.08 }) * e
                } else {
                    0.0
                }
            }
            Role::Neck => {
                if dead >= 0.0 {
                    -0.4 * (s.index == 0) as u8 as f32
                } else {
                    let bob = if walking { (ph * 2.0).sin() * 0.06 * speed } else { 0.0 };
                    bob - 0.35 * settle * (s.index == 0) as u8 as f32 + idle(i, 0.03)
                }
            }
            Role::Head => {
                if dead >= 0.0 {
                    -0.3
                } else {
                    -0.1 * settle + idle(i, 0.025)
                }
            }
            Role::Mouth => {
                if dead >= 0.0 {
                    0.25 * (s.side >= 0) as u8 as f32
                } else {
                    idle(i, 0.02)
                }
            }
            Role::Spine | Role::Horn => {
                if dead >= 0.0 {
                    0.0
                } else {
                    idle(i, 0.02) - 0.2 * settle * (s.role == Role::Spine) as u8 as f32
                }
            }
            Role::Float | Role::Film | Role::Glow | Role::Store | Role::Brood | Role::Gill | Role::Sense => {
                if dead >= 0.0 {
                    -0.3 * dead
                } else {
                    idle(i, 0.04)
                        + if st.mode == Locomotion::Drift && s.role == Role::Film {
                            (ph + i as f32).sin() * 0.1
                        } else {
                            0.0
                        }
                }
            }
        };
        bend[i] = b;
    }
    // Lay out from the root with the bends, in world units.
    let sc = st.scale;
    let mut base = vec![[0.0f32; 2]; n];
    let mut tip = vec![[0.0f32; 2]; n];
    let mut dir = vec![0.0f32; n];
    for (i, s) in segs.iter().enumerate() {
        let (b, d) = if s.parent < 0 {
            ([s.base[0] * sc, s.base[1] * sc], s.dir + bend[i])
        } else {
            let p = s.parent as usize;
            let a = s.along;
            (
                [base[p][0] + (tip[p][0] - base[p][0]) * a, base[p][1] + (tip[p][1] - base[p][1]) * a],
                dir[p] + s.angle + bend[i],
            )
        };
        base[i] = b;
        dir[i] = d;
        tip[i] = [b[0] + d.cos() * s.length * sc, b[1] + d.sin() * s.length * sc];
    }
    // Whatever is lowest rests at the body's altitude: feet on the ground,
    // a belly on the ground, a hull in the air.
    let mut low = f32::MAX;
    for (i, s) in segs.iter().enumerate() {
        if present & (1 << i) == 0 {
            continue;
        }
        let r = s.width * 0.5 * sc * s.taper.min(1.0);
        low = low.min(base[i][1].min(tip[i][1]) - r);
    }
    let shift = if low < f32::MAX { st.alt - low } else { 0.0 };
    let parts = segs
        .iter()
        .enumerate()
        .filter(|(i, _)| present & (1 << i) != 0)
        .map(|(i, _)| PartPose {
            seg: i as u16,
            u: (base[i][0] + tip[i][0]) * 0.5,
            alt: (base[i][1] + tip[i][1]) * 0.5 + shift,
            angle: dir[i],
        })
        .collect();
    Moment { pos: st.pos, heading: st.heading, face: st.face, scale: sc, parts }
}

/// A plain standing pose, for galleries: side on, still, grown.
pub fn standing(body: &Body, scale: f32, face: f32) -> Moment {
    let st = Stance {
        mode: Locomotion::Walk,
        gait: 0.0,
        effort: 0.0,
        pace: 0.0,
        tilt: 0.0,
        settle: 0.0,
        dead: None,
        face,
        heading: if face < 0.0 { std::f32::consts::PI } else { 0.0 },
        scale,
        alt: 0.0,
        pos: [0.0; 2],
        salt: 0.0,
        time: 0.0,
    };
    posture(body, u64::MAX, &st)
}
