//! Brains: small recurrent neural networks grown from the genome, which keep
//! learning through life. Evolution sets the wiring a creature is born with;
//! experience rewires it.
//!
//! Learning works the way it seems to in animals. Each brain carries a critic
//! that predicts how well things are about to go. What drives learning is not
//! pleasure or pain itself but surprise: the gap between what happened and
//! what the critic expected, a dopamine-like signal. Outputs wander a little
//! on their own, so a brain tries things, and whichever wandering came just
//! before a pleasant surprise is strengthened. Synapses keep a fading trace of
//! what fired together, so credit reaches back over the last few seconds, and
//! even the hidden layer reshapes itself by a Hebbian rule that surprise
//! gates. How fast each part learns is itself evolved, and an output neuron
//! of its own can turn learning up or down, so evolution can make brains
//! that learn only when it pays.
//!
//! What is learned comes in two speeds. New changes are fast and fragile and
//! fade within minutes unless they are consolidated, which happens mostly at
//! rest, as in sleep. Consolidated learning lasts for hours, slowly relaxing
//! back towards instinct.

use crate::genome::BrainGenes;
use serde::{Deserialize, Serialize};

/// Sensor inputs.
pub mod i {
    pub const BIAS: usize = 0;
    pub const ENERGY: usize = 1;
    pub const HUNGER: usize = 2;
    pub const HEALTH: usize = 3;
    pub const PAIN: usize = 4;
    pub const AGE: usize = 5;
    /// Grown, fed and ready to breed.
    pub const READY: usize = 6;
    pub const LIGHT: usize = 7;
    /// Too hot is positive, too cold negative.
    pub const HEAT: usize = 8;
    pub const WET: usize = 9;
    pub const ALT: usize = 10;
    /// Nearest food: closeness 0..1, which way (+ ahead), how good it has tasted.
    pub const FOOD_NEAR: usize = 11;
    pub const FOOD_DIR: usize = 12;
    pub const FOOD_TASTE: usize = 13;
    /// Nearest other creature.
    pub const OTHER_NEAR: usize = 14;
    pub const OTHER_DIR: usize = 15;
    /// Its size relative to mine, log scale.
    pub const OTHER_SIZE: usize = 16;
    pub const OTHER_KIN: usize = 17;
    /// What I remember of it: -1 enemy .. 1 friend.
    pub const OTHER_BOND: usize = 18;
    /// Its dominance relative to mine.
    pub const OTHER_RANK: usize = 19;
    /// How good its kind has tasted.
    pub const OTHER_TASTE: usize = 20;
    /// It is attacking something right now.
    pub const OTHER_FIGHTING: usize = 21;
    /// The most dangerous thing in sight.
    pub const THREAT: usize = 22;
    pub const THREAT_DIR: usize = 23;
    /// Light signals understood from others nearby.
    pub const HEARD_DANGER: usize = 24;
    pub const HEARD_FOOD: usize = 25;
    pub const HEARD_MATE: usize = 26;
    pub const HEARD_THREAT: usize = 27;
    /// How many of my own kind are close.
    pub const CROWD: usize = 28;
    /// My eggs or young nearby, and how threatened they are.
    pub const YOUNG: usize = 29;
    /// Which way my remembered feeding ground lies for this season.
    pub const HOME_DIR: usize = 30;
    pub const NOISE: usize = 31;
    /// A call to gather, and a warning of danger from above.
    pub const HEARD_GATHER: usize = 32;
    pub const HEARD_ALOFT: usize = 33;
    /// Moods: fear, curiosity, loneliness and tiredness, 0..1.
    pub const FEAR: usize = 34;
    pub const CURIOUS: usize = 35;
    pub const LONELY: usize = 36;
    pub const TIRED: usize = 37;
    /// How rough the weather is here: rain, dust, ash, a cold snap.
    pub const STORM: usize = 38;
    /// Which way the nearest cover lies, and how covered it is here.
    pub const SHELTER_DIR: usize = 39;
    pub const SHELTERED: usize = 40;
    /// The best remembered place to feed: how good it is, which way.
    pub const PLACE_FOOD: usize = 41;
    pub const PLACE_FOOD_DIR: usize = 42;
    /// A remembered place of danger: how close and bad, which way.
    pub const PLACE_DANGER: usize = 43;
    pub const PLACE_DANGER_DIR: usize = 44;
    /// Which way my friends and kin are gathered.
    pub const GROUP_DIR: usize = 45;
    /// A friend or kin is being attacked nearby.
    pub const ALLY_HURT: usize = 46;
    /// Something my allies are fighting: how close, which way.
    pub const QUARRY: usize = 47;
    pub const QUARRY_DIR: usize = 48;
    /// The nearest other is making a threat display.
    pub const OTHER_DISPLAY: usize = 49;
    /// How unfamiliar this stretch of land is.
    pub const NOVELTY: usize = 50;
    /// The threat is above (+) or below (-) me.
    pub const THREAT_ALT: usize = 51;
}

/// Motor and action outputs.
pub mod o {
    pub const THRUST: usize = 0;
    pub const TURN: usize = 1;
    pub const RISE: usize = 2;
    pub const SEEK_FOOD: usize = 3;
    /// Positive approaches the nearest other, negative avoids it.
    pub const SEEK_OTHER: usize = 4;
    pub const FLEE: usize = 5;
    pub const HOME: usize = 6;
    pub const EAT: usize = 7;
    pub const ATTACK: usize = 8;
    pub const MATE: usize = 9;
    pub const SHARE: usize = 10;
    pub const GROOM: usize = 11;
    pub const GUARD: usize = 12;
    pub const SIGNAL: usize = 13;
    /// Which meaning to flash: one output per meaning in `mind`.
    pub const SAY: usize = 14;
    pub const LAY: usize = 20;
    /// Keep with friends and kin.
    pub const FOLLOW: usize = 21;
    /// Settle down to rest: cheaper living, and learning consolidates.
    pub const REST: usize = 22;
    /// Head for land it hasn't seen much of.
    pub const EXPLORE: usize = 23;
    /// Head for the best feeding place it remembers.
    pub const RECALL: usize = 24;
    /// A threat display: rear up, gape, bristle, and see who backs down.
    pub const DISPLAY: usize = 25;
    /// Make for cover.
    pub const SHELTER: usize = 26;
    /// Neuromodulation: turns learning up or down.
    pub const MOD: usize = 27;
}

pub const NI: usize = 52;
pub const NO: usize = 28;
const W_MAX: f32 = 4.0;
/// Seconds for fresh learning to fade by half, if not consolidated.
const FAST_HALF_LIFE: f32 = 90.0;
/// Seconds for consolidated learning to relax half way back to instinct.
const SLOW_HALF_LIFE: f32 = 3.0 * 3600.0;
/// Consolidation while awake, relative to at rest.
const AWAKE_CONSOLIDATION: f32 = 0.1;

/// Weights that learning can change: what they are now, how far life has
/// moved them from instinct (quickly and lastingly), and the eligibility
/// trace of what recently fired through them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Plastic {
    /// Instinct plus both kinds of learning.
    w: Vec<f32>,
    fast: Vec<f32>,
    slow: Vec<f32>,
    e: Vec<f32>,
}

impl Plastic {
    fn new(base: Vec<f32>) -> Self {
        let n = base.len();
        Self { w: base, fast: vec![0.0; n], slow: vec![0.0; n], e: vec![0.0; n] }
    }

    /// Moves every weight by `k` times its eligibility.
    fn apply(&mut self, k: f32) {
        for ((w, f), e) in self.w.iter_mut().zip(self.fast.iter_mut()).zip(&self.e) {
            let dw = k * e;
            if dw != 0.0 {
                let nw = (*w + dw).clamp(-W_MAX, W_MAX);
                *f += nw - *w;
                *w = nw;
            }
        }
    }

    fn nudge(&mut self, i: usize, dw: f32) {
        let nw = (self.w[i] + dw).clamp(-W_MAX, W_MAX);
        self.fast[i] += nw - self.w[i];
        self.w[i] = nw;
    }

    /// Fresh learning fades or is consolidated; consolidated learning
    /// relaxes slowly back towards instinct.
    fn settle(&mut self, keep_fast: f32, keep_slow: f32, consolidate: f32) {
        for ((w, f), s) in self.w.iter_mut().zip(self.fast.iter_mut()).zip(self.slow.iter_mut()) {
            let moved = *f * consolidate;
            let lost_fast = (*f - moved) * (1.0 - keep_fast);
            *f -= moved + lost_fast;
            let slow = *s + moved;
            let lost_slow = slow * (1.0 - keep_slow);
            *s = slow - lost_slow;
            *w -= lost_fast + lost_slow;
        }
    }

    fn learned(&self) -> f32 {
        self.fast.iter().zip(&self.slow).map(|(f, s)| (f + s).abs()).sum()
    }
}

/// The learning rule's evolved settings, read from the genome.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rule {
    /// Output layer: trace follows a*pre*post + b*pre + c*post + d; then eta.
    out: [f32; 5],
    /// Hidden layer, the same shape.
    hid: [f32; 5],
    keep: f32,
    gamma: f32,
    critic: f32,
    consolidate: f32,
    explore: f32,
}

impl Rule {
    fn of(g: &BrainGenes) -> Self {
        Self {
            out: g.plasticity,
            hid: g.plasticity_h,
            keep: g.trace_keep,
            gamma: g.gamma,
            critic: g.critic_rate,
            consolidate: g.consolidate,
            explore: g.explore,
        }
    }
}

/// What the rest of the creature asks of the brain this step.
#[derive(Clone, Copy, Debug)]
pub struct Tone {
    /// How freely its outputs wander, 1 normal.
    pub explore: f32,
    /// How readily it learns, 1 normal.
    pub plasticity: f32,
}

impl Default for Tone {
    fn default() -> Self {
        Self { explore: 1.0, plasticity: 1.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brain {
    hidden: usize,
    /// Input to hidden, a row per input.
    ih: Plastic,
    /// Hidden to output, a row per hidden neuron.
    ho: Plastic,
    /// Input straight to output (reflexes), a row per input.
    io: Plastic,
    /// The critic's weights on the hidden neurons and on the inputs.
    vh: Plastic,
    vi: Plastic,
    #[serde(skip)]
    w_hh: Vec<f32>,
    #[serde(skip)]
    b_h: Vec<f32>,
    #[serde(skip)]
    b_o: Vec<f32>,
    #[serde(skip)]
    rule: Rule,
    h: Vec<f32>,
    pub out: [f32; NO],
    /// The wandering added to each output: slow, so it explores rather than twitches.
    wander: [f32; NO],
    input: Vec<f32>,
    /// What the critic expects of the future, last step.
    pub value: f32,
    /// The last prediction error: positive when things went better than expected.
    pub delta: f32,
    /// Recent size of prediction errors, smoothed.
    pub surprise: f32,
    fresh: bool,
    seed: u32,
}

/// tanh to within about 2e-4, without the libm call.
#[inline]
fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-4.97, 4.97);
    let x2 = x * x;
    let p = x * (135135.0 + x2 * (17325.0 + x2 * (378.0 + x2)));
    let q = 135135.0 + x2 * (62370.0 + x2 * (3150.0 + x2 * 28.0));
    p / q
}

fn f(v: &[i8]) -> Vec<f32> {
    v.iter().map(|&x| x as f32 / 32.0).collect()
}

impl Brain {
    pub fn new(g: &BrainGenes) -> Self {
        let seed = g.w_ih.iter().chain(&g.w_ho).fold(0x9e37_79b9u32, |s, &w| s.rotate_left(5) ^ (w as u8 as u32));
        Self {
            hidden: g.hidden,
            ih: Plastic::new(f(&g.w_ih)),
            ho: Plastic::new(f(&g.w_ho)),
            io: Plastic::new(f(&g.w_io)),
            vh: Plastic::new(f(&g.critic_h)),
            vi: Plastic::new(f(&g.critic_i)),
            w_hh: f(&g.w_hh),
            b_h: f(&g.b_h),
            b_o: f(&g.b_o),
            rule: Rule::of(g),
            h: vec![0.0; g.hidden],
            out: [0.0; NO],
            wander: [0.0; NO],
            input: vec![0.0; NI],
            value: 0.0,
            delta: 0.0,
            surprise: 0.0,
            fresh: true,
            seed: seed | 1,
        }
    }

    /// Puts back the fixed wiring and the learning rule a saved brain leaves
    /// out, from its genome.
    pub fn attach(&mut self, g: &BrainGenes) {
        self.w_hh = f(&g.w_hh);
        self.b_h = f(&g.b_h);
        self.b_o = f(&g.b_o);
        self.rule = Rule::of(g);
    }

    pub fn neurons(&self) -> usize {
        self.hidden
    }

    /// How far experience has moved this brain from its instincts.
    pub fn learned(&self) -> f32 {
        self.ih.learned() + self.ho.learned() + self.io.learned()
    }

    fn gauss(&mut self) -> f32 {
        // Sum of four xorshift uniforms: cheap, bounded, near enough normal.
        let mut s = 0.0;
        for _ in 0..4 {
            let mut x = self.seed;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.seed = x;
            s += (x >> 8) as f32 / (1u32 << 24) as f32;
        }
        (s - 2.0) * 1.732
    }

    /// One step of thought. `reward` is what happened to the body since the
    /// last step. Returns the prediction error it learned from.
    pub fn step(&mut self, input: &[f32; NI], reward: f32, tone: Tone) -> f32 {
        let hn = self.hidden;
        let rule = self.rule;
        // Most senses are quiet most of the time, so only the active ones are
        // summed. Every loop below runs along contiguous weights.
        let mut active = [0u8; NI];
        let mut na = 0;
        for (j, &x) in input.iter().enumerate() {
            if x != 0.0 {
                active[na] = j as u8;
                na += 1;
            }
        }
        let active = &active[..na];
        let mut acc = [0.0f32; 32];
        let acc_h = &mut acc[..hn];
        acc_h.copy_from_slice(&self.b_h);
        for &j in active {
            let x = input[j as usize];
            let row = &self.ih.w[j as usize * hn..][..hn];
            for (a, w) in acc_h.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        for (j, &x) in self.h.iter().enumerate() {
            let row = &self.w_hh[j * hn..][..hn];
            for (a, w) in acc_h.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        for (h, a) in self.h.iter_mut().zip(acc_h.iter()) {
            *h = fast_tanh(*a);
        }
        let mut acc_o = [0.0f32; NO];
        acc_o.copy_from_slice(&self.b_o);
        for (k, &x) in self.h.iter().enumerate() {
            let row = &self.ho.w[k * NO..][..NO];
            for (a, w) in acc_o.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        for &j in active {
            let x = input[j as usize];
            let row = &self.io.w[j as usize * NO..][..NO];
            for (a, w) in acc_o.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        // Each output wanders on its own, slowly.
        let sigma = rule.explore * tone.explore;
        for o in 0..NO {
            let n = self.gauss();
            // Violence is tried more warily than anything else.
            let sigma = if o == self::o::ATTACK { sigma * 0.3 } else { sigma };
            self.wander[o] = self.wander[o] * 0.9 + n * sigma * 0.436;
            self.out[o] = fast_tanh(acc_o[o] + self.wander[o]);
        }

        // What the critic expects now, and how wrong it was last step.
        let mut value = 0.0;
        let mut norm = 1.0;
        for (h, v) in self.h.iter().zip(&self.vh.w) {
            value += h * v;
            norm += h * h;
        }
        for &j in active {
            let x = input[j as usize];
            value += x * self.vi.w[j as usize];
            norm += x * x;
        }
        let delta = if self.fresh { 0.0 } else { reward + rule.gamma * value - self.value };
        self.fresh = false;
        self.value = value;
        self.delta = delta;
        self.surprise += (delta.abs().min(2.0) - self.surprise) * 0.05;

        // Learn from the surprise, along what recently fired together.
        let m = delta.clamp(-1.0, 1.0) * tone.plasticity * (1.0 + self.out[o::MOD]);
        if m.abs() > 1e-4 {
            if rule.out[4] > 0.0 {
                self.ho.apply(rule.out[4] * m);
                self.io.apply(rule.out[4] * m);
            }
            if rule.hid[4] > 0.0 {
                self.ih.apply(rule.hid[4] * m);
            }
        }
        let k = rule.critic * delta.clamp(-2.0, 2.0) / norm;
        if k.abs() > 1e-6 {
            self.vh.apply(k);
            self.vi.apply(k);
        }

        // Eligibility: what just fired together. Outputs: the evolved rule,
        // a*pre*post + b*pre + c*post + d, plus whatever wandering the output
        // did, which is what lets surprise find the actions that caused it.
        let [a, b, c, d, _] = rule.out;
        let keep = rule.keep;
        let mut u = [0.0f32; NO];
        let mut v = [0.0f32; NO];
        for o in 0..NO {
            u[o] = a * self.out[o] + b + self.wander[o];
            v[o] = c * self.out[o] + d;
        }
        for (k, &pre) in self.h.iter().enumerate() {
            let row = &mut self.ho.e[k * NO..][..NO];
            for o in 0..NO {
                row[o] = keep * row[o] + pre * u[o] + v[o];
            }
        }
        trace_rows(&mut self.io.e, input, NO, keep, |row, pre| {
            for o in 0..NO {
                row[o] = keep * row[o] + pre * u[o] + v[o];
            }
        });
        let [ah, bh, ch, dh, eta_h] = rule.hid;
        if eta_h > 0.0 {
            let mut uh = [0.0f32; 32];
            let mut vh = [0.0f32; 32];
            for (k, &h) in self.h.iter().enumerate() {
                uh[k] = ah * h + bh;
                vh[k] = ch * h + dh;
            }
            trace_rows(&mut self.ih.e, input, hn, keep, |row, pre| {
                for k in 0..hn {
                    row[k] = keep * row[k] + pre * uh[k] + vh[k];
                }
            });
        }
        // The critic's trace: TD(lambda) over its features.
        let lam = rule.gamma * keep;
        for (e, &h) in self.vh.e.iter_mut().zip(&self.h) {
            *e = lam * *e + h;
        }
        for (e, &x) in self.vi.e.iter_mut().zip(input) {
            *e = lam * *e + x;
        }
        self.input.copy_from_slice(input);
        delta
    }

    /// Lets fresh learning fade or settle in over `dt` seconds. At rest far
    /// more of it is kept, as in sleep.
    pub fn settle(&mut self, dt: f32, resting: bool) {
        let keep_fast = (-dt * std::f32::consts::LN_2 / FAST_HALF_LIFE).exp();
        let keep_slow = (-dt * std::f32::consts::LN_2 / SLOW_HALF_LIFE).exp();
        let rate = self.rule.consolidate * if resting { 1.0 } else { AWAKE_CONSOLIDATION };
        let c = (rate * dt).min(1.0);
        for p in [&mut self.ih, &mut self.ho, &mut self.io, &mut self.vh, &mut self.vi] {
            p.settle(keep_fast, keep_slow, c);
        }
    }

    /// Nudges this brain's outputs toward what another did in the same
    /// moment: learning by watching. `target` is the demonstrator's output.
    pub fn imitate(&mut self, target: &[f32; NO], rate: f32, which: &[usize]) {
        for &o in which {
            let err = target[o] - self.out[o];
            if err.abs() < 0.05 {
                continue;
            }
            let g = rate * err * (1.0 - self.out[o] * self.out[o]);
            for k in 0..self.hidden {
                self.ho.nudge(k * NO + o, g * self.h[k]);
            }
            for j in 0..NI {
                let x = self.input[j];
                if x != 0.0 {
                    self.io.nudge(j * NO + o, g * x * 0.5);
                }
            }
        }
    }
}

/// Updates a trace with a row per input: active rows by `f`, quiet rows just fade.
#[inline]
fn trace_rows(e: &mut [f32], input: &[f32; NI], width: usize, keep: f32, mut f: impl FnMut(&mut [f32], f32)) {
    for (j, &pre) in input.iter().enumerate() {
        let row = &mut e[j * width..][..width];
        if pre == 0.0 {
            for x in row.iter_mut() {
                *x *= keep;
            }
        } else {
            f(row, pre);
        }
    }
}

/// Gives a fresh random brain a few sensible reflexes, so founders can feed
/// and flee before evolution and learning take over, and a critic that
/// expects a fed, healthy, unafraid body to do well.
pub fn seed_reflexes(b: &mut BrainGenes) {
    let mut set = |inp: usize, out: usize, v: f32| {
        b.w_io[inp * NO + out] = (v * 32.0) as i8;
    };
    set(i::HUNGER, o::SEEK_FOOD, 1.5);
    set(i::FOOD_NEAR, o::EAT, 1.8);
    set(i::BIAS, o::EAT, -0.4);
    set(i::BIAS, o::THRUST, 0.4);
    set(i::NOISE, o::TURN, 0.8);
    set(i::PAIN, o::FLEE, 1.6);
    set(i::THREAT, o::FLEE, 1.2);
    set(i::READY, o::MATE, 1.4);
    set(i::READY, o::LAY, 1.4);
    set(i::BIAS, o::LAY, -0.5);
    set(i::BIAS, o::ATTACK, -1.2);
    set(i::BIAS, o::SHARE, -1.5);
    set(i::BIAS, o::GROOM, -1.5);
    set(i::BIAS, o::GUARD, -1.2);
    set(i::BIAS, o::SIGNAL, -1.2);
    set(i::HEAT, o::HOME, 0.5);
    set(i::OTHER_KIN, o::SEEK_OTHER, 0.4);
    set(i::OTHER_KIN, o::ATTACK, -1.5);
    set(i::OTHER_SIZE, o::ATTACK, -0.6);
    set(i::OTHER_TASTE, o::ATTACK, 0.8);
    // Join a fight a friend is in.
    set(i::QUARRY, o::ATTACK, 0.7);
    set(i::ALLY_HURT, o::ATTACK, 0.3);
    // Fear runs from danger, and towards company.
    set(i::FEAR, o::FLEE, 0.6);
    set(i::PLACE_DANGER, o::FLEE, 0.5);
    set(i::FEAR, o::FOLLOW, 0.6);
    set(i::LONELY, o::FOLLOW, 1.0);
    set(i::BIAS, o::FOLLOW, -0.3);
    // Rest when tired and safe.
    set(i::TIRED, o::REST, 1.8);
    set(i::BIAS, o::REST, -0.9);
    set(i::THREAT, o::REST, -2.0);
    set(i::FEAR, o::REST, -1.2);
    set(i::HUNGER, o::REST, -0.8);
    // Explore when curious and unafraid.
    set(i::CURIOUS, o::EXPLORE, 1.2);
    set(i::BIAS, o::EXPLORE, -0.5);
    set(i::FEAR, o::EXPLORE, -0.8);
    // Go back to where food was, when hungry and none is in sight.
    set(i::HUNGER, o::RECALL, 1.0);
    set(i::PLACE_FOOD, o::RECALL, 0.8);
    set(i::FOOD_NEAR, o::RECALL, -1.2);
    set(i::BIAS, o::RECALL, -0.6);
    // Square up to rivals no bigger than itself, rather than fight them.
    set(i::BIAS, o::DISPLAY, -1.3);
    set(i::OTHER_NEAR, o::DISPLAY, 0.6);
    set(i::OTHER_SIZE, o::DISPLAY, -0.6);
    set(i::OTHER_KIN, o::DISPLAY, -0.6);
    set(i::OTHER_DISPLAY, o::DISPLAY, 0.4);
    set(i::READY, o::DISPLAY, 0.3);
    // Take cover from storms and from danger overhead.
    set(i::STORM, o::SHELTER, 1.4);
    set(i::HEARD_ALOFT, o::SHELTER, 1.0);
    set(i::HEAT, o::SHELTER, -0.5);
    set(i::BIAS, o::SHELTER, -0.6);
    set(i::HEARD_ALOFT, o::RISE, -0.8);
    // Cry out at danger: overhead and on the ground in different ways.
    set(i::THREAT, o::SIGNAL, 0.9);
    set(i::THREAT, o::SAY, 0.6);
    set(i::THREAT_ALT, o::SAY, -0.6);
    set(i::THREAT_ALT, o::SAY + 5, 0.9);
    set(i::QUARRY, o::SAY + 4, 0.6);
    set(i::LONELY, o::SAY + 4, 0.4);
    let mut crit = |inp: usize, v: f32| {
        b.critic_i[inp] = (v * 32.0) as i8;
    };
    crit(i::ENERGY, 0.6);
    crit(i::HEALTH, 0.4);
    crit(i::PAIN, -0.6);
    crit(i::THREAT, -0.5);
    crit(i::FEAR, -0.3);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::BrainGenes;
    use crate::rng::Rng;

    fn plain(rng: &mut Rng) -> BrainGenes {
        let mut g = BrainGenes::random(rng, 8);
        g.plasticity = [0.0, 0.0, 0.0, 0.0, 0.08];
        g.plasticity_h = [0.0; 5];
        g.trace_keep = 0.6;
        g.explore = 0.4;
        g.critic_rate = 0.1;
        g
    }

    /// Food on one side or the other, cued by an input. Turning towards it is
    /// rewarded. A brain should learn the cue by trial and surprise.
    #[test]
    fn learns_which_way_to_turn_from_surprise() {
        let mut rng = Rng::new(3);
        let mut wins = [0u32; 2];
        let g = plain(&mut rng);
        let mut b = Brain::new(&g);
        let mut reward = 0.0;
        for trial in 0..6000 {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let mut input = [0.0; NI];
            input[i::BIAS] = 1.0;
            input[i::FOOD_DIR] = side;
            b.step(&input, reward, Tone::default());
            let turned = b.out[o::TURN];
            reward = if turned * side > 0.1 { 0.5 } else { -0.1 };
            if trial >= 5000 {
                wins[(turned * side > 0.1) as usize] += 1;
            }
        }
        let rate = wins[1] as f32 / (wins[0] + wins[1]) as f32;
        assert!(rate > 0.75, "turned the right way only {:.0}% of the time", rate * 100.0);
    }

    /// The critic learns to expect what a cue predicts.
    #[test]
    fn critic_learns_to_expect() {
        let mut rng = Rng::new(9);
        let g = plain(&mut rng);
        let mut b = Brain::new(&g);
        let mut reward = 0.0;
        let mut seen = [0.0f32; 2];
        for t in 0..4000 {
            let good = (t / 7) % 2 == 0;
            let mut input = [0.0; NI];
            input[i::BIAS] = 1.0;
            input[i::FOOD_NEAR] = good as u8 as f32;
            b.step(&input, reward, Tone { explore: 0.0, plasticity: 0.0 });
            reward = if good { 0.2 } else { 0.0 };
            if t > 3000 {
                seen[good as usize] += b.value;
            }
        }
        assert!(seen[1] > seen[0] + 50.0, "values {seen:?}");
    }

    /// Fresh learning fades unless it is consolidated, and rest keeps more.
    #[test]
    fn rest_consolidates() {
        let mut rng = Rng::new(4);
        let mut g = plain(&mut rng);
        g.consolidate = 0.03;
        let mut a = Brain::new(&g);
        let mut input = [0.0; NI];
        input[i::BIAS] = 1.0;
        input[i::HUNGER] = 1.0;
        a.step(&input, 0.0, Tone::default());
        a.step(&input, 1.0, Tone::default());
        a.step(&input, 1.0, Tone::default());
        let mut b = a.clone();
        let start = a.learned();
        assert!(start > 0.0);
        for _ in 0..600 {
            a.settle(1.0, true);
            b.settle(1.0, false);
        }
        assert!(a.learned() > b.learned() * 2.0, "rested {} awake {}", a.learned(), b.learned());
        assert!(b.learned() < start);
    }
}
