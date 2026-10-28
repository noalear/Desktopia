//! Saving and loading whole worlds. A save holds everything, including the
//! random state, so a loaded world carries on exactly where it left off.

use crate::body::Body;
use crate::ecology;
use crate::sim::Sim;
use crate::world::{COLS, World};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

const MAGIC: &[u8; 8] = b"VESPERW\0";
const VERSION: u32 = 6;

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    NotASave,
    /// Saved by a different version of the simulation.
    Version(u32),
    Corrupt,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "can't read the save: {e}"),
            LoadError::NotASave => write!(f, "not a Vesper world"),
            LoadError::Version(v) => write!(f, "saved by world format {v}, this build reads {VERSION}"),
            LoadError::Corrupt => write!(f, "the save is damaged"),
        }
    }
}

impl std::error::Error for LoadError {}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The world as bytes.
pub fn to_bytes(sim: &Sim) -> Vec<u8> {
    let body = postcard::to_allocvec(&sim.w).expect("worlds always serialize");
    let mut out = Vec::with_capacity(body.len() + 24);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&fnv(&body).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn from_bytes(bytes: &[u8]) -> Result<Sim, LoadError> {
    if bytes.len() < 24 || &bytes[..8] != MAGIC {
        return Err(LoadError::NotASave);
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != VERSION {
        return Err(LoadError::Version(version));
    }
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let sum = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let body = bytes.get(24..24 + len).ok_or(LoadError::Corrupt)?;
    if fnv(body) != sum {
        return Err(LoadError::Corrupt);
    }
    let mut w: World = postcard::from_bytes(body).map_err(|_| LoadError::Corrupt)?;
    // Bodies aren't saved; they grow again from the genomes exactly as before.
    for c in &mut w.creatures {
        c.body = Arc::new(Body::develop(&c.genome));
        // Brains keep only what was learned; instincts come from the genome.
        c.brain.attach(&c.genome.brain);
    }
    for p in &mut w.plants {
        p.body = Arc::new(p.genome.develop());
    }
    w.canopy = vec![0.0; COLS];
    w.rebuild_index();
    w.ground.refresh(w.env.river);
    ecology::rebuild_plant_grid(&mut w);
    Ok(Sim::from_world(w))
}

/// Writes the world to a file safely: to a temporary file first, then renamed
/// over the old save, so a crash mid-write never loses the world.
pub fn save(sim: &Sim, path: &Path) -> io::Result<()> {
    let bytes = to_bytes(sim);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes)?;
    // Keep the previous save as a backup.
    if path.exists() {
        let _ = fs::rename(path, path.with_extension("bak"));
    }
    fs::rename(&tmp, path)
}

/// Loads a world, falling back to the backup if the main save is damaged.
pub fn load(path: &Path) -> Result<Sim, LoadError> {
    let main = fs::read(path).map_err(LoadError::Io).and_then(|b| from_bytes(&b));
    match main {
        Ok(s) => Ok(s),
        Err(e) => {
            let bak = path.with_extension("bak");
            match fs::read(&bak) {
                Ok(b) => from_bytes(&b).map_err(|_| e),
                Err(_) => Err(e),
            }
        }
    }
}
