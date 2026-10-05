//! The recorder's NAMED LIST REST events (#2971 / #2758), split from
//! witness.rs for the file budget.
//!
//! `[h, ..t]` in a match arm materializes `t` as a fresh list block
//! (patterns.rs `emit_pattern_binds`) that neither the local's rebind nor the
//! epilogue releases: the ARM releases it once its body has run
//! (`release_arm_rests`). The local is therefore a holder that is not an
//! owner — no dec-old, no iteration-end release — whose one credit the
//! arm's explicit `$dec` settles.

use super::paths::Ev;
use super::WitnessRecorder;

impl WitnessRecorder {
    /// The rest block was built into `local`: a new object it holds (`i`).
    pub fn rest_born(&mut self, local: u32) {
        let o = self.fresh_obj(local, false);
        self.ops(o, "i");
    }

    /// The arm released the rest (`d`). A path that left the arm since the
    /// block was built (a `!` exit, a `return_call`, a loop jump) skipped
    /// that release and still holds it: the frame declines rather than
    /// certify a block no route frees on that path.
    pub fn rest_release(&mut self, local: u32) {
        let born = self.log.iter().rposition(|e| matches!(e, Ev::Bind { local: l, .. } if *l == local));
        if born.is_some_and(|i| self.log[i..].iter().any(|e| matches!(e, Ev::Jump | Ev::Exit))) {
            self.decline("pattern:list-rest:early-exit");
        } else if !self.held_ops(local, "d") {
            self.poison();
        }
    }
}
