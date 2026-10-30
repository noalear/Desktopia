//! What each founder's body is equipped with, and whether its food pays.
//!
//! cargo run --release -p vesper-sim --example kits
use vesper_sim::Sim;
use vesper_sim::organ::{Fare, Organ};

fn main() {
    let sim = Sim::new(std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1), 1_800_000_000.0);
    let w = sim.world();
    let mut seen = std::collections::BTreeSet::new();
    for c in &w.creatures {
        if !seen.insert(c.species) {
            continue;
        }
        let mut c = c.clone();
        c.maturity = 1.0;
        c.matter = c.adult_matter();
        c.refresh();
        let up = c.abil.upkeep;
        let organs: Vec<String> =
            c.pheno.kit.list().iter().map(|&o| format!("{o:?} {:.2}", c.pheno.organ(o))).collect();
        println!(
            "sp {:?} {:?} matter {:.4} upkeep {:.4} | leaf x{:.1} fruit x{:.1} nectar x{:.1} flesh x{:.1} carrion x{:.1} plankton x{:.1} soil x{:.1} film x{:.1} | strike {:.4} sight {:.3} smell {:.3} hear {:.3} touch {:.3} | cold {:.2} hot {:.2}",
            c.species,
            c.abil.locomotion,
            c.matter,
            up,
            c.yield_rate(Fare::Leaf, 1.0, c.genome.palate) / up,
            c.yield_rate(Fare::Fruit, 1.0, c.genome.palate) / up,
            c.yield_rate(Fare::Nectar, 1.0, c.genome.palate) / up,
            c.yield_rate(Fare::Flesh, 1.0, 0.0) / up,
            c.yield_rate(Fare::Carrion, 1.0, 0.0) / up,
            c.yield_rate(Fare::Plankton, 1.0, 0.0) / up,
            c.yield_rate(Fare::Soil, 1.0, 0.0) / up,
            c.abil.film * 0.5 / up,
            c.abil.strike,
            c.abil.sight,
            c.abil.smell,
            c.abil.hearing,
            c.abil.touch,
            c.bearable().0,
            c.bearable().1,
        );
        println!("    {}", organs.join(" "));
        let _ = Organ::Eye;
    }
}
