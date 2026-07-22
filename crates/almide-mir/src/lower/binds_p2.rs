impl LowerCtx {
    /// The type-driven scope-end drop handle for a `Map[String, <Named>]` value (the
    /// desugared map literal's `from_list_hobj` result, split layout): a VARIANT value
    /// routes to the generated `$__drop_map_<V>` (key rc_dec + flat/recursive value free,
    /// generated for EVERY variant); a SCALAR-ONLY record to `$__drop_map_rec_<R>`
    /// (both slots flat rc_dec). A heap-field RECORD value returns `None` — no generated
    /// sweep exists, so the bind keeps the honest deferral/wall (never a leaky flat link).
    pub(crate) fn map_named_value_drop(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Map, a) = ty else { return None };
        if a.len() != 2 || !matches!(a[0], Ty::String) {
            return None;
        }
        let Ty::Named(n, _) = &a[1] else { return None };
        let ns = n.as_str();
        if self.variant_layouts.by_type.contains_key(ns) {
            return Some(format!("map_{}", crate::lower::drop_fn_ident(ns)));
        }
        if crate::lower::canonical_record_key(&self.record_layouts, ns).is_some()
            && self
                .aggregate_field_tys(&a[1])
                .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t)))
        {
            return Some(format!("map_rec_{}", crate::lower::drop_fn_ident(ns)));
        }
        None
    }

    pub(crate) fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // `let r = e!` (Unwrap — effect-fn error propagation) bound to a let/var was a deferred
        // `Const`/`Alloc{Opaque}` = a SILENT MISCOMPILE (`int.parse(s)!` bound 0, `g()!` empty).
        // The faithful lowering needs early-return-on-Err (a later brick); until then WALL it —
        // NEVER bind a silently-wrong value (the ② cardinal rule). Both scalar + heap paths.
        if matches!(&value.kind, IrExprKind::Unwrap { .. }) {
            return Err(LowerError::Unsupported(
                "unwrap `!` bound to a let/var cannot be faithfully computed (needs early-return \
                 propagation; a Const/Opaque would be a silently wrong value) not in this brick"
                    .into(),
            ));
        }
        // A BLOCK-valued bind (`let a = { let n = 5; n * n }` — an inlined pipe-lambda, or any block
        // in value position): lower the block's statements as effects in the current scope, then bind
        // `var` to the block's TAIL by recursing. Without this the Block falls through to the scalar
        // path's deferred `Const` and mis-lowers to 0. A block-local `let` extends to the outer scope
        // — a conservative, memory-safe lifetime extension (the same discipline as a deferred reassign).
        if let IrExprKind::Block { stmts, expr: Some(tail) } = &value.kind {
            for stmt in stmts {
                self.lower_stmt(stmt)?;
            }
            return self.lower_bind(var, ty, tail);
        }
        // A SHARED-CELL var (captured by a lambda AND mutated — cells.rs): bind it
        // into a 1-slot heap cell instead of a plain local, so the closure and the
        // enclosing scope share storage. Only the admitted inner classes take a cell;
        // an unadmitted class binds normally and `lift_lambda`'s mutated-capture
        // gate refuses the lift — an honest wall, never the value-copy miscompile.
        if self.cell_vars.contains(&var) {
            if let Some(class) = cell_class_of(ty) {
                return self.lower_cell_bind(var, ty, value, class);
            }
        }
        // Decomposed (#781, cog 272): the SCALAR path and the HEAP path are
        // verbatim text moves into `lower_bind_scalar` / `lower_bind_heap` —
        // behavior proven by the classify wall-list byte-identity ladder.
        if !is_heap_ty(ty) {
            return self.lower_bind_scalar(var, ty, value);
        }
        self.lower_bind_heap(var, ty, value)
    }

    /// The SCALAR half of [`Self::lower_bind`] (`!is_heap_ty(ty)`): Copy values,
    /// no ownership accounting — executable scalar calls / literals / arithmetic /
    /// if- and match-values / `??` / var copies, else the deferred `Const`
    /// (strict mode walls it). Verbatim text move.
    fn lower_bind_scalar(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // Scalar binding: a Copy value, no ownership accounting. A RESOLVABLE
        // scalar call (`let n = add(2, 3)`, `let m = string.len(s)`) is lowered to
        // a real executable `CallFn` (args materialized, the scalar result bound)
        // so it RUNS. Any other scalar value — arithmetic, a literal, an
        // unresolvable Method/Computed call — keeps the deferred `Const` + elided-
        // caps marker: its CONTENT is carried by a later brick, its calls still
        // folded for capabilities (`var n = obj.m()` elided ⇒ honest caps taint).
        if let Some(dst) = self.try_lower_scalar_call(value, ty) {
            self.value_of.insert(var, dst);
            return Ok(());
        }
        // An INT literal carries its real value (`ConstInt` → `(i64.const v)`),
        // the scalar-value foundation; other scalars stay the deferred `Const`. A FLOAT
        // literal carries its f64 BITS the same way (the float-floor render reinterprets).
        // A BOOL literal materializes to ConstInt 0/1 (else `var b=true` stays a deferred
        // Const 0, and `if b` / `match b { true=>.. }` always takes the false arm).
        if let IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } = &value.kind
        {
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
        }
        // A scalar Int Add/Sub/Mul computes its real value (IntBinOp), and a
        // scalar prim-floor call (`let n = prim.load32(a)`) becomes an Op::Prim —
        // both via lower_scalar_value; outside the subset it rolls back to `Const`.
        // A UnOp (`let hc = not list.is_empty(xs)`, `let m = -n`) goes the SAME way —
        // without it, `not <call>` fell to the deferred `Const` below (the operand call
        // unemitted, the var silently 0 → the `not list.is_empty` render_el miscompile).
        if let IrExprKind::BinOp { .. }
        | IrExprKind::UnOp { .. }
        | IrExprKind::RuntimeCall { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
            self.ops.truncate(mark);
        }
        // A scalar `if`/`match` VALUE (`let step = if c then 0 else 1`) EXECUTES — only
        // the taken arm runs — via the if-marker machinery; a non-literal `match` or a
        // non-scalar subject falls through to the deferred `Const`.
        if let IrExprKind::If { cond, then, else_ } = &value.kind {
            if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
        }
        if let IrExprKind::Match { subject, arms } = &value.kind {
            // A single-arm tuple-destructure `let n = match pair { (_, n) => n }` extracting a
            // SCALAR component — semantically `let n = pair.<i>` (the non-tail tuple-accumulator
            // `fold` cursor extraction). Load the real scalar slot value (a Copy — no ownership).
            if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
                if !is_heap_ty(&elem_ty) {
                    let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                    let mark = self.ops.len();
                    if let Some(dst) = self.lower_scalar_value(&synth) {
                        self.value_of.insert(var, dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                }
            }
            // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
            // `let v = match t { Num(n) => n, … }`. Without this the ctor-pattern match
            // fell through to a deferred Const 0 (a silent miscompile).
            if let Some(dst) = self.try_lower_custom_variant_match(subject, arms, ty) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
            // A VARIANT (Option/Result) subject — execute the tag-read value-match
            // (only the taken arm runs, the scalar payload bound). A ctor pattern is not
            // `subj == lit`, so it can't reach `desugar_match_to_if`; without this the
            // result stayed an unset deferred Const (a silent 0).
            if is_variant_ty(&subject.ty) {
                if let Some(dst) = self.try_lower_variant_value_match(subject, arms, ty) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                // Outside the executable subset a Const-0 would silently pick a wrong
                // arm — WALL (the discipline: an unfaithful variant match rejects, never
                // emits a deferred 0).
                return Err(LowerError::Unsupported(
                    "variant (Option/Result) match bound to a let/var outside the \
                     executable subset cannot be faithfully computed (a Const-0 would \
                     silently pick a wrong arm) not in this brick"
                        .into(),
                ));
            }
            // A single-arm tuple-destructure `let r = match t { (a, b) => <body> }` binding
            // MULTIPLE components (not the single-extract case above): bind each component from its
            // tuple SLOT (the layout-aware loader), then lower the arm body as the bound value.
            // WITHOUT this the multi-bind tuple match fell to the deferred `Const 0` below (a, b
            // read 0). SCALAR result only (a heap arm value needs the merged-result path); rolls
            // back to the Const on a miss.
            if matches!(subject.ty, Ty::Tuple(_))
                && arms.len() == 1
                && arms[0].guard.is_none()
                && matches!(&arms[0].pattern, almide_ir::IrPattern::Tuple { .. })
                && !is_heap_ty(ty)
            {
                if let almide_ir::IrPattern::Tuple { elements } = &arms[0].pattern {
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    // Materialize the tuple subject as a borrowed handle (its slots are real).
                    if let Ok(Some(CallArg::Handle(subj))) = self
                        .lower_call_args(std::slice::from_ref(subject))
                        .map(|v| v.into_iter().next())
                    {
                        if self.try_lower_tuple_destructure(elements, subj, Some(&subject.ty)) {
                            if let Some(dst) = self.lower_scalar_value(&arms[0].body) {
                                self.value_of.insert(var, dst);
                                return Ok(());
                            }
                        }
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
            }
            if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
                // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
                // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
                let mark = self.ops.len();
                let lhh = self.live_heap_handles.len();
                if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                self.ops.truncate(mark);
                self.live_heap_handles.truncate(lhh);
            }
        }
        // `let idx = string.index_of(s, x) ?? -1` — a `??` over a materialized Option
        // EXECUTES to a scalar (tag read + payload/fallback), unwrapping the self-host
        // Option[Int] fns; outside the subset a `??` over a VARIANT operand can't read
        // the tag (e.g. a USER-function Option/Result result not yet tracked as
        // materialized) — a Const-0 would be a wrong value, so WALL (never silently 0).
        if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
            if is_variant_ty(&expr.ty) {
                return Err(LowerError::Unsupported(
                    "?? over an Option/Result operand outside the executable subset (e.g. a \
                     user-function result not tracked as materialized) cannot be faithfully \
                     computed (a Const-0 would be a wrong value) not in this brick"
                        .into(),
                ));
            }
        }
        // `let v = w` aliasing a SCALAR var — v denotes the SAME value (a scalar is freely
        // duplicable: no copy, no ownership). Without this, a bare-Var scalar RHS fell to the
        // deferred `Const` below and silently became 0 (the param-alias zeroing trap).
        //
        // BUT a MUTABLE `var v = w` must get its OWN local: if it aliased w's local, a later
        // `v = …` reassignment would `SetLocal` w's slot and SILENTLY CORRUPT w (the sha1
        // `var a = h0; … a = temp` trap that clobbered h0). Seed a fresh scalar local with a
        // type-agnostic i64 copy (`v = w + 0` — integer-add of 0 is identity on the i64-uniform
        // bits of Int/Float/Bool), so reassigning `v` never touches `w`. An immutable `let v = w`
        // is never reassigned, so the cheaper alias stays.
        if let IrExprKind::Var { id } = &value.kind {
            if let Ok(src) = self.value_for(*id) {
                if self.binding_is_mutable && !is_heap_ty(&value.ty) {
                    let zero = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: zero, value: 0 });
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp {
                        dst,
                        op: crate::IntOp::Add,
                        a: src,
                        b: zero,
                    });
                    self.value_of.insert(var, dst);
                } else {
                    self.value_of.insert(var, src);
                }
                return Ok(());
            }
        }
        // `let d = r.x` / `let d = t.0` / `let d = xs[i]` — a SCALAR field / element
        // projection LOADS the real value from the materialized aggregate's layout slot
        // (the VALUE MODEL); `xs[i]` is a bounds-checked `$elem_addr` load. Outside the
        // materialized subset it rolls back to the deferred `Const`.
        // A `Var` RHS reaches here only when the alias arm above MISSED (`value_for`
        // resolves locals, not globals): `let id = region_count` — a GLOBAL read. The
        // scalar-value path routes it through `value_or_global` (a mutable global's
        // slot Load / an immutable one's const materialization), a fresh dst either
        // way — no alias to protect, so the mutable-binding `+0` copy is not needed.
        if let IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::Var { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return Ok(());
            }
            self.ops.truncate(mark);
        }
        let dst = self.fresh_value();
        self.value_of.insert(var, dst);
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall("binding"));
        }
        self.ops.push(Op::Const { dst });
        self.record_elided_calls(value);
        return Ok(());
    }

    /// The HEAP half of [`Self::lower_bind`]: the heap-`??` executable subset,
    /// then the fresh-vs-alias match over every heap producer. Verbatim text move.
    fn lower_bind_heap(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        if self.try_lower_bind_heap_unwrap_or_precheck(var, value)? {
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
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(true);
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
            // value (the unwrapped/defaulted/mapped result, deferred like every
            // Opaque); its operand's calls are captured by `record_elided_calls`.
            // (Almide has NO try/catch: `e?` is `Result → Option`, `e ?? d` is
            // unwrap-or-default, `e?.f` is optional chaining — all TOTAL value maps, no
            // control flow. Only `e!` (`Unwrap`, effect-fn) PROPAGATES an error — an
            // early return that is DEFERRED here: the always-continue path is self-
            // consistent (each handle still drops exactly once, so memory-safe); error
            // propagation is functional, not a safety property.)
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
            | IrExprKind::Range { .. }
            // A RUNTIME CALL result is a fresh value (its call is elided ⇒ the gate
            // taints the function honestly, like Method/Computed).
            | IrExprKind::RuntimeCall { .. } => {
                self.lower_bind_heap_fresh(var, ty, value)
            }
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
            other => Err(LowerError::Unsupported(format!(
                "heap bind from {} not in this brick",
                kind_name(other)
            ))),
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
}
