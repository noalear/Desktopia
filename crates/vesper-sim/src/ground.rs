//! A per-column table of the ground and river, so creatures don't evaluate
//! terrain noise every tick. It is rebuilt when the river moves enough to
//! matter, and matches `vesper_terrain` to well under a body length.

use vesper_terrain::{self as terrain, RiverState, WORLD_W};

const N: usize = 1024;
const STEP: f32 = WORLD_W / (N - 1) as f32;

#[derive(Clone, Debug, Default)]
pub struct Ground {
    /// Distance in metres to the back of the ground plane.
    back: Vec<f32>,
    /// River centre and half-width.
    river: Vec<[f32; 2]>,
    /// The river state the table was built for, rounded so that a world
    /// loaded from a save builds exactly the same table as one that ran on.
    state: Option<(i32, i32)>,
}

impl Ground {
    /// Brings the river up to date; cheap when it hasn't moved.
    pub fn refresh(&mut self, s: RiverState) {
        if self.back.is_empty() {
            self.back = (0..N).map(|i| terrain::back_z(i as f32 * STEP)).collect();
        }
        let key = ((s.phase / 2e-4).round() as i32, (s.flood / 2e-3).round() as i32);
        if self.state == Some(key) {
            return;
        }
        let s = RiverState { phase: key.0 as f32 * 2e-4, flood: key.1 as f32 * 2e-3 };
        self.river = (0..N).map(|i| terrain::river(i as f32 * STEP, s)).collect();
        self.state = Some(key);
    }

    fn at(x: f32) -> (usize, f32) {
        let f = (x / STEP).clamp(0.0, (N - 1) as f32 - 1e-3);
        (f as usize, f.fract())
    }

    /// Screen h of a point on the ground plane, depth 0..1.
    pub fn floor_h(&self, x: f32, depth: f32) -> f32 {
        let (i, t) = Self::at(x);
        terrain::floor_h_in(self.back[i] + (self.back[i + 1] - self.back[i]) * t, depth)
    }

    fn river_at(&self, x: f32) -> [f32; 2] {
        let (i, t) = Self::at(x);
        let [a, b] = [self.river[i], self.river[i + 1]];
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    /// As `vesper_terrain::wetness`.
    pub fn wetness(&self, x: f32, depth: f32) -> f32 {
        let [c, w] = self.river_at(x);
        if w <= 0.0 {
            return 0.0;
        }
        let d = (self.floor_h(x, depth) - c).abs();
        terrain::smoothstep(w * 3.5, w * 0.8, d)
    }

    /// As `vesper_terrain::in_river`.
    pub fn in_river(&self, x: f32, depth: f32) -> bool {
        let [c, w] = self.river_at(x);
        let h = self.floor_h(x, depth);
        w > 0.0 && (h - c).abs() < w && terrain::pan_wet(x, h) > 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_terrain() {
        let s = RiverState { phase: 0.7, flood: 0.3 };
        let mut g = Ground::default();
        g.refresh(s);
        let mut wrong = 0;
        for i in 0..4000 {
            let x = i as f32 / 4000.0 * WORLD_W;
            for d in [0.1, 0.3, 0.45, 0.6, 0.9] {
                assert!((g.wetness(x, d) - terrain::wetness(x, d, s)).abs() < 0.05);
                wrong += (g.in_river(x, d) != terrain::in_river(x, d, s)) as u32;
            }
        }
        assert!(wrong < 20, "{wrong} river cells differ");
    }
}
