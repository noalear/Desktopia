//! What a saved world's life has become: per species, the body it grows,
//! the organs it has and what they do.
//!
//! cargo run --release -p vesper-sim --example census -- WORLD_FILE
use vesper_sim::organ::{Fare, Organ};

fn main() {
    let path = std::env::args().nth(1).expect("world file");
    let sim = vesper_sim::save::load(std::path::Path::new(&path)).expect("load");
    let w = sim.world();
    println!("time {:.1} h, creatures {}, plants {}", sim.time() / 3600.0, w.creatures.len(), w.plants.len());
    let mut by: std::collections::BTreeMap<u64, Vec<&vesper_sim::creature::Creature>> = Default::default();
    for c in &w.creatures {
        by.entry(c.species.0).or_default().push(c);
    }
    for (id, cs) in by {
        let n = cs.len() as f32;
        let mean = |f: &dyn Fn(&vesper_sim::creature::Creature) -> f32| cs.iter().map(|c| f(c)).sum::<f32>() / n;
        let mut loco: std::collections::BTreeMap<String, u32> = Default::default();
        for c in &cs {
            *loco.entry(format!("{:?}", c.abil.locomotion)).or_default() += 1;
        }
        println!(
            "sp {id} n {} gen {} seg {:.1} len {:.4} diet {:.2} | {:?}",
            cs.len(),
            cs.iter().map(|c| c.generation).max().unwrap_or(0),
            mean(&|c| c.body.segments.len() as f32),
            mean(&|c| c.length()),
            mean(&|c| c.genome.diet),
            loco
        );
        let mut organs: Vec<(f32, Organ)> =
            Organ::ALL.iter().map(|&o| (mean(&|c| c.pheno.organ(o)), o)).filter(|x| x.0 > 0.005).collect();
        organs.sort_by(|a, b| b.0.total_cmp(&a.0));
        let t: Vec<String> = organs.iter().map(|(v, o)| format!("{o:?} {v:.2}")).collect();
        println!("    organs {}", t.join(" "));
        println!(
            "    strike {:.4} leaf {:.5} fruit {:.5} flesh {:.5} gut {:.5} | sight {:.3} smell {:.3} hear {:.3} touch {:.3} | armor {:.2} spines {:.2} venom {:.2} | consp {:.2} orn {:.2} | cold {:.2} hot {:.2} | speed {:.4} legs {:.1} buoy {:.2}",
            mean(&|c| c.abil.strike),
            mean(&|c| c.intake(Fare::Leaf, 1.0)),
            mean(&|c| c.intake(Fare::Fruit, 1.0)),
            mean(&|c| c.intake(Fare::Flesh, 1.0)),
            mean(&|c| c.abil.gut),
            mean(&|c| c.abil.sight),
            mean(&|c| c.abil.smell),
            mean(&|c| c.abil.hearing),
            mean(&|c| c.abil.touch),
            mean(&|c| c.abil.armor),
            mean(&|c| c.abil.spines),
            mean(&|c| c.abil.venom),
            mean(&|c| c.abil.conspicuous),
            mean(&|c| c.abil.ornament),
            mean(&|c| c.bearable().0),
            mean(&|c| c.bearable().1),
            mean(&|c| c.abil.speed),
            mean(&|c| c.abil.legs as f32),
            mean(&|c| c.abil.buoyancy),
        );
        let c = cs[0];
        let segs: Vec<String> = c
            .body
            .segments
            .iter()
            .map(|s| {
                format!(
                    "{}{:?} {:.2}x{:.2}",
                    if s.side != 0 { "2" } else { "" },
                    Organ::of(s.tissue, s.form),
                    s.length,
                    s.width
                )
            })
            .collect();
        println!("    eg: {}", segs.join(", "));
    }
    let mut by: std::collections::BTreeMap<u64, Vec<&vesper_sim::plant::Plant>> = Default::default();
    for p in &w.plants {
        by.entry(p.species.0).or_default().push(p);
    }
    for (id, ps) in by {
        let n = ps.len() as f32;
        let m = |f: &dyn Fn(&vesper_sim::plant::PlantGenome) -> f32| ps.iter().map(|p| f(&p.genome)).sum::<f32>() / n;
        let sd = |f: &dyn Fn(&vesper_sim::plant::PlantGenome) -> f32| {
            let mu = m(f);
            (ps.iter().map(|p| (f(&p.genome) - mu).powi(2)).sum::<f32>() / n).sqrt()
        };
        println!(
            "plant {id} n {} gen {} tough {:.2}±{:.2} scale {:.4}±{:.4} stem {:.1} br {:.1} lv {:.1} leaf {:.2} width {:.2}±{:.2} split {:.2} knots {:.2} spines {:.2} tox {:.2} trap {:.2} fruit {:.2} nectar {:.2} wind {:.2} vent {:.2} thirst {:.2} float {:.2} x {:.2} hue {:.2}",
            ps.len(),
            ps.iter().map(|p| p.generation).max().unwrap_or(0),
            m(&|g| g.tough),
            sd(&|g| g.tough),
            m(&|g| g.adult_scale),
            sd(&|g| g.adult_scale),
            m(&|g| g.stem_segments as f32),
            m(&|g| g.branches as f32),
            m(&|g| g.leaves as f32),
            m(&|g| g.leaf),
            m(&|g| g.leaf_width),
            sd(&|g| g.leaf_width),
            m(&|g| g.leaf_split),
            m(&|g| g.knots),
            m(&|g| g.spines),
            m(&|g| g.toxin),
            m(&|g| g.trap),
            m(&|g| g.fruit),
            m(&|g| g.nectar),
            m(&|g| g.wind),
            m(&|g| g.vent),
            m(&|g| g.thirst),
            m(&|g| g.floating),
            ps.iter().map(|p| p.x).sum::<f32>() / n,
            m(&|g| g.look[0]),
        );
    }
}
