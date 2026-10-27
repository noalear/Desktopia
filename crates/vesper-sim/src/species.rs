//! Species: lineages that have drifted far enough from their ancestors to
//! count as new, and the record of every one that ever lived.

use crate::body::{Body, Phenotype};
use crate::creature::abilities;
use crate::genome::{Genome, Tissue};
use crate::history::{Fact, Kingdom, Locomotion, Reproduction, SpeciesId, Traits};
use crate::plant::PlantGenome;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Genetic distance at which a lineage becomes a new species.
pub const SPLIT: f32 = 7.0;
pub const PLANT_SPLIT: f32 = 5.0;

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
        self.add(None, Rep::Creature(g.clone()), now, facts)
    }

    pub fn found_plant(&mut self, g: &Arc<PlantGenome>, now: f64, facts: &mut Vec<(f64, Fact)>) -> SpeciesId {
        self.add(None, Rep::Plant(g.clone()), now, facts)
    }

    /// The species a newborn belongs to: its parent's, a sister species it
    /// matches, or a brand new one.
    pub fn classify_creature(
        &mut self,
        g: &Arc<Genome>,
        parent: SpeciesId,
        now: f64,
        facts: &mut Vec<(f64, Fact)>,
    ) -> SpeciesId {
        let d = match &self.get(parent).rep {
            Rep::Creature(r) => distance(g, r),
            Rep::Plant(_) => f32::MAX,
        };
        if d < SPLIT {
            return parent;
        }
        // Siblings that split off the same parent recently join the same new species.
        for r in self.list.iter().rev().take(64) {
            if r.parent == Some(parent)
                && r.extinct.is_none()
                && let Rep::Creature(rg) = &r.rep
                && distance(g, rg) < SPLIT * 0.6
            {
                return r.id;
            }
        }
        self.add(Some(parent), Rep::Creature(g.clone()), now, facts)
    }

    pub fn classify_plant(
        &mut self,
        g: &Arc<PlantGenome>,
        parent: SpeciesId,
        now: f64,
        facts: &mut Vec<(f64, Fact)>,
    ) -> SpeciesId {
        let d = match &self.get(parent).rep {
            Rep::Plant(r) => g.distance(r),
            Rep::Creature(_) => f32::MAX,
        };
        if d < PLANT_SPLIT {
            return parent;
        }
        for r in self.list.iter().rev().take(64) {
            if r.parent == Some(parent)
                && r.extinct.is_none()
                && let Rep::Plant(rg) = &r.rep
                && g.distance(rg) < PLANT_SPLIT * 0.6
            {
                return r.id;
            }
        }
        self.add(Some(parent), Rep::Plant(g.clone()), now, facts)
    }

    pub fn born(&mut self, id: SpeciesId) {
        let r = self.get_mut(id);
        r.alive += 1;
        r.ever += 1;
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

/// How far apart two creature genomes are.
pub fn distance(a: &Genome, b: &Genome) -> f32 {
    let mut d = 0.0;
    let n = a.parts.len().max(b.parts.len());
    for i in 0..n {
        match (a.parts.get(i), b.parts.get(i)) {
            (Some(p), Some(q)) => {
                if p.tissue != q.tissue {
                    d += 1.0;
                }
                d += (p.length - q.length).abs() * 0.5 + (p.width - q.width).abs();
                d += (p.recursion as f32 - q.recursion as f32).abs() * 0.3;
                d += (p.links.len() as f32 - q.links.len() as f32).abs() * 0.5;
                for (l, m) in p.links.iter().zip(&q.links) {
                    if l.to != m.to {
                        d += 0.4;
                    }
                    if l.pair != m.pair {
                        d += 0.3;
                    }
                    d += (l.angle - m.angle).abs() * 0.1;
                }
            }
            _ => d += 1.5,
        }
    }
    d += ((a.adult_scale / b.adult_scale).ln()).abs() * 2.0;
    d += (a.diet - b.diet).abs() * 3.0;
    d += ((a.sexual > 0.5) != (b.sexual > 0.5)) as u8 as f32 * 1.5;
    d += ((a.clutch / b.clutch).ln()).abs() * 0.5;
    d += ((a.live_birth > 0.5) != (b.live_birth > 0.5)) as u8 as f32;
    d += (a.helpers - b.helpers).abs() * 2.0;
    d += (a.toxin - b.toxin).abs() * 1.5;
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
    }
}

/// Expected natural lifespan in seconds.
pub fn lifespan(g: &Genome) -> f32 {
    crate::life::LIFESPAN_K * (g.adult_scale / 0.01).powf(0.6) / g.aging.max(0.1) + g.growth_time
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
    }
}
