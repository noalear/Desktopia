//! Acting: moving, eating, fighting, mating, sharing, grooming, guarding and
//! signalling, as each creature's brain decided.

use crate::brain::o;
use crate::creature::{Action, Creature};
use crate::genome::{Genome, Tissue};
use crate::history::{Behavior, DeathCause, Locomotion};
use crate::life;
use crate::mind::{self, creature_bin, plant_bin};
use crate::think::{Food, Percept, season_quarter};
use crate::world::{ALT_MAX, DEPTH, E_DETRITUS, E_MEAT, E_PLANT, TICK, World, col_of, dist3, pair_mut, wrap_angle};
use vesper_terrain::{self as terrain, WORLD_W};

fn unit(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
    [d[0] / l, d[1] / l, d[2] / l]
}

pub fn reach(c: &Creature) -> f32 {
    c.length() * 0.7 + 0.012
}

pub fn movement(w: &mut World, percepts: &[Percept]) {
    let dt = TICK;
    let positions: Vec<[f32; 3]> = w.creatures.iter().map(|c| c.pos).collect();
    let quarter = season_quarter(w);
    for (ci, c) in w.creatures.iter_mut().enumerate() {
        if c.action == Action::Held || c.doom.is_some() {
            c.vel = [0.0; 3];
            continue;
        }
        let p = &percepts[ci];
        let out = c.brain.out;
        let mut dir = [0.0f32; 3];
        let add = |dir: &mut [f32; 3], v: [f32; 3], k: f32| {
            dir[0] += v[0] * k;
            dir[1] += v[1] * k;
            dir[2] += v[2] * k;
        };
        c.heading = wrap_angle(c.heading + out[o::TURN] * 1.2 * dt);
        add(&mut dir, [c.heading.cos(), c.heading.sin(), 0.0], out[o::THRUST].max(0.0));
        if p.food.is_some() && p.food != Some(Food::Air) {
            add(&mut dir, unit(c.pos, p.food_pos), out[o::SEEK_FOOD].max(0.0));
        }
        if let Some(j) = p.other {
            add(&mut dir, unit(c.pos, positions[j]), out[o::SEEK_OTHER]);
        }
        let flee = out[o::FLEE].max(0.0);
        if flee > 0.0 {
            if let Some(j) = p.threat {
                add(&mut dir, unit(positions[j], c.pos), flee * 1.5);
            } else if let Some(j) = p.other {
                add(&mut dir, unit(positions[j], c.pos), flee);
            }
        }
        let home = c.mind.home[quarter];
        let go_home = out[o::HOME].max(0.0);
        if go_home > 0.0 {
            if home >= 0.0 {
                dir[0] += (home - c.pos[0]).signum() * go_home;
            } else {
                // No remembered ground: head for comfortable air.
                let t = w.env.temperature(c.pos[0], c.pos[2]);
                dir[0] += (t * 2.0).clamp(-1.0, 1.0) * go_home;
            }
        }
        let want = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        if want > 0.02 {
            let goal = dir[1].atan2(dir[0]);
            let turn = wrap_angle(goal - c.heading);
            c.heading = wrap_angle(c.heading + turn.clamp(-2.5 * dt, 2.5 * dt));
        }
        let effort = want.min(1.0);
        let guarding = out[o::GUARD] > 0.3;
        let mut speed = c.abil.speed * effort * (0.5 + 0.5 * c.health) * if guarding { 0.3 } else { 1.0 };
        let loco = c.abil.locomotion;
        let airborne = matches!(loco, Some(Locomotion::Drift | Locomotion::Swim | Locomotion::Glide));
        let on_floor = !airborne || c.pos[2] <= 0.0;
        let wet = on_floor && terrain::in_river(c.pos[0], c.pos[1] / DEPTH, w.env.river);
        if wet {
            speed *= 0.5;
        }
        let face = [c.heading.cos(), c.heading.sin()];
        let mut target = [face[0] * speed, face[1] * speed, 0.0];
        if airborne {
            let lift = (out[o::RISE] + dir[2]).clamp(-1.0, 1.0);
            target[2] = lift * c.abil.speed * 0.6;
            if c.abil.buoyancy < 1.0 {
                target[2] -= (1.0 - c.abil.buoyancy) * 0.02 * (1.0 - effort);
            }
            let drift = if loco == Some(Locomotion::Drift) { 1.0 } else { 0.4 };
            target[0] += w.env.wind(c.pos[2]) * drift;
        } else {
            target[2] = -0.05;
        }
        if wet {
            target[0] -= 0.004 * (1.0 + w.env.river.flood * 2.0);
        }
        let k = (dt * 3.0).min(1.0);
        for a in 0..3 {
            c.vel[a] += (target[a] - c.vel[a]) * k;
        }
        for a in 0..3 {
            c.pos[a] += c.vel[a] * dt;
        }
        if c.pos[0] < 0.02 || c.pos[0] > WORLD_W - 0.02 {
            c.pos[0] = c.pos[0].clamp(0.02, WORLD_W - 0.02);
            c.heading = wrap_angle(std::f32::consts::PI - c.heading);
            c.vel[0] = 0.0;
        }
        if c.pos[1] < 0.004 || c.pos[1] > DEPTH - 0.004 {
            c.pos[1] = c.pos[1].clamp(0.004, DEPTH - 0.004);
            c.heading = wrap_angle(-c.heading);
            c.vel[1] = 0.0;
        }
        c.pos[2] = c.pos[2].clamp(0.0, ALT_MAX);
        let v = (c.vel[0] * c.vel[0] + c.vel[1] * c.vel[1] + c.vel[2] * c.vel[2]).sqrt();
        c.gait = (c.gait + v / c.length().max(1e-3) * dt * 2.0) % std::f32::consts::TAU;
        c.effort = effort;
        c.action = if guarding {
            Action::Guarding
        } else if flee > 0.3 && p.threat.is_some() {
            Action::Fleeing
        } else if v > c.abil.speed * 0.15 {
            Action::Moving
        } else {
            Action::Idle
        };
        c.target = None;
    }
}

/// Eats some matter: energy from it now, growth from it if there's room,
/// and the rest passed back to the soil.
pub fn digest(w: &mut World, ci: usize, m: f32, e_per: f32, eff: f32, bin: usize, harm: f32) {
    let c = &mut w.creatures[ci];
    let cap = c.abil.energy_cap.max(1e-6);
    let gain = m * eff * e_per;
    c.energy = (c.energy + gain).min(cap);
    let room = (life::growth_room(c)).max(0.0);
    let keep = (m * eff).min(room);
    c.matter += keep;
    c.tally.eaten += m;
    c.intake += gain / cap;
    let valence = (gain / cap * 4.0 - harm).clamp(-1.0, 1.0);
    c.reward += valence;
    c.last_meal = Some((bin as u8, valence, w.time));
    c.mind.learn_taste(bin, valence, 0.2);
    let pos = c.pos;
    w.shed(pos, m - keep);
}

pub fn actions(w: &mut World, percepts: &[Percept]) {
    let dt = TICK;
    let n = w.creatures.len();
    for ci in 0..n {
        if w.creatures[ci].doom.is_some() || w.creatures[ci].action == Action::Held {
            continue;
        }
        let p = &percepts[ci];
        let out = w.creatures[ci].brain.out;
        let r = reach(&w.creatures[ci]);

        // Eating.
        if out[o::EAT] > 0.0
            && let Some(food) = p.food
        {
            let c = &w.creatures[ci];
            let close = food == Food::Air || dist3(c.pos, p.food_pos) < r;
            if close {
                eat(w, ci, food, out[o::EAT]);
            }
        }

        let other = p.other.filter(|&j| p.other_dist < r && w.creatures[j].doom.is_none());

        // Fighting.
        if out[o::ATTACK] > 0.3
            && let Some(j) = other
        {
            let c = &w.creatures[ci];
            if c.pheno.has(Tissue::Mouth) || c.pheno.has(Tissue::Spine) {
                life::attack(w, ci, j, out[o::ATTACK]);
            }
        }
        if w.creatures[ci].doom.is_some() {
            continue;
        }

        // Mating.
        if out[o::MATE] > 0.3
            && let Some(j) = other
            && w.creatures[ci].genome.sexual > 0.5
            && w.creatures[j].brain.out[o::MATE] > 0.3
        {
            let (a, b) = (&w.creatures[ci], &w.creatures[j]);
            let accept = |x: &Creature, y: &Creature| {
                Genome::look_distance(&x.genome.mate_pref, &y.genome.look) < 1.0 - x.genome.choosiness * 0.85
            };
            if a.ready() && b.ready() && a.species == b.species && accept(a, b) && accept(b, a) {
                let (mother, father) = if a.energy >= b.energy { (ci, j) } else { (j, ci) };
                life::breed(w, mother, Some(father));
                w.creatures[ci].action = Action::Mating;
                w.creatures[ci].target = Some(w.creatures[j].id);
            }
        }

        // Breeding alone.
        if out[o::LAY] > 0.3 && w.creatures[ci].genome.sexual <= 0.5 && w.creatures[ci].ready() {
            life::breed(w, ci, None);
            w.creatures[ci].action = Action::Laying;
        }
        if w.creatures[ci].doom.is_some() {
            continue;
        }

        // Sharing food.
        if out[o::SHARE] > 0.3
            && let Some(j) = other
        {
            let (a, b) = pair_mut(&mut w.creatures, ci, j);
            if a.energy > a.abil.energy_cap * 0.4 && b.energy < b.abil.energy_cap * 0.8 {
                let give = a.energy * 0.12 * out[o::SHARE];
                a.energy -= give;
                b.energy = (b.energy + give * 0.9).min(b.abil.energy_cap);
                // Young are fed matter too, so they grow.
                if b.parent == Some(a.id) && b.maturity < 0.9 {
                    let m = (a.matter * 0.02).min(crate::life::growth_room(b).max(0.0));
                    a.matter -= m;
                    b.matter += m;
                }
                b.reward += 0.3;
                let kin = Genome::kinship(&a.genome.marker, &b.genome.marker) > 0.6;
                let now = w.time;
                b.mind.feel(a.id, 0.25, kin, now);
                a.mind.feel(b.id, 0.05, kin, now);
                a.tally.kindness += 1;
                a.action = Action::Sharing;
                a.target = Some(b.id);
                let sp = a.species;
                let id = a.id;
                w.first(Behavior::FoodSharing, sp, Some(id));
            }
        }

        // Grooming and tending wounds.
        if out[o::GROOM] > 0.3
            && let Some(j) = other
        {
            let now = w.time;
            let (a, b) = pair_mut(&mut w.creatures, ci, j);
            if a.species == b.species {
                b.bleeding *= 0.6;
                b.health = (b.health + 0.01).min(1.0);
                for inf in &mut b.infections {
                    if let Some(s) = w.strains.get(inf.slot as usize)
                        && s.visible > 0.5
                    {
                        inf.load = (inf.load - 0.03).max(0.0);
                    }
                }
                b.reward += 0.15;
                let kin = Genome::kinship(&a.genome.marker, &b.genome.marker) > 0.6;
                b.mind.feel(a.id, 0.12, kin, now);
                a.mind.feel(b.id, 0.06, kin, now);
                a.tally.kindness += 1;
                a.action = Action::Grooming;
                a.target = Some(b.id);
                // Close contact passes on whatever either carries.
                life::contact(w, ci, j, 2.0 * dt);
                let (sp, id) = (w.creatures[ci].species, w.creatures[ci].id);
                w.first(Behavior::Grooming, sp, Some(id));
            }
        }

        if out[o::GUARD] > 0.3 {
            let c = &w.creatures[ci];
            let mine = w.eggs.iter().any(|e| e.mother == Some(c.id) && dist3(e.pos, c.pos) < 0.06);
            if mine {
                let (sp, id) = (c.species, c.id);
                w.first(Behavior::Guarding, sp, Some(id));
            }
        }

        // Light signals.
        if out[o::SIGNAL] > 0.2 {
            signal(w, ci, &out, p);
        } else {
            w.creatures[ci].flash *= 0.6;
        }

        // Watching others: what they eat, where they go, how they act.
        if let Some(j) = p.other
            && (w.tick + ci as u64) % 5 == 0
        {
            observe(w, ci, j);
        }
    }
}

fn eat(w: &mut World, ci: usize, food: Food, drive: f32) {
    let dt = TICK;
    let c = &w.creatures[ci];
    let bite = c.abil.bite * dt * drive.min(1.0);
    let diet = c.genome.diet;
    let id = c.id;
    match food {
        Food::Air => {
            let col = col_of(c.pos[0]);
            let m = (w.air[col] * 0.01 * dt).min(c.abil.gut * dt);
            if m <= 0.0 {
                return;
            }
            w.air[col] -= m;
            digest(w, ci, m, E_DETRITUS, 0.6, 48, 0.0);
            w.creatures[ci].action = Action::Eating;
        }
        Food::Plant(j) => {
            if j >= w.plants.len() || bite <= 0.0 {
                return;
            }
            let pl = &mut w.plants[j];
            let from_fruit = bite.min(pl.fruit);
            pl.fruit -= from_fruit;
            let from_body = (bite - from_fruit).min((pl.matter - pl.adult_matter * 0.15).max(0.0));
            pl.matter -= from_body;
            pl.health = (pl.health - from_body / pl.matter.max(1e-4) * 0.08).max(0.0);
            let spines = pl.genome.spines;
            let toxin = pl.genome.toxin;
            let bin = plant_bin(&pl.genome.look);
            let seed = if from_fruit > 0.0 {
                Some(crate::plant::Seed {
                    genome: pl.genome.clone(),
                    species: pl.species,
                    generation: pl.generation + 1,
                })
            } else {
                None
            };
            let m = from_fruit + from_body;
            if m <= 0.0 {
                return;
            }
            let eff = 0.15 + 0.85 * (1.0 - diet);
            let c = &mut w.creatures[ci];
            let harm = spines * 0.6 * (from_body / m) + toxin * 1.5;
            let hurt = (spines * from_body * 3.0 + toxin * m * 4.0) / c.matter.max(1e-4);
            c.health -= hurt * 0.3;
            c.pain += hurt;
            if let Some(g) = seed
                && c.seed.is_none()
            {
                c.seed = Some(g);
                c.seed_timer = 20.0 + c.genome.adult_scale * 6000.0;
            }
            c.action = Action::Eating;
            // Fruit is sweeter than leaves.
            let e = if from_fruit > 0.0 { E_PLANT * 1.6 } else { E_PLANT };
            digest(w, ci, m, e, eff, bin, harm);
            if w.creatures[ci].health <= 0.0 {
                w.creatures[ci].doom = Some(DeathCause::Starvation);
            }
        }
        Food::Remains(j) => {
            if j >= w.remains.len() {
                return;
            }
            let rm = &mut w.remains[j];
            let m = bite.min(rm.matter);
            if m <= 0.0 {
                return;
            }
            rm.matter -= m;
            let rot = rm.decay01();
            let toxin = rm.toxin;
            let bin = creature_bin(&rm.look, rm.scale * rm.body.extent);
            let strain = rm.strain;
            let offered = rm.offered_to;
            let killed_by_other = rm.killer.is_some_and(|k| k != id);
            let eff = (0.1 + 0.9 * diet) * (1.0 - rot) + 0.5 * rot;
            let e = E_MEAT * (1.0 - rot * 0.6);
            let c = &mut w.creatures[ci];
            let hurt = toxin * m * 4.0 / c.matter.max(1e-4);
            c.health -= hurt * 0.3;
            c.pain += hurt;
            c.action = Action::Eating;
            let mother = offered.is_some() && offered == c.parent;
            let sp = c.species;
            digest(w, ci, m, e, eff, bin, toxin * 1.5);
            if let Some(s) = strain {
                life::infect_by_eating(w, ci, s);
            }
            if mother {
                w.first(Behavior::Matriphagy, sp, Some(id));
            } else if killed_by_other {
                w.first(Behavior::Scavenging, sp, Some(id));
            }
        }
        Food::Egg(j) => {
            if j >= w.eggs.len() {
                return;
            }
            let e = &mut w.eggs[j];
            let m = bite.min(e.matter);
            e.matter -= m;
            let eff = 0.2 + 0.8 * diet;
            w.creatures[ci].action = Action::Eating;
            digest(w, ci, m, E_MEAT, eff, 48, 0.0);
        }
    }
}

/// Flashes a meaning in this creature's light dialect. Whoever sees it
/// understands it as best their own experience allows, and learns.
fn signal(w: &mut World, ci: usize, out: &[f32; crate::brain::NO], p: &Percept) {
    let mut meaning = 0;
    for m in 1..mind::MEANINGS {
        if out[o::SAY + m] > out[o::SAY + meaning] {
            meaning = m;
        }
    }
    let now = w.time;
    let innovate = w.rng.chance(w.creatures[ci].genome.innovation * 0.05);
    let fresh = w.rng.below(mind::SYMBOLS) as u8;
    let c = &mut w.creatures[ci];
    if innovate {
        c.mind.voice[meaning] = fresh;
    }
    let symbol = c.mind.voice[meaning];
    c.flash = out[o::SIGNAL].clamp(0.0, 1.0);
    c.symbol = symbol;
    c.lure = Some((meaning, now));
    // Spend a little light.
    c.energy -= c.abil.upkeep * 0.2 * TICK * (1.0 + c.pheno.frac(Tissue::Glow) * 4.0);
    let (pos, range, species, id, rank, age, has_food, attacking) =
        (c.pos, c.abil.glow, c.species, c.id, c.rank, c.age, p.food.is_some(), c.action == Action::Attacking);
    let ready = c.ready();
    let threat_near = p.threat_level > 0.3;
    let mut listeners = Vec::new();
    w.cgrid.near(pos[0], pos[2], range, |j| {
        if j != ci && dist3(pos, w.creatures[j].pos) < range {
            listeners.push(j);
        }
    });
    let mut understood = 0;
    for j in listeners {
        let l = &mut w.creatures[j];
        let d = dist3(pos, l.pos);
        let strength = 1.0 - d / range;
        let heard = l.mind.understand(symbol);
        if let Some((m, conf)) = heard {
            l.mind.hear(m, conf * strength, pos[0]);
        }
        // Check the signal against what can be seen.
        let truth = match meaning {
            mind::DANGER => threat_near,
            mind::FOOD => has_food,
            mind::MATE => ready && l.species == species,
            _ => attacking || l.mind.bond(id) < 0.0,
        };
        if truth {
            l.mind.associate(symbol, meaning, 12);
            if heard.map(|h| h.0) == Some(meaning) {
                understood += 1;
            }
            // Conformity: the young and the subordinate copy their elders' voice.
            let elder = l.species == species && (age > l.age * 1.5 || rank > l.rank + 0.2 || l.parent == Some(id));
            if elder && w.rng.chance(l.genome.imitation * 0.3) {
                l.mind.voice[meaning] = symbol;
            }
        } else if let Some((m, _)) = heard {
            l.mind.distrust(symbol, m, 6);
        }
    }
    if understood > 0 {
        w.first(Behavior::Signaling, species, Some(id));
    }
}

/// Learning by watching another: what it eats and how that went, and where
/// it goes to feed.
fn observe(w: &mut World, ci: usize, j: usize) {
    let now = w.time;
    let quarter = season_quarter(w);
    let (c, o) = pair_mut(&mut w.creatures, ci, j);
    let rate = c.genome.imitation;
    if rate <= 0.02 {
        return;
    }
    if let Some((bin, valence, t)) = o.last_meal
        && now - t < 3.0
    {
        c.mind.learn_taste(bin as usize, valence, rate * 0.3);
    }
    if o.species != c.species {
        return;
    }
    let trusted = c.parent == Some(o.id) || c.mind.bond(o.id) > 0.3 || o.age > c.age * 2.0;
    if !trusted {
        return;
    }
    // Copy a route from someone who is eating well.
    let fed = o.energy / o.abil.energy_cap.max(1e-6);
    if o.mind.home[quarter] >= 0.0 && fed > 0.6 && (c.mind.home[quarter] < 0.0 || c.intake < o.intake) {
        let h = c.mind.home[quarter];
        let target = o.mind.home[quarter];
        c.mind.home[quarter] = if h < 0.0 { target } else { h + (target - h) * rate * 0.2 };
        c.mind.route_from = o.mind.route_from;
    }
    // Copy what it does when it's doing well.
    if o.reward > 0.05 && rate > 0.3 {
        let target = o.brain.out;
        c.brain.imitate(&target, rate * 0.03, &[o::EAT, o::SEEK_FOOD, o::FLEE, o::ATTACK, o::HOME]);
    }
}
