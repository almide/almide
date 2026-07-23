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
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(result_ty) || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        // A SCALAR-Ok subject reads len-as-tag (@4); a HEAP-Ok subject is the cap-as-tag
        // 1-slot block (len@4 always 1, payload handle @12 low-32, tag @16) — the SAME
        // uniform Err=then(tag≠0)/Ok=else(tag 0) skeleton, only the tag offset and the
        // Ok-payload load differ (scalar copy vs borrowed LoadHandle — the statement-match
        // twin's `materialized_results_str` discipline). Opens the desugared
        // `let v = result.collect_map(..)!; ok(v)` tail (a heap-Ok match returned).
        let (ok_pay_ty, err_pay_ty) = match &subject.ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                (a[0].clone(), a[1].clone())
            }
            _ => return None,
        };
        let heap_ok = is_heap_ty(&ok_pay_ty);
        let tag_off = if heap_ok { 16 } else { 4 };
        let mut ok_arm: Option<(&IrExpr, Option<VarId>)> = None;
        let mut err_arm: Option<(&IrExpr, Option<VarId>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, .. } => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return None,
                    };
                    if ok_arm.is_some() {
                        return None;
                    }
                    ok_arm = Some((&arm.body, bind));
                }
                IrPattern::Err { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, .. } => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return None,
                    };
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
        let ((ok_body, ok_bind), (err_body, err_bind)) = match (ok_arm, err_arm) {
            (Some(o), Some(e)) => (o, e),
            _ => return None,
        };
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize the subject to an OWNED tracked temp (a call result is fresh-owned; a
        // Var is borrowed — Dup it so the scope-end drop discipline is uniform). The temp is
        // in live_heap_handles → freed by the epilogue AFTER the merge move-out.
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
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
        if let Some(var) = err_bind {
            let payload = if is_heap_ty(&err_pay_ty) {
                let p = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.param_values.insert(p);
                p
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(var, payload);
        }
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let err_obj = match self.lower_heap_result_arm(err_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_err: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(err_obj) });
        // ELSE (tag == 0 = Ok): a scalar payload copies; a HEAP payload (cap-as-tag
        // subject) binds the @12 handle as a BORROW — the subject temp still owns it
        // (freed by the epilogue), an arm that returns it acquires its own ref (Dup).
        if let Some(var) = ok_bind {
            let payload = if heap_ok {
                let p = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.param_values.insert(p);
                p
            } else {
                let _ = &ok_pay_ty;
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(var, payload);
        }
        let live_after_err: Vec<ValueId> = self.live_heap_handles.clone();
        let ok_obj = match self.lower_heap_result_arm(ok_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_ok: Vec<ValueId> = live_after_err
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        // Release parity across the arms (the lower_heap_result_if_inner discipline).
        for x in &consumed_by_err {
            if !consumed_by_ok.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_ok {
            if !consumed_by_err.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(ok_obj) });
        Some(dst)
    }

    /// TAIL-VALUE match over a LIST subject with exactly one `[]` arm and one catch-all
    /// (`_` or a bind-all `ys`) — the len-tag twin of [`Self::try_lower_result_match_value`]:
    ///   `match list.filter(xs, f) { [] => None, ys => list.get(ys, 0) }`
    /// The subject is an OWNED tracked temp (a call result is fresh-owned; a Var is Dup'd).
    /// tag = len@4: THEN (len != 0) = the non-empty arm — a bind-all var ALIASES the subject
    /// temp itself (arm calls borrow it; if the arm MOVES it out, the release-parity sweep
    /// compensates with a drop on the empty side). ELSE (len == 0) = the `[]` arm. Same
    /// IfThen/Else/EndIf merge + release-parity discipline as the Result opener.
    pub(crate) fn try_lower_list_match_value(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(result_ty) || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        if !matches!(&subject.ty, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1) {
            return None;
        }
        let mut empty_arm: Option<&IrExpr> = None;
        let mut rest_arm: Option<(&IrExpr, Option<VarId>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::List { elements } if elements.is_empty() => {
                    if empty_arm.is_some() {
                        return None;
                    }
                    empty_arm = Some(&arm.body);
                }
                IrPattern::Bind { var, .. } => {
                    if rest_arm.is_some() {
                        return None;
                    }
                    rest_arm = Some((&arm.body, Some(*var)));
                }
                IrPattern::Wildcard => {
                    if rest_arm.is_some() {
                        return None;
                    }
                    rest_arm = Some((&arm.body, None));
                }
                _ => return None,
            }
        }
        let (empty_body, (rest_body, rest_bind)) = match (empty_arm, rest_arm) {
            (Some(e), Some(r)) => (e, r),
            _ => return None,
        };
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        // THEN (len != 0): the non-empty arm; the bind-all aliases the subject temp.
        if let Some(var) = rest_bind {
            self.value_of.insert(var, subj);
        }
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let rest_obj = match self.lower_heap_result_arm(rest_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_rest: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(rest_obj) });
        // ELSE (len == 0): the `[]` arm.
        let live_after_rest: Vec<ValueId> = self.live_heap_handles.clone();
        let empty_obj = match self.lower_heap_result_arm(empty_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_empty: Vec<ValueId> = live_after_rest
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        // Release parity across the arms (the lower_heap_result_if_inner discipline).
        for x in &consumed_by_rest {
            if !consumed_by_empty.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_empty {
            if !consumed_by_rest.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(empty_obj) });
        Some(dst)
    }

    /// Is `pat` structurally valid for the NESTED-refinement chain over variant
    /// `tyname` — every ctor resolves in the layout with matching arity, every
    /// nested arg a wildcard / Int-Bool literal (over a scalar field) / a ctor
    /// over a variant-typed field (recursively)? A Bind or any other shape → false
    /// (the chain has no arm-scope payload binds).
    fn nested_refinement_pat_valid(&self, pat: &IrPattern, tyname: &str) -> bool {
        let IrPattern::Constructor { name, args } = pat else { return false };
        let Some(layout) = self.variant_layouts.by_type.get(tyname) else { return false };
        let Some(case) = layout.cases.iter().find(|c| c.ctor.as_str() == name.as_str()) else {
            return false;
        };
        if args.len() != case.fields.len() && !args.is_empty() {
            return false;
        }
        args.iter().zip(case.fields.iter()).all(|(a, (_, fty))| match a {
            IrPattern::Wildcard => true,
            IrPattern::Literal { expr } => {
                !is_heap_ty(fty)
                    && matches!(expr.kind, IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. })
            }
            IrPattern::Constructor { .. } => self
                .custom_variant_type_name(fty)
                .is_some_and(|inner| self.nested_refinement_pat_valid(a, &inner)),
            _ => false,
        })
    }

    /// Emit the SHORT-CIRCUIT refinement condition for `pat` over the (borrowed)
    /// variant block `block` of type `tyname`: outer tag equality, and — ONLY when
    /// it holds (a FLAT-MARKER if, so an out-of-case slot is never dereferenced) —
    /// the conjunction (0/1 multiply) of every nested tag/literal refinement.
    /// All reads are borrows (Handle/Load/LoadHandle — no ownership event).
    /// Caller guarantees [`Self::nested_refinement_pat_valid`].
    fn nested_refinement_cond(
        &mut self,
        block: ValueId,
        pat: &IrPattern,
        tyname: &str,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        let IrPattern::Constructor { name, args } = pat else { return None };
        let layout = self.variant_layouts.by_type.get(tyname)?.clone();
        let case = layout.cases.iter().find(|c| c.ctor.as_str() == name.as_str())?.clone();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![block] });
        let tagv = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
        let want = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: want, value: case.tag as i64 });
        let eq = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: eq, op: IntOp::Eq, a: tagv, b: want });
        let refut: Vec<(usize, &IrPattern)> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, IrPattern::Constructor { .. } | IrPattern::Literal { .. }))
            .collect();
        if refut.is_empty() {
            return Some(eq);
        }
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: eq, dst: Some(dst) });
        let mut conj: Option<ValueId> = None;
        for (i, sub) in refut {
            let off = (12 + 8 * (i + 1)) as i64;
            let ci = match sub {
                IrPattern::Literal { expr } => {
                    let fv = self.load_at_offset(h, off, PrimKind::Load { width: 8 });
                    let lit = self.fresh_value();
                    let value = match &expr.kind {
                        IrExprKind::LitInt { value } => *value,
                        IrExprKind::LitBool { value } => *value as i64,
                        _ => return None,
                    };
                    self.ops.push(Op::ConstInt { dst: lit, value });
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Eq, a: fv, b: lit });
                    c
                }
                IrPattern::Constructor { .. } => {
                    let fblock = self.load_at_offset(h, off, PrimKind::LoadHandle);
                    let inner = self.custom_variant_type_name(&case.fields[i].1)?;
                    self.nested_refinement_cond(fblock, sub, &inner)?
                }
                _ => unreachable!("filtered above"),
            };
            conj = Some(match conj {
                None => ci,
                Some(prev) => {
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Mul, a: prev, b: ci });
                    c
                }
            });
        }
        self.ops.push(Op::Else { val: Some(conj?) });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::EndIf { val: Some(zero) });
        Some(dst)
    }

    /// The ordered arm chain of the nested-refinement match: `if cond1 then body1
    /// else if cond2 then body2 … else <irrefutable body>` over the flat markers.
    fn nested_refinement_chain(
        &mut self,
        subj: ValueId,
        arms: &[IrMatchArm],
        tyname: &str,
    ) -> Option<ValueId> {
        let arm = arms.first()?;
        if matches!(arm.pattern, IrPattern::Wildcard) {
            return self.lower_scalar_arm(&arm.body);
        }
        let cond = self.nested_refinement_cond(subj, &arm.pattern, tyname)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        let then_v = self.lower_scalar_arm(&arm.body)?;
        self.ops.push(Op::Else { val: Some(then_v) });
        let rest_v = self.nested_refinement_chain(subj, &arms[1..], tyname)?;
        self.ops.push(Op::EndIf { val: Some(rest_v) });
        Some(dst)
    }

    /// EXECUTE a `match` over a custom variant with NESTED refutable constructor
    /// patterns (`Node(Red, Node(_, _, 5, _), _, _)` — the C-070 boxed-refinement
    /// class) as an ORDERED refinement chain: each arm's condition is the outer tag
    /// equality plus every inner tag/literal constraint, evaluated SHORT-CIRCUIT
    /// (inner slots read only under their ctor's matching tag — no out-of-case
    /// dereference), then dispatched first-match-wins via the flat-marker if chain.
    /// ADMITTED: ctor/wildcard/Int-Bool-literal args (a BINDER would need arm-scope
    /// payload binds — declined), a SCALAR result, no guards, a trailing wildcard,
    /// and a Var subject that is a real block (param / materialized). Every payload
    /// read is a BORROW; the whole attempt rolls back on any miss.
    fn try_lower_nested_refinement_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if is_heap_ty(result_ty) {
            return None;
        }
        let type_name = self.custom_variant_type_name(&subject.ty)?;
        // Only fire when some arm actually NESTS a refutable arg — the flat
        // tag-switch machinery owns every other shape.
        let has_nested = |p: &IrPattern| {
            matches!(p, IrPattern::Constructor { args, .. }
                if args.iter().any(|a| matches!(a,
                    IrPattern::Constructor { .. } | IrPattern::Literal { .. })))
        };
        if !arms.iter().any(|a| has_nested(&a.pattern)) {
            return None;
        }
        if !matches!(arms.last()?.pattern, IrPattern::Wildcard) {
            return None;
        }
        if !arms[..arms.len() - 1]
            .iter()
            .all(|a| self.nested_refinement_pat_valid(&a.pattern, &type_name))
        {
            return None;
        }
        // The subject must be a REAL block: a borrowed variant param or a locally
        // materialized value (a deferred Opaque's tag read would be garbage).
        let IrExprKind::Var { id } = &subject.kind else { return None };
        let subj = self.value_for(*id).ok()?;
        if !self.param_values.contains(&subj) && !self.materialized_aggregates.contains(&subj) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match self.nested_refinement_chain(subj, arms, &type_name) {
            Some(dst) => Some(dst),
            None => {
                if std::env::var("ALMIDE_DBG_NESTED_MATCH").is_ok() {
                    eprintln!("[nested-match] chain declined for {}", self.fn_name);
                }
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// EXECUTE a `match (a, b, …) { (P1, P2, …) => …, _ => … }` over a TUPLE of
    /// BOUND VARS — the frontend's factored form of a multi-arm nested-ctor match
    /// (`match t { Node(Red, Node(...), _, _) => …, Node(...) => …, … }` desugars
    /// to ONE outer `Node(a,b,c,d)` arm whose body re-matches `(a,b,c,d)` — the
    /// C-070 class). Each arm's condition is the CONJUNCTION of per-element
    /// refinements (a variant element via [`Self::nested_refinement_cond`], a
    /// scalar element via literal equality; wildcards free), dispatched
    /// first-match-wins on the flat-marker chain. Scalar result, no guards, a
    /// trailing wildcard, all elements plain Vars.
    pub(crate) fn try_lower_tuple_refinement_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let dbg = std::env::var("ALMIDE_DBG_NESTED_MATCH").is_ok();
        if dbg {
            eprintln!("[tuple-match] entry: subj={:?} arms={}", std::mem::discriminant(&subject.kind), arms.len());
        }
        if arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        let IrExprKind::Tuple { elements } = &subject.kind else { return None };
        if !matches!(arms.last()?.pattern, IrPattern::Wildcard) {
            if dbg {
                eprintln!("[tuple-match] last arm not wildcard");
            }
            return None;
        }
        // The marks precede the ELEMENT resolution: a scalar-EXPRESSION element
        // (`n % 3` — the fizz shape) ANF-lowers here, so every decline below must
        // roll its ops back.
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let mut rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
        };
        // Elements: a plain Var (variant block or scalar), or a SCALAR expression
        // materialized to a fresh value (read once — exactly the by-value tuple
        // subject semantics; the match reads only these copies).
        let mut elems: Vec<(ValueId, Ty)> = Vec::with_capacity(elements.len());
        for e in elements {
            let v = match &e.kind {
                IrExprKind::Var { id } => match self.value_for(*id) {
                    Ok(v) => v,
                    Err(_) => {
                        rollback(self);
                        return None;
                    }
                },
                _ if !is_heap_ty(&e.ty) => match self.lower_scalar_value(e) {
                    Some(v) => v,
                    None => {
                        rollback(self);
                        return None;
                    }
                },
                _ => {
                    rollback(self);
                    return None;
                }
            };
            elems.push((v, e.ty.clone()));
        }
        // Validate every refutable arm up front (no mid-emission decline).
        for a in &arms[..arms.len() - 1] {
            let IrPattern::Tuple { elements: pats } = &a.pattern else {
                rollback(self);
                return None;
            };
            if pats.len() != elems.len() {
                rollback(self);
                return None;
            }
            for (p, (_, ty)) in pats.iter().zip(elems.iter()) {
                let ok = match p {
                    IrPattern::Wildcard => true,
                    IrPattern::Literal { expr } => {
                        !is_heap_ty(ty)
                            && matches!(
                                expr.kind,
                                IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. }
                            )
                    }
                    IrPattern::Constructor { .. } => self
                        .custom_variant_type_name(ty)
                        .is_some_and(|n| self.nested_refinement_pat_valid(p, &n)),
                    _ => false,
                };
                if !ok {
                    rollback(self);
                    return None;
                }
            }
        }
        match self.tuple_refinement_chain(&elems, arms, result_ty) {
            Some(dst) => Some(dst),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// The UNIT-statement sibling of [`Self::try_lower_tuple_refinement_match`] —
    /// the heap-branch tail duplication turns `let s = match (a, b) { … }; use(s)`
    /// into a STATEMENT match whose arms carry the continuation's effects, so the
    /// refinement chain must run them under REAL `IfThen`/`Else`/`EndIf` markers
    /// (only the taken arm executes — `unit_arm_depth` raised per arm, exactly the
    /// `lower_variant_unit_arm` discipline). Returns `true` iff fully lowered;
    /// rolls back and returns `false` on any decline.
    pub(crate) fn try_lower_tuple_refinement_unit_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
    ) -> bool {
        if arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
            return false;
        }
        let IrExprKind::Tuple { elements } = &subject.kind else { return false };
        if !matches!(arms.last().map(|a| &a.pattern), Some(IrPattern::Wildcard)) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let mut elems: Vec<(ValueId, Ty)> = Vec::with_capacity(elements.len());
        for e in elements {
            let v = match &e.kind {
                IrExprKind::Var { id } => self.value_for(*id).ok(),
                _ if !is_heap_ty(&e.ty) => self.lower_scalar_value(e),
                _ => None,
            };
            match v {
                Some(v) => elems.push((v, e.ty.clone())),
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return false;
                }
            }
        }
        let mut valid = true;
        'outer: for a in &arms[..arms.len() - 1] {
            let IrPattern::Tuple { elements: pats } = &a.pattern else {
                valid = false;
                break;
            };
            if pats.len() != elems.len() {
                valid = false;
                break;
            }
            for (p, (_, ty)) in pats.iter().zip(elems.iter()) {
                let ok = match p {
                    IrPattern::Wildcard => true,
                    IrPattern::Literal { expr } => {
                        !is_heap_ty(ty)
                            && matches!(
                                expr.kind,
                                IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. }
                            )
                    }
                    IrPattern::Constructor { .. } => self
                        .custom_variant_type_name(ty)
                        .is_some_and(|n| self.nested_refinement_pat_valid(p, &n)),
                    _ => false,
                };
                if !ok {
                    valid = false;
                    break 'outer;
                }
            }
        }
        if !valid || !self.tuple_refinement_unit_chain(&elems, arms) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        true
    }

    fn tuple_refinement_unit_chain(
        &mut self,
        elems: &[(ValueId, Ty)],
        arms: &[IrMatchArm],
    ) -> bool {
        let Some(arm) = arms.first() else { return true };
        let lower_unit_arm = |s: &mut Self, body: &IrExpr| -> bool {
            let mark = s.live_heap_handles.len();
            s.unit_arm_depth += 1;
            let r = s.lower_branch_arm(None, body);
            s.unit_arm_depth -= 1;
            if r.is_err() {
                return false;
            }
            s.drop_arm_locals(mark);
            true
        };
        if matches!(arm.pattern, IrPattern::Wildcard) {
            return lower_unit_arm(self, &arm.body);
        }
        let IrPattern::Tuple { elements: pats } = &arm.pattern else { return false };
        let mut conj: Option<ValueId> = None;
        for (p, (v, ty)) in pats.iter().zip(elems.iter()) {
            let ci = match p {
                IrPattern::Wildcard => continue,
                IrPattern::Literal { expr } => {
                    let lit = self.fresh_value();
                    let value = match &expr.kind {
                        IrExprKind::LitInt { value } => *value,
                        IrExprKind::LitBool { value } => *value as i64,
                        _ => return false,
                    };
                    self.ops.push(Op::ConstInt { dst: lit, value });
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Eq, a: *v, b: lit });
                    c
                }
                IrPattern::Constructor { .. } => {
                    let Some(vname) = self.custom_variant_type_name(ty) else { return false };
                    match self.nested_refinement_cond(*v, p, &vname) {
                        Some(c) => c,
                        None => return false,
                    }
                }
                _ => return false,
            };
            conj = Some(match conj {
                None => ci,
                Some(prev) => {
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Mul, a: prev, b: ci });
                    c
                }
            });
        }
        let Some(cond) = conj else {
            return lower_unit_arm(self, &arm.body);
        };
        self.ops.push(Op::IfThen { cond, dst: None });
        if !lower_unit_arm(self, &arm.body) {
            return false;
        }
        self.ops.push(Op::Else { val: None });
        if !self.tuple_refinement_unit_chain(elems, &arms[1..]) {
            return false;
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }
}
