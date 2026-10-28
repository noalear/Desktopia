//! Runs a world headless and prints how it is doing, for tuning.
//!
//! cargo run --release -p vesper-sim --example run -- [hours] [seed]

use std::time::{Duration, Instant};
use vesper_sim::Sim;
use vesper_sim::history::{Fact, Kingdom};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let hours: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    sim.enable_profile();
    let start = Instant::now();
    let mut facts: std::collections::BTreeMap<String, u32> = Default::default();
    let report_every: f64 = std::env::var("EVERY").ok().and_then(|s| s.parse().ok()).unwrap_or(600.0);
    let mut next = report_every;
    let mut step_time = Duration::ZERO;
    let mut steps = 0u32;
    while sim.time() < hours * 3600.0 {
        let t = Instant::now();
        sim.step();
        step_time += t.elapsed();
        steps += 1;
        for (_, f) in sim.drain_history() {
            let name = match &f {
                Fact::First { behavior, .. } => {
                    println!("  {:>7.0}s FIRST {behavior:?}", sim.time());
                    format!("First")
                }
                Fact::SpeciesArose { .. } => "SpeciesArose".into(),
                Fact::SpeciesExtinct { .. } => "SpeciesExtinct".into(),
                Fact::Death { cause, .. } => {
                    format!("Death::{}", format!("{cause:?}").split([' ', '{']).next().unwrap())
                }
                Fact::Nature { kind, .. } => format!("Nature::{kind:?}"),
                Fact::Census(_) => "Census".into(),
                other => format!("{}", format!("{other:?}").split([' ', '{']).next().unwrap()),
            };
            *facts.entry(name).or_default() += 1;
        }
        if sim.time() >= next {
            next += report_every;
            let w = sim.world();
            let alive_c = w
                .species
                .list
                .iter()
                .filter(|s| s.extinct.is_none() && matches!(s.rep, vesper_sim::species::Rep::Creature(_)))
                .count();
            let alive_p = w
                .species
                .list
                .iter()
                .filter(|s| s.extinct.is_none() && matches!(s.rep, vesper_sim::species::Rep::Plant(_)))
                .count();
            let n = w.creatures.len().max(1) as f32;
            let diet = w.creatures.iter().map(|c| c.genome.diet).sum::<f32>() / n;
            let neurons = w.creatures.iter().map(|c| c.genome.brain.hidden as f32).sum::<f32>() / n;
            let size = w.creatures.iter().map(|c| c.length()).sum::<f32>() / n;
            let infected = w.creatures.iter().filter(|c| !c.infections.is_empty()).count();
            let cm: f32 = w.creatures.iter().map(|c| c.matter).sum();
            let pm: f32 = w.plants.iter().map(|p| p.matter).sum();
            let rm: f32 = w.remains.iter().map(|r| r.matter).sum();
            let sm: f32 = w.soil.iter().sum();
            let am: f32 = w.air.iter().sum();
            let generation = w.creatures.iter().map(|c| c.generation).max().unwrap_or(0);
            println!(
                "{:>6.1}h c {:>5} p {:>5} egg {:>4} rem {:>4} | sp {:>3}c {:>3}p grp {:>3} inf {:>4} | diet {:.2} neur {:>4.1} len {:.4} gen {:>4} | matter c {:>6.1} p {:>6.1} r {:>5.1} soil {:>7.1} air {:>5.1} deep {:>6.1} | {:.2} ms/step",
                sim.time() / 3600.0,
                w.creatures.len(),
                w.plants.len(),
                w.eggs.len(),
                w.remains.len(),
                alive_c,
                alive_p,
                w.society.groups.len(),
                infected,
                diet,
                neurons,
                size,
                generation,
                cm,
                pm,
                rm,
                sm,
                am,
                w.deep,
                step_time.as_secs_f64() * 1000.0 / steps as f64,
            );
            let c = &w.counters;
            println!(
                "        births {} deaths {} (kills {} starved {} aged {} disease {} exposure {}) plants +{} -{}",
                c.births, c.deaths, c.kills, c.starved, c.aged, c.disease, c.exposure, c.plant_births, c.plant_deaths
            );
            let mut acts: std::collections::BTreeMap<String, u32> = Default::default();
            for c in &w.creatures {
                *acts.entry(format!("{:?}", c.action)).or_default() += 1;
            }
            let mean =
                |f: &dyn Fn(&vesper_sim::creature::Creature) -> f32| w.creatures.iter().map(|c| f(c)).sum::<f32>() / n;
            println!(
                "        minds: learned {:.1} surprise {:.3} value {:.2} places {:.1} known {:.1} wary {:.2} | fear {:.2} curious {:.2} lonely {:.2} tired {:.2} shelter {:.2}",
                mean(&|c| c.brain.learned()),
                mean(&|c| c.brain.surprise),
                mean(&|c| c.brain.value),
                mean(&|c| c.mind.places.len() as f32),
                mean(&|c| c.mind.known.len() as f32),
                mean(&|c| c.mind.wary.iter().map(|&v| v.max(0) as f32 / 127.0).sum::<f32>()),
                mean(&|c| c.mind.mood.fear),
                mean(&|c| c.mind.mood.curious),
                mean(&|c| c.mind.mood.lonely),
                mean(&|c| c.mind.mood.tired),
                mean(&|c| c.shelter),
            );
            let acts: Vec<String> = acts.iter().map(|(k, v)| format!("{k} {v}")).collect();
            println!("        acts: {}", acts.join(", "));
            if let Some(p) = sim.take_profile() {
                let total: f64 = p.iter().sum();
                let parts: Vec<String> = p
                    .iter()
                    .zip(vesper_sim::PHASE_NAMES)
                    .map(|(t, n)| format!("{n} {:.0}%", t / total * 100.0))
                    .collect();
                println!("        {}", parts.join(" "));
            }
            step_time = Duration::ZERO;
            steps = 0;
        }
    }
    println!("\nfacts:");
    for (k, v) in facts {
        println!("  {k}: {v}");
    }
    let _ = Kingdom::Rooted;
    println!("wall time {:.1}s", start.elapsed().as_secs_f64());
}
