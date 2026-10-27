//! Times ticks of a warmed-up world. The warm world is cached in the target dir.
use std::time::Instant;
use vesper_sim::{Sim, save};
fn main() {
    let path = std::path::Path::new("target/bench-world.vesper");
    let mut sim = match save::load(path) {
        Ok(s) => s,
        Err(_) => {
            let mut s = Sim::new(1, 1_800_000_000.0);
            for _ in 0..300 {
                s.step();
            }
            save::save(&s, path).unwrap();
            s
        }
    };
    let steps: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(200);
    let n = sim.world().creatures.len();
    sim.enable_profile();
    let t = Instant::now();
    for _ in 0..steps {
        sim.step();
    }
    println!("{} creatures {:.2} ms/step", n, t.elapsed().as_secs_f64() * 1000.0 / steps as f64);
    if let Some(p) = sim.take_profile() {
        for (s, name) in p.iter().zip(vesper_sim::PHASE_NAMES) {
            println!("  {name:<12} {:.2} ms/step", s * 1000.0 / steps as f64);
        }
    }
}
