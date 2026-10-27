//! A uniform bucket grid over x and altitude for finding neighbours.

use crate::world::ALT_MAX;
use vesper_terrain::WORLD_W;

pub const CELL: f32 = 0.06;
const ALT_CELL: f32 = 0.1;

#[derive(Clone, Debug, Default)]
pub struct Grid {
    nx: usize,
    na: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    pub fn clear(&mut self) {
        if self.cells.is_empty() {
            self.nx = (WORLD_W / CELL).ceil() as usize + 1;
            self.na = (ALT_MAX / ALT_CELL).ceil() as usize + 1;
            self.cells = vec![Vec::new(); self.nx * self.na];
        }
        for c in &mut self.cells {
            c.clear();
        }
    }

    fn cell(&self, x: f32, alt: f32) -> (usize, usize) {
        let cx = ((x / CELL).max(0.0) as usize).min(self.nx - 1);
        let ca = ((alt / ALT_CELL).max(0.0) as usize).min(self.na - 1);
        (cx, ca)
    }

    pub fn insert(&mut self, i: usize, x: f32, alt: f32) {
        let (cx, ca) = self.cell(x, alt);
        self.cells[cx * self.na + ca].push(i as u32);
    }

    /// Calls `f` with every index whose cell lies within `r` of the point.
    pub fn near(&self, x: f32, alt: f32, r: f32, mut f: impl FnMut(usize)) {
        if self.cells.is_empty() {
            return;
        }
        let (x0, a0) = self.cell(x - r, alt - r);
        let (x1, a1) = self.cell(x + r, alt + r);
        for cx in x0..=x1 {
            for ca in a0..=a1 {
                for &i in &self.cells[cx * self.na + ca] {
                    f(i as usize);
                }
            }
        }
    }
}
