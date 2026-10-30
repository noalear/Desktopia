//! Prints each founder plan's grown body, part by part, as organs.
use vesper_sim::body::{Body, Phenotype};
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
            let segs: Vec<String> = body
                .segments
                .iter()
                .map(|s| {
                    format!(
                        "{:?}{} {:.2}x{:.2}",
                        Organ::of(s.tissue, s.form),
                        if s.side != 0 { "²" } else { "" },
                        s.length,
                        s.width
                    )
                })
                .collect();
            println!("{kind:?} {label}: area {:.2} | {}", p.total_area, segs.join(", "));
        }
    }
}
