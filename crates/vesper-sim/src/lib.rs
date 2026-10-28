//! Vesper's living world: creatures and plants grown from evolving genomes,
//! minds that learn, ecology with conserved matter, disease and parasites,
//! societies and their wars, all running headless and cheaply on the CPU.
//!
//! The host steps a [`Sim`], reads a [`frame::Frame`] for the renderer each
//! tick, turns frames into drawable segments with [`pose`] and acts out what
//! each creature is doing with [`animate`], forwards [`history`] facts to the
//! chronicle, and saves the world with [`save`].

mod act;
pub mod animate;
pub mod body;
pub mod brain;
pub mod creature;
mod ecology;
pub mod genetics;
pub mod frame;
pub mod genome;
mod grid;
pub mod ground;
pub mod history;
mod life;
pub mod mind;
pub mod physics;
pub mod plant;
pub mod pose;
pub mod rng;
pub mod runner;
pub mod save;
mod sim;
pub mod society;
pub mod species;
pub mod strain;
mod think;
pub mod world;

pub use genome::Tissue;
pub use sim::{PHASE_NAMES, PHASES, Sim};
