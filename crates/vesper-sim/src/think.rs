//! Sensing and thinking: each creature looks around, feels its body and its
//! moods, recalls what it knows, and its brain decides what to do.

use crate::brain::{NI, Tone, i, o};
use crate::creature::Action;
use crate::genome::{Genome, Tissue};
use crate::history::CreatureId;
use crate::mind::{self, PlaceKind, creature_bin, plant_bin};
use crate::world::{ALT_MAX, TICK, World, col_of, dist2, dist3};

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
    /// Something its allies are fighting, or something fighting its allies.
    pub quarry: Option<usize>,
    /// Friends and kin in sight: how many, where on average, which way they go.
    pub allies: u32,
    pub ally_center: [f32; 3],
    pub ally_heading: [f32; 2],
    /// Friends and kin pressed close enough to share warmth.
    pub huddle: u32,
    /// The nearest cover from the weather and from things overhead.
    pub cover: Option<[f32; 3]>,
    /// A remembered danger close by.
    pub danger_at: Option<[f32; 2]>,
    /// The remembered feeding place it would go back to.
    pub recall: Option<[f32; 2]>,
    /// Which way along the band lies land it knows less.
    pub frontier: f32,
}

fn ahead(heading: f32, from: [f32; 3], to: [f32; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dz = to[1] - from[1];
    let len = (dx * dx + dz * dz).sqrt().max(1e-5);
    (dx * heading.cos() + dz * heading.sin()) / len
}

/// How rough the weather is at a spot, 0..1.
pub fn storm_at(w: &World, x: f32) -> f32 {
    (w.env.rain(x) + w.env.dust_at(x) + w.env.ash * 0.8 + w.env.cold_at(x) * 0.5).min(1.0)
}

pub fn think(w: &mut World, percepts: &mut Vec<Percept>) {
    let n = w.creatures.len();
    percepts.clear();
    percepts.resize(n, Percept::default());
    let quarter = season_quarter(w);
    let now = w.time;
    let tick = w.tick;
    let mut inputs = [0.0f32; NI];
    let mut huddles = Vec::new();
    w.rgrid.clear();
    for (j, r) in w.remains.iter().enumerate() {
        w.rgrid.insert(j, r.pos);
    }
    w.egrid.clear();
    for (j, e) in w.eggs.iter().enumerate() {
        w.egrid.insert(j, e.pos);
    }
    // Compact copies of what others can see of each creature and plant, so
    // the neighbour scans below stay in cache.
    let seen: Vec<Glimpse> = w
        .creatures
        .iter()
        .map(|o| Glimpse {
            pos: o.pos,
            vel: [o.vel[0], o.vel[1]],
            len: o.length(),
            hunter: ((o.genome.diet - 0.3) * 2.0).clamp(0.0, 1.0),
            marker: o.genome.marker,
            species: o.species.0,
            id: o.id,
            bin: creature_bin(&o.genome.look, o.length()) as u8,
            striking: if o.action == Action::Attacking { o.target } else { None },
            displaying: if o.action == Action::Displaying { o.target } else { None },
            parent: o.parent,
            young: o.maturity < 0.5,
        })
        .collect();
    let flora: Vec<Patch> = w
        .plants
        .iter()
        .map(|pl| Patch {
            pos: [pl.x, pl.depth, pl.alt],
            top: pl.alt + pl.height(),
            bin: plant_bin(&pl.genome.look) as u8,
            value: (pl.matter + pl.fruit * 4.0 + pl.genome.trap * pl.genome.glow * pl.adult_matter * 2.0).powf(0.3),
            cover: if pl.alt > 0.05 { 0.0 } else { (pl.leaf_area() * 4000.0).min(1.0) },
        })
        .collect();
    for ci in 0..n {
        let c = &w.creatures[ci];
        let me = &seen[ci];
        let r = c.abil.sense;
        let r2 = r * r;
        let mut p = Percept { other_dist: f32::MAX, ..Default::default() };
        let mut young = 0.0f32;
        let mut ally_hurt = 0.0f32;
        let mut quarry_score = 0.0f32;
        let mut threat_alt = 0.0f32;
        let my_len = me.len.max(1e-4);
        let mut center = [0.0f32; 3];
        let mut heading = [0.0f32; 2];
        let mut displayed_at = false;
        let ally = |o: &Glimpse| {
            o.species == me.species
                && (Genome::kinship(&me.marker, &o.marker) > 0.6 || c.mind.bond(o.id) > 0.2 || c.parent == Some(o.id))
        };
        let friend_of_mine = |id: CreatureId| c.mind.find(id).is_some_and(|a| a.bond > 0.2 || (a.kin && a.bond >= 0.0));
        w.cgrid.near(c.pos, r, |j| {
            if j == ci {
                return;
            }
            let o = &seen[j];
            let d2 = dist2(me.pos, o.pos);
            if d2 > r2 {
                return;
            }
            let d = d2.sqrt();
            let close = 1.0 - d / r;
            if d < p.other_dist {
                p.other_dist = d;
                p.other = Some(j);
                displayed_at = o.displaying == Some(me.id);
            }
            let kin = Genome::kinship(&me.marker, &o.marker);
            if o.species == me.species && kin > 0.6 {
                p.kin_near += 1;
            }
            let is_ally = ally(o);
            if is_ally {
                p.allies += 1;
                for k in 0..3 {
                    center[k] += o.pos[k];
                }
                heading[0] += o.vel[0];
                heading[1] += o.vel[1];
                if d < my_len * 1.2 + o.len * 0.6 {
                    p.huddle += 1;
                }
            }
            // Danger: its kind of body, what it has learned of its kind, and
            // what it remembers of it.
            let mut danger =
                if o.hunter > 0.0 { o.hunter * (o.len / my_len * 0.8).min(2.0) * (1.0 - kin) } else { 0.0 };
            danger += c.mind.wary(o.bin as usize) * (o.len / my_len).min(2.0) * (1.0 - kin);
            if o.striking == Some(me.id) {
                danger += 1.0;
            }
            if !c.mind.known.is_empty() {
                let bond = c.mind.bond(o.id);
                if bond < 0.0 {
                    danger -= bond;
                }
            }
            if is_ally {
                danger *= 0.3;
            }
            let level = danger * close;
            if level > p.threat_level {
                p.threat_level = level;
                p.threat = Some(j);
                threat_alt = ((o.pos[2] - me.pos[2]) / 0.15).clamp(-1.0, 1.0);
            }
            if o.young && o.parent == Some(me.id) {
                young = young.max(close);
            }
            // A fight involving a friend: whoever is hurting my friend, or
            // whatever my friend is hunting.
            if let Some(t) = o.striking
                && t != me.id
            {
                if friend_of_mine(t) && !is_ally {
                    ally_hurt = ally_hurt.max(close);
                    // Its own kind it only holds off; against another kind, it fights.
                    if close + 0.5 > quarry_score && o.species != me.species {
                        quarry_score = close + 0.5;
                        p.quarry = Some(j);
                    }
                } else if is_ally
                    && !friend_of_mine(t)
                    && let Some(tj) = w.find(t)
                    && close > quarry_score
                    // Joining in on its own kind only against a known foe.
                    && (w.creatures[tj].species != c.species || c.mind.bond(t) < 0.0)
                {
                    quarry_score = close;
                    p.quarry = Some(tj);
                }
            }
        });
        if p.allies > 0 {
            let k = 1.0 / p.allies as f32;
            p.ally_center = [center[0] * k, center[1] * k, center[2] * k];
            p.ally_heading = [heading[0] * k, heading[1] * k];
        }

        // Food, by what this creature can eat and has learned to like, and
        // cover, in the same look at the plants around it.
        let diet = c.genome.diet;
        let mouth = c.pheno.has(Tissue::Mouth);
        let mut best = 0.0f32;
        let mut cover_d = f32::MAX;
        let mut sheltered = 0.0f32;
        {
            let reach_alt = c.pos[2];
            let herb = 1.0 - diet;
            w.pgrid.near_any_alt(c.pos, r, |j| {
                let Some(pl) = flora.get(j) else { return };
                let pp = pl.pos;
                if pl.cover > 0.05 && pl.top > c.pos[2] + my_len {
                    let dx = ((pp[0] - c.pos[0]).powi(2) + (pp[1] - c.pos[1]).powi(2)).sqrt();
                    let spread = (pl.top - pp[2]) * 0.5;
                    if dx < spread {
                        sheltered = sheltered.max(pl.cover * (1.0 - dx / spread));
                    }
                    if dx < cover_d && dx < r {
                        cover_d = dx;
                        p.cover = Some([pp[0], pp[1], 0.0]);
                    }
                }
                if !mouth || reach_alt > pl.top + my_len {
                    return;
                }
                let at = [pp[0], pp[1], reach_alt.min(pl.top).max(pp[2])];
                let d2 = dist2(c.pos, at);
                if d2 > r2 {
                    return;
                }
                let d = d2.sqrt();
                let bin = pl.bin as usize;
                let taste = c.mind.taste(bin);
                let s = herb * (1.0 + taste) * (1.0 - c.mind.wary(bin)) * pl.value / (d + 0.02);
                if s > best {
                    best = s;
                    p.food = Some(Food::Plant(j));
                    p.food_pos = [pp[0], pp[1], at[2]];
                }
            });
        }
        if mouth {
            w.rgrid.near(c.pos, r, |j| {
                let rm = &w.remains[j];
                let d2 = dist2(c.pos, rm.pos);
                if d2 > r2 {
                    return;
                }
                // Not the body of one it grieves.
                if c.mind.grief.is_some_and(|g| g.who == rm.of) {
                    return;
                }
                let d = d2.sqrt();
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
            w.egrid.near(c.pos, r, |j| {
                let e = &w.eggs[j];
                if e.mother == Some(c.id) {
                    return;
                }
                let d2 = dist2(c.pos, e.pos);
                if d2 > r2 {
                    return;
                }
                let d = d2.sqrt();
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
        // Huddled bodies keep each other warm and dry.
        let huddled = (p.huddle as f32 * 0.25).min(0.75);
        if p.huddle >= 2 {
            let (cold, _) = c.bearable();
            if w.env.temperature(c.pos[0], c.pos[2]) < cold + 0.15 {
                huddles.push((c.species, c.id));
            }
        }
        let shelter = sheltered.max(huddled);
        if p.cover.is_none()
            && let Some([x, d]) = c.mind.nearest_shelter(c.pos[0], c.pos[1])
        {
            p.cover = Some([x, d, 0.0]);
        }

        // What it remembers.
        let recall = c.mind.best_food(c.pos[0], c.pos[1]);
        p.recall = recall.map(|(pl, _)| [pl.x, pl.depth]);
        let recall_value = recall.map_or(0.0, |(_, v)| v);
        let danger_place = c.mind.danger_near(c.pos[0], c.pos[1], 0.25);
        p.danger_at = danger_place.map(|(_, at)| at);
        let novelty = c.mind.novelty(c.pos[0]);
        p.frontier = c.mind.frontier(c.pos[0]);
        let storm = storm_at(w, c.pos[0]);

        // Moods move towards what the moment calls for: fear quickly, the
        // rest slowly, each by temperament.
        let (boldness, curiosity, sociality) = (c.genome.boldness, c.genome.curiosity, c.genome.sociality);
        let youth = (1.0 - c.maturity).clamp(0.0, 1.0);
        let mut mood = c.mind.mood;
        let before = mood;
        let alarm = p
            .threat_level
            .max(c.pain * 0.7)
            .max(c.mind.heard[mind::DANGER])
            .max(c.mind.heard[mind::ALOFT])
            .max(danger_place.map_or(0.0, |(v, _)| v * 0.8))
            .max(ally_hurt * 0.5)
            .min(1.0);
        if alarm > mood.fear {
            mood.fear += (alarm - mood.fear) * (0.5 * (1.3 - boldness)).min(1.0);
        } else {
            mood.fear += (alarm - mood.fear) * (0.01 + 0.05 * boldness);
        }
        let dull = curiosity * (1.0 - novelty) * (1.0 + youth * 0.5);
        mood.curious += (dull.min(1.0) - mood.curious) * 0.01;
        let alone = if p.allies == 0 { sociality * (1.0 + youth * 0.5) } else { 0.0 };
        mood.lonely += (alone.min(1.0) - mood.lonely) * if alone > mood.lonely { 0.004 } else { 0.08 };

        // Fill the senses.
        inputs.fill(0.0);
        let cap = c.abil.energy_cap.max(1e-6);
        let e01 = (c.energy / cap).clamp(0.0, 1.0);
        let temp = w.env.temperature(c.pos[0], c.pos[2]);
        let (cold, hot) = c.bearable();
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
        inputs[i::WET] = w.ground.wetness(c.pos[0], c.pos[1] / crate::world::DEPTH) * c.on_ground() as u8 as f32;
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
            inputs[i::OTHER_TASTE] = c.mind.taste(seen[j].bin as usize);
            inputs[i::OTHER_FIGHTING] = (o.action == Action::Attacking) as u8 as f32;
            inputs[i::OTHER_DISPLAY] = displayed_at as u8 as f32;
        }
        if let Some(j) = p.threat {
            inputs[i::THREAT] = p.threat_level.min(1.0);
            inputs[i::THREAT_DIR] = ahead(c.heading, c.pos, w.creatures[j].pos);
            inputs[i::THREAT_ALT] = threat_alt;
        }
        inputs[i::HEARD_DANGER] = c.mind.heard[mind::DANGER];
        inputs[i::HEARD_FOOD] = c.mind.heard[mind::FOOD];
        inputs[i::HEARD_MATE] = c.mind.heard[mind::MATE];
        inputs[i::HEARD_THREAT] = c.mind.heard[mind::THREAT];
        inputs[i::HEARD_GATHER] = c.mind.heard[mind::GATHER];
        inputs[i::HEARD_ALOFT] = c.mind.heard[mind::ALOFT];
        inputs[i::CROWD] = (p.kin_near as f32 / 6.0).min(1.0);
        inputs[i::YOUNG] = young;
        let home = c.mind.home[quarter];
        if home >= 0.0 {
            inputs[i::HOME_DIR] = ((home - c.pos[0]) / 0.3).clamp(-1.0, 1.0);
        }
        inputs[i::NOISE] = w.rng.signed();
        inputs[i::FEAR] = mood.fear;
        inputs[i::CURIOUS] = mood.curious;
        inputs[i::LONELY] = mood.lonely;
        inputs[i::TIRED] = mood.tired;
        inputs[i::STORM] = storm;
        if let Some(cv) = p.cover {
            inputs[i::SHELTER_DIR] = ahead(c.heading, c.pos, cv);
        }
        inputs[i::SHELTERED] = shelter;
        if let Some([x, d]) = p.recall {
            inputs[i::PLACE_FOOD] = recall_value.min(1.0);
            inputs[i::PLACE_FOOD_DIR] = ahead(c.heading, c.pos, [x, d, 0.0]);
        }
        if let Some((v, [x, d])) = danger_place {
            inputs[i::PLACE_DANGER] = v.min(1.0);
            inputs[i::PLACE_DANGER_DIR] = ahead(c.heading, c.pos, [x, d, 0.0]);
        }
        if p.allies > 0 && dist3(c.pos, p.ally_center) > my_len {
            inputs[i::GROUP_DIR] = ahead(c.heading, c.pos, p.ally_center);
        }
        inputs[i::ALLY_HURT] = ally_hurt;
        if let Some(q) = p.quarry {
            let qp = w.creatures[q].pos;
            inputs[i::QUARRY] = 1.0 - (dist3(c.pos, qp) / r).min(1.0);
            inputs[i::QUARRY_DIR] = ahead(c.heading, c.pos, qp);
        }
        inputs[i::NOVELTY] = novelty;

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
            inputs[i::FEAR] *= 1.0 - steer;
        }

        // Feelings that are rewards in themselves: relief from fear and from
        // loneliness, and, for the curious, new ground.
        let mut reward = std::mem::take(&mut w.creatures[ci].reward);
        reward += (before.fear - mood.fear).max(0.0) * 0.5;
        reward += (before.lonely - mood.lonely) * sociality;
        let explore_new = (tick + ci as u64) % 5 == 0;
        if explore_new {
            reward += curiosity * novelty * 0.08 * (1.0 + youth);
        }
        let tone = Tone {
            explore: (0.5 + mood.curious + youth * 0.5 - mood.fear * 0.5).clamp(0.2, 2.0),
            // Young brains are the most plastic: a sensitive period.
            plasticity: 1.0 + youth,
        };
        let resting = w.creatures[ci].action == Action::Resting;
        let c = &mut w.creatures[ci];
        c.shelter = shelter;
        c.brain.step(&inputs, reward, tone);
        c.mind.mood = mood;
        if steer > 0.0 {
            let out = &mut c.brain.out;
            out[o::FLEE] -= steer;
            out[o::SEEK_OTHER] += steer;
        }
        if explore_new {
            c.brain.settle(TICK * 5.0, resting);
            c.mind.visit(c.pos[0]);
            // Good cover found when it was needed is worth coming back to.
            if sheltered > 0.5 && (storm > 0.2 || resting) {
                let (x, d) = (c.pos[0], c.pos[1]);
                c.mind.remember(PlaceKind::Shelter, x, d, sheltered, now, false);
            }
        }
        if (tick + ci as u64) % 50 == 0 {
            c.mind.forget(TICK * 50.0);
            c.mind.known.retain(|a| now - a.seen < 3600.0 || a.kin);
            if c.mind.grief.is_some_and(|g| g.until < now) {
                c.mind.grief = None;
            }
        }
        percepts[ci] = p;
    }
    for (sp, id) in huddles {
        w.first(crate::history::Behavior::Huddling, sp, Some(id));
    }
}

struct Glimpse {
    pos: [f32; 3],
    vel: [f32; 2],
    len: f32,
    /// How much of a hunter it is, 0..1.
    hunter: f32,
    marker: [u8; 8],
    species: u64,
    id: CreatureId,
    bin: u8,
    striking: Option<CreatureId>,
    displaying: Option<CreatureId>,
    parent: Option<CreatureId>,
    young: bool,
}

struct Patch {
    pos: [f32; 3],
    top: f32,
    bin: u8,
    value: f32,
    /// How much cover its leaves give, 0..1.
    cover: f32,
}

pub fn season_quarter(w: &World) -> usize {
    let days = (w.epoch + w.time) / 86400.0;
    let phase = (days / vesper_terrain::SEASON_DAYS).fract();
    ((phase * 4.0) as usize).min(3)
}
