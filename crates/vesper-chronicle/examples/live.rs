//! Runs a real world headless into a chronicle folder, drawing species as the
//! wallpaper does, for trying the chronicle window on real data:
//! `cargo run --release -p vesper-chronicle --example live -- 30 out-dir [seed]`
//! runs 30 sim minutes.

use vesper_chronicle::Chronicle;
use vesper_chronicle::likeness::Sketchbook;
use vesper_sim::Sim;
use vesper_sim::frame::Frame;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minutes: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let dir = std::path::PathBuf::from(args.get(2).map_or("live-out", |s| s.as_str()));
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let _ = std::fs::remove_dir_all(&dir);
    let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    let mut sim = Sim::new(seed, epoch - minutes * 60.0);
    let mut chronicle = Chronicle::open(&dir);
    chronicle.new_world();
    let mut book = Sketchbook::default();
    let mut next_report = 0.0;
    let mut next_look = 0.0;
    while sim.time() < minutes * 60.0 {
        sim.step();
        chronicle.record_all(sim.drain_history());
        // Frames only once a sim minute, when a sketch may be due; the wallpaper has them anyway.
        if sim.time() >= next_look {
            next_look = sim.time() + 60.0;
            for (species, l) in book.sketch(&Frame::of(sim.world()), chronicle.history()) {
                chronicle.record_likeness(species, l);
            }
        }
        if sim.time() >= next_report {
            next_report += 600.0;
            let h = chronicle.history();
            eprintln!(
                "{:>5.0} min: {} species, {} portraits, {} entries",
                sim.time() / 60.0,
                h.living_species(),
                h.likenesses.len(),
                h.entries.len()
            );
        }
    }
    chronicle.checkpoint();
}

