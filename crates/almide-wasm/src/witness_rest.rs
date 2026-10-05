//! The recorder's SCOPED BLOCKS (#2758), split from witness.rs for the file
//! budget: a block a construct takes a credit on and settles itself when the
//! construct ends — a named list rest and a map walk's cursor. Both are
//! frame credits for the construct's duration, so every edge that leaves it
//! early releases them on that edge (#3374 the cursor, #3377 the rest).
//!
//! `[h, ..t]` in a match arm materializes `t` as a fresh list block
//! (patterns.rs `emit_pattern_binds`) that neither the local's rebind nor the
//! epilogue releases: the ARM releases it once its body has run
//! (arm_rests.rs `release_arm_rests`), and an exit or a loop jump out of the
//! arm releases it first (the exit plan's `witness_dec`, or the jump's). The
//! local is therefore a holder that is not an owner — no dec-old, no
//! iteration-end release — whose one credit exactly one `$dec` per path
//! settles.

use super::WitnessRecorder;

impl WitnessRecorder {
    /// The rest block was built into `local`: a new object it holds (`i`).
    pub fn rest_born(&mut self, local: u32) {
        let o = self.fresh_obj(local, false);
        self.ops(o, "i");
    }

    /// The arm released the rest (`d`). A path that left the arm since the
    /// block was built (a `!` exit, a guard return, a `return_call`, a loop
    /// jump) released it on its own edge (#3377) and is dead here.
    pub fn rest_release(&mut self, local: u32) {
        if !self.held_ops(local, "d") {
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
