//! Species, and the record of every one that ever lived.
//!
//! Nothing stops two species from mating; what keeps them apart is what
//! keeps real species apart: they rarely meet, rarely fancy each other, and
//! when they do, their hybrids fail. So a species here is not a label handed
//! out at birth. It is a cluster of genomes that stays a cluster, and the
//! world checks every minute whether any species has come apart into two.

use crate::body::{Body, Phenotype};
use crate::creature::abilities;
use crate::genetics;
use crate::genome::{Genome, Tissue};
use crate::history::{Fact, Kingdom, Locomotion, Reproduction, SpeciesId, Traits};
use crate::plant::PlantGenome;
use crate::rng::Rng;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Genetic gap between two clusters of one species at which they count as two.
pub const SPLIT: f32 = 7.0;
pub const PLANT_SPLIT: f32 = 5.0;
/// Members each side of a split needs.
const MIN_SPLIT: usize = 6;
/// Weight of the interacting genes' divergence in the gap between two creatures.
const COMPAT_GAP: f32 = 6.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Rep {
    Creature(Arc<Genome>),
    Plant(Arc<PlantGenome>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: SpeciesId,
    pub parent: Option<SpeciesId>,
    pub born: f64,
    pub extinct: Option<f64>,
    /// The genome it was founded with; descendants are measured against it.
    pub rep: Rep,
    /// Living members, and eggs or seeds waiting.
    pub alive: u32,
    pub ever: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    pub list: Vec<Record>,
}

impl Registry {
    pub fn get(&self, id: SpeciesId) -> &Record {
        &self.list[(id.0 - 1) as usize]
    }

    pub fn get_mut(&mut self, id: SpeciesId) -> &mut Record {
        &mut self.list[(id.0 - 1) as usize]
    }

    fn add(&mut self, parent: Option<SpeciesId>, rep: Rep, now: f64, facts: &mut Vec<(f64, Fact)>) -> SpeciesId {
        let id = SpeciesId(self.list.len() as u64 + 1);
        let traits = match &rep {
            Rep::Creature(g) => creature_traits(g),
            Rep::Plant(g) => plant_traits(g),
        };
        self.list.push(Record { id, parent, born: now, extinct: None, rep, alive: 0, ever: 0 });
        facts.push((now, Fact::SpeciesArose { id, parent, traits }));
        id
    }

    pub fn found_creature(&mut self, g: &Arc<Genome>, now: f64, facts: &mut Vec<(f64, Fact)>) -> SpeciesId {
        self.add(None, Rep::Creature(Arc::new(g.clone_shallow())), now, facts)
    }

    pub fn found_plant(&mut self, g: &Arc<PlantGenome>, now: f64, facts: &mut Vec<(f64, Fact)>) -> SpeciesId {
        self.add(None, Rep::Plant(Arc::new(g.clone_shallow())), now, facts)
    }

    /// Members that now count as a new species.
    fn moved(&mut self, from: SpeciesId, to: SpeciesId, n: u32) {
        let f = self.get_mut(from);
        f.alive = f.alive.saturating_sub(n);
        let t = self.get_mut(to);
        t.alive += n;
        t.ever += n;
    }

    pub fn born(&mut self, id: SpeciesId) {
        let r = self.get_mut(id);
        r.alive += 1;
        r.ever += 1;
    }

    /// The species a long-buried plant seed sprouts as: its own, or, if that
    /// died out meanwhile, a new one descended from it.
    pub(crate) fn sprouting(
        &mut self,
        id: SpeciesId,
        g: &PlantGenome,
        now: f64,
        facts: &mut Vec<(f64, Fact)>,
    ) -> SpeciesId {
        if self.get(id).extinct.is_none() {
            return id;
        }
        self.add(Some(id), Rep::Plant(Arc::new(g.clone_shallow())), now, facts)
    }

    /// A member died (or an egg failed). Returns true when that was the last.
    pub fn died(&mut self, id: SpeciesId, now: f64, facts: &mut Vec<(f64, Fact)>) -> bool {
        let r = self.get_mut(id);
        r.alive = r.alive.saturating_sub(1);
        if r.alive == 0 && r.extinct.is_none() {
            r.extinct = Some(now);
            facts.push((now, Fact::SpeciesExtinct { id }));
            return true;
        }
        false
    }
}

/// Looks for species that have come apart into two clusters, and names the
/// smaller one as a new species. Lineages separated by distance or by habit
/// drift apart; once they no longer interbreed, nothing pulls them back.
pub(crate) fn speciate(w: &mut World) {
    let now = w.time;
    let mut by: BTreeMap<SpeciesId, Vec<usize>> = BTreeMap::new();
    for (i, c) in w.creatures.iter().enumerate() {
        by.entry(c.species).or_default().push(i);
    }
    for (s, members) in by {
        let creatures = &w.creatures;
        let gap = |a: usize, b: usize| creature_gap(&creatures[a].genome, &creatures[b].genome);
        let Some((leave, medoids)) = split(&members, gap, SPLIT, &mut w.rng) else { continue };
        let rep = w.creatures[medoids[1]].genome.clone_shallow();
        let new = w.species.add(Some(s), Rep::Creature(Arc::new(rep)), now, &mut w.history);
        for &i in &leave {
            w.creatures[i].species = new;
        }
        let mut moved = leave.len() as u32;
        // Eggs go with whichever side they take after.
        let (a, b) = (w.creatures[medoids[0]].genome.clone(), w.creatures[medoids[1]].genome.clone());
        for e in w.eggs.iter_mut().filter(|e| e.species == s) {
            if creature_gap(&e.genome, &b) < creature_gap(&e.genome, &a) {
                e.species = new;
                moved += 1;
            }
        }
        w.species.moved(s, new, moved);
    }
    let mut by: BTreeMap<SpeciesId, Vec<usize>> = BTreeMap::new();
    for (i, p) in w.plants.iter().enumerate() {
        by.entry(p.species).or_default().push(i);
    }
    for (s, members) in by {
        let plants = &w.plants;
        let gap = |a: usize, b: usize| {
            let (x, y) = (&plants[a].genome, &plants[b].genome);
            x.distance(y) + x.compat_divergence(y) * COMPAT_GAP * 0.7
        };
        let Some((leave, medoids)) = split(&members, gap, PLANT_SPLIT, &mut w.rng) else { continue };
        let rep = Arc::new(w.plants[medoids[1]].genome.clone_shallow());
        let new = w.species.add(Some(s), Rep::Plant(rep), now, &mut w.history);
        for &i in &leave {
            w.plants[i].species = new;
        }
        w.species.moved(s, new, leave.len() as u32);
    }
}

/// Splits a species' members in two if they fall into two clusters further
/// apart than `threshold` and well apart compared with their own spread.
/// Returns the members of the smaller cluster, and the medoid of the one
/// that stays and of the one that leaves.
fn split(
    members: &[usize],
    gap: impl Fn(usize, usize) -> f32,
    threshold: f32,
    rng: &mut Rng,
) -> Option<(Vec<usize>, [usize; 2])> {
    if members.len() < MIN_SPLIT * 2 {
        return None;
    }
    let farthest = |from: usize| {
        members.iter().copied().map(|m| (gap(from, m), m)).max_by(|x, y| x.0.total_cmp(&y.0)).map(|x| x.1).unwrap()
    };
    let start = members[rng.below(members.len())];
    let a = farthest(start);
    let b = farthest(a);
    if gap(a, b) < threshold {
        return None;
    }
    let mut medoids = [a, b];
    let mut sides: [Vec<usize>; 2] = Default::default();
    for _ in 0..3 {
        sides = Default::default();
        for &m in members {
            let near_b = gap(m, medoids[1]) < gap(m, medoids[0]);
            sides[near_b as usize].push(m);
        }
        if sides.iter().any(|s| s.len() < MIN_SPLIT) {
            return None;
        }
        // The member closest to a sample of its own side.
        for (k, side) in sides.iter().enumerate() {
            let sample: Vec<usize> = (0..12).map(|_| side[rng.below(side.len())]).collect();
            medoids[k] = *side
                .iter()
                .take(24)
                .chain(std::iter::once(&medoids[k]))
                .min_by(|x, y| {
                    let sx: f32 = sample.iter().map(|&o| gap(**x, o)).sum();
                    let sy: f32 = sample.iter().map(|&o| gap(**y, o)).sum();
                    sx.total_cmp(&sy)
                })
                .unwrap();
        }
    }
    let between = gap(medoids[0], medoids[1]);
    let spread = |k: usize| sides[k].iter().map(|&m| gap(m, medoids[k])).sum::<f32>() / sides[k].len() as f32;
    let within = spread(0).max(spread(1));
    if between < threshold || between < within * 2.0 {
        return None;
    }
    // The bigger side keeps the name.
    if sides[0].len() < sides[1].len() {
        sides.swap(0, 1);
        medoids.swap(0, 1);
    }
    Some((std::mem::take(&mut sides[1]), medoids))
}

/// How far apart two creatures are as kinds: in body, life and looks, and
/// in the interacting genes that decide whether their young would live.
pub fn creature_gap(a: &Genome, b: &Genome) -> f32 {
    distance(a, b) + genetics::compat_divergence(a, b) * COMPAT_GAP
}

/// How far apart two creature genomes are.
pub fn distance(a: &Genome, b: &Genome) -> f32 {
    let mut d = a.plan.distance(&b.plan);
    d += ((a.adult_scale / b.adult_scale).ln()).abs() * 2.0;
    d += (a.diet - b.diet).abs() * 3.0;
    d += ((a.sexual > 0.5) != (b.sexual > 0.5)) as u8 as f32 * 1.5;
    d += ((a.clutch / b.clutch).ln()).abs() * 0.5;
    d += ((a.live_birth > 0.5) != (b.live_birth > 0.5)) as u8 as f32;
    d += (a.helpers - b.helpers).abs() * 2.0;
    d += (a.toxin - b.toxin).abs() * 1.5;
    let p = (a.palate - b.palate).rem_euclid(1.0);
    d += p.min(1.0 - p) * 4.0 * (1.0 - a.breadth.min(b.breadth));
    d += (a.brain.hidden as f32 - b.brain.hidden as f32).abs() * 0.08;
    d += Genome::look_distance(&a.look, &b.look) * 4.0;
    d += (1.0 - Genome::kinship(&a.marker, &b.marker)) * 2.0;
    d
}

pub fn creature_traits(g: &Genome) -> Traits {
    let body = Body::develop(g);
    let p = Phenotype::read(&body, 1.0, 0);
    let a = abilities(g, &body, &p, g.adult_scale, g.brain.hidden, crate::creature::adult_matter(g, &body));
    let reproduction = if g.sexual > 0.5 {
        Reproduction::Mating
    } else if g.invest < 0.03 && g.clutch > 8.0 {
        Reproduction::Spores
    } else if g.live_birth > 0.5 {
        Reproduction::Budding
    } else {
        Reproduction::Eggs
    };
    let film = p.frac(Tissue::LightFilm);
    Traits {
        kingdom: Kingdom::Motile,
        locomotion: a.locomotion.unwrap_or(Locomotion::Crawl),
        reproduction,
        size: a.length,
        speed: a.speed,
        diet: if p.has(Tissue::Mouth) || p.has(Tissue::Gut) { g.diet } else { 0.0 },
        armor: p.frac(Tissue::Armor),
        spines: p.frac(Tissue::Spine),
        toxin: g.toxin,
        photosynthesis: (film * 3.0).min(1.0),
        neurons: g.brain.hidden as u16,
        sociality: 0.0,
        glow: p.frac(Tissue::Glow),
        zone: 0.5,
        lifespan: lifespan(g),
        clutch: g.clutch,
        look: [g.look[0], g.look[1], g.look[2], g.look[3]],
        segments: body.segments.len() as u16,
        diversity: genetics::heterozygosity(g),
        inbreeding: 1.0 - g.vigor,
        selfing: if g.sexual <= 0.25 {
            1.0
        } else if g.sexual <= 0.5 {
            0.6
        } else {
            g.selfing * 0.2
        },
        picky: 1.0 - g.breadth,
        features: p
            .kit
            .list()
            .into_iter()
            .filter(|&o| o.notable() && p.organ(o) > 0.015)
            .map(|o| o.name().to_string())
            .collect(),
    }
}

/// Expected natural lifespan in seconds: the same clock that ages them.
pub fn lifespan(g: &Genome) -> f32 {
    crate::life::lifespan(g)
}

pub fn plant_traits(g: &PlantGenome) -> Traits {
    let body = g.develop();
    Traits {
        kingdom: Kingdom::Rooted,
        locomotion: if g.floats() { Locomotion::FloatingMat } else { Locomotion::Rooted },
        reproduction: Reproduction::Seeds,
        size: body.extent * g.adult_scale,
        speed: 0.0,
        diet: if g.trap > 0.3 { g.trap } else { 0.0 },
        armor: 0.0,
        spines: g.spines,
        toxin: g.toxin,
        photosynthesis: 1.0 - g.vent,
        neurons: 0,
        sociality: 0.0,
        glow: g.glow,
        zone: 0.5,
        lifespan: g.lifespan,
        clutch: 1.0,
        look: [g.look[0], g.look[1], g.look[2], g.look[3]],
        segments: body.segments.len() as u16,
        diversity: g.heterozygosity(),
        inbreeding: 1.0 - g.vigor,
        selfing: g.selfing,
        picky: 0.0,
        features: plant_features(g),
    }
}

/// The parts a plant's genes give it, as words, most prominent first.
fn plant_features(g: &PlantGenome) -> Vec<String> {
    let leaves = if g.leaf_split > 0.5 {
        "feathery fronds"
    } else if g.tough > 0.6 && g.leaf_width >= 0.3 {
        "leathery scale-leaves"
    } else if g.leaf_width < 0.3 {
        "needles"
    } else if g.leaf_width > 0.7 {
        "broad pads"
    } else {
        "blade leaves"
    };
    let mut v: Vec<(f32, &str)> = vec![(0.5, leaves)];
    let mut has = |on: bool, weight: f32, word: &'static str| {
        if on {
            v.push((weight, word));
        }
    };
    has(g.spines > 0.5, g.spines, "thorns");
    has(g.trap > 0.3, g.trap + 0.3, "snapping traps");
    has(g.nectar > 0.3, g.nectar, "flowers");
    has(g.fruit > 0.25, g.fruit, "fruit");
    has(g.wind > 0.6, g.wind * 0.5, "seed plumes");
    has(g.floats(), 0.9, "gas bladders");
    has(!g.floats() && g.root > 0.6, g.root * 0.4, "prop roots");
    has(g.knots > 0.35, g.knots * 0.6, "water-filled knots");
    has(g.tendrils > 0.3, g.tendrils * 0.6, "climbing tendrils");
    has(g.rosette > 0.2, g.rosette * 0.5, "a basal rosette");
    has(g.tough > 0.6, g.tough * 0.5, "tough leaves");
    has(g.toxin > 0.5, g.toxin * 0.6, "poison");
    has(g.glow > 0.4, g.glow * 0.6, "glowing tips");
    has(g.stem_segments >= 5, 0.4, "a tall trunk");
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    v.into_iter().map(|(_, w)| w.to_string()).collect()
}
