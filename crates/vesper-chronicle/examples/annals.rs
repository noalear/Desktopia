//! Prints a demo world's annals: `cargo run -p vesper-chronicle --example annals -- 5`.

fn main() {
    let days: f64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(3.0);
    let seed: u64 = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(7);
    let h = vesper_chronicle::demo::history(days, seed);
    for e in &h.entries {
        println!("[{:>7.2}h] {:?}/{} {}: {}", e.tick / 3600.0, e.kind, e.weight, e.title, h.plain(&e.text));
    }
    println!(
        "\n{} species ({} living), {} notables, {} wars, {} outbreaks, {} eras, {} entries",
        h.species.len(),
        h.living_species(),
        h.notables.len(),
        h.wars.len(),
        h.outbreaks.len(),
        h.eras.len(),
        h.entries.len()
    );
}
