//! Deterministic pseudo-random source.
//!
//! Every generated program is fully reproducible from `(seed, index)`:
//! the driver seeds one `SplitMix64` per program as
//! `SplitMix64::new(seed ^ mix(index))`, so a finding can be replayed
//! byte-for-byte from the two integers recorded alongside it. There is
//! no wall-clock, thread, or OS-entropy input anywhere in the pipeline.
//!
//! SplitMix64 is chosen over a larger generator (xoshiro, PCG) on
//! purpose: it is a single 64-bit state word with a closed-form `next`,
//! which keeps the "replay from a seed" contract trivially auditable
//! and has no platform-dependent behaviour.

/// A small, fast, fully deterministic 64-bit generator (Steele,
/// Lea & Flood — "Fast Splittable Pseudorandom Number Generators").
#[derive(Clone)]
pub struct SplitMix64 {
    state: u64,
}

/// The SplitMix64 increment (golden-ratio odd constant). Mixing it into
/// the state each step is what gives the generator its full period.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// First avalanche multiplier from the reference finalizer.
const MIX_MULT_1: u64 = 0xBF58_476D_1CE4_E5B9;
/// Second avalanche multiplier from the reference finalizer.
const MIX_MULT_2: u64 = 0x94D0_49BB_1331_11EB;

impl SplitMix64 {
    /// Seed the generator. Any seed (including zero) yields a full-period
    /// stream — SplitMix64 has no weak seeds.
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Derive a fresh, well-separated sub-stream for program `index`
    /// from a campaign `seed`. Running the finalizer over the index
    /// before XOR-ing decorrelates adjacent indices, so program N and
    /// program N+1 do not share early structure.
    pub fn for_program(seed: u64, index: u64) -> Self {
        SplitMix64::new(seed ^ finalize(index.wrapping_mul(GOLDEN_GAMMA)))
    }

    /// Advance the state and return the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        finalize(self.state)
    }

    /// Uniform `u32` in `[0, bound)`. `bound` must be non-zero.
    /// Lemire's nearly-divisionless rejection keeps the distribution
    /// unbiased without a modulo on the hot path.
    pub fn below(&mut self, bound: u32) -> u32 {
        debug_assert!(bound != 0, "below() bound must be non-zero");
        let mut x = self.next_u64() as u32;
        let mut m = (x as u64).wrapping_mul(bound as u64);
        let mut lo = m as u32;
        if lo < bound {
            // Rejection threshold = -bound mod bound, computed without %.
            let threshold = bound.wrapping_neg() % bound;
            while lo < threshold {
                x = self.next_u64() as u32;
                m = (x as u64).wrapping_mul(bound as u64);
                lo = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// Inclusive integer range `[lo, hi]`.
    pub fn in_range(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(lo <= hi, "in_range requires lo <= hi");
        let span = (hi - lo) as u64;
        if span == u64::MAX {
            return self.next_u64() as i64;
        }
        lo + self.below_u64(span + 1) as i64
    }

    /// Uniform `u64` in `[0, bound)`. `bound` must be non-zero.
    fn below_u64(&mut self, bound: u64) -> u64 {
        // 128-bit Lemire multiply-shift.
        let mut x = self.next_u64();
        let mut m = (x as u128).wrapping_mul(bound as u128);
        let mut lo = m as u64;
        if lo < bound {
            let threshold = bound.wrapping_neg() % bound;
            while lo < threshold {
                x = self.next_u64();
                m = (x as u128).wrapping_mul(bound as u128);
                lo = m as u64;
            }
        }
        (m >> 64) as u64
    }

    /// `true` with probability `numerator / denominator`.
    pub fn chance(&mut self, numerator: u32, denominator: u32) -> bool {
        debug_assert!(numerator <= denominator && denominator != 0);
        self.below(denominator) < numerator
    }

    /// Pick a reference from a non-empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        debug_assert!(!items.is_empty(), "pick() on empty slice");
        &items[self.below(items.len() as u32) as usize]
    }

    /// Pick an index from a weighted table. Each entry's weight is its
    /// relative selection probability; weights need not sum to any
    /// particular total. Returns the chosen index into `weights`.
    pub fn pick_weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        debug_assert!(total != 0, "pick_weighted with all-zero weights");
        let mut roll = self.below(total);
        for (i, &w) in weights.iter().enumerate() {
            if roll < w {
                return i;
            }
            roll -= w;
        }
        weights.len() - 1
    }
}

/// SplitMix64 finalizer (the avalanche mixing function). Exposed at
/// module scope so `for_program` can decorrelate the index seed with
/// the exact same mixing the stream uses.
fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(MIX_MULT_1);
    z = (z ^ (z >> 27)).wrapping_mul(MIX_MULT_2);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn program_streams_are_reproducible() {
        let mut a = SplitMix64::for_program(7, 123);
        let mut b = SplitMix64::for_program(7, 123);
        assert_eq!(a.next_u64(), b.next_u64());
        // Different index ⇒ different early output (decorrelated).
        let mut c = SplitMix64::for_program(7, 124);
        assert_ne!(
            SplitMix64::for_program(7, 123).next_u64(),
            c.next_u64()
        );
    }

    #[test]
    fn below_is_in_bounds() {
        let mut r = SplitMix64::new(1);
        for bound in [1u32, 2, 3, 7, 100, 1000] {
            for _ in 0..200 {
                assert!(r.below(bound) < bound);
            }
        }
    }

    #[test]
    fn weighted_respects_zero_weight() {
        let mut r = SplitMix64::new(9);
        // Middle option has zero weight ⇒ never selected.
        let weights = [3, 0, 3];
        for _ in 0..500 {
            assert_ne!(r.pick_weighted(&weights), 1);
        }
    }
}
