//! A small deterministic random number generator that saves with the world,
//! so a loaded world carries on exactly as it would have.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Self { state: seed ^ 0x9E37_79B9_7F4A_7C15 };
        r.next_u64();
        r
    }

    /// A new generator whose stream doesn't overlap this one's in practice.
    pub fn fork(&mut self) -> Self {
        Self::new(self.next_u64())
    }

    /// SplitMix64.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Uniform index in 0..n. n must be above zero.
    pub fn below(&mut self, n: usize) -> usize {
        ((self.next_u64() >> 32) * n as u64 >> 32) as usize
    }

    /// Roughly normal, mean 0 and deviation 1 (sum of uniforms; cheap and bounded).
    pub fn normal(&mut self) -> f32 {
        let s = self.f32() + self.f32() + self.f32() + self.f32();
        (s - 2.0) * 1.732
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Mixes a value into a hash, for stable per-entity variety.
pub fn hash(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub fn hash01(x: u64) -> f32 {
    (hash(x) >> 40) as f32 / (1u64 << 24) as f32
}
