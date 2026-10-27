//! Headless checks on bodies in motion: how often they flip the way they
//! face, how much they turn, and how big their rigs are.
use vesper_sim::Sim;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    for _ in 0..300 {
        sim.step();
    }
    let before: Vec<(u64, f32, f32)> = sim.world().creatures.iter().map(|c| (c.id.0, c.heading, c.rig.face)).collect();
    let mut turn = 0.0f32;
    let mut flips = 0;
    let mut n = 0;
    let mut last = before.clone();
    for _ in 0..50 {
        sim.step();
        let now: Vec<(u64, f32, f32)> = sim.world().creatures.iter().map(|c| (c.id.0, c.heading, c.rig.face)).collect();
        for (id, h, f) in &now {
            if let Some((_, h0, f0)) = last.iter().find(|x| x.0 == *id) {
                let mut d = (h - h0).abs();
                if d > std::f32::consts::PI {
                    d = std::f32::consts::TAU - d;
                }
                turn += d;
                n += 1;
                if f.signum() != f0.signum() {
                    flips += 1;
                }
            }
        }
        last = now;
    }
    let w = sim.world();
    for c in w.creatures.iter().take(15) {
        println!(
            "  {:?} parts {:>2} toil {:.2e}/s upkeep {:.2e}/s cap {:.2e} energy {:.2e} stride {:.2} power {:.2}",
            c.abil.locomotion,
            c.rig.parts.len(),
            c.toil * vesper_sim::physics::MOTOR_COST,
            c.abil.upkeep,
            c.abil.energy_cap,
            c.energy,
            c.stride,
            c.drive[0]
        );
    }
    let parts: usize = w.creatures.iter().map(|c| c.rig.parts.len()).sum();
    println!(
        "{} creatures, {} parts | mean turn {:.3} rad/tick, facing flips {} per 1000 creature-ticks",
        w.creatures.len(),
        parts,
        turn / n as f32,
        flips * 1000 / n.max(1)
    );
}
