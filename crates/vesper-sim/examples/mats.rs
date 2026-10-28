//! How floating mats fare: how many, how high, and where along the band.
use vesper_sim::Sim;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let hours: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    let mut next = 0.0;
    while sim.time() < hours * 3600.0 {
        if sim.time() >= next {
            next += 600.0;
            let w = sim.world();
            let mats: Vec<_> = w.plants.iter().filter(|p| p.genome.floats()).collect();
            let n = mats.len().max(1) as f32;
            let mut bins = [0u32; 8];
            for p in &mats {
                bins[((p.x / vesper_terrain::WORLD_W * 8.0) as usize).min(7)] += 1;
            }
            println!(
                "{:>5.0}s creatures {:>3} plants {:>4} mats {:>4} alt {:.3} along {:?}",
                sim.time(),
                w.creatures.len(),
                w.plants.len(),
                mats.len(),
                mats.iter().map(|p| p.alt).sum::<f32>() / n,
                bins
            );
        }
        sim.step();
    }
}
