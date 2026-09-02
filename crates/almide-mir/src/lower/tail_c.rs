// ── tail of tail_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

impl LowerCtx {
    /// Extracted from `Self::lower_tail_heap` (fourth-round split, cog reduction): the
    /// `Call{Computed}` closure arm body, verbatim, re-narrowed via `let-else`.
    fn lower_tail_heap_call_computed(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &tail.kind else { unreachable!() };
        let mark = self.live_heap_handles.len();
        let blk = self.closure_value_of(callee).expect("the caller's match guard already proved closure_value_of(callee).is_some() for the same callee");
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        let repr = repr_of(&tail.ty)?;
        self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
        self.drop_arm_locals(mark);
        Ok(Some(dst))
    }

    /// The SCALAR tail of [`Self::lower_tail`] (Copy value, no ownership).
    /// Verbatim text move. Since the codopsy round-2 sweep (#852) this is the
    /// dispatch-only ROUTER: each heavy arm's body moved verbatim into a
    /// `lower_tail_scalar_*` helper below, tried in the ORIGINAL arm order.
    fn lower_tail_scalar(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // Scalar return value (Copy — no ownership accounting). A scalar `BinOp`/
        // `UnOp` is a FRESH computed scalar (arithmetic / comparison / logic), so it
        // is a `Const` like a literal — its operands carry their own ownership.
        match &tail.kind {
            IrExprKind::Var { id } => Ok(Some(self.value_or_global(*id)?)),
            // A scalar-result resolvable CALL tail (`fn f() = g()`, `= add(2, 3)`,
            // `= string.len(s)`): a real executable `CallFn` (args materialized, the
            // scalar result returned). An unresolvable Method/Computed callee (or an
            // unsupported arg) falls through to the deferred `Const` + elided-caps
            // marker below — the call is captured for caps, its value deferred.
            IrExprKind::Call { .. } => self.lower_tail_scalar_call(tail),
            // An INT literal materializes to a real value (the scalar-value
            // foundation): `ConstInt` renders `(i64.const v)`, so a fn returning a
            // literal returns the right value, not the deferred-`Const` zero. This is
            // what lets a self-hosted runtime fn compute real offsets/lengths.
            IrExprKind::LitInt { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value });
                Ok(Some(dst))
            }
            // A FLOAT literal returned directly (`fn pi() = 3.14159`) materializes its REAL f64
            // BITS as a `ConstInt` (the i64-uniform Float repr), so the fn returns the constant,
            // not the deferred-`Const` zero — the same materialization `lower_scalar_value` does
            // for a LitFloat operand. (The frontend folds `{ let p = 3.14; p }` to this form.)
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: crate::lower::float_lit_bits(*value, &tail.ty) });
                Ok(Some(dst))
            }
            // A BOOL literal returned directly (`(x) => true` — a constant/param-ignoring predicate
            // for list.all/any/count, or `fn t() = true`) materializes its 0/1 as a `ConstInt`, NOT
            // the deferred-`Const` ZERO it used to fall into below (which made `(x) => true` return
            // FALSE — a silent miscompile of every constant-true predicate). Bool is an i64 0/1, the
            // same materialization lower_scalar_value does for a LitBool operand.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value as i64 });
                Ok(Some(dst))
            }
            // A scalar Int Add/Sub/Mul computes its REAL value (IntBinOp over
            // recursively-lowered operands), so a fn `add(a, b) = a + b` returns the
            // sum — not the deferred-Const zero. Outside the int-arith subset (Div/
            // Mod/cmp/logic/Float) it rolls back and falls through to the Const below.
            // A scalar Int Add/Sub/Mul OR a scalar prim-floor call (`= prim.load32(a)`)
            // computes a real value via lower_scalar_value (IntBinOp / Op::Prim);
            // outside the subset it rolls back to the deferred Const + elided marker.
            IrExprKind::BinOp { .. } | IrExprKind::RuntimeCall { .. } => {
                self.lower_tail_scalar_value_or_deferred(tail)
            }
            // A SCALAR field / tuple element / list element TAIL (`(p) => p.x`, `fn fst(t) = t.0`,
            // `fn at(xs, i) = xs[i]`) — LOAD the real value from the materialized aggregate's layout
            // slot (the VALUE MODEL read side, what makes `list.map(points, (p)=>p.x)` return the
            // real field); `xs[i]` is the bounds-checked `$elem_addr` load. `lower_scalar_value`
            // dispatches each. Outside the materialized subset it rolls back to the deferred `Const`
            // (its container's calls elided), exactly as before.
            IrExprKind::Member { .. }
            | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } => {
                self.lower_tail_scalar_value_or_deferred(tail)
            }
            // A scalar UNARY op RETURNED directly (`fn ineg(n) = -n`, `fn flip(b) = not b`,
            // `fn fneg(x) = -x`) computes its REAL value via `lower_scalar_value` (the
            // UnOp arm: int neg `0 - x`, float neg the `f64.neg` prim, bool `not` `1 - b`)
            // — NOT the deferred-`Const` zero this used to fall into. This is the TAIL-
            // position twin of the value-position UnOp fix: a function whose body IS a
            // `UnOp` is a value position, so it must compute, not read 0. Outside the
            // scalar subset (a non-lowerable operand) it rolls back to the Const below,
            // exactly like the `BinOp` tail arm.
            IrExprKind::UnOp { .. } if !is_heap_ty(&tail.ty) => {
                self.lower_tail_scalar_value_or_deferred(tail)
            }
            // A SCALAR map extraction is an unambiguous COPY (a scalar is never
            // reference-counted), so it is a `Const` — its container carries its own
            // ownership. (A HEAP extraction is an ALIAS / share — it needs a layout-aware
            // field-access op with `Dup` semantics and stays walled until that brick.)
            IrExprKind::MapAccess { .. }
            // A SCALAR error-operator result (`x?.f` yielding a scalar) is
            // likewise a fresh `Const`; the operator's value + early-return are deferred.
            | IrExprKind::Try { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE returned: a fresh `Const` (no ownership); any analyzable callee
            // inside it is captured for caps by `record_elided_calls`. (A scalar-result
            // CALL is handled by its own arm above — a real executable `CallFn` when
            // resolvable, else the same deferred `Const` + elided marker.)
            | IrExprKind::Range { .. } => self.lower_tail_scalar_deferred_const(tail),
            // A TAIL `e!` (Unwrap — effect-fn error propagation): `f() = g()!` propagates g's
            // Result unchanged, i.e. it IS `f() = g()`. Strip the `!` and lower `e` as the tail.
            IrExprKind::Unwrap { expr } => self.lower_tail(Some(expr)),
            // A SCALAR tail `??` (`fn parse_or_zero(s) = int.parse(s) ?? 0`, the canonical
            // form) EXECUTES the unwrap (tag read + payload-or-fallback) — it was a deferred
            // `Const` 0 here (a silent wrong value, neither payload nor fallback). Outside the
            // executable subset a `??` over a VARIANT operand WALLs (a Const-0 would be wrong);
            // a non-variant operand keeps the deferred `Const`.
            IrExprKind::UnwrapOr { .. } => self.lower_tail_scalar_unwrap_or(tail),
            // A scalar `if` tail EXECUTES (only the taken arm runs) via try_lower_scalar_if
            // — the IfThen/Else/EndIf markers — when the cond + both arms are in the
            // scalar subset; otherwise it falls back to the deferred linearize + Const.
            IrExprKind::If { .. } => self.lower_tail_scalar_if(tail),
            // A scalar-result `match` over INT literal patterns EXECUTES: desugar to a
            // nested `if subject == lit then arm else …` and lower it via the scalar-if
            // machinery (only the matched arm runs). Non-literal patterns / guards / a
            // non-scalar subject fall back to the deferred linearize + merged `Const`.
            IrExprKind::Match { .. } => self.lower_tail_scalar_match(tail),
            other => Err(LowerError::Unsupported(format!(
                "scalar tail {} not in this brick",
                kind_name(other)
            ))),
        }
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// scalar `Call` arm body, verbatim — try the real executable `CallFn`, else the
    /// deferred `Const` + elided-caps marker.
    fn lower_tail_scalar_call(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.try_lower_scalar_call(tail, &tail.ty) {
            return Ok(Some(dst));
        }
        self.lower_tail_scalar_deferred_const(tail)
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// body shared — textually identical — by the `BinOp`/`RuntimeCall`,
    /// `Member`/`TupleIndex`/`IndexAccess`, and scalar-`UnOp` arms, verbatim: compute
    /// the REAL scalar value via `lower_scalar_value`; outside its subset roll the op
    /// stream back and fall to the deferred `Const` + elided marker.
    fn lower_tail_scalar_value_or_deferred(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let mark = self.ops.len();
        if let Some(dst) = self.lower_scalar_value(tail) {
            return Ok(Some(dst));
        }
        self.ops.truncate(mark);
        self.lower_tail_scalar_deferred_const(tail)
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// deferred `Const` + elided-caps marker every out-of-subset scalar tail fell to,
    /// verbatim (a strict-values build walls instead of deferring).
    fn lower_tail_scalar_deferred_const(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let dst = self.fresh_value();
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall("tail"));
        }
        self.ops.push(Op::Const { dst });
        self.record_elided_calls(tail);
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// `UnwrapOr` arm body, verbatim, re-narrowed via `let-else` — execute the unwrap
    /// when possible, WALL a variant operand outside the executable subset, keep the
    /// deferred `Const` for a non-variant operand.
    fn lower_tail_scalar_unwrap_or(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::UnwrapOr { expr, fallback } = &tail.kind else { unreachable!() };
        if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, false) {
            return Ok(Some(dst));
        }
        if is_variant_ty(&expr.ty) {
            return Err(LowerError::Unsupported(
                "?? over an Option/Result operand in tail position outside the \
                 executable subset cannot be faithfully computed (a Const-0 would be \
                 a wrong value) not in this brick"
                    .into(),
            ));
        }
        self.lower_tail_scalar_deferred_const(tail)
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// scalar-`if` arm body, verbatim, re-narrowed via `let-else` — execute only the
    /// taken arm when the scalar subset allows, else the deferred linearize + `Const`.
    fn lower_tail_scalar_if(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::If { cond, then, else_ } = &tail.kind else { unreachable!() };
        if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, &tail.ty) {
            return Ok(Some(dst));
        }
        self.lower_tail_scalar_linearized_const(tail)
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// deferred linearize + merged `Const` fallback shared — textually identical — by
    /// the scalar-`if`, Unit-result variant-`match`, and scalar-`match` fallback
    /// paths, verbatim: run the branch for effects/caps (only the taken arm when the
    /// subject is trackable), merge the deferred value as a `Const` (NO elided-calls
    /// record — `lower_branch` already captured the caps).
    fn lower_tail_scalar_linearized_const(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        self.lower_branch(tail)?;
        let dst = self.fresh_value();
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall("tail"));
        }
        self.ops.push(Op::Const { dst });
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// scalar-`match` arm body, verbatim, re-narrowed via `let-else` — the ordered
    /// attempt chain (tuple-extract slot load, custom-variant dispatch, tuple
    /// refinement, variant value-match, literal-desugar to `if`) in the ORIGINAL
    /// order, then the deferred linearize + merged `Const` fallback.
    fn lower_tail_scalar_match(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        let IrExprKind::Match { subject, arms } = &tail.kind else { unreachable!() };
        if let Some(dst) = self.try_lower_tuple_extract_scalar_slot(subject, arms) {
            return Ok(Some(dst));
        }
        // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
        // `fn val(t: Tok) -> Int = match t { Num(n) => n, … }`.
        if let Some(dst) =
            self.try_lower_custom_variant_match(subject, arms, &tail.ty)
        {
            return Ok(Some(dst));
        }
        // A TUPLE subject of scalar elements/expressions with a SCALAR result
        // (`match (a % 2, b % 3) { (0, 0) => 100, … }`) — the ordered
        // refinement chain (the scalar sibling of the heap-tail hook).
        if let Some(dst) = self.try_lower_tuple_refinement_match(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // A VARIANT (Option/Result) subject returned by a function — execute the
        // tag-read value-match (only the taken arm runs, the scalar payload bound);
        // `fn pick(o) = match o { Some(x) => x, None => -1 }` is the canonical form.
        // A ctor pattern is not `subj == lit`, so it can't reach `desugar_match_to_if`.
        if is_variant_ty(&subject.ty) {
            return self.lower_tail_scalar_variant_match(subject, arms, tail);
        }
        if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &tail.ty) {
            // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
            // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
            let mark = self.ops.len();
            let lhh = self.live_heap_handles.len();
            if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                return Ok(Some(dst));
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh);
        }
        self.lower_tail_scalar_linearized_const(tail)
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// tuple-destructure attempt heading the scalar-`match` arm, verbatim — decides
    /// whether the match is really a `t.<i>` scalar slot load and lowers it via the
    /// scalar value model, rolling the op stream back on a miss.
    fn try_lower_tuple_extract_scalar_slot(
        &mut self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Option<ValueId> {
        // A single-arm tuple-destructure `match t { (_, n) => n }` extracting ONE SCALAR
        // component — semantically `t.<i>` (the tuple-accumulator `fold` cursor extraction).
        // Lower the synthetic `TupleIndex` via the scalar value model (a real slot load).
        if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
            if !is_heap_ty(&elem_ty) {
                let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(&synth) {
                    return Some(dst);
                }
                self.ops.truncate(mark);
            }
        }
        None
    }

    /// Extracted from `Self::lower_tail_scalar` (codopsy round-2 sweep, #852): the
    /// variant (Option/Result) subject leg of the scalar-`match` arm, verbatim —
    /// execute the tag-read value-match, delegate a Unit-result match to
    /// `lower_branch`, and WALL the rest (a Const-0 would silently pick a wrong arm).
    fn lower_tail_scalar_variant_match(
        &mut self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
        tail: &IrExpr,
    ) -> Result<Option<ValueId>, LowerError> {
        if let Some(dst) = self.try_lower_variant_value_match(subject, arms, &tail.ty) {
            return Ok(Some(dst));
        }
        // A UNIT-result tail variant match (`match write_summary(..) { ok(p) =>
        // {…effects…}, err(e) => {…effects…} }` — the run_all_finish shape): the arms
        // produce no VALUE, only effects, so there is nothing to "pick" — this is
        // exactly the statement/Unit-position dispatch `lower_branch` already executes
        // (track the Result subject → `try_lower_result_match` reads the tag and runs
        // ONLY the taken arm; an untrackable subject linearizes both arms, the
        // existing caps-union-sound fallback). DELEGATE to it rather than wall — the
        // function's Unit return is the merged `Const` below (no value escapes the
        // branch). The same proven machinery every non-tail Unit match uses; gated to
        // `Unit` so a SCALAR/HEAP-result variant match (whose value DOES matter) keeps
        // walling here (`lower_branch` would discard its value = a silent miscompile).
        if matches!(tail.ty, Ty::Unit) {
            return self.lower_tail_scalar_linearized_const(tail);
        }
        Err(LowerError::Unsupported(
            "variant (Option/Result) match in tail position outside the \
             executable subset cannot be faithfully computed (a Const-0 would \
             silently pick a wrong arm) not in this brick"
                .into(),
        ))
    }
}

impl LowerCtx {
    /// #1134: the TAIL-position heap `??`. `r ?? fb` returns the payload when
    /// present and the fallback otherwise — exactly `match r { ok($p) => $p,
    /// err(_) => fb }` (Option's polarity is the some/none pair). Building
    /// that match here hands the construct to `lower_tail_heap_match`, whose
    /// executable subset is already proven; nothing else in the pipeline
    /// changes, so the shapes that lower today keep their own paths.
    fn lower_tail_heap_unwrap_or(&mut self, tail: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        use almide_ir::{IrMatchArm, IrPattern, VarId};
        let IrExprKind::UnwrapOr { expr, fallback } = &tail.kind else { unreachable!() };
        // ONLY the Result polarity was walled. A tail-position OPTION `??`
        // already lowers through `lower_call_args` → `option.unwrap_or_str`,
        // a proven executable path — rewriting it into a match takes a
        // working shape off that path for nothing, which is the same
        // over-reach that made the first (whole-body) version of this fix
        // regress the C-149 share shapes. Pinned by
        // `heap_unwrap_or_tail_position_executes`, which asserts the
        // `option.unwrap_or_str` route survives.
        if !expr.ty.is_result() {
            return self.lower_tail_heap_fresh(tail);
        }
        let payload_ty = tail.ty.clone();
        let p = VarId(crate::lower::desugar_var_seed());
        let bind = IrPattern::Bind { var: p, ty: payload_ty.clone() };
        let (hit, miss) = if expr.ty.is_result() {
            (
                IrPattern::Ok { inner: Box::new(bind) },
                IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
            )
        } else {
            (IrPattern::Some { inner: Box::new(bind) }, IrPattern::None)
        };
        let payload = IrExpr {
            kind: IrExprKind::Var { id: p },
            ty: payload_ty,
            span: tail.span.clone(),
            def_id: None,
        };
        let rewritten = IrExpr {
            kind: IrExprKind::Match {
                subject: expr.clone(),
                arms: vec![
                    IrMatchArm { pattern: hit, guard: None, body: payload },
                    IrMatchArm { pattern: miss, guard: None, body: (**fallback).clone() },
                ],
            },
            ty: tail.ty.clone(),
            span: tail.span.clone(),
            def_id: tail.def_id,
        };
        self.lower_tail_heap_match(&rewritten)
    }
}
