//! #4644: the scaling judge the map/set remove and delete gates share.
//!
//! A wall-clock 4n/n ratio from two separate runs is not a property of the
//! code under load: a busy runner's cache contention slows the 4n collection
//! SYSTEMATICALLY (a constant-time remove read 2.5–4.4 in half of 50 loaded
//! rounds). Dividing by a lookup's growth on the same collection hid it, but
//! also hid a linear op whose access is sequential: the lookup's size effect
//! is the random-access one, so a linear control read 2.43 on CI (#4734).
//!
//! The judge now runs both sizes in ONE process, worked in alternating timed
//! chunks of equal op count (the program prints both per-op times): the two
//! collections see the same load at the same instant and each other's cache
//! pressure, so contention cancels in the ratio instead of being divided out,
//! and nothing is divided by a reference with its own size effect.

/// Runs per measurement; each size keeps its best.
pub const ROUNDS: usize = 5;

/// One scaling reading: the best per-op time at `n` and at `4n`.
pub struct Scaling {
    pub small: f64,
    pub large: f64,
}

impl Scaling {
    /// The judged ratio: the per-op time at 4n over the per-op time at n.
    pub fn ratio(&self) -> f64 {
        self.large / self.small.max(1.0)
    }
    pub fn describe(&self, n: u64) -> String {
        format!("{:.0} ns at {n}, {:.0} ns at {} — ratio {:.2}", self.small, self.large, 4 * n, self.ratio())
    }
}

/// [`ROUNDS`] runs of `at`, each returning (per-op ns at n, per-op ns at 4n)
/// measured in that one run; each size keeps its best.
pub fn scaling(mut at: impl FnMut() -> (f64, f64)) -> Scaling {
    let mut s = Scaling { small: f64::INFINITY, large: f64::INFINITY };
    for _ in 0..ROUNDS {
        let (small, large) = at();
        s.small = s.small.min(small);
        s.large = s.large.min(large);
    }
    s
}
