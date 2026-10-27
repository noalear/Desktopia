//! Vesper's chronicle: the world's written history.
//!
//! The simulation reports [`Fact`]s; the chronicle turns them into a
//! [`History`] of species, lineages, notable lives, wars, plagues, famines and
//! eras, and keeps it on disk. The wallpaper owns one [`Chronicle`] and feeds it;
//! the chronicle window is a separate process that follows the same files with a
//! [`Reader`], so nothing extra runs while the window is closed.

pub mod demo;
pub mod facts;
pub mod history;
pub mod names;
mod store;

pub use facts::*;
pub use history::{Entry, History, Kind, Piece, Ref, Tally, notability};
pub use store::{Chronicle, Reader};
