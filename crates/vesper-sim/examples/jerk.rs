//! Prints, tick by tick, how far each creature moves along x (1e-5 world
//! units), to find stop-go motion. `*` marks a tick spent on an action.
//!
//! cargo run --release -p vesper-sim --example jerk -- [warm-up ticks]
use vesper_sim::Sim;
fn main() {
    let warm: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(25);
    let mut sim = Sim::new(5, 1_800_000_000.0);
    for _ in 0..warm {
        sim.step();
    }
    let mut rows: std::collections::BTreeMap<u64, Vec<String>> = Default::default();
    let mut prev = sim.frame();
    for _ in 0..40 {
        sim.step();
        let f = sim.frame();
        for c in &f.creatures {
            if let Some(p) = prev.creatures.iter().find(|p| p.id == c.id) {
                let d = c.pos[0] - p.pos[0];
                rows.entry(c.id).or_default().push(format!(
                    "{:>6.0}{}",
                    d * 1e5,
                    match c.action {
                        vesper_sim::creature::Action::Idle => "",
                        _ => "*",
                    }
                ));
            }
        }
        prev = f;
    }
    let f = sim.frame();
    for (id, r) in rows {
        let c = f.creatures.iter().find(|c| c.id == id);
        println!("{id:>4} {:?} {}", c.map(|c| c.locomotion), r.join(""));
    }
}
