//! Verlet chains for feelers, tentacles and tail filaments, so trailing parts
//! lag, swing and settle under gravity and drag instead of being keyframed.

use crate::{add, len, mul, rot, sub};

pub struct Rope {
    pub pts: Vec<[f32; 2]>,
    prev: Vec<[f32; 2]>,
    pub seg_len: f32,
    /// Downward pull per second squared; negative floats upward.
    pub gravity: f32,
    /// How strongly each link tries to keep pointing along `rest_dir`.
    pub stiffness: f32,
    pub rest_dir: [f32; 2],
    /// Turn per link, radians: feelers arc, tentacles hook.
    pub curl: f32,
    /// How much the rest shape ripples over time.
    pub wave: f32,
    time: f32,
}

impl Rope {
    pub fn new(anchor: [f32; 2], dir: [f32; 2], n: usize, seg_len: f32, gravity: f32, stiffness: f32) -> Self {
        let pts: Vec<_> = (0..=n).map(|i| add(anchor, mul(dir, seg_len * i as f32))).collect();
        Self { prev: pts.clone(), pts, seg_len, gravity, stiffness, rest_dir: dir, curl: 0.0, wave: 0.0, time: 0.0 }
    }

    /// Moves the root to `anchor` and lets the rest follow. `drift` is the
    /// air's own motion, in world units per second.
    pub fn step(&mut self, anchor: [f32; 2], dir: [f32; 2], drift: [f32; 2], dt: f32) {
        self.rest_dir = dir;
        self.time += dt;
        let n = self.pts.len();
        // Thick air: heavy damping, and the chain is carried by the drift.
        let damp = 0.9f32.powf(dt * 30.0);
        for i in 1..n {
            let p = self.pts[i];
            let v = sub(p, self.prev[i]);
            self.prev[i] = p;
            let mut np = add(p, mul(v, damp));
            np = add(np, [drift[0] * dt * 0.3, (drift[1] - self.gravity) * dt * dt * 10.0]);
            self.pts[i] = np;
        }
        self.pts[0] = anchor;
        self.prev[0] = anchor;
        for _ in 0..4 {
            for i in 1..n {
                // Bend back toward the rest direction, more near the root.
                let fi = i as f32;
                let turn = self.curl * fi + self.wave * (self.time * 0.9 - fi * 0.7).sin();
                let want = add(self.pts[i - 1], mul(rot(self.rest_dir, turn), self.seg_len));
                let k = self.stiffness * (1.0 - i as f32 / n as f32 * 0.7);
                self.pts[i] = add(self.pts[i], mul(sub(want, self.pts[i]), k * 0.25));
                // Keep link length.
                let d = sub(self.pts[i], self.pts[i - 1]);
                let l = len(d).max(1e-7);
                self.pts[i] = add(self.pts[i - 1], mul(d, self.seg_len / l));
            }
        }
    }
}
