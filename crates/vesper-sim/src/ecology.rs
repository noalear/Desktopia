//! Plants: growing on light, vents, water and soil, shading each other out,
//! fruiting and seeding, catching animals, and dying back into the ground.

use crate::history::{Behavior, DeathCause, PlantId, SpeciesId};
use crate::plant::{POLLEN_ANIMAL, POLLEN_WIND, Plant, PlantGenome, Seed};
use crate::species::PLANT_SPLIT;
use crate::world::{DEPTH, Dormant, MAX_MATS, MAX_PLANTS, World, col_of, dist3};

/// Seeds lying dormant at most.
const MAX_DORMANT: usize = 800;
use std::sync::Arc;
use vesper_terrain::{self as terrain, WORLD_W};

/// Grows a seed into a seedling, if it lands somewhere it can live. `matter`
/// is what the seed carries; zero draws a seedling's worth from the soil.
pub fn sow(w: &mut World, seed: &Seed, x: f32, depth: f32, alt: f32, matter: f32) -> bool {
    plant_seed(w, seed, x, depth, alt, matter, false)
}

/// Seeds lying in dry ground sprout when rain has soaked it, all at once,
/// greening the land after a drought; those that wait too long rot.
pub fn seedbank(w: &mut World) {
    let mut i = 0;
    while i < w.seedbank.len() {
        let d = &w.seedbank[i];
        let water = w.ground.wetness(d.x, d.depth / DEPTH) + w.env.moisture(d.x) * 0.9 + 0.05;
        let need = d.seed.genome.thirst * 0.8 + 0.1;
        if water >= need * 0.8 {
            let d = w.seedbank.swap_remove(i);
            plant_seed(w, &d.seed, d.x, d.depth, 0.0, d.matter, true);
        } else if w.time - d.since > 2.0 * 86400.0 {
            let d = w.seedbank.swap_remove(i);
            w.to_soil(d.x, d.depth, d.matter);
        } else {
            i += 1;
        }
    }
}

fn plant_seed(w: &mut World, seed: &Seed, x: f32, depth: f32, alt: f32, matter: f32, waking: bool) -> bool {
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
            crowd(w, x, depth) < 7 || gap(w, seed.species, x, depth)
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
    // On ground too dry for it, a seed lies dormant until rain comes.
    if !floats && alt < 0.05 && !waking {
        let water = w.ground.wetness(x, depth / DEPTH) + w.env.moisture(x) * 0.9 + 0.05;
        if water < (g.thirst * 0.8 + 0.1) * 0.6 && w.seedbank.len() < MAX_DORMANT {
            w.seedbank.push(Dormant { seed: seed.clone(), x, depth, matter: m, since: w.time });
            return true;
        }
    }
    let species = if waking { w.species.sprouting(seed.species, g, w.time, &mut w.history) } else { seed.species };
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
        alarm: 0.0,
        primed: 0.0,
        dormant: 0.0,
        hardship: 0.0,
        acclim: 0.0,
        wilt: 0.0,
        frost: 0.0,
        burn: 0.0,
        coat: 0.0,
        nectar: 0.0,
        visits: 0.0,
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

/// Gap dynamics: in a full stand a seed can still take root where a
/// neighbour is failing. The frailest dying, wasted or senescent plant
/// nearby gives way (it dies on the next pass), so a stand turns over
/// and new variants keep getting their chance instead of the first
/// comers holding the ground until they die of age. A kind that already
/// fills the stand gets no such gaps: its own pests and the grazers that
/// learn it keep the commonest plant from crowding every other kind out.
fn gap(w: &mut World, species: SpeciesId, x: f32, depth: f32) -> bool {
    let mut worst: Option<(usize, f32)> = None;
    let (mut near, mut kin) = (0u32, 0u32);
    w.pgrid.near([x, depth, 0.0], 0.025, |j| {
        let p = &w.plants[j];
        if p.alt >= 0.05 || p.health <= 0.0 || (p.x - x).abs() >= 0.025 || (p.depth - depth).abs() >= 0.08 {
            return;
        }
        near += 1;
        kin += (p.species == species) as u32;
        // How close to giving up: sick, wasted away, or near the end of its
        // days. (Vigor is not used: it follows the daylight.)
        let life = p.age / p.genome.lifespan;
        let due = p.adult_matter * 0.7 * life.min(1.0);
        let wasted = if life > 0.1 { 1.0 - p.matter / due.max(1e-9) } else { 0.0 };
        let frailty = (1.0 - p.health).max(wasted).max((life - 0.7) * 3.0);
        if frailty > 0.6 && worst.is_none_or(|(_, f)| frailty > f) {
            worst = Some((j, frailty));
        }
    });
    if near > 0 && kin * 2 > near {
        return false;
    }
    match worst {
        Some((j, _)) => {
            w.plants[j].health = 0.0;
            true
        }
        None => false,
    }
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
        p.x = (p.x + env.wind_at(p.x, p.alt) * dt * 0.8).clamp(0.05, WORLD_W - 0.05);
    }
}

pub fn plants(w: &mut World, dt: f32) {
    seedbank(w);
    let env = w.env.clone();
    let mut dead = Vec::new();
    let mut seeds = Vec::new();
    let mut warnings = Vec::new();
    let mut events = Vec::new();
    for i in 0..w.plants.len() {
        let col = col_of(w.plants[i].x);
        let canopy = w.canopy.get(col).copied().unwrap_or(0.0);
        let (px, pd) = (w.plants[i].x, w.plants[i].depth / DEPTH);
        let wet = w.ground.wetness(px, pd);
        let flooded = env.river.flood > 0.1 && w.ground.in_river(px, pd);
        let p = &mut w.plants[i];
        let g = p.genome.clone();
        // A dormant plant's clock runs slow.
        p.age += dt * (1.0 - 0.5 * p.dormant);
        p.trap_shut = (p.trap_shut - dt).max(0.0);
        let floats = g.floats();
        let height = p.height();
        let leaf = p.leaf_area();
        let mut light = env.light(p.x, p.alt + height * 0.5);
        if !floats {
            // Tendrils climb over the neighbours' leaves toward the light.
            let others = (canopy - leaf).max(0.0) * (1.0 - (height * 3.0).min(0.7)) * (1.0 - 0.5 * g.tendrils);
            light /= 1.0 + others / (0.0005 + leaf * 2.0);
            // Low plants vanish under lying snow.
            light *= 1.0 - 0.9 * env.snow_cover(p.x) * terrain::smoothstep(0.03, 0.005, height);
        }
        // Ash and dust settle on the leaves until rain washes them clean.
        p.coat += (env.ashfall_at(p.x) * 0.004 + env.dust_at(p.x) * 0.0004) * dt;
        p.coat -= p.coat * (env.rain(p.x) * 0.01 + 0.0002 * env.gust(p.x)) * dt;
        p.coat = p.coat.clamp(0.0, 1.0);
        light *= 1.0 - 0.7 * p.coat;
        // Water: the river's banks, the ground's own (rain soaked in over the
        // past hours, gone in a drought), or for mats the damp of the air.
        let water = if floats { 0.3 + 0.7 * env.humidity(p.x).min(1.0) } else { wet + env.moisture(p.x) * 0.9 + 0.05 };
        let need = g.thirst * 0.8 + 0.1;
        let short = ((need - water) / need).clamp(0.0, 1.0);
        // A plant wilts over hours of drought and recovers faster once watered.
        // Water stored in swollen stems holds it off for longer.
        p.wilt += (short - p.wilt) * dt / if short > p.wilt { 5400.0 * (1.0 + 2.0 * g.knots) } else { 1200.0 };
        let water_factor = (1.0 + g.thirst * 0.8) * (1.0 - g.thirst * (1.0 - water.min(1.0))) * (1.0 - 0.8 * p.wilt);
        let temp = env.temperature(p.x, p.alt);
        // Hardening: tolerance shifts slowly towards the warmth it keeps meeting.
        let reach = 0.4 * g.hardening;
        let middle = (g.heat_tolerance - g.cold_tolerance) * 0.5;
        let pull = ((temp - middle) * 0.5).clamp(-reach, reach);
        p.acclim += (pull - p.acclim) * (g.hardening * dt / 600.0).min(1.0);
        let hot = 0.4 + g.heat_tolerance + p.acclim.max(0.0);
        let cold = -(0.4 + g.cold_tolerance + (-p.acclim).max(0.0));
        let stress = if temp > hot {
            temp - hot
        } else if temp < cold {
            cold - temp
        } else {
            0.0
        };
        // Frost and scorching build up over hours rather than killing at
        // once. Snow lying over a low plant keeps the worst of the frost off.
        let blanket = if floats { 0.0 } else { env.snow_cover(p.x) * terrain::smoothstep(0.04, 0.01, height) };
        if temp < cold {
            // Needles stand frost better than broad leaves.
            p.frost += (cold - temp) * 0.004 * (1.0 - 0.8 * blanket) * (0.5 + g.leaf_width) * dt;
        } else {
            p.frost -= dt / 3600.0;
        }
        if temp > hot {
            // Finely divided leaves shed heat.
            p.burn += (temp - hot) * 0.004 * (1.0 + p.wilt) * (1.0 - 0.4 * g.leaf_split) * dt;
        } else {
            p.burn -= dt / 3600.0;
        }
        p.frost = p.frost.clamp(0.0, 1.0);
        p.burn = p.burn.clamp(0.0, 1.0);
        light *= 1.0 - 0.6 * p.frost.max(p.burn);
        // Dormancy: when heat, cold, drought or darkness have gone on long
        // enough it shuts down, and it wakes again once they ease.
        let dry = if floats { 0.0 } else { g.thirst * (1.0 - water.min(1.0)) };
        let dark = if g.vent < 0.5 { ((0.15 - light) * 4.0).max(0.0) } else { 0.0 };
        p.hardship += (stress * 2.0 + dry + dark - p.hardship) * (dt / 60.0).min(1.0);
        if g.dormancy > 0.05 {
            let threshold = 0.2 + (1.0 - g.dormancy) * 0.8;
            let goal = if p.hardship > threshold {
                1.0
            } else if p.hardship < threshold * 0.5 {
                0.0
            } else {
                p.dormant.round()
            };
            let was = p.dormant;
            p.dormant += (goal - p.dormant) * (dt / 30.0).min(1.0);
            if was < 0.8 && p.dormant >= 0.8 {
                events.push((Behavior::Dormancy, p.species));
            }
        } else {
            p.dormant = 0.0;
        }
        let sleep = p.dormant;
        // Hail shreds leaves; floods drown roots that can't stand them. A
        // dormant plant rides most of it out.
        let drowned = !floats && g.thirst < 0.6 && flooded;
        let harm = (p.wilt - 0.6).max(0.0) * 0.004
            + (p.frost - 0.5).max(0.0) * 0.006 * (1.0 - 0.7 * sleep)
            + (p.burn - 0.5).max(0.0) * 0.006 * (1.0 - 0.7 * sleep)
            + env.hail_at(p.x) * 0.003 * (0.4 + 0.8 * g.leaf_width)
            + drowned as u8 as f32 * env.river.flood * 0.002;
        p.health = (p.health - harm * dt + 0.002 * dt).min(1.0);
        let s = p.scale();
        let area: f32 = p.body.segments.iter().map(|q| q.area()).sum::<f32>() * s * s;
        // Tough leaves are thick and catch less light for their size.
        let photo = leaf * light * 60.0 * (1.0 - g.vent) * (1.0 - 0.3 * g.tough);
        let chemo = terrain::vent_heat(p.x) * g.vent * area * 40.0 * (p.alt < 0.05) as u8 as f32;
        let potential = (photo + chemo) * g.growth * water_factor * dt * (1.0 - 0.9 * sleep);
        let upkeep = p.matter
            * 0.0012
            * (1.0
                + g.spines * 0.3
                + g.toxin * 0.5
                + g.trap * 0.3
                + g.glow * 0.3
                + g.root * 0.2
                + p.alarm * g.induce * 0.6
                + g.warn * 0.1
                + g.listen * 0.05
                + p.acclim.abs() * 0.5
                + g.hardening * 0.1
                + g.knots * 0.2
                + g.tendrils * 0.15
                + g.tough * 0.2)
            * dt
            * (1.0 - 0.7 * sleep);
        p.vigor += (((light + chemo * 50.0).min(1.0) * (1.0 - sleep)) - p.vigor) * 0.05 * dt;
        // Defences raised against grazing fade, and so does the memory of it.
        p.alarm *= (-dt / 240.0).exp();
        p.primed *= (-dt / 3600.0).exp();
        if p.alarm > 0.25 && g.warn > 0.05 {
            warnings.push((i, p.alarm * g.warn));
        }
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
        // Gales tear at whatever stands tall, and hail strips leaves.
        let gust = env.gust(x);
        if !floats && (gust > 2.2 || env.hail_at(x) > 0.0) {
            let torn = p.matter * ((gust - 2.2).max(0.0) * height * 0.3 + env.hail_at(x) * 0.002) * dt;
            let torn = torn.min(p.matter * 0.05);
            p.matter -= torn;
            litter += torn;
        }
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
        // Flowers fill with nectar while awake, paid for out of the plant's
        // own body; unvisited nectar sours away.
        if g.nectar > 0.3 && p.growth01() > 0.5 {
            let cap = p.adult_matter * 0.03 * g.nectar;
            let make = (p.adult_matter * 0.0004 * g.nectar * dt * (1.0 - sleep)).min((cap - p.nectar).max(0.0));
            let make = make.min(p.matter * 0.01);
            p.matter -= make;
            p.nectar += make;
        }
        let sour = p.nectar * 0.001 * dt;
        p.nectar -= sour;
        litter += sour;
        p.visits *= (-dt / 900.0).exp();
        // Seeds. Some plants hold theirs back and let it all go when rain
        // comes, together with their neighbours, so seed-eaters can't keep up
        // and the seedlings start in wet ground.
        let seed_m = p.adult_matter * g.seed_size;
        if p.growth01() > 0.5 && p.seeds >= seed_m {
            if g.mast > 0.3 && !floats {
                let hoard = p.adult_matter * (0.1 + g.mast * 0.6);
                let wet = env.rain(x) > 0.35;
                if wet || p.seeds > hoard || p.age > g.lifespan * 0.95 {
                    let k = ((p.seeds / seed_m).floor() as usize).min(12);
                    p.seeds -= seed_m * k as f32;
                    for _ in 0..k {
                        seeds.push((i, seed_m));
                    }
                    if wet && k >= 3 {
                        events.push((Behavior::Masting, p.species));
                    }
                }
            } else {
                p.seeds -= seed_m;
                seeds.push((i, seed_m));
            }
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
    warn(w, &warnings, dt, &mut events);
    for (b, sp) in events {
        w.first(b, sp, None);
    }
    // Scatter seeds, each fertilised by pollen from a neighbour or, failing
    // that, from the plant itself.
    for (i, m) in seeds {
        let p = &w.plants[i];
        let g = p.genome.clone();
        let (x, depth, alt, species, generation) = (p.x, p.depth, p.alt, p.species, p.generation);
        let pollen = pollinate(w, i);
        let child = g.cross(&pollen, &mut w.rng);
        // Hybrids may set no good seed, and some seeds never sprout.
        if !w.rng.chance(g.fertility() * child.viability()) {
            if alt > 0.05 {
                w.air[col_of(x)] += m;
            } else {
                w.to_soil(x, depth, m);
            }
            continue;
        }
        let spread = 0.015 + g.wind * 0.25;
        let nx = x + w.rng.normal() * spread + env.wind_at(x, 0.1) * 20.0 * g.wind;
        let nd = depth + w.rng.normal() * 0.05;
        let seed = Seed { genome: Arc::new(child), species, generation: generation + 1 };
        sow(w, &seed, nx, nd, alt, m);
    }

    // Clear the dead, last first so indices stay valid.
    dead.sort_unstable();
    dead.dedup();
    for &i in dead.iter().rev() {
        let p = w.plants.swap_remove(i);
        let m = p.matter + p.fruit + p.seeds + p.nectar;
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

/// The pollen that fertilises a plant's next seed. Wind carries it a little
/// way, animals drawn to nectar much further; a flower takes only pollen
/// close enough to its own kind to recognise. A plant that gets none, or
/// that mostly pollinates itself, uses its own.
fn pollinate(w: &mut World, i: usize) -> Arc<PlantGenome> {
    let p = &w.plants[i];
    let g = p.genome.clone();
    if w.rng.chance(g.selfing) {
        return g;
    }
    let at = [p.x, p.depth, p.alt];
    // Animals that drank at its flowers carry its pollen far; wind only a little way.
    let visitors = (p.visits / 10.0).min(1.0);
    let reach = 0.03 + POLLEN_WIND * g.wind + POLLEN_ANIMAL * visitors;
    // A few candidates, picked fairly from everything in reach.
    let mut picked = [usize::MAX; 6];
    let mut seen = 0usize;
    let (plants, rng) = (&w.plants, &mut w.rng);
    w.pgrid.near_any_alt(at, reach, |j| {
        let q = &plants[j];
        if j == i || q.growth01() < 0.5 || (q.x - at[0]).abs() > reach || (q.depth - at[1]).abs() > reach {
            return;
        }
        seen += 1;
        let k = if seen <= picked.len() { seen - 1 } else { rng.below(seen) };
        if k < picked.len() {
            picked[k] = j;
        }
    });
    picked
        .iter()
        .filter(|&&j| j != usize::MAX)
        .map(|&j| &w.plants[j].genome)
        .find(|q| g.distance(q) < PLANT_SPLIT * 1.5)
        .cloned()
        .unwrap_or(g)
}

/// Plants being eaten give off a scent. Neighbours that pick it up arm
/// themselves before the grazers reach them, their own kind most readily,
/// and hunters that smell it know prey is about.
fn warn(w: &mut World, warnings: &[(usize, f32)], dt: f32, events: &mut Vec<(Behavior, SpeciesId)>) {
    let mut heard = Vec::new();
    for &(i, strength) in warnings {
        let (x, depth, alt, species) = {
            let p = &w.plants[i];
            (p.x, p.depth, p.alt, p.species)
        };
        let r = 0.03 + 0.08 * strength.min(1.0);
        heard.clear();
        w.pgrid.near([x, depth, alt], r, |j| {
            if j != i {
                heard.push(j);
            }
        });
        let mut roused = 0;
        for &j in &heard {
            let q = &mut w.plants[j];
            let d = dist3([x, depth, alt], [q.x, q.depth, q.alt]);
            if d > r {
                continue;
            }
            let own = if q.species == species { 1.0 } else { 0.5 };
            let k = q.genome.listen * strength * (1.0 - d / r) * own * dt * 0.2;
            if k > 0.002 {
                q.alarm = (q.alarm + k * q.genome.induce.max(0.2) * (1.0 + 2.0 * q.primed)).min(1.0);
                q.primed = (q.primed + k * 0.5).min(1.0);
                roused += 1;
            }
        }
        if roused >= 2 {
            events.push((Behavior::PlantWarning, species));
        }
        let mut hunters = Vec::new();
        w.cgrid.near([x, depth, 0.0], 0.15, |k| {
            if w.creatures.get(k).is_some_and(|c| c.genome.diet > 0.5) {
                hunters.push(k);
            }
        });
        for k in hunters {
            w.creatures[k].mind.hear(crate::mind::FOOD, 0.3 * strength.min(1.0), x);
        }
    }
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
