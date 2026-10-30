#![allow(clippy::type_complexity)]
//! Plant species over time: how many, how big, how well, and what they look
//! like, for tuning plant evolution.
//!
//! cargo run --release -p vesper-sim --example pdiag -- [seconds] [seed]
use std::collections::BTreeMap;
use vesper_sim::Sim;
use vesper_sim::species::{Rep, plant_traits};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3600.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let every: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(600.0);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    let mut next = 0.0;
    while sim.time() < secs {
        sim.step();
        sim.drain_history();
        if sim.time() >= next {
            next += every;
            let w = sim.world();
            let mut rows: BTreeMap<vesper_sim::history::SpeciesId, (u32, f32, f32, f32, f32, f32, f32)> =
                BTreeMap::new();
            for p in &w.plants {
                let r = rows.entry(p.species).or_default();
                r.0 += 1;
                r.1 += p.matter / p.adult_matter.max(1e-9);
                r.2 += p.health;
                r.3 += p.age / p.genome.lifespan;
                r.4 += p.vigor;
                r.5 += p.genome.distance(match &w.species.get(p.species).rep {
                    Rep::Plant(g) => g,
                    _ => &p.genome,
                });
                r.6 += p.generation as f32;
            }
            println!("t {:.0}s plants {} seedbank {}", sim.time(), w.plants.len(), w.seedbank.len());
            for (id, r) in rows {
                let n = r.0 as f32;
                let feats = match &w.species.get(id).rep {
                    Rep::Plant(g) => plant_traits(g).features.join(","),
                    _ => String::new(),
                };
                println!(
                    "  sp{:<4} n {:>5} grown {:.2} health {:.2} age {:.2} vigor {:.2} drift {:.2} gen {:.1} | {feats}",
                    id.0,
                    r.0,
                    r.1 / n,
                    r.2 / n,
                    r.3 / n,
                    r.4 / n,
                    r.5 / n,
                    r.6 / n
                );
            }
        }
    }
}
