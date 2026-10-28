//! Groups, leaders, dialects and wars, read from who bonds with whom. Nothing
//! here makes creatures social; it only notices when they are.

use crate::creature::Creature;
use crate::genome::Genome;
use crate::history::{
    Behavior, Census, CreatureId, Fact, GroupCensus, GroupId, SpeciesCensus, SpeciesId, StrainCensus,
};
use crate::mind::{MEANINGS, intelligibility};
use crate::species::{Rep, creature_traits, plant_traits};
use crate::think::season_quarter;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Bond strength that ties two creatures into the same group.
const TIE: f32 = 0.2;
const MIN_GROUP: usize = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub species: SpeciesId,
    pub members: Vec<CreatureId>,
    pub leader: Option<CreatureId>,
    pub parent: Option<GroupId>,
    pub born: f64,
    pub voice: [u8; MEANINGS],
    pub x_range: [f32; 2],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Society {
    pub groups: Vec<Group>,
    /// Attacks between groups since the last update: (attacker, defender) -> (attacks, deaths, x).
    pub clashes: BTreeMap<(u64, u64), (u32, u32, f32)>,
}

impl Society {
    pub fn note_attack(&mut self, a: GroupId, d: GroupId, x: f32, killed: bool) {
        let e = self.clashes.entry((a.0, d.0)).or_insert((0, 0, x));
        e.0 += 1;
        e.1 += killed as u32;
        e.2 = x;
    }

    pub fn leader_of(&self, g: GroupId) -> Option<CreatureId> {
        self.groups.iter().find(|x| x.id == g).and_then(|x| x.leader)
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Regroups everyone by their bonds and reports what changed.
pub fn regroup(w: &mut World) {
    let n = w.creatures.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let c = &w.creatures[i];
        for a in &c.mind.known {
            if a.bond < TIE && !(a.kin && a.bond >= 0.0) {
                continue;
            }
            let Some(j) = w.find(a.id) else { continue };
            let o = &w.creatures[j];
            if o.species != c.species || (o.pos[0] - c.pos[0]).abs() > 0.4 {
                continue;
            }
            // Both ways, or kin.
            if a.kin || o.mind.bond(c.id) > 0.0 {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }
    let mut sets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        sets.entry(r).or_default().push(i);
    }
    let old: HashMap<CreatureId, GroupId> = w.creatures.iter().filter_map(|c| c.group.map(|g| (c.id, g))).collect();
    let mut taken: Vec<GroupId> = Vec::new();
    let mut groups = Vec::new();
    let mut formed = Vec::new();
    for members in sets.into_values().filter(|m| m.len() >= MIN_GROUP) {
        // The group this set mostly came from keeps its id.
        let mut votes: BTreeMap<u64, usize> = BTreeMap::new();
        for &i in &members {
            if let Some(g) = old.get(&w.creatures[i].id) {
                *votes.entry(g.0).or_default() += 1;
            }
        }
        let mut ranked: Vec<(u64, usize)> = votes.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let keep = ranked.iter().find(|(g, _)| !taken.contains(&GroupId(*g))).map(|(g, _)| GroupId(*g));
        let from = ranked.first().map(|(g, _)| GroupId(g.to_owned()));
        let (id, born, parent_group) = match keep {
            Some(g) => (g, w.society.groups.iter().find(|x| x.id == g).map(|x| x.born).unwrap_or(w.time), None),
            None => (GroupId(w.id()), w.time, from),
        };
        taken.push(id);
        let species = w.creatures[members[0]].species;
        let leader = members
            .iter()
            .max_by(|&&a, &&b| w.creatures[a].rank.total_cmp(&w.creatures[b].rank))
            .map(|&i| w.creatures[i].id);
        let voice = majority_voice(members.iter().map(|&i| &w.creatures[i]));
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &i in &members {
            lo = lo.min(w.creatures[i].pos[0]);
            hi = hi.max(w.creatures[i].pos[0]);
        }
        let prev = w.society.groups.iter().find(|x| x.id == id);
        if let (Some(p), Some(l)) = (prev, leader)
            && p.leader.is_some()
            && p.leader != Some(l)
        {
            let old_leader = p.leader;
            // Overthrown if the old leader is still alive in the group.
            let overthrown = old_leader.is_some_and(|ol| {
                members.iter().any(|&i| {
                    let c = &w.creatures[i];
                    c.id == ol && c.last_attacker.is_some_and(|(by, t)| by == l && w.time - t < 120.0)
                })
            });
            w.history.push((w.time, Fact::LeaderChanged { group: id, old: old_leader, new: l, overthrown }));
            if overthrown {
                w.first(Behavior::Overthrow, species, None);
            }
        }
        if keep.is_none() {
            formed.push((id, species, parent_group, members.len() as u32));
        }
        let ids: Vec<CreatureId> = members.iter().map(|&i| w.creatures[i].id).collect();
        for &i in &members {
            w.creatures[i].group = Some(id);
        }
        groups.push(Group { id, species, members: ids, leader, parent: parent_group, born, voice, x_range: [lo, hi] });
    }
    // Everyone not in a group now has none.
    let grouped: std::collections::HashSet<CreatureId> =
        groups.iter().flat_map(|g| g.members.iter().copied()).collect();
    for c in &mut w.creatures {
        if !grouped.contains(&c.id) {
            c.group = None;
        }
    }
    for g in &w.society.groups {
        if !groups.iter().any(|x| x.id == g.id) {
            w.history.push((w.time, Fact::GroupGone { id: g.id }));
        }
    }
    for (id, species, parent_group, members) in formed {
        w.history.push((w.time, Fact::GroupFormed { id, species, parent_group, members }));
    }
    // Dialects: two groups of one species that no longer share signals.
    let mut dialects = Vec::new();
    for (a, ga) in groups.iter().enumerate() {
        for gb in groups.iter().skip(a + 1) {
            if ga.species == gb.species && intelligibility(&ga.voice, &gb.voice) <= 0.25 {
                dialects.push(ga.species);
            }
        }
    }
    for sp in dialects {
        w.first(Behavior::Dialect, sp, None);
    }
    // Traditions and migrations: a route still followed after its finder died.
    let mut found = Vec::new();
    let quarter = season_quarter(w);
    for g in &groups {
        let mut routes: BTreeMap<u64, usize> = BTreeMap::new();
        let mut spans = Vec::new();
        for id in &g.members {
            if let Some(i) = w.find(*id) {
                let c = &w.creatures[i];
                if let Some(r) = c.mind.route_from {
                    *routes.entry(r.0).or_default() += 1;
                }
                let h = c.mind.home;
                if h.iter().all(|&v| v >= 0.0) {
                    let lo = h.iter().copied().fold(f32::MAX, f32::min);
                    let hi = h.iter().copied().fold(f32::MIN, f32::max);
                    spans.push(hi - lo);
                }
            }
        }
        if let Some((&finder, &count)) = routes.iter().max_by_key(|(_, c)| **c)
            && count >= 3
            && w.time - g.born > 1200.0
            && w.find(CreatureId(finder)).is_none()
        {
            found.push((Behavior::Tradition, g.species));
        }
        if spans.len() >= 3 && spans.iter().filter(|&&s| s > 0.3).count() * 2 > spans.len() {
            found.push((Behavior::Migration, g.species));
        }
        let sterile = g.members.iter().filter(|id| w.find(**id).is_some_and(|i| w.creatures[i].sterile)).count();
        if sterile >= 3 && sterile < g.members.len() {
            found.push((Behavior::Eusociality, g.species));
        }
    }
    for (b, sp) in found {
        w.first(b, sp, None);
    }
    let _ = quarter;
    // Clashes since the last regroup.
    let clashes = std::mem::take(&mut w.society.clashes);
    for ((a, d), (attacks, deaths, x)) in clashes {
        if attacks >= 2 {
            w.history.push((w.time, Fact::Clash { attacker: GroupId(a), defender: GroupId(d), deaths, x }));
        }
    }
    w.society.groups = groups;
}

fn majority_voice<'a>(members: impl Iterator<Item = &'a Creature>) -> [u8; MEANINGS] {
    let mut counts = [[0u16; crate::mind::SYMBOLS]; MEANINGS];
    for c in members {
        for m in 0..MEANINGS {
            counts[m][c.mind.voice[m] as usize % 16] += 1;
        }
    }
    let mut v = [0u8; MEANINGS];
    for m in 0..MEANINGS {
        v[m] = counts[m].iter().enumerate().max_by_key(|(_, n)| **n).map(|(s, _)| s as u8).unwrap_or(0);
    }
    v
}

/// Who is worth following: long-lived, prolific, deadly, kind, scarred, leaders.
pub fn notability(c: &Creature, is_leader: bool) -> f32 {
    let t = &c.tally;
    (c.age / 600.0).sqrt()
        + t.kills as f32 * 0.8
        + (t.offspring as f32).sqrt()
        + t.kindness as f32 * 0.05
        + t.scars as f32 * 0.6
        + is_leader as u8 as f32 * 2.0
        + (c.generation as f32).ln_1p() * 0.1
}

pub fn census(w: &World) -> Census {
    struct Acc {
        count: u32,
        biomass: f32,
        x: f32,
        social: f32,
        sample: Option<usize>,
    }
    let mut acc: BTreeMap<u64, Acc> = BTreeMap::new();
    for (i, c) in w.creatures.iter().enumerate() {
        let a = acc.entry(c.species.0).or_insert(Acc { count: 0, biomass: 0.0, x: 0.0, social: 0.0, sample: None });
        a.count += 1;
        a.biomass += c.matter;
        a.x += c.pos[0];
        a.social += c.group.is_some() as u8 as f32;
        if a.sample.is_none_or(|s| w.creatures[s].maturity < c.maturity) {
            a.sample = Some(i);
        }
    }
    let mut species = Vec::new();
    for (id, a) in &acc {
        let c = &w.creatures[a.sample.unwrap()];
        let mut traits = creature_traits(&c.genome);
        traits.sociality = a.social / a.count as f32;
        traits.zone = (a.x / a.count as f32) / vesper_terrain::WORLD_W;
        species.push(SpeciesCensus {
            id: SpeciesId(*id),
            count: a.count,
            biomass: a.biomass,
            mean_x: a.x / a.count as f32,
            traits,
        });
    }
    let mut pacc: BTreeMap<u64, (u32, f32, f32, usize)> = BTreeMap::new();
    for (i, p) in w.plants.iter().enumerate() {
        let e = pacc.entry(p.species.0).or_insert((0, 0.0, 0.0, i));
        e.0 += 1;
        e.1 += p.matter;
        e.2 += p.x;
    }
    for (id, (count, biomass, x, sample)) in pacc {
        let mut traits = plant_traits(&w.plants[sample].genome);
        traits.zone = (x / count as f32) / vesper_terrain::WORLD_W;
        species.push(SpeciesCensus { id: SpeciesId(id), count, biomass, mean_x: x / count as f32, traits });
    }
    let groups = w
        .society
        .groups
        .iter()
        .map(|g| GroupCensus {
            id: g.id,
            species: g.species,
            members: g.members.len() as u32,
            x_range: g.x_range,
            leader: g.leader,
            dialect: g.voice.iter().fold(0x811c_9dc5u32, |h, &v| (h ^ v as u32).wrapping_mul(0x0100_0193)),
            signals: [g.voice[0], g.voice[1], g.voice[2], g.voice[3]],
        })
        .collect();
    let strains = w
        .strains
        .iter()
        .filter(|s| s.hosts > 0)
        .map(|s| StrainCensus { id: s.id, infected: s.hosts, traits: s.traits() })
        .collect();
    let mut ranked: Vec<(f32, usize)> = w
        .creatures
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let lead = c.group.is_some_and(|g| w.society.leader_of(g) == Some(c.id));
            (notability(c, lead), i)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let standouts = ranked
        .iter()
        .take(20)
        .map(|&(_, i)| {
            let c = &w.creatures[i];
            let mut l = c.life();
            l.is_leader = c.group.is_some_and(|g| w.society.leader_of(g) == Some(c.id));
            l
        })
        .collect();
    let living =
        w.creatures.iter().map(|c| c.matter).sum::<f32>() + w.plants.iter().map(|p| p.matter + p.fruit).sum::<f32>();
    let dead = w.remains.iter().map(|r| r.matter).sum::<f32>() + w.air.iter().sum::<f32>();
    let _ = Genome::kinship;
    let _: Option<&Rep> = None;
    Census {
        creatures: w.creatures.len() as u32,
        plants: w.plants.len() as u32,
        living_matter: living,
        dead_matter: dead,
        soil_matter: w.soil.iter().sum(),
        species,
        groups,
        strains,
        standouts,
    }
}
