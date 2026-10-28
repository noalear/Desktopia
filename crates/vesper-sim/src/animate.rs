//! Behaviour drawn on top of physics. The simulation says what each creature
//! is doing and its body's physics says how it stands and walks; this bends
//! that physical pose by the act. Heads dip to feed and yank to tear, hunters
//! wind up and lunge, the threatened rear up, gape and bristle, the bitten
//! flinch, givers heave food up for beggars, partners press and rock
//! together, layers squat and strain, the sick shiver and retch, the trapped
//! thrash, and the dead spasm before they go slack.
//!
//! It runs on the host at render rate and never feeds back into the
//! simulation. Acts flicker on and off from one sim tick to the next, so each
//! is eased in and out, and one-tick events such as a mating are played out
//! for a few seconds.

use crate::body::Body;
use crate::creature::Action;
use crate::frame::{CreatureView, Frame, Moment, PartPose, RemainsView};
use crate::genome::Tissue;
use crate::pose::{self, Pose};
use crate::world::{DEPTH, wrap_angle};
use std::collections::HashMap;
use std::f32::consts::TAU;

/// How long one mating and one laying are played out, seconds.
const MATE_SECS: f32 = 5.0;
const LAY_SECS: f32 = 4.0;
/// How long a fresh corpse keeps spasming, seconds.
const THROES_SECS: f32 = 7.0;

// The eased acts.
const EAT: usize = 0;
const STRIKE: usize = 1;
const GUARD: usize = 2;
const DEFEND: usize = 3;
const FLEE: usize = 4;
const GIVE: usize = 5;
const TAKE: usize = 6;
const GROOM: usize = 7;
const GROOMED: usize = 8;
const HELD: usize = 9;
const SICK: usize = 10;
const WEAK: usize = 11;
const ACTS: usize = 12;

/// How fast each act eases in and out, per second.
const RISE: f32 = 7.0;
const FALL: f32 = 2.2;
/// Sickness and weakness come and go slowly.
const SLOW: f32 = 0.8;

#[derive(Clone, Debug, Default)]
struct Acting {
    goal: [f32; ACTS],
    w: [f32; ACTS],
    /// What it is eating, biting or touching: x, depth 0..1, altitude.
    focus: Option<[f32; 3]>,
    /// Whoever is biting it.
    from: Option<[f32; 3]>,
    /// Startle from a fresh wound, 1 falling to 0.
    hurt: f32,
    /// Seconds into a mating, and where the partner was.
    mate: Option<f32>,
    partner: Option<[f32; 3]>,
    /// Seconds into laying eggs or budding.
    lay: Option<f32>,
    health: f32,
    tick: u64,
}

impl Acting {
    fn advance(&mut self, dt: f32) {
        for k in 0..ACTS {
            let (g, w) = (self.goal[k], self.w[k]);
            let rate = if k == SICK || k == WEAK {
                SLOW
            } else if g > w {
                RISE
            } else {
                FALL
            };
            self.w[k] = w + (g - w) * (1.0 - (-rate * dt).exp());
        }
        self.hurt *= (-5.0 * dt).exp();
        self.mate = self.mate.map(|t| t + dt).filter(|&t| t < MATE_SECS);
        self.lay = self.lay.map(|t| t + dt).filter(|&t| t < LAY_SECS);
    }
}

/// Keeps what every creature is doing, eased over time, and poses bodies
/// acting it out. The host owns one and updates it every drawn frame.
#[derive(Default)]
pub struct Animator {
    acts: HashMap<u64, Acting>,
    /// Seconds on the host's clock at the last update.
    clock: Option<f64>,
    tick: Option<u64>,
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads the newest frame when it is new and moves every act on to
    /// `now`, seconds on any steady clock.
    pub fn update(&mut self, frame: &Frame, now: f64) {
        let dt = self.clock.map_or(0.0, |c| (now - c).clamp(0.0, 0.25) as f32);
        self.clock = Some(now);
        if self.tick != Some(frame.tick) {
            self.tick = Some(frame.tick);
            self.read(frame);
        }
        for a in self.acts.values_mut() {
            a.advance(dt);
        }
    }

    fn read(&mut self, frame: &Frame) {
        let index: HashMap<u64, usize> = frame.creatures.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let near = |id: u64| index.get(&id).map(|&j| reach_point(&frame.creatures[j]));
        for c in &frame.creatures {
            let a = self.acts.entry(c.id).or_insert_with(|| Acting { health: c.health, ..Default::default() });
            let mut g = [0.0; ACTS];
            match c.action {
                Action::Eating => g[EAT] = 1.0,
                Action::Attacking => g[STRIKE] = 1.0,
                Action::Guarding => g[GUARD] = 1.0,
                Action::Fleeing => g[FLEE] = 1.0,
                Action::Sharing => g[GIVE] = 1.0,
                Action::Grooming => g[GROOM] = 1.0,
                Action::Held => g[HELD] = 1.0,
                Action::Mating if a.mate.is_none() => {
                    a.mate = Some(0.0);
                    a.partner = c.target.and_then(near);
                }
                Action::Laying if a.lay.is_none() => a.lay = Some(0.0),
                _ => {}
            }
            g[SICK] =
                c.infection.as_ref().filter(|i| !i.symbiont).map_or(0.0, |i| ((i.load - 0.1) / 0.5).clamp(0.0, 1.0));
            let starving = ((0.2 - c.energy) / 0.2).clamp(0.0, 1.0);
            let worn = ((c.age - 0.6) / 0.4).clamp(0.0, 1.0);
            let wounded = ((0.5 - c.health) / 0.5).clamp(0.0, 1.0);
            g[WEAK] = starving.max(worn).max(wounded);
            // A fresh wound, from a bite or a thorn or poison.
            if c.health < a.health - 0.015 {
                a.hurt = 1.0;
            }
            a.health = c.health;
            a.focus = c.focus.or_else(|| c.target.and_then(near));
            a.from = None;
            a.goal = g;
            a.tick = frame.tick;
        }
        // What others are doing to them.
        for c in &frame.creatures {
            let Some(t) = c.target else { continue };
            let me = reach_point(c);
            let Some(o) = self.acts.get_mut(&t) else { continue };
            match c.action {
                Action::Attacking => {
                    o.from = Some(me);
                    o.goal[DEFEND] = 1.0;
                    o.hurt = o.hurt.max(0.7);
                }
                Action::Sharing => {
                    o.goal[TAKE] = 1.0;
                    o.focus = Some(me);
                }
                Action::Grooming => o.goal[GROOMED] = 1.0,
                Action::Mating if o.mate.is_none() => {
                    o.mate = Some(0.0);
                    o.partner = Some(me);
                }
                _ => {}
            }
        }
        let tick = frame.tick;
        self.acts.retain(|_, a| a.tick == tick);
    }

    /// Extra glow from what it is doing: warning flashes, a mating flare.
    pub fn signal(&self, c: &CreatureView) -> f32 {
        let Some(a) = self.acts.get(&c.id) else { return c.flash };
        let t = self.time() + seed(c.id) * 10.0;
        let mut glow = a.w[GUARD].max(a.w[DEFEND] * 0.6) * 0.6 * on((t * TAU * 2.2).sin() - 0.2);
        if let Some(m) = a.mate {
            glow = glow.max(clip_env(m, MATE_SECS) * (0.55 + 0.45 * (t * TAU * 1.4).sin()));
        }
        if let Some(l) = a.lay {
            glow = glow.max(clip_env(l, LAY_SECS) * 0.35 * (l * TAU * 1.1).sin().max(0.0));
        }
        glow = glow.max(a.w[STRIKE] * 0.35 * bump((t * 1.6).fract(), 0.55, 0.75));
        glow = glow.max(a.w[HELD] * 0.5 * on((t * 23.0).sin() - 0.4));
        c.flash.max(glow.min(1.0))
    }

    fn time(&self) -> f32 {
        // Wrapped so sines keep their precision on a host that runs for weeks.
        (self.clock.unwrap_or(0.0) % 3600.0) as f32
    }

    /// Poses a living creature as `pose::creature` does, acting out what it is doing.
    pub fn creature(&self, c: &CreatureView, t: f32, out: &mut Pose) {
        let (Some(mut m), Some(a)) = (pose::sample(&c.motion, t), self.acts.get(&c.id)) else {
            return pose::creature(c, t, out);
        };
        out.clear();
        let mut swell = [1.0f32; 12];
        if let Some(mut p) = Puppet::new(&c.body, &mut m) {
            let view = View::of(p.moment_pos, p.heading, p.face);
            let local = |q: Option<[f32; 3]>| q.map(|q| view.local(q));
            let time = self.time();
            act(&mut p, a, c, time, local(a.focus), local(a.from), local(a.partner), &mut swell);
            p.finish(true);
        }
        pose::physical(&c.body, &m, |_, _| true, pose::wounds(c), out);
        swollen(out, &swell);
    }

    /// Poses a body as `pose::remains` does, with the spasms of a fresh death.
    pub fn remains(&self, r: &RemainsView, t: f32, out: &mut Pose) {
        let Some(mut m) = pose::sample(&r.motion, t) else { return pose::remains(r, t, out) };
        out.clear();
        if let Some(mut p) = Puppet::new(&r.body, &mut m) {
            throes(&mut p, r, self.time());
            p.finish(false);
        }
        let d = r.decay;
        pose::physical(&r.body, &m, |_, s| d < pose::lasts(s.tissue), |_, _| (0.3 + d).min(1.0), out);
    }
}

/// Where to reach for another creature: a little above its feet.
fn reach_point(c: &CreatureView) -> [f32; 3] {
    [c.pos[0], c.pos[1], c.pos[2] + c.scale * c.body.extent * 0.25]
}

/// Maps points in the world into a body's own upright plane, as `pose` draws it.
struct View {
    at: [f32; 2],
    shrink: f32,
    fore: f32,
}

impl View {
    fn of(pos: [f32; 2], heading: f32, face: f32) -> Self {
        let (at, shrink, _) = pose::project([pos[0], pos[1] / DEPTH, 0.0]);
        let fore = face * heading.cos().abs().max(0.45);
        let fore = if fore.abs() < 0.2 { 0.2f32.copysign(fore) } else { fore };
        Self { at, shrink, fore }
    }

    fn local(&self, q: [f32; 3]) -> [f32; 2] {
        let (s, _, _) = pose::project(q);
        [(s[0] - self.at[0]) / (self.fore * self.shrink), (s[1] - self.at[1]) / self.shrink]
    }
}

#[derive(Clone, Copy)]
enum Pivot {
    Centre,
    /// Its rearmost low point, to rear up on.
    Hind,
    /// Its foremost low point, to squat back from.
    Front,
}

/// A posed body that acts can bend. Joint bends gather first, then are laid
/// out from the root outward, then the whole body tilts and moves.
struct Puppet<'a> {
    body: &'a Body,
    parts: &'a mut Vec<PartPose>,
    moment_pos: [f32; 2],
    heading: f32,
    face: f32,
    scale: f32,
    /// Index into `parts` of each segment, if it is there.
    at: Vec<Option<usize>>,
    bend: Vec<f32>,
    turns: Vec<(f32, Pivot)>,
    push: [f32; 2],
    /// Upward, after its feet are put back on the ground: a hop.
    hop: f32,
    /// +1 if its head points along +u in its plane, -1 if back along it.
    fwd: f32,
    /// Its length, world units.
    len: f32,
    /// How much of its length is neck and head, 0..1.
    neck: f32,
    roles: Roles,
    low: f32,
}

#[derive(Default)]
struct Roles {
    head: Option<usize>,
    /// Segments from just below the root out to the head.
    chain: Vec<usize>,
    mouths: Vec<usize>,
    tail: Vec<usize>,
    /// Legs, and whether each is a hind one.
    legs: Vec<(usize, bool)>,
    /// Fins, sails, floats and spines: what spreads to look bigger.
    fans: Vec<usize>,
    spines: Vec<usize>,
}

impl<'a> Puppet<'a> {
    fn new(body: &'a Body, m: &'a mut Moment) -> Option<Self> {
        let n = body.segments.len();
        if n == 0 || m.parts.is_empty() {
            return None;
        }
        let mut at = vec![None; n];
        for (k, p) in m.parts.iter().enumerate() {
            if let Some(slot) = at.get_mut(p.seg as usize) {
                *slot = Some(k);
            }
        }
        let mut p = Self {
            body,
            moment_pos: m.pos,
            heading: m.heading,
            face: m.face,
            scale: m.scale,
            parts: &mut m.parts,
            at,
            bend: vec![0.0; n],
            turns: Vec::new(),
            push: [0.0; 2],
            hop: 0.0,
            fwd: 1.0,
            len: body.extent * m.scale,
            neck: 0.0,
            roles: Roles::default(),
            low: 0.0,
        };
        p.roles = p.cast();
        p.fwd = p.forward();
        p.neck = p.roles.chain.iter().map(|&s| body.segments[s].length).sum::<f32>() / body.extent.max(1e-4);
        p.low = p.lowest();
        Some(p)
    }

    fn has(&self, s: usize) -> bool {
        self.at[s].is_some()
    }

    fn part(&self, s: usize) -> &PartPose {
        &self.parts[self.at[s].unwrap()]
    }

    fn half(&self, s: usize) -> f32 {
        self.body.segments[s].length * self.scale * 0.5
    }

    fn base(&self, s: usize) -> [f32; 2] {
        let p = self.part(s);
        let h = self.half(s);
        [p.u - p.angle.cos() * h, p.alt - p.angle.sin() * h]
    }

    fn tip(&self, s: usize) -> [f32; 2] {
        let p = self.part(s);
        let h = self.half(s);
        [p.u + p.angle.cos() * h, p.alt + p.angle.sin() * h]
    }

    fn centre(&self) -> [f32; 2] {
        let n = self.parts.len().max(1) as f32;
        let (u, a) = self.parts.iter().fold((0.0, 0.0), |(u, a), p| (u + p.u, a + p.alt));
        [u / n, a / n]
    }

    fn lowest(&self) -> f32 {
        (0..self.at.len())
            .filter(|&s| self.has(s))
            .map(|s| self.base(s)[1].min(self.tip(s)[1]))
            .fold(f32::MAX, f32::min)
    }

    /// Picks out head, neck, mouths, tail, legs and fins from the body at rest.
    fn cast(&self) -> Roles {
        let segs = &self.body.segments;
        let mut r = Roles::default();
        let present = |s: usize| self.has(s);
        let front = |kind: Option<Tissue>| {
            (0..segs.len())
                .filter(|&s| present(s) && kind.is_none_or(|k| segs[s].tissue == k))
                .max_by(|&a, &b| segs[a].tip[0].total_cmp(&segs[b].tip[0]))
        };
        r.head = front(Some(Tissue::Mouth)).or_else(|| front(Some(Tissue::Sense))).or_else(|| front(None));
        let mut in_chain = vec![false; segs.len()];
        if let Some(h) = r.head {
            let mut s = h as i16;
            while s >= 0 && segs[s as usize].parent >= 0 {
                r.chain.push(s as usize);
                in_chain[s as usize] = true;
                s = segs[s as usize].parent;
            }
            r.chain.reverse();
        }
        let extent = self.body.extent;
        for (i, s) in segs.iter().enumerate() {
            if !present(i) || s.parent < 0 || in_chain[i] {
                continue;
            }
            if s.tissue == Tissue::Mouth {
                r.mouths.push(i);
            }
            let (sin, cos) = s.dir.sin_cos();
            if s.tissue == Tissue::Spine {
                r.spines.push(i);
                r.fans.push(i);
            } else if matches!(s.tissue, Tissue::LightFilm | Tissue::Float) || s.membrane {
                r.fans.push(i);
            } else if sin < -0.45 {
                r.legs.push((i, s.base[0] < 0.0));
            } else if s.side == 0 && cos < -0.4 && s.base[0] < extent * 0.1 {
                r.tail.push(i);
            }
        }
        if let Some(h) = r.head
            && segs[h].tissue == Tissue::Mouth
            && segs[h].parent >= 0
        {
            r.mouths.push(h);
        }
        r
    }

    fn forward(&self) -> f32 {
        let root = (0..self.at.len()).find(|&s| self.has(s) && self.body.segments[s].parent < 0);
        let way = match (self.roles.head, root) {
            (Some(h), Some(r)) if h != r => self.tip(h)[0] - self.part(r).u,
            (_, Some(r)) => self.part(r).angle.cos(),
            _ => 1.0,
        };
        if way < 0.0 { -1.0 } else { 1.0 }
    }

    /// Raises the head (lowers it if negative): a long neck bends, a body
    /// with little or none tilts on its hind or front feet.
    fn nod(&mut self, d: f32) {
        let body = (1.0 - self.neck * 2.5).clamp(0.2, 1.0);
        self.spread(d * self.fwd * (1.0 - body * 0.5));
        let about = if d > 0.0 { Pivot::Hind } else { Pivot::Front };
        self.turns.push((d * self.fwd * body * 0.55, about));
    }

    fn spread(&mut self, d: f32) {
        if self.roles.chain.is_empty() {
            self.turns.push((d * 0.5, Pivot::Centre));
        } else {
            let k = d / self.roles.chain.len() as f32;
            for i in 0..self.roles.chain.len() {
                self.bend[self.roles.chain[i]] += k;
            }
        }
    }

    /// Turns the head toward a point in its plane, by `k` of the way.
    fn aim(&mut self, q: [f32; 2], k: f32) {
        let Some(h) = self.roles.head else { return };
        let pivot = self.roles.chain.first().map_or_else(|| self.centre(), |&c| self.base(c));
        let tip = self.tip(h);
        let now = (tip[1] - pivot[1]).atan2(tip[0] - pivot[0]);
        let want = (q[1] - pivot[1]).atan2(q[0] - pivot[0]);
        let d = wrap_angle(want - now);
        // Behind it: it would have to turn round first.
        if d.abs() < 2.2 {
            self.spread(d.clamp(-0.9, 0.9) * k);
        }
    }

    /// Spreads a segment away from its parent's line (folds it in if negative).
    fn fan(&mut self, s: usize, d: f32) {
        let parent = self.body.segments[s].parent;
        let away = if parent >= 0 && self.has(parent as usize) {
            wrap_angle(self.part(s).angle - self.part(parent as usize).angle)
        } else {
            0.0
        };
        let sign = if away.abs() < 0.05 { self.fwd } else { away.signum() };
        self.bend[s] += d * sign;
    }

    /// Lets a segment hang toward the ground (lifts it if negative).
    fn droop(&mut self, s: usize, d: f32) {
        let c = self.part(s).angle.cos();
        if c.abs() > 0.2 {
            self.bend[s] -= d * c.signum();
        }
    }

    fn gape(&mut self, d: f32) {
        if self.roles.mouths.is_empty() {
            // One piece of jaw: the head snaps up a little instead.
            self.nod(d * 0.25);
        }
        for i in 0..self.roles.mouths.len() {
            let m = self.roles.mouths[i];
            self.fan(m, d);
        }
    }

    fn fans(&mut self, d: f32) {
        for i in 0..self.roles.fans.len() {
            let f = self.roles.fans[i];
            self.fan(f, d);
        }
    }

    fn spines(&mut self, d: f32) {
        for i in 0..self.roles.spines.len() {
            let f = self.roles.spines[i];
            self.fan(f, d);
        }
    }

    fn tail(&mut self, lift: f32) {
        let n = self.roles.tail.len().max(1) as f32;
        for i in 0..self.roles.tail.len() {
            let t = self.roles.tail[i];
            self.droop(t, -lift / n.sqrt());
        }
    }

    /// Swings legs forward (back if negative): hind and fore separately.
    fn legs(&mut self, hind: f32, fore: f32) {
        for i in 0..self.roles.legs.len() {
            let (l, is_hind) = self.roles.legs[i];
            self.bend[l] += if is_hind { hind } else { fore } * self.fwd;
        }
    }

    /// Curls every joint in toward its parent, as a body clenches.
    fn curl(&mut self, d: f32) {
        for s in 0..self.bend.len() {
            if self.has(s) && self.body.segments[s].parent >= 0 {
                self.fan(s, -d);
            }
        }
    }

    /// Shakes every joint, each at its own rate.
    fn shake(&mut self, amp: f32, hz: f32, time: f32, salt: f32) {
        for s in 0..self.bend.len() {
            if self.body.segments[s].parent >= 0 {
                let own = hash(s as u64 * 7919 + (salt * 1000.0) as u64);
                self.bend[s] += amp * (time * TAU * hz * (0.7 + 0.6 * own) + s as f32 * 2.1 + salt * 9.0).sin();
            }
        }
    }

    fn turn(&mut self, d: f32, about: Pivot) {
        self.turns.push((d, about));
    }

    /// Moves the body forward along its plane (back if negative).
    fn lunge(&mut self, d: f32) {
        self.push[0] += d * self.fwd;
    }

    /// Lays the bends out from the root outward, tilts and moves the body,
    /// and keeps whatever was on the ground on it.
    fn finish(&mut self, alive: bool) {
        let n = self.bend.len();
        let mut sub = vec![0u64; n];
        for i in (0..n).rev() {
            sub[i] |= 1 << i;
            let p = self.body.segments[i].parent;
            if p >= 0 {
                sub[p as usize] |= sub[i];
            }
        }
        for s in 0..n {
            let d = self.bend[s];
            if d.abs() < 1e-5 || !self.has(s) {
                continue;
            }
            let pivot = self.base(s);
            self.rotate(sub[s], pivot, d);
        }
        for k in 0..self.turns.len() {
            let (d, about) = self.turns[k];
            let pivot = self.pivot(about);
            self.rotate(u64::MAX, pivot, d);
        }
        for p in self.parts.iter_mut() {
            p.u += self.push[0];
            p.alt += self.push[1];
        }
        let low = self.lowest();
        // Feet stay on the ground as the body moves on them; nothing sinks into it.
        let lift =
            if alive && self.low < self.len * 0.15 { self.low - low } else { (self.low.min(0.0) - low).max(0.0) };
        let lift = lift + self.hop;
        for p in self.parts.iter_mut() {
            p.alt += lift;
        }
    }

    fn pivot(&self, about: Pivot) -> [f32; 2] {
        let way = match about {
            Pivot::Centre => return self.centre(),
            Pivot::Hind => -self.fwd,
            Pivot::Front => self.fwd,
        };
        // The furthest point that way among those near the ground.
        let low = self.lowest();
        let mut best = self.centre();
        let mut score = f32::MIN;
        for s in 0..self.at.len() {
            if !self.has(s) {
                continue;
            }
            for q in [self.base(s), self.tip(s)] {
                let v = q[0] * way - (q[1] - low) * 2.0;
                if v > score {
                    score = v;
                    best = q;
                }
            }
        }
        best
    }

    fn rotate(&mut self, mask: u64, pivot: [f32; 2], d: f32) {
        let (s, c) = d.sin_cos();
        for p in self.parts.iter_mut() {
            if (p.seg as usize) < 64 && mask & (1 << p.seg) == 0 {
                continue;
            }
            let (x, y) = (p.u - pivot[0], p.alt - pivot[1]);
            p.u = pivot[0] + x * c - y * s;
            p.alt = pivot[1] + x * s + y * c;
            p.angle = wrap_angle(p.angle + d);
        }
    }
}

/// Acts out everything a creature is doing at once, each by its weight.
#[allow(clippy::too_many_arguments)]
fn act(
    p: &mut Puppet,
    a: &Acting,
    c: &CreatureView,
    time: f32,
    focus: Option<[f32; 2]>,
    from: Option<[f32; 2]>,
    partner: Option<[f32; 2]>,
    swell: &mut [f32; 12],
) {
    let w = &a.w;
    let salt = seed(c.id);
    let t = time + salt * 37.0;
    let len = p.len;
    let mate = a.mate.map_or(0.0, |m| clip_env(m, MATE_SECS));
    let lay = a.lay.map_or(0.0, |l| clip_env(l, LAY_SECS));
    let busy = (w[..SICK].iter().sum::<f32>() + mate + lay + a.hurt).min(1.0);
    let toward = |q: Option<[f32; 2]>, fwd: f32| q.map_or(fwd, |q| if q[0] < 0.0 { -1.0 } else { 1.0 });
    let mut grow = |t: Tissue, k: f32| swell[t.index()] *= k;

    // Always: it breathes, and when nothing else is going on it looks about.
    let breath = (t * TAU * 0.28).sin();
    grow(Tissue::Gut, 1.0 + 0.04 * breath);
    grow(Tissue::Store, 1.0 + 0.03 * breath);
    let calm = 1.0 - busy;
    if calm > 0.0 {
        p.nod(0.14 * calm * wander(t * 0.35));
        p.fans(0.07 * calm * (t * TAU * 0.45).sin());
    }

    // Feeding: the head goes down to the food and bites, yanking back to tear.
    let e = w[EAT];
    if e > 0.01 {
        match focus {
            Some(f) => p.aim(f, 0.85 * e),
            None => p.nod(-0.2 * e),
        }
        let chomp = (t * TAU * 2.4).sin();
        p.gape((0.12 + 0.4 * chomp.max(0.0)) * e);
        let tug = (t * TAU * 0.9).sin().max(0.0).powi(3);
        p.nod(0.3 * tug * e);
        p.lunge(-0.05 * len * tug * e);
        grow(Tissue::Gut, 1.0 + 0.12 * e);
    }

    // Attacking: wind up, lunge, snap, shake what it has hold of.
    let k = w[STRIKE];
    if k > 0.01 {
        let ph = (t * 1.6).fract();
        let l = strike(ph);
        if let Some(f) = focus {
            p.aim(f, 0.6 * k);
        }
        let way = toward(focus, p.fwd) * p.fwd;
        p.nod(-0.45 * l * k);
        p.lunge(0.3 * len * l * k * way);
        p.gape(0.75 * k * bump(ph, 0.35, 0.64));
        p.legs(0.15 * l * k, 0.65 * l * k);
        p.spines(0.3 * k);
        for i in 0..p.roles.tail.len() {
            let s = p.roles.tail[i];
            p.bend[s] += 0.35 * k * (t * TAU * 1.6 + i as f32).sin();
        }
        p.turn(0.09 * k * (t * TAU * 7.0).sin() * l.max(0.0), Pivot::Centre);
    }

    // Bitten: it flinches away, head jerking up, body clenching.
    let h = a.hurt;
    if h > 0.01 {
        let away = -toward(from, p.fwd) * p.fwd;
        p.lunge(0.12 * len * h * away);
        p.hop += 0.08 * len * h;
        p.nod(0.35 * h);
        p.curl(0.18 * h);
        p.turn(0.1 * h * away * p.fwd, Pivot::Centre);
    }

    // Defending against an attacker: face it, gape, bristle, snap back.
    let d = w[DEFEND] * (1.0 - w[STRIKE]);
    if d > 0.01 {
        if let Some(f) = from {
            p.aim(f, 0.7 * d);
        }
        let ph = (t * 2.3).fract();
        let l = strike(ph);
        p.nod(-0.25 * l * d);
        p.lunge(0.1 * len * l * d * toward(from, p.fwd) * p.fwd);
        p.gape(0.6 * d * (0.5 + 0.5 * bump(ph, 0.3, 0.65)));
        p.spines(0.6 * d);
        p.fans(0.3 * d);
        p.legs(-0.15 * d, 0.3 * d);
    }

    // Guarding: a threat display. It rears, spreads, puffs up and gapes.
    let g = w[GUARD];
    if g > 0.01 {
        p.turn(0.26 * g * p.fwd, Pivot::Hind);
        p.legs(-0.3 * g, 0.55 * g);
        p.gape(0.45 * g * (0.8 + 0.2 * (t * TAU * 0.7).sin()));
        p.spines(0.65 * g);
        p.fans(0.45 * g);
        p.tail(0.45 * g);
        p.turn(0.05 * g * (t * TAU * 0.5).sin(), Pivot::Centre);
        grow(Tissue::Float, 1.0 + 0.25 * g);
        grow(Tissue::Spine, 1.0 + 0.1 * g);
    }

    // Fleeing: low and streamlined, everything folded back.
    let f = w[FLEE];
    if f > 0.01 {
        p.turn(-0.18 * f * p.fwd, Pivot::Centre);
        p.nod(0.1 * f);
        p.fans(-0.35 * f);
        p.spines(-0.3 * f);
        p.tail(0.15 * f);
    }

    // Sharing: it leans over the other and heaves food up from its gut.
    let gv = w[GIVE];
    if gv > 0.01 {
        if let Some(q) = focus {
            p.aim(q, 0.7 * gv);
            p.lunge(0.06 * len * gv * toward(focus, p.fwd) * p.fwd);
        }
        p.nod(-0.15 * gv);
        let heave = pulse(t * 0.75);
        p.gape((0.25 + 0.5 * heave) * gv);
        p.curl(0.15 * heave * gv);
        grow(Tissue::Gut, 1.0 + 0.3 * heave * gv);
        grow(Tissue::Store, 1.0 + 0.15 * heave * gv);
    }

    // Being fed: crouched low, face up to the giver, mouth open and quivering.
    let tk = w[TAKE];
    if tk > 0.01 {
        if let Some(q) = focus {
            p.aim(q, 0.8 * tk);
        }
        p.nod(0.2 * tk);
        p.gape((0.3 + 0.2 * (t * TAU * 5.0).sin()) * tk);
        p.turn(0.05 * tk * (t * TAU * 2.4).sin(), Pivot::Centre);
        p.legs(0.3 * tk, 0.3 * tk);
        p.fans(-0.2 * tk);
    }

    // Grooming: the head works slowly over the other, nibbling.
    let gr = w[GROOM];
    if gr > 0.01 {
        if let Some(q) = focus {
            p.aim(q, 0.6 * gr);
        }
        p.nod(0.3 * gr * (t * TAU * 0.4).sin());
        p.gape(0.22 * gr * (t * TAU * 3.2).sin().abs());
    }

    // Being groomed: it settles, head low, fins open.
    let gd = w[GROOMED];
    if gd > 0.01 {
        p.nod(-0.15 * gd);
        p.fans(0.25 * gd);
        p.legs(0.15 * gd, 0.15 * gd);
    }

    // Caught: it thrashes, weaker as it fails.
    let hd = w[HELD] * (0.4 + 0.6 * c.health);
    if hd > 0.01 {
        p.shake(0.55 * hd, 2.6, t, salt);
        p.turn(0.15 * hd * (t * TAU * 1.3).sin(), Pivot::Centre);
        p.gape(0.6 * hd * (t * TAU * 1.1).sin().abs());
    }

    // Sick: drooping and shivering, with fits of retching.
    let sk = w[SICK];
    if sk > 0.01 {
        p.nod(-0.35 * sk);
        p.tail(-0.25 * sk);
        for i in 0..p.roles.fans.len() {
            let s = p.roles.fans[i];
            p.droop(s, 0.3 * sk);
        }
        p.shake(0.05 * sk, 9.0, t, salt);
        let period = 2.5 + 2.0 * salt;
        let tau = t % period;
        let fit = if tau < 0.12 { tau / 0.12 } else { (-(tau - 0.12) * 4.5).exp() } * sk;
        p.curl(0.3 * fit);
        p.nod(-0.35 * fit + 0.25 * (tau * 30.0).sin() * fit);
        p.gape(0.65 * fit);
        p.lunge(-0.04 * len * fit);
        grow(Tissue::Gut, 1.0 + 0.3 * fit + 0.05 * sk * (t * TAU * 0.8).sin());
    }

    // Starving, badly hurt or old: it sags and trembles.
    let wk = w[WEAK] * (1.0 - sk * 0.5);
    if wk > 0.01 {
        p.nod(-0.3 * wk);
        for i in 0..p.roles.fans.len() {
            let s = p.roles.fans[i];
            p.droop(s, 0.25 * wk);
        }
        p.tail(-0.2 * wk);
        p.legs(0.15 * wk, 0.15 * wk);
        p.shake(0.025 * wk, 6.0, t, salt);
    }

    // Mating: the pair press together and rock, fins quivering, brood swelling.
    if mate > 0.01 {
        if let Some(q) = partner {
            p.aim(q, 0.6 * mate);
            // Close in until they touch, not through each other.
            let gap = q[0].abs();
            let close = ((gap - 0.8 * len) * 0.5).clamp(0.0, 0.3 * len);
            p.lunge(close * mate * toward(partner, p.fwd) * p.fwd);
        }
        let rock = (t * TAU * 1.4).sin();
        let way = toward(partner, p.fwd) * p.fwd;
        p.lunge(0.06 * len * rock * mate * way);
        p.turn(0.1 * rock * mate * p.fwd, Pivot::Hind);
        p.tail(0.45 * mate);
        p.fans(mate * (0.35 + 0.1 * (t * TAU * 6.0).sin()));
        grow(Tissue::Brood, 1.0 + 0.4 * mate);
        grow(Tissue::Glow, 1.0 + 0.3 * mate);
    }

    // Laying or budding: it squats, tail up, and strains.
    if lay > 0.01 {
        let lt = a.lay.unwrap_or(0.0);
        let strain = (lt * TAU * 1.1).sin().max(0.0).powi(2);
        p.legs(0.6 * lay, 0.2 * lay);
        p.tail(0.5 * lay);
        p.turn(0.15 * lay * p.fwd, Pivot::Front);
        p.curl(0.15 * strain * lay);
        p.shake(0.03 * lay, 8.0, t, salt);
        grow(Tissue::Brood, 1.0 + 0.5 * lay * (1.0 - lt / LAY_SECS) + 0.15 * strain);
    }
}

/// A fresh corpse spasms, then the jaw falls open and the limbs curl.
fn throes(p: &mut Puppet, r: &RemainsView, time: f32) {
    let salt = seed(r.id);
    let t = time + salt * 37.0;
    let age = r.age;
    let fresh = 1.0 - r.decay * 2.0;
    if fresh <= 0.0 {
        return;
    }
    if age < THROES_SECS {
        let e = (1.0 - age / THROES_SECS).powi(2) * fresh;
        // Spasms come in bursts.
        let burst = 0.25 + 0.75 * ((t * 1.9).sin() * 0.5 + 0.5).powi(3);
        p.shake(0.7 * e * burst, 3.5, t, salt);
        // One last great clench as it dies.
        p.curl(0.5 * bump(age / 0.9, 0.0, 1.0) * fresh);
    }
    let slack = (age / 2.0).min(1.0) * fresh;
    p.gape(0.45 * slack);
    let rigor = (age / 10.0).min(1.0) * fresh;
    for i in 0..p.roles.legs.len() {
        let (l, _) = p.roles.legs[i];
        p.fan(l, -0.35 * rigor);
    }
}

fn swollen(out: &mut Pose, swell: &[f32; 12]) {
    for s in &mut out.segs {
        let k = swell[s.tissue.index()];
        s.ra *= k;
        s.rb *= k;
    }
}

/// A lunge through one cycle: back to wind up (to -0.35), a fast strike to
/// full reach (1), then a slow recovery.
fn strike(ph: f32) -> f32 {
    let ease = |x: f32| x * x * (3.0 - 2.0 * x);
    if ph < 0.5 {
        -0.35 * ease(ph / 0.5)
    } else if ph < 0.62 {
        -0.35 + 1.35 * ease((ph - 0.5) / 0.12)
    } else {
        1.0 - ease((ph - 0.62) / 0.38)
    }
}

/// 0 outside a..b, rising to 1 halfway through.
fn bump(x: f32, a: f32, b: f32) -> f32 {
    if x <= a || x >= b { 0.0 } else { ((x - a) / (b - a) * std::f32::consts::PI).sin() }
}

/// A sharp heave once a cycle.
fn pulse(x: f32) -> f32 {
    (x * TAU).sin().max(0.0).powi(3)
}

fn on(x: f32) -> f32 {
    if x > 0.0 { 1.0 } else { 0.0 }
}

/// Easing in and out of a clip of `secs` seconds.
fn clip_env(t: f32, secs: f32) -> f32 {
    (t / 0.6).min(1.0).min((secs - t).max(0.0))
}

/// Slow, smooth, unrepeating wandering, about -1..1.
fn wander(x: f32) -> f32 {
    ((x).sin() + (x * 2.31 + 1.3).sin() * 0.6) / 1.6
}

fn seed(id: u64) -> f32 {
    hash(id)
}

fn hash(x: u64) -> f32 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}
