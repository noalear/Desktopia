//! The written history: everything the chronicle knows, built by applying facts
//! one at a time. Applying the same facts always gives the same history, so the
//! window can rebuild it from the files the wallpaper writes.

use crate::facts::*;
use crate::likeness::Likeness;
use crate::names::{self, mix};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Life whose notability passes this gets a biography.
pub const NOTABLE: f32 = 22.0;
/// A notable death gets its own line in the annals past this, at the least.
/// Each such line raises the bar by `DEATH_BAR_STEP`, and the bar sinks back
/// over `DEATH_BAR_EASE` seconds, so a massacre is told through its most
/// remarkable dead rather than all of them.
const NOTABLE_DEATH: f32 = 30.0;
const DEATH_BAR_STEP: f32 = 6.0;
const DEATH_BAR_EASE: f64 = 2.0 * 3600.0;
/// At most this many biographies are kept; the least notable dead go first.
const MAX_NOTABLES: usize = 400;
/// Deaths between two groups, inside `FEUD_WINDOW`, that make a war.
const WAR_DEATHS: u32 = 10;
const FEUD_WINDOW: f64 = 3600.0;
/// A war ends after this long without a clash.
const PEACE_AFTER: f64 = 3600.0;
/// Share of a host species infected that makes an outbreak, and that ends one.
const OUTBREAK_START: f32 = 0.15;
const OUTBREAK_END: f32 = 0.03;
/// Members, and age in seconds, that make a group worth writing about.
const GROUP_NOTABLE: u32 = 30;
const GROUP_SETTLE: f64 = 600.0;
/// Seconds an unremarkable group is kept after it scatters.
const GROUP_FORGET: f64 = 120.0;
/// A new species is written about once it numbers this many, or has lasted
/// `SPECIES_SETTLE` seconds; most branches die out within moments.
const SPECIES_NOTABLE: u32 = 20;
const SPECIES_SETTLE: f64 = 1800.0;
/// The same kind of weather is written about at most this often.
const NATURE_QUIET: f64 = 3600.0;
/// Censuses of population kept per species for spotting famines.
const FAMINE_WINDOW: usize = 20;
/// Time a new species must hold the most biomass before an era changes.
const ERA_HOLD: f64 = 6.0 * 3600.0;

/// How much a life stands out. The simulation can use it to choose a census's
/// standouts; past [`NOTABLE`] the chronicle keeps a biography.
pub fn notability(life: &Life) -> f32 {
    // Deeds count with diminishing returns: the hundredth kill matters less
    // than the tenth. Kindness is counted per moment of sharing, so it runs high.
    let mut score = 4.0 * (life.kills as f32).ln_1p()
        + 3.0 * (life.offspring as f32).ln_1p()
        + 2.0 * life.scars as f32
        + 1.5 * (life.kindness as f32 / 50.0).ln_1p()
        + 3.0 * life.rank.max(0.0)
        + 3.0 * (life.age / 600.0).ln_1p();
    if life.is_leader {
        score += 6.0;
    }
    score
}

/// A value over time, thinned as it grows so it never holds more than a few
/// hundred points.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Series {
    pub points: Vec<(f64, f32)>,
    step: f64,
}

const SERIES_MAX: usize = 400;

impl Series {
    pub fn push(&mut self, t: f64, v: f32) {
        if self.step == 0.0 {
            self.step = 60.0;
        }
        let n = self.points.len();
        if n >= 2 && t - self.points[n - 2].0 < self.step {
            self.points[n - 1] = (t, v);
        } else {
            self.points.push((t, v));
        }
        if self.points.len() > SERIES_MAX {
            self.step *= 2.0;
            let last = *self.points.last().unwrap();
            let mut kept: Vec<(f64, f32)> = Vec::with_capacity(SERIES_MAX / 2 + 2);
            for &p in &self.points {
                if kept.last().is_none_or(|l| p.0 - l.0 >= self.step) {
                    kept.push(p);
                }
            }
            if kept.last() != Some(&last) {
                kept.push(last);
            }
            self.points = kept;
        }
    }

    pub fn last(&self) -> f32 {
        self.points.last().map_or(0.0, |p| p.1)
    }

    pub fn max(&self) -> f32 {
        self.points.iter().fold(0.0, |m, p| m.max(p.1))
    }

    /// The value at time `t`, holding the latest point at or before it.
    pub fn at(&self, t: f64) -> f32 {
        match self.points.partition_point(|p| p.0 <= t) {
            0 => 0.0,
            i => self.points[i - 1].1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    /// A species arose.
    Origin,
    Extinction,
    /// A notable creature died.
    Death,
    War,
    Plague,
    Famine,
    /// Groups forming, splitting, leaders, dialects.
    Society,
    /// A behavior seen for the first time.
    First,
    Nature,
    /// A new species came to dominate.
    Era,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Origin,
        Kind::Extinction,
        Kind::Death,
        Kind::War,
        Kind::Plague,
        Kind::Famine,
        Kind::Society,
        Kind::First,
        Kind::Nature,
        Kind::Era,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Origin => "New species",
            Kind::Extinction => "Extinctions",
            Kind::Death => "Notable deaths",
            Kind::War => "Wars",
            Kind::Plague => "Plagues",
            Kind::Famine => "Famines",
            Kind::Society => "Societies",
            Kind::First => "Firsts",
            Kind::Nature => "Nature",
            Kind::Era => "Eras",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ref {
    Species(SpeciesId),
    Creature(CreatureId),
    Group(GroupId),
    Strain(StrainId),
    War(usize),
}

/// One line of the annals. `text` holds links as `{s:12}` (species, singular),
/// `{S:12}` (species, plural), `{c:..}` creature, `{g:..}` group, `{p:..}`
/// plague strain and `{w:..}` war; [`History::pieces`] resolves them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub tick: f64,
    /// Unix seconds when it was written.
    pub wall: f64,
    pub kind: Kind,
    /// 1 minor, 2 notable, 3 major.
    pub weight: u8,
    pub title: String,
    pub text: String,
    pub refs: Vec<Ref>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CauseKind {
    Predation,
    Cannibalism,
    Infanticide,
    Violence,
    Sacrifice,
    Devoured,
    Starvation,
    Disease,
    Parasite,
    Age,
    Heat,
    Cold,
    Trap,
    Lava,
    Lightning,
    Flood,
    Thirst,
    Choked,
}

impl CauseKind {
    pub fn of(c: &DeathCause) -> Self {
        match c {
            DeathCause::Predation { .. } => Self::Predation,
            DeathCause::Cannibalism { .. } => Self::Cannibalism,
            DeathCause::Infanticide { .. } => Self::Infanticide,
            DeathCause::Violence { .. } => Self::Violence,
            DeathCause::Sacrifice { .. } => Self::Sacrifice,
            DeathCause::Devoured { .. } => Self::Devoured,
            DeathCause::Starvation => Self::Starvation,
            DeathCause::Disease { .. } => Self::Disease,
            DeathCause::Parasite { .. } => Self::Parasite,
            DeathCause::Age => Self::Age,
            DeathCause::Heat => Self::Heat,
            DeathCause::Cold => Self::Cold,
            DeathCause::Trap { .. } => Self::Trap,
            DeathCause::Lava => Self::Lava,
            DeathCause::Lightning => Self::Lightning,
            DeathCause::Flood => Self::Flood,
            DeathCause::Thirst => Self::Thirst,
            DeathCause::Choked => Self::Choked,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Predation => "eaten",
            Self::Cannibalism => "eaten by kin",
            Self::Infanticide => "killed young",
            Self::Violence => "war",
            Self::Sacrifice => "died defending",
            Self::Devoured => "fed to young",
            Self::Starvation => "starved",
            Self::Disease => "disease",
            Self::Parasite => "parasites",
            Self::Age => "old age",
            Self::Heat => "heat",
            Self::Cold => "cold",
            Self::Trap => "trap plants",
            Self::Lava => "lava",
            Self::Lightning => "lightning",
            Self::Flood => "floods",
            Self::Thirst => "thirst",
            Self::Choked => "ash and dust",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Species {
    pub id: SpeciesId,
    pub parent: Option<SpeciesId>,
    pub children: Vec<SpeciesId>,
    pub genus: String,
    pub epithet: String,
    /// Field-guide name, like "plated rust trudger".
    pub common: String,
    pub arose: f64,
    pub extinct: Option<f64>,
    /// Traits when it arose, and now.
    pub founding: Traits,
    pub traits: Traits,
    pub population: Series,
    pub count: u32,
    pub peak: u32,
    pub biomass: f32,
    pub x: f32,
    /// Deaths by cause.
    pub deaths: BTreeMap<CauseKind, u32>,
    /// Kills it made, by victim species.
    pub prey: BTreeMap<SpeciesId, u32>,
    /// Its members killed, by killer species.
    pub predators: BTreeMap<SpeciesId, u32>,
    /// Steps from a founding species.
    pub depth: u32,
    /// Whether the annals have told of its origin yet.
    #[serde(default)]
    pub announced: bool,
    last_death: Option<(CreatureId, DeathCause, f32)>,
    recent: VecDeque<u32>,
    starved: u32,
    famine_until: f64,
}

impl Species {
    pub fn alive(&self) -> bool {
        self.extinct.is_none()
    }

    pub fn latin(&self) -> String {
        format!("{} {}", self.genus, self.epithet)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub species: SpeciesId,
    pub parent: Option<GroupId>,
    pub name: String,
    pub formed: f64,
    pub gone: Option<f64>,
    pub members: u32,
    pub peak: u32,
    pub x: [f32; 2],
    pub leader: Option<CreatureId>,
    pub dialect: u32,
    /// Its light signals for danger, food, mating and threat.
    pub signals: [u8; 4],
    /// Big enough to be written about.
    pub notable: bool,
    /// Enemies it killed, and members it lost, in clashes.
    pub kills: u32,
    pub losses: u32,
    own_dialect: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Strain {
    pub id: StrainId,
    pub parent: Option<StrainId>,
    pub host: SpeciesId,
    pub traits: StrainTraits,
    pub arose: f64,
    pub gone: Option<f64>,
    pub name: String,
    pub infected: u32,
    pub peak: u32,
    pub deaths: u32,
    /// Index into `outbreaks` while one is running.
    pub outbreak: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outbreak {
    pub strain: StrainId,
    pub host: SpeciesId,
    pub began: f64,
    pub ended: Option<f64>,
    pub peak: u32,
    pub deaths: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct War {
    pub a: GroupId,
    pub b: GroupId,
    pub name: String,
    pub began: f64,
    pub last_clash: f64,
    pub ended: Option<f64>,
    pub deaths: u32,
    pub clashes: u32,
    pub outcome: Option<String>,
    /// Where it began.
    pub x: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Feud {
    a: GroupId,
    b: GroupId,
    first: f64,
    deaths: u32,
    x: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Era {
    pub species: SpeciesId,
    pub began: f64,
    pub ended: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notable {
    pub id: CreatureId,
    pub species: SpeciesId,
    pub name: String,
    /// "the Devourer".
    pub title: String,
    pub life: Life,
    pub score: f32,
    pub first_seen: f64,
    pub died: Option<f64>,
    pub cause: Option<DeathCause>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct History {
    /// Latest tick applied, and the wall clock then.
    pub now: f64,
    pub wall: f64,
    /// Wall clock of the first fact.
    pub began_wall: f64,
    pub species: BTreeMap<SpeciesId, Species>,
    pub groups: BTreeMap<GroupId, Group>,
    pub strains: BTreeMap<StrainId, Strain>,
    pub notables: BTreeMap<CreatureId, Notable>,
    /// Names given, kept after a biography is dropped while the annals mention them.
    pub names: BTreeMap<CreatureId, String>,
    pub outbreaks: Vec<Outbreak>,
    pub wars: Vec<War>,
    pub eras: Vec<Era>,
    pub entries: Vec<Entry>,
    pub firsts: Vec<Behavior>,
    /// Living creatures, and living plants.
    pub population: Series,
    pub plants: Series,
    /// Where the world's fixed matter is: living, dead, and in the soil.
    pub matter: [f32; 3],
    pub deaths: u64,
    genera: BTreeMap<String, Traits>,
    feuds: Vec<Feud>,
    era_rival: Option<(SpeciesId, f64)>,
    /// What the annals mention: kept when it would otherwise be forgotten.
    #[serde(default)]
    cited: BTreeSet<Ref>,
    /// The notability a death needs for its own line, and when it was set.
    #[serde(default)]
    death_bar: (f32, f64),
    /// When each kind of weather was last written about.
    #[serde(default)]
    weather: Vec<(NatureKind, f64)>,
    /// What each species looks like, from the last time one was drawn.
    #[serde(default)]
    pub likenesses: BTreeMap<SpeciesId, Likeness>,
}

/// A run of entry text: plain words, or a link to something.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    Link(Ref, String),
}

fn sp(id: SpeciesId) -> String {
    format!("{{s:{}}}", id.0)
}
/// "a plated trudger" / "an ember fan".
fn asp(id: SpeciesId) -> String {
    format!("{{a:{}}}", id.0)
}
fn sps(id: SpeciesId) -> String {
    format!("{{S:{}}}", id.0)
}
fn cr(id: CreatureId) -> String {
    format!("{{c:{}}}", id.0)
}
fn gr(id: GroupId) -> String {
    format!("{{g:{}}}", id.0)
}
fn st(id: StrainId) -> String {
    format!("{{p:{}}}", id.0)
}
fn wr(i: usize) -> String {
    format!("{{w:{i}}}")
}

/// English plural of a name's last word.
pub fn plural(name: &str) -> String {
    if name.ends_with('s') || name.ends_with('x') || name.ends_with("sh") || name.ends_with("ch") {
        format!("{name}es")
    } else if name.ends_with('y') && !name.ends_with("ay") && !name.ends_with("ey") && !name.ends_with("oy") {
        format!("{}ies", &name[..name.len() - 1])
    } else {
        format!("{name}s")
    }
}

/// Distance between two trait sets, roughly 0..2; above 0.6 starts a new genus.
fn distance(a: &Traits, b: &Traits) -> f32 {
    let mut d = (a.size.max(1e-4) / b.size.max(1e-4)).ln().abs() * 0.25
        + (a.diet - b.diet).abs()
        + (a.armor - b.armor).abs() * 0.5
        + (a.sociality - b.sociality).abs() * 0.5
        + (a.zone - b.zone).abs() * 0.5
        + (((a.neurons + 1) as f32) / ((b.neurons + 1) as f32)).ln().abs() * 0.15;
    if a.locomotion != b.locomotion {
        d += 0.4;
    }
    if a.kingdom != b.kingdom {
        d += 1.0;
    }
    d
}

impl History {
    /// Applies one fact. `wall` is Unix seconds when it happened.
    pub fn apply(&mut self, tick: f64, wall: f64, fact: &Fact) {
        if self.began_wall == 0.0 {
            self.began_wall = wall;
        }
        self.now = self.now.max(tick);
        self.wall = self.wall.max(wall);
        match fact {
            Fact::SpeciesArose { id, parent, traits } => self.species_arose(*id, *parent, traits),
            Fact::SpeciesExtinct { id } => self.species_extinct(*id),
            Fact::Death { who, species, cause, life } => self.death(*who, *species, cause, life),
            Fact::GroupFormed { id, species, parent_group, .. } => self.group_formed(*id, *species, *parent_group),
            Fact::GroupGone { id } => self.group_gone(*id),
            Fact::Clash { attacker, defender, deaths, x } => self.clash(*attacker, *defender, *deaths, *x),
            Fact::LeaderChanged { group, old, new, overthrown } => self.leader_changed(*group, *old, *new, *overthrown),
            Fact::PathogenArose { id, parent, host, traits } => {
                self.strains.insert(
                    *id,
                    Strain {
                        id: *id,
                        parent: *parent,
                        host: *host,
                        traits: traits.clone(),
                        arose: tick,
                        gone: None,
                        name: names::plague_name(id.0, traits),
                        infected: 0,
                        peak: 0,
                        deaths: 0,
                        outbreak: None,
                    },
                );
            }
            Fact::PathogenGone { id } => self.pathogen_gone(*id),
            Fact::Nature { kind, x, strength } => self.nature(*kind, *x, *strength),
            Fact::First { behavior, species, who } => self.first(*behavior, *species, *who),
            Fact::Census(c) => self.census(c),
        }
    }

    fn write(&mut self, kind: Kind, weight: u8, title: String, text: String, refs: Vec<Ref>) {
        self.cited.extend(refs.iter().copied());
        let ids = |tag: &str| -> Vec<u64> {
            text.split(tag).skip(1).filter_map(|t| t.split('}').next().and_then(|n| n.parse().ok())).collect()
        };
        self.cited.extend(ids("{c:").into_iter().map(|c| Ref::Creature(CreatureId(c))));
        self.cited.extend(ids("{g:").into_iter().map(|g| Ref::Group(GroupId(g))));
        self.cited.extend(ids("{p:").into_iter().map(|p| Ref::Strain(StrainId(p))));
        self.entries.push(Entry { tick: self.now, wall: self.wall, kind, weight, title, text, refs });
    }

    fn common(&self, id: SpeciesId) -> String {
        self.species.get(&id).map_or_else(|| "unknown life".into(), |s| s.common.clone())
    }

    fn species_arose(&mut self, id: SpeciesId, parent: Option<SpeciesId>, t: &Traits) {
        if self.species.contains_key(&id) {
            return;
        }
        let parent_rec = parent.and_then(|p| self.species.get(&p));
        // Stay in the parent's genus unless the body plan has wandered far from
        // the genus founder.
        let genus = match parent_rec {
            Some(p) if self.genera.get(&p.genus).is_some_and(|g| distance(g, t) < 0.6) => p.genus.clone(),
            _ => {
                let mut g = names::genus(id.0);
                let mut n = 0;
                while self.genera.contains_key(&g) {
                    n += 1;
                    g = names::genus(id.0 ^ (n << 32));
                }
                self.genera.insert(g.clone(), t.clone());
                g
            }
        };
        let depth = parent_rec.map_or(0, |p| p.depth + 1);
        let mut variant = 0;
        let common = loop {
            let c = names::common_name(t, id.0, variant);
            if variant > 12 || !self.species.values().any(|s| s.common == c) {
                break c;
            }
            variant += 1;
        };
        let rec = Species {
            id,
            parent,
            children: Vec::new(),
            genus,
            epithet: names::epithet(id.0),
            common,
            arose: self.now,
            extinct: None,
            founding: t.clone(),
            traits: t.clone(),
            population: Series::default(),
            count: 0,
            peak: 0,
            biomass: 0.0,
            x: t.zone,
            deaths: BTreeMap::new(),
            prey: BTreeMap::new(),
            predators: BTreeMap::new(),
            depth,
            announced: parent.is_none(),
            last_death: None,
            recent: VecDeque::new(),
            starved: 0,
            famine_until: 0.0,
        };
        if let Some(p) = parent.and_then(|p| self.species.get_mut(&p)) {
            p.children.push(id);
        }
        let first = parent.is_none();
        self.species.insert(id, rec);
        if first {
            let s = &self.species[&id];
            let title = format!("The {} take hold", plural(&s.common));
            let text = format!("{} are among the first life on Vesper, found in {}.", sps(id), names::region(t.zone));
            self.write(Kind::Origin, 1, title, text, vec![Ref::Species(id)]);
        }
    }

    /// Drops a branch that died before the annals told of it, and any such
    /// branches it leaves childless, so a long-lived world stays small.
    fn forget(&mut self, id: SpeciesId) {
        let Some(s) = self.species.get(&id) else { return };
        if s.announced || s.alive() || !s.children.is_empty() {
            return;
        }
        let (parent, genus) = (s.parent, s.genus.clone());
        self.species.remove(&id);
        self.likenesses.remove(&id);
        if !self.species.values().any(|o| o.genus == genus) {
            self.genera.remove(&genus);
        }
        if let Some(p) = parent.and_then(|p| self.species.get_mut(&p)) {
            p.children.retain(|c| *c != id);
            let p = p.id;
            self.forget(p);
        }
    }

    /// A common name no species told of so far has used.
    fn unique_common(&self, id: SpeciesId) -> String {
        let s = &self.species[&id];
        let taken = |c: &str| self.species.values().any(|o| o.id != id && o.announced && o.common == c);
        if !taken(&s.common) {
            return s.common.clone();
        }
        (0..24)
            .map(|v| names::common_name(&s.founding, id.0, v))
            .chain((0..64).map(|k| names::marked(mix(id.0 ^ (k << 24)), &s.common)))
            .find(|c| !taken(c))
            .unwrap_or_else(|| s.common.clone())
    }

    /// Tells of a branch of life once it has taken hold.
    fn announce(&mut self, id: SpeciesId) {
        let common = self.unique_common(id);
        let Some(s) = self.species.get_mut(&id) else { return };
        s.announced = true;
        s.common = common;
        let Some(p) = s.parent.and_then(|p| self.species.get(&p)) else { return };
        let s = &self.species[&id];
        let diff = names::difference(&s.founding, &p.traits);
        let new_genus = p.genus != s.genus;
        let title = format!("The {} appear", plural(&s.common));
        let mut text = format!(
            "{} branched off from {} in {} {} ago: {diff}.",
            sps(id),
            sps(p.id),
            names::region(s.founding.zone),
            names::span(self.now - s.arose)
        );
        if new_genus {
            text.push_str(" Different enough to start a new genus.");
        }
        text.push_str(&format!(" They have taken hold and number {}.", s.count));
        let refs = vec![Ref::Species(id), Ref::Species(p.id)];
        self.write(Kind::Origin, if new_genus { 2 } else { 1 }, title, text, refs);
    }

    fn species_extinct(&mut self, id: SpeciesId) {
        let now = self.now;
        let Some(s) = self.species.get_mut(&id) else { return };
        if s.extinct.is_some() {
            return;
        }
        s.extinct = Some(now);
        s.count = 0;
        s.population.push(now, 0.0);
        if !s.announced {
            self.forget(id);
            return;
        }
        let lasted = names::span(now - s.arose);
        let peak = s.peak;
        let last = s.last_death;
        let children: Vec<SpeciesId> = s.children.clone();
        let latin = s.latin();
        let common = s.common.clone();
        let living_kids = children.iter().filter(|c| self.species.get(c).is_some_and(|c| c.alive())).count();
        let mut text = match last {
            Some((who, cause, x)) => format!("The last {} {}. ", sp(id), self.cause_phrase(who, &cause, x)),
            None => format!("The last {} is gone. ", sp(id)),
        };
        text.push_str(&format!("The species ({latin}) lasted {lasted} and numbered {peak} at its height. "));
        text.push_str(&match living_kids {
            0 if children.is_empty() => "It left no descendants.".to_string(),
            0 => "Its descendant species are all gone too; the line has ended.".to_string(),
            1 => "One descendant species carries its line on.".to_string(),
            n => format!("{n} descendant species carry its line on."),
        });
        let weight = if peak >= 200 || (children.is_empty() && peak >= 50) { 3 } else { 2 };
        let mut refs = vec![Ref::Species(id)];
        if let Some((who, ..)) = last
            && self.names.contains_key(&who)
        {
            refs.push(Ref::Creature(who));
        }
        self.write(Kind::Extinction, weight, format!("Extinction of the {}", plural(&common)), text, refs);
        // An era ends with its species.
        if let Some(e) = self.eras.last_mut()
            && e.species == id
            && e.ended.is_none()
        {
            e.ended = Some(now);
        }
    }

    /// "was torn apart by a lantern stalker", with links.
    fn cause_phrase(&self, who: CreatureId, cause: &DeathCause, x: f32) -> String {
        let named = |c: CreatureId| c != who && self.names.contains_key(&c);
        match *cause {
            DeathCause::Predation { killer, killer_species } if named(killer) => {
                format!("was torn apart and eaten by {}, {}", cr(killer), asp(killer_species))
            }
            DeathCause::Predation { killer_species, .. } => {
                format!("was torn apart and eaten by {}", asp(killer_species))
            }
            DeathCause::Cannibalism { killer } if named(killer) => {
                format!("was killed and eaten by one of its own, {}", cr(killer))
            }
            DeathCause::Cannibalism { .. } => "was killed and eaten by one of its own kind".into(),
            DeathCause::Infanticide { killer } if named(killer) => format!("was killed as a newborn by {}", cr(killer)),
            DeathCause::Infanticide { .. } => "was killed as a newborn by an adult that was not its parent".into(),
            DeathCause::Violence { by_group, killer } if named(killer) => {
                format!("was cut down by {} of {}", cr(killer), gr(by_group))
            }
            DeathCause::Violence { by_group, .. } => format!("was killed fighting {}", gr(by_group)),
            DeathCause::Sacrifice { defending } if named(defending) => {
                format!("died defending {}", cr(defending))
            }
            DeathCause::Sacrifice { .. } => "died defending its kin".into(),
            DeathCause::Devoured { .. } => "gave its own body to its young as their first meal".into(),
            DeathCause::Starvation => "starved to death".into(),
            DeathCause::Disease { strain } => format!("wasted away from {}", st(strain)),
            DeathCause::Parasite { strain } => format!("was eaten from the inside by {}", st(strain)),
            DeathCause::Age => "died of old age".into(),
            DeathCause::Heat => format!("died of the heat in {}", names::place(x)),
            DeathCause::Cold => format!("froze to death in {}", names::place(x)),
            DeathCause::Trap { plant_species } => format!("was caught and slowly digested by {}", asp(plant_species)),
            DeathCause::Lava => "was burned alive by lava".into(),
            DeathCause::Lightning => "was struck dead by lightning".into(),
            DeathCause::Flood => "was swept away in a flood".into(),
            DeathCause::Thirst => format!("dried out and died of thirst in {}", names::place(x)),
            DeathCause::Choked => format!("choked on ash and dust in {}", names::place(x)),
        }
    }

    /// Whether a death is too ordinary to name: the creature never stood out
    /// and doesn't in death either. These only change counts, so the log keeps
    /// them as a [`Tally`] instead of a line each.
    pub fn is_routine(&self, who: CreatureId, life: &Life) -> bool {
        !self.notables.contains_key(&who) && notability(life) < NOTABLE + 4.0
    }

    /// Keeps a new portrait of a species the chronicle knows.
    pub fn apply_likeness(&mut self, tick: f64, wall: f64, species: SpeciesId, likeness: &Likeness) {
        if self.began_wall == 0.0 {
            self.began_wall = wall;
        }
        self.now = self.now.max(tick);
        self.wall = self.wall.max(wall);
        if self.species.contains_key(&species) {
            self.likenesses.insert(species, Likeness { wall, ..likeness.clone() });
        }
    }

    /// Applies a batch of routine deaths, exactly as applying each would.
    pub fn apply_tally(&mut self, tick: f64, wall: f64, tally: &Tally) {
        if self.began_wall == 0.0 {
            self.began_wall = wall;
        }
        self.now = self.now.max(tick);
        self.wall = self.wall.max(wall);
        for &(species, cause, n) in &tally.counts {
            self.count_deaths(species, &cause, n);
        }
        for &(species, who, cause, x) in &tally.last {
            if let Some(s) = self.species.get_mut(&species) {
                s.last_death = Some((who, cause, x));
            }
        }
    }

    fn count_deaths(&mut self, species: SpeciesId, cause: &DeathCause, n: u32) {
        self.deaths += n as u64;
        let kind = CauseKind::of(cause);
        if let Some(s) = self.species.get_mut(&species) {
            *s.deaths.entry(kind).or_default() += n;
            if kind == CauseKind::Starvation {
                s.starved += n;
            }
        }
        match *cause {
            DeathCause::Predation { killer_species, .. } => {
                if let Some(k) = self.species.get_mut(&killer_species) {
                    *k.prey.entry(species).or_default() += n;
                }
                if let Some(s) = self.species.get_mut(&species) {
                    *s.predators.entry(killer_species).or_default() += n;
                }
            }
            DeathCause::Disease { strain } | DeathCause::Parasite { strain } => {
                if let Some(s) = self.strains.get_mut(&strain) {
                    s.deaths += n;
                    if let Some(o) = s.outbreak.and_then(|i| self.outbreaks.get_mut(i)) {
                        o.deaths += n;
                    }
                }
            }
            _ => {}
        }
    }

    fn death(&mut self, who: CreatureId, species: SpeciesId, cause: &DeathCause, life: &Life) {
        self.count_deaths(species, cause, 1);
        if let Some(s) = self.species.get_mut(&species) {
            s.last_death = Some((who, *cause, life.x));
        }

        let score = notability(life);
        let known = self.notables.contains_key(&who);
        // Lives that were never in a census can still earn a biography in death.
        if !known && score < NOTABLE + 4.0 {
            return;
        }
        if !known {
            self.promote(who, species, life, score);
        }
        let now = self.now;
        let n = self.notables.get_mut(&who).unwrap();
        n.life = life.clone();
        n.score = n.score.max(score);
        n.died = Some(now);
        n.cause = Some(*cause);
        n.title = title_for(life, &n.title);
        let (name, title) = (n.name.clone(), n.title.clone());

        let other_named = other_party(cause).is_some_and(|c| self.names.contains_key(&c));
        let led_a_people = life.is_leader && life.group.and_then(|g| self.groups.get(&g)).is_some_and(|g| g.notable);
        // Leaders of peoples and those tied to a named creature need less.
        let bar = self.death_bar();
        let bonus = if led_a_people || other_named { 8.0 } else { 0.0 };
        if score + bonus < bar {
            self.trim_notables();
            return;
        }
        self.death_bar = (bar + DEATH_BAR_STEP, now);
        let phrase = self.cause_phrase(who, cause, life.x);
        let mut text = format!("{}, {title}, {phrase}. ", cr(who));
        text.push_str(&format!("It lived {}", names::span(life.age as f64)));
        let mut deeds = Vec::new();
        if life.offspring > 0 {
            deeds.push(format!("had {} offspring", life.offspring));
        }
        if life.kills > 0 {
            deeds.push(format!("killed {}", life.kills));
        }
        if life.is_leader
            && let Some(g) = life.group
        {
            deeds.push(format!("led {}", gr(g)));
        }
        match deeds.len() {
            0 => text.push('.'),
            1 => text.push_str(&format!(" and {}.", deeds[0])),
            _ => {
                let last = deeds.pop().unwrap();
                text.push_str(&format!(", {} and {last}.", deeds.join(", ")));
            }
        }
        let weight = match score {
            s if s >= 50.0 => 3,
            s if s >= 40.0 => 2,
            _ => 1,
        };
        let mut refs = vec![Ref::Creature(who), Ref::Species(species)];
        if let Some(other) = other_party(cause)
            && self.names.contains_key(&other)
        {
            refs.push(Ref::Creature(other));
        }
        self.write(Kind::Death, weight, format!("Death of {name}"), text, refs);
        self.trim_notables();
    }

    fn promote(&mut self, id: CreatureId, species: SpeciesId, life: &Life, score: f32) {
        let mut name = names::creature_name(id.0);
        let mut n = 0u64;
        while self.notables.values().any(|x| x.name == name) {
            n += 1;
            name = names::creature_name(id.0 ^ (n << 40));
        }
        self.names.insert(id, name.clone());
        self.notables.insert(
            id,
            Notable {
                id,
                species,
                name,
                title: title_for(life, ""),
                life: life.clone(),
                score,
                first_seen: self.now,
                died: None,
                cause: None,
            },
        );
    }

    /// The bar a death must clear now: raised by each death written, easing back.
    fn death_bar(&self) -> f32 {
        let (bar, set) = self.death_bar;
        let above = (bar - NOTABLE_DEATH).max(0.0);
        NOTABLE_DEATH + above * (-(self.now - set) / DEATH_BAR_EASE).exp() as f32
    }

    fn trim_notables(&mut self) {
        while self.notables.len() > MAX_NOTABLES {
            let worst = self.notables.values().filter(|n| n.died.is_some()).min_by(|a, b| a.score.total_cmp(&b.score));
            match worst {
                Some(w) => {
                    let id = w.id;
                    self.notables.remove(&id);
                    if !self.cited.contains(&Ref::Creature(id)) {
                        self.names.remove(&id);
                    }
                }
                None => break,
            }
        }
    }

    fn group_formed(&mut self, id: GroupId, species: SpeciesId, parent: Option<GroupId>) {
        let traits = self.species.get(&species).map(|s| &s.traits);
        let mut name = names::group_name(id.0, traits);
        let mut n = 0u64;
        while self.groups.values().any(|g| g.name == name) {
            n += 1;
            name = names::group_name(id.0 ^ (n << 36), traits);
        }
        let zone = traits.map_or(0.5, |t| t.zone) * names::WORLD_W;
        let (dialect, signals) =
            parent.and_then(|p| self.groups.get(&p)).map_or((0, [0; 4]), |p| (p.dialect, p.signals));
        self.groups.insert(
            id,
            Group {
                id,
                species,
                parent,
                name,
                formed: self.now,
                gone: None,
                members: 0,
                peak: 0,
                x: [zone, zone],
                leader: None,
                dialect,
                signals,
                notable: false,
                kills: 0,
                losses: 0,
                own_dialect: false,
            },
        );
    }

    fn group_gone(&mut self, id: GroupId) {
        let now = self.now;
        let Some(g) = self.groups.get_mut(&id) else { return };
        if g.gone.is_some() {
            return;
        }
        g.gone = Some(now);
        let (notable, peak, formed, species) = (g.notable, g.peak, g.formed, g.species);
        let at_war = self.wars.iter().any(|w| w.ended.is_none() && (w.a == id || w.b == id));
        if !notable && !at_war {
            return;
        }
        // A group that dies during a war was destroyed by it.
        let war = self.wars.iter().position(|w| w.ended.is_none() && (w.a == id || w.b == id));
        if let Some(wi) = war {
            let w = &mut self.wars[wi];
            let victor = if w.a == id { w.b } else { w.a };
            w.ended = Some(now);
            w.outcome = Some(format!("{} was wiped out by {}", gr(id), gr(victor)));
            let text = format!(
                "{} is no more, destroyed by {} in {}. It had stood for {} and once numbered {peak}.",
                gr(id),
                gr(victor),
                wr(wi),
                names::span(now - formed)
            );
            let title = format!("Fall of {}", self.groups[&id].name);
            self.write(Kind::War, 3, title, text, vec![Ref::Group(id), Ref::Group(victor), Ref::War(wi)]);
        } else if notable {
            let text = format!(
                "{} of the {} scattered after {}. At its height it numbered {peak}.",
                gr(id),
                sps(species),
                names::span(now - formed)
            );
            let title = format!("End of {}", self.groups[&id].name);
            self.write(Kind::Society, 1, title, text, vec![Ref::Group(id), Ref::Species(species)]);
        }
    }

    fn clash(&mut self, attacker: GroupId, defender: GroupId, deaths: u32, x: f32) {
        let now = self.now;
        if let Some(g) = self.groups.get_mut(&attacker) {
            g.kills += deaths;
        }
        if let Some(g) = self.groups.get_mut(&defender) {
            g.losses += deaths;
        }
        // Only groups the chronicle knows can go to war.
        if !self.groups.contains_key(&attacker) || !self.groups.contains_key(&defender) {
            return;
        }
        let pair =
            |w_a: GroupId, w_b: GroupId| (w_a == attacker && w_b == defender) || (w_a == defender && w_b == attacker);
        if let Some(wi) = self.wars.iter().position(|w| w.ended.is_none() && pair(w.a, w.b)) {
            let w = &mut self.wars[wi];
            w.deaths += deaths;
            w.clashes += 1;
            w.last_clash = now;
            if deaths >= 5 {
                let text = format!(
                    "{} fell on {} in {}. {deaths} died. The war has now cost {} lives.",
                    gr(attacker),
                    gr(defender),
                    names::place(x),
                    self.wars[wi].deaths
                );
                self.write(
                    Kind::War,
                    2,
                    format!("Battle in {}", names::place(x)),
                    text,
                    vec![Ref::War(wi), Ref::Group(attacker), Ref::Group(defender)],
                );
            }
            return;
        }
        self.feuds.retain(|f| now - f.first <= FEUD_WINDOW);
        let fi = match self.feuds.iter().position(|f| pair(f.a, f.b)) {
            Some(i) => i,
            None => {
                self.feuds.push(Feud { a: attacker, b: defender, first: now, deaths: 0, x });
                self.feuds.len() - 1
            }
        };
        self.feuds[fi].deaths += deaths;
        if self.feuds[fi].deaths < WAR_DEATHS {
            return;
        }
        let f = self.feuds.remove(fi);
        let x = f.x;
        let name = format!(
            "the {} {}",
            names::capital(names::pick(
                mix(f.a.0 ^ f.b.0),
                &["Ash", "Hollow", "Red", "Glass", "Long", "Silent", "Shoal", "Ember", "Vent", "Ice", "Dusk"]
            )),
            names::pick(mix(f.b.0), &["War", "War", "Feud", "Slaughter", "Raids"])
        );
        for g in [f.a, f.b] {
            if let Some(g) = self.groups.get_mut(&g) {
                g.notable = true;
            }
        }
        let same_species = self.groups.get(&f.a).map(|g| g.species) == self.groups.get(&f.b).map(|g| g.species);
        let kin = match (self.groups.get(&f.a), self.groups.get(&f.b)) {
            (Some(a), Some(b)) => {
                a.parent == Some(b.id) || b.parent == Some(a.id) || (a.parent.is_some() && a.parent == b.parent)
            }
            _ => false,
        };
        self.wars.push(War {
            a: f.a,
            b: f.b,
            name: name.clone(),
            began: f.first,
            last_clash: now,
            ended: None,
            deaths: f.deaths,
            clashes: 1,
            outcome: None,
            x,
        });
        let wi = self.wars.len() - 1;
        let mut text =
            format!("{} and {} are at war in {}. {} have died already.", gr(f.a), gr(f.b), names::place(x), f.deaths);
        if kin {
            text.push_str(" They were one people once.");
        } else if !same_species {
            text.push_str(" It is a war between species.");
        }
        self.write(
            Kind::War,
            3,
            format!("{} begins", names::capital(&name)),
            text,
            vec![Ref::War(wi), Ref::Group(f.a), Ref::Group(f.b)],
        );
    }

    fn nature(&mut self, event: NatureKind, x: f32, strength: f32) {
        if strength < 0.25 {
            return;
        }
        let last = self.weather.iter().find(|w| w.0 == event).map_or(f64::MIN, |w| w.1);
        let quiet = if event == NatureKind::Lightning { NATURE_QUIET * 4.0 } else { NATURE_QUIET };
        if self.now - last < quiet {
            return;
        }
        self.weather.retain(|w| w.0 != event);
        self.weather.push((event, self.now));
        let region = names::place(x);
        let seed = mix(self.now.to_bits());
        let (title, text) = match event {
            NatureKind::Eruption => (
                format!("Eruption in {region}"),
                names::pick(
                    seed,
                    &[
                        "A vent field tore open. Lava ran downslope and ash fell for hours.",
                        "The mountain woke. Burning rock and ash buried the slopes around it.",
                        "An eruption lit the sky; whatever could not flee the lava did not.",
                    ],
                )
                .to_string(),
            ),
            NatureKind::Flood => (
                format!("Flood in {region}"),
                "Meltwater broke loose from the ice and swept across the low ground.".into(),
            ),
            NatureKind::RiverShift => (
                format!("The river moves in {region}"),
                "The river cut itself a new course, draining old pools.".into(),
            ),
            NatureKind::Storm => (
                format!("Great storm over {region}"),
                "A storm wall rolled through, with rain, wind and lightning strikes.".into(),
            ),
            NatureKind::ColdFall => (
                format!("Cold fall in {region}"),
                "Frozen air spilled down from the night side and caught the slow and the small.".into(),
            ),
            NatureKind::DustStorm => (
                format!("Dust storm in {region}"),
                "Scalding dust blew in off the day side and blotted out the light.".into(),
            ),
            NatureKind::Lightning => (
                format!("Lightning in {region}"),
                "Lightning walked across the ground, setting the air glowing.".into(),
            ),
            NatureKind::Hailstorm => (
                format!("Hail over {region}"),
                names::pick(
                    seed,
                    &[
                        "A storm tower rose so high its rain froze, and hail stripped the leaves and battered the backs of everything caught in the open.",
                        "Hail fell out of a towering storm, shredding the plants and bruising whatever could not find cover.",
                    ],
                )
                .to_string(),
            ),
            NatureKind::Drought => (
                format!("Drought across {region}"),
                "No rain had fallen for hours on end. The ground cracked, leaves curled, and the thirsty began to gather at the river.".into(),
            ),
            NatureKind::Rains => (
                format!("The rains return to {region}"),
                "Rain fell at last on the parched ground. Buried seeds woke, and the land began to green.".into(),
            ),
            NatureKind::Heatwave => (
                format!("Heat wave over {region}"),
                "The air over the twilight grew hotter than anyone living had known it, and stayed that way.".into(),
            ),
            NatureKind::Blizzard => (
                format!("Blizzard in {region}"),
                "Snow drove in sideways on a freezing wind and buried the ground. Whatever was caught out in it had to shelter or freeze.".into(),
            ),
            NatureKind::AshWinter => (
                "Ash winter".to_string(),
                "The eruption's ash hung high over the whole world. The light dimmed, the air cooled, and the cold would last for days.".into(),
            ),
        };
        let weight = if event == NatureKind::Lightning {
            1
        } else if strength > 0.75 {
            3
        } else if strength > 0.45 {
            2
        } else {
            1
        };
        self.write(Kind::Nature, weight, title, text, vec![]);
    }

    fn first(&mut self, behavior: Behavior, species: SpeciesId, who: Option<CreatureId>) {
        if self.firsts.contains(&behavior) {
            return;
        }
        self.firsts.push(behavior);
        let actor = match who {
            Some(c) if self.names.contains_key(&c) => cr(c),
            _ => asp(species),
        };
        let line = match behavior {
            Behavior::Hunting => format!("For the first time, {actor} hunted down and ate another creature."),
            Behavior::Scavenging => {
                format!("{actor} fed on a corpse it did not kill. Death now feeds the living directly.")
            }
            Behavior::Cannibalism => {
                format!("{actor} killed and ate one of its own kind. The first cannibal on Vesper.")
            }
            Behavior::Infanticide => format!("{actor} killed a rival's young. It will not be the last."),
            Behavior::Parasitism => {
                format!("Something now lives inside {}, feeding on them from within.", sps(species))
            }
            Behavior::Symbiosis => {
                format!("{} took in a partner that lives inside them and feeds them in return.", sps(species))
            }
            Behavior::Signaling => format!("{} began flashing signals to each other in light.", sps(species)),
            Behavior::Dialect => {
                format!("Two groups of {} no longer read each other's light. Their language has split.", sps(species))
            }
            Behavior::Deception => format!("{actor} flashed a false signal, and ate what came to answer it."),
            Behavior::Guarding => format!("{actor} stood guard over its young instead of leaving them to luck."),
            Behavior::FoodSharing => format!("{actor} gave food to another that was not its own young."),
            Behavior::Grooming => format!("{actor} tended another's body, cleaning and soothing it."),
            Behavior::Sacrifice => format!("{actor} died so that its kin could escape."),
            Behavior::Matriphagy => format!("{} gave its own body to its young as their first meal.", asp(species)),
            Behavior::Eusociality => {
                format!("Among the {}, only one breeds now; the rest work and die for it.", sps(species))
            }
            Behavior::SexualReproduction => {
                format!("{} began to court and mate, mixing two lives into each child.", sps(species))
            }
            Behavior::Migration => {
                format!(
                    "{} moved together across the band to new ground, and their young learned the way.",
                    sps(species)
                )
            }
            Behavior::Tradition => {
                format!("A habit of the {} outlived the one who found it, passed from elders to young.", sps(species))
            }
            Behavior::Trapping => {
                format!("{} caught and digested a living creature. The plants hunt now.", asp(species))
            }
            Behavior::War => format!("Two groups of {} fought as groups, for ground.", sps(species)),
            Behavior::Overthrow => format!("{actor} fought its group's leader and took its place."),
        };
        let mut refs = vec![Ref::Species(species)];
        refs.extend(who.filter(|c| self.names.contains_key(c)).map(Ref::Creature));
        self.write(Kind::First, 3, format!("First {}", behavior_word(behavior)), line, refs);
    }

    fn leader_changed(&mut self, group: GroupId, old: Option<CreatureId>, new: CreatureId, overthrown: bool) {
        let Some(g) = self.groups.get_mut(&group) else { return };
        g.leader = Some(new);
        if !g.notable {
            return;
        }
        let name = g.name.clone();
        let named = |c: CreatureId| self.names.contains_key(&c);
        // A dead leader's successor is news only if it's someone.
        if !overthrown && !named(new) {
            return;
        }
        let leader = if named(new) { cr(new) } else { "A newcomer".into() };
        let (title, text, weight) = match old {
            Some(old) if overthrown => (
                format!("Overthrow in {name}"),
                format!(
                    "{leader} fought {} for {} and won.",
                    if named(old) { cr(old) } else { "the old leader".into() },
                    gr(group)
                ),
                if named(new) || named(old) { 2 } else { 1 },
            ),
            Some(old) if named(old) && self.notables.get(&old).is_some_and(|n| n.died.is_none()) => (
                format!("New leader of {name}"),
                format!("{leader} now leads {} in place of {}.", gr(group), cr(old)),
                1,
            ),
            _ => (format!("New leader of {name}"), format!("{leader} now leads {}.", gr(group)), 1),
        };
        let mut refs = vec![Ref::Group(group)];
        refs.extend([Some(new), old].into_iter().flatten().filter(|c| named(*c)).map(Ref::Creature));
        self.write(Kind::Society, weight, title, text, refs);
    }

    fn pathogen_gone(&mut self, id: StrainId) {
        let now = self.now;
        let Some(s) = self.strains.get_mut(&id) else { return };
        s.gone = Some(now);
        s.infected = 0;
        match s.outbreak.take() {
            Some(i) => self.end_outbreak(i, " and is gone from the world"),
            // A strain that never spread far is forgotten.
            None if !self.cited.contains(&Ref::Strain(id)) && self.outbreaks.iter().all(|o| o.strain != id) => {
                self.strains.remove(&id);
            }
            None => {}
        }
    }

    fn end_outbreak(&mut self, i: usize, how: &str) {
        let now = self.now;
        let o = &mut self.outbreaks[i];
        o.ended = Some(now);
        let (strain, host, deaths, peak, began) = (o.strain, o.host, o.deaths, o.peak, o.began);
        let name = self.strains.get(&strain).map_or("the sickness".into(), |s| s.name.clone());
        let text = format!(
            "{} has burned itself out among the {}{how}. It lasted {}; {peak} were sick at its height and {deaths} died.",
            st(strain),
            sps(host),
            names::span(now - began)
        );
        let weight = if deaths >= 50 { 3 } else { 2 };
        self.write(
            Kind::Plague,
            weight,
            format!("{} ends", names::capital(&name)),
            text,
            vec![Ref::Strain(strain), Ref::Species(host)],
        );
    }

    fn census(&mut self, c: &Census) {
        let now = self.now;
        self.population.push(now, c.creatures as f32);
        self.plants.push(now, c.plants as f32);
        self.matter = [c.living_matter, c.dead_matter, c.soil_matter];

        let mut famines = Vec::new();
        for sc in &c.species {
            let Some(s) = self.species.get_mut(&sc.id) else { continue };
            if !s.alive() {
                continue;
            }
            s.count = sc.count;
            s.peak = s.peak.max(sc.count);
            s.biomass = sc.biomass;
            s.x = sc.mean_x;
            s.traits = sc.traits.clone();
            s.population.push(now, sc.count as f32);
            s.recent.push_back(sc.count);
            if s.recent.len() > FAMINE_WINDOW {
                s.recent.pop_front();
            }
            // A famine is a big fall in numbers, mostly from hunger.
            let high = s.recent.iter().copied().max().unwrap_or(0);
            if s.recent.len() == FAMINE_WINDOW && now >= s.famine_until && high >= 20 {
                let fell = high - sc.count;
                if fell as f32 >= high as f32 * 0.3 && s.starved as f32 >= fell as f32 * 0.5 {
                    famines.push((sc.id, high, sc.count, s.starved, sc.mean_x));
                    s.famine_until = now + 4.0 * 3600.0;
                }
            }
            if s.recent.len() == FAMINE_WINDOW {
                s.starved = s.starved.saturating_sub(s.starved / FAMINE_WINDOW as u32 + 1);
            }
        }
        // Groups that scattered unremarked are forgotten, once the clashes
        // reported as they broke up are in.
        let cited = &self.cited;
        let wars = &self.wars;
        let parents: BTreeSet<GroupId> = self.groups.values().filter(|g| g.notable).filter_map(|g| g.parent).collect();
        self.groups.retain(|id, g| {
            g.notable
                || parents.contains(id)
                || g.gone.is_none_or(|t| now - t < GROUP_FORGET)
                || cited.contains(&Ref::Group(*id))
                || wars.iter().any(|w| w.a == *id || w.b == *id)
        });
        let groups = &self.groups;
        self.feuds.retain(|f| groups.contains_key(&f.a) && groups.contains_key(&f.b));
        let ready: Vec<SpeciesId> = self
            .species
            .values()
            .filter(|s| {
                !s.announced
                    && s.alive()
                    && s.count > 0
                    && (s.count >= SPECIES_NOTABLE || now - s.arose >= SPECIES_SETTLE)
            })
            .map(|s| s.id)
            .collect();
        for id in ready {
            self.announce(id);
        }
        for (id, high, now_count, starved, x) in famines {
            let text = format!(
                "Hunger came to the {} in {}. They fell from {high} to {now_count}; {starved} starved.",
                sps(id),
                names::place(x)
            );
            let weight = if now_count * 3 < high { 3 } else { 2 };
            let title = format!("Famine among the {}", plural(&self.common(id)));
            self.write(Kind::Famine, weight, title, text, vec![Ref::Species(id)]);
        }

        for gc in &c.groups {
            let parent = self.groups.get(&gc.id).and_then(|g| g.parent).and_then(|p| self.groups.get(&p));
            let parent_dialect = parent.map(|p| p.dialect);
            // A parent long scattered and forgotten isn't worth naming.
            let parent = parent.map(|p| p.id);
            let Some(g) = self.groups.get_mut(&gc.id) else { continue };
            g.members = gc.members;
            g.peak = g.peak.max(gc.members);
            g.x = gc.x_range;
            g.leader = gc.leader.or(g.leader);
            g.dialect = gc.dialect;
            g.signals = gc.signals;
            let mut news = Vec::new();
            if !g.notable && gc.members >= GROUP_NOTABLE && now - g.formed >= GROUP_SETTLE {
                g.notable = true;
                news.push(match parent {
                    Some(p) => (
                        format!("{} breaks away", names::capital(&g.name)),
                        format!("{} broke away from {} and went its own way.", gr(g.id), gr(p)),
                        vec![Ref::Group(g.id), Ref::Group(p)],
                    ),
                    None => (
                        format!("Rise of {}", g.name),
                        format!("{} has grown into a people of its own among the {}.", gr(g.id), sps(g.species)),
                        vec![Ref::Group(g.id), Ref::Species(g.species)],
                    ),
                });
            }
            if g.notable
                && !g.own_dialect
                && let Some(p) = g.parent
                && parent_dialect.is_some_and(|d| d != gc.dialect)
            {
                g.own_dialect = true;
                news.push((
                    format!("{} speaks its own tongue", names::capital(&g.name)),
                    format!(
                        "The light-signals of {} have drifted so far that {} can no longer read them.",
                        gr(g.id),
                        gr(p)
                    ),
                    vec![Ref::Group(g.id), Ref::Group(p)],
                ));
            }
            for (title, text, refs) in news {
                self.write(Kind::Society, 1, title, text, refs);
            }
        }

        for sc in &c.strains {
            let Some(s) = self.strains.get_mut(&sc.id) else { continue };
            s.infected = sc.infected;
            s.peak = s.peak.max(sc.infected);
            s.traits = sc.traits.clone();
            let host = s.host;
            let harmful = s.traits.virulence > 0.0;
            let host_count = self.species.get(&host).map_or(0, |h| h.count).max(1);
            let share = sc.infected as f32 / host_count as f32;
            match s.outbreak {
                None if share >= OUTBREAK_START && sc.infected >= 8 => {
                    self.outbreaks.push(Outbreak {
                        strain: sc.id,
                        host,
                        began: now,
                        ended: None,
                        peak: sc.infected,
                        deaths: 0,
                    });
                    s.outbreak = Some(self.outbreaks.len() - 1);
                    let name = s.name.clone();
                    let (title, text, weight) = if harmful {
                        (
                            format!("Outbreak of {name}"),
                            format!(
                                "{} is spreading through the {}. {} of {host_count} are sick.",
                                st(sc.id),
                                sps(host),
                                sc.infected
                            ),
                            2,
                        )
                    } else {
                        (
                            format!("{} spreads", names::capital(&name)),
                            format!(
                                "{} now lives inside {} of {host_count} {}, and they are better fed for it.",
                                st(sc.id),
                                sc.infected,
                                sps(host)
                            ),
                            1,
                        )
                    };
                    self.write(Kind::Plague, weight, title, text, vec![Ref::Strain(sc.id), Ref::Species(host)]);
                }
                Some(i) => {
                    let o = &mut self.outbreaks[i];
                    o.peak = o.peak.max(sc.infected);
                    if share < OUTBREAK_END {
                        s.outbreak = None;
                        if harmful {
                            self.end_outbreak(i, "");
                        } else {
                            self.outbreaks[i].ended = Some(now);
                        }
                    }
                }
                None => {}
            }
        }

        for life in &c.standouts {
            let score = notability(life);
            match self.notables.get_mut(&life.id) {
                Some(n) => {
                    n.title = title_for(life, &n.title);
                    n.life = life.clone();
                    n.score = n.score.max(score);
                }
                None if score >= NOTABLE => self.promote(life.id, life.species, life, score),
                None => {}
            }
        }
        self.trim_notables();

        // Wars end in quiet.
        for i in 0..self.wars.len() {
            let w = &self.wars[i];
            if w.ended.is_none() && now - w.last_clash > PEACE_AFTER {
                let text = format!(
                    "The fighting between {} and {} has stopped. {} lasted {} and cost {} lives.",
                    gr(w.a),
                    gr(w.b),
                    wr(i),
                    names::span(w.last_clash - w.began),
                    w.deaths
                );
                let refs = vec![Ref::War(i), Ref::Group(w.a), Ref::Group(w.b)];
                let title = format!("{} ends", names::capital(&w.name));
                let w = &mut self.wars[i];
                w.ended = Some(w.last_clash);
                w.outcome = Some("an uneasy peace".into());
                self.write(Kind::War, 2, title, text, refs);
            }
        }

        self.track_era(c);
    }

    /// An era is named for the motile species with the most living biomass.
    fn track_era(&mut self, c: &Census) {
        let now = self.now;
        let top = c
            .species
            .iter()
            .filter(|s| s.traits.kingdom == Kingdom::Motile && s.count > 0)
            .max_by(|a, b| a.biomass.total_cmp(&b.biomass))
            .map(|s| s.id);
        let Some(top) = top else { return };
        let current = self.eras.last().filter(|e| e.ended.is_none()).map(|e| e.species);
        if current == Some(top) {
            self.era_rival = None;
            return;
        }
        let since = match self.era_rival {
            Some((s, t)) if s == top => t,
            _ => {
                self.era_rival = Some((top, now));
                now
            }
        };
        // The first era starts at once; later ones must hold the lead.
        if current.is_some() && now - since < ERA_HOLD {
            return;
        }
        if let Some(e) = self.eras.last_mut()
            && e.ended.is_none()
        {
            e.ended = Some(since);
        }
        // After an era that ended with its species, the next begins no earlier.
        let began = self.eras.last().and_then(|e| e.ended).map_or(since, |end| since.max(end));
        self.eras.push(Era { species: top, began, ended: None });
        self.era_rival = None;
        let name = plural(&self.common(top));
        let text = match current {
            Some(prev) => format!(
                "The {} now outweigh every other moving thing on Vesper, taking the lead from the {}.",
                sps(top),
                sps(prev)
            ),
            None => format!("The {} are the most abundant moving life on Vesper.", sps(top)),
        };
        let mut refs = vec![Ref::Species(top)];
        refs.extend(current.map(Ref::Species));
        self.write(Kind::Era, 2, format!("The Age of the {}", names::title_case(&name)), text, refs);
    }

    /// Splits entry text into plain runs and named links.
    pub fn pieces(&self, text: &str) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else { break };
            let token = &rest[start + 1..start + len];
            let parsed = token.split_once(':').and_then(|(k, v)| v.parse::<u64>().ok().map(|v| (k, v)));
            let Some((k, v)) = parsed else {
                out.push(Piece::Text(rest[..start + len + 1].to_string()));
                rest = &rest[start + len + 1..];
                continue;
            };
            if start > 0 {
                out.push(Piece::Text(rest[..start].to_string()));
            }
            let piece = match k {
                "s" => Piece::Link(Ref::Species(SpeciesId(v)), self.common(SpeciesId(v))),
                "a" => {
                    let name = self.common(SpeciesId(v));
                    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
                    out.push(Piece::Text(format!("{article} ")));
                    Piece::Link(Ref::Species(SpeciesId(v)), name)
                }
                "S" => Piece::Link(Ref::Species(SpeciesId(v)), plural(&self.common(SpeciesId(v)))),
                "c" => Piece::Link(Ref::Creature(CreatureId(v)), self.creature_name(CreatureId(v))),
                "g" => Piece::Link(
                    Ref::Group(GroupId(v)),
                    self.groups.get(&GroupId(v)).map_or("a group".into(), |g| g.name.clone()),
                ),
                "p" => Piece::Link(
                    Ref::Strain(StrainId(v)),
                    self.strains.get(&StrainId(v)).map_or("a sickness".into(), |s| s.name.clone()),
                ),
                "w" => Piece::Link(
                    Ref::War(v as usize),
                    self.wars.get(v as usize).map_or("the war".into(), |w| w.name.clone()),
                ),
                _ => Piece::Text(token.to_string()),
            };
            out.push(piece);
            rest = &rest[start + len + 1..];
        }
        if !rest.is_empty() {
            out.push(Piece::Text(rest.to_string()));
        }
        // Capitalise whatever opens a sentence.
        let mut opens = true;
        for p in &mut out {
            let (Piece::Text(s) | Piece::Link(_, s)) = p;
            if opens {
                *s = names::capital(s);
            }
            let end = s.trim_end();
            opens = end.ends_with(['.', '!', '?']) || (opens && end.is_empty());
        }
        out
    }

    /// Entry text with links replaced by names.
    pub fn plain(&self, text: &str) -> String {
        self.pieces(text)
            .into_iter()
            .map(|p| match p {
                Piece::Text(t) | Piece::Link(_, t) => t,
            })
            .collect()
    }

    pub fn creature_name(&self, id: CreatureId) -> String {
        self.names.get(&id).cloned().unwrap_or_else(|| "an unnamed creature".into())
    }

    /// The species' ancestors, oldest first.
    pub fn lineage(&self, id: SpeciesId) -> Vec<SpeciesId> {
        let mut out = Vec::new();
        let mut cur = self.species.get(&id).and_then(|s| s.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.species.get(&p).and_then(|s| s.parent);
            if out.len() > 10_000 {
                break;
            }
        }
        out.reverse();
        out
    }

    pub fn living_species(&self) -> usize {
        self.species.values().filter(|s| s.alive()).count()
    }

    /// The era running now.
    pub fn era(&self) -> Option<&Era> {
        self.eras.last().filter(|e| e.ended.is_none())
    }

    /// Entries that mention `r`, newest first.
    pub fn entries_about(&self, r: Ref) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev().filter(move |e| e.refs.contains(&r))
    }

    /// A biography for a notable creature.
    pub fn biography(&self, n: &Notable) -> String {
        let l = &n.life;
        let mut s = asp(n.species);
        if l.generation > 0 {
            s.push_str(&format!(" of the {} generation", ordinal(l.generation)));
        }
        if let Some(p) = l.parent.filter(|p| self.names.contains_key(p)) {
            s.push_str(&format!(", child of {}", cr(p)));
        }
        s.push_str(". ");
        if let Some(g) = l.group {
            if l.is_leader {
                s.push_str(&format!("It led {}. ", gr(g)));
            } else {
                s.push_str(&format!("It belonged to {}. ", gr(g)));
            }
        }
        let mut deeds = Vec::new();
        if l.kills > 0 {
            deeds.push(format!("killed {}", l.kills));
        }
        if l.offspring > 0 {
            deeds.push(format!("had {} offspring", l.offspring));
        }
        if l.scars > 0 {
            deeds.push(format!("survived losing {} limbs or segments", l.scars));
        }
        if l.kindness > 0 {
            deeds.push(format!("fed or healed others {} times", l.kindness));
        }
        if !deeds.is_empty() {
            let last = deeds.pop().unwrap();
            if deeds.is_empty() {
                s.push_str(&format!("It {last}"));
            } else {
                s.push_str(&format!("It {} and {last}", deeds.join(", ")));
            }
            s.push_str(". ");
        }
        match (n.died, n.cause) {
            (Some(_), Some(cause)) => s.push_str(&format!(
                "It {} at the age of {}.",
                self.cause_phrase(n.id, &cause, l.x),
                names::span(l.age as f64)
            )),
            _ => s.push_str(&format!("Alive at {}, in {}.", names::span(l.age as f64), names::place(l.x))),
        }
        s
    }
}

/// Routine deaths counted together: how many of each species died of each
/// cause, and the last of them per species, which an extinction may name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tally {
    /// Causes with the creatures in them left out; counts don't need them.
    pub counts: Vec<(SpeciesId, DeathCause, u32)>,
    pub last: Vec<(SpeciesId, CreatureId, DeathCause, f32)>,
}

impl Tally {
    pub fn add(&mut self, who: CreatureId, species: SpeciesId, cause: DeathCause, x: f32) {
        let key = faceless(cause);
        match self.counts.iter_mut().find(|(s, c, _)| *s == species && *c == key) {
            Some(row) => row.2 += 1,
            None => self.counts.push((species, key, 1)),
        }
        match self.last.iter_mut().find(|l| l.0 == species) {
            Some(l) => *l = (species, who, cause, x),
            None => self.last.push((species, who, cause, x)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    pub fn deaths(&self) -> u32 {
        self.counts.iter().map(|c| c.2).sum()
    }
}

/// A cause of death with the individual creatures blanked out.
fn faceless(c: DeathCause) -> DeathCause {
    let nobody = CreatureId(0);
    match c {
        DeathCause::Predation { killer_species, .. } => DeathCause::Predation { killer: nobody, killer_species },
        DeathCause::Cannibalism { .. } => DeathCause::Cannibalism { killer: nobody },
        DeathCause::Infanticide { .. } => DeathCause::Infanticide { killer: nobody },
        DeathCause::Violence { by_group, .. } => DeathCause::Violence { by_group, killer: nobody },
        DeathCause::Sacrifice { .. } => DeathCause::Sacrifice { defending: nobody },
        DeathCause::Devoured { .. } => DeathCause::Devoured { by_offspring: nobody },
        other => other,
    }
}

/// The other creature named in a cause of death, if any.
fn other_party(c: &DeathCause) -> Option<CreatureId> {
    match *c {
        DeathCause::Predation { killer, .. }
        | DeathCause::Cannibalism { killer }
        | DeathCause::Infanticide { killer }
        | DeathCause::Violence { killer, .. } => Some(killer),
        DeathCause::Sacrifice { defending } => Some(defending),
        DeathCause::Devoured { by_offspring } => Some(by_offspring),
        _ => None,
    }
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, x) if x != 11 => "st",
        (2, x) if x != 12 => "nd",
        (3, x) if x != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn behavior_word(b: Behavior) -> &'static str {
    match b {
        Behavior::Hunting => "hunt",
        Behavior::Scavenging => "scavenger",
        Behavior::Cannibalism => "cannibal",
        Behavior::Infanticide => "infanticide",
        Behavior::Parasitism => "parasite",
        Behavior::Symbiosis => "partnership",
        Behavior::Signaling => "signals",
        Behavior::Dialect => "split tongue",
        Behavior::Deception => "lie",
        Behavior::Guarding => "guardian",
        Behavior::FoodSharing => "gift of food",
        Behavior::Grooming => "grooming",
        Behavior::Sacrifice => "sacrifice",
        Behavior::Matriphagy => "parent eaten by its young",
        Behavior::Eusociality => "colony",
        Behavior::SexualReproduction => "mating",
        Behavior::Migration => "migration",
        Behavior::Tradition => "tradition",
        Behavior::Trapping => "hunting plant",
        Behavior::War => "war",
        Behavior::Overthrow => "overthrow",
    }
}

/// The title a life has earned. Keeps `old` unless something now stands out more.
fn title_for(l: &Life, old: &str) -> String {
    let mut options: Vec<(f32, &str)> = vec![
        (l.kills as f32 / 25.0, if l.kills >= 100 { "the Devourer" } else { "the Hunter" }),
        (l.offspring as f32 / 10.0, if l.offspring >= 40 { "Root of Many" } else { "the Prolific" }),
        (l.scars as f32 / 3.0, "the Scarred"),
        (l.kindness as f32 / 2500.0, "the Generous"),
        (l.rank / 2.0, "the Dominant"),
        (l.age / 1200.0, "the Old"),
    ];
    if l.is_leader {
        options.push((2.0, "the Leader"));
    }
    options.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (v, best) = options[0];
    if v >= 1.0 || old.is_empty() { best.to_string() } else { old.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{blank_life, seed_traits};

    fn traits(zone: f32) -> Traits {
        Traits { zone, ..seed_traits(1) }
    }

    fn census() -> Census {
        Census {
            creatures: 0,
            plants: 0,
            living_matter: 0.0,
            dead_matter: 0.0,
            soil_matter: 0.0,
            species: vec![],
            groups: vec![],
            strains: vec![],
            standouts: vec![],
        }
    }

    #[test]
    fn series_stays_small_and_keeps_the_latest() {
        let mut s = Series::default();
        for i in 0..100_000 {
            s.push(i as f64 * 60.0, i as f32);
        }
        assert!(s.points.len() <= SERIES_MAX);
        assert_eq!(s.last(), 99_999.0);
        assert!(s.at(30.0 * 60.0 * 1000.0) > 0.0);
    }

    #[test]
    fn extinction_names_the_last_death() {
        let mut h = History::default();
        h.apply(0.0, 1.0, &Fact::SpeciesArose { id: SpeciesId(1), parent: None, traits: traits(0.5) });
        h.apply(10.0, 2.0, &Fact::SpeciesArose { id: SpeciesId(2), parent: Some(SpeciesId(1)), traits: traits(0.9) });
        let count = |n| SpeciesCensus { id: SpeciesId(2), count: n, biomass: 1.0, mean_x: 1.0, traits: traits(0.9) };
        h.apply(15.0, 2.5, &Fact::Census(Census { species: vec![count(25)], ..census() }));
        assert!(h.entries.iter().any(|e| e.kind == Kind::Origin && e.refs[0] == Ref::Species(SpeciesId(2))));
        h.apply(
            20.0,
            3.0,
            &Fact::Death {
                who: CreatureId(9),
                species: SpeciesId(2),
                cause: DeathCause::Predation { killer: CreatureId(4), killer_species: SpeciesId(1) },
                life: blank_life(CreatureId(9), SpeciesId(2)),
            },
        );
        h.apply(20.0, 3.0, &Fact::SpeciesExtinct { id: SpeciesId(2) });
        let e = h.entries.last().unwrap();
        assert_eq!(e.kind, Kind::Extinction);
        let text = h.plain(&e.text);
        assert!(text.contains("torn apart"), "{text}");
        assert_eq!(h.species[&SpeciesId(1)].prey[&SpeciesId(2)], 1);
        assert_eq!(h.lineage(SpeciesId(2)), vec![SpeciesId(1)]);
    }

    #[test]
    fn branches_that_die_at_once_go_untold() {
        let mut h = History::default();
        h.apply(0.0, 1.0, &Fact::SpeciesArose { id: SpeciesId(1), parent: None, traits: traits(0.5) });
        let told = h.entries.len();
        h.apply(10.0, 2.0, &Fact::SpeciesArose { id: SpeciesId(2), parent: Some(SpeciesId(1)), traits: traits(0.9) });
        h.apply(30.0, 3.0, &Fact::SpeciesExtinct { id: SpeciesId(2) });
        assert_eq!(h.entries.len(), told);
        assert!(!h.species.contains_key(&SpeciesId(2)));
        assert!(h.species[&SpeciesId(1)].children.is_empty());
    }

    #[test]
    fn repeated_clashes_become_a_war_and_quiet_ends_it() {
        let mut h = History::default();
        h.apply(0.0, 0.0, &Fact::SpeciesArose { id: SpeciesId(1), parent: None, traits: traits(0.5) });
        h.apply(0.0, 0.0, &Fact::GroupFormed { id: GroupId(1), species: SpeciesId(1), parent_group: None, members: 5 });
        let split =
            Fact::GroupFormed { id: GroupId(2), species: SpeciesId(1), parent_group: Some(GroupId(1)), members: 5 };
        h.apply(0.0, 0.0, &split);
        for t in 0..5 {
            let clash = Fact::Clash { attacker: GroupId(1), defender: GroupId(2), deaths: 2, x: 0.5 };
            h.apply(t as f64 * 100.0, 0.0, &clash);
        }
        assert_eq!(h.wars.len(), 1);
        assert!(h.plain(&h.entries.last().unwrap().text).contains("one people once"));
        h.apply(10_000.0, 0.0, &Fact::Census(census()));
        assert!(h.wars[0].ended.is_some());
    }

    #[test]
    fn pieces_resolve_links() {
        let mut h = History::default();
        h.apply(0.0, 0.0, &Fact::SpeciesArose { id: SpeciesId(3), parent: None, traits: traits(0.5) });
        let p = h.pieces("{S:3} rule. {x}");
        assert!(matches!(&p[0], Piece::Link(Ref::Species(SpeciesId(3)), s) if s.ends_with('s')));
        assert_eq!(h.plain("a {nope}. {s:3}"), "A {nope}. Rust float");
    }

    #[test]
    fn history_round_trips_through_json() {
        let mut h = History::default();
        h.apply(0.0, 0.0, &Fact::SpeciesArose { id: SpeciesId(3), parent: None, traits: traits(0.5) });
        let json = serde_json::to_string(&h).unwrap();
        let back: History = serde_json::from_str(&json).unwrap();
        assert_eq!(back.species.len(), 1);
    }
}
