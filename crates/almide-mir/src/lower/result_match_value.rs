// The TAIL-VALUE Result-match opener and the eight deciders it dispatches through — the
// shape gate, the arm split, the subject materialization, the payload binds and the
// release-parity sweep. Split out of control_p2_c.rs at the opener boundary (max-lines,
// #852); everything moved verbatim.

impl LowerCtx {

    /// TAIL-VALUE Result match over a len-as-tag subject with HEAP-result arms — the
    /// Camp-4 opener for the `compute` class:
    ///   `match safe_div(a, b) { ok(v) => ok(int.to_string(v)), err(e) => <heap arm> }`
    /// The SUBJECT is materialized as an OWNED tracked temp (a call result / Dup'd var —
    /// dropped at scope end, AFTER the arms: each arm binds its payload as a BORROW (a
    /// scalar copy for Ok @12; the slot-0 HANDLE for a heap Err — `param_values`, not a
    /// second owner) and constructs its own FRESH result via `lower_heap_result_arm`
    /// (which Dups any borrowed payload it re-wraps), so nothing outlives the subject.
    /// The IfThen/Else/EndIf merge carries the arm value out — the released-merge cert
    /// shape `lower_heap_result_if_inner` already proves, incl. the release-parity sweep.
    pub(crate) fn try_lower_result_match_value(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // A SCALAR-Ok subject reads len-as-tag (@4); a HEAP-Ok subject is the cap-as-tag
        // 1-slot block (len@4 always 1, payload handle @12 low-32, tag @16) — the SAME
        // uniform Err=then(tag≠0)/Ok=else(tag 0) skeleton, only the tag offset and the
        // Ok-payload load differ (scalar copy vs borrowed LoadHandle — the statement-match
        // twin's `materialized_results_str` discipline). Opens the desugared
        // `let v = result.collect_map(..)!; ok(v)` tail (a heap-Ok match returned).
        let (ok_pay_ty, err_pay_ty) = Self::result_match_payload_types(subject, arms, result_ty)?;
        let heap_ok = is_heap_ty(&ok_pay_ty);
        let tag_off = if heap_ok { 16 } else { 4 };
        let ((ok_body, ok_bind), (err_body, err_bind)) = Self::split_result_match_arms(arms)?;
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize the subject to an OWNED tracked temp (a call result is fresh-owned; a
        // Var is borrowed — Dup it so the scope-end drop discipline is uniform). The temp is
        // in live_heap_handles → freed by the epilogue AFTER the merge move-out.
        let Some(subj) = self.materialize_owned_match_subject(subject) else {
            self.rollback_match_attempt(ops_mark, lifted_mark, lhh_mark);
            return None;
        };
        if heap_ok {
            // The cap-as-tag wrapper's epilogue drop must release its @12 payload too:
            // DropListStr rc_decs the single slot handle (len@4 is always 1) + frees the
            // wrapper — the Ok arm's Dup keeps a returned payload alive, so the dec is the
            // borrow's release, never a double-free. (A heap-ERR inner's own nested strings
            // free flat — the statement twin's `else heap_elem_lists` leak-parity bucket.)
            self.heap_elem_lists.insert(subj);
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        // THEN (tag != 0 = Err): payload = slot 0 (borrowed).
        self.bind_result_arm_payload(h, err_bind, &err_pay_ty);
        let Some((err_obj, consumed_by_err)) =
            self.lower_arm_tracking_consumed(err_body, result_ty)
        else {
            self.rollback_match_attempt(ops_mark, lifted_mark, lhh_mark);
            return None;
        };
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(err_obj) });
        // ELSE (tag == 0 = Ok): a scalar payload copies; a HEAP payload (cap-as-tag
        // subject) binds the @12 handle as a BORROW — the subject temp still owns it
        // (freed by the epilogue), an arm that returns it acquires its own ref (Dup).
        self.bind_result_arm_payload(h, ok_bind, &ok_pay_ty);
        let Some((ok_obj, consumed_by_ok)) = self.lower_arm_tracking_consumed(ok_body, result_ty)
        else {
            self.rollback_match_attempt(ops_mark, lifted_mark, lhh_mark);
            return None;
        };
        self.emit_arm_release_parity(&consumed_by_err, &consumed_by_ok, else_marker_at);
        self.ops.push(Op::EndIf { val: Some(ok_obj) });
        Some(dst)
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): the SHAPE gate — a heap result, exactly two unguarded arms, and a
    /// `Result[Ok, Err]` subject — yielding the `(ok_payload, err_payload)` types the
    /// opener reads its tag offset and its two payload loads from.
    fn result_match_payload_types(
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<(Ty, Ty)> {
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(result_ty) || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        match &subject.ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                Some((a[0].clone(), a[1].clone()))
            }
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): the payload BINDER of one `ok(..)` / `err(..)` inner pattern — a
    /// `Bind` yields its var, a `Wildcard` yields no binder, and any other shape is
    /// inadmissible (the outer `None`, which declines the whole match).
    fn result_arm_payload_bind(inner: &IrPattern) -> Option<Option<VarId>> {
        match inner {
            IrPattern::Bind { var, .. } => Some(Some(*var)),
            IrPattern::Wildcard => Some(None),
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): sorts the two arms into `((ok_body, ok_bind), (err_body, err_bind))`
    /// — a bare `_` arm serves as the Err arm — declining a duplicated side, an
    /// inadmissible binder, or a missing side. Runs BEFORE any op is emitted, so a
    /// decline here needs no rollback.
    #[allow(clippy::type_complexity)]
    fn split_result_match_arms(
        arms: &[IrMatchArm],
    ) -> Option<((&IrExpr, Option<VarId>), (&IrExpr, Option<VarId>))> {
        let mut ok_arm: Option<(&IrExpr, Option<VarId>)> = None;
        let mut err_arm: Option<(&IrExpr, Option<VarId>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = Self::result_arm_payload_bind(inner)?;
                    if ok_arm.is_some() {
                        return None;
                    }
                    ok_arm = Some((&arm.body, bind));
                }
                IrPattern::Err { inner } => {
                    let bind = Self::result_arm_payload_bind(inner)?;
                    if err_arm.is_some() {
                        return None;
                    }
                    err_arm = Some((&arm.body, bind));
                }
                IrPattern::Wildcard => {
                    if err_arm.is_some() {
                        return None;
                    }
                    err_arm = Some((&arm.body, None));
                }
                _ => return None,
            }
        }
        match (ok_arm, err_arm) {
            (Some(o), Some(e)) => Some((o, e)),
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): undoes a declined attempt back to the three marks taken before the
    /// subject was materialized — emitted ops, lifted functions, and the tracked
    /// live-heap handles — so the caller can return `None` with nothing left behind.
    fn rollback_match_attempt(&mut self, ops_mark: usize, lifted_mark: usize, lhh_mark: usize) {
        self.ops.truncate(ops_mark);
        self.lifted.truncate(lifted_mark);
        self.live_heap_handles.truncate(lhh_mark);
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): materializes the match subject as ONE owned tracked handle, or
    /// declines — a non-handle call arg, or a deferred Opaque bind (whose block is not
    /// real yet, so the tag read would be garbage). The caller owns the rollback.
    fn materialize_owned_match_subject(&mut self, subject: &IrExpr) -> Option<ValueId> {
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => return None,
        };
        if self.deferred_opaque_binds.contains(&subj) {
            return None;
        }
        Some(subj)
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): binds one arm's payload var (if the pattern has one) to a BORROWED
    /// read of slot @12 off the subject block `h` — a heap payload is a `LoadHandle`
    /// registered in `param_values` (a borrow, never a second owner), a scalar payload a
    /// plain 8-byte copy. Serves both the Err (THEN) and the Ok (ELSE) side.
    fn bind_result_arm_payload(&mut self, h: ValueId, bind: Option<VarId>, pay_ty: &Ty) {
        use crate::PrimKind;
        if let Some(var) = bind {
            let payload = if is_heap_ty(pay_ty) {
                let p = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.param_values.insert(p);
                p
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(var, payload);
        }
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): lowers one arm body to its result object and reports which of the
    /// handles that were live BEFORE it the arm consumed (moved out) — the input to the
    /// release-parity sweep. Declines (leaving the rollback to the caller) when the arm
    /// itself does not lower.
    fn lower_arm_tracking_consumed(
        &mut self,
        body: &IrExpr,
        result_ty: &Ty,
    ) -> Option<(ValueId, Vec<ValueId>)> {
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let obj = self.lower_heap_result_arm(body, result_ty)?;
        let consumed: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        Some((obj, consumed))
    }

    /// Extracted verbatim from [`Self::try_lower_result_match_value`] (codopsy round-3
    /// sweep, #852): release parity across the arms (the lower_heap_result_if_inner
    /// discipline) — a handle consumed by only ONE side gets a compensating drop on the
    /// other: appended after the THEN body, or inserted at the `Else` marker for the
    /// ELSE-only case.
    fn emit_arm_release_parity(
        &mut self,
        consumed_by_then: &[ValueId],
        consumed_by_else: &[ValueId],
        else_marker_at: usize,
    ) {
        for x in consumed_by_then {
            if !consumed_by_else.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in consumed_by_else {
            if !consumed_by_then.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
    }
}
