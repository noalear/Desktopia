use vesper_sim::Sim;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    let w = sim.world();
    let mut seen = std::collections::BTreeSet::new();
    for c in &w.creatures {
        if seen.insert(c.species) {
            println!(
                "species {:?} segs {} loco {:?} matter {:.4} body {:.4} adult {:.4} cap {:.3} upkeep {:.4} film {:.4} bite {:.5} gut {:.5} speed {:.4} sense {:.3} len {:.4} mat {:.2} filter {}",
                c.species,
                c.body.segments.len(),
                c.abil.locomotion,
                c.matter,
                c.abil.body_matter,
                c.adult_matter(),
                c.abil.energy_cap,
                c.abil.upkeep,
                c.abil.film,
                c.abil.strike,
                c.abil.gut,
                c.abil.speed,
                c.abil.sense,
                c.length(),
                c.maturity,
                c.abil.takes[vesper_sim::organ::Fare::Plankton.index()] > 0.0
            );
        }
    }
    let p = &w.plants[0];
    println!(
        "plant matter {:.4} adult {:.4} leaf {:.6} height {:.4}",
        p.matter,
        p.adult_matter,
        p.leaf_area(),
        p.height()
    );
    for _ in 0..300 {
        sim.step();
    }
    let w = sim.world();
    let mut seen = std::collections::BTreeSet::new();
    for c in &w.creatures {
        if seen.insert(c.species) {
            println!(
                "t60 species {:?} n {} energy {:.3}/{:.3} matter {:.4} eaten {:.4} action {:?} out eat {:.2} seek {:.2} thrust {:.2}",
                c.species,
                w.creatures.iter().filter(|o| o.species == c.species).count(),
                c.energy,
                c.abil.energy_cap,
                c.matter,
                c.tally.eaten,
                c.action,
                c.brain.out[7],
                c.brain.out[3],
                c.brain.out[0]
            );
        }
    }
}
