//! Follows one founder of each kind through its first minute.
use vesper_sim::Sim;
use vesper_sim::history::Locomotion;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    let picks: Vec<u64> = [Locomotion::Drift, Locomotion::Swim, Locomotion::Walk]
        .iter()
        .filter_map(|l| sim.world().creatures.iter().find(|c| c.abil.locomotion == Some(*l)).map(|c| c.id.0))
        .collect();
    let mut last: Vec<f32> = vec![0.0; picks.len()];
    for t in 0..400 {
        sim.step();
        if t % 25 == 0 {
            let w = sim.world();
            for (k, id) in picks.iter().enumerate() {
                if let Some(c) = w.creatures.iter().find(|c| c.id.0 == *id) {
                    let light = w.env.light(c.pos[0], c.pos[2]);
                    println!(
                        "{:>3.0}s {:?} energy {:.3} (d {:+.3}) cap {:.2} film {:.4} light {:.2} upkeep {:.4} effort {:.2} toil {:.4} alt {:.3} matter {:.2} action {:?} health {:.2}",
                        t as f32 * 0.2,
                        c.abil.locomotion.unwrap(),
                        c.energy,
                        c.energy - last[k],
                        c.abil.energy_cap,
                        c.abil.film * light,
                        light,
                        c.abil.upkeep,
                        c.effort,
                        c.toil * vesper_sim::physics::MOTOR_COST,
                        c.pos[2],
                        c.matter / c.abil.body_matter,
                        c.action,
                        c.health
                    );
                    last[k] = c.energy;
                } else {
                    println!("{:>3.0}s #{id} gone", t as f32 * 0.2);
                }
            }
        }
    }
    for (_, f) in sim.drain_history() {
        if let vesper_sim::history::Fact::Death { .. } = f {
            let s = format!("{f:?}");
            if picks.iter().any(|id| s.contains(&format!("CreatureId({id})"))) {
                println!("{s}");
            }
        }
    }
}
