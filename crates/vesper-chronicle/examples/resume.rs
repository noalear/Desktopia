//! Carries on a saved world and its chronicle headless, drawing species as
//! the wallpaper does: `resume SAVE CHRONICLE-DIR MINUTES`.

use vesper_chronicle::Chronicle;
use vesper_chronicle::likeness::Sketchbook;
use vesper_sim::frame::Frame;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut sim = vesper_sim::save::load(std::path::Path::new(&args[1])).expect("load the world");
    let mut chronicle = Chronicle::open(&args[2]);
    chronicle.rewind_to(sim.time());
    let minutes: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let end = sim.time() + minutes * 60.0;
    let mut book = Sketchbook::default();
    let mut next_look = 0.0;
    while sim.time() < end {
        sim.step();
        chronicle.record_all(sim.drain_history());
        if sim.time() >= next_look {
            next_look = sim.time() + 60.0;
            for (species, l) in book.sketch(&Frame::of(sim.world()), chronicle.history()) {
                chronicle.record_likeness(species, l);
            }
        }
    }
    chronicle.checkpoint();
    eprintln!("{} species alive", chronicle.history().living_species());
}
