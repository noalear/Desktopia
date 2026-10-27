//! A snapshot of everything visible, made once per tick for the renderer.
//! Bodies are shared, so a frame is cheap to make and to send across threads.

use crate::body::Body;
use crate::creature::Action;
use crate::genome::LOOK_GENES;
use crate::history::Locomotion;
use crate::world::{COLS, DEPTH, ROWS, World};
use std::sync::Arc;
use vesper_terrain::RiverState;

#[derive(Clone, Debug, Default)]
pub struct EnvView {
    /// The twilight band's seasonal slide, world units.
    pub band_shift: f32,
    pub storm_x: f32,
    /// 0..1.
    pub storm: f32,
    /// 0..1 flash strength; fades over about a second.
    pub lightning: f32,
    pub lightning_x: f32,
    /// 0..1 how hard the volcano is erupting.
    pub eruption: f32,
    /// How far lava has run each way from the volcano, world units.
    pub lava: f32,
    /// 0..1 ash in the sky.
    pub ash: f32,
    pub river: RiverState,
    /// A cold fall off the glacier: strength and where its front is.
    pub cold: f32,
    pub cold_x: f32,
    /// A dust storm off the day side: strength and where it is.
    pub dust: f32,
    pub dust_x: f32,
    /// Wind along x near the ground and up high, world units per second.
    pub wind_low: f32,
    pub wind_high: f32,
}

#[derive(Clone, Debug)]
pub struct InfectionView {
    pub strain: u64,
    /// 0..1 how far it has grown.
    pub load: f32,
    /// 0 a microbe (sores, a sickly tint), 1 a parasite you can see inside.
    pub visible: f32,
    pub hue: f32,
    /// A partner that feeds its host rather than a disease.
    pub symbiont: bool,
}

/// A body's pose at one physics step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moment {
    /// x along the band, depth on the ground plane (world units), and the
    /// heading of the body's own upright plane.
    pub pos: [f32; 2],
    pub heading: f32,
    /// Which way it is drawn facing, -1 .. 1, swinging over through 0 as it turns.
    pub face: f32,
    /// World units per body unit.
    pub scale: f32,
    pub parts: Vec<PartPose>,
}

/// Where one body segment's centre is in the body's plane (forward from the
/// body's position, and altitude) and which way it points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose {
    pub seg: u16,
    pub u: f32,
    pub alt: f32,
    pub angle: f32,
}

impl Moment {
    pub fn of(rig: &crate::physics::Rig, pos: [f32; 3], heading: f32) -> Self {
        Self {
            pos: [pos[0], pos[1]],
            heading,
            face: rig.face,
            scale: rig.scale,
            parts: rig
                .parts
                .iter()
                .map(|p| PartPose { seg: p.seg, u: p.pos[0], alt: p.pos[1], angle: p.angle })
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreatureView {
    pub id: u64,
    pub species: u64,
    /// x along the band, depth 0 (front of the ground plane) .. 1 (back), altitude above the floor.
    pub pos: [f32; 3],
    /// World units per second, same axes as `pos` but depth in world units.
    pub vel: [f32; 3],
    /// Facing in the x-depth plane; 0 faces the night edge (right).
    pub heading: f32,
    /// World units per body unit.
    pub scale: f32,
    pub body: Arc<Body>,
    /// Segments torn off, as a bit mask over `body.segments`.
    pub lost: u64,
    /// 0 newborn .. 1 adult; segments with a later onset haven't grown yet.
    pub maturity: f32,
    pub look: [f32; LOOK_GENES],
    pub locomotion: Locomotion,
    /// Gait phase, radians, and how hard it is moving, 0..1.
    pub gait: f32,
    pub effort: f32,
    pub action: Action,
    pub target: Option<u64>,
    /// Light signal shown now, 0..1, and which of 16 flash patterns.
    pub flash: f32,
    pub symbol: u8,
    pub health: f32,
    /// Leaking glowing fluid, 0..1.
    pub bleeding: f32,
    pub energy: f32,
    /// 0 young .. 1 dying of age.
    pub age: f32,
    pub infection: Option<InfectionView>,
    pub group: Option<u64>,
    pub leader: bool,
    /// The body through this tick: where it was when the tick began, then at
    /// each physics step. Drawing samples along it.
    pub motion: Vec<Moment>,
}

#[derive(Clone, Debug)]
pub struct PlantView {
    pub id: u64,
    pub species: u64,
    /// x, depth 0..1, altitude (floating mats only).
    pub pos: [f32; 3],
    pub scale: f32,
    pub body: Arc<Body>,
    pub look: [f32; LOOK_GENES],
    pub growth: f32,
    pub health: f32,
    /// 0..1 how well-lit and open.
    pub vigor: f32,
    /// 0..1 how laden with fruit.
    pub fruit: f32,
    /// A trap that has closed on something.
    pub trap_shut: bool,
    pub floating: bool,
    pub glow: f32,
    pub spines: f32,
    pub toxin: f32,
}

#[derive(Clone, Debug)]
pub struct RemainsView {
    pub id: u64,
    pub species: u64,
    pub pos: [f32; 3],
    pub heading: f32,
    pub scale: f32,
    pub body: Arc<Body>,
    /// Segments not part of these remains.
    pub missing: u64,
    pub look: [f32; LOOK_GENES],
    /// 0 fresh .. 1 almost gone: soft tissue goes first, then shell and frame.
    pub decay: f32,
    /// Seconds since death.
    pub age: f32,
    /// The body through this tick, while it is still falling or settling.
    pub motion: Vec<Moment>,
}

#[derive(Clone, Debug)]
pub struct EggView {
    pub id: u64,
    pub species: u64,
    pub pos: [f32; 3],
    pub look: [f32; LOOK_GENES],
    /// Diameter in world units.
    pub size: f32,
    /// 0 laid .. 1 hatching.
    pub progress: f32,
    pub spore: bool,
}

#[derive(Clone, Debug)]
pub struct SporeView {
    pub pos: [f32; 3],
    pub strength: f32,
    pub hue: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Frame {
    /// Sim seconds since the world was born.
    pub time: f64,
    pub tick: u64,
    pub env: EnvView,
    pub creatures: Vec<CreatureView>,
    pub plants: Vec<PlantView>,
    pub remains: Vec<RemainsView>,
    pub eggs: Vec<EggView>,
    pub spores: Vec<SporeView>,
    /// Soil nutrient per column, averaged over depth: richer ground looks darker and greener.
    pub soil: Vec<f32>,
    /// Detritus drifting in the air per column, for marine snow.
    pub air: Vec<f32>,
}

/// The moments of a body's last tick, or where it is now if it didn't move.
fn motion(moments: &[Moment], rig: &crate::physics::Rig, pos: [f32; 3], heading: f32) -> Vec<Moment> {
    if moments.len() >= 2 { moments.to_vec() } else { vec![Moment::of(rig, pos, heading)] }
}

impl Frame {
    pub fn of(w: &World) -> Self {
        let e = &w.env;
        let env = EnvView {
            band_shift: e.band_shift,
            storm_x: e.storm_x,
            storm: e.storm,
            lightning: e.lightning,
            lightning_x: e.lightning_x,
            eruption: e.eruption,
            lava: e.lava,
            ash: e.ash,
            river: e.river,
            cold: e.cold,
            cold_x: e.cold_x,
            dust: e.dust,
            dust_x: e.dust_x,
            wind_low: e.wind(0.0),
            wind_high: e.wind(0.5),
        };
        let leaders: Vec<u64> = w.society.groups.iter().filter_map(|g| g.leader.map(|l| l.0)).collect();
        let creatures = w
            .creatures
            .iter()
            .map(|c| CreatureView {
                id: c.id.0,
                species: c.species.0,
                pos: [c.pos[0], c.pos[1] / DEPTH, c.pos[2]],
                vel: c.vel,
                heading: c.heading,
                scale: c.scale(),
                body: c.body.clone(),
                lost: c.lost,
                maturity: c.maturity,
                look: c.genome.look,
                locomotion: c.abil.locomotion.unwrap_or(Locomotion::Crawl),
                gait: c.gait,
                effort: c.effort,
                action: c.action,
                target: c.target.map(|t| t.0),
                flash: c.flash,
                symbol: c.symbol,
                health: c.health.clamp(0.0, 1.0),
                bleeding: c.bleeding.min(1.0),
                energy: (c.energy / c.abil.energy_cap.max(1e-6)).clamp(0.0, 1.0),
                age: c.wear.min(1.0),
                infection: c.infections.iter().max_by(|a, b| a.load.total_cmp(&b.load)).and_then(|i| {
                    w.strains.get(i.slot as usize).map(|s| InfectionView {
                        strain: s.id.0,
                        load: i.load,
                        visible: s.visible,
                        hue: s.hue,
                        symbiont: s.is_symbiont(),
                    })
                }),
                group: c.group.map(|g| g.0),
                leader: leaders.contains(&c.id.0),
                motion: motion(&c.moments, &c.rig, c.pos, c.heading),
            })
            .collect();
        let plants = w
            .plants
            .iter()
            .map(|p| PlantView {
                id: p.id.0,
                species: p.species.0,
                pos: [p.x, p.depth / DEPTH, p.alt],
                scale: p.scale(),
                body: p.body.clone(),
                look: p.genome.look,
                growth: p.growth01(),
                health: p.health.clamp(0.0, 1.0),
                vigor: p.vigor.clamp(0.0, 1.0),
                fruit: (p.fruit / (p.adult_matter * 0.25 * p.genome.fruit).max(1e-6)).clamp(0.0, 1.0),
                trap_shut: p.trap_shut > 0.0,
                floating: p.genome.floats(),
                glow: p.genome.glow,
                spines: p.genome.spines,
                toxin: p.genome.toxin,
            })
            .collect();
        let remains = w
            .remains
            .iter()
            .map(|r| RemainsView {
                id: r.id,
                species: r.species.0,
                pos: [r.pos[0], r.pos[1] / DEPTH, r.pos[2]],
                heading: r.heading,
                scale: r.scale,
                body: r.body.clone(),
                missing: r.missing,
                look: r.look,
                decay: r.decay01(),
                age: r.age,
                motion: r.rig.as_ref().map(|rig| motion(&r.moments, rig, r.pos, r.heading)).unwrap_or_default(),
            })
            .collect();
        let eggs = w
            .eggs
            .iter()
            .map(|e| EggView {
                id: e.id,
                species: e.species.0,
                pos: [e.pos[0], e.pos[1] / DEPTH, e.pos[2]],
                look: e.genome.look,
                size: (e.matter / crate::world::MATTER_DENSITY).sqrt() * 1.6,
                progress: (1.0 - e.hatch_in / (10.0 + 240.0 * e.genome.invest.sqrt())).clamp(0.0, 1.0),
                spore: e.spore,
            })
            .collect();
        let spores = w
            .spores
            .iter()
            .map(|s| SporeView {
                pos: [s.pos[0], s.pos[1] / DEPTH, s.pos[2]],
                strength: s.strength,
                hue: w.strain_slot(s.strain).map(|i| w.strains[i].hue).unwrap_or(0.0),
            })
            .collect();
        let soil = (0..COLS).map(|c| w.soil[c * ROWS..(c + 1) * ROWS].iter().sum::<f32>() / ROWS as f32).collect();
        Frame { time: w.time, tick: w.tick, env, creatures, plants, remains, eggs, spores, soil, air: w.air.clone() }
    }
}
