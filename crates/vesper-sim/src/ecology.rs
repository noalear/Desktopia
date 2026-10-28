//! Plants: growing on light, vents, water and soil, shading each other out,
//! fruiting and seeding, catching animals, and dying back into the ground.

use crate::history::{Behavior, DeathCause, PlantId};
use crate::plant::{Plant, Seed};
use crate::world::{DEPTH, MAX_MATS, MAX_PLANTS, World, col_of, dist3};
use std::sync::Arc;
use vesper_terrain::{self as terrain, WORLD_W};

/// Grows a seed into a seedling, if it lands somewhere it can live. `matter`
/// is what the seed carries; zero draws a seedling's worth from the soil.
pub fn sow(w: &mut World, seed: &Seed, x: f32, depth: f32, alt: f32, matter: f32) -> bool {
    let g = &seed.genome;
    let floats = g.floats();
    let x = x.clamp(0.03, WORLD_W - 0.03);
    let depth = depth.clamp(0.005, DEPTH - 0.005);
    let ok = w.plants.len() < MAX_PLANTS
        && (floats || !w.ground.in_river(x, depth / DEPTH))
        && !w.env.lava_at(x)
        && if floats {
            crowd_aloft(w, x, depth, alt) < 3 && w.plants.iter().filter(|p| p.genome.floats()).count() < MAX_MATS
        } else {
            crowd(w, x, depth) < 7
        };
    if !ok {
        w.to_soil(x, depth, matter);
        return false;
    }
    let body = Arc::new(g.develop());
    let adult = g.adult_matter(&body).max(1e-5);
    let mut m = matter;
    if m <= 0.0 {
        m = w.take_soil(x, depth, adult * g.seed_size);
        if m <= adult * 0.002 {
            w.to_soil(x, depth, m);
            return false;
        }
    }
    let species = w.species.classify_plant(g, seed.species, w.time, &mut w.history);
    w.species.born(species);
    let id = PlantId(w.id());
    w.plants.push(Plant {
        id,
        species,
        genome: g.clone(),
        body,
        x,
        depth,
        alt: if floats { alt.max(0.05) } else { 0.0 },
        matter: m,
        adult_matter: adult,
        age: 0.0,
        health: 1.0,
        fruit: 0.0,
        seeds: 0.0,
        trap_shut: 0.0,
        vigor: 1.0,
        born: w.time,
        generation: seed.generation,
    });
    w.counters.plant_births += 1;
    true
}

fn crowd(w: &World, x: f32, depth: f32) -> usize {
    let mut n = 0;
    w.pgrid.near([x, depth, 0.0], 0.025, |j| {
        let p = &w.plants[j];
        if p.alt < 0.05 && (p.x - x).abs() < 0.025 && (p.depth - depth).abs() < 0.08 {
            n += 1;
        }
    });
    n
}

/// Floating mats near a spot in the air, which shade and starve each other.
fn crowd_aloft(w: &World, x: f32, depth: f32, alt: f32) -> usize {
    let mut n = 0;
    w.pgrid.near_any_alt([x, depth, alt], 0.1, |j| {
        let p = &w.plants[j];
        if p.alt >= 0.05 && (p.x - x).abs() < 0.04 && (p.depth - depth).abs() < 0.1 && (p.alt - alt).abs() < 0.08 {
            n += 1;
        }
    });
    n
}

pub fn rebuild_plant_grid(w: &mut World) {
    w.pgrid.clear();
    for (i, p) in w.plants.iter().enumerate() {
        w.pgrid.insert(i, [p.x, p.depth, p.alt]);
    }
}

/// Floating mats ride the wind, every tick so they glide rather than hop.
/// The wind runs night-ward up high and day-ward low down, so a mat steers
/// by its height: up when too warm, down when too cold, and between the two
/// it wanders slowly back and forth.
pub fn drift(w: &mut World, dt: f32) {
    let World { plants, env, .. } = w;
    // Where the two winds cancel.
    let still = 0.35 * 4.0 / 9.0;
    for p in plants.iter_mut().filter(|p| p.genome.floats()) {
        let g = &p.genome;
        // Each mat likes its own warmth, so they spread along the band.
        let own = (p.id.0 % 97) as f32 / 97.0 - 0.5;
        let comfort = (g.heat_tolerance - g.cold_tolerance) * 0.5 + (g.look[5] - 0.5) * 0.8 + own * 0.6;
        let warm = env.temperature(p.x, p.alt) - comfort;
        let wander = 0.07 * (p.age * 0.02 + (p.id.0 % 64) as f32).sin();
        let goal = (still + (warm * 0.15).clamp(-0.12, 0.25) + wander).clamp(0.03, 0.45);
        p.alt += (goal - p.alt) * 0.1 * dt;
        p.x = (p.x + env.wind(p.alt) * dt * 0.8).clamp(0.05, WORLD_W - 0.05);
    }
}

pub fn plants(w: &mut World, dt: f32) {
    let env = w.env.clone();
    let mut dead = Vec::new();
    let mut seeds = Vec::new();
    for i in 0..w.plants.len() {
        let col = col_of(w.plants[i].x);
        let canopy = w.canopy.get(col).copied().unwrap_or(0.0);
        let p = &mut w.plants[i];
        let g = p.genome.clone();
        p.age += dt;
        p.trap_shut = (p.trap_shut - dt).max(0.0);
        let floats = g.floats();
        let height = p.height();
        let leaf = p.leaf_area();
        let mut light = env.light(p.x, p.alt + height * 0.5);
        if !floats {
            let others = (canopy - leaf).max(0.0) * (1.0 - (height * 3.0).min(0.7));
            light /= 1.0 + others / (0.0005 + leaf * 2.0);
        }
        let water = if floats {
            0.4 + env.rain(p.x)
        } else {
            w.ground.wetness(p.x, p.depth / DEPTH) + env.rain(p.x) * 0.8 + 0.15
        };
        let water_factor = (1.0 + g.thirst * 0.8) * (1.0 - g.thirst * (1.0 - water.min(1.0)));
        let temp = env.temperature(p.x, p.alt);
        let hot = 0.4 + g.heat_tolerance;
        let cold = -(0.4 + g.cold_tolerance);
        let stress = if temp > hot {
            temp - hot
        } else if temp < cold {
            cold - temp
        } else {
            0.0
        };
        p.health = (p.health - stress * 0.01 * dt + 0.002 * dt).min(1.0);
        let s = p.scale();
        let area: f32 = p.body.segments.iter().map(|q| q.area()).sum::<f32>() * s * s;
        let photo = leaf * light * 60.0 * (1.0 - g.vent);
        let chemo = terrain::vent_heat(p.x) * g.vent * area * 40.0 * (p.alt < 0.05) as u8 as f32;
        let potential = (photo + chemo) * g.growth * water_factor * dt;
        let upkeep = p.matter
            * 0.0012
            * (1.0 + g.spines * 0.3 + g.toxin * 0.5 + g.trap * 0.3 + g.glow * 0.3 + g.root * 0.2)
            * dt;
        p.vigor += ((light + chemo * 50.0).min(1.0) - p.vigor) * 0.05 * dt;
        let (x, depth, alt) = (p.x, p.depth, p.alt);
        let reach = 0.3 + g.root;
        let got = if floats {
            let c = col_of(x);
            let m = (w.air[c] * 0.006 * dt).min(potential);
            w.air[c] -= m;
            m
        } else {
            w.take_soil(x, depth, potential * reach.min(1.0))
        };
        let p = &mut w.plants[i];
        // Take in what it got, pay its upkeep back to the ground as litter, and
        // put anything beyond a full-grown body into fruit and seeds.
        p.matter += got;
        let pay = upkeep.min(p.matter);
        p.matter -= pay;
        let mut litter = pay;
        let surplus = (p.matter - p.adult_matter).max(0.0);
        if surplus > 0.0 {
            p.matter -= surplus;
            let cap = p.adult_matter * 0.25 * g.fruit;
            let to_fruit = (surplus * g.fruit).min((cap - p.fruit).max(0.0));
            p.fruit += to_fruit;
            p.seeds += surplus - to_fruit;
        }
        let rot = p.fruit * 0.002 * dt;
        p.fruit -= rot;
        litter += rot;
        // Seeds.
        let seed_m = p.adult_matter * g.seed_size;
        if p.seeds >= seed_m && p.growth01() > 0.5 {
            p.seeds -= seed_m;
            seeds.push((i, seed_m));
        }
        let lava = !floats && env.lava_at(x);
        if p.age > g.lifespan || p.health <= 0.0 || p.matter < p.adult_matter * 0.004 || lava {
            dead.push(i);
        }
        if litter > 0.0 {
            if alt > 0.05 {
                w.air[col_of(x)] += litter;
            } else {
                w.to_soil(x, depth, litter);
            }
        }
    }
    // Scatter seeds.
    for (i, m) in seeds {
        let p = &w.plants[i];
        let g = p.genome.clone();
        let (x, depth, alt, species, generation) = (p.x, p.depth, p.alt, p.species, p.generation);
        let mut child = (*g).clone();
        child.mutate(&mut w.rng);
        let spread = 0.015 + g.wind * 0.25;
        let nx = x + w.rng.normal() * spread + env.wind(0.1) * 20.0 * g.wind;
        let nd = depth + w.rng.normal() * 0.05;
        let seed = Seed { genome: Arc::new(child), species, generation: generation + 1 };
        sow(w, &seed, nx, nd, alt, m);
    }
    // Clear the dead, last first so indices stay valid.
    dead.sort_unstable();
    dead.dedup();
    for &i in dead.iter().rev() {
        let p = w.plants.swap_remove(i);
        let m = p.matter + p.fruit + p.seeds;
        if p.alt > 0.05 {
            w.air[col_of(p.x)] += m;
        } else {
            w.to_soil(p.x, p.depth, m);
        }
        w.species.died(p.species, w.time, &mut w.history);
        w.counters.plant_deaths += 1;
    }
    rebuild_plant_grid(w);
    traps(w);
}

/// Trap plants close on small animals that wander into them.
fn traps(w: &mut World) {
    for i in 0..w.plants.len() {
        let p = &w.plants[i];
        if p.genome.trap < 0.3 || p.trap_shut > 0.0 || p.growth01() < 0.4 {
            continue;
        }
        let h = p.height();
        let tip = [p.x, p.depth, p.alt + h * 0.8];
        let r = 0.006 + h * 0.25;
        let max_len = p.genome.trap * h * 0.7;
        let mut caught = None;
        w.cgrid.near(tip, r, |j| {
            let c = &w.creatures[j];
            if caught.is_none() && c.doom.is_none() && c.length() < max_len && dist3(c.pos, tip) < r {
                caught = Some(j);
            }
        });
        let Some(j) = caught else { continue };
        let species = w.plants[i].species;
        let c = &mut w.creatures[j];
        c.doom = Some(DeathCause::Trap { plant_species: species });
        c.action = crate::creature::Action::Held;
        let m = c.matter;
        c.matter = 0.0;
        let (x, depth) = (c.pos[0], c.pos[1]);
        let p = &mut w.plants[i];
        let room = (p.adult_matter * 1.2 - p.matter).max(0.0);
        let keep = m.min(room);
        p.matter += keep;
        p.trap_shut = 40.0;
        w.to_soil(x, depth, m - keep);
        w.first(Behavior::Trapping, species, None);
    }
}
