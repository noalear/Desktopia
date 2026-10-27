//! A copy of `vesper_sim::history` (sim branch at d06685e), which owns these types.
//! When vesper-sim is on main this file becomes `pub use vesper_sim::history::*;`.
//!
//! Facts the simulation reports as they happen, for the chronicle to keep.
//!
//! The simulation only reports; deciding what is notable, naming species and
//! telling the story is the chronicle's job. Ids are never reused within a
//! world, and times are sim seconds since the world was born.

use serde::{Deserialize, Serialize};

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);
    };
}

id!(CreatureId);
id!(PlantId);
id!(SpeciesId);
id!(GroupId);
id!(StrainId);

/// Sim seconds since the world was born.
pub type Tick = f64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kingdom {
    Rooted,
    Motile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locomotion {
    /// Plants fixed in the ground.
    Rooted,
    /// Plants that float free on the wind.
    FloatingMat,
    /// Hangs in the thick air and goes where the wind goes.
    Drift,
    /// Swims through the air with fins or a tail.
    Swim,
    /// Walks the ground on legs.
    Walk,
    /// Drags itself along the ground without legs.
    Crawl,
    /// Spends its life down in the soil.
    Burrow,
    /// Rides the air on broad membranes.
    Glide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reproduction {
    /// Clones split off the parent's body.
    Budding,
    /// Clouds of tiny untended offspring.
    Spores,
    /// Eggs laid by one parent.
    Eggs,
    /// Two parents court, mate and mix genomes.
    Mating,
    /// Plants: seeds carried by wind, water or animals.
    Seeds,
}

/// What a species is like, averaged over its members.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Traits {
    pub kingdom: Kingdom,
    pub locomotion: Locomotion,
    pub reproduction: Reproduction,
    /// Adult body length in world units (monitor heights).
    pub size: f32,
    /// Top speed in world units per second.
    pub speed: f32,
    /// 0 eats plants (or makes its own food), 1 eats meat.
    pub diet: f32,
    /// 0..1 how much of the body is armor or shell.
    pub armor: f32,
    /// 0..1 spines or thorns.
    pub spines: f32,
    /// 0..1 poison in the flesh.
    pub toxin: f32,
    /// 0..1 how much of its energy comes from light.
    pub photosynthesis: f32,
    /// Hidden neurons in the brain (0 for plants).
    pub neurons: u16,
    /// 0..1 how strongly it seeks out its own kind.
    pub sociality: f32,
    /// 0..1 glow cells.
    pub glow: f32,
    /// Where it lives, 0 day edge .. 1 night edge.
    pub zone: f32,
    /// Typical lifespan in sim seconds.
    pub lifespan: f32,
    /// Offspring per breeding.
    pub clutch: f32,
    /// The four most visible look genes (hue, value, pattern, glow hue).
    pub look: [f32; 4],
    /// Number of body segments (creatures) or branches (plants).
    pub segments: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeathCause {
    Predation {
        killer: CreatureId,
        killer_species: SpeciesId,
    },
    /// Killed and eaten by its own species.
    Cannibalism {
        killer: CreatureId,
    },
    /// Killed young by an adult that wasn't its parent.
    Infanticide {
        killer: CreatureId,
    },
    /// Killed by another group in a fight over ground.
    Violence {
        by_group: GroupId,
        killer: CreatureId,
    },
    /// Died fighting in defence of kin or young.
    Sacrifice {
        defending: CreatureId,
    },
    /// Gave its own body to its young.
    Devoured {
        by_offspring: CreatureId,
    },
    Starvation,
    Disease {
        strain: StrainId,
    },
    /// Burst open by a parasite, or drained by one.
    Parasite {
        strain: StrainId,
    },
    Age,
    Heat,
    Cold,
    /// Caught by a trap plant.
    Trap {
        plant_species: SpeciesId,
    },
    Lava,
    Lightning,
    /// Swept away by a flood.
    Flood,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NatureKind {
    Eruption,
    Flood,
    RiverShift,
    Storm,
    ColdFall,
    DustStorm,
    Lightning,
}

/// A behavior seen for the first time anywhere in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Behavior {
    Hunting,
    Scavenging,
    Cannibalism,
    Infanticide,
    Parasitism,
    Symbiosis,
    Signaling,
    /// Two groups of one species whose light signals no longer match.
    Dialect,
    /// A signal that brought prey to a hunter.
    Deception,
    Guarding,
    FoodSharing,
    Grooming,
    Sacrifice,
    /// A parent eaten by its young.
    Matriphagy,
    /// Sterile helpers raising a single breeder's young.
    Eusociality,
    SexualReproduction,
    /// A seasonal route shared by a group and learned by its young.
    Migration,
    /// A habit learned from elders that outlived the one who found it.
    Tradition,
    /// Plants catching animals.
    Trapping,
    War,
    Overthrow,
}

/// Running record of one creature's life.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Life {
    pub id: CreatureId,
    pub species: SpeciesId,
    pub parent: Option<CreatureId>,
    pub generation: u32,
    pub born: Tick,
    /// Seconds lived.
    pub age: f32,
    pub kills: u32,
    pub offspring: u32,
    /// Matter eaten over its life.
    pub eaten: f32,
    /// Dominance within its species, 0 is average.
    pub rank: f32,
    pub group: Option<GroupId>,
    pub is_leader: bool,
    pub x: f32,
    /// Times it gave food away or healed another.
    pub kindness: u32,
    /// Limbs or segments lost to attacks and survived.
    pub scars: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrainTraits {
    /// 0 a microbe seen as sores or glow, 1 a parasite seen growing inside.
    pub visible: f32,
    /// Chance of passing on contact.
    pub contagion: f32,
    /// Energy taken from the host per second; negative gives energy (symbiosis).
    pub virulence: f32,
    /// 0..1 how hard it steers its host's mind.
    pub manipulation: f32,
    /// 0..1 bursts out of the host when grown.
    pub bursts: f32,
    /// 0..1 provides light-feeding to its host.
    pub photosynthetic: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeciesCensus {
    pub id: SpeciesId,
    pub count: u32,
    /// Living matter across all members.
    pub biomass: f32,
    pub mean_x: f32,
    pub traits: Traits,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupCensus {
    pub id: GroupId,
    pub species: SpeciesId,
    pub members: u32,
    pub x_range: [f32; 2],
    pub leader: Option<CreatureId>,
    /// Identifies the group's light dialect; equal ids understand each other.
    pub dialect: u32,
    /// The group's light signal for danger, food, mating and threat.
    pub signals: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrainCensus {
    pub id: StrainId,
    pub infected: u32,
    pub traits: StrainTraits,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Census {
    pub creatures: u32,
    pub plants: u32,
    /// Matter in living things, remains, soil and air (the last three are where
    /// the rest of the world's fixed matter waits).
    pub living_matter: f32,
    pub dead_matter: f32,
    pub soil_matter: f32,
    pub species: Vec<SpeciesCensus>,
    pub groups: Vec<GroupCensus>,
    pub strains: Vec<StrainCensus>,
    /// Up to twenty living individuals worth following.
    pub standouts: Vec<Life>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fact {
    SpeciesArose {
        id: SpeciesId,
        parent: Option<SpeciesId>,
        traits: Traits,
    },
    SpeciesExtinct {
        id: SpeciesId,
    },
    Death {
        who: CreatureId,
        species: SpeciesId,
        cause: DeathCause,
        life: Life,
    },
    GroupFormed {
        id: GroupId,
        species: SpeciesId,
        parent_group: Option<GroupId>,
        members: u32,
    },
    GroupGone {
        id: GroupId,
    },
    /// Members of one group attacked another's over the last stretch of time.
    Clash {
        attacker: GroupId,
        defender: GroupId,
        deaths: u32,
        x: f32,
    },
    LeaderChanged {
        group: GroupId,
        old: Option<CreatureId>,
        new: CreatureId,
        overthrown: bool,
    },
    PathogenArose {
        id: StrainId,
        parent: Option<StrainId>,
        host: SpeciesId,
        traits: StrainTraits,
    },
    PathogenGone {
        id: StrainId,
    },
    Nature {
        kind: NatureKind,
        x: f32,
        strength: f32,
    },
    First {
        behavior: Behavior,
        species: SpeciesId,
        who: Option<CreatureId>,
    },
    Census(Census),
}
