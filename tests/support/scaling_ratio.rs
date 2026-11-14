//! #4644: the scaling judge the map/set remove and delete gates share.
//!
//! A wall-clock 4n/n ratio is not a property of the code under load. A busy
//! runner's cache contention slows the 4n collection SYSTEMATICALLY (a
//! constant-time remove read 2.5–4.4 in half of 50 loaded rounds, and one
//! linear control read 2.16 the other way), so no minimum filters it out.
//! The judged ratio divides the operation's growth by the growth of a
//! constant-time lookup timed on the same collection in the same process:
//! the contention both see cancels, and a linear operation still reads ~4.

/// Rounds per size.
pub const ROUNDS: usize = 5;

/// One scaling reading (#4644): the best per-op time of the operation and of
/// the constant-time reference at `n` and at `4n`.
pub struct Scaling {
    pub op: (f64, f64),
    pub reference: (f64, f64),
}

impl Scaling {
    pub fn op_ratio(&self) -> f64 {
        self.op.1 / self.op.0.max(1.0)
    }
    pub fn reference_ratio(&self) -> f64 {
        self.reference.1 / self.reference.0.max(1.0)
    }
    /// The judged ratio: the operation's growth from n to 4n over the
    /// reference's growth on the same collection in the same runs.
    pub fn normalized(&self) -> f64 {
        self.op_ratio() / self.reference_ratio().max(f64::MIN_POSITIVE)
    }
    pub fn describe(&self, n: u64) -> String {
        format!(
            "{:.0} ns at {n}, {:.0} ns at {} — ratio {:.2} (reference lookup {:.2}), normalized {:.2}",
            self.op.0,
            self.op.1,
            4 * n,
            self.op_ratio(),
            self.reference_ratio(),
            self.normalized()
        )
    }
}

/// The two sizes alternate, each keeping its best of [`ROUNDS`] runs per
/// measure; `at(k)` returns (operation, reference) per-op nanoseconds at k.
pub fn scaling(mut at: impl FnMut(u64) -> (f64, f64), n: u64) -> Scaling {
    let mut s = Scaling { op: (f64::INFINITY, f64::INFINITY), reference: (f64::INFINITY, f64::INFINITY) };
    for _ in 0..ROUNDS {
        let (op, r) = at(n);
        s.op.0 = s.op.0.min(op);
        s.reference.0 = s.reference.0.min(r);
        let (op, r) = at(4 * n);
        s.op.1 = s.op.1.min(op);
        s.reference.1 = s.reference.1.min(r);
    }
    s
}
