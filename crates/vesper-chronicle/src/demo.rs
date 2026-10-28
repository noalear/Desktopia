//! A toy world that reports plausible facts, so the chronicle and its window
//! can be built and shown before the real simulation exists. Nothing here is
//! the real Vesper: populations follow simple logistic curves, and wars,
//! plagues and lives are rolled with dice.

use crate::facts::*;
use crate::history::{History, notability};
use crate::names::WORLD_W;

/// Seconds between censuses.
const STEP: f64 = 60.0;
/// The twilight band's seasonal slide, in seconds.
const SEASON: f64 = 4.0 * 86_400.0;

/// Ordinary traits to start from.
pub fn seed_traits(seed: u64) -> Traits {
    Traits {
        kingdom: Kingdom::Motile,
        locomotion: Locomotion::Drift,
        reproduction: Reproduction::Budding,
        size: 0.02,
        speed: 0.05,
        diet: 0.1,
        armor: 0.1,
        spines: 0.0,
        toxin: 0.0,
        photosynthesis: 0.1,
        neurons: 12,
        sociality: 0.1,
        glow: 0.2,
        zone: 0.5,
        lifespan: 1800.0,
        clutch: 4.0,
        look: [(seed % 12) as f32 / 12.0, 0.5, 0.5, 0.5],
        segments: 4,
        diversity: 0.4,
        inbreeding: 0.05,
        selfing: 0.3,
    }
}

/// A newborn's life record.
pub fn blank_life(id: CreatureId, species: SpeciesId) -> Life {
    Life {
        id,
        species,
        parent: None,
        generation: 0,
        born: 0.0,
        age: 0.0,
        kills: 0,
        offspring: 0,
        eaten: 0.0,
        rank: 0.0,
        group: None,
        is_leader: false,
        x: 0.5,
        kindness: 0,
        scars: 0,
    }
}

/// Builds a history of `days` of demo time.
pub fn history(days: f64, seed: u64) -> History {
    let mut world = DemoWorld::new(seed);
    let mut h = History::default();
    let start_wall = 1_790_000_000.0;
    while world.t < days * 86_400.0 {
        for (tick, fact) in world.step() {
            h.apply(tick, start_wall + tick, &fact);
        }
    }
    h
}

struct Sp {
    id: SpeciesId,
    t: Traits,
    pop: f32,
    k: f32,
    alive: bool,
    tracked: Vec<Life>,
}

struct Gr {
    id: GroupId,
    species: usize,
    members: f32,
    x: f32,
    dialect: u32,
    signals: [u8; 4],
    leader: Option<CreatureId>,
    born: f64,
    alive: bool,
}

struct Strn {
    id: StrainId,
    host: usize,
    infected: f32,
    beta: f32,
    traits: StrainTraits,
    born: f64,
    alive: bool,
}

struct Hostility {
    a: usize,
    b: usize,
    until: f64,
}

pub struct DemoWorld {
    rng: u64,
    pub t: f64,
    next: u64,
    species: Vec<Sp>,
    groups: Vec<Gr>,
    strains: Vec<Strn>,
    feuds: Vec<Hostility>,
    firsts: Vec<Behavior>,
    out: Vec<(f64, Fact)>,
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

impl DemoWorld {
    pub fn new(seed: u64) -> Self {
        let mut w = Self {
            rng: seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1,
            t: 0.0,
            next: 1,
            species: Vec::new(),
            groups: Vec::new(),
            strains: Vec::new(),
            feuds: Vec::new(),
            firsts: Vec::new(),
            out: Vec::new(),
        };
        let plant = |zone: f32, hue: f32, photo: f32, loc: Locomotion| Traits {
            kingdom: Kingdom::Rooted,
            locomotion: loc,
            reproduction: Reproduction::Seeds,
            size: 0.03,
            speed: 0.0,
            diet: 0.0,
            neurons: 0,
            photosynthesis: photo,
            zone,
            lifespan: 20_000.0,
            look: [hue, 0.6, 0.4, 0.5],
            ..seed_traits(0)
        };
        let founders = [
            plant(0.35, 0.12, 0.95, Locomotion::Rooted),
            plant(0.55, 0.45, 0.9, Locomotion::Rooted),
            plant(0.85, 0.85, 0.1, Locomotion::Rooted),
            plant(0.5, 0.35, 0.8, Locomotion::FloatingMat),
            Traits { zone: 0.45, look: [0.75, 0.5, 0.5, 0.6], ..seed_traits(0) },
            Traits {
                locomotion: Locomotion::Crawl,
                size: 0.012,
                zone: 0.6,
                lifespan: 1200.0,
                look: [0.55, 0.5, 0.5, 0.5],
                ..seed_traits(0)
            },
            Traits {
                locomotion: Locomotion::Swim,
                size: 0.01,
                zone: 0.55,
                lifespan: 2400.0,
                sociality: 0.7,
                glow: 0.45,
                neurons: 30,
                look: [0.48, 0.5, 0.5, 0.6],
                ..seed_traits(0)
            },
            Traits {
                locomotion: Locomotion::Walk,
                size: 0.06,
                zone: 0.3,
                lifespan: 9000.0,
                armor: 0.5,
                neurons: 40,
                sociality: 0.55,
                reproduction: Reproduction::Eggs,
                look: [0.06, 0.5, 0.5, 0.5],
                ..seed_traits(0)
            },
        ];
        for t in founders {
            w.add_species(None, t, 200.0);
        }
        w
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rand() < p
    }

    fn gauss(&mut self) -> f32 {
        (self.rand() + self.rand() + self.rand() - 1.5) * 2.0
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    fn emit(&mut self, fact: Fact) {
        self.out.push((self.t, fact));
    }

    fn first(&mut self, behavior: Behavior, species: usize, who: Option<CreatureId>) {
        if !self.firsts.contains(&behavior) {
            self.firsts.push(behavior);
            let species = self.species[species].id;
            self.emit(Fact::First { behavior, species, who });
        }
    }

    fn add_species(&mut self, parent: Option<SpeciesId>, t: Traits, pop: f32) -> usize {
        let id = SpeciesId(self.id());
        self.emit(Fact::SpeciesArose { id, parent, traits: t.clone() });
        self.species.push(Sp { id, t, pop, k: pop, alive: true, tracked: Vec::new() });
        self.species.len() - 1
    }

    fn mutate(&mut self, p: &Traits) -> Traits {
        let mut t = p.clone();
        t.size = (t.size * (self.gauss() * 0.4).exp()).clamp(0.002, 0.3);
        t.zone = (t.zone + self.gauss() * 0.1).clamp(0.03, 0.97);
        t.armor = clamp01(t.armor + self.gauss() * 0.15);
        t.spines = clamp01(t.spines + self.gauss() * 0.12);
        t.toxin = clamp01(t.toxin + self.gauss() * 0.12);
        t.glow = clamp01(t.glow + self.gauss() * 0.15);
        t.look[0] = (t.look[0] + self.gauss() * 0.06).rem_euclid(1.0);
        t.lifespan = (t.lifespan * (self.gauss() * 0.3).exp()).clamp(300.0, 90_000.0);
        if t.kingdom == Kingdom::Rooted {
            t.diet = if self.chance(0.06) { 0.6 } else { (t.diet + self.gauss() * 0.05).clamp(0.0, 0.8) };
            t.photosynthesis = clamp01(t.photosynthesis + self.gauss() * 0.1);
        } else {
            t.diet = clamp01(t.diet + self.gauss() * 0.22);
            t.sociality = clamp01(t.sociality + self.gauss() * 0.15);
            t.neurons = ((t.neurons as f32) * (self.gauss() * 0.35).exp()).clamp(4.0, 400.0) as u16;
            t.speed = (t.speed * (self.gauss() * 0.3).exp()).clamp(0.005, 0.5);
            if self.chance(0.15) {
                let all = [
                    Locomotion::Drift,
                    Locomotion::Swim,
                    Locomotion::Walk,
                    Locomotion::Crawl,
                    Locomotion::Burrow,
                    Locomotion::Glide,
                ];
                t.locomotion = all[(self.rand() * all.len() as f32) as usize % all.len()];
            }
            if self.chance(0.1) {
                let all = [Reproduction::Budding, Reproduction::Spores, Reproduction::Eggs, Reproduction::Mating];
                t.reproduction = all[(self.rand() * all.len() as f32) as usize % all.len()];
            }
        }
        t.segments = (t.segments as i32 + (self.gauss() * 2.0) as i32).clamp(1, 40) as u16;
        t
    }

    fn band(&self) -> f32 {
        0.5 + 0.13 * (self.t / SEASON * std::f64::consts::TAU).sin() as f32
    }

    fn overlap(a: f32, b: f32) -> f32 {
        (-(a - b).powi(2) / 0.03).exp()
    }

    /// Runs one census interval and returns the facts it produced.
    pub fn step(&mut self) -> Vec<(f64, Fact)> {
        self.t += STEP;
        self.populations();
        self.speciate();
        self.lives();
        self.societies();
        self.sickness();
        self.weather();
        self.census();
        std::mem::take(&mut self.out)
    }

    fn producer(t: &Traits) -> bool {
        t.kingdom == Kingdom::Rooted && t.diet < 0.4
    }

    fn populations(&mut self) {
        let band = self.band();
        let n = self.species.len();
        for i in 0..n {
            if !self.species[i].alive {
                continue;
            }
            let t = self.species[i].t.clone();
            let suit = 0.03 + 0.97 * (-(t.zone - band).powi(2) / 0.03).exp();
            let mut food = 0.0;
            let mut rivals = 0.0;
            for j in 0..n {
                let o = &self.species[j];
                if j == i || !o.alive {
                    continue;
                }
                let near = Self::overlap(t.zone, o.t.zone);
                let o_producer = Self::producer(&o.t);
                let o_hunter = o.t.diet > 0.6 && o.t.kingdom == Kingdom::Motile;
                if Self::producer(&t) {
                    if o_producer {
                        rivals += near * o.pop;
                    }
                } else if t.diet < 0.4 || t.kingdom == Kingdom::Rooted {
                    if o_producer {
                        food += near * o.pop;
                    } else if !o_hunter && o.t.kingdom == Kingdom::Motile && (t.diet - o.t.diet).abs() < 0.3 {
                        rivals += near * o.pop;
                    }
                } else if t.diet >= 0.6 {
                    if o.t.kingdom == Kingdom::Motile && !o_hunter {
                        food += near * o.pop;
                    } else if o_hunter {
                        rivals += near * o.pop;
                    }
                } else {
                    food += near * o.pop * 0.5;
                }
            }
            let mass = (t.size / 0.02).powf(0.7);
            let k = if Self::producer(&t) {
                1500.0 * suit / (1.0 + rivals / 400.0)
            } else if t.diet >= 0.6 {
                food * 0.12 * suit / mass / (1.0 + rivals / 60.0)
            } else {
                food * 0.25 * suit / mass / (1.0 + rivals / 150.0)
            };
            let r = (STEP as f32 / t.lifespan * 1.5).min(0.4);
            let noise = self.gauss() * 0.25;
            let s = &mut self.species[i];
            s.k = k;
            s.pop = (s.pop + r * s.pop * (1.0 - s.pop / k.max(0.5)) + noise * s.pop.sqrt()).max(0.0);
            // Too few to find each other: gone.
            let doomed = s.pop < 0.6 || (s.pop < 5.0 && self.rng.is_multiple_of(23));
            if doomed {
                self.extinct(i);
            }
        }
    }

    fn extinct(&mut self, i: usize) {
        let s = &mut self.species[i];
        if !s.alive {
            return;
        }
        s.alive = false;
        s.pop = 0.0;
        let id = s.id;
        let x = s.t.zone;
        let starving = s.k < 1.0;
        let mut last = s.tracked.pop().unwrap_or_else(|| blank_life(CreatureId(0), id));
        s.tracked.clear();
        if last.id.0 == 0 {
            last.id = CreatureId(self.id());
            last.x = x * WORLD_W;
            last.age = self.species[i].t.lifespan * 0.6;
        }
        let cause = if starving { DeathCause::Starvation } else { self.random_cause(i) };
        // Only creatures' deaths are reported.
        if self.species[i].t.kingdom == Kingdom::Motile {
            self.emit(Fact::Death { who: last.id, species: id, cause, life: last });
        }
        self.emit(Fact::SpeciesExtinct { id });
        for g in 0..self.groups.len() {
            if self.groups[g].species == i && self.groups[g].alive {
                self.groups[g].alive = false;
                let gid = self.groups[g].id;
                self.emit(Fact::GroupGone { id: gid });
            }
        }
    }

    fn speciate(&mut self) {
        let living = self.species.iter().filter(|s| s.alive).count();
        let rate = if living > 30 {
            0.00003
        } else if living < 8 {
            0.004
        } else {
            0.0016
        };
        for i in 0..self.species.len() {
            if !self.species[i].alive || self.species[i].pop < 20.0 {
                continue;
            }
            if !self.chance(rate) {
                continue;
            }
            let parent = self.species[i].t.clone();
            let t = self.mutate(&parent);
            let share = self.species[i].pop * 0.3;
            self.species[i].pop -= share;
            let pid = self.species[i].id;
            let ni = self.add_species(Some(pid), t.clone(), share);
            if t.kingdom == Kingdom::Motile {
                if t.diet > 0.6 {
                    self.first(Behavior::Hunting, ni, None);
                } else if t.diet > 0.4 {
                    self.first(Behavior::Scavenging, ni, None);
                }
                if t.reproduction == Reproduction::Mating {
                    self.first(Behavior::SexualReproduction, ni, None);
                }
                if t.glow > 0.5 && t.sociality > 0.4 {
                    self.first(Behavior::Signaling, ni, None);
                }
                if t.sociality > 0.85 {
                    self.first(Behavior::Eusociality, ni, None);
                }
            } else if t.diet > 0.4 {
                self.first(Behavior::Trapping, ni, None);
            }
        }
    }

    fn random_cause(&mut self, i: usize) -> DeathCause {
        let x = self.species[i].t.zone;
        let hunters: Vec<usize> = (0..self.species.len())
            .filter(|&j| {
                let o = &self.species[j];
                o.alive && o.t.kingdom == Kingdom::Motile && o.t.diet > 0.6 && Self::overlap(o.t.zone, x) > 0.3
            })
            .collect();
        let roll = self.rand();
        if !hunters.is_empty() && roll < 0.45 {
            let j = hunters[(self.rand() * hunters.len() as f32) as usize % hunters.len()];
            let followed = if self.chance(0.3) { self.species[j].tracked.first().map(|l| l.id) } else { None };
            let killer = match followed {
                Some(id) => id,
                None => CreatureId(self.id()),
            };
            if let Some(l) = self.species[j].tracked.iter_mut().find(|l| l.id == killer) {
                l.kills += 1;
            }
            return DeathCause::Predation { killer, killer_species: self.species[j].id };
        }
        let sick = self.strains.iter().find(|s| s.alive && s.host == i && s.traits.virulence > 0.0).map(|s| s.id);
        match roll {
            _ if sick.is_some() && roll < 0.65 => DeathCause::Disease { strain: sick.unwrap() },
            r if r < 0.6 && self.species[i].pop > self.species[i].k => DeathCause::Starvation,
            r if r < 0.66 && x < 0.15 => DeathCause::Heat,
            r if r < 0.66 && x > 0.85 => DeathCause::Cold,
            r if r < 0.68 => {
                let traps = (0..self.species.len()).find(|&j| {
                    let o = &self.species[j];
                    o.alive && o.t.kingdom == Kingdom::Rooted && o.t.diet > 0.4 && Self::overlap(o.t.zone, x) > 0.3
                });
                match traps {
                    Some(j) => DeathCause::Trap { plant_species: self.species[j].id },
                    None => DeathCause::Age,
                }
            }
            r if r < 0.7 && self.species[i].t.diet > 0.6 && self.species[i].pop > self.species[i].k => {
                self.first(Behavior::Cannibalism, i, None);
                DeathCause::Cannibalism { killer: CreatureId(self.id()) }
            }
            _ => DeathCause::Age,
        }
    }

    fn lives(&mut self) {
        // Background deaths, so each species' tally of causes fills in.
        for i in 0..self.species.len() {
            let s = &self.species[i];
            if !s.alive || s.t.kingdom == Kingdom::Rooted || s.pop < 2.0 || !self.chance(0.12) {
                continue;
            }
            let who = CreatureId(self.id());
            let cause = self.random_cause(i);
            let s = &self.species[i];
            let mut life = blank_life(who, s.id);
            life.age = s.t.lifespan * 0.5;
            life.x = s.t.zone * WORLD_W;
            let id = s.id;
            self.emit(Fact::Death { who, species: id, cause, life });
        }

        // A few followed lives per species.
        for i in 0..self.species.len() {
            if !self.species[i].alive || self.species[i].t.kingdom == Kingdom::Rooted || self.species[i].pop < 3.0 {
                continue;
            }
            while self.species[i].tracked.len() < 2 {
                let id = CreatureId(self.id());
                let s = &self.species[i];
                let mut l = blank_life(id, s.id);
                l.born = self.t;
                l.x = s.t.zone * WORLD_W;
                if let Some(p) = s.tracked.first() {
                    l.parent = Some(p.id);
                    l.generation = p.generation + 1;
                } else {
                    l.generation = (self.t / s.t.lifespan as f64) as u32;
                }
                self.species[i].tracked.push(l);
            }
            let t = self.species[i].t.clone();
            let mut k = 0;
            while k < self.species[i].tracked.len() {
                let hunt = t.diet > 0.6 && self.chance(0.01 + t.diet * 0.02);
                let breed = self.chance(STEP as f32 / t.lifespan * 1.5);
                let scar = self.chance(0.004 + t.armor * 0.004);
                let kind = t.sociality > 0.5 && self.chance(0.02 * t.sociality);
                let rank = self.gauss() * 0.05;
                let clutch = t.clutch.max(1.0) * (0.5 + self.rand());
                let l = &mut self.species[i].tracked[k];
                l.age += STEP as f32;
                l.kills += hunt as u32;
                l.offspring += if breed { clutch as u32 } else { 0 };
                l.scars += scar as u32;
                l.kindness += kind as u32;
                l.rank = (l.rank + rank).clamp(-2.0, 3.0);
                l.eaten += 0.01;
                let age_frac = l.age / t.lifespan;
                let p_die = (STEP as f32 / t.lifespan) * 0.3 + age_frac.powi(4) * 0.02;
                if self.chance(p_die) || age_frac > 1.8 {
                    let l = self.species[i].tracked.remove(k);
                    let cause = if age_frac > 0.9 && self.chance(0.6) { DeathCause::Age } else { self.random_cause(i) };
                    let cause = match cause {
                        DeathCause::Age
                            if t.sociality > 0.6 && t.reproduction == Reproduction::Eggs && self.chance(0.3) =>
                        {
                            self.first(Behavior::Matriphagy, i, Some(l.id));
                            DeathCause::Devoured { by_offspring: CreatureId(self.id()) }
                        }
                        DeathCause::Predation { .. } if t.sociality > 0.6 && self.chance(0.25) => {
                            self.first(Behavior::Sacrifice, i, Some(l.id));
                            DeathCause::Sacrifice { defending: CreatureId(self.id()) }
                        }
                        c => c,
                    };
                    let id = self.species[i].id;
                    self.emit(Fact::Death { who: l.id, species: id, cause, life: l });
                } else {
                    k += 1;
                }
            }
        }
    }

    fn societies(&mut self) {
        for i in 0..self.species.len() {
            let s = &self.species[i];
            if !s.alive || s.t.kingdom == Kingdom::Rooted || s.t.sociality < 0.45 || s.pop < 6.0 {
                continue;
            }
            if !self.groups.iter().any(|g| g.alive && g.species == i) {
                let (zone, sid, members) = (s.t.zone, s.id, s.pop as u32);
                let id = GroupId(self.id());
                let dialect = self.rand().to_bits();
                let signals = [(self.rand() * 255.0) as u8, (self.rand() * 255.0) as u8, 40, 200];
                self.groups.push(Gr {
                    id,
                    species: i,
                    members: members as f32,
                    x: zone,
                    dialect,
                    signals,
                    leader: None,
                    born: self.t,
                    alive: true,
                });
                self.emit(Fact::GroupFormed { id, species: sid, parent_group: None, members });
            }
        }

        // Members share their species' numbers; big groups split.
        for g in 0..self.groups.len() {
            if !self.groups[g].alive {
                continue;
            }
            let si = self.groups[g].species;
            let n = self.groups.iter().filter(|o| o.alive && o.species == si).count() as f32;
            let target = self.species[si].pop / n;
            let gr = &mut self.groups[g];
            gr.members += (target - gr.members) * 0.1;
            if gr.members < 2.0 {
                gr.alive = false;
                let id = gr.id;
                self.emit(Fact::GroupGone { id });
                continue;
            }
            let ripe = gr.members > 25.0 && self.t - gr.born > 3600.0 && n < 3.0;
            if ripe && self.chance(0.004) {
                let shift = self.gauss() * 0.06;
                let gr = &mut self.groups[g];
                let parent = gr.id;
                let x = (gr.x + shift).clamp(0.02, 0.98);
                let (dialect, signals) = (gr.dialect, gr.signals);
                gr.members /= 2.0;
                let members = gr.members;
                let id = GroupId(self.id());
                self.groups.push(Gr {
                    id,
                    species: si,
                    members,
                    x,
                    dialect,
                    signals,
                    leader: None,
                    born: self.t,
                    alive: true,
                });
                let sid = self.species[si].id;
                self.emit(Fact::GroupFormed { id, species: sid, parent_group: Some(parent), members: members as u32 });
            }
            // Dialects drift once groups have been apart a while.
            if self.groups[g].dialect != 0 && self.chance(0.0015) {
                self.groups[g].dialect = self.rand().to_bits();
                self.groups[g].signals[0] = self.groups[g].signals[0].wrapping_add(60);
                self.first(Behavior::Dialect, si, None);
            }
            // Leaders come from followed lives.
            let leader = self.groups[g].leader;
            let candidates: Vec<CreatureId> = self.species[si].tracked.iter().map(|l| l.id).collect();
            let alive = leader.is_some_and(|l| candidates.contains(&l));
            if (!alive || self.chance(0.001)) && !candidates.is_empty() {
                let new = candidates[(self.rand() * candidates.len() as f32) as usize % candidates.len()];
                if Some(new) != leader {
                    let overthrown = alive;
                    if overthrown {
                        self.first(Behavior::Overthrow, si, Some(new));
                    }
                    self.groups[g].leader = Some(new);
                    let group = self.groups[g].id;
                    self.emit(Fact::LeaderChanged { group, old: leader, new, overthrown });
                }
            }
            if let Some(lid) = self.groups[g].leader {
                let gid = self.groups[g].id;
                for l in &mut self.species[si].tracked {
                    l.is_leader = l.id == lid;
                    if l.is_leader {
                        l.group = Some(gid);
                    }
                }
            }
        }

        // Hostility between nearby groups, more likely when food is short.
        let alive: Vec<usize> = (0..self.groups.len()).filter(|&g| self.groups[g].alive).collect();
        for (n, &a) in alive.iter().enumerate() {
            for &b in &alive[n + 1..] {
                let near = Self::overlap(self.groups[a].x, self.groups[b].x);
                let hungry = {
                    let s = &self.species[self.groups[a].species];
                    s.pop > s.k * 0.95
                };
                let p = 0.000003 * near * if hungry { 4.0 } else { 1.0 };
                if !self.feuds.iter().any(|f| (f.a, f.b) == (a, b)) && self.chance(p) {
                    let until = self.t + 3600.0 * (1.0 + self.rand() as f64 * 4.0);
                    self.feuds.push(Hostility { a, b, until });
                }
            }
        }
        let now = self.t;
        self.feuds.retain(|f| f.until > now);
        for f in 0..self.feuds.len() {
            let (a, b) = (self.feuds[f].a, self.feuds[f].b);
            if !self.groups[a].alive || !self.groups[b].alive || !self.chance(0.08) {
                continue;
            }
            let (att, def) = if self.chance(0.5) { (a, b) } else { (b, a) };
            let deaths = 1 + (self.rand() * 4.0) as u32;
            let x = self.groups[def].x;
            self.groups[def].members -= deaths as f32;
            let si = self.groups[def].species;
            self.species[si].pop = (self.species[si].pop - deaths as f32).max(0.0);
            let (attacker, defender) = (self.groups[att].id, self.groups[def].id);
            self.first(Behavior::War, self.groups[att].species, None);
            self.emit(Fact::Clash { attacker, defender, deaths, x: x * WORLD_W });
            if let Some(l) = self.species[self.groups[att].species].tracked.first_mut() {
                l.kills += 1;
            }
            if self.groups[def].members < 2.0 {
                self.groups[def].alive = false;
                self.emit(Fact::GroupGone { id: defender });
            }
        }
    }

    fn sickness(&mut self) {
        for i in 0..self.species.len() {
            if self.species[i].alive && self.species[i].pop > 60.0 && self.chance(0.00006) {
                let id = StrainId(self.id());
                let symbiont = self.chance(0.15);
                let traits = StrainTraits {
                    visible: if self.chance(0.3) { 0.8 } else { 0.1 },
                    contagion: 0.2 + self.rand() * 0.5,
                    virulence: if symbiont { -0.2 } else { 0.1 + self.rand() * 0.6 },
                    manipulation: if self.chance(0.2) { 0.7 } else { 0.0 },
                    bursts: if self.chance(0.2) { 0.8 } else { 0.0 },
                    photosynthetic: if symbiont { 0.8 } else { 0.0 },
                };
                let host = self.species[i].id;
                let beta = 0.06 + self.rand() * 0.16;
                self.strains.push(Strn {
                    id,
                    host: i,
                    infected: 1.0,
                    beta,
                    traits: traits.clone(),
                    born: self.t,
                    alive: true,
                });
                self.emit(Fact::PathogenArose { id, parent: None, host, traits });
                if symbiont {
                    self.first(Behavior::Symbiosis, i, None);
                } else if self.strains.last().unwrap().traits.visible > 0.5 {
                    self.first(Behavior::Parasitism, i, None);
                }
            }
        }
        for k in 0..self.strains.len() {
            let s = &self.strains[k];
            if !s.alive {
                continue;
            }
            let host = &self.species[s.host];
            let pop = host.pop.max(1.0);
            let gamma = 0.05;
            let i = s.infected;
            let next = (i + s.beta * i * (1.0 - i / pop) - gamma * i).clamp(0.0, pop);
            let kill = next * s.traits.virulence.max(0.0) * 0.01;
            let (alive, born) = (host.alive, s.born);
            self.species[self.strains[k].host].pop -= kill;
            let s = &mut self.strains[k];
            s.infected = next;
            if !alive || (next < 0.5 && self.t - born > 600.0) {
                s.alive = false;
                let id = s.id;
                self.emit(Fact::PathogenGone { id });
            }
        }
    }

    fn weather(&mut self) {
        let roll = self.rand();
        let (kind, x) = match roll {
            r if r < 0.0002 => (NatureKind::Eruption, 0.65 + self.rand() * 0.3),
            r if r < 0.0006 => (NatureKind::Flood, 0.3 + self.rand() * 0.5),
            r if r < 0.0008 => (NatureKind::RiverShift, 0.2 + self.rand() * 0.6),
            r if r < 0.0018 => (NatureKind::Storm, 0.2 + self.rand() * 0.6),
            r if r < 0.0022 => (NatureKind::ColdFall, 0.6 + self.rand() * 0.35),
            r if r < 0.0026 => (NatureKind::DustStorm, self.rand() * 0.35),
            _ => return,
        };
        let strength = 0.2 + self.rand() * 0.8;
        self.emit(Fact::Nature { kind, x: x * WORLD_W, strength });
        for s in &mut self.species {
            if s.alive {
                s.pop *= 1.0 - strength * 0.4 * Self::overlap(s.t.zone, x);
            }
        }
    }

    fn census(&mut self) {
        let mut creatures = 0.0;
        let mut plants = 0.0;
        let mut species = Vec::new();
        let mut standouts: Vec<Life> = Vec::new();
        for s in self.species.iter().filter(|s| s.alive) {
            if s.t.kingdom == Kingdom::Rooted {
                plants += s.pop;
            } else {
                creatures += s.pop;
            }
            species.push(SpeciesCensus {
                id: s.id,
                count: s.pop.round() as u32,
                biomass: s.pop * s.t.size * s.t.size * 2000.0,
                mean_x: s.t.zone * WORLD_W,
                traits: s.t.clone(),
            });
            standouts.extend(s.tracked.iter().cloned());
        }
        standouts.sort_by(|a, b| notability(b).total_cmp(&notability(a)));
        standouts.truncate(20);
        let groups = self
            .groups
            .iter()
            .filter(|g| g.alive)
            .map(|g| GroupCensus {
                id: g.id,
                species: self.species[g.species].id,
                members: g.members.round() as u32,
                x_range: [(g.x - 0.04) * WORLD_W, (g.x + 0.04) * WORLD_W],
                leader: g.leader,
                dialect: g.dialect,
                signals: g.signals,
            })
            .collect();
        let strains = self
            .strains
            .iter()
            .filter(|s| s.alive)
            .map(|s| StrainCensus { id: s.id, infected: s.infected.round() as u32, traits: s.traits.clone() })
            .collect();
        let living = creatures * 0.4 + plants * 0.6;
        self.emit(Fact::Census(Census {
            creatures: creatures as u32,
            plants: plants as u32,
            living_matter: living,
            dead_matter: living * 0.3,
            soil_matter: 10_000.0 - living * 1.3,
            species,
            groups,
            strains,
            standouts,
        }));
    }
}

#[cfg(test)]
mod tests {
    use crate::history::Kind;

    #[test]
    fn a_demo_week_has_every_kind_of_story() {
        let h = super::history(7.0, 7);
        let kinds: Vec<Kind> = h.entries.iter().map(|e| e.kind).collect();
        for k in [Kind::Origin, Kind::Extinction, Kind::Death, Kind::Nature, Kind::First] {
            assert!(kinds.contains(&k), "no {k:?} in {} entries", kinds.len());
        }
        assert!(h.species.len() > 15, "{} species", h.species.len());
        assert!(h.living_species() > 3);
    }
}
