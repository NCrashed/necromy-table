//! Seeded RNG owned by the game state.
//!
//! SplitMix64: tiny, fast and bit-for-bit identical on every platform, which
//! is what replays and a server/client split need. Not for cryptography.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// An independent stream derived from a seed and a label, e.g. one per
    /// bot turn, so that consuming it never shifts the game's own stream.
    pub fn derived(seed: u64, label: &[u64]) -> Self {
        let mut rng = Self::new(seed);
        for &l in label {
            rng.state ^= l.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            rng.next_u64();
        }
        rng
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n`. `n` must be non-zero.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "Rng::below(0)");
        // Lemire's multiply-shift; the bias is negligible for board-game sizes.
        (((self.next_u64() >> 32) * u64::from(n)) >> 32) as u32
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len() as u32) as usize])
        }
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(7);
        for n in 1..50 {
            for _ in 0..50 {
                assert!(rng.below(n) < n);
            }
        }
    }
}
