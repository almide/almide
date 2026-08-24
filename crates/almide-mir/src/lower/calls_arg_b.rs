// ── Call-argument dispatch: the operator and call groups ─────────
//
// The second half of the table `lower_call_arg_into` routes between, split from
// `calls_arg.rs` so neither file is oversized. `include!`d from there.

impl LowerCtx {
    /// `??`, the unwrap family, ranges and the operator forms.
    ///
    /// One group of `lower_call_arg_into`'s dispatch table, arms verbatim and in
    /// source order. `Ok(None)` means "not my group" — the router tries the groups
    /// in that order, so which rule an argument takes is unchanged, and an `Err`
    /// still aborts the whole call as before.
    ///
    /// Each arm's BODY now lives in its own helper below (codopsy round-2 complexity
    /// sweep, cyc 58 / cog 75): this is a pure dispatch table, so the arm patterns,
    /// their guards and their order — and therefore which rule an argument takes —
    /// are byte-for-byte the ones that were here before.
    fn lower_call_arg_operator(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<Option<ArgOutcome>, LowerError> {
        let _ = &out;
        match &a.kind {
            IrExprKind::UnwrapOr { expr, fallback } => {
                self.lower_call_arg_unwrap_or(a, expr, fallback).map(Some)
            }
            // A scalar-result `match` over a HEAP subject must EXECUTE: a VARIANT
            // (Option/Result) via the tag-read value-match, a scalar-pattern subject via
            // the desugared if-chain. If it falls outside the executable subset (e.g. a
            // `match s { "a" => 1, _ => 9 }` over a String — string equality is not yet
            // lowered) a Const-0 fallback would SILENTLY pick a wrong arm, so WALL it. The
            // executing forms (`match o`/`match list.get(..)`/`match n { 1 => .. }`)
            // return a real `CallArg::Scalar` here.
            IrExprKind::Match { subject, .. }
                if !is_heap_ty(&a.ty) && is_heap_ty(&subject.ty) =>
            {
                self.lower_call_arg_scalar_match_over_heap(a).map(Some)
            }
            // A fresh BinOp/UnOp result as an argument (`f(a + b)`, `f(-n)`), or an
            // ERROR OPERATOR result (`f(x!)`, `f(x ?? d)`, `f(x?.field)`): a fresh
            // computed value — a heap result is materialized via `Alloc` (borrowed
            // and dropped), a scalar result is a `Const`. Operands carry their own
            // ownership; the operator's value (and any early-return) is deferred.
            // An `f(x!)` (Unwrap — effect-fn error propagation) as a call ARGUMENT was a
            // deferred `Const`/`Opaque` = a SILENT MISCOMPILE (`f(int.parse(s)!)` passed 0).
            // The faithful lowering needs early-return-on-Err (a later brick); until then
            // WALL it — NEVER pass a silently-wrong value (the ② cardinal rule).
            IrExprKind::Unwrap { .. } => Err(LowerError::Unsupported(
                "unwrap `!` in a call-argument position cannot be faithfully computed \
                 (needs early-return propagation; a Const/Opaque would be a silently \
                 wrong value) not in this brick"
                    .into(),
            )),
            // A RANGE argument with SCALAR bounds (`f(0..n)` — the gguf
            // parse_metadata_entries `for _ in 0..count` append-accumulator desugar):
            // materialize the REAL list via the self-hosted `list.range` (a fresh owned
            // List[Int], borrowed into the call, dropped at scope end). An inclusive
            // range widens the end by one (`a..=b` = `range(a, b+1)`), exactly v0's
            // iteration space. Non-scalar bounds still wall below.
            IrExprKind::Range { start, end, inclusive } if is_heap_ty(&a.ty) => self
                .lower_call_arg_range_list(a, start, end, *inclusive, out)
                .map(Some),
            IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. }
            | IrExprKind::Try { .. }
            // (UnwrapOr is handled in full — scalar + heap — by the dedicated arm above.)
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE (`f(0..n)`), a RUNTIME CALL, or an `if`/`match` ARGUMENT is a
            // fresh value of the same shape — a deferred `Alloc{Opaque}`/`Const`,
            // its calls (incl. the branch arms' calls) captured by
            // `record_elided_calls`; the arms' values/effects are deferred.
            | IrExprKind::Range { .. }
            | IrExprKind::RuntimeCall { .. }
            | IrExprKind::If { .. }
            | IrExprKind::Match { .. } => {
                self.lower_call_arg_fresh_operator_value(a).map(Some)
            }
            // A field/element/tuple EXTRACTION argument. A SCALAR result is an
            // unambiguous COPY → `Const`. A HEAP result is an ALIAS/share of
            // the container → `Op::Dup` of the container value (the container-
            // grain field access), borrowed into the call and dropped at scope
            // end. (A nested-container extraction stays walled inside
            // `lower_heap_extraction`.)
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
                self.lower_call_arg_extraction(a, out).map(Some)
            }
            // A custom-variant CONSTRUCTOR argument (`val(Num(7))`, `f(Eof)`) — NOT a
            // function call: materialize the tagged value-model block (tag@slot0 + scalar
            // fields@slot1..) via `try_lower_variant_ctor`, borrowed into the call and
            // dropped at scope end (cert `i` + `d`, like the record-literal arg above).
            // Must PRECEDE the generic Named-call arm, which would emit a dangling
            // `CallFn "Num"` (an unlinked call = invalid wasm). Outside the subset (a
            // heap/recursive ctor field — ADT brick 5) it WALLs, never a wrong-bytes block.
            _ => Ok(None),
        }
    }

    /// A `??` ARGUMENT: the self-host-materialized unwrap when
    /// `try_lower_option_unwrap_or` executes it (a heap result is BORROWED, a scalar
    /// passed by value), otherwise the speculative ops + handles are rolled back to
    /// their marks and [`Self::deferred_unwrap_or_call_arg`] decides the fallback.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the `UnwrapOr` arm verbatim, rollback marks and all.
    fn lower_call_arg_unwrap_or(
        &mut self,
        a: &IrExpr,
        expr: &IrExpr,
        fallback: &IrExpr,
    ) -> Result<ArgOutcome, LowerError> {
        let mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        Ok(ArgOutcome::Value(
            match self.try_lower_option_unwrap_or(expr, fallback, true) {
                Some(v) if is_heap_ty(&a.ty) => CallArg::Handle(v),
                Some(v) => CallArg::Scalar(v),
                None => {
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    self.lifted.truncate(lifted_mark);
                    // A RESULT-polarity HEAP `??` argument (`list.len(out ?? empty)`
                    // — the branch_lift cond, #1134): ANF it through the bind
                    // machinery, which executes this class via the tail-proven
                    // `match ok/err` rewrite; the synthetic temp is scope-tracked
                    // like a user-written `let t = out ?? empty` and BORROWED here.
                    // A decline rolls back to the honest wall below.
                    if is_heap_ty(&a.ty) && expr.ty.is_result() {
                        let tmp = self.fresh_synth_var();
                        if self.lower_bind(tmp, &a.ty, a).is_ok() {
                            if let Ok(v) = self.value_for(tmp) {
                                return Ok(ArgOutcome::Value(CallArg::Handle(v)));
                            }
                        }
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        self.lifted.truncate(lifted_mark);
                    }
                    self.deferred_unwrap_or_call_arg(a, expr)?
                }
            },
        ))
    }

    /// What a NON-lowerable `??` argument becomes once the speculative lowering has
    /// been rolled back: a WALL for the two shapes where the deferred value would be
    /// a silent miscompile (a heap result, an Option operand), the deferred `Const`
    /// otherwise.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the `UnwrapOr` arm's `None` branch verbatim, every guard in source order.
    fn deferred_unwrap_or_call_arg(
        &mut self,
        a: &IrExpr,
        expr: &IrExpr,
    ) -> Result<CallArg, LowerError> {
        if is_heap_ty(&a.ty) {
            // A non-lowerable `??` with a HEAP result as a call ARGUMENT
            // would borrow an empty deferred heap value = a SILENT
            // MISCOMPILE. Reject. (A SCALAR `??` falls to the deferred
            // `Const` 0 below — the separate silent-zero class, left as-is.)
            return Err(LowerError::Unsupported(
                "non-lowerable `??` with a heap result in a call-argument \
                 position (would borrow an empty deferred heap value)"
                    .into(),
            ));
        }
        // A `??` over an OPTION operand whose Some-payload could NOT be read
        // (`r.opt ?? -1.0` over an `Option[scalar]` FIELD access — no tracked
        // handle) must NOT silently take the fallback: a `Const 0` reads the
        // WRONG arm when the Option is `Some` (a silent miscompile, exposed once
        // derived-Codec `Option` decode links — codec_float_int). WALL it. A
        // Result `??` (`int.parse(s) ?? -1`) keeps the Const-0 class below.
        if matches!(&expr.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _))
        {
            return Err(LowerError::Unsupported(
                "non-lowerable `??` over an Option operand in a call-argument \
                 position cannot be faithfully computed (a Const-0 would take \
                 the fallback when the Option is Some) not in this brick"
                    .into(),
            ));
        }
        let dst = self.fresh_value();
        self.record_elided_calls(a);
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall(&format!(
                "call argument ({})",
                kind_name(&a.kind)
            )));
        }
        self.ops.push(Op::Const { dst });
        Ok(CallArg::Scalar(dst))
    }

    /// A scalar-result `match` over a HEAP subject: the executing forms yield a real
    /// `CallArg::Scalar`, anything outside the executable subset rolls the ops back
    /// and WALLs (a Const-0 would silently pick a wrong arm).
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the guarded `Match` arm verbatim.
    fn lower_call_arg_scalar_match_over_heap(
        &mut self,
        a: &IrExpr,
    ) -> Result<ArgOutcome, LowerError> {
        let mark = self.ops.len();
        Ok(ArgOutcome::Value(match self.lower_scalar_value(a) {
            Some(v) => CallArg::Scalar(v),
            None => {
                self.ops.truncate(mark);
                return Err(LowerError::shaped(
                    a.span,
                    WallShape::VariantValueMatch,
                    "scalar-result match over a heap subject in a call-argument \
                     position outside the executable subset cannot be faithfully \
                     computed (a Const-0 would silently pick a wrong arm) not in \
                     this brick",
                ));
            }
        }))
    }

    /// A heap-result RANGE argument with scalar bounds: the REAL `list.range` list,
    /// pushed to `out` itself (a multi-push shape) and dropped at scope end; a
    /// non-scalar bound rolls the ops back and WALLs.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the guarded `Range` arm verbatim, including its own `out.push` + `Pushed`.
    fn lower_call_arg_range_list(
        &mut self,
        a: &IrExpr,
        start: &IrExpr,
        end: &IrExpr,
        inclusive: bool,
        out: &mut Vec<CallArg>,
    ) -> Result<ArgOutcome, LowerError> {
        let range_mark = self.ops.len();
        let (s_v, e_v0) = match (
            self.lower_scalar_value(start),
            self.lower_scalar_value(end),
        ) {
            (Some(sv), Some(ev)) => (sv, ev),
            _ => {
                self.ops.truncate(range_mark);
                return Err(LowerError::Unsupported(
                    "heap-result Range in a call-argument position cannot be                                  faithfully computed in this brick (non-scalar bound)"
                        .into(),
                ));
            }
        };
        let mut e_v = e_v0;
        if inclusive {
            let one = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: one, value: 1 });
            let e2 = self.fresh_value();
            self.ops.push(Op::IntBinOp {
                dst: e2,
                op: crate::IntOp::Add,
                a: e_v,
                b: one,
            });
            e_v = e2;
        }
        let repr = repr_of(&a.ty)?;
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "list.range".to_string(),
            args: vec![CallArg::Scalar(s_v), CallArg::Scalar(e_v)],
            result: Some(repr),
        });
        out.push(self.materialized_call_arg(dst, repr, &a.ty));
        Ok(ArgOutcome::Pushed)
    }

    /// The grouped operator / branch argument (BinOp, UnOp, Try, ToOption,
    /// OptionalChain, Range, RuntimeCall, `if`, `match`): a heap result WALLs, a
    /// scalar computes its REAL value or rolls back to the deferred `Const`.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the grouped arm's body verbatim, its `is_heap_ty` half turned into a guard
    /// clause over the SAME condition.
    fn lower_call_arg_fresh_operator_value(
        &mut self,
        a: &IrExpr,
    ) -> Result<ArgOutcome, LowerError> {
        if is_heap_ty(&a.ty) {
            // A heap-result operator / branch as a call ARGUMENT (`f(a ++ b)`
            // unlowered, `f(if c then "a" else "b")`, `f(0..n)`) would borrow an
            // empty deferred heap value into the callee = a SILENT MISCOMPILE.
            return Err(LowerError::Unsupported(format!(
                "heap-result {} in a call-argument position cannot be faithfully \
                 computed in this brick (would borrow an empty deferred heap value)",
                kind_name(&a.kind)
            )));
        }
        // A scalar Int arithmetic / comparison / prim arg computes its
        // REAL value (`f(n / 10)` → IntBinOp); outside that subset it
        // rolls back to the deferred Const + elided caps marker.
        let mark = self.ops.len();
        Ok(ArgOutcome::Value(match self.lower_scalar_value(a) {
            Some(v) => CallArg::Scalar(v),
            None => {
                self.ops.truncate(mark);
                let dst = self.fresh_value();
                self.record_elided_calls(a);
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall(&format!(
                        "call argument ({})",
                        kind_name(&a.kind)
                    )));
                }
                self.ops.push(Op::Const { dst });
                CallArg::Scalar(dst)
            }
        }))
    }

    /// A field/element/tuple EXTRACTION argument: the heap half aliases the
    /// container, the scalar half loads the real field/element value.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the grouped extraction arm's `is_heap_ty` split, each half verbatim in the
    /// helper below it.
    fn lower_call_arg_extraction(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<ArgOutcome, LowerError> {
        if is_heap_ty(&a.ty) {
            return self.alias_heap_extraction_call_arg(a);
        }
        self.load_scalar_extraction_call_arg(a, out)
    }

    /// A HEAP extraction argument: the container-grain `Op::Dup` alias, or — when
    /// the extraction resolved to a precise heap-field BORROW the container already
    /// owns — a bare `Handle` that stays out of the scope-end drop set.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the extraction arm's `is_heap_ty` branch verbatim.
    fn alias_heap_extraction_call_arg(
        &mut self,
        a: &IrExpr,
    ) -> Result<ArgOutcome, LowerError> {
        let repr = repr_of(&a.ty)?;
        // A non-var container (`f().x`) cannot be aliased (no single `src` to
        // `Dup`); the deferred Opaque empty value borrowed into the callee is a
        // SILENT MISCOMPILE, so a failed extraction rejects here.
        let dst = self.lower_heap_extraction(a)?;
        // A precise heap-field BORROW (`b.label`) is in `param_values` — the
        // container owns it, so it is passed by Handle WITHOUT joining the
        // scope-end drop set (no second owner, no double-free). A container-
        // grain Dup / deferred Opaque is a fresh owned temp → tracked normally.
        Ok(ArgOutcome::Value(if self.param_values.contains(&dst) {
            CallArg::Handle(dst)
        } else {
            self.materialized_call_arg(dst, repr, &a.ty)
        }))
    }

    /// A SCALAR extraction argument: the real field/element load, else the ANF-lift
    /// of a call-result field, else the computed-call-result WALL, else the deferred
    /// `Const` copy with the container's calls elided.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the extraction arm's scalar branch verbatim, guards in source order.
    fn load_scalar_extraction_call_arg(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<ArgOutcome, LowerError> {
        // A SCALAR extraction (`r.x`, `t.0`, `xs[i]`) — load the REAL field /
        // element value from the block's layout slot when the container is a
        // materialized scalar aggregate / a tracked list (the VALUE MODEL).
        // `lower_scalar_value` dispatches Member/TupleIndex to the field load and
        // IndexAccess to the bounds-checked `$elem_addr` load. Outside that subset
        // (a non-var / heap-field-aggregate container, or a computed container
        // `g().field`) it rolls back to a deferred `Const` copy with the
        // container's calls elided (the caps fold then sees them), as before.
        let mark = self.ops.len();
        Ok(ArgOutcome::Value(match self.lower_scalar_value(a) {
            Some(v) => CallArg::Scalar(v),
            None => {
                self.ops.truncate(mark);
                let lifted = self.lift_call_result_field_load(a);
                if let Some(v) = lifted {
                    out.push(CallArg::Scalar(v));
                    return Ok(ArgOutcome::Pushed);
                }
                self.ops.truncate(mark);
                // A scalar field access on a COMPUTED CALL result (`mk(5).x`)
                // — the call result is not a tracked aggregate, so a Const-0
                // reads a WRONG value (and the record-returning callee now
                // renders, making it observable). WALL it. A tracked-Var
                // container (`r.x`) lowered above and never reaches here; other
                // computed containers keep the deferred Const (unchanged).
                if let IrExprKind::Member { object, .. } = &a.kind {
                    if matches!(object.kind, IrExprKind::Call { .. }) {
                        return Err(LowerError::Unsupported(
                            "scalar field access on a computed call result \
                             cannot be faithfully computed in this brick (a \
                             Const-0 would read a wrong value) not in this brick"
                                .into(),
                        ));
                    }
                }
                let dst = self.fresh_value();
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall(&format!(
                        "call argument ({})",
                        kind_name(&a.kind)
                    )));
                }
                self.ops.push(Op::Const { dst });
                self.record_elided_calls(a);
                CallArg::Scalar(dst)
            }
        }))
    }

    /// ANF-LIFT `f().x` (a scalar field on a call result — the
    /// paren-defaults `mk_defaults().port` shape): bind the call
    /// to a SYNTHETIC temp exactly like `let tmp = f(); tmp.x`
    /// (the tail.rs heap-extraction discipline — the bind tracks
    /// + seeds the record's read shape), then the field-slot load
    /// resolves over the tracked temp.
    ///
    /// Extracted from `lower_call_arg_operator` (codopsy round-2 complexity sweep):
    /// the scalar-extraction arm's `lifted` binding verbatim. A `None` (a non-Member
    /// argument, a non-heap-call container, a failed bind or load) leaves the caller
    /// to roll `self.ops` back to its mark, exactly as before.
    fn lift_call_result_field_load(&mut self, a: &IrExpr) -> Option<ValueId> {
        let IrExprKind::Member { object, field } = &a.kind else {
            return None;
        };
        if matches!(object.kind, IrExprKind::Call { .. }) && is_heap_ty(&object.ty) {
            let tmp = self.fresh_synth_var();
            let field = *field;
            let obj_ty = object.ty.clone();
            self.lower_bind(tmp, &obj_ty, object).ok().and_then(|_| {
                let synth = IrExpr {
                    kind: IrExprKind::Member {
                        object: Box::new(IrExpr {
                            kind: IrExprKind::Var { id: tmp },
                            ty: obj_ty,
                            span: object.span.clone(),
                            def_id: None,
                        }),
                        field,
                    },
                    ty: a.ty.clone(),
                    span: a.span.clone(),
                    def_id: None,
                };
                self.lower_scalar_value(&synth)
            })
        } else {
            None
        }
    }

    /// Calls — named, module-qualified and computed — plus member access.
    ///
    /// One group of `lower_call_arg_into`'s dispatch table, arms verbatim and in
    /// source order. `Ok(None)` means "not my group" — the router tries the groups
    /// in that order, so which rule an argument takes is unchanged, and an `Err`
    /// still aborts the whole call as before.
    fn lower_call_arg_call(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<Option<ArgOutcome>, LowerError> {
        Ok(Some(ArgOutcome::Value(match &a.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_variant_ctor(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(format!(
                            "variant constructor `{}` argument cannot be faithfully \
                             materialized in this brick (a heap/recursive field — ADT brick 5)",
                            name.as_str()
                        )))
                    }
                }
            }
            // A Named user-call result, materialized into an owned temp.
            IrExprKind::Call { target: CallTarget::Named { name }, args: inner, .. } => {
                let inner_args = self.lower_call_args(inner)?;
                let dst = self.fresh_value();
                let repr = repr_of(&a.ty)?;
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: inner_args,
                    result: Some(repr),
                });
                let arg = self.materialized_call_arg(dst, repr, &a.ty);
                // A user function returning Option/Result yields a REAL same-layout variant
                // block (an in-profile `-> Option[T]` callee returns `OptSome`/`OptNone`,
                // a `-> Result[..]` the DynListStr — the v1 calling convention, the SAME
                // evidence as a variant PARAM). Seed the READ-shape so a `match`/`??` over
                // this owned call result EXECUTES (reads the tag) instead of WALLing/deferring.
                // Ownership is unchanged — `materialized_call_arg` already registered the
                // scope-end drop; `seed_variant_param` adds only layout knowledge.
                if is_variant_ty(&a.ty) {
                    self.seed_variant_param(dst, &a.ty);
                }
                arg
            }
            // A first-order pure stdlib Module-call result, materialized (the
            // purity + higher-order gates live in `lower_pure_module_value_call`).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args: inner, .. } => {
                let dst = self.lower_pure_module_value_call(
                    module.as_str(),
                    func.as_str(),
                    inner,
                    &a.ty,
                )?;
                let repr = repr_of(&a.ty)?;
                // A prim BORROW result (`prim.load_str` = LoadHandle, marked in `param_values`)
                // is owned by its source block — pass it by Handle WITHOUT joining the scope-end
                // drop set, exactly like a precise heap-field borrow (`b.label`). Dropping it
                // would double-free with the owner's drop (a Str Value's tag-4 payload free).
                if self.param_values.contains(&dst) {
                    CallArg::Handle(dst)
                } else {
                    self.materialized_call_arg(dst, repr, &a.ty)
                }
            }
            // A `Method`/`Computed`-target call argument (`f(obj.m())`,
            // `f((g)())`): an UNRESOLVABLE callee (dispatch / closure value not
            // known here) — model it as a DEFERRED fresh value (a heap `Alloc`
            // borrowed+dropped, a scalar `Const`). Its receiver's/args' calls are
            // captured by `record_elided_calls`, but the method/computed call
            // itself is NOT (skipped), so the source has MORE call nodes than the
            // MIR ⇒ the `ir_calls > mir_calls` gate TAINTS the function caps-
            // unverified (honest — the callee's capabilities are unknown). The
            // result value is deferred, like every Opaque.
            IrExprKind::Call { target, args: inner, .. } => {
                return self.lower_call_arg_opaque_call(a, target, inner, out)
            }
            _ => return Ok(None),
        })))
    }

    /// A `Method`/`Computed`-target call argument (`f(obj.m())`, `f((g)())`): an
    /// UNRESOLVABLE callee (dispatch / closure value not known here).
    ///
    /// Its receiver's/args' calls are captured by `record_elided_calls`, but the
    /// method/computed call itself is NOT (skipped), so the source has MORE call
    /// nodes than the MIR ⇒ the `ir_calls > mir_calls` gate TAINTS the function
    /// caps-unverified (honest — the callee's capabilities are unknown).
    ///
    /// A HEAP result must either EXECUTE (the two [`Self::try_heap_opaque_computed_arg`]
    /// routes) or be REJECTED: borrowing an empty deferred heap value into the callee
    /// is a SILENT MISCOMPILE. A SCALAR result may still fall to the deferred
    /// `Const` 0 (the silent-zero class, gated by `strict_values`).
    fn lower_call_arg_opaque_call(
        &mut self,
        a: &IrExpr,
        target: &CallTarget,
        inner: &[IrExpr],
        out: &mut Vec<CallArg>,
    ) -> Result<Option<ArgOutcome>, LowerError> {
        if is_heap_ty(&a.ty) {
            if let CallTarget::Computed { callee } = target {
                if let Some(v) = self.try_heap_opaque_computed_arg(callee, inner, &a.ty) {
                    out.push(CallArg::Handle(v));
                    return Ok(Some(ArgOutcome::Pushed));
                }
            }
            return Err(LowerError::Unsupported(
                "unresolvable method/computed call with a heap result in a \
                 call-argument position (would borrow an empty deferred heap value)"
                    .into(),
            ));
        }
        // C1 DIRECT-CALL INLINE: a SCALAR-result `Computed` call `f(x)` whose callee
        // is a statically-known let-bound INLINE lambda is DEFUNCTIONALIZED to its
        // inlined body (`try_lower_scalar_call`'s Computed arm). This EXECUTES
        // `int.to_string(f(1))` (= 3 for `let f = (x) => string.len(s) + x`) instead
        // of the deferred `Const 0` below. `try_lower_scalar_call` is rollback-safe
        // (restores ops + handles on a miss), so a non-inlinable Method/Computed
        // callee falls through to the deferred `Const` exactly as before — the caps
        // fold still tags it via `record_elided_calls`.
        let mark = self.ops.len();
        if let Some(v) = self.try_lower_scalar_call(a, &a.ty) {
            return Ok(Some(ArgOutcome::Value(CallArg::Scalar(v))));
        }
        self.ops.truncate(mark);
        let dst = self.fresh_value();
        self.record_elided_calls(a);
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall(&format!(
                "call argument ({})",
                kind_name(&a.kind)
            )));
        }
        self.ops.push(Op::Const { dst });
        Ok(Some(ArgOutcome::Value(CallArg::Scalar(dst))))
    }

    /// The two routes a HEAP-result `Computed` call argument can still EXECUTE
    /// through, tried in order; `None` leaves the caller to reject. Both leave the
    /// op stream and live-handle set untouched on a miss.
    ///
    /// 1. C1 HEAP DIRECT-CALL INLINE — a callee that is a statically-known let-bound
    ///    INLINE lambda is DEFUNCTIONALIZED to its inlined body, a FRESH OWNED heap
    ///    value (tracked for scope-end drop) BORROWED into this outer call. This
    ///    executes `"${param_ty(p)}"` (the bindgen `generate_dts` inner-map cell)
    ///    instead of walling.
    /// 2. A call THROUGH a KNOWN CLOSURE value (`println(hi("world"))` where `hi`
    ///    bound a closure block) — the closure dispatch, likewise a fresh OWNED
    ///    value (cert `i`, scope-end drop). Mirrors the bind-position closure call
    ///    (binds_p2).
    ///
    /// Either way the result is ALREADY in `live_heap_handles`, so the caller passes
    /// it by Handle WITHOUT `materialized_call_arg` (which would double-track → a
    /// double-free). A String result drops via the flat `Op::Drop` (rc_dec), already
    /// correct for the default scope-end drop.
    fn try_heap_opaque_computed_arg(
        &mut self,
        callee: &IrExpr,
        inner: &[IrExpr],
        ty: &Ty,
    ) -> Option<ValueId> {
        let mark = self.ops.len();
        let lhh = self.live_heap_handles.len();
        if let Some(v) = self.try_inline_direct_lambda_call_heap(callee, inner, ty) {
            return Some(v);
        }
        self.ops.truncate(mark);
        self.live_heap_handles.truncate(lhh);
        let blk = self.closure_block_of_mut(callee)?;
        if let (Ok(crepr), Ok(lowered)) = (repr_of(ty), self.lower_call_args(inner)) {
            let dst = self.fresh_value();
            self.emit_closure_call(blk, Some(dst), lowered, Some(crepr));
            self.live_heap_handles.push(dst);
            return Some(dst);
        }
        self.ops.truncate(mark);
        self.live_heap_handles.truncate(lhh);
        None
    }
}

impl LowerCtx {
    /// Materialize a list LITERAL argument (`list.flatten([first, second])`,
    /// `[c_add(e, t)]`) as a BORROW-VIEW: a fresh 2-slot block whose slots hold
    /// BORROWED handles (a tracked Var stays owned by its binding; a fresh call
    /// result is pushed to `live_heap_handles` and freed at scope end). The view
    /// block itself is tracked with NO element set, so its scope-end drop is the
    /// plain block-only `Op::Drop` — the slots' owners are untouched (no double
    /// free, no leak; the callee rc_incs whatever it keeps). Flat-content element
    /// types only (deeper nesting keeps walling).
    /// One slot of the [`Self::try_lower_heap_var_list_literal`] borrow view: a
    /// tracked heap Var (borrowed — Dup'd into the slot by the caller) or a
    /// heap-returning NAMED call (`[c_add(e, t)]` — fft's concat element), whose
    /// fresh OWNED result is moved into the slot directly (no Dup) and freed at
    /// scope end, AFTER the callee borrowed it through the view. Any other element
    /// shape declines; the caller owns the rollback.
    fn heap_view_slot_handle(&mut self, e: &IrExpr) -> Option<ValueId> {
        match &e.kind {
            IrExprKind::Var { id } => self.value_for(*id).ok(),
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                if !self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                let lowered = self.lower_call_args(args).ok()?;
                let erepr = repr_of(&e.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(erepr),
                });
                self.live_heap_handles.push(dst);
                Some(dst)
            }
            // Any OTHER element shape (a desugared map literal — a Module
            // from_list call — a heap `if`, an Option/Result ctor) delegates to
            // the SINGLE-ARG lowering: the full call-argument machinery already
            // materializes each of these as a fresh OWNED temp with its exact
            // type-routed scope-end drop (`materialized_call_arg`'s seeding),
            // which is precisely the ownership story a view slot borrows
            // against. A shape that machinery declines errors out and the view
            // rolls back — the same honest wall as before.
            _ => {
                let mut out: Vec<CallArg> = Vec::new();
                match self.lower_call_arg_into(e, &mut out) {
                    Ok(()) => match out.pop() {
                        Some(CallArg::Handle(v)) => Some(v),
                        _ => None,
                    },
                    Err(_) => None,
                }
            }
        }
    }

    pub(crate) fn try_lower_heap_var_list_literal(&mut self, a: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &a.kind else { return None };
        if elements.is_empty() {
            return None;
        }
        let elem_ty = match &a.ty {
            Ty::Applied(TypeConstructorId::List, ts) if ts.len() == 1 => &ts[0],
            _ => return None,
        };
        let flat_content = matches!(elem_ty, Ty::String | Ty::Bytes)
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Bytes, _))
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::List, inner)
                    if inner.len() == 1
                        && (!is_heap_ty(&inner[0])
                            || self.aggregate_field_tys(&inner[0])
                                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                                .is_some()))
            // An all-scalar aggregate element itself (`[c_add(e, t)]` — a (Float, Float)
            // Complex): its block holds only inline scalars, rc_dec IS its full free.
            || self
                .aggregate_field_tys(elem_ty)
                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                .is_some();
        // Beyond the flat classes: a ROUTABLE-MAP element (#1527's List-argument
        // top family — `list.get_or([m0, m1], i, d)` over Map values). The view
        // itself never reads element bytes (slots are borrowed handles) and the
        // hshare accessor family the router sends these callees to is likewise
        // handle-level; what must be EXACT is the scope-end drop story of any
        // element temp and of the shared-out result — and for every map class
        // below that is precisely the type-keyed route `materialized_call_arg`'s
        // seeding ladder already installs (named-value sweep / hval / ivh / msv /
        // mlo / the skv key sweep / the interleaved Map[heap,heap] DropListStr).
        // The FAMILY is the parameter (the same router classes the map ops use),
        // never a hardcoded key type.
        // ONE-LEVEL-EXACT under a per-slot dec: a scalar side is untouched, a
        // String/Bytes/flat-block/Flat-ctor side decs clean. Shared by the
        // map-layout arm below and the Result-element arm.
        let one_exact = |t: &Ty| !is_heap_ty(t)
            || matches!(t, Ty::String | Ty::Bytes)
            || crate::lower::is_flat_scalar_block_ty(t)
            || crate::lower::lenlist_elem_class(t)
                == Some(crate::lower::CtorElemClass::Flat);
        let routable_map = self.map_named_value_drop(elem_ty).is_some()
            || crate::lower::is_map_hval_ty(elem_ty)
            || crate::lower::is_map_ivh_ty(elem_ty)
            || crate::lower::is_map_msv_ty(elem_ty)
            || crate::lower::is_map_mlo_ty(elem_ty)
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Map, a)
                    if a.len() == 2 && one_exact(&a[0]) && one_exact(&a[1]))
            // A `Result[<one-level-exact payload>, String]` element
            // (`list.get_or([acc0, n1], 5, x2)` over Result[Option[Float],
            // String] — the #1527 List-argument Result bucket's flat half):
            // the element and the shared-out result both ride the DynListStr
            // family's per-slot sweep (`is_heap_elem_list_ty` -> flat_elems),
            // which is exact precisely when the Ok payload is one-level-exact
            // (the Err String always is). A NESTED payload (Result[Result[..]],
            // Result[Map[..]], Option[String]) stays declined — its slot-0 dec
            // would leak the payload's interior.
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Result, a)
                    if a.len() == 2 && matches!(a[1], Ty::String) && one_exact(&a[0]))
            ;
        if !flat_content && !routable_map {
            return None;
        }
        let ops_mark = self.ops.len();
        // Each element is a tracked heap Var (borrowed — Dup'd into the slot below) or a
        // heap-returning NAMED call (`[c_add(e, t)]` — fft's concat element): the call's
        // fresh OWNED result is moved into the slot directly (no Dup).
        let lhh_mark = self.live_heap_handles.len();
        let mut handles: Vec<ValueId> = Vec::with_capacity(elements.len());
        for e in elements {
            let Some(src) = self.heap_view_slot_handle(e) else {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            };
            handles.push(src);
        }
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: list,
            repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynListStr { len: n },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, src) in handles.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
            let oh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(oh), args: vec![src] });
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, oh],
            });
        }
        // The view block itself: tracked with NO element set → plain block-only Drop.
        self.live_heap_handles.push(list);
        Some(list)
    }
}
