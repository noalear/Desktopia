//! Organs: what a part does, read from what it is made of and the shape it
//! grows into. Sense tissue swollen into a ball on a stalk is an eye; the
//! same tissue drawn out into a feathery or ringed feeler is a nose; spread
//! flat it is an ear. Mouth tissue is jaws as a plain tube, a beak as a
//! hook, shearing mandibles as a pincer, fangs as a fork, grinding teeth as
//! plates, a rasping tongue as beads, a sucking proboscis as a coil and a
//! sieve as a frond or tuft.
//!
//! So every organ a creature has comes from genes evolution already
//! changes: a mutation in a part's tissue or form gains, loses or swaps an
//! organ, a duplicated part brings a second copy that can drift into
//! something new, and parts that no longer pay for their upkeep shrink and
//! go. The table follows the broad kinds of animal organ on Earth (Kardong,
//! "Vertebrates: Comparative Anatomy"; Ruppert & Barnes, "Invertebrate
//! Zoology"), though the bodies they make are not Earth's.

use crate::genome::{Form, Tissue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Organ {
    // Senses.
    /// A camera eye: sees far, but only by light.
    Eye,
    /// Simple eyes and light-sensitive spots: tell light from shadow and
    /// movement close by.
    Ocellus,
    /// Antennae and nostrils: smell food, carrion and others in any light,
    /// through fog and dust.
    Antenna,
    /// Ears and vibration pads: hear anything that moves, in any light.
    Ear,
    /// Whiskers, palps and feelers: touch and taste close up, so poison
    /// can be tasted before it is swallowed.
    Feeler,
    // Mouths.
    /// Plain jaws with all-purpose teeth.
    Jaw,
    /// A hooked beak: cracks fruit, seed and egg, tears flesh.
    Beak,
    /// Shearing mandibles: cut leaves and flesh alike.
    Mandible,
    /// Fangs: stab and hold prey; a venomous animal injects through them.
    Fang,
    /// Ridged grinding teeth: mill tough leaves and crusts.
    Grinder,
    /// A rasping tongue: scrapes crusts and films off rock.
    Radula,
    /// A sucking tube: drinks nectar, fruit juice or blood.
    Proboscis,
    /// A sieve of plates or bristles: strains drifting food from the air.
    Sieve,
    /// A wide gullet that swallows food whole: eggs, fruit, small animals.
    Gullet,
    /// A shovel bill: digs rotting matter out of the soil.
    Scoop,
    // Limbs, grips and weapons.
    /// A plain walking or swimming limb.
    Limb,
    /// Hooked claws and talons: grip, climb and rake.
    Talon,
    /// Crushing claws.
    Chela,
    /// Digging hands and forks.
    Digger,
    /// Fins and flippers.
    Fin,
    /// A coiling, grasping limb or tail.
    Tendril,
    // Defences.
    /// Spines and quills, which hurt whatever bites.
    Spine,
    /// Barbed spines and stinging hairs, which break off in the wound.
    Barb,
    /// A stinger, which injects the body's poison.
    Stinger,
    /// Horns, tusks and antlers: for fighting rivals and showing off.
    Horn,
    /// Overlapping armour plates.
    Plating,
    /// A shell to pull in behind.
    Shell,
    /// A bony club on a tail or limb.
    Club,
    /// A neck frill or shield: guards and shows off.
    Frill,
    /// Scales: a dry skin that keeps water in and turns small blows.
    Scales,
    // Climate.
    /// Fur and bristles: keep body heat in.
    Fur,
    /// Feathers and plumes: keep heat in, and show off.
    Plume,
    /// Thin fins, sails and big ears full of blood: shed heat.
    Radiator,
    /// Fat under the skin: energy for famine and a coat against cold.
    Fat,
    /// A hump: fat and water stored for drought.
    Hump,
    // Insides.
    /// Gut: digests what the mouth takes in.
    Stomach,
    /// Gills and lungs: breathe hard for long efforts, but clog in ash.
    Gill,
    /// Gas bladders: lift in the thick air.
    Bladder,
    /// Sails of gas-filled skin that glide.
    Sail,
    /// Light-feeding film.
    Film,
    /// Light organs.
    Photophore,
    /// A glowing bulb that lures prey close.
    Lure,
    /// Bright panels and crests: show off to mates and rivals.
    Crest,
    /// Brood chambers.
    Brood,
    /// A pouch the young ride in.
    Pouch,
    /// Plain body.
    Skeleton,
}

pub const ORGANS: usize = 46;

impl Organ {
    pub const ALL: [Organ; ORGANS] = {
        use Organ::*;
        [
            Eye, Ocellus, Antenna, Ear, Feeler, Jaw, Beak, Mandible, Fang, Grinder, Radula, Proboscis, Sieve, Gullet,
            Scoop, Limb, Talon, Chela, Digger, Fin, Tendril, Spine, Barb, Stinger, Horn, Plating, Shell, Club, Frill,
            Scales, Fur, Plume, Radiator, Fat, Hump, Stomach, Gill, Bladder, Sail, Film, Photophore, Lure, Crest,
            Brood, Pouch, Skeleton,
        ]
    };

    pub fn index(self) -> usize {
        self as usize
    }

    /// What a part of this tissue and form works as.
    pub fn of(tissue: Tissue, form: Form) -> Organ {
        use Form as F;
        use Organ::*;
        match tissue {
            Tissue::Sense => match form {
                F::Bulb => Eye,
                F::Tube | F::Plates => Ocellus,
                F::Beads | F::Fork | F::Frond | F::Coil => Antenna,
                F::Paddle | F::Fan => Ear,
                F::Tuft | F::Hook | F::Pincer => Feeler,
            },
            Tissue::Mouth => match form {
                F::Tube => Jaw,
                F::Hook => Beak,
                F::Pincer => Mandible,
                F::Fork => Fang,
                F::Plates => Grinder,
                F::Beads => Radula,
                F::Coil => Proboscis,
                F::Frond | F::Tuft => Sieve,
                F::Bulb => Gullet,
                F::Paddle | F::Fan => Scoop,
            },
            Tissue::Muscle => match form {
                F::Hook => Talon,
                F::Pincer => Chela,
                F::Fork => Digger,
                F::Paddle | F::Fan => Fin,
                F::Coil => Tendril,
                F::Tuft => Fur,
                F::Frond => Plume,
                _ => Limb,
            },
            Tissue::Frame => match form {
                F::Tuft => Fur,
                F::Frond => Plume,
                F::Paddle | F::Fan => Radiator,
                F::Plates => Scales,
                F::Fork => Horn,
                F::Hook => Talon,
                F::Coil => Tendril,
                F::Bulb => Club,
                _ => Skeleton,
            },
            Tissue::Armor => match form {
                F::Coil => Shell,
                F::Bulb => Club,
                F::Paddle | F::Fan => Frill,
                F::Hook | F::Fork => Horn,
                F::Pincer => Chela,
                F::Tuft | F::Beads => Scales,
                _ => Plating,
            },
            Tissue::Spine => match form {
                F::Coil | F::Pincer => Stinger,
                F::Fork => Horn,
                F::Hook | F::Frond | F::Tuft => Barb,
                _ => Spine,
            },
            Tissue::Store => match form {
                F::Bulb => Hump,
                F::Tuft => Fur,
                _ => Fat,
            },
            Tissue::Float => match form {
                F::Fan | F::Paddle | F::Frond => Sail,
                _ => Bladder,
            },
            Tissue::LightFilm => Film,
            Tissue::Gut => match form {
                F::Frond | F::Tuft => Gill,
                _ => Stomach,
            },
            Tissue::Glow => match form {
                F::Bulb => Lure,
                F::Fan | F::Paddle | F::Frond => Crest,
                _ => Photophore,
            },
            Tissue::Brood => match form {
                F::Bulb | F::Paddle => Pouch,
                _ => Brood,
            },
        }
    }

    /// How much more (or less) a part works as its organ than its bare area
    /// says: a feathery antenna or gill has more surface, plates and shells
    /// cover more, a ball eye gathers more light.
    pub fn gain(self) -> f32 {
        use Organ::*;
        match self {
            Eye => 1.4,
            Antenna | Gill => 1.3,
            Plating | Shell => 1.3,
            Horn | Stinger => 1.2,
            _ => 1.0,
        }
    }

    pub fn name(self) -> &'static str {
        use Organ::*;
        match self {
            Eye => "eyes",
            Ocellus => "eyespots",
            Antenna => "antennae",
            Ear => "ears",
            Feeler => "feelers",
            Jaw => "jaws",
            Beak => "a beak",
            Mandible => "mandibles",
            Fang => "fangs",
            Grinder => "grinding teeth",
            Radula => "a rasping tongue",
            Proboscis => "a proboscis",
            Sieve => "a sieve mouth",
            Gullet => "a gaping gullet",
            Scoop => "a shovel bill",
            Limb => "limbs",
            Talon => "talons",
            Chela => "crushing claws",
            Digger => "digging hands",
            Fin => "fins",
            Tendril => "grasping coils",
            Spine => "spines",
            Barb => "barbs",
            Stinger => "a stinger",
            Horn => "horns",
            Plating => "armour plates",
            Shell => "a shell",
            Club => "a club",
            Frill => "a frill",
            Scales => "scales",
            Fur => "fur",
            Plume => "plumes",
            Radiator => "heat fins",
            Fat => "fat",
            Hump => "a hump",
            Stomach => "a gut",
            Gill => "gills",
            Bladder => "gas bladders",
            Sail => "gliding sails",
            Film => "light film",
            Photophore => "light organs",
            Lure => "a lure",
            Crest => "a crest",
            Brood => "brood sacs",
            Pouch => "a pouch",
            Skeleton => "a body",
        }
    }

    /// Organs worth a line in the chronicle when a lineage gains or loses them.
    pub fn notable(self) -> bool {
        !matches!(self, Organ::Skeleton | Organ::Limb | Organ::Stomach | Organ::Brood | Organ::Fat)
    }
}

/// The foods there are, each needing its own kind of mouth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Fare {
    /// Leaves and stems.
    Leaf,
    /// Fruit.
    Fruit,
    /// Nectar in flowers.
    Nectar,
    /// Crusts of vent growth on rock.
    Crust,
    /// Flesh of a live animal.
    Flesh,
    /// Remains.
    Carrion,
    Egg,
    /// Drifting specks in the air.
    Plankton,
    /// Rotting matter in the soil.
    Soil,
}

pub const FARES: usize = 9;

impl Fare {
    pub fn index(self) -> usize {
        self as usize
    }

    /// How long a mouthful takes the gut to deal with, relative to fruit
    /// flesh: leaves are fibre, crusts are grit, nectar is sugar water.
    pub fn toughness(self) -> f32 {
        match self {
            Fare::Leaf => 1.0,
            Fare::Crust => 1.3,
            Fare::Soil => 1.1,
            Fare::Fruit => 0.35,
            Fare::Nectar => 0.12,
            Fare::Flesh => 0.45,
            Fare::Carrion => 0.55,
            Fare::Egg => 0.3,
            Fare::Plankton => 0.5,
        }
    }
}

/// How well each mouth takes in each food, per unit of its size:
/// leaf, fruit, nectar, crust, flesh, carrion, egg, plankton, soil.
pub fn mouth_takes(o: Organ) -> [f32; FARES] {
    use Organ::*;
    match o {
        Jaw => [0.6, 0.7, 0.0, 0.3, 0.6, 0.7, 0.7, 0.0, 0.15],
        Beak => [0.4, 1.1, 0.0, 0.3, 0.55, 0.6, 1.1, 0.0, 0.3],
        Mandible => [0.95, 0.6, 0.0, 0.45, 0.8, 0.6, 0.5, 0.0, 0.1],
        Fang => [0.1, 0.3, 0.0, 0.0, 1.25, 0.6, 0.6, 0.0, 0.0],
        Grinder => [1.2, 0.6, 0.0, 0.9, 0.15, 0.3, 0.3, 0.0, 0.2],
        Radula => [0.5, 0.3, 0.0, 1.25, 0.1, 0.45, 0.2, 0.0, 0.6],
        Proboscis => [0.0, 0.6, 1.6, 0.0, 0.3, 0.2, 0.4, 0.0, 0.0],
        Sieve => [0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0, 1.0, 0.35],
        Gullet => [0.2, 0.9, 0.0, 0.0, 0.7, 0.8, 1.3, 0.3, 0.0],
        Scoop => [0.25, 0.3, 0.0, 0.2, 0.1, 0.6, 0.5, 0.2, 1.0],
        _ => [0.0; FARES],
    }
}

/// How hard each striking organ hits, per unit of its size.
pub fn strikes(o: Organ) -> f32 {
    use Organ::*;
    match o {
        Fang => 1.3,
        Mandible => 1.0,
        Jaw => 0.8,
        Beak => 0.7,
        Gullet | Grinder | Scoop | Radula => 0.25,
        Chela => 0.8,
        Talon => 0.6,
        Horn => 0.5,
        Club => 0.6,
        Stinger => 0.3,
        Digger => 0.25,
        _ => 0.0,
    }
}

/// What a body is equipped with: the area of each organ, in body units
/// squared, already weighted by how well its form serves it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kit {
    /// One per organ, in `Organ::ALL` order.
    pub area: Vec<f32>,
}

impl Default for Kit {
    fn default() -> Self {
        Self { area: vec![0.0; ORGANS] }
    }
}

impl Kit {
    pub fn add(&mut self, o: Organ, a: f32) {
        self.area[o.index()] += a * o.gain();
    }

    pub fn get(&self, o: Organ) -> f32 {
        self.area.get(o.index()).copied().unwrap_or(0.0)
    }

    pub fn has(&self, o: Organ) -> bool {
        self.get(o) > 0.0
    }

    /// Share of a total area, 0..1.
    pub fn frac(&self, o: Organ, total: f32) -> f32 {
        if total > 0.0 { self.get(o) / total } else { 0.0 }
    }

    /// The organs it has at all, most first.
    pub fn list(&self) -> Vec<Organ> {
        let mut v: Vec<Organ> = Organ::ALL.iter().copied().filter(|&o| self.has(o)).collect();
        v.sort_by(|a, b| self.get(*b).total_cmp(&self.get(*a)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organs_listed_in_order() {
        for (i, o) in Organ::ALL.iter().enumerate() {
            assert_eq!(o.index(), i);
        }
    }

    #[test]
    fn every_tissue_and_form_is_some_organ() {
        let mut seen = std::collections::HashSet::new();
        for t in Tissue::ALL {
            for f in Form::ALL {
                seen.insert(Organ::of(t, f));
            }
        }
        // Everything but plain skeleton is reachable from more than one road.
        assert!(seen.len() >= 44, "{}", seen.len());
    }

    #[test]
    fn mouths_take_food_and_others_do_not() {
        for o in Organ::ALL {
            let mouth = Organ::of(Tissue::Mouth, Form::Tube) == o
                || Form::ALL.iter().any(|&f| Organ::of(Tissue::Mouth, f) == o);
            assert_eq!(mouth, mouth_takes(o).iter().any(|&v| v > 0.0), "{o:?}");
        }
    }
}
