//! Things that live inside creatures: microbes that cause disease, parasites
//! you can watch growing through a translucent hide, and partners that feed
//! their host with light. They are one family of evolving lineages, so a
//! parasite can drift into a partner, or a partner turn deadly.

use crate::history::{SpeciesId, StrainId, StrainTraits};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strain {
    pub id: StrainId,
    pub parent: Option<StrainId>,
    /// The host species it first appeared in.
    pub origin_host: SpeciesId,
    /// 0 microbe .. 1 visible parasite.
    pub visible: f32,
    /// Chance per second of passing on close contact, at full load.
    pub contagion: f32,
    /// Passing on when its host is eaten.
    pub ingestion: f32,
    /// Energy drained per second per unit of host matter at full load. Negative feeds the host.
    pub virulence: f32,
    /// Load growth per second.
    pub growth: f32,
    /// Light-feeding it gives its host, per unit of host light film... or its own.
    pub photosynthetic: f32,
    /// How hard it steers its host toward predators and crowds.
    pub manipulation: f32,
    /// Bursts out of the host when grown, killing it and scattering spores.
    pub bursts: f32,
    /// Signature it is adapted to; hosts far from it resist.
    pub key: [f32; 4],
    /// Colour the renderer can tint infected tissue with.
    pub hue: f32,
    pub born: f64,
    /// Hosts currently carrying it.
    pub hosts: u32,
    /// Died out; kept so infection slots stay valid.
    pub gone: bool,
}

/// A strain living in one host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Infection {
    pub strain: StrainId,
    /// Index into the world's strain table (refreshed when strains are pruned).
    pub slot: u32,
    /// 0..1 how far it has grown.
    pub load: f32,
}

impl Strain {
    pub fn traits(&self) -> StrainTraits {
        StrainTraits {
            visible: self.visible,
            contagion: self.contagion,
            virulence: self.virulence,
            manipulation: self.manipulation,
            bursts: self.bursts,
            photosynthetic: self.photosynthetic,
        }
    }

    /// A strain arising from nothing in particular: soil microbes finding a host.
    pub fn wild(id: StrainId, host: SpeciesId, host_key: [f32; 4], now: f64, rng: &mut Rng) -> Self {
        let visible = if rng.chance(0.35) { rng.range(0.5, 1.0) } else { rng.range(0.0, 0.3) };
        let mut key = host_key;
        for k in key.iter_mut() {
            *k = (*k + rng.normal() * 0.05).clamp(0.0, 1.0);
        }
        Self {
            id,
            parent: None,
            origin_host: host,
            visible,
            contagion: rng.range(0.02, 0.2),
            ingestion: rng.range(0.1, 0.6),
            virulence: rng.range(-0.02, 0.25),
            growth: rng.range(0.003, 0.02),
            photosynthetic: if rng.chance(0.15) { rng.range(0.2, 0.8) } else { 0.0 },
            manipulation: if visible > 0.5 { rng.range(0.0, 0.6) } else { 0.0 },
            bursts: if visible > 0.5 { rng.range(0.0, 1.0) } else { 0.0 },
            key,
            hue: rng.f32(),
            born: now,
            hosts: 0,
            gone: false,
        }
    }

    /// A copy that changed while passing between hosts. Returns whether the
    /// change was big enough to count as a new strain.
    pub fn drift(&self, rng: &mut Rng) -> Option<Self> {
        if !rng.chance(0.008) {
            return None;
        }
        let mut s = self.clone();
        let mut nudge = |v: &mut f32, sd: f32, lo: f32, hi: f32| *v = (*v + rng.normal() * sd).clamp(lo, hi);
        nudge(&mut s.contagion, 0.03, 0.0, 0.6);
        nudge(&mut s.ingestion, 0.05, 0.0, 1.0);
        nudge(&mut s.virulence, 0.04, -0.15, 0.6);
        nudge(&mut s.growth, 0.003, 0.001, 0.05);
        nudge(&mut s.photosynthetic, 0.05, 0.0, 1.0);
        nudge(&mut s.manipulation, 0.05, 0.0, 1.0);
        nudge(&mut s.bursts, 0.05, 0.0, 1.0);
        nudge(&mut s.visible, 0.03, 0.0, 1.0);
        nudge(&mut s.hue, 0.03, 0.0, 1.0);
        for k in s.key.iter_mut() {
            *k = (*k + rng.normal() * 0.06).clamp(0.0, 1.0);
        }
        s.parent = Some(self.id);
        s.hosts = 0;
        Some(s)
    }

    /// How well it takes hold in a host with this immune signature, 0..1.
    pub fn fit(&self, immune: &[f32; 4]) -> f32 {
        let d: f32 = self.key.iter().zip(immune).map(|(a, b)| (a - b).abs()).sum::<f32>() / 4.0;
        (1.0 - d * 4.0).clamp(0.0, 1.0)
    }

    /// A partner rather than a burden.
    pub fn is_symbiont(&self) -> bool {
        self.virulence < 0.0 || self.photosynthetic > 0.4 && self.virulence < 0.05
    }
}
