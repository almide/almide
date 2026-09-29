//! The Emitter-side witness hooks (#1696): one per RC-affecting route,
//! each called right where the route emitted its instruction, so the
//! recorder (witness.rs) mirrors the instruction stream event for event.
//!
//! STEP 4 (statement calls + module calls): a native arm declares what it
//! does with each argument (arm.rs `ArgMode`) and what it hands back
//! (`Lowered`); `lower_arg` is the one place an arm's argument crosses
//! into it, so the hook there records the declaration's RC instruction —
//! Borrow: a Var shares nothing, a parked temporary is born and released
//! by the scope (`id`); Retain: a Var takes the `rc_share_guard` +1 and
//! its credit moves into the arm (`am`), a temporary moves in (`im`);
//! Raw: nothing. The wrapper (`lower_module_call`) asks whether a hook
//! fired for EACH of the arm's own argument nodes (by node identity — a
//! count would let a nested argument's inner hooks stand in for an outer
//! one the arm lowered bare, #2755): an arm that lowered an argument any
//! other way is unaudited and the frame DECLINES — counted, never
//! under-recorded. A droppable `View` result declines too: its share
//! lands on an object the frame does not track by local, and this
//! phase's object identity is the local map.
//!
//! TRUSTED (not mechanically checked here): an arm that routed every
//! argument through `lower_arg` emits no other rc_inc / dec on those
//! arguments in-frame — the arm.rs doctrine ("Retain IS the share, no
//! arm repeats it"). The runtime helpers' element bookkeeping inside
//! `$block_copy` / `$dec_flat` is the helper's contract, as for phase A.

use crate::arm::{ArgMode, Lowered, Own};
use crate::emitter::Emitter;
use crate::SliceTy;

/// A hooked node's identity for the module-call audit.
fn node(e: &almide_ir::IrExpr) -> usize {
    e as *const almide_ir::IrExpr as usize
}

impl Emitter<'_> {
    /// The local whose object a value SHARES: a Var, read through block
    /// tails (`{ let t = …; t }` is `t`'s object — the routes' +1 lands on
    /// the tail's value, `rc_owned_result` reads through blocks the same way).
    fn witness_src_local(&self, e: &almide_ir::IrExpr) -> Option<u32> {
        if let almide_ir::IrExprKind::Var { id } = &crate::rc_ownership::rc_tail(e).kind {
            self.locals.get(id).map(|&(l, _)| l)
        } else {
            None
        }
    }

    /// The Bind-route hook (stmts.rs): called right after the local joins
    /// `rc_owned`. Attribution mirrors the instructions the route just
    /// emitted: an OWNED rhs (fresh, or a call's handed-over credit, #1986)
    /// is a NEW object; a borrowed Var rhs of any droppable shape took
    /// `rc_inc_top` (Maps and Sets too since #2010 Map stage b retired the
    /// bind-time `$block_copy`), so the SOURCE object gains a share. A
    /// borrowed non-Var rhs (a native arm's View) declines; anything else
    /// under an armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_bind(&mut self, idx: u32, _declared: SliceTy, value: &almide_ir::IrExpr) {
        let src_local = self.witness_src_local(value);
        let owned = self.rc_owned_result(value);
        let view = crate::witness_unwrap::is_extraction_view(value);
        let Some(w) = self.witness.as_mut() else { return };
        if owned {
            w.bind_fresh(idx);
            return;
        }
        match src_local {
            Some(src) if w.bind_alias(idx, src) => {}
            // #2758: a `!` payload read out of a bound carrier.
            None if view => w.bind_view(idx),
            // The frame withdraws; the local still gets an (opaque) object
            // so its later release or loop-back carry is attributed, not
            // mistaken for a hook disagreement.
            None => {
                w.decline("bind:view-result");
                w.param_borrowed(idx);
            }
            _ => w.poison(),
        }
    }

    /// The call-argument hook (calls.rs, right after `rc_arg_guard`):
    /// a droppable Var argument's object gained a real `rc_inc` and its
    /// credit moves into the callee; a fresh temporary is born and moves.
    /// Non-droppable arguments have no RC site. Anything else under an
    /// armed recorder is a gate/hook disagreement — poison.
    pub(crate) fn witness_arg(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(node(e));
        if !self.rc_droppable(ty) {
            return;
        }
        self.witness_share_or_move(e, "call-arg:borrowed-temp");
    }

    /// Mirrors rc_arg_guard / rc_share_guard on a droppable value handed to
    /// a new holder: an OWNED value (fresh literal or a call result carrying
    /// its one credit) is born and moves (`im`); a borrowed Var shares and
    /// moves (`am`). A borrowed NON-Var (a native arm's View, #2755's nested
    /// arguments reach it) took a real `rc_inc` on an object this frame
    /// does not track by local — withdraw with `reason`, never under-record.
    fn witness_share_or_move(&mut self, e: &almide_ir::IrExpr, reason: &str) {
        let src_local = self.witness_src_local(e);
        let fresh = self.rc_owned_result(e);
        let view = crate::witness_unwrap::is_extraction_view(e);
        let Some(w) = self.witness.as_mut() else { return };
        if fresh {
            w.temp_move();
            return;
        }
        match src_local {
            Some(l) if w.arg_share_move(l) => {}
            None if view => w.view_share_move(),
            None => w.decline(reason),
            _ => w.poison(),
        }
    }

    /// The payload-store hook (#2755: `some` / `ok` / `err` and a variant
    /// case, right after `rc_share_guard`): the container becomes the
    /// payload's holder. Mirrors the guard: an i64/f64 slot and a non-handle
    /// Var carry no RC site; a Var of a handle shape takes the real +1 and
    /// its credit moves in (`am`, `witness_retain_var`); an owned temporary
    /// moves in (`im`); a borrowed non-Var droppable took an untracked +1
    /// (decline).
    pub(crate) fn witness_store(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        if self.witness.is_none() || ty.val_type() != wasm_encoder::ValType::I32 {
            return;
        }
        if let almide_ir::IrExprKind::Var { .. } = &e.kind {
            // A scalar i32 slot (Bool, a narrow int) has no site; any
            // droppable Var goes through the retain mirror, which declines
            // a cell, a global and a droppable-but-flat local.
            if self.rc_droppable(ty) {
                self.witness_retain_var(e, "store");
            }
            return;
        }
        if self.rc_droppable(ty) {
            self.witness_share_or_move(e, "store:borrowed-temp");
        }
    }

    /// The call-argument hook for a BORROWED callee param (#2028): a Var
    /// argument has no RC site (the callee holds nothing); a fresh
    /// temporary is born and released by the site (`id`).
    pub(crate) fn witness_arg_borrowed(&mut self, e: &almide_ir::IrExpr, ty: SliceTy, fresh: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(node(e));
        if !self.rc_droppable(ty) {
            return;
        }
        // A borrowed non-Var (a View through a block tail, a nested
        // native arm's result) is lent as is: no RC site.
        let Some(w) = self.witness.as_mut() else { return };
        if fresh {
            w.temp_borrowed();
        }
    }

    /// The native-arm argument hook (arm.rs `lower_arg`, after the mode's
    /// instruction): `parked` = the Borrow mode parked an owned temporary
    /// for the scope's release.
    pub(crate) fn witness_module_arg(&mut self, e: &almide_ir::IrExpr, got: SliceTy, mode: ArgMode, parked: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(node(e));
        if !self.rc_droppable(got) {
            return;
        }
        let is_var = matches!(e.kind, almide_ir::IrExprKind::Var { .. });
        match mode {
            ArgMode::Raw => {}
            // A Borrow lends the value as is (a Var, a static, a View: no
            // RC site); only a parked owned temporary has one (`id`).
            ArgMode::Borrow => {
                if parked && let Some(w) = self.witness.as_mut() {
                    w.temp_borrowed();
                }
            }
            ArgMode::Retain if is_var => self.witness_retain_var(e, "module-arg"),
            ArgMode::Retain => self.witness_share_or_move(e, "module-arg:retain-borrowed-temp"),
        }
    }

    /// Retain of a Var mirrors `rc_share_guard`: a cell var shares
    /// nothing (decline — the cell's credit is not this frame's); a
    /// handle-typed local took the real `rc_inc` and its credit moves
    /// into the arm (`am`); a droppable local that is not a handle took
    /// no +1 at all — the arm retains what it did not share: decline.
    fn witness_retain_var(&mut self, e: &almide_ir::IrExpr, position: &str) {
        let almide_ir::IrExprKind::Var { id } = &e.kind else { return };
        if self.cells.contains(id) {
            self.witness_decline(&format!("{position}:retain-cell"));
            return;
        }
        let Some(&(l, vt)) = self.locals.get(id) else {
            self.witness_decline(&format!("{position}:retain-unknown-local"));
            return;
        };
        if !self.elem_is_handle(vt) {
            self.witness_decline(&format!("{position}:retain-flat"));
            return;
        }
        if let Some(w) = self.witness.as_mut()
            && !w.arg_share_move(l)
        {
            w.poison();
        }
    }

    /// #2758: a capture stored into a new closure's env (emitter_values.rs
    /// `lower_lambda_value`). The env is a holder: a handle-typed capture
    /// takes the `share_handle_top` +1 and its credit moves into the env
    /// (`am`, released by the env's drop glue). A C-319 cell co-owns the
    /// cell, not the value (decline); a droppable capture that is not a
    /// handle took no +1 (decline, as `witness_retain_var`).
    pub(crate) fn witness_capture(&mut self, idx: u32, t: SliceTy, is_cell: bool) {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return;
        }
        if is_cell {
            self.witness_decline("capture:cell");
            return;
        }
        if !self.elem_is_handle(t) {
            self.witness_decline("capture:flat");
            return;
        }
        if let Some(w) = self.witness.as_mut()
            && !w.arg_share_move(idx)
        {
            w.poison();
        }
    }

    /// #2758: a top-let's value becomes its global's (func.rs
    /// `store_top_let`). A copied shape (List / Map / Set / Bytes): an owned
    /// initializer is born and released after the copy (`id`), the fresh
    /// copy moves into the global (`im`). Any other shape: an owned value
    /// moves into the global (`im`); a borrowed one takes a share at the
    /// store (#2992) on a source object this frame does not track (another
    /// global, a pool static) — not modelled, so it declines.
    pub(crate) fn witness_top_let(&mut self, declared: SliceTy, owned: bool, copied: bool) {
        if !self.rc_droppable(declared) {
            return;
        }
        let Some(w) = self.witness.as_mut() else { return };
        match (copied, owned) {
            (true, true) => {
                w.temp_discarded();
                w.temp_move();
            }
            (true, false) | (false, true) => w.temp_move(),
            (false, false) => w.decline("top-let:borrowed"),
        }
    }

    /// The module-call wrapper's audit (calls_modules.rs): a hook fired for
    /// EACH of the call's own argument nodes, or the frame declines; a droppable
    /// `View` result declines (identity, see the module doc).
    pub(crate) fn witness_module_result(
        &mut self,
        name: &str,
        args: &[almide_ir::IrExpr],
        hooks_before: usize,
        lowered: Option<Lowered>,
    ) {
        let Some(w) = self.witness.as_ref() else { return };
        // A SCALAR argument has no RC site of its own (an arm may lower it
        // bare — `math.pow`'s operands); any site inside it (a nested
        // call's arguments) is that call's own hook. Every droppable — or
        // untyped — argument must have gone through `lower_arg`.
        let scalar = |a: &almide_ir::IrExpr| {
            crate::ty::slice_ty_of(&a.ty, self.types).is_some_and(|t| !self.rc_droppable(t))
        };
        if !args.iter().all(|a| scalar(a) || w.hooked_since(hooks_before, node(a))) {
            self.witness_decline(&format!("module-arm:unaudited:{name}"));
            return;
        }
        if let Some(l) = lowered
            && l.own == Own::View
            && self.rc_droppable(l.ty)
        {
            self.witness_decline(&format!("module-result:view:{name}"));
        }
    }

    /// The statement-position discard (stmts.rs): an owned droppable
    /// result arrived with its one credit and the route released it —
    /// born and released in this frame (`id`).
    pub(crate) fn witness_discard(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.temp_discarded();
        }
    }

    /// The owned-tail hook (func.rs): a droppable tail that needs no
    /// ret-inc — a user-fn call result or a fresh literal — moves its
    /// one credit out of the frame.
    pub(crate) fn witness_tail_owned(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.tail_owned_move();
        }
    }

    /// The heap-Var tail hook (func.rs, after the ret-inc): the bound
    /// Var's object is shared and moves out (`am`); a non-Var borrowed
    /// tail (a native arm's View) declines; anything else is poison.
    pub(crate) fn witness_tail_var(&mut self, tail: &almide_ir::IrExpr) {
        let src = self.witness_src_local(tail);
        let view = crate::witness_unwrap::is_extraction_view(tail);
        let Some(w) = self.witness.as_mut() else { return };
        // After a `return_call` (a tail `f(x)!` seen through, C-069) the
        // wrap the emitter still writes is dead: nothing to record.
        if w.dead() {
            return;
        }
        match src {
            Some(l) if w.ret_move(l) => {}
            None if view => w.view_share_move(),
            None => w.decline("tail:view-result"),
            _ => w.poison(),
        }
    }

    /// A self tail call in LOOP form (#2757, tco.rs) ends this activation's
    /// path. Each owner local it CARRIES keeps its block until the local's
    /// next rebind releases it (the Bind route's dec-old, stmts.rs) or the
    /// epilogue does — exactly one release per bound block, on every path.
    /// That release is recorded HERE, at the carry, as the block's `d`; the
    /// rebind's dec-old is therefore never recorded (on a first binding it
    /// is the release of NULL), and a local born on another path is not
    /// touched (witness_paths.rs skips an unborn object's events).
    pub(crate) fn witness_loop_back(&mut self, carried: &[u32]) {
        let Some(w) = self.witness.as_mut() else { return };
        for &idx in carried {
            if !w.dec_local(idx) {
                w.poison();
            }
        }
        w.frame_replaced();
    }

    /// #2757 / #2976: under the raw-address rule a loop-form self call
    /// releases nothing at the loop-back, so an owned param the arguments
    /// do not hand straight through (`moved`) keeps its old block forever —
    /// the regex capture accumulator's leak. The witness withdraws rather
    /// than certify a leaking path.
    pub(crate) fn witness_raw_loop_back(&mut self, loop_form_raw: bool, moved: &[u32]) {
        if loop_form_raw
            && self.witness.is_some()
            && self.rc_frame_params.iter().any(|p| !moved.contains(p) && !self.tail_consumed.contains(p))
        {
            self.witness_decline("loop-back:raw-param-kept");
        }
    }

    /// An argument whose credit MOVES without a share (#2757): the param a
    /// loop-form self call hands straight through under the raw-address
    /// rule, and the accumulator a `$str_append` / `$list_push` window
    /// consumes (tail_append.rs). One credit in, one out — `m` on its
    /// object, no `a`.
    pub(crate) fn witness_arg_moved(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(node(e));
        if !self.rc_droppable(ty) {
            return;
        }
        let src = self.witness_src_local(e);
        let Some(w) = self.witness.as_mut() else { return };
        match src {
            Some(l) if w.move_local(l) => {}
            _ => w.poison(),
        }
    }

    /// The epilogue hook (func.rs): one `d` per `$dec_flat` emitted.
    pub(crate) fn witness_dec(&mut self, idx: u32) {
        if let Some(w) = self.witness.as_mut()
            && !w.dec_local(idx)
        {
            w.poison();
        }
    }

    /// A branch site opens / an arm begins / the site joins (#2756): the
    /// recorder logs the structure its per-object lines are rendered from.
    pub(crate) fn witness_branch_open(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.branch_open();
        }
    }

    pub(crate) fn witness_branch_arm(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.branch_arm();
        }
    }

    pub(crate) fn witness_branch_close(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.branch_close();
        }
    }

    /// A `for` / `while` body opens / closes, and a `break` / `continue`
    /// ends an iteration (#2757).
    pub(crate) fn witness_loop_open(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.loop_open();
        }
    }

    pub(crate) fn witness_loop_close(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.loop_close();
        }
    }

    pub(crate) fn witness_loop_jump(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.loop_jump();
        }
    }

    /// A loop variable bound from the element it walks (#2757): a view,
    /// like a pattern binder — known, no credit held.
    pub(crate) fn witness_view_local(&mut self, idx: u32, ty: SliceTy) {
        if self.rc_droppable(ty)
            && let Some(w) = self.witness.as_mut()
        {
            w.param_borrowed(idx);
        }
    }

    /// The general `Assign` route (#2757, stmts.rs), right after it stored
    /// the new occupant: `released_old` = the route released the old one
    /// (not when the rhs spends the var's credit, which declines here — its
    /// callee's write-back is not audited). The new occupant mirrors the
    /// Bind route: an owned rhs is a new object, a borrowed Var rhs took
    /// `rc_inc_top` on its source, anything else declines.
    pub(crate) fn witness_assign(&mut self, idx: u32, released_old: bool, spends: bool, value: &almide_ir::IrExpr) {
        if self.witness.is_none() {
            return;
        }
        if spends {
            self.witness_decline("assign:spends-var");
            return;
        }
        let owned = self.rc_owned_result(value);
        let src = self.witness_src_local(value);
        let Some(w) = self.witness.as_mut() else { return };
        let ok = match (owned, src) {
            (true, _) => w.assign(idx, released_old, None),
            (false, Some(s)) => w.assign(idx, released_old, Some(s)),
            (false, None) => {
                w.decline("assign:view-result");
                true
            }
        };
        if !ok {
            w.poison();
        }
    }

    /// A value arm hands the join its one credit (#2756): an owned arm
    /// value moves (`im`), a borrowed Var arm took the normalizing +1 and
    /// moves (`am`), a borrowed non-Var declines. Mirrors `lower_if_arms` /
    /// `lower_arm_body`, which call it exactly where they settle the credit.
    pub(crate) fn witness_arm_value(&mut self, e: &almide_ir::IrExpr) {
        if self.witness.is_some() {
            self.witness_share_or_move(e, "arm-value:borrowed-temp");
        }
    }

    /// A match arm's pattern binds (#2756): each droppable binder is a VIEW
    /// of the subject's payload — a known object the frame holds no credit
    /// of (patterns.rs binds by `local.set`, no share, no release).
    pub(crate) fn witness_pattern_views(&mut self, p: &almide_ir::IrPattern) {
        if self.witness.is_none() {
            return;
        }
        let mut vars = Vec::new();
        pattern_binders(p, &mut vars);
        for v in vars {
            let Some(&(idx, ty)) = self.locals.get(&v) else { continue };
            if self.rc_droppable(ty)
                && let Some(w) = self.witness.as_mut()
            {
                w.param_borrowed(idx);
            }
        }
    }

    /// An emission-time decline: the route this frame took has an RC
    /// site this phase does not record — withdraw the certificate with
    /// a reason the histogram counts (never a silent under-count).
    pub(crate) fn witness_decline(&mut self, reason: &str) {
        if let Some(w) = self.witness.as_mut() {
            w.decline(reason);
        }
    }
}

/// Every variable a pattern binds (the list-rest binder included: the gate
/// declines a named rest before any frame reaches here).
fn pattern_binders(p: &almide_ir::IrPattern, out: &mut Vec<almide_ir::VarId>) {
    use almide_ir::IrPattern as P;
    match p {
        P::Bind { var, .. } => out.push(*var),
        P::As { var, inner, .. } => {
            out.push(*var);
            pattern_binders(inner, out);
        }
        P::Constructor { args: ps, .. } | P::Tuple { elements: ps } => ps.iter().for_each(|q| pattern_binders(q, out)),
        P::Some { inner } | P::Ok { inner } | P::Err { inner } => pattern_binders(inner, out),
        P::RecordPattern { fields, .. } => {
            fields.iter().filter_map(|f| f.pattern.as_ref()).for_each(|q| pattern_binders(q, out));
        }
        P::List { elements, rest } => {
            elements.iter().for_each(|q| pattern_binders(q, out));
            if let Some(r) = rest {
                pattern_binders(r, out);
            }
        }
        P::Wildcard | P::Literal { .. } | P::None => {}
    }
}
