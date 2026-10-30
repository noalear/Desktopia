//! How the founding populations fare in their first minutes, by way of moving.
use vesper_sim::Sim;
fn main() {
    let mut sim = Sim::new(1, 1_800_000_000.0);
    for t in 0..=1800 {
        if t % 300 == 0 {
            let w = sim.world();
            let mut rows: std::collections::BTreeMap<String, (u32, f32, f32, f32, f32)> = Default::default();
            for c in &w.creatures {
                let r = rows.entry(format!("{:?}", c.motion.mode)).or_default();
                r.0 += 1;
                r.1 += c.motion.speed.abs() / c.length().max(1e-6);
                r.2 += c.energy / c.abil.energy_cap.max(1e-9);
                r.3 += c.matter / c.abil.body_matter.max(1e-9);
                r.4 += c.drive[0].abs();
            }
            print!("{:>4.0}s ", t as f32 * 0.2);
            for (k, r) in rows {
                let n = r.0 as f32;
                print!(
                    "| {k} {} speed {:.2} len/s energy {:.2} matter {:.2} power {:.2} ",
                    r.0,
                    r.1 / n,
                    r.2 / n,
                    r.3 / n,
                    r.4 / n
                );
            }
            println!();
        }
        sim.step();
    }
}
