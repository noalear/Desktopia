//! `--weather HOURS`: runs only a real world's weather (no life) and draws
//! the sky it makes, for checking how weather looks without growing a world.
//! `--until WHAT` then keeps going until something worth seeing is happening:
//! storm, rain, snow, dust, cold, erupt, drought or flood.

use vesper_render::WorldView;
use vesper_sim::{Sim, world};

pub struct Weather {
    sim: Sim,
}

impl Weather {
    pub fn grow(seed: u64, hours: f64, until: Option<&str>) -> Self {
        let mut w = Self { sim: Sim::new(seed, 1_800_000_000.0) };
        w.step(hours * 3600.0);
        if let Some(what) = until {
            let give_up = w.sim.time() + 6.0 * 86400.0;
            while !w.showing(what) && w.sim.time() < give_up {
                w.step(60.0);
            }
            eprintln!("weather: {what} at {:.1} h", w.sim.time() / 3600.0);
            for c in &w.sim.world().env.atmos.cells {
                let a = &w.sim.world().env.atmos;
                let i = ((c.x / vesper_terrain::WORLD_W * a.precip.len() as f32) as usize).min(a.precip.len() - 1);
                eprintln!(
                    "  cell at {:.2}: power {:.2}, {:.0}% through its life, precip under it {:.2}",
                    c.x,
                    c.power,
                    c.life01() * 100.0,
                    a.precip[i]
                );
            }
        }
        w
    }

    pub fn step(&mut self, secs: f64) {
        let w = self.sim.world_mut();
        let end = w.time + secs;
        while w.time < end {
            w.time += 1.0;
            w.ground.refresh(w.env.river);
            world::weather(w, 1.0);
            w.history.clear();
        }
    }

    fn showing(&self, what: &str) -> bool {
        let e = &self.sim.world().env;
        let a = &e.atmos;
        let n = a.precip.len();
        let x_of = |i: usize| (i as f32 + 0.5) / n as f32 * vesper_terrain::WORLD_W;
        let falling = |lo: f32, hi: f32, snow: bool| {
            (0..n).any(|i| (lo..hi).contains(&x_of(i)) && a.precip[i] > 0.35 && (a.snowy[i] > 0.5) == snow)
        };
        match what {
            "storm" => a.cells.iter().any(|c| c.power > 0.8 && (0.9..2.7).contains(&c.x) && c.life01() > 0.35),
            "daystorm" => a.cells.iter().any(|c| c.power > 0.7 && c.x < 1.5 && c.life01() > 0.35),
            "rain" => falling(1.0, 2.4, false),
            "snow" => falling(2.2, 3.4, true),
            "dust" => e.dust > 0.5 && (0.4..3.0).contains(&e.dust_x),
            "cold" => e.cold > 0.5 && e.cold_x < 2.6,
            "erupt" => e.eruption > 0.6,
            "drought" => a.dry_for > 6.0 * 3600.0,
            "flood" => e.river.flood > 0.6,
            other => panic!("--until: unknown weather {other}"),
        }
    }

    /// Copies the weather into `view`, as the wallpaper does.
    pub fn fill(&self, view: &mut WorldView) {
        let e = &self.sim.world().env;
        view.band_shift = e.band_shift;
        view.storm = e.storm;
        view.storm_x = e.storm_x;
        view.lightning = e.lightning;
        view.lightning_x = e.lightning_x;
        view.volcano = e.eruption;
        view.lava = e.lava;
        view.ash = e.ash;
        view.river_phase = e.river.phase;
        view.river_flood = e.river.flood;
        view.cold = e.cold;
        view.cold_x = e.cold_x;
        view.dust = e.dust;
        view.dust_x = e.dust_x;
        view.wind_low = e.wind(0.0);
        view.wind_high = e.wind(0.5);
        view.sky = vesper_render::Sky::from_rows(&e.atmos.sky(if e.cold > 0.0 { -0.0012 } else { 0.0 }, e.dust_v));
    }
}
