//! Acting: moving, eating, fighting, mating, sharing, grooming, guarding and
//! signalling, as each creature's brain decided.

use crate::brain::o;
use crate::creature::{Action, Creature};
use crate::frame::Moment;
use crate::genetics;
use crate::genome::{Genome, Tissue};
use crate::ground::Ground;
use crate::history::{Behavior, CreatureId, DeathCause};
use crate::life;
use crate::mind::{self, Grief, PlaceKind, creature_bin, plant_bin};
use crate::organ::Fare;
use crate::physics::{self, AIR, Drive, MOTOR_COST, Medium, Rig, STEP, STEPS_PER_TICK, WATER};
use crate::think::{Food, Percept, season_quarter};
use crate::world::{
    ALT_MAX, DEPTH, E_DETRITUS, E_MEAT, E_PLANT, Env, Remains, TICK, World, col_of, dist3, pair_mut, wrap_angle,
};
use vesper_terrain::WORLD_W;

/// Above this a creature will mate when it can, even if it also breeds alone.
const FACULTATIVE: f32 = 0.25;

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
    let now = w.time;
    let positions: Vec<[f32; 3]> = w.creatures.iter().map(|c| c.pos).collect();
    // Whoever each creature is backing down from, if anyone.
    let yielding: Vec<Option<usize>> = w
        .creatures
        .iter()
        .map(|c| c.mind.yielding.filter(|&(_, until)| until > now).and_then(|(id, _)| w.find(id)))
        .collect();
    let ids: Vec<CreatureId> = w.creatures.iter().map(|c| c.id).collect();
    let quarter = season_quarter(w);
    let mut events = Vec::new();
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
            } else if let Some([x, d]) = p.danger_at {
                add(&mut dir, unit([x, d, c.pos[2]], c.pos), flee);
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
        // Keep with friends and kin: towards them, and the way they go. A
        // call to gather draws it towards whoever called.
        let follow = out[o::FOLLOW].max(0.0);
        if follow > 0.0 {
            if p.allies > 0 {
                if dist3(c.pos, p.ally_center) > c.length() * 1.5 {
                    add(&mut dir, unit(c.pos, p.ally_center), follow * 0.8);
                }
                let [hx, hz] = p.ally_heading;
                let hl = (hx * hx + hz * hz).sqrt();
                if hl > 1e-5 {
                    add(&mut dir, [hx / hl, hz / hl, 0.0], follow * 0.5);
                }
            }
            let call = c.mind.heard[mind::GATHER];
            if call > 0.05 && (c.mind.heard_x[mind::GATHER] - c.pos[0]).abs() > c.length() * 2.0 {
                dir[0] += (c.mind.heard_x[mind::GATHER] - c.pos[0]).signum() * follow * call;
            }
        }
        // Go and see somewhere new.
        let explore = out[o::EXPLORE].max(0.0);
        if explore > 0.0 {
            if p.frontier != 0.0 {
                dir[0] += p.frontier * explore;
            } else {
                add(&mut dir, [c.heading.cos(), c.heading.sin(), 0.0], explore * 0.5);
            }
        }
        // Go back to where food was.
        let recall = out[o::RECALL].max(0.0);
        if recall > 0.0
            && let Some([x, d]) = p.recall
        {
            add(&mut dir, unit(c.pos, [x, d, c.pos[2]]), recall);
            if (x - c.pos[0]).abs() < 0.02 && (d - c.pos[1]).abs() < 0.04 && p.food.is_none() {
                // Nothing here any more.
                c.mind.disappointed(x, d);
            }
        }
        // Make for cover, and get down under it.
        let shelter = out[o::SHELTER].max(0.0);
        if shelter > 0.0
            && let Some(cv) = p.cover
        {
            if dist3([c.pos[0], c.pos[1], cv[2]], cv) > c.length() * 0.5 {
                add(&mut dir, unit([c.pos[0], c.pos[1], 0.0], cv), shelter);
            }
            dir[2] -= shelter * 0.5;
        }
        // Join a fight its friends are in.
        let attack = out[o::ATTACK].max(0.0);
        if attack > 0.3
            && let Some(q) = p.quarry
        {
            add(&mut dir, unit(c.pos, positions[q]), attack);
        }
        // Grief draws it back to the body.
        let mut mourning = false;
        if let Some(g) = c.mind.grief
            && g.until > now
            && p.threat_level < 0.3
            && c.energy > 0.35 * c.abil.energy_cap
        {
            let d = dist3(c.pos, g.pos);
            if d > c.length() * 1.5 {
                add(&mut dir, unit(c.pos, g.pos), 0.8);
            } else {
                mourning = true;
            }
        }
        // Backing down from whoever won.
        let backing = yielding[ci];
        if let Some(j) = backing {
            add(&mut dir, unit(positions[j], c.pos), 1.5);
        }
        // Whatever the brain wants, a body turns back from heat or cold it
        // can't stand: towards the night edge and up into thinner air from
        // heat, towards the day edge and down from cold.
        let (cold, hot) = c.bearable();
        let t = w.env.temperature(c.pos[0], c.pos[2]);
        let edge = 0.1;
        // The more the weather has already worn into it, the harder it goes.
        let ex = c.exposure;
        let flee_heat = if t > hot - edge {
            ((t - hot + edge) * 6.0 + ex.heat * 1.5).min(2.5)
        } else if t < cold + edge {
            -((cold + edge - t) * 6.0 + ex.chill * 1.5).min(2.5)
        } else {
            0.0
        };
        dir[0] += flee_heat;
        dir[2] += flee_heat;
        let threatened = p.threat_level > 0.25;
        let resting = out[o::REST] > 0.3 && !threatened && backing.is_none() && flee_heat.abs() < 0.3;
        let rival = p.other.filter(|&j| dist3(c.pos, positions[j]) < c.abil.sense * 0.6);
        let displaying = out[o::DISPLAY] > 0.3 && rival.is_some() && backing.is_none();
        if displaying && let Some(j) = rival {
            // Face the rival and hold ground.
            let to = unit(c.pos, positions[j]);
            dir = [to[0] * 0.05, to[1] * 0.05, 0.0];
        }
        let want = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        if want > 0.02 {
            let goal = dir[1].atan2(dir[0]);
            let turn = wrap_angle(goal - c.heading);
            c.heading = wrap_angle(c.heading + turn.clamp(-2.5 * dt, 2.5 * dt));
        }
        let mut effort = want.min(1.0);
        if resting || mourning {
            effort *= 0.05;
        } else if displaying {
            effort = effort.min(0.1);
        }
        // Tiredness builds with effort and passes with rest.
        let m = &mut c.mind.mood;
        if resting {
            m.tired = (m.tired - 0.004).max(0.0);
        } else {
            // Gills and lungs keep a hard-working body going longer.
            m.tired = (m.tired + 0.00012 + effort * 0.0003 * (1.0 - 0.5 * c.abil.breath)).min(1.0);
        }
        let weary = 1.0 - m.tired * 0.4;
        let guarding = out[o::GUARD] > 0.3;
        // Its rhythm run whichever way has carried it further.
        let way = if c.back > c.stride { -1.0 } else { 1.0 };
        // Chilled bodies stiffen and slow; parched or choking ones flag.
        let torpor = 1.0
            - 0.5 * c.exposure.chill.min(1.0)
            - 0.25 * (c.exposure.thirst - 0.5).clamp(0.0, 1.0)
            - 0.25 * (c.exposure.grit - 0.4).clamp(0.0, 1.0)
            - 0.5 * c.venom.min(1.0);
        let torpor = torpor.max(0.1);
        let power = effort * way * (0.5 + 0.5 * c.health) * weary * torpor * if guarding { 0.3 } else { 1.0 };
        c.drive = [power, (out[o::RISE] + dir[2]).clamp(-1.0, 1.0) * if resting { 0.2 } else { 1.0 }];
        c.effort = effort;
        let v = (c.vel[0] * c.vel[0] + c.vel[1] * c.vel[1] + c.vel[2] * c.vel[2]).sqrt();
        c.target = None;
        c.focus = None;
        c.action = if backing.is_some() {
            c.target = backing.map(|j| ids[j]);
            Action::Fleeing
        } else if guarding {
            Action::Guarding
        } else if displaying {
            c.target = rival.map(|j| ids[j]);
            Action::Displaying
        } else if mourning {
            c.focus = c.mind.grief.map(|g| g.pos);
            events.push((Behavior::Mourning, c.species, c.id));
            Action::Mourning
        } else if resting {
            if p.huddle > 0 {
                events.push((Behavior::Sleep, c.species, c.id));
            }
            Action::Resting
        } else if flee > 0.3 && p.threat.is_some() {
            Action::Fleeing
        } else if v > c.length() * 0.1 {
            Action::Moving
        } else {
            Action::Idle
        };
    }
    for (b, sp, id) in events {
        w.first(b, sp, Some(id));
    }
}

/// Runs the bodies of every creature and corpse through one tick of physics,
/// and moves each through the world by however far its body carried it.
/// Bodies don't touch each other here, so `helpers` extra threads can share
/// them out with the same result as one thread.
pub fn physics(w: &mut World, helpers: usize) {
    if w.coarse {
        return coarse(w);
    }
    let World { creatures, remains, env, ground, .. } = w;
    let (env, ground) = (&*env, &*ground);
    crate::sim::share(creatures, helpers, |c| body(c, env, ground));
    crate::sim::share(remains, helpers.min(1), |r| corpse(r, env, ground));
}

fn body(c: &mut Creature, env: &Env, ground: &Ground) {
    let mask = physics::present(&c.body, c.maturity, c.lost);
    let scale = c.scale();
    if mask != c.rig.mask || (scale / c.rig.scale - 1.0).abs() > 0.04 {
        c.rig = c.rig.rebuild(&c.body, mask, scale);
    }
    c.rig.ensure_pairs(&c.body);
    c.moments.clear();
    c.moments.push(Moment::of(&c.rig, c.pos, c.heading));
    for _ in 0..STEPS_PER_TICK {
        let held = c.action == Action::Held || c.doom.is_some();
        let drive = Drive { power: c.drive[0], rise: c.drive[1], hz: c.genome.gait_hz, limp: held };
        let before = c.rig.parts.first().map_or(0.0, |p| p.pos[1]);
        let du = carry(&mut c.rig, &mut c.pos, &c.heading, drive, env, ground);
        let (s, co) = c.heading.sin_cos();
        let vy = (c.rig.parts.first().map_or(0.0, |p| p.pos[1]) - before) / STEP;
        c.vel = [du * co / STEP, du * s / STEP, vy];
        let work = std::mem::take(&mut c.rig.work);
        c.energy -= work * MOTOR_COST;
        if drive.power.abs() > 0.3 && !held {
            let made = du / STEP / c.length().max(1e-5) / drive.power.abs();
            let learnt = if drive.power > 0.0 { &mut c.stride } else { &mut c.back };
            *learnt += (made - *learnt) * 0.02;
            c.toil += (work / STEP / drive.power.abs() - c.toil) * 0.02;
        }
        if drive.rise.abs() > 0.3 && !held && !c.rig.grounded {
            c.climb += (vy / drive.rise - c.climb) * 0.02;
        }
        c.gait = c.rig.clock * std::f32::consts::TAU;
        c.moments.push(Moment::of(&c.rig, c.pos, c.heading));
    }
}

fn corpse(r: &mut Remains, env: &Env, ground: &Ground) {
    r.moments.clear();
    let Some(rig) = &mut r.rig else { return };
    if rig.still > 3.0 {
        return;
    }
    rig.ensure_pairs(&r.body);
    r.moments.push(Moment::of(rig, r.pos, r.heading));
    for _ in 0..STEPS_PER_TICK {
        if rig.still > 3.0 {
            break;
        }
        let limp = Drive { limp: true, ..Default::default() };
        carry(rig, &mut r.pos, &r.heading, limp, env, ground);
        r.moments.push(Moment::of(rig, r.pos, r.heading));
    }
}

/// Fast-forward: each body goes as far as its learned gait would take it,
/// paying what that gait costs, without moving a limb.
fn coarse(w: &mut World) {
    let World { creatures, remains, .. } = w;
    for c in creatures.iter_mut() {
        c.moments.clear();
        let mask = physics::present(&c.body, c.maturity, c.lost);
        let scale = c.scale();
        if mask != c.rig.mask || (scale / c.rig.scale - 1.0).abs() > 0.04 {
            c.rig = c.rig.rebuild(&c.body, mask, scale);
        }
        if c.action == Action::Held || c.doom.is_some() {
            c.vel = [0.0; 3];
            continue;
        }
        let [power, rise] = c.drive;
        let speed = if power > 0.0 { c.stride } else { c.back } * power.abs() * c.length();
        let (s, co) = c.heading.sin_cos();
        let du = speed * TICK;
        c.pos[0] += du * co;
        c.pos[1] += du * s;
        c.pos[0] = c.pos[0].clamp(0.02, WORLD_W - 0.02);
        c.pos[1] = c.pos[1].clamp(0.004, DEPTH - 0.004);
        let vy = if c.rig.grounded { 0.0 } else { c.climb * rise };
        let alt = (c.pos[2] + vy * TICK).clamp(0.0, ALT_MAX);
        c.rig.raise(alt - c.pos[2]);
        c.rig.turn_to(c.heading, TICK);
        c.pos[2] = alt;
        c.vel = [speed * co, speed * s, vy];
        c.energy -= c.toil * power.abs() * TICK * MOTOR_COST;
        c.gait = (c.gait + c.genome.gait_hz * power * TICK * std::f32::consts::TAU) % std::f32::consts::TAU;
    }
    // Bodies lie where they fell.
    for r in remains.iter_mut() {
        r.moments.clear();
    }
}

/// One physics step for a body in its plane, then the plane moved along with
/// it so the body stays at its origin. Returns how far it went.
fn carry(rig: &mut Rig, pos: &mut [f32; 3], heading: &f32, drive: Drive, env: &Env, ground: &Ground) -> f32 {
    let (s, c) = heading.sin_cos();
    let wet = pos[2] < 0.01 && ground.in_river(pos[0], pos[1] / DEPTH);
    let mut flow = env.wind_at(pos[0], pos[2]);
    if wet {
        flow -= 0.004 * (1.0 + env.river.flood * 2.0);
    }
    let medium = Medium { density: if wet { WATER } else { AIR }, flow: [flow * c, 0.0], ceiling: ALT_MAX };
    rig.step(drive, medium);
    let du = rig.parts.first().map_or(0.0, |p| p.pos[0]);
    rig.shift(-du);
    pos[0] += du * c;
    pos[1] += du * s;
    // At the edges of the world it slides along them; turning it round
    // would only have it turn back next tick, flickering.
    pos[0] = pos[0].clamp(0.02, WORLD_W - 0.02);
    pos[1] = pos[1].clamp(0.004, DEPTH - 0.004);
    pos[2] = rig.lowest().clamp(0.0, ALT_MAX);
    if !drive.limp {
        rig.turn_to(*heading, STEP);
    }
    du
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
    // Food is mostly water.
    c.exposure.thirst = (c.exposure.thirst - m * eff / c.abil.body_matter.max(1e-6) * 1.5).max(0.0);
    c.intake += gain / cap;
    let valence = (gain / cap * 4.0 - harm).clamp(-1.0, 1.0);
    c.reward += valence;
    c.last_meal = Some((bin as u8, valence, w.time));
    c.mind.learn_taste(bin, valence, 0.2);
    if valence > 0.03 {
        let (x, d, now) = (c.pos[0], c.pos[1], w.time);
        c.mind.remember(PlaceKind::Food, x, d, (valence * 4.0).clamp(0.2, 1.0), now, false);
    }
    // A food its parent showed it, now found and eaten on its own.
    let taught = c.mind.taught == Some(bin as u8) && valence > 0.0;
    if c.mind.taught == Some(bin as u8) {
        c.mind.taught = None;
    }
    let (pos, sp, parent) = (c.pos, c.species, c.parent);
    w.shed(pos, m - keep);
    if taught {
        w.first(Behavior::Teaching, sp, parent);
    }
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
        let resting = w.creatures[ci].action == Action::Resting;

        // Eating.
        if out[o::EAT] > 0.0
            && !resting
            && let Some(food) = p.food
        {
            let c = &w.creatures[ci];
            let close = matches!(food, Food::Air | Food::Soil) || dist3(c.pos, p.food_pos) < r;
            if close {
                eat(w, ci, food, out[o::EAT]);
                if !matches!(food, Food::Air | Food::Soil) && w.creatures[ci].action == Action::Eating {
                    w.creatures[ci].focus = Some(p.food_pos);
                }
            }
        }

        let alive = |w: &World, j: usize| j < w.creatures.len() && w.creatures[j].doom.is_none();
        let within = |w: &World, j: usize| dist3(w.creatures[ci].pos, w.creatures[j].pos) < r;
        let other = p.other.filter(|&j| p.other_dist < r && alive(w, j));

        // Displays settle who backs down without a fight.
        if w.creatures[ci].action == Action::Displaying
            && let Some(j) = p.other.filter(|&j| alive(w, j))
        {
            display(w, ci, j);
        }

        // Fighting, or among the young, playing at it. A fight its friends are
        // in comes before whatever is nearest.
        if out[o::ATTACK] > 0.3 && !resting {
            let target = p.quarry.filter(|&q| alive(w, q) && within(w, q)).or(other);
            if let Some(j) = target {
                let (c, t) = (&w.creatures[ci], &w.creatures[j]);
                if plays(c, t) {
                    play(w, ci, j);
                } else if c.abil.strike > 0.0 {
                    life::attack(w, ci, j, out[o::ATTACK]);
                    if p.quarry == Some(j) {
                        joined(w, ci, j);
                    }
                    if (w.tick + ci as u64) % 5 == 0 {
                        witness(w, ci, j, false);
                    }
                }
            }
        } else if let Some(j) = other {
            // The curious young play when they meet.
            let c = &w.creatures[ci];
            if c.mind.mood.curious > 0.45 && out[o::SEEK_OTHER] > 0.3 && plays(c, &w.creatures[j]) {
                play(w, ci, j);
            }
        }
        if w.creatures[ci].doom.is_some() {
            continue;
        }

        // Mating. Obligate breeders (sexual above 0.5) must mate; between
        // 0.25 and 0.5 a creature buds alone but mates when it meets a willing
        // partner, as aphids and water fleas do.
        if out[o::MATE] > 0.3
            && let Some(j) = other
            && w.creatures[ci].genome.sexual > FACULTATIVE
            && w.creatures[j].brain.out[o::MATE] > 0.3
        {
            let (a, b) = (&w.creatures[ci], &w.creatures[j]);
            // Each judges the other by looks, by scent for immune genes unlike
            // (or like) its own, and, the choosier it is, by how healthy and
            // well fed the other looks, which only good genes can keep up.
            // Nothing but taste keeps species apart.
            let accept = |x: &Creature, y: &Creature| {
                let looks = Genome::look_distance(&x.genome.mate_pref, &y.genome.look);
                let scent = (genetics::immune_likeness(&x.genome, &y.genome) - 0.5) * x.genome.mhc_pref * 0.3;
                let condition = (y.health * y.energy / y.abil.energy_cap.max(1e-9)).clamp(0.0, 1.0);
                let poor = (1.0 - condition) * x.genome.choosiness * 0.4;
                // Crests, plumes, frills and horns, which cost to grow and draw
                // hunters' eyes, are what a choosy partner falls for.
                let show = (y.abil.ornament * 2.0).min(0.35) * x.genome.choosiness;
                looks + scent + poor - show < 1.0 - x.genome.choosiness * 0.85
            };
            if a.ready() && b.ready() && b.genome.sexual > FACULTATIVE && accept(a, b) && accept(b, a) {
                let (mother, father) = if a.energy >= b.energy { (ci, j) } else { (j, ci) };
                let before = w.creatures[mother].tally.offspring;
                life::breed(w, mother, Some(father));
                // Only shown when it came to something: a crowded world refuses many.
                if w.creatures[mother].tally.offspring > before {
                    w.creatures[ci].action = Action::Mating;
                    w.creatures[ci].target = Some(w.creatures[j].id);
                }
            }
        }

        // Breeding alone, or for a mating kind with no mate about, now and
        // then fertilising its own eggs.
        if out[o::LAY] > 0.3 && w.creatures[ci].ready() {
            let g = &w.creatures[ci].genome;
            let before = w.creatures[ci].tally.offspring;
            if g.sexual <= 0.5 {
                life::breed(w, ci, None);
            } else if w.rng.chance(g.selfing * 0.01) {
                life::breed(w, ci, Some(ci));
            }
            if w.creatures[ci].tally.offspring > before {
                w.creatures[ci].action = Action::Laying;
            }
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
                witness(w, ci, j, true);
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
                // Making up after a fight mends more than an ordinary grooming.
                let made_up = a.mind.find(b.id).is_some_and(|x| x.fought >= 0.0 && now - x.fought < 300.0);
                let mend = if made_up { 3.0 } else { 1.0 };
                b.mind.feel(a.id, 0.12 * mend, kin, now);
                a.mind.feel(b.id, 0.06 * mend, kin, now);
                b.mind.mood.fear *= 0.9;
                if made_up {
                    a.mind.fought(b.id, kin, -1.0);
                    b.mind.fought(a.id, kin, -1.0);
                }
                a.tally.kindness += 1;
                a.action = Action::Grooming;
                a.target = Some(b.id);
                // Close contact passes on whatever either carries.
                life::contact(w, ci, j, 2.0 * dt);
                let (sp, id) = (w.creatures[ci].species, w.creatures[ci].id);
                w.first(Behavior::Grooming, sp, Some(id));
                if made_up {
                    w.first(Behavior::Reconciliation, sp, Some(id));
                }
                witness(w, ci, j, true);
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

        // Watching others: what they eat, where they go, what they fear, how they act.
        if let Some(j) = p.other
            && (w.tick + ci as u64) % 5 == 0
        {
            observe(w, ci, j, p);
        }
    }
}

/// Whether two creatures would play rather than fight: both young, kin or
/// friends, fed and unafraid.
fn plays(c: &Creature, t: &Creature) -> bool {
    c.maturity < 0.6
        && t.maturity < 0.75
        && c.species == t.species
        && (Genome::kinship(&c.genome.marker, &t.genome.marker) > 0.6 || c.mind.bond(t.id) > 0.2)
        && c.energy > c.abil.energy_cap * 0.45
        && c.mind.mood.fear < 0.3
}

/// A mock fight: no harm done, a little tiring, good for the bond, and fun.
fn play(w: &mut World, ci: usize, j: usize) {
    let now = w.time;
    let (a, b) = pair_mut(&mut w.creatures, ci, j);
    a.action = Action::Playing;
    a.target = Some(b.id);
    a.energy -= a.abil.upkeep * 0.5 * TICK;
    a.mind.mood.tired = (a.mind.mood.tired + 0.002).min(1.0);
    a.mind.mood.curious *= 0.995;
    a.reward += 0.04 * (0.5 + a.genome.curiosity);
    b.reward += 0.02;
    let kin = Genome::kinship(&a.genome.marker, &b.genome.marker) > 0.6;
    a.mind.feel(b.id, 0.01, kin, now);
    b.mind.feel(a.id, 0.01, kin, now);
    let (sp, id) = (a.species, a.id);
    w.first(Behavior::Play, sp, Some(id));
}

/// How formidable a creature looks in a contest.
fn presence(c: &Creature) -> f32 {
    c.length()
        * (0.5 + c.health)
        * (1.0 + c.rank.clamp(-1.0, 2.0) * 0.3)
        * (1.0 + c.abil.spines * 0.3 + c.abil.armor * 0.5 + c.abil.ornament * 2.0)
        * (0.7 + c.genome.boldness * 0.6)
        * (1.0 - c.mind.mood.tired * 0.3)
}

/// A threat display at another. If the other displays back, the less
/// formidable backs down; if not, it backs down only from one plainly
/// stronger. A hunter faced down by its prey learns to leave that kind alone.
fn display(w: &mut World, ci: usize, j: usize) {
    let now = w.time;
    let roll = w.rng.range(0.8, 1.25);
    let (a, b) = pair_mut(&mut w.creatures, ci, j);
    // Showing off costs.
    a.energy -= a.abil.upkeep * 0.8 * TICK;
    if b.mind.yielding.is_some_and(|(id, t)| id == a.id && t > now) {
        return;
    }
    let mutual = b.action == Action::Displaying && b.target == Some(a.id);
    let (pa, pb) = (presence(a) * roll, presence(b));
    let (winner, loser) = if mutual {
        if pa >= pb { (a, b) } else { (b, a) }
    } else if pa > pb * 1.3 {
        (a, b)
    } else {
        return;
    };
    loser.mind.yielding = Some((winner.id, now + 15.0));
    loser.mind.mood.fear = (loser.mind.mood.fear + 0.3).min(1.0);
    loser.reward -= 0.3;
    winner.reward += 0.3;
    let same = winner.species == loser.species;
    let kin = Genome::kinship(&winner.genome.marker, &loser.genome.marker) > 0.6;
    if same {
        winner.rank += 0.03;
        loser.rank -= 0.03;
        loser.mind.feel(winner.id, -0.05, kin, now);
    } else {
        let bin = creature_bin(&winner.genome.look, winner.length());
        loser.mind.learn_wary(bin, 0.7, 0.3);
    }
    let sp = winner.species;
    let id = winner.id;
    if same {
        w.first(Behavior::Ritual, sp, Some(id));
    } else {
        w.first(Behavior::Deterrence, sp, Some(id));
    }
}

/// A creature joined a fight one of its friends was in: when two or more of
/// a kind are on one creature together, it is a pack hunt or a mob.
fn joined(w: &mut World, ci: usize, q: usize) {
    let c = &w.creatures[ci];
    let quarry = &w.creatures[q];
    let together = w
        .creatures
        .iter()
        .filter(|o| {
            o.id != c.id && o.species == c.species && o.action == Action::Attacking && o.target == Some(quarry.id)
        })
        .count();
    if together == 0 || quarry.species == c.species {
        return;
    }
    let (sp, id) = (c.species, c.id);
    if quarry.genome.diet > c.genome.diet + 0.2 {
        w.first(Behavior::Mobbing, sp, Some(id));
    } else if c.genome.diet > 0.35 {
        w.first(Behavior::PackHunting, sp, Some(id));
    }
}

/// Others who see a deed think the better or worse of whoever did it: kindness
/// raises it, and an attack lowers it with the victim's friends but raises
/// it with the victim's enemies.
pub fn witness(w: &mut World, actor: usize, target: usize, kind: bool) {
    let (aid, tid, apos, asp) = {
        let a = &w.creatures[actor];
        (a.id, w.creatures[target].id, a.pos, a.species)
    };
    let now = w.time;
    let mut near = Vec::new();
    w.cgrid.near(apos, 0.12, |k| {
        if k != actor && k != target && w.creatures[k].species == asp {
            near.push(k);
        }
    });
    for k in near {
        let b = &mut w.creatures[k];
        if dist3(b.pos, apos) > b.abil.sense {
            continue;
        }
        let toward = b.mind.bond(tid);
        let delta = if kind {
            0.03
        } else if toward > 0.2 || b.mind.find(tid).is_some_and(|x| x.kin) {
            -0.1
        } else if toward < -0.2 {
            0.03
        } else {
            0.0
        };
        if delta != 0.0 {
            b.mind.feel(aid, delta, false, now);
        }
    }
}

fn eat(w: &mut World, ci: usize, food: Food, drive: f32) {
    let dt = TICK;
    let c = &w.creatures[ci];
    // A full gut takes no more.
    if c.energy >= c.abil.energy_cap * 0.95 && life::growth_room(c) <= 0.0 {
        return;
    }
    // Once it has decided to eat, its mouth and gut set the pace.
    let _ = drive;
    let drive = dt;
    let id = c.id;
    match food {
        Food::Air => {
            let col = col_of(c.pos[0]);
            let m = (w.air[col] * 0.01 * dt).min(c.intake(Fare::Plankton, 1.0) * drive);
            if m <= 0.0 {
                return;
            }
            w.air[col] -= m;
            let eff = c.efficiency(Fare::Plankton, 0.0);
            digest(w, ci, m, E_DETRITUS, eff, 48, 0.0);
            w.creatures[ci].action = Action::Eating;
        }
        Food::Soil => {
            let (x, d) = (c.pos[0], c.pos[1]);
            let want = c.intake(Fare::Soil, 1.0) * drive;
            let eff = c.efficiency(Fare::Soil, 0.0);
            let m = w.take_soil(x, d, want);
            if m <= 0.0 {
                return;
            }
            digest(w, ci, m, crate::creature::fare_energy(Fare::Soil), eff, 49, 0.0);
            w.creatures[ci].action = Action::Eating;
        }
        Food::Nectar(j) => {
            if j >= w.plants.len() {
                return;
            }
            let pl = &w.plants[j];
            let chem = pl.genome.look[0];
            let bin = plant_bin(&pl.genome.look);
            let m = (c.intake(Fare::Nectar, 1.0) * drive).min(pl.nectar);
            if m <= 0.0 {
                return;
            }
            let eff = c.efficiency(Fare::Nectar, chem);
            let pl = &mut w.plants[j];
            pl.nectar -= m;
            // A visit: the pollen it carries off and brings in.
            pl.visits = (pl.visits + dt).min(30.0);
            w.creatures[ci].action = Action::Eating;
            digest(w, ci, m, crate::creature::fare_energy(Fare::Nectar), eff, bin, 0.0);
        }
        Food::Plant(j) => {
            if j >= w.plants.len() {
                return;
            }
            let pl = &w.plants[j];
            // Tall plants keep most of their leaves out of a small grazer's
            // reach; long necks and flight get at the rest.
            let reach = if pl.alt > 0.05 {
                1.0
            } else {
                ((c.pos[2] + c.length() * 1.2) / pl.height().max(1e-4)).clamp(0.4, 1.0)
            };
            let tough = 1.0 + 1.5 * pl.genome.tough;
            let fare = if pl.genome.vent > 0.5 { Fare::Crust } else { Fare::Leaf };
            let chem = pl.genome.look[0];
            let fruit_rate = c.intake(Fare::Fruit, 1.0) * drive * reach;
            let leaf_rate = c.intake(fare, tough) * drive * reach;
            let pl = &mut w.plants[j];
            let from_fruit = fruit_rate.min(pl.fruit);
            pl.fruit -= from_fruit;
            // What time is left after the fruit goes on leaves.
            let left = if fruit_rate > 0.0 { 1.0 - from_fruit / fruit_rate } else { 1.0 };
            let from_body = (leaf_rate * left).min((pl.matter - pl.adult_matter * 0.15).max(0.0));
            pl.matter -= from_body;
            let wound = from_body / pl.matter.max(1e-4);
            pl.health = (pl.health - wound * 0.08).max(0.0);
            // Being eaten rouses its defences, faster if it has been eaten before.
            pl.alarm = (pl.alarm + pl.genome.induce * wound * 4.0 * (1.0 + 2.0 * pl.primed)).min(1.0);
            pl.primed = (pl.primed + wound * 2.0).min(1.0);
            let spines = pl.spines();
            let toxin = pl.toxin();
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
            let c = &mut w.creatures[ci];
            // Feelers taste poison before it is swallowed.
            let toxin = c.genome.felt(toxin) * (1.0 - 0.6 * c.abil.taste);
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
            let e = (from_fruit * c.efficiency(Fare::Fruit, chem) * crate::creature::fare_energy(Fare::Fruit)
                + from_body * c.efficiency(fare, chem) * E_PLANT)
                / m;
            digest(w, ci, m, e, 1.0, bin, harm);
            if w.creatures[ci].health <= 0.0 {
                w.creatures[ci].doom = Some(DeathCause::Starvation);
            }
        }
        Food::Remains(j) => {
            if j >= w.remains.len() {
                return;
            }
            let rm = &w.remains[j];
            let rot = rm.decay01();
            // A fresh kill is still meat; it takes a scavenger's gut once it rots.
            let fare = if rot < 0.2 { Fare::Flesh } else { Fare::Carrion };
            let m = (c.intake(fare, 1.0) * drive).min(rm.matter);
            if m <= 0.0 {
                return;
            }
            let eff = c.efficiency(fare, 0.0) * (1.0 - rot) + 0.5 * rot;
            let rm = &mut w.remains[j];
            rm.matter -= m;
            let toxin = rm.toxin;
            let bin = creature_bin(&rm.look, rm.scale * rm.body.extent);
            let strain = rm.strain;
            let offered = rm.offered_to;
            let killed_by_other = rm.killer.is_some_and(|k| k != id);
            let e = E_MEAT * (1.0 - rot * 0.6);
            let c = &mut w.creatures[ci];
            let toxin = c.genome.felt(toxin) * (1.0 - 0.6 * c.abil.taste);
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
            let m = (c.intake(Fare::Egg, 1.0) * drive).min(w.eggs[j].matter);
            let eff = c.efficiency(Fare::Egg, 0.0);
            w.eggs[j].matter -= m;
            if m <= 0.0 {
                return;
            }
            w.creatures[ci].action = Action::Eating;
            digest(w, ci, m, E_MEAT, eff, 48, 0.0);
        }
    }
}

/// Flashes a meaning in this creature's light dialect. Whoever sees it
/// understands it as best their own experience allows, believes it as far as
/// it trusts the one flashing, and learns.
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
    let threat_alt = p.threat.map(|t| w.creatures[t].pos[2]);
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
    let marker = c.genome.marker;
    let ready = c.ready();
    let threat_near = p.threat_level > 0.3;
    let above = threat_alt.is_some_and(|a| a > pos[2] + 0.04);
    let mut listeners = Vec::new();
    w.cgrid.near(pos, range, |j| {
        if j != ci && dist3(pos, w.creatures[j].pos) < range {
            listeners.push(j);
        }
    });
    let mut understood = 0;
    for j in listeners {
        let l = &mut w.creatures[j];
        let d = dist3(pos, l.pos);
        let strength = (1.0 - d / range) * (0.4 + l.mind.trust(id) * 1.2);
        let heard = l.mind.understand(symbol);
        if let Some((m, conf)) = heard {
            l.mind.hear(m, conf * strength.min(1.0), pos[0]);
        }
        // Check the signal against what can be seen.
        let truth = match meaning {
            mind::DANGER => threat_near && !above,
            mind::ALOFT => threat_near && above,
            mind::FOOD => has_food,
            mind::MATE => ready && l.species == species,
            mind::GATHER => {
                l.species == species && (l.mind.bond(id) > 0.0 || Genome::kinship(&l.genome.marker, &marker) > 0.6)
            }
            _ => attacking || l.mind.bond(id) < 0.0,
        };
        if truth {
            l.mind.associate(symbol, meaning, 12);
            if heard.map(|h| h.0) == Some(meaning) {
                understood += 1;
                l.mind.believe(id, 0.02, now);
            }
            // Conformity: the young and the subordinate copy their elders' voice.
            let elder = l.species == species && (age > l.age * 1.5 || rank > l.rank + 0.2 || l.parent == Some(id));
            if elder && w.rng.chance(l.genome.imitation * 0.3) {
                l.mind.voice[meaning] = symbol;
            }
        } else if let Some((m, _)) = heard {
            l.mind.distrust(symbol, m, 6);
            // Caught out: it said what it knew to be untrue.
            if m == meaning {
                l.mind.believe(id, -0.08, now);
            }
        }
    }
    if understood > 0 {
        w.first(Behavior::Signaling, species, Some(id));
        if meaning == mind::ALOFT {
            w.first(Behavior::AlarmCalls, species, Some(id));
        }
        if meaning == mind::GATHER && understood >= 2 {
            w.first(Behavior::Recruitment, species, Some(id));
        }
    }
}

/// Learning by watching another: what it eats and how that went, what it
/// runs from, where it goes to feed and what it avoids, and how it acts. A
/// parent is watched most closely, and teaches without meaning to.
fn observe(w: &mut World, ci: usize, j: usize, p: &Percept) {
    let now = w.time;
    let quarter = season_quarter(w);
    let threat_bin = p.threat.filter(|&t| w.creatures[t].species != w.creatures[ci].species).map(|t| {
        let x = &w.creatures[t];
        creature_bin(&x.genome.look, x.length())
    });
    let (c, o) = pair_mut(&mut w.creatures, ci, j);
    let mut rate = c.genome.imitation;
    if rate <= 0.02 {
        return;
    }
    let parent = c.parent == Some(o.id);
    if parent && c.maturity < 0.8 {
        rate = (rate * 3.0).min(1.0);
    }
    if let Some((bin, valence, t)) = o.last_meal
        && now - t < 3.0
    {
        let before = c.mind.taste(bin as usize);
        c.mind.learn_taste(bin as usize, valence, rate * 0.3);
        if parent && valence > 0.3 && before < 0.1 {
            c.mind.taught = Some(bin);
        }
    }
    // Fear is catching: seeing another run from something teaches to fear it.
    if o.action == Action::Fleeing
        && let Some(bin) = threat_bin
    {
        c.mind.learn_wary(bin, o.mind.mood.fear, rate * 0.15);
    }
    if o.species != c.species {
        return;
    }
    let trusted = parent || c.mind.bond(o.id) > 0.3 || o.age > c.age * 2.0;
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
    // Places: where it feeds, and where it was hurt.
    let mine = c.mind.best_food(c.pos[0], c.pos[1]).map_or(0.0, |(_, v)| v);
    if let Some((pl, v)) = o.mind.best_food(c.pos[0], c.pos[1])
        && v > mine
    {
        let (x, d, value) = (pl.x, pl.depth, pl.value * (0.4 + rate * 0.4));
        c.mind.remember(PlaceKind::Food, x, d, value, now, true);
    }
    if let Some(pl) =
        o.mind.places.iter().filter(|q| q.kind == PlaceKind::Danger).max_by(|a, b| a.value.total_cmp(&b.value))
        && pl.value > 0.4
    {
        let (x, d, value) = (pl.x, pl.depth, pl.value * 0.5);
        c.mind.remember(PlaceKind::Danger, x, d, value, now, true);
    }
    // Copy what it does when it's doing well.
    if o.reward > 0.05 && rate > 0.3 {
        let target = o.brain.out;
        c.brain.imitate(
            &target,
            rate * 0.03,
            &[o::EAT, o::SEEK_FOOD, o::FLEE, o::ATTACK, o::HOME, o::SHELTER, o::RECALL, o::FOLLOW],
        );
    }
}

/// Those who see a death take it in: fear, where it happened and what did
/// it, and those who knew the dead stay by the body a while.
pub fn bereave(w: &mut World, dead: &Creature, cause: DeathCause) {
    let now = w.time;
    let pos = dead.pos;
    let killer = match cause {
        DeathCause::Predation { killer, .. }
        | DeathCause::Cannibalism { killer }
        | DeathCause::Infanticide { killer }
        | DeathCause::Violence { killer, .. } => Some(killer),
        _ => None,
    };
    // What did it, as a kind of thing to fear. Not one's own kind: that
    // would make every neighbour a killer.
    let killer_bin = if let Some(k) = killer {
        w.creatures
            .iter()
            .find(|x| x.id == k && x.species != dead.species)
            .map(|x| creature_bin(&x.genome.look, x.length()))
    } else if let DeathCause::Trap { plant_species } = cause {
        w.plants
            .iter()
            .filter(|pl| pl.species == plant_species && (pl.x - pos[0]).abs() < 0.08)
            .min_by(|a, b| (a.x - pos[0]).abs().total_cmp(&(b.x - pos[0]).abs()))
            .map(|pl| plant_bin(&pl.genome.look))
    } else {
        None
    };
    let violent = killer_bin.is_some()
        || matches!(cause, DeathCause::Lava | DeathCause::Lightning | DeathCause::Flood | DeathCause::Trap { .. });
    let body_left = dead.matter > 0.0015 && cause != DeathCause::Lava;
    let mut learned = Vec::new();
    for l in w.creatures.iter_mut() {
        if l.doom.is_some() || l.id == dead.id {
            continue;
        }
        let d = dist3(l.pos, pos);
        if d > l.abil.sense {
            continue;
        }
        let known = l.mind.find(dead.id).map(|a| (a.bond, a.kin));
        let close = known.is_some_and(|(b, kin)| b > 0.3 || (kin && b > 0.1));
        let same = l.species == dead.species;
        if violent && (same || close) && Some(l.id) != killer {
            let near = 1.0 - d / l.abil.sense;
            l.mind.mood.fear = (l.mind.mood.fear + 0.4 * near).min(1.0);
            l.mind.remember(PlaceKind::Danger, pos[0], pos[1], 0.7, now, false);
            if let Some(b) = killer_bin {
                let before = l.mind.wary(b);
                l.mind.learn_wary(b, 1.0, 0.15 + 0.3 * l.genome.imitation);
                if before < 0.3 && l.mind.wary(b) >= 0.3 {
                    learned.push((l.species, l.id));
                }
            }
        }
        if close && body_left {
            let bond = known.map_or(0.0, |(b, _)| b.max(0.3));
            l.mind.grief = Some(Grief { who: dead.id, pos, until: now + 20.0 + 60.0 * bond as f64 });
            l.reward -= 0.5 * bond;
        }
    }
    for (sp, id) in learned {
        w.first(Behavior::LearnedFear, sp, Some(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Sim;
    use std::sync::Arc;

    fn world() -> Sim {
        let mut sim = Sim::new(17, 1_800_000_000.0);
        for _ in 0..20 {
            sim.step();
        }
        sim
    }

    /// Two creatures of one species and one of another, side by side.
    fn trio(w: &mut World) -> (usize, usize, usize) {
        let a = 0;
        let sp = w.creatures[a].species;
        let b = (1..w.creatures.len()).find(|&i| w.creatures[i].species == sp).unwrap();
        let k = (1..w.creatures.len()).find(|&i| w.creatures[i].species != sp).unwrap();
        for (n, &i) in [a, b, k].iter().enumerate() {
            let c = &mut w.creatures[i];
            c.pos = [1.5 + n as f32 * 0.01, 0.1, 0.0];
            c.doom = None;
            c.health = 1.0;
        }
        w.rebuild_index();
        (a, b, k)
    }

    #[test]
    fn witnesses_learn_to_fear_a_killer_and_mourn_a_friend() {
        let mut sim = world();
        let w = sim.world_mut();
        let (a, b, k) = trio(w);
        let (aid, kid) = (w.creatures[a].id, w.creatures[k].id);
        let now = w.time;
        w.creatures[b].mind.feel(aid, 0.8, true, now);
        let bin = creature_bin(&w.creatures[k].genome.look, w.creatures[k].length());
        let bid = w.creatures[b].id;
        w.creatures[a].doom = Some(DeathCause::Predation { killer: kid, killer_species: w.creatures[k].species });
        life::bury(w);
        let b = w.creatures.iter().find(|c| c.id == bid).unwrap();
        assert!(b.mind.wary(bin) > 0.1, "wary {}", b.mind.wary(bin));
        assert!(b.mind.grief.is_some_and(|g| g.who == aid));
        assert!(b.mind.danger_near(1.5, 0.1, 0.2).is_some());
        assert!(b.mind.mood.fear > 0.1);
    }

    #[test]
    fn a_plainly_stronger_rival_wins_without_blood() {
        let mut sim = world();
        let w = sim.world_mut();
        let (a, b, _) = trio(w);
        w.creatures[a].abil.length *= 3.0;
        w.creatures[a].rank = 1.0;
        let (ha, hb) = (w.creatures[a].health, w.creatures[b].health);
        w.creatures[a].action = Action::Displaying;
        display(w, a, b);
        let bid = w.creatures[a].id;
        assert_eq!(w.creatures[b].mind.yielding.map(|y| y.0), Some(bid));
        assert!(w.creatures[a].rank > 1.0);
        assert_eq!((w.creatures[a].health, w.creatures[b].health), (ha, hb));
    }

    #[test]
    fn grazed_plants_warn_their_neighbours() {
        let mut sim = world();
        let w = sim.world_mut();
        let sp = w.plants[0].species;
        let mut g = (*w.plants[0].genome).clone();
        g.warn = 0.8;
        g.listen = 0.8;
        g.induce = 0.8;
        let g = Arc::new(g);
        let others: Vec<usize> = (1..w.plants.len()).filter(|&i| w.plants[i].species == sp).take(3).collect();
        for (n, &i) in [0].iter().chain(&others).enumerate() {
            let p = &mut w.plants[i];
            p.genome = g.clone();
            p.x = 1.2 + n as f32 * 0.01;
            p.depth = 0.1;
            p.alt = 0.0;
            p.alarm = 0.0;
        }
        crate::ecology::rebuild_plant_grid(w);
        w.plants[0].alarm = 1.0;
        let ids: Vec<_> = others.iter().map(|&i| w.plants[i].id).collect();
        for _ in 0..5 {
            crate::ecology::plants(w, 1.0);
        }
        let roused = w.plants.iter().filter(|p| ids.contains(&p.id) && p.alarm > 0.01).count();
        assert!(roused >= 2, "only {roused} neighbours roused");
    }
}
