//! The inlined-predicate, find, fan, fold, view, assign and decline witness
//! hooks (split from witness_hooks.rs).

use super::*;

impl Emitter<'_> {
    /// #2755: a Bool predicate callback inlined by `list.any` / `all` /
    /// `count` (list_comb.rs): one activation per element, the body lowered
    /// inside it; the Bool carries no RC site.
    pub(crate) fn lower_inlined_predicate(
        &mut self,
        cb: &almide_ir::IrExpr,
        body: &almide_ir::IrExpr,
    ) -> Result<(), crate::EmitError> {
        self.witness_callback_open(cb, None);
        self.lower(body, Some(crate::BOOL))?;
        self.witness_loop_close();
        Ok(())
    }

    /// #2755: `list.find`'s hit (list.rs), recorded right after the callback
    /// body: a one-arm branch where the element is stored into the fresh
    /// some-cell — `share_handle_top`, so a handle-typed element's view takes
    /// a share that moves into the cell (`am`) — and the scan breaks. The
    /// activation closes after the site; the cell is the arm's owned result.
    pub(crate) fn witness_find_hit(&mut self, elem_local: u32, elem: SliceTy) {
        let shares = self.rc_droppable(elem) && self.elem_is_handle(elem);
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_open();
        w.branch_arm();
        if shares && !w.arg_share_move(elem_local) {
            w.poison();
        }
        w.branch_arm();
        w.branch_close();
        w.loop_close();
    }

    /// #2755: `fan.map` / `fan.any` / `any_map`'s sequential accumulator
    /// (fan.rs), per element: the callback body's Result carrier is a
    /// temporary born here (the body hands the loop its one credit). A
    /// borrowed body would take a share the recorder does not name: decline.
    pub(crate) fn witness_fan_carrier(&mut self, owned_body: bool) -> Option<u32> {
        if !owned_body {
            self.witness_decline("fan:borrowed-body");
            return None;
        }
        Some(self.witness.as_mut()?.temp_born())
    }

    /// The carrier's routes, as a branch: it LEAVES as the whole fan's
    /// result (`map`'s lowest-index err, `any`'s winner — the consumer
    /// records the owned result, the call-result convention), or it is
    /// consumed here: `any` releases a losing err (`d`), `map` releases the
    /// shell (`d`) after the ok payload's credit moved into the accumulator (a
    /// fresh value moved, `im`, when the payload is droppable), and — every
    /// element runs (ADR-0024 D1) — releases an err after the first (`d`).
    pub(crate) fn witness_fan_step(&mut self, c: Option<u32>, first_ok_wins: bool, payload: SliceTy) {
        let payload_moves = !first_ok_wins && self.rc_droppable(payload);
        let (Some(w), Some(o)) = (self.witness.as_mut(), c) else { return };
        w.branch_open();
        w.branch_arm();
        w.temp_ops(o, "m");
        w.branch_arm();
        w.temp_ops(o, "d");
        if payload_moves {
            w.temp_move();
        }
        if !first_ok_wins {
            w.branch_arm();
            w.temp_ops(o, "d");
        }
        w.branch_close();
    }

    /// #2755: one step of a heap `list.fold` accumulator (list.rs), after the
    /// share guard: the body's value takes the accumulator's credit on to the
    /// next iteration (or out of the loop — the fold's result is an owned
    /// value its consumer records) — an owned value moves (`im`), a Var
    /// shares and moves (`am`). The release of the block the iteration
    /// received (the arm's `$dec` before the rebind) is the `d` the recorder
    /// writes for every owner local bound in a loop body at the iteration's
    /// end (witness_paths.rs), so it is not logged twice on the certificate;
    /// it is logged for the byte check at that `$dec` (#4277). A droppable
    /// accumulator that is not a handle takes neither instruction: declined.
    pub(crate) fn witness_fold_step(&mut self, body: &almide_ir::IrExpr, acc: u32, ty: SliceTy) {
        if self.witness.is_none() || !self.rc_droppable(ty) {
            return;
        }
        if !self.elem_is_handle(ty) {
            self.witness_decline("fold-acc:flat");
            return;
        }
        if let Some(w) = self.witness.as_mut() {
            w.iteration_release(acc);
        }
        self.witness_store(body, ty);
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
    /// `rc_inc_top` on its source, a view read out of a bound block (a slot,
    /// a `!` payload, a borrowed `o ?? fallback` join over a bound carrier)
    /// or a top-let global took it on the view (`bind_view`'s share),
    /// anything else declines.
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
        let view = crate::witness_unwrap::is_extraction_view(value) || self.witness_top_let_ty(value).is_some();
        let Some(w) = self.witness.as_mut() else { return };
        let ok = match (owned, src) {
            (true, _) => w.assign(idx, released_old, None),
            (false, Some(s)) => w.assign(idx, released_old, Some(s)),
            (false, None) if view => w.assign_view(idx, released_old),
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
    /// An arm that already left (a `panic`'s abort) settles nothing: its
    /// settling instructions are unreachable.
    pub(crate) fn witness_arm_value(&mut self, e: &almide_ir::IrExpr) {
        if self.witness.as_ref().is_some_and(|w| !w.dead()) {
            self.witness_share_or_move(e, "arm-value:borrowed-temp");
        }
    }

    /// A match arm's pattern binds (#2756): each droppable binder is a VIEW
    /// of the subject's payload — a known object the frame holds no credit
    /// of (patterns.rs binds by `local.set`, no share, no release). A NAMED
    /// list rest is a fresh block instead, which its arm releases (#2971,
    /// witness_rest.rs).
    pub(crate) fn witness_pattern_views(&mut self, p: &almide_ir::IrPattern) {
        if self.witness.is_none() {
            return;
        }
        let (mut vars, mut rests) = (Vec::new(), Vec::new());
        pattern_binders(p, &mut vars);
        crate::patterns::named_rests(p, &mut rests);
        for v in vars.iter().map(|v| (v, false)).chain(rests.iter().map(|v| (v, true))) {
            let Some(&(idx, ty)) = self.locals.get(v.0) else { continue };
            if self.rc_droppable(ty)
                && let Some(w) = self.witness.as_mut()
            {
                if v.1 { w.rest_born(idx) } else { w.param_borrowed(idx) }
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

/// Every variable a pattern binds as a view (a named list rest is not one:
/// `crate::patterns::named_rests`).
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
            if let Some(r) = rest.as_deref().filter(|r| !matches!(r, P::Bind { .. })) {
                pattern_binders(r, out);
            }
        }
        P::Wildcard | P::Literal { .. } | P::None => {}
    }
}
