//! Sensing and thinking: each creature looks around, feels its body, and
//! its brain decides what to do.

use crate::brain::{NI, i};
use crate::genome::{Genome, Tissue};
use crate::mind::{self, creature_bin, plant_bin};
use crate::world::{ALT_MAX, World, col_of, dist3};
use vesper_terrain as terrain;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Food {
    Plant(usize),
    Remains(usize),
    Egg(usize),
    /// Detritus strained from the air.
    Air,
}

/// What a creature noticed this tick, used again when it acts.
#[derive(Clone, Debug, Default)]
pub struct Percept {
    pub food: Option<Food>,
    pub food_pos: [f32; 3],
    pub other: Option<usize>,
    pub other_dist: f32,
    pub threat: Option<usize>,
    pub threat_level: f32,
    pub kin_near: u32,
}

fn ahead(heading: f32, from: [f32; 3], to: [f32; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dz = to[1] - from[1];
    let len = (dx * dx + dz * dz).sqrt().max(1e-5);
    (dx * heading.cos() + dz * heading.sin()) / len
}

pub fn think(w: &mut World, percepts: &mut Vec<Percept>) {
    let n = w.creatures.len();
    percepts.clear();
    percepts.resize(n, Percept::default());
    let quarter = season_quarter(w);
    let mut inputs = [0.0f32; NI];
    w.rgrid.clear();
    for (j, r) in w.remains.iter().enumerate() {
        w.rgrid.insert(j, r.pos[0], r.pos[2]);
    }
    w.egrid.clear();
    for (j, e) in w.eggs.iter().enumerate() {
        w.egrid.insert(j, e.pos[0], e.pos[2]);
    }
    for ci in 0..n {
        let c = &w.creatures[ci];
        let r = c.abil.sense;
        let mut p = Percept { other_dist: f32::MAX, ..Default::default() };
        let mut young = 0.0f32;
        let my_len = c.length().max(1e-4);
        w.cgrid.near(c.pos[0], c.pos[2], r, |j| {
            if j == ci {
                return;
            }
            let o = &w.creatures[j];
            let d = dist3(c.pos, o.pos);
            if d > r {
                return;
            }
            if d < p.other_dist {
                p.other_dist = d;
                p.other = Some(j);
            }
            let kin = Genome::kinship(&c.genome.marker, &o.genome.marker);
            if o.species == c.species && kin > 0.6 {
                p.kin_near += 1;
            }
            let ratio = o.length() / my_len;
            let hunter = ((o.genome.diet - 0.3) * 2.0).clamp(0.0, 1.0) * (ratio * 0.8).min(2.0);
            let mut danger = hunter * (1.0 - kin);
            if o.target == Some(c.id) && o.action == crate::creature::Action::Attacking {
                danger += 1.0;
            }
            let bond = c.mind.bond(o.id);
            if bond < 0.0 {
                danger -= bond;
            }
            let level = danger * (1.0 - d / r);
            if level > p.threat_level {
                p.threat_level = level;
                p.threat = Some(j);
            }
            if o.parent == Some(c.id) && o.maturity < 0.5 {
                young = young.max(1.0 - d / r);
            }
        });

        // Food, by what this creature can eat and has learned to like.
        let diet = c.genome.diet;
        let mouth = c.pheno.has(Tissue::Mouth);
        let mut best = 0.0f32;
        if mouth {
            let reach_alt = c.pos[2];
            w.pgrid.near(c.pos[0], c.pos[2], r, |j| {
                let pl = &w.plants[j];
                let pp = [pl.x, pl.depth, pl.alt];
                let top = pl.alt + pl.height();
                if reach_alt > top + c.length() {
                    return;
                }
                let at = [pl.x, pl.depth, reach_alt.min(top).max(pl.alt)];
                let d = dist3(c.pos, at);
                if d > r {
                    return;
                }
                let taste = c.mind.taste(plant_bin(&pl.genome.look));
                let s = (1.0 - diet) * (1.0 + taste) * (pl.matter + pl.fruit * 4.0).powf(0.3) / (d + 0.02);
                if s > best {
                    best = s;
                    p.food = Some(Food::Plant(j));
                    p.food_pos = [pp[0], pp[1], at[2]];
                }
            });
            w.rgrid.near(c.pos[0], c.pos[2], r, |j| {
                let rm = &w.remains[j];
                let d = dist3(c.pos, rm.pos);
                if d > r {
                    return;
                }
                let taste = c.mind.taste(creature_bin(&rm.look, rm.scale * rm.body.extent));
                let rot = rm.decay01();
                let s = (diet * 0.9 + rot * 0.3 + 0.05) * (1.0 + taste) * rm.matter.powf(0.3) / (d + 0.02);
                let s = if rm.offered_to.is_some() && rm.offered_to == c.parent { s * 10.0 } else { s };
                if s > best {
                    best = s;
                    p.food = Some(Food::Remains(j));
                    p.food_pos = rm.pos;
                }
            });
            w.egrid.near(c.pos[0], c.pos[2], r, |j| {
                let e = &w.eggs[j];
                if e.mother == Some(c.id) {
                    return;
                }
                let d = dist3(c.pos, e.pos);
                if d > r {
                    return;
                }
                let s = diet * 1.2 * e.matter.powf(0.3) / (d + 0.02);
                if s > best {
                    best = s;
                    p.food = Some(Food::Egg(j));
                    p.food_pos = e.pos;
                }
            });
        }
        let air = w.air[col_of(c.pos[0])];
        if p.food.is_none() && c.abil.filter && air > 0.01 {
            p.food = Some(Food::Air);
            p.food_pos = c.pos;
        }

        // Fill the senses.
        inputs.fill(0.0);
        let cap = c.abil.energy_cap.max(1e-6);
        let e01 = (c.energy / cap).clamp(0.0, 1.0);
        let temp = w.env.temperature(c.pos[0], c.pos[2]);
        let hot = 0.35 + c.genome.heat_tolerance * 0.8 + c.abil.armor * 0.5;
        let cold = -(0.35 + c.genome.cold_tolerance * 0.8 + c.pheno.frac(Tissue::Store) * 1.5);
        inputs[i::BIAS] = 1.0;
        inputs[i::ENERGY] = e01;
        inputs[i::HUNGER] = 1.0 - e01;
        inputs[i::HEALTH] = c.health;
        inputs[i::PAIN] = c.pain.min(1.0);
        inputs[i::AGE] = c.wear;
        inputs[i::READY] = c.ready() as u8 as f32;
        inputs[i::LIGHT] = w.env.light(c.pos[0], c.pos[2]);
        inputs[i::HEAT] = if temp > hot {
            (temp - hot).min(1.0)
        } else if temp < cold {
            (temp - cold).max(-1.0)
        } else {
            0.0
        };
        inputs[i::WET] =
            terrain::wetness(c.pos[0], c.pos[1] / crate::world::DEPTH, w.env.river) * c.on_ground() as u8 as f32;
        inputs[i::ALT] = c.pos[2] / ALT_MAX;
        if let Some(f) = p.food {
            let d = dist3(c.pos, p.food_pos);
            inputs[i::FOOD_NEAR] = if f == Food::Air { (air * 4.0).min(1.0) } else { 1.0 - (d / r).min(1.0) };
            inputs[i::FOOD_DIR] = ahead(c.heading, c.pos, p.food_pos);
            inputs[i::FOOD_TASTE] = match f {
                Food::Plant(j) => c.mind.taste(plant_bin(&w.plants[j].genome.look)),
                Food::Remains(j) => {
                    let rm = &w.remains[j];
                    c.mind.taste(creature_bin(&rm.look, rm.scale * rm.body.extent))
                }
                _ => 0.0,
            };
        }
        if let Some(j) = p.other {
            let o = &w.creatures[j];
            inputs[i::OTHER_NEAR] = 1.0 - (p.other_dist / r).min(1.0);
            inputs[i::OTHER_DIR] = ahead(c.heading, c.pos, o.pos);
            inputs[i::OTHER_SIZE] = (o.length() / my_len).ln().clamp(-2.0, 2.0) * 0.5;
            inputs[i::OTHER_KIN] =
                Genome::kinship(&c.genome.marker, &o.genome.marker) * (o.species == c.species) as u8 as f32;
            inputs[i::OTHER_BOND] = c.mind.bond(o.id);
            inputs[i::OTHER_RANK] = (o.rank - c.rank).clamp(-1.0, 1.0) * (o.species == c.species) as u8 as f32;
            inputs[i::OTHER_TASTE] = c.mind.taste(creature_bin(&o.genome.look, o.length()));
            inputs[i::OTHER_FIGHTING] = (o.action == crate::creature::Action::Attacking) as u8 as f32;
        }
        if let Some(j) = p.threat {
            inputs[i::THREAT] = p.threat_level.min(1.0);
            inputs[i::THREAT_DIR] = ahead(c.heading, c.pos, w.creatures[j].pos);
        }
        inputs[i::HEARD_DANGER] = c.mind.heard[mind::DANGER];
        inputs[i::HEARD_FOOD] = c.mind.heard[mind::FOOD];
        inputs[i::HEARD_MATE] = c.mind.heard[mind::MATE];
        inputs[i::HEARD_THREAT] = c.mind.heard[mind::THREAT];
        inputs[i::CROWD] = (p.kin_near as f32 / 6.0).min(1.0);
        inputs[i::YOUNG] = young;
        let home = c.mind.home[quarter];
        if home >= 0.0 {
            inputs[i::HOME_DIR] = ((home - c.pos[0]) / 0.3).clamp(-1.0, 1.0);
        }
        inputs[i::NOISE] = w.rng.signed();

        // Parasites steer their hosts: toward crowds and predators, away from fear.
        let mut steer = 0.0f32;
        for inf in &c.infections {
            if let Some(s) = w.strains.get(inf.slot as usize) {
                steer = steer.max(s.manipulation * inf.load);
            }
        }
        if steer > 0.0 {
            inputs[i::THREAT] *= 1.0 - steer;
            inputs[i::PAIN] *= 1.0 - steer;
        }

        let c = &mut w.creatures[ci];
        let reward = std::mem::take(&mut c.reward);
        c.brain.learn(reward);
        c.brain.step(&inputs);
        if steer > 0.0 {
            let out = &mut c.brain.out;
            out[crate::brain::o::FLEE] -= steer;
            out[crate::brain::o::SEEK_OTHER] += steer;
        }
        percepts[ci] = p;
    }
}

pub fn season_quarter(w: &World) -> usize {
    let days = (w.epoch + w.time) / 86400.0;
    let phase = (days / vesper_terrain::SEASON_DAYS).fract();
    ((phase * 4.0) as usize).min(3)
}
