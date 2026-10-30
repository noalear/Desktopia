//! Who starves and why: per way of moving, energy, effort, food in reach.
use std::collections::BTreeMap;
use vesper_sim::Sim;
use vesper_sim::history::Fact;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(600.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    let mut deaths: BTreeMap<String, u32> = BTreeMap::new();
    let mut next = 0.0;
    while sim.time() < secs {
        sim.step();
        for (_, f) in sim.drain_history() {
            if let Fact::Death { cause, species, .. } = &f {
                let k = format!("{} {:?}", species.0, format!("{cause:?}").split([' ', '{']).next().unwrap());
                *deaths.entry(k).or_default() += 1;
            }
        }
        if sim.time() >= next {
            next += 120.0;
            let w = sim.world();
            let mut rows: BTreeMap<String, (u32, f32, f32, f32, f32, f32, BTreeMap<String, u32>)> = BTreeMap::new();
            for c in &w.creatures {
                let key = format!("sp{} {:?}", c.species.0, c.motion.mode);
                let r = rows.entry(key).or_default();
                r.0 += 1;
                r.1 += c.energy / c.abil.energy_cap.max(1e-9);
                r.2 += c.drive[0].abs();
                r.3 += c.motion.speed.abs() / vesper_sim::motion::top(&c.abil, c.motion.mode).max(1e-6);
                r.4 += c.maturity;
                r.5 += c.pos[2];
                *r.6.entry(format!("{:?}", c.action)).or_default() += 1;
            }
            println!("t {:.0}s creatures {}", sim.time(), w.creatures.len());
            for r in w.species.list.iter().filter(|r| r.extinct.is_none()) {
                if let vesper_sim::species::Rep::Creature(g) = &r.rep {
                    let mods: Vec<String> = g
                        .plan
                        .modules
                        .iter()
                        .map(|m| format!("{:?}{}x{}", m.kind, if m.paired { "p" } else { "" }, m.count))
                        .collect();
                    let members: Vec<_> = w.creatures.iter().filter(|c| c.species == r.id).collect();
                    let n = members.len().max(1) as f32;
                    let avg = |f: &dyn Fn(&vesper_sim::creature::Creature) -> f32| {
                        members.iter().map(|c| f(c)).sum::<f32>() / n
                    };
                    println!(
                        "    sp{} alive {} {:?}/{:?} diet {:.2} scale {:.4} walk {:.3} swim {:.3} fly {:.3} buoy {:.2} bite {:.2} armour {:.2} | {}",
                        r.id.0,
                        r.alive,
                        g.plan.trunk.tissue,
                        g.plan.trunk.form,
                        g.diet,
                        g.adult_scale,
                        avg(&|c| c.abil.walk),
                        avg(&|c| c.abil.swim),
                        avg(&|c| c.abil.fly),
                        avg(&|c| c.abil.buoyancy),
                        avg(&|c| c.abil.strike),
                        avg(&|c| c.abil.armor),
                        mods.join(" ")
                    );
                }
            }
            for (k, r) in rows {
                let n = r.0 as f32;
                println!(
                    "  {k:<16} n {:>3} energy {:.2} power {:.2} pace {:.2} maturity {:.2} alt {:.3} {:?}",
                    r.0,
                    r.1 / n,
                    r.2 / n,
                    r.3 / n,
                    r.4 / n,
                    r.5 / n,
                    r.6
                );
            }
        }
    }
    println!("deaths: {deaths:?}");
}
