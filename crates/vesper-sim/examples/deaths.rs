//! How each species dies over the first minutes, .
//!
//! cargo run --release -p vesper-sim --example deaths -- [minutes] [seed]
use vesper_sim::Sim;
use vesper_sim::history::{DeathCause, Fact};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minutes: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    let mut rows: std::collections::BTreeMap<u64, std::collections::BTreeMap<String, u32>> = Default::default();
    for _ in 0..(minutes * 300.0) as u64 {
        sim.step();
        for (_, f) in sim.drain_history() {
            match f {
                Fact::Death { species, cause, .. } => {
                    let name = match cause {
                        DeathCause::Predation { killer_species, .. } => format!("eaten by {}", killer_species.0),
                        other => format!("{other:?}").split([' ', '{', '(']).next().unwrap().to_string(),
                    };
                    *rows.entry(species.0).or_default().entry(name).or_default() += 1;
                }
                _ => {}
            }
        }
    }
    let w = sim.world();
    for (sp, r) in &rows {
        let alive = w.creatures.iter().filter(|c| c.species.0 == *sp).count();
        println!("sp {sp:>3} alive {alive:>3} | {r:?}");
    }
}
