//! Brains: small recurrent neural networks grown from the genome, which keep
//! learning through life by a reward-modulated Hebbian rule whose shape is
//! itself evolved. Evolution sets the wiring a creature is born with; pleasure
//! and pain rewire it while it lives.

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
    /// Which meaning to flash: danger, food, mate, threat.
    pub const SAY: usize = 14;
    pub const LAY: usize = 18;
}

pub const NI: usize = 32;
pub const NO: usize = 19;
const W_MAX: f32 = 4.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Brain {
    hidden: usize,
    w_ih: Vec<f32>,
    w_hh: Vec<f32>,
    w_ho: Vec<f32>,
    w_io: Vec<f32>,
    b_h: Vec<f32>,
    b_o: Vec<f32>,
    /// Eligibility traces for the plastic hidden-to-output and reflex weights.
    e_ho: Vec<f32>,
    e_io: Vec<f32>,
    h: Vec<f32>,
    pub out: [f32; NO],
    input: [f32; NI],
    rule: [f32; 5],
    trace_keep: f32,
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
        Self {
            hidden: g.hidden,
            w_ih: f(&g.w_ih),
            w_hh: f(&g.w_hh),
            w_ho: f(&g.w_ho),
            w_io: f(&g.w_io),
            b_h: f(&g.b_h),
            b_o: f(&g.b_o),
            e_ho: vec![0.0; g.hidden * NO],
            e_io: vec![0.0; NI * NO],
            h: vec![0.0; g.hidden],
            out: [0.0; NO],
            input: [0.0; NI],
            rule: g.plasticity,
            trace_keep: g.trace_keep,
        }
    }

    pub fn neurons(&self) -> usize {
        self.hidden
    }

    /// One step of thought.
    pub fn step(&mut self, input: &[f32; NI]) -> &[f32; NO] {
        let hn = self.hidden;
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
            let row = &self.w_ih[j as usize * hn..][..hn];
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
        let mut acc_o = self.out;
        acc_o.copy_from_slice(&self.b_o);
        for (k, &x) in self.h.iter().enumerate() {
            let row = &self.w_ho[k * NO..][..NO];
            for (a, w) in acc_o.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        for &j in active {
            let x = input[j as usize];
            let row = &self.w_io[j as usize * NO..][..NO];
            for (a, w) in acc_o.iter_mut().zip(row) {
                *a += x * w;
            }
        }
        for (o, a) in self.out.iter_mut().zip(acc_o) {
            *o = fast_tanh(a);
        }
        self.input = *input;
        // Eligibility: what just fired together, by the evolved rule
        // a*pre*post + b*pre + c*post + d, split into a part that scales with
        // pre and a part that doesn't.
        let [a, b, c, d, _] = self.rule;
        let keep = self.trace_keep;
        let mut u = [0.0f32; NO];
        let mut v = [0.0f32; NO];
        for o in 0..NO {
            u[o] = a * self.out[o] + b;
            v[o] = c * self.out[o] + d;
        }
        for (k, &pre) in self.h.iter().enumerate() {
            let row = &mut self.e_ho[k * NO..][..NO];
            for o in 0..NO {
                row[o] = keep * row[o] + pre * u[o] + v[o];
            }
        }
        for (j, &pre) in input.iter().enumerate() {
            let row = &mut self.e_io[j * NO..][..NO];
            if pre == 0.0 {
                for e in row.iter_mut() {
                    *e *= keep;
                }
            } else {
                for o in 0..NO {
                    row[o] = keep * row[o] + pre * u[o] + v[o];
                }
            }
        }
        &self.out
    }

    /// Applies a reward (positive) or pain (negative) signal to whatever
    /// connections were recently active.
    pub fn learn(&mut self, modulator: f32) {
        let eta = self.rule[4];
        if eta <= 0.0 || modulator.abs() < 1e-4 {
            return;
        }
        let k = eta * modulator.clamp(-1.0, 1.0);
        for (w, e) in self.w_ho.iter_mut().zip(&self.e_ho) {
            *w = (*w + k * e).clamp(-W_MAX, W_MAX);
        }
        for (w, e) in self.w_io.iter_mut().zip(&self.e_io) {
            *w = (*w + k * e).clamp(-W_MAX, W_MAX);
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
                let w = &mut self.w_ho[k * NO + o];
                *w = (*w + g * self.h[k]).clamp(-W_MAX, W_MAX);
            }
            for j in 0..NI {
                let w = &mut self.w_io[j * NO + o];
                *w = (*w + g * self.input[j] * 0.5).clamp(-W_MAX, W_MAX);
            }
        }
    }
}

/// Gives a fresh random brain a few sensible reflexes, so founders can feed
/// and flee before evolution and learning take over.
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
}
