//! Runs only the weather of a world, fast, and prints how it went: a strip
//! per interval across the band (day edge left, night edge right) and a tally
//! of what happened, for tuning the climate.
//!
//! cargo run --release -p vesper-sim --example weather -- [days] [seed] [minutes per line]

use vesper_sim::Sim;
use vesper_sim::climate::{self, N};
use vesper_sim::history::Fact;
use vesper_sim::world;

fn glyph(v: f32, ramp: &[char]) -> char {
    let k = ((v.clamp(0.0, 0.999)) * ramp.len() as f32) as usize;
    ramp[k.min(ramp.len() - 1)]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let days: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let every: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20.0) * 60.0;
    let field = std::env::var("FIELD").unwrap_or_else(|_| "precip".into());
    let mut sim = Sim::new(seed, 1_800_000_000.0);
    let mut tally: std::collections::BTreeMap<String, u32> = Default::default();
    let mut next = 0.0;
    let ramp = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let mut sums = [0.0f64; 8];
    let mut samples = 0.0;
    let mut cells_born = 0usize;
    println!("{field} strip, day edge left; then: cells, storm, flood, twilight moisture, anomaly, veil, era");
    while sim.time() < days * 86400.0 {
        let w = sim.world_mut();
        w.time += 1.0;
        w.ground.refresh(w.env.river);
        world::weather(w, 1.0);
        for (_, f) in w.history.drain(..) {
            if let Fact::Nature { kind, .. } = f {
                *tally.entry(format!("{kind:?}")).or_default() += 1;
            }
        }
        let a = &w.env.atmos;
        let tw: Vec<usize> = (0..N).filter(|&i| (1.0..2.6).contains(&climate::col_x(i))).collect();
        let m = tw.iter().map(|&i| a.moisture[i]).sum::<f32>() / tw.len() as f32;
        let rain = tw.iter().map(|&i| a.precip[i]).sum::<f32>() / tw.len() as f32;
        sums[0] += m as f64;
        sums[1] += rain as f64;
        sums[2] += (a.cells.len() > 0) as u8 as f64;
        sums[3] += w.env.river.flood as f64;
        sums[4] += tw.iter().filter(|&&i| a.precip[i] > 0.05).count() as f64 / tw.len() as f64;
        sums[5] += a.cells.iter().map(|c| c.power).fold(0.0f32, f32::max) as f64;
        sums[6] += (w.env.atmos.dry_for > 4.0 * 3600.0) as u8 as f64;
        sums[7] += tw.iter().map(|&i| a.anom[i]).sum::<f32>() as f64 / tw.len() as f64;
        let born = a.cells.iter().filter(|c| c.age == 1.0).count();
        cells_born += born;
        samples += 1.0;
        if w.time >= next {
            next += every;
            let strip: String = (0..N)
                .map(|i| {
                    let v = match field.as_str() {
                        "precip" => a.precip[i],
                        "cloud" => a.cloud[i] * 4.0,
                        "moisture" => a.moisture[i],
                        "snow" => a.snow[i] / 0.3,
                        "anom" => 0.5 + a.anom[i],
                        "vapour" => a.vapour[i] / climate::saturation(climate::base(climate::col_x(i), w.env.band_shift)),
                        "fog" => a.fog[i] * 12.0,
                        "dust" => a.dust[i],
                        "ash" => a.ash[i],
                        "gust" => (a.gust[i] - 1.0) / 3.0,
                        _ => 0.0,
                    };
                    glyph(v, &ramp)
                })
                .collect();
            let clouds: String = (0..N).map(|i| glyph(a.cloud[i] * 5.0, &ramp)).collect();
            if std::env::var_os("DEBUG").is_some() {
                for x in [0.4f32, 0.9, 1.4, 1.8, 2.2, 2.6, 3.0, 3.4] {
                    let i = ((x / climate::DX) as usize).min(N - 1);
                    let t = climate::base(x, w.env.band_shift) + climate::offset(a.era, a.veil) + a.anom[i];
                    println!(
                        "    x {x:.1} T {t:+.2} v {:.3}/{:.3} hi {:.3}/{:.3} cloud {:.3} p {:.2} m {:.2} gust {:.2}",
                        a.vapour[i],
                        climate::saturation(t),
                        a.vapour_hi[i],
                        climate::saturation(climate::aloft(t)),
                        a.cloud[i],
                        a.precip[i],
                        a.moisture[i],
                        a.gust[i]
                    );
                }
            }
            let anom = tw.iter().map(|&i| a.anom[i]).sum::<f32>() / tw.len() as f32;
            println!(
                "{:6.2}h |{strip}|{clouds}| {} {:.2} {:.2} {:.2} {:+.2} {:.2} {:+.2}",
                w.time / 3600.0,
                a.cells.len(),
                w.env.storm,
                w.env.river.flood,
                m,
                anom,
                a.veil,
                a.era
            );
        }
    }
    println!(
        "twilight: moisture {:.3}, rain {:.3}, raining {:.1}% of ground-time, drought {:.1}% of time, anomaly {:+.3}",
        sums[0] / samples,
        sums[1] / samples,
        sums[4] / samples * 100.0,
        sums[6] / samples * 100.0,
        sums[7] / samples
    );
    println!(
        "storm cells: {:.1} a day, one up {:.1}% of the time, mean top power {:.2}; flood {:.3}",
        cells_born as f64 / days,
        sums[2] / samples * 100.0,
        sums[5] / samples,
        sums[3] / samples
    );
    for (k, v) in tally {
        println!("{k}: {v}");
    }
}
