//! What a creature remembers and learns beyond its wiring: what things taste
//! like and which kinds of thing are dangerous, who it knows and how far it
//! trusts them, the places it has fed and been hurt, how much of the land it
//! has seen, its light language, and its moods. All of it can be learned from
//! experience or copied from others, which is where culture comes from.

use crate::history::CreatureId;
use serde::{Deserialize, Serialize};

/// Appearance classes a creature can tell apart: 48 for creatures (hue x
/// value x size) and 16 for plants (hue x value).
pub const TASTE_BINS: usize = 64;
/// Acquaintances a creature can keep track of at most, with the biggest brain.
pub const KNOWN: usize = 16;
/// Places it can remember at once.
pub const PLACES: usize = 12;
/// Stretches of the band it keeps count of visiting.
pub const VISIT_BINS: usize = 64;
/// Signal symbols: patterns of flashes a glow organ can make.
pub const SYMBOLS: usize = 16;
/// Meanings a signal can carry.
pub const MEANINGS: usize = 6;
/// Danger on the ground or close by.
pub const DANGER: usize = 0;
pub const FOOD: usize = 1;
pub const MATE: usize = 2;
/// A threat to me: back off.
pub const THREAT: usize = 3;
/// Come here: a contact call, or a call for help.
pub const GATHER: usize = 4;
/// Danger from above.
pub const ALOFT: usize = 5;

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

pub fn visit_bin(x: f32) -> usize {
    ((x / vesper_terrain::WORLD_W * VISIT_BINS as f32) as usize).min(VISIT_BINS - 1)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Acquaintance {
    pub id: CreatureId,
    /// -1 enemy .. 1 friend.
    pub bond: f32,
    pub kin: bool,
    /// When last met, sim seconds.
    pub seen: f64,
    /// How far its signals have proved true, 0..1.
    pub trust: f32,
    /// When it last fought this one, sim seconds, or negative.
    pub fought: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaceKind {
    Food,
    Danger,
    Shelter,
}

/// A place remembered: where, what for, how strongly, and since when.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub x: f32,
    pub depth: f32,
    pub kind: PlaceKind,
    /// 0..1 how good or how bad.
    pub value: f32,
    pub when: f64,
    /// Learned from another rather than found itself.
    pub told: bool,
}

/// Slow internal states that colour what a creature does, 0..1 each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mood {
    pub fear: f32,
    pub curious: f32,
    pub lonely: f32,
    pub tired: f32,
}

/// Staying by the body of one it knew.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grief {
    pub who: CreatureId,
    pub pos: [f32; 3],
    pub until: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mind {
    /// Learned worth of eating each appearance class, -1 .. 1.
    pub taste: Vec<i8>,
    /// Learned danger of each appearance class, 0 .. 1.
    pub wary: Vec<i8>,
    pub known: Vec<Acquaintance>,
    /// How many others it can keep track of; bigger brains know more.
    pub capacity: u8,
    /// Remembered best feeding x for each quarter of the season, or negative.
    pub home: [f32; 4],
    /// Who first found the route this creature follows.
    pub route_from: Option<CreatureId>,
    pub places: Vec<Place>,
    /// How often it has been in each stretch of the band, fading.
    pub visits: Vec<u8>,
    /// The symbol this creature flashes for each meaning.
    pub voice: [u8; MEANINGS],
    /// How strongly each symbol has meant each meaning, in its experience.
    pub lexicon: Vec<[u8; MEANINGS]>,
    /// Meanings currently understood from nearby signals, and the x they came from.
    pub heard: [f32; MEANINGS],
    pub heard_x: [f32; MEANINGS],
    pub mood: Mood,
    /// An appearance class its parent taught it to eat, until it has eaten it.
    pub taught: Option<u8>,
    pub grief: Option<Grief>,
    /// Lost a contest to this one and backs off until then.
    pub yielding: Option<(CreatureId, f64)>,
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
            wary: vec![0; TASTE_BINS],
            known: Vec::new(),
            capacity: 8,
            home: [-1.0; 4],
            route_from: None,
            places: Vec::new(),
            visits: vec![0; VISIT_BINS],
            voice,
            lexicon,
            heard: [0.0; MEANINGS],
            heard_x: [0.0; MEANINGS],
            mood: Mood { curious: 0.5, ..Default::default() },
            taught: None,
            grief: None,
            yielding: None,
        }
    }

    /// A newborn's mind: its parent's voice (it hears it first), a hint of the
    /// parent's tastes, fears, routes and places, and no acquaintances but its parent.
    pub fn inherit(parent: &Mind, parent_id: CreatureId, now: f64) -> Self {
        let mut m = Self::hatch(parent.voice, parent.home, parent.route_from, &parent.wary, &parent.places);
        for (t, p) in m.taste.iter_mut().zip(&parent.taste) {
            *t = p / 2;
        }
        m.known.push(Acquaintance { id: parent_id, bond: 0.8, kin: true, seen: now, trust: 0.8, fought: -1.0 });
        m
    }

    /// A mind from what a mother passes on through the egg: her voice and
    /// routes, and a fainter sense of what she feared and where.
    pub fn hatch(
        voice: [u8; MEANINGS],
        home: [f32; 4],
        route_from: Option<CreatureId>,
        wary: &[i8],
        places: &[Place],
    ) -> Self {
        let mut m = Self::new(voice);
        m.home = home;
        m.route_from = route_from;
        for (w, p) in m.wary.iter_mut().zip(wary) {
            *w = p / 2;
        }
        m.places = places
            .iter()
            .filter(|p| p.value > 0.2)
            .map(|p| Place { value: p.value * 0.5, told: true, ..p.clone() })
            .collect();
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

    /// How dangerous it has learned this kind of thing is, 0..1.
    pub fn wary(&self, bin: usize) -> f32 {
        self.wary[bin].max(0) as f32 / 127.0
    }

    pub fn learn_wary(&mut self, bin: usize, danger: f32, rate: f32) {
        let t = self.wary(bin);
        let v = (t + (danger.clamp(0.0, 1.0) - t) * rate.clamp(0.0, 1.0)).clamp(0.0, 1.0);
        self.wary[bin] = (v * 127.0) as i8;
    }

    pub fn find(&self, id: CreatureId) -> Option<&Acquaintance> {
        self.known.iter().find(|a| a.id == id)
    }

    fn slot(&mut self, id: CreatureId, kin: bool, now: f64) -> &mut Acquaintance {
        if let Some(i) = self.known.iter().position(|a| a.id == id) {
            let a = &mut self.known[i];
            a.kin |= kin;
            a.seen = now;
            return a;
        }
        let a = Acquaintance { id, bond: 0.0, kin, seen: now, trust: 0.5, fought: -1.0 };
        if self.known.len() < (self.capacity as usize).min(KNOWN) {
            self.known.push(a);
            return self.known.last_mut().unwrap();
        }
        // The least important memory is forgotten when there is no room.
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
        &mut self.known[weakest]
    }

    /// Changes how this creature feels about another, remembering it if new.
    pub fn feel(&mut self, id: CreatureId, delta: f32, kin: bool, now: f64) {
        let a = self.slot(id, kin, now);
        a.bond = (a.bond + delta).clamp(-1.0, 1.0);
    }

    /// Remembers a fight with another, for making up afterwards.
    pub fn fought(&mut self, id: CreatureId, kin: bool, now: f64) {
        self.slot(id, kin, now).fought = now;
    }

    /// Adjusts how far it believes another's signals.
    pub fn believe(&mut self, id: CreatureId, delta: f32, now: f64) {
        let a = self.slot(id, false, now);
        a.trust = (a.trust + delta).clamp(0.0, 1.0);
    }

    pub fn bond(&self, id: CreatureId) -> f32 {
        self.find(id).map(|a| a.bond).unwrap_or(0.0)
    }

    pub fn trust(&self, id: CreatureId) -> f32 {
        self.find(id).map(|a| a.trust).unwrap_or(0.5)
    }

    /// Remembers a place, or strengthens one already remembered close by.
    pub fn remember(&mut self, kind: PlaceKind, x: f32, depth: f32, value: f32, now: f64, told: bool) {
        let value = value.clamp(0.0, 1.0);
        if let Some(p) =
            self.places.iter_mut().find(|p| p.kind == kind && (p.x - x).abs() < 0.06 && (p.depth - depth).abs() < 0.12)
        {
            let k = if told { 0.2 } else { 0.4 };
            p.x += (x - p.x) * k;
            p.depth += (depth - p.depth) * k;
            // Finding it good again keeps it fresh; going sour is `disappointed`.
            p.value = p.value.max(p.value + (value - p.value) * k);
            p.when = now;
            p.told &= told;
            return;
        }
        let place = Place { x, depth, kind, value, when: now, told };
        if self.places.len() < PLACES {
            self.places.push(place);
            return;
        }
        if let Some((i, weakest)) = self.places.iter().enumerate().min_by(|a, b| a.1.value.total_cmp(&b.1.value))
            && weakest.value < value
        {
            self.places[i] = place;
        }
    }

    /// Lowers a remembered food place that turned out empty.
    pub fn disappointed(&mut self, x: f32, depth: f32) {
        for p in &mut self.places {
            if p.kind == PlaceKind::Food && (p.x - x).abs() < 0.04 && (p.depth - depth).abs() < 0.1 {
                p.value *= 0.6;
            }
        }
    }

    /// The best place to go and feed from here, worth less the further it is.
    pub fn best_food(&self, x: f32, depth: f32) -> Option<(&Place, f32)> {
        self.places
            .iter()
            .filter(|p| p.kind == PlaceKind::Food)
            .map(|p| {
                let d = ((p.x - x).powi(2) + (p.depth - depth).powi(2)).sqrt();
                (p, p.value / (1.0 + d * 4.0))
            })
            .filter(|(p, _)| (p.x - x).abs() > 0.02 || (p.depth - depth).abs() > 0.04)
            .max_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The worst remembered danger close by: how bad and near, and where.
    pub fn danger_near(&self, x: f32, depth: f32, range: f32) -> Option<(f32, [f32; 2])> {
        self.places
            .iter()
            .filter(|p| p.kind == PlaceKind::Danger)
            .filter_map(|p| {
                let d = ((p.x - x).powi(2) + (p.depth - depth).powi(2)).sqrt();
                (d < range).then(|| (p.value * (1.0 - d / range), [p.x, p.depth]))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    pub fn nearest_shelter(&self, x: f32, depth: f32) -> Option<[f32; 2]> {
        self.places
            .iter()
            .filter(|p| p.kind == PlaceKind::Shelter)
            .min_by(|a, b| {
                let da = (a.x - x).abs() + (a.depth - depth).abs();
                let db = (b.x - x).abs() + (b.depth - depth).abs();
                da.total_cmp(&db)
            })
            .map(|p| [p.x, p.depth])
    }

    /// Notes being here. Called about once a second.
    pub fn visit(&mut self, x: f32) {
        let v = &mut self.visits[visit_bin(x)];
        *v = v.saturating_add(1);
    }

    /// How unfamiliar this stretch of land is, 0..1.
    pub fn novelty(&self, x: f32) -> f32 {
        1.0 / (1.0 + self.visits[visit_bin(x)] as f32 / 6.0)
    }

    /// Which way along the band lies the less familiar land: -1, 0 or +1.
    pub fn frontier(&self, x: f32) -> f32 {
        let b = visit_bin(x);
        let left: u32 = self.visits[b.saturating_sub(3)..b].iter().map(|&v| v as u32).sum();
        let right: u32 = self.visits[(b + 1).min(VISIT_BINS)..(b + 4).min(VISIT_BINS)].iter().map(|&v| v as u32).sum();
        let (lw, rw) = (b.min(3) as u32, (VISIT_BINS - 1 - b).min(3) as u32);
        if lw == 0 {
            return 1.0;
        }
        if rw == 0 {
            return -1.0;
        }
        let (l, r) = (left as f32 / lw as f32, right as f32 / rw as f32);
        if (l - r).abs() < 1.0 { 0.0 } else { (l - r).signum() }
    }

    /// Memories fade over `dt` seconds.
    pub fn forget(&mut self, dt: f32) {
        for p in &mut self.places {
            let life = match p.kind {
                PlaceKind::Food => 1500.0,
                PlaceKind::Danger => 2400.0,
                PlaceKind::Shelter => 3600.0,
            };
            p.value *= (-dt / life).exp();
        }
        self.places.retain(|p| p.value > 0.04);
        // Visits: half-forgotten every ten minutes or so.
        let keep = (-dt / 900.0).exp();
        for v in &mut self.visits {
            *v = (*v as f32 * keep) as u8;
        }
        // Old fears fade unless something renews them: about a step in 127
        // every ten seconds, so a full fear is gone in some twenty minutes.
        let soften = (-dt / 3600.0).exp();
        for w in &mut self.wary {
            *w = (*w as f32 * soften) as i8;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_merge_and_fade() {
        let mut m = Mind::new([0, 1, 2, 3, 4, 5]);
        m.remember(PlaceKind::Food, 1.0, 0.1, 0.8, 0.0, false);
        m.remember(PlaceKind::Food, 1.02, 0.12, 0.6, 1.0, false);
        assert_eq!(m.places.len(), 1);
        m.remember(PlaceKind::Danger, 1.5, 0.1, 0.9, 2.0, false);
        assert_eq!(m.places.len(), 2);
        let (p, _) = m.best_food(1.4, 0.1).unwrap();
        assert!((p.x - 1.0).abs() < 0.03);
        assert!(m.danger_near(1.45, 0.1, 0.2).is_some());
        m.forget(20000.0);
        assert!(m.places.is_empty());
    }

    #[test]
    fn the_unvisited_way_is_the_frontier() {
        let mut m = Mind::new([0; MEANINGS]);
        let x = vesper_terrain::WORLD_W * 0.5;
        let b = visit_bin(x);
        for _ in 0..30 {
            for k in b - 3..b {
                m.visits[k] = 40;
            }
        }
        assert_eq!(m.frontier(x), 1.0);
        assert!(m.novelty(x) > 0.9);
        m.visit(x);
        assert!(m.novelty(x) < 1.0);
    }
}
