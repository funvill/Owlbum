//! A seeded generator with no dependency, so the same seed gives the same
//! album on every system and with every version of every crate.

/// SplitMix64. Small, fast and fully specified.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// A generator for item `index` of the stream `seed`, independent of
    /// how many numbers earlier items used.
    pub fn for_item(seed: u64, index: u64) -> Self {
        let mut r = Rng(seed ^ index.wrapping_mul(0xa076_1d64_78bd_642f));
        r.next_u64();
        r
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `lo..hi` (`hi` excluded). `lo < hi`.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        debug_assert!(lo < hi);
        lo + (self.next_u64() % u64::from(hi - lo)) as u32
    }

    /// True `percent` times in a hundred.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.range(0, 100) < percent
    }

    pub fn byte(&mut self) -> u8 {
        (self.next_u64() >> 56) as u8
    }

    pub fn colour(&mut self) -> [u8; 3] {
        [self.byte(), self.byte(), self.byte()]
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for b in out {
            *b = self.byte();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        // SplitMix64's published first output for seed 0.
        assert_eq!(Rng::new(0).next_u64(), 0xe220_a839_7b1d_cdaf);
    }

    #[test]
    fn range_stays_in_range() {
        let mut r = Rng::new(9);
        for _ in 0..1000 {
            let v = r.range(5, 9);
            assert!((5..9).contains(&v));
        }
    }

    #[test]
    fn items_differ() {
        assert_ne!(
            Rng::for_item(1, 0).next_u64(),
            Rng::for_item(1, 1).next_u64()
        );
    }
}
