//! How the founding populations fare in their first minutes, by way of moving.
use vesper_sim::Sim;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    for t in 0..=1800 {
        if t % 300 == 0 {
            let w = sim.world();
            let mut rows: std::collections::BTreeMap<String, (u32, f32, f32, f32, f32)> = Default::default();
            for c in &w.creatures {
                let r = rows.entry(format!("{:?}", c.abil.locomotion)).or_default();
                r.0 += 1;
                r.1 += c.stride;
                r.2 += c.energy / c.abil.energy_cap.max(1e-9);
                r.3 += c.matter / c.abil.body_matter.max(1e-9);
                r.4 += c.drive[0].abs();
            }
            print!("{:>4.0}s ", t as f32 * 0.2);
            for (k, r) in rows {
                let n = r.0 as f32;
                print!("| {k} {} stride {:.2} energy {:.2} matter {:.2} power {:.2} ", r.0, r.1 / n, r.2 / n, r.3 / n, r.4 / n);
            }
            println!();
        }
        sim.step();
        if t == 100 {
            for c in sim.world().creatures.iter().filter(|c| c.abil.locomotion == Some(vesper_sim::history::Locomotion::Walk)).take(10) {
                let v = (c.vel[0] * c.vel[0] + c.vel[1] * c.vel[1]).sqrt() / c.length();
                for (flow, power) in [(0.0, 1.0), (0.0, -1.0), (-0.004, 1.0), (0.004, 1.0), (0.0, 0.5)] {
                    let mut rig = c.rig.clone();
                    let start = rig.centre()[0];
                    let m = vesper_sim::physics::Medium { density: vesper_sim::physics::AIR, flow: [flow, 0.0], ceiling: 0.68 };
                    for _ in 0..300 {
                        rig.step(vesper_sim::physics::Drive { power, rise: 0.0, hz: c.genome.gait_hz, limp: false }, m);
                    }
                    let alone = (rig.centre()[0] - start) / 10.0 / c.length();
                    print!("   alone flow {flow} power {power}: {alone:+.2} len/s");
                }
                println!(" | back {:+.2}", c.back);
                println!("   walker power {:+.2} speed {:.2} len/s stride {:+.2} maturity {:.2} parts {} grounded {} action {:?}", c.drive[0], v, c.stride, c.maturity, c.rig.parts.len(), c.rig.grounded, c.action);
            }
        }
    }
}
