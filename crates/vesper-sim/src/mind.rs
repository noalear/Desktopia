//! What a creature remembers and learns beyond its wiring: what things taste
//! like, who it knows, where it feeds each season, and its light language.
//! All of it can be learned from experience or copied from others, which is
//! where culture comes from.

use crate::history::CreatureId;
use serde::{Deserialize, Serialize};

/// Appearance classes a creature can tell apart: 48 for creatures (hue x
/// value x size) and 16 for plants (hue x value).
pub const TASTE_BINS: usize = 64;
pub const KNOWN: usize = 8;
/// Signal symbols: patterns of flashes a glow organ can make.
pub const SYMBOLS: usize = 16;
/// Meanings a signal can carry.
pub const MEANINGS: usize = 4;
pub const DANGER: usize = 0;
pub const FOOD: usize = 1;
pub const MATE: usize = 2;
pub const THREAT: usize = 3;

pub fn creature_bin(look: &[f32], size: f32) -> usize {
    let hue = ((look[0] * 8.0) as usize).min(7);
    let val = (look[1] > 0.5) as usize;
    let sz = if size < 0.012 {
        0
    } else if size < 0.035 {
        1
    } else {
        2
    };
    (hue * 2 + val) * 3 + sz
}

pub fn plant_bin(look: &[f32]) -> usize {
    let hue = ((look[0] * 8.0) as usize).min(7);
    48 + hue * 2 + (look[1] > 0.5) as usize
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Acquaintance {
    pub id: CreatureId,
    /// -1 enemy .. 1 friend.
    pub bond: f32,
    pub kin: bool,
    /// When last met, sim seconds.
    pub seen: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mind {
    /// Learned worth of eating each appearance class, -1 .. 1.
    pub taste: Vec<i8>,
    pub known: Vec<Acquaintance>,
    /// Remembered best feeding x for each quarter of the season, or negative.
    pub home: [f32; 4],
    /// Who first found the route this creature follows.
    pub route_from: Option<CreatureId>,
    /// The symbol this creature flashes for each meaning.
    pub voice: [u8; MEANINGS],
    /// How strongly each symbol has meant each meaning, in its experience.
    pub lexicon: Vec<[u8; MEANINGS]>,
    /// Meanings currently understood from nearby signals, and the x they came from.
    pub heard: [f32; MEANINGS],
    pub heard_x: [f32; MEANINGS],
}

impl Mind {
    pub fn new(voice: [u8; MEANINGS]) -> Self {
        let mut lexicon = vec![[0u8; MEANINGS]; SYMBOLS];
        // A creature understands its own voice from birth.
        for (m, &s) in voice.iter().enumerate() {
            lexicon[s as usize][m] = 60;
        }
        Self {
            taste: vec![0; TASTE_BINS],
            known: Vec::new(),
            home: [-1.0; 4],
            route_from: None,
            voice,
            lexicon,
            heard: [0.0; MEANINGS],
            heard_x: [0.0; MEANINGS],
        }
    }

    /// A newborn's mind: its parent's voice (it hears it first), a hint of the
    /// parent's tastes and routes, and no acquaintances but its parent.
    pub fn inherit(parent: &Mind, parent_id: CreatureId, now: f64) -> Self {
        let mut m = Self::new(parent.voice);
        for (t, p) in m.taste.iter_mut().zip(&parent.taste) {
            *t = p / 2;
        }
        m.home = parent.home;
        m.route_from = parent.route_from;
        m.known.push(Acquaintance { id: parent_id, bond: 0.8, kin: true, seen: now });
        m
    }

    pub fn taste(&self, bin: usize) -> f32 {
        self.taste[bin] as f32 / 127.0
    }

    pub fn learn_taste(&mut self, bin: usize, valence: f32, rate: f32) {
        let t = self.taste(bin);
        let v = (t + (valence.clamp(-1.0, 1.0) - t) * rate).clamp(-1.0, 1.0);
        self.taste[bin] = (v * 127.0) as i8;
    }

    pub fn find(&self, id: CreatureId) -> Option<&Acquaintance> {
        self.known.iter().find(|a| a.id == id)
    }

    /// Changes how this creature feels about another, remembering it if new.
    /// The least important memory is forgotten when there is no room.
    pub fn feel(&mut self, id: CreatureId, delta: f32, kin: bool, now: f64) {
        if let Some(a) = self.known.iter_mut().find(|a| a.id == id) {
            a.bond = (a.bond + delta).clamp(-1.0, 1.0);
            a.kin |= kin;
            a.seen = now;
            return;
        }
        let a = Acquaintance { id, bond: delta.clamp(-1.0, 1.0), kin, seen: now };
        if self.known.len() < KNOWN {
            self.known.push(a);
            return;
        }
        let weakest = self
            .known
            .iter()
            .enumerate()
            .min_by(|(_, x), (_, y)| {
                let wx = x.bond.abs() + x.kin as u8 as f32 - ((now - x.seen) as f32 / 600.0);
                let wy = y.bond.abs() + y.kin as u8 as f32 - ((now - y.seen) as f32 / 600.0);
                wx.total_cmp(&wy)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.known[weakest] = a;
    }

    pub fn bond(&self, id: CreatureId) -> f32 {
        self.find(id).map(|a| a.bond).unwrap_or(0.0)
    }

    /// What a symbol most likely means, and how sure.
    pub fn understand(&self, symbol: u8) -> Option<(usize, f32)> {
        let row = &self.lexicon[symbol as usize % SYMBOLS];
        let (m, &best) = row.iter().enumerate().max_by_key(|(_, v)| **v)?;
        let total: u32 = row.iter().map(|&v| v as u32).sum();
        if best < 20 {
            return None;
        }
        Some((m, best as f32 / total.max(1) as f32))
    }

    /// Learns that a symbol was used for a meaning (seen to be true).
    pub fn associate(&mut self, symbol: u8, meaning: usize, rate: u8) {
        let row = &mut self.lexicon[symbol as usize % SYMBOLS];
        for (m, v) in row.iter_mut().enumerate() {
            if m == meaning {
                *v = v.saturating_add(rate);
            } else {
                *v = v.saturating_sub(rate / 3);
            }
        }
    }

    /// Learns that a symbol turned out not to mean what was understood.
    pub fn distrust(&mut self, symbol: u8, meaning: usize, rate: u8) {
        let row = &mut self.lexicon[symbol as usize % SYMBOLS];
        row[meaning] = row[meaning].saturating_sub(rate);
    }

    pub fn hear(&mut self, meaning: usize, strength: f32, x: f32) {
        if strength > self.heard[meaning] {
            self.heard[meaning] = strength;
            self.heard_x[meaning] = x;
        }
    }

    pub fn fade(&mut self, k: f32) {
        for h in &mut self.heard {
            *h *= k;
        }
    }
}

/// How many meanings two voices share, 0..1.
pub fn intelligibility(a: &[u8; MEANINGS], b: &[u8; MEANINGS]) -> f32 {
    a.iter().zip(b).filter(|(x, y)| x == y).count() as f32 / MEANINGS as f32
}
