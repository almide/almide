// The HEAP half of the bind lowering, split out of `binds_p2.rs` to keep each
// file under the line ceiling. `include!`d, so it shares that module's imports.

impl LowerCtx {
    /// The HEAP half of [`Self::lower_bind`]: the heap-`??` executable subset,
    /// then the fresh-vs-alias match over every heap producer. Verbatim text move.
    fn lower_bind_heap(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if self.try_lower_bind_heap_unwrap_or_precheck(var, ty, value)? {
            return Ok(());
        }
        self.lower_bind_heap_kind(var, ty, value)
    }

    /// Extracted from `Self::lower_bind_heap` (third-round split, cog reduction): the
    /// leading heap `??` executable-subset precheck, verbatim. `Ok(true)` means the
    /// caller already bound `var` and should return immediately.
    fn try_lower_bind_heap_unwrap_or_precheck(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // `let s = opt ?? "default"` — a HEAP-String `??` over a materialized Option[String]
        // EXECUTES via the self-host `option.unwrap_or_str` CALL (try_lower_option_unwrap_or's heap
        // branch): a fresh owned String, bound + dropped like any heap value. This CLOSES the
        // silent-empty `Alloc{Opaque}` hole the deferred arm below leaves for heap `??` (the
        // `list.get(xs,i) ?? "d"` / `json.as_string(v) ?? "d"` miscompile). Outside the subset
        // (a non-String heap payload, a non-materialized operand) it falls through to the deferred
        // `Alloc{Opaque}` arm below — unchanged, the existing memory-safe incompleteness.
        if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
            let lifted_mark = self.lifted.len();
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(true);
            }
            // The declined attempt above rolled its OPS back but a lambda it lifted
            // while resolving the operand (a fallible-HOF closure arg) SURVIVES in
            // `lifted` — the match rewrite below would lift it AGAIN, and the twin
            // lambda bodies double-count the callback's calls (a mir>ir caps breach,
            // not just waste). Roll the lift back before re-lowering.
            self.lifted.truncate(lifted_mark);
            // A RESULT-polarity heap `??` the direct route declined (`let a = list.map(xs,
            // (s) => f(s)!) ?? [0]` — the fallible-HOF bind, #1134 Shape 2): rewrite to the
            // SAME `match expr { ok(p) => p, err(_) => fallback }` the TAIL position proved
            // (lower_tail_heap_unwrap_or) and route it through the bind-position heap-match
            // machinery. Result polarity ONLY, exactly as the tail rewrite gates it — an
            // Option operand keeps its proven `option.unwrap_or_str` route above, and a
            // decline inside the match ROLLS BACK to the honest wall below (never wrong
            // bytes; the rewrite is speculative, ops truncated on failure).
            if expr.ty.is_result() {
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                let rewritten = Self::unwrap_or_as_result_match(value, expr, fallback);
                if self.lower_bind(var, ty, &rewritten).is_ok() {
                    return Ok(true);
                }
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.lifted.truncate(lifted_mark);
            }
            // The OPTION-polarity mirror (#1270): a heap `??` whose operand is
            // Option-typed and whose PAYLOAD is itself Option (`sn ?? none`
            // over Option[Option[Int]] — the nested-Option elimination)
            // declined the direct route above. Rewrite to `match expr {
            // some(p) => p, none => fallback }` through the same speculative
            // bind-position heap-match machinery; a decline ROLLS BACK to the
            // honest wall below. GATED to Option payloads: an Option[record]
            // subject through THIS synthesized route mis-reads the record's
            // String fields on wasm (measured: "x" printed as blanks) even
            // though a user-written match over the same source is correct —
            // the record case stays on the wall until the bind-position
            // synthesized-match route is fixed (#1270 follow-up).
            if expr.ty.is_option() && ty.is_option() {
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                let rewritten = Self::unwrap_or_as_option_match(value, expr, fallback);
                if self.lower_bind(var, ty, &rewritten).is_ok() {
                    return Ok(true);
                }
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.lifted.truncate(lifted_mark);
            }
            // A HEAP-result `??` over an Option/Result operand that `try_lower_option_unwrap_or`
            // declined (e.g. `Option[record]` — no faithful record-payload unwrap-or yet) must
            // NOT fall to the `Alloc{Opaque}` below: that binds an EMPTY heap value the caller
            // OBSERVES as a wrong record (both arms of `list.get(tools,i) ?? {…}` printed empty /
            // garbage vs v0). WALL it — an honest refusal, never a silently-wrong value.
            if is_variant_ty(&expr.ty) {
                return Err(LowerError::Unsupported(
                    "heap-result ?? over an Option/Result operand outside the executable subset \
                     (e.g. an Option[record] default) cannot be faithfully computed in this brick"
                        .into(),
                ));
            }
        }
        Ok(false)
    }

    /// The `e ?? d` → `match e { ok(p) => p, err(_) => d }` rewrite, shared shape with
    /// [`Self::lower_tail_heap_unwrap_or`]'s inline version (Result polarity only —
    /// the caller gates).
    fn unwrap_or_as_result_match(value: &IrExpr, expr: &IrExpr, fallback: &IrExpr) -> IrExpr {
        use almide_ir::{IrMatchArm, IrPattern};
        let payload_ty = value.ty.clone();
        let p = VarId(crate::lower::max_var_id(value) + 1);
        let bind = IrPattern::Bind { var: p, ty: payload_ty.clone() };
        let payload = IrExpr {
            kind: IrExprKind::Var { id: p },
            ty: payload_ty,
            span: value.span.clone(),
            def_id: None,
        };
        IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(expr.clone()),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Ok { inner: Box::new(bind) },
                        guard: None,
                        body: payload,
                    },
                    IrMatchArm {
                        pattern: IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
                        guard: None,
                        body: fallback.clone(),
                    },
                ],
            },
            ty: value.ty.clone(),
            span: value.span.clone(),
            def_id: value.def_id,
        }
    }

    /// The Option-polarity mirror of [`Self::unwrap_or_as_result_match`] (#1270):
    /// `e ?? d` → `match e { some(p) => p, none => d }`, same speculative
    /// discipline (the caller gates on `is_option` and rolls back on decline).
    fn unwrap_or_as_option_match(value: &IrExpr, expr: &IrExpr, fallback: &IrExpr) -> IrExpr {
        use almide_ir::{IrMatchArm, IrPattern};
        let payload_ty = value.ty.clone();
        let p = VarId(crate::lower::max_var_id(value) + 1);
        let bind = IrPattern::Bind { var: p, ty: payload_ty.clone() };
        let payload = IrExpr {
            kind: IrExprKind::Var { id: p },
            ty: payload_ty,
            span: value.span.clone(),
            def_id: None,
        };
        IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(expr.clone()),
                arms: vec![
                    IrMatchArm {
                        pattern: IrPattern::Some { inner: Box::new(bind) },
                        guard: None,
                        body: payload,
                    },
                    IrMatchArm {
                        pattern: IrPattern::None,
                        guard: None,
                        body: fallback.clone(),
                    },
                ],
            },
            ty: value.ty.clone(),
            span: value.span.clone(),
            def_id: value.def_id,
        }
    }

    /// Extracted from `Self::lower_bind_heap` (third-round split, cog reduction): the
    /// `value.kind` dispatch match, verbatim (the router now only handles the `??`
    /// precheck above it).
    fn lower_bind_heap_kind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        match &value.kind {
            // Alias: `var b = a` — b is a NEW handle denoting the SAME heap
            // object as a, acquiring its own owned reference (the single
            // fresh-vs-alias decision). `value_or_global` (not `value_for`):
            // `let x = toplib.SYSTEM` aliases a MODULE-LEVEL global — the global
            // materializes its cached fresh owned copy (const-init only, zero
            // calls injected), and the Dup below co-owns it (#486 bind shape).
            IrExprKind::Var { .. } => self.lower_bind_heap_var_alias(var, ty, value),
            // A fresh heap value (literal container / string / Option·Result
            // variant). Constructors lower like a container literal: a fresh
            // `Alloc` (value-semantics — the payload is copied, not consumed), the
            // proven-sound convention the corpus already verifies for List/Record.
            // An ERROR OPERATOR (`e!`/`e?`/`e ?? d`/`e?.f`) likewise yields a FRESH
            // value. The reachable positions are TRANSFORMED before this bind ever
            // sees them (`desugar_effect_unwrap` pushes the continuation into the
            // Ok arm); one that SURVIVES here is outside that transform's reach —
            // a bind nested in a loop/if arm, whose Err propagation would need a
            // mid-loop early return the MIR has no Op for — and under STRICT value
            // mode the fresh-path terminal WALLS it instead of deferring: the old
            // rationale ("total value maps, no control flow") was true of the
            // operators' CONTROL FLOW but not of the deferred VALUE, which the
            // program goes on to read as empty (minesweeper's 81-cell minefield
            // read as `[]` — the #810 census). The permissive caps-counting path
            // still defers, calls captured by `record_elided_calls`.
            // A `let f = (params) => body` lambda. A NON-CAPTURING one LIFTS to a fresh
            // top-level function bound via `Op::FuncRef` (a scalar table slot) — so a later
            // `f(args)` lowers to `Op::CallIndirect` and the closure EXECUTES. A CAPTURING
            // lambda (its body references an enclosing local) needs an environment the
            // proven model has no representation for, so it falls through to the deferred
            // `Alloc{Opaque}` (its calls elided ⇒ honest caps taint), unchanged.
            IrExprKind::Lambda { .. } => self.lower_bind_heap_lambda(var, ty, value),
            IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            | IrExprKind::Record { .. }
            | IrExprKind::SpreadRecord { .. }
            | IrExprKind::Tuple { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::StringInterp { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. }
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A CLOSURE value (`var f = (x) => …`) is a fresh heap env, and a RANGE is
            // a fresh value — both `Alloc{Opaque}`. The closure is NOT invoked here, so
            // its body's calls are elided ⇒ the gate taints the function caps-unverified
            // honestly (the closure's invocation capabilities are unknown).
            // (A NON-CAPTURING `Lambda` is intercepted ABOVE and LIFTED to a FuncRef; only
            // a capturing one — a real environment — reaches this deferred Opaque arm.)
            | IrExprKind::ClosureCreate { .. }
            // A RUNTIME CALL result is a fresh value (its call is elided ⇒ the gate
            // taints the function honestly, like Method/Computed).
            | IrExprKind::RuntimeCall { .. } => {
                self.lower_bind_heap_fresh(var, ty, value)
            }
            // `let r = 0..<n` — a RANGE initializer. This sat in the deferred-Opaque
            // arm above, which is exactly the "deferred EMPTY value observed by a
            // later read" miscompile this file rejects everywhere else: a `for i in r`
            // borrowed the empty Opaque and iterated ZERO times (#1272, wasm printed 0
            // where native printed 3). Materialize the REAL list via the self-hosted
            // `list.range`, mirroring the call-arg path (`lower_call_arg_range_list`).
            IrExprKind::Range { .. } => self.lower_bind_heap_range(var, ty, value),
            // `var v = r.x` / `xs[i]` — a HEAP extraction: alias the container
            // (`Op::Dup`), bound here and dropped at scope end (cert `a` + `d`). When
            // the container is NOT a tracked var (`f().x`, nested `a.b.c`), there is no
            // single `src` to `Dup`; the deferred Opaque EMPTY value the binding would
            // hold is observed by any later read of `v` = a SILENT MISCOMPILE, so a failed
            // extraction rejects here.
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => self.lower_bind_heap_extraction_arm(var, ty, value),
            // `var x = f(...)` — a USER call returning a heap value. The result is
            // a FRESH OWNED heap value (the callee's return-mode signature, read
            // from the bind's heap type — the checker need not open the callee).
            IrExprKind::Call { target: CallTarget::Named { .. }, .. } => {
                self.lower_bind_heap_call_named(var, ty, value)
            }
            // `var x = string.trim(s)` — a stdlib MODULE call returning a heap
            // value. Admitted only when first-order + pure (else walled); the
            // fresh owned result is bound and dropped at scope end, exactly like
            // the `Named` case above.
            IrExprKind::Call { target: CallTarget::Module { .. }, .. } => {
                self.lower_bind_heap_call_module(var, ty, value)
            }
            // `var o = f(x)` where `f` is a lifted lambda / function-typed param returning a
            // HEAP value (`(Int) -> Option[Int]` / `-> List[Int]`): EXECUTE the closure via a
            // heap-result `Op::CallIndirect`. The result is a FRESH OWNED value (the closure
            // moves it out — cert `i`, dropped at scope end — the foundation for filter_map /
            // flat_map). A Computed callee that is NOT a known funcref falls through to the
            // deferred Opaque below.
            IrExprKind::Call { target: CallTarget::Computed { callee }, .. }
                if self.closure_value_of(callee).is_some()
                    || Self::is_fn_member_callee(callee) =>
            {
                self.lower_bind_heap_call_computed(var, ty, value)
            }
            // `var x = obj.method(args)` / `var x = (g)(args)` — an UNRESOLVABLE
            // `Method`/`Computed` callee bound to a heap var. The deferred Opaque EMPTY
            // value the binding would hold is observed by any later read of `x` = a SILENT
            // MISCOMPILE, so reject explicitly.
            IrExprKind::Call { .. } => {
                Err(LowerError::Unsupported(
                    "heap-result method/computed call bound to a var cannot be faithfully \
                     computed in this brick (would bind an empty deferred heap value)"
                        .into(),
                ))
            }
            // `let s = if c then "A" else "B"; …` / `let x = match … { … }` — a heap-result
            // branch in a NON-TAIL, let-bound position. There is NO faithful executable
            // encoding here: a tail heap-result `if` moves each arm's value OUT (the
            // per-arm `"im"` balance), but a LET-BOUND value is held and dropped at scope
            // end — a trailing `Drop` of the merged `IfThen` dst would release a moved-out
            // object (the checker REJECTS the resulting `im·im·d` — accept⟹safe violated),
            // and attributing ONE scope-end drop to exactly-one-of-two arm allocs needs a
            // checker/Coq change (out of scope). The OLD fallback bound `x` to a deferred
            // `Init::Opaque` — an EMPTY heap value — so `println(s)` printed EMPTY instead
            // of "A"/"B": a SILENT MISCOMPILE. Reject explicitly so the function walls
            // cleanly instead of emitting wrong bytes.
            IrExprKind::Match { .. } => {
                self.lower_bind_heap_match(var, ty, value)
            }
            IrExprKind::If { .. } => {
                self.lower_bind_heap_if(var, ty, value)
            }
            // `var x = { stmts; tail }` — a heap BLOCK value. Lower the block's
            // statements (their locals ride to the enclosing scope and are dropped at
            // scope end), then bind `x` to the block's heap TAIL via `lower_bind` (a var
            // alias / fresh literal / call result / nested branch — all proven shapes).
            // A tail-less block is never heap-typed, so it falls through to the wall.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                self.lower_bind(var, ty, tail)
            }
            other => {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!("[heap-bind] declined value (ty {ty:?}): {other:#?}")
                });
                Err(LowerError::Unsupported(format!(
                    "heap bind from {} not in this brick",
                    kind_name(other)
                )))
            }
        }
    }

    /// Extracted from `Self::lower_bind_heap` (third-round split, cog reduction): the
    /// Var-alias arm body, verbatim, re-narrowed via `let-else`.
    fn lower_bind_heap_var_alias(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Var { id } = &value.kind else { unreachable!() };
        let src = self.value_or_global(*id)?;
        let dst = self.fresh_value();
        self.value_of.insert(var, dst);
        self.ops.push(Op::Dup { dst, src });
        self.live_heap_handles.push(dst);
        // The alias denotes the SAME block: a materialized aggregate/option/
        // result source keeps those properties through the Dup (`let x =
        // toplib.CFG; { ...x, name: "y" }` — the #502 rebound spread base).
        // The LIST registrations propagate too: `mains = mains2` then
        // `mains[i]` gated on `materialized_lists` declined on the fresh Dup
        // vid (the whole enclosing loop then rolled back to the strict wall —
        // the ceangal resolve_line_flex class), and the DROP-ROUTE sets must
        // follow the alias so the dup'd reference frees its block by the same
        // recursive route when it happens to be the last one (a flat rc_dec
        // of a heap-element list's final ref leaks the elements).
        if self.materialized_aggregates.contains(&src) {
            self.materialized_aggregates.insert(dst);
        }
        if self.materialized_lists.contains(&src) {
            self.materialized_lists.insert(dst);
        }
        // An alias of a BORROWED param/slot handle (`v = __mp_buf` — the C-132
        // write-back Assign, where `__mp_buf` is a destructured tuple slot in
        // `param_values`) denotes the same GENUINE block the borrow does, so a
        // scalar-element list alias is directly indexable. The Dup above is the
        // new owned reference; only the read-shape knowledge is added here.
        if self.param_values.contains(&src) && is_scalar_elem_list_ty(ty) {
            self.materialized_lists.insert(dst);
        }
        if self.heap_elem_lists.contains(&src) {
            self.heap_elem_lists.insert(dst);
        }
        if self.str_str_elem_lists.contains(&src) {
            self.str_str_elem_lists.insert(dst);
        }
        if self.value_handles.contains(&src) {
            self.value_handles.insert(dst);
        }
        if let Some(mask) = self.record_masks.get(&src).cloned() {
            self.record_masks.insert(dst, mask);
        }
        if let Some(route) = self.variant_drop_handles.get(&src).cloned() {
            self.variant_drop_handles.insert(dst, route);
        }
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap` (third-round split, cog reduction): the
    /// Lambda arm body, verbatim, re-narrowed via `let-else`.
    fn lower_bind_heap_lambda(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Lambda { params, body, .. } = &value.kind else { unreachable!() };
        // C1 DIRECT-CALL INLINE: record the lambda (params + body) so a later DIRECT
        // call `f(args)` to this `var` is DEFUNCTIONALIZED (the body inlined with the
        // params bound to the args, captures resolved through `value_of`). Recorded
        // for BOTH the liftable and the capturing case — the call site prefers inline.
        self.lambda_bindings.insert(var, (params.clone(), (**body).clone()));
        if let Some(dst) = self.lift_lambda(params, body) {
            self.value_of.insert(var, dst);
            return Ok(());
        }
        // A CAPTURING / non-liftable lambda — NO `Op::FuncRef` slot exists, but the
        // direct-call inline above can still EXECUTE a `f(args)`. Bind a placeholder
        // value so `f` is in `value_of` (a lone `f` never invoked carries no
        // observable, and a captured-`f`-passed-to-a-HOF is the C2 first-class case
        // that WALLS at that HOF). The deferred Opaque keeps the value memory-safe.
        let dst = self.fresh_value();
        let repr = repr_of(ty)?;
        let init = alloc_init(value);
        // A DEFERRED Opaque bind is an EMPTY block — record it so a custom-variant
        // `match` over this var WALLS instead of reading a garbage tag (the
        // record-ctor mt2 miscompile class).
        if matches!(init, Init::Opaque) {
            self.deferred_opaque_binds.insert(dst);
        }
        self.value_of.insert(var, dst);
        self.ops.push(Op::Alloc { dst, repr, init });
        self.live_heap_handles.push(dst);
        self.record_elided_calls(value);
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap` (third-round split, cog reduction): the
    /// Member/IndexAccess/MapAccess/TupleIndex heap-extraction arm body, verbatim (the
    /// arm never destructured `value.kind` beyond the top-level match, so this helper
    /// doesn't either).
    fn lower_bind_heap_extraction_arm(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let dst = self.lower_heap_extraction(value)?;
        self.value_of.insert(var, dst);
        // A Fn-typed field extraction (`let f = h.run` — the record_fn_field
        // "field access then call" shape): the borrowed slot handle IS a closure
        // block — track it so a later `f("world")` dispatches via the closure
        // machinery (closure_value_of) instead of walling as unresolvable.
        if matches!(ty, Ty::Fn { .. }) {
            self.closure_values.insert(dst);
        }
        // A precise heap-field BORROW (a `LoadHandle` of a slot in a still-owning
        // container) is in `param_values` — it is NOT a second owner, so it must NOT
        // join the scope-end drop set (the container's masked drop frees the field).
        if !self.param_values.contains(&dst) {
            self.live_heap_handles.push(dst);
        }
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_fresh(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if self.try_lower_bind_heap_fresh_quick(var, ty, value)? {
            return Ok(());
        }
        if self.try_lower_bind_heap_fresh_variant_honest_wall(var, ty, value)? {
            return Ok(());
        }
        if self.try_lower_bind_heap_fresh_tuple(var, value)? {
            return Ok(());
        }
        if self.try_lower_bind_heap_fresh_record(var, value)? {
            return Ok(());
        }
        if self.try_lower_bind_heap_fresh_spread_record(var, value)? {
            return Ok(());
        }
        if self.try_lower_bind_heap_fresh_scalar_list(var, ty, value)? {
            return Ok(());
        }
        self.lower_bind_heap_fresh_opaque(var, ty, value)
    }

    /// `let r = start..<end` / `start...end` — materialize the REAL `list.range`
    /// list (the call-arg path's `lower_call_arg_range_list`, in bind position) and
    /// seed it exactly like a `let r = list.range(s, e)` module-call bind, so a
    /// later `for i in r` / `r[i]` reads a populated block. A non-scalar bound
    /// walls (a deferred Opaque here was the #1272 silent zero-iteration).
    fn lower_bind_heap_range(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Range { start, end, inclusive } = &value.kind else { unreachable!() };
        let range_mark = self.ops.len();
        let (s_v, e_v0) = match (self.lower_scalar_value(start), self.lower_scalar_value(end)) {
            (Some(sv), Some(ev)) => (sv, ev),
            _ => {
                self.ops.truncate(range_mark);
                return Err(LowerError::Unsupported(
                    "a Range initializer with a non-scalar bound cannot be materialized \
                     in this brick (a deferred empty value would iterate zero times)"
                        .into(),
                ));
            }
        };
        let mut e_v = e_v0;
        if *inclusive {
            let one = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: one, value: 1 });
            let e2 = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: e2, op: crate::IntOp::Add, a: e_v, b: one });
            e_v = e2;
        }
        let repr = repr_of(ty)?;
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "list.range".to_string(),
            args: vec![CallArg::Scalar(s_v), CallArg::Scalar(e_v)],
            result: Some(repr),
        });
        self.value_of.insert(var, dst);
        self.seed_call_module_heap_read_shape(dst, ty, "list", "range", true);
        self.seed_call_module_heap_drop_route(dst, ty);
        Ok(())
    }
}
