//! Follows one creature of a species through its ticks.
use vesper_sim::Sim;
use vesper_sim::organ::Fare;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sp: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let mut sim = Sim::new(1, 1_800_000_000.0);
    for _ in 0..50 {
        sim.step();
    }
    let id =
        sim.world().creatures.iter().filter(|c| c.species.0 == sp && c.maturity > 0.6).map(|c| c.id).next().unwrap();
    {
        let c = sim.world().creatures.iter().find(|c| c.id == id).unwrap();
        println!(
            "matter {:.4} adult {:.4} upkeep {:.5} cap {:.3} takes leaf {:.5} gut {:.5} intake(leaf,1.3) {:.5} eff {:.2} buoy {:.2} {:?} speed {:.4} swim {:.4} walk {:.4}",
            c.matter,
            c.adult_matter(),
            c.abil.upkeep,
            c.abil.energy_cap,
            c.abil.takes[Fare::Leaf.index()],
            c.abil.gut,
            c.intake(Fare::Leaf, 1.3),
            c.efficiency(Fare::Leaf, c.genome.palate),
            c.abil.buoyancy,
            c.abil.locomotion,
            c.abil.speed,
            c.abil.swim,
            c.abil.walk
        );
        println!(
            "organs {:?}",
            c.pheno.kit.list().iter().map(|&o| format!("{o:?} {:.3}", c.pheno.organ(o))).collect::<Vec<_>>()
        );
    }
    let mut last = 0.0;
    for t in 0..400 {
        sim.step();
        let w = sim.world();
        let Some(c) = w.creatures.iter().find(|c| c.id == id) else {
            println!("gone at tick {t}");
            break;
        };
        if t % 10 == 0 {
            println!(
                "t{t:>4} e {:.3} d {:+.4} m {:.4} {:?} {:?} power {:+.2} rise {:+.2} alt {:.3} speed {:.4} focus {:?} health {:.2} wait {:.0}",
                c.energy,
                c.energy - last,
                c.matter,
                c.action,
                c.motion.mode,
                c.drive[0],
                c.drive[1],
                c.pos[2],
                c.motion.speed,
                c.focus.map(|f| f[2]),
                c.health,
                c.breed_wait
            );
        }
        last = c.energy;
    }
}
