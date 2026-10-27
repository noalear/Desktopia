//! A uniform bucket grid over x, depth and altitude for finding neighbours.

use crate::world::{ALT_MAX, DEPTH};
use vesper_terrain::WORLD_W;

pub const CELL: f32 = 0.03;
const DEPTH_CELL: f32 = DEPTH / 4.0;
const ALT_CELL: f32 = 0.08;

#[derive(Clone, Debug, Default)]
pub struct Grid {
    nx: usize,
    nd: usize,
    na: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    pub fn clear(&mut self) {
        if self.cells.is_empty() {
            self.nx = (WORLD_W / CELL).ceil() as usize + 1;
            self.nd = (DEPTH / DEPTH_CELL).ceil() as usize;
            self.na = (ALT_MAX / ALT_CELL).ceil() as usize + 1;
            self.cells = vec![Vec::new(); self.nx * self.nd * self.na];
        }
        for c in &mut self.cells {
            c.clear();
        }
    }

    fn cell(&self, p: [f32; 3]) -> [usize; 3] {
        [
            ((p[0] / CELL).max(0.0) as usize).min(self.nx - 1),
            ((p[1] / DEPTH_CELL).max(0.0) as usize).min(self.nd - 1),
            ((p[2] / ALT_CELL).max(0.0) as usize).min(self.na - 1),
        ]
    }

    pub fn insert(&mut self, i: usize, p: [f32; 3]) {
        let [x, d, a] = self.cell(p);
        self.cells[(x * self.nd + d) * self.na + a].push(i as u32);
    }

    /// Like `near`, but at every altitude.
    pub fn near_any_alt(&self, p: [f32; 3], r: f32, mut f: impl FnMut(usize)) {
        if self.cells.is_empty() {
            return;
        }
        let lo = self.cell([p[0] - r, p[1] - r, 0.0]);
        let hi = self.cell([p[0] + r, p[1] + r, 0.0]);
        for x in lo[0]..=hi[0] {
            for d in lo[1]..=hi[1] {
                let row = (x * self.nd + d) * self.na;
                for a in 0..self.na {
                    for &i in &self.cells[row + a] {
                        f(i as usize);
                    }
                }
            }
        }
    }

    /// Calls `f` with every index whose cell lies within `r` of the point.
    pub fn near(&self, p: [f32; 3], r: f32, mut f: impl FnMut(usize)) {
        if self.cells.is_empty() {
            return;
        }
        let lo = self.cell([p[0] - r, p[1] - r, p[2] - r]);
        let hi = self.cell([p[0] + r, p[1] + r, p[2] + r]);
        for x in lo[0]..=hi[0] {
            for d in lo[1]..=hi[1] {
                let row = (x * self.nd + d) * self.na;
                for a in lo[2]..=hi[2] {
                    for &i in &self.cells[row + a] {
                        f(i as usize);
                    }
                }
            }
        }
    }
}
