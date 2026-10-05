//! The recorder's SCOPED BLOCKS (#2758), split from witness.rs for the file
//! budget: a block a construct takes a credit on and settles itself when the
//! construct ends — a named list rest and a map walk's cursor. A path that
//! leaves a list rest's arm before that settlement still holds the credit,
//! so the frame declines; a map walk's cursor is a frame credit for the
//! walk's duration, so every exit edge releases it (#3374).
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

    /// `for (k, v) in m` (stmts.rs `lower_forin`): the cursor takes its credit
    /// on the subject before the loop — an OWNED subject is the cursor's own
    /// block (`i`), a borrowed one is shared (`a`, the route's `rc_inc_top`,
    /// a holder line of its own beside the block's owner). The cursor local
    /// `local` holds it (not an owner: no dec-old, no iteration-end release),
    /// so the release that settles it is resolved per path through `local`.
    pub fn cursor_take(&mut self, owned: bool, local: u32) {
        let o = self.fresh_obj(local, false);
        self.ops(o, if owned { "i" } else { "a" });
    }

    /// After the loop the cursor releases its credit (`d`). An exit from the
    /// body (a `!`, a guard return) released it on its own edge (#3374: the
    /// walk lends the cursor local to the frame's exit plan for the body's
    /// duration, so the exit's `witness_dec` is that path's `d`), and that
    /// path is dead here.
    pub fn cursor_release(&mut self, local: u32) {
        if !self.held_ops(local, "d") {
            self.poison();
        }
    }
}
