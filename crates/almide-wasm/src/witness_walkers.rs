//! The I/O walkers' witness hooks (#2755 / #3137): the fs line walkers
//! (fs.rs, fs_meta.rs, fs_range.rs) and the prefetch fans (fan.rs).
//!
//! A line walker is an inlined-callback loop like `list.fold`: one activation
//! per line (`witness_callback_open`), each param a VIEW, a heap accumulator
//! a loop-carried owner (`witness_fold_step`). What it adds is the LINE: a
//! fresh block the walk allocates per iteration (`i`) and releases after the
//! callback returned (`d`, `fs_frames_foreach_borrowed`). A heap accumulator
//! leaves the loop as an owned value (`i`, as `list.fold`'s consumer receives
//! it) and is moved into the ok shell (`m`) or released with an err (`d`).
//!
//! A prefetch fan runs no callback body: each awaited read is a Result
//! carrier born in the frame, settled as the sequential fan's
//! (`witness_fan_step`).
//!
//! An instance-parallel chunk `fan.__par_K(xs, fallback, c…)`
//! (fan_par_lower.rs) is a branch site on the host's answer: the SERVED arm
//! builds the result list from the answer room (one activation per element,
//! a tuple result's fresh block moving into the list, `im`), the other arm
//! lowers `fallback` — the sequential `list.map`, its sites the ordinary
//! hooks — whose owned result is the site's value. The request and answer
//! rooms are born before the site and freed after it (`id` each).

use crate::emitter::Emitter;
use crate::SliceTy;

/// How a line walker's accumulator is recorded: `Carried` = `list.fold`'s
/// per-iteration owner (`None` for a scalar or no accumulator), `Outer` = a
/// frame owner bound before the loop (the fallible fold, local given).
#[derive(Clone, Copy)]
pub(crate) enum WalkAcc {
    Carried(Option<u32>),
    Outer(u32),
}

impl Emitter<'_> {
    /// A walk iteration opens: the activation of `cb` (carrying `acc`), and
    /// the line block the walk allocated for it. Returns the line's object.
    pub(crate) fn witness_line_open(&mut self, cb: &almide_ir::IrExpr, acc: WalkAcc) -> Option<u32> {
        match acc {
            WalkAcc::Carried(a) => self.witness_callback_open(cb, a),
            // The accumulator is a frame owner bound before the loop: every
            // other param is a view.
            WalkAcc::Outer(a) => {
                let w = self.witness.as_mut()?;
                w.note_arg(cb as *const almide_ir::IrExpr as usize);
                w.loop_open();
                let almide_ir::IrExprKind::Lambda { params, .. } = &cb.kind else { return None };
                for (var, _) in params {
                    if let Some(&(idx, ty)) = self.locals.get(var)
                        && idx != a
                    {
                        self.witness_view_local(idx, ty);
                    }
                }
            }
        }
        Some(self.witness.as_mut()?.temp_born())
    }

    /// The walk released the line, and the iteration ends.
    pub(crate) fn witness_line_close(&mut self, line: Option<u32>) {
        if let (Some(w), Some(o)) = (self.witness.as_mut(), line) {
            w.temp_ops(o, "d");
        }
        self.witness_loop_close();
    }

    /// After the walk: a heap accumulator leaves the loop owned (`i`), and
    /// is released with the err result (first arm) or moved into the ok
    /// shell (second arm). A scalar or Map accumulator carries no credit.
    pub(crate) fn witness_walk_result(&mut self, acc: SliceTy) {
        if !self.rc_droppable(acc) {
            return;
        }
        let Some(w) = self.witness.as_mut() else { return };
        let o = w.temp_born();
        w.branch_open();
        w.branch_arm();
        w.temp_ops(o, "d");
        w.branch_arm();
        w.temp_ops(o, "m");
        w.branch_close();
    }

    /// `fs.fold_lines_chunked`: a range's accumulator leaves its line walk
    /// owned (`i`) and is stored into the partials list (`m`).
    pub(crate) fn witness_walk_stored(&mut self, acc: SliceTy) {
        if self.rc_droppable(acc)
            && let Some(w) = self.witness.as_mut()
        {
            w.temp_move();
        }
    }

    /// A fallible walker (`__fallible_fold_lines` / `__fallible_for_each_line`)
    /// keeps a heap accumulator across an err and the skipped lines after
    /// it — an iteration that neither releases nor hands it on — so the
    /// accumulator is not `list.fold`'s per-iteration owner but a FRAME
    /// owner bound before the loop (the Retain'd init's credit, `i`), which
    /// an ok rebinds (`witness_fallible_close`) and the walk's result
    /// settles (`witness_fallible_result`). A borrowed body's carrier takes a
    /// share the recorder does not name: withdraw. Returns whether to record.
    pub(crate) fn witness_fallible_walk(&mut self, acc: Option<(u32, SliceTy)>, owned_body: bool) -> bool {
        if self.witness.is_none() {
            return false;
        }
        if !owned_body {
            self.witness_decline("fs-fallible:borrowed-body");
            return false;
        }
        if let Some((l, t)) = acc
            && self.rc_droppable(t)
        {
            // A droppable accumulator that is not a handle is never released.
            if !self.elem_is_handle(t) {
                self.witness_decline("fs-fallible:flat-acc");
                return false;
            }
            if let Some(w) = self.witness.as_mut() {
                w.carried_owned(l);
            }
        }
        true
    }

    /// After a fallible fold: a heap accumulator is released with the read's
    /// err (first arm) or the callback's (second site, first arm), else it
    /// moves into the ok shell.
    pub(crate) fn witness_fallible_result(&mut self, record: bool, acc: u32, t: SliceTy) {
        if !record || !self.rc_droppable(t) {
            return;
        }
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_open();
        w.branch_arm();
        let ok = w.dec_local(acc);
        w.branch_arm();
        w.branch_open();
        w.branch_arm();
        let ok = ok && w.dec_local(acc);
        w.branch_arm();
        let ok = ok && w.move_local(acc);
        w.branch_close();
        w.branch_close();
        if !ok {
            w.poison();
        }
    }

    /// The fallible walk's per-line site: the callback runs only while no err
    /// landed (first arm; its carrier is born here), else the line is
    /// skipped (second arm, `witness_fallible_close`).
    pub(crate) fn witness_fallible_open(&mut self, record: bool) {
        if record {
            self.witness_branch_open();
            self.witness_branch_arm();
        }
    }

    /// The callback's Result carrier, born after the body (owned).
    pub(crate) fn witness_fallible_carrier(&mut self, record: bool) -> Option<u32> {
        if !record {
            return None;
        }
        Some(self.witness.as_mut()?.temp_born())
    }

    /// The carrier settles: an ok is released, its payload moved into the
    /// accumulator — for a heap one (`rebind`), the old block released and
    /// the payload's credit a new object the local holds; an err becomes the
    /// walk's result (`m`). `ok_first`: the arm order the instructions take.
    /// Then the skip arm, and the site joins.
    pub(crate) fn witness_fallible_close(&mut self, c: Option<u32>, ok_first: bool, rebind: Option<u32>) {
        let (Some(w), Some(o)) = (self.witness.as_mut(), c) else { return };
        let ok_arm = |w: &mut crate::witness::WitnessRecorder| {
            w.temp_ops(o, "d");
            rebind.is_none_or(|l| w.assign(l, true, None))
        };
        w.branch_open();
        w.branch_arm();
        let fine = if ok_first {
            ok_arm(w)
        } else {
            w.temp_ops(o, "m");
            true
        };
        w.branch_arm();
        let fine = fine
            && if ok_first {
                w.temp_ops(o, "m");
                true
            } else {
                ok_arm(w)
            };
        w.branch_close();
        w.branch_arm();
        w.branch_close();
        if !fine {
            w.poison();
        }
    }

    /// A prefetch fan (fan.rs): the callback node is the arm's argument,
    /// never lowered (the reads are the arm's own host ops).
    pub(crate) fn witness_prefetch_open(&mut self, cb: &almide_ir::IrExpr) {
        if let Some(w) = self.witness.as_mut() {
            w.note_arg(cb as *const almide_ir::IrExpr as usize);
        }
    }

    /// One awaited read of `fan.map`'s prefetch: after the first err the
    /// remaining awaits only drain — every read already ran, started in
    /// phase A (ADR-0024 D1) — (second arm, no carrier); otherwise the
    /// read's Result carrier is born and settled as the sequential `map`'s.
    pub(crate) fn witness_prefetch_map_step(&mut self) {
        if self.witness.is_none() {
            return;
        }
        self.witness_branch_open();
        self.witness_branch_arm();
        let c = self.witness.as_mut().map(|w| w.temp_born());
        self.witness_fan_step(c, false, crate::STR);
        self.witness_branch_arm();
        self.witness_branch_close();
    }

    /// One awaited read of `fan.any`'s prefetch: its carrier wins (leaves as
    /// the result) or, a losing err, is released.
    pub(crate) fn witness_prefetch_any_step(&mut self) {
        let c = self.witness.as_mut().map(|w| w.temp_born());
        self.witness_fan_step(c, true, crate::STR);
    }

    /// #2758: one arm of a `fan { … }` block (fan.rs `lower_fan_block`), right
    /// after it was lowered. A Result arm's OWNED carrier is born here and its
    /// spine released (`id`, the `$dec_flat` every path runs): the ok payload,
    /// or the err message the block's abort reads, keeps the carrier's credit
    /// on it. A borrowed carrier and a pure arm have no site here; every arm's
    /// value is settled at the tuple slot (`witness_fan_block_slot`) or, for a
    /// one-arm block, is the block's value its consumer records.
    pub(crate) fn witness_fan_block_arm(&mut self, carrier: bool, owned: bool) {
        if carrier && owned {
            self.witness_discard();
        }
    }

    /// A slot of the fan block's fresh tuple, after the share the route took
    /// for a borrowed value (`share_handle_top`): an OWNED value — a pure
    /// arm's own credit, or an owned carrier's payload whose credit stayed
    /// with it — moves in (`im`); a borrowed one shares and moves (`am`): a
    /// carrier's payload is a view of its slot, a pure arm a Var or a view.
    /// A droppable slot that is no handle takes no share: declined.
    pub(crate) fn witness_fan_block_slot(&mut self, arm: &almide_ir::IrExpr, carrier: bool, p: SliceTy, owned: bool) {
        if self.witness.is_none() || !self.rc_droppable(p) {
            return;
        }
        if owned {
            if let Some(w) = self.witness.as_mut() {
                w.temp_move();
            }
        } else if !self.elem_is_handle(p) {
            self.witness_decline("fan:flat-slot");
        } else if carrier {
            if let Some(w) = self.witness.as_mut() {
                w.view_share_move();
            }
        } else {
            self.witness_share_or_move(arm, "fan:borrowed-slot");
        }
    }

    /// `fan.__par_K`'s request room was allocated (before the request loop).
    pub(crate) fn witness_par_room(&mut self) -> Option<u32> {
        self.witness.as_mut().map(|w| w.temp_born())
    }

    /// The host op's verdict site opens on its SERVED arm: one activation of
    /// the copy-out loop, where a tuple element's fresh block moves into the
    /// result list (`im`).
    pub(crate) fn witness_par_served(&mut self, tuple: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_open();
        w.branch_arm();
        w.loop_open();
        if tuple {
            w.temp_move();
        }
        w.loop_close();
    }

    /// The not-served arm lowers `fallback` in place (never through
    /// `lower_arg`): the node is hooked here for the module-call audit, and
    /// its owned result is the site's value.
    pub(crate) fn witness_par_fallback(&mut self, fallback: &almide_ir::IrExpr) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(fallback as *const almide_ir::IrExpr as usize);
        w.branch_arm();
    }

    /// The site joins, and the two rooms are freed.
    pub(crate) fn witness_par_close(&mut self, rooms: [Option<u32>; 2]) {
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_close();
        for o in rooms.into_iter().flatten() {
            w.temp_ops(o, "d");
        }
    }
}
