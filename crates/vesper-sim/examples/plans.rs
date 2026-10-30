//! Prints each founder plan's grown body, part by part, as organs, with
//! what the body can do.
use vesper_sim::body::{Body, Phenotype};
use vesper_sim::creature::{abilities, adult_matter};
use vesper_sim::genome::{Founder, Genome};
use vesper_sim::organ::Organ;
use vesper_sim::rng::Rng;

fn main() {
    let seed: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    for kind in Founder::ALL {
        let mut rng = Rng::new(seed);
        for (label, g) in [("plan", Genome::founder_plan(kind, &mut rng)), ("founder", Genome::founder(kind, &mut rng))]
        {
            let body = Body::develop(&g);
            let p = Phenotype::read(&body, 1.0, 0);
            let a = abilities(&g, &body, &p, g.adult_scale, g.brain.hidden, adult_matter(&g, &body));
            let segs: Vec<String> = body
                .segments
                .iter()
                .map(|s| {
                    format!(
                        "{:?}:{:?}{} {:.2}x{:.2}",
                        s.role,
                        Organ::of(s.tissue, s.form),
                        if s.side != 0 { "²" } else { "" },
                        s.length,
                        s.width
                    )
                })
                .collect();
            println!(
                "{kind:?} {label}: {} segs, area {:.2}, mass {:.2}, legs {} ({:.2}), buoyancy {:.2}, {:?} speed {:.4} (walk {:.4} swim {:.4} fly {:.4}) len {:.4} upkeep {:.4} cap {:.2}",
                body.segments.len(),
                p.total_area,
                p.mass,
                p.legs,
                p.leg,
                a.buoyancy,
                a.locomotion,
                a.speed,
                a.walk,
                a.swim,
                a.fly,
                a.length,
                a.upkeep,
                a.energy_cap
            );
            println!("    {}", segs.join(", "));
        }
    }
}
