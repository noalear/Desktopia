//! Each species' energy budget over the first minutes: what it takes in
//! against what it spends, and how often it is eating.
//!
//! cargo run --release -p vesper-sim --example budget -- [minutes] [seed]
use vesper_sim::Sim;
use vesper_sim::creature::Action;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let minutes: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    // species -> (samples, energy01, gain/upkeep, eating, maturity, food seen, n)
    let mut rows: std::collections::BTreeMap<u64, [f64; 7]> = Default::default();
    let ticks = (minutes * 300.0) as u64;
    for t in 0..ticks {
        sim.step();
        if t % 25 != 0 {
            continue;
        }
        let w = sim.world();
        for c in &w.creatures {
            let r = rows.entry(c.species.0).or_default();
            r[0] += 1.0;
            r[1] += (c.energy / c.abil.energy_cap.max(1e-9)) as f64;
            r[2] += (c.intake * c.abil.energy_cap / 30.0 / c.abil.upkeep.max(1e-9)) as f64;
            r[3] += (c.action == Action::Eating) as u8 as f64;
            r[4] += c.maturity as f64;
            r[5] += c.health as f64;
        }
        if t % 1500 == 0 {
            println!("{:.0} min: {} creatures", t as f64 / 300.0, w.creatures.len());
            for (sp, r) in &rows {
                let n = r[0].max(1.0);
                println!(
                    "  sp {sp:>3}: energy {:.2} gain/upkeep {:.2} eating {:.2} maturity {:.2} health {:.2} (n {})",
                    r[1] / n,
                    r[2] / n,
                    r[3] / n,
                    r[4] / n,
                    r[5] / n,
                    w.creatures.iter().filter(|c| c.species.0 == *sp).count()
                );
            }
            rows.clear();
        }
    }
    let c = &sim.world().counters;
    println!("births {} deaths {} starved {} kills {}", c.births, c.deaths, c.starved, c.kills);
}
