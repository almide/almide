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
                        if self.try_lower_tuple_destructure(elements, subj) {
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
                return Ok(());
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
        match &value.kind {
            // Alias: `var b = a` — b is a NEW handle denoting the SAME heap
            // object as a, acquiring its own owned reference (the single
            // fresh-vs-alias decision). `value_or_global` (not `value_for`):
            // `let x = toplib.SYSTEM` aliases a MODULE-LEVEL global — the global
            // materializes its cached fresh owned copy (const-init only, zero
            // calls injected), and the Dup below co-owns it (#486 bind shape).
            IrExprKind::Var { id } => {
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
            IrExprKind::Lambda { params, body, .. } => {
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
                // `let s = a + b` — a string concat EXECUTES to a fresh owned String (via the
                // self-host __str_concat), held by the binding and dropped at scope end.
                if let Some(dst) = self.try_lower_concat_str(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // `let ys = xs + [7]` — a SCALAR-element list concat EXECUTES to a fresh owned list
                // (via the self-host __list_concat), held by the binding and dropped at scope end.
                // The result is a REAL, POPULATED block (len(a)+len(b) copied slots), so a later
                // `ys[i]` may index it directly. (A heap-element list concat returns None and falls
                // through to the deferred Opaque.)
                if let Some(dst) = self.try_lower_concat_list(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    self.materialized_lists.insert(dst);
                    return Ok(());
                }
                // `let s = "x=${n} y=${t}"` — a STRING INTERPOLATION over the executable
                // subset (Lit / String Var/LitStr / Int Var/LitInt parts) EXECUTES to a
                // fresh owned String via the same __str_concat chain, byte-matching v0;
                // held by the binding and dropped at scope end. An interp with a compound
                // (`${list}`) or call (`${f()}`) operand falls through to the deferred
                // `Alloc{Opaque}` below, unchanged.
                if let IrExprKind::StringInterp { parts } = &value.kind {
                    if let Some(dst) = self.try_lower_string_interp(parts) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    // STRICT value mode: an interp the executable subset declined (a
                    // BLOCK-bodied operand — `${int.to_string({ let x = …; x * 3 })}` —
                    // or another non-lowerable piece) must NOT defer to the Opaque
                    // below: the binding reads back as an EMPTY string while native
                    // prints the real text — a silent wrong value on the verified
                    // default (the C-136 elide family, interp edition). REFUSE — the
                    // fn walls and v0 emits the correct bytes.
                    if crate::lower::strict_values() {
                        return Err(LowerError::Unsupported(
                            "string interpolation outside the executable subset — \
                             deferring it would read back an empty string not in this \
                             brick"
                                .into(),
                        ));
                    }
                }
                // `let xs = ["a" + "b", "c"]` — a List[String] literal with fresh-owned elements
                // (the heap-container-element concat position; the −214 caps recovery).
                if let Some(dst) = self.try_lower_str_list_literal(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // An Option ctor in the executable subset (`Some(scalar)` / `None`) is
                // MATERIALIZED + tracked so a later `match` over the bound var executes;
                // everything else is the deferred fresh `Alloc` (value-semantics).
                if let Some(dst) = self.try_lower_option_ctor(value, ty) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // HONEST-WALL SAFETY NET: a `some(<list>)` / `ok(<list>)` whose LIST payload the ctor
                // materializer DECLINED (an exotic element the scalar/String/literal arms don't cover —
                // e.g. a computed List[record]/List[List]) must NOT fall to the deferred Opaque `Alloc`
                // below, which would read `none` / `ok([])` (the some(computed)/ok(computed) silent
                // miscompile the adversarial fuzz surfaced). Wall instead — a wall is always safe, a
                // wrong byte never is.
                if let IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } = &value.kind {
                    use almide_lang::types::constructor::TypeConstructorId;
                    if matches!(&expr.ty,
                        Ty::Applied(TypeConstructorId::List, _) | Ty::Applied(TypeConstructorId::Map, _))
                    {
                        return Err(LowerError::Unsupported(
                            "some/ok of a list or map payload outside the executable subset cannot be \
                             faithfully materialized in this brick (e.g. an empty `[:]` — would defer \
                             to an empty container)"
                                .into(),
                        ));
                    }
                }
                // A scalar-field tuple `(a, b)` of NON-LITERAL fields (vars / scalar exprs) — a
                // literal `(3, 7)` is already an `Init::IntList` below. Construct the 2-slot block
                // and store each field's computed value (the tuple-machinery construction sibling
                // of the precise destructure). A heap-field tuple falls through to the Opaque alloc.
                if let IrExprKind::Tuple { elements } = &value.kind {
                    if let Some(dst) = self.try_lower_scalar_tuple_construct(elements) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    // A HEAP-element tuple (`(1, "a")`, `(p, 9)`) — materialize the mixed block
                    // + track its heap-slot mask, so `t.0`/`${tuple}` execute and the block (with
                    // its owned heap elements) is reclaimed by a masked recursive drop. Rolls back
                    // on a non-lowerable element (then Opaque → the Display walls).
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_tuple_construct(elements) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A SCALAR-only record `R { x: 3, y: 4 }` — build the tight-packed,
                // width-aware block + store each field at its layout slot (the VALUE
                // MODEL: `r.x`/`r.y` read back exactly what was stored). A HEAP-field
                // record (a String/List field) needs an ownership-aware recursive drop
                // this brick does not have, so it falls through to the deferred Opaque
                // (which the field-access path then WALLS rather than mis-reads).
                if let IrExprKind::Record { .. } = &value.kind {
                    if let Some(dst) = self.try_lower_scalar_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    // A VARIANT record-ctor literal (`Data { … }`) outside the builder's
                    // subset must WALL, not defer: a deferred Opaque variant read through a
                    // CALL arg (`tree_sum(t)`) bypasses the match-side deferred gate and the
                    // callee reads a garbage tag — the same miscompile class the Call-ctor
                    // bind gate above already errors on.
                    if let IrExprKind::Record { name: Some(n), .. } = &value.kind {
                        if self.variant_layouts.ctor_to_type.contains_key(n.as_str()) {
                            return Err(LowerError::Unsupported(format!(
                                "variant record-ctor `{}` bound to a let/var cannot be \
                                 faithfully materialized in this brick (a field outside the \
                                 ctor subset)",
                                n.as_str()
                            )));
                        }
                    }
                    // A record with one or more HEAP fields (`R { name: "x", n: i }`) —
                    // materialize the mixed scalar+heap block + track its heap-slot mask, so a
                    // `r.n` scalar read AND a `r.name` heap read execute and the block (with its
                    // owned heap fields) is reclaimed by a masked recursive drop. Rolls back on
                    // a partially-lowered out-of-subset field (a heap-returning-call field).
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A SPREAD record `R { ...base, f: override }` — build a fresh block of the
                // same layout, COPYING each non-overridden field from `base` (a scalar load,
                // a heap-handle Dup so both records own a distinct reference) and storing the
                // overrides. So `let b2 = Box { ...b, value: 8 }` reads `b2.value=8
                // b2.label=old` while `b.label` still reads `old`. Rolls back to the deferred
                // Opaque (whose field reads WALL) on a non-materialized base / out-of-subset
                // override — never wrong bytes.
                if let IrExprKind::SpreadRecord { .. } = &value.kind {
                    let mark = self.ops.len();
                    let lhh_mark = self.live_heap_handles.len();
                    if let Some(dst) = self.try_lower_spread_record_construct(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                // A scalar `List[Int/Float/Bool]` literal with COMPUTED elements (`[1.0, inf, 0.5]`,
                // `[a, a]`) — build the block + store each slot (an all-literal list is the IntList
                // path in `alloc_init` below; a computed element can't fold to a constant).
                if let Some(dst) = self.try_lower_scalar_list_construct(value) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
                // A NON-EMPTY `List[heap]` LITERAL that NONE of the materialization paths above
                // could build — a list of heap-FIELD records/tuples (`[R{name:String,…}, …]`), a
                // list of lists, a list of heap call-results. The flat `Init::Opaque` fallback
                // below would emit an EMPTY len-0 block (`list_new(0, …)`); a later `list.map` /
                // `list.sort_by` / `xs[i]` over it then silently reads NOTHING = wrong/empty bytes.
                // (A heap-field-record element needs a TWO-LEVEL recursive drop — the list frees
                // each record, each record frees its String fields — which the single-level
                // `DropListStr` cannot express without a new ownership op; that is the
                // nested-ownership frontier, out of this brick.) WALL the function cleanly instead
                // of mis-valuing it — the render discards it (no invalid wasm, no empty output).
                // GATED to a NON-EMPTY heap-element `List` LITERAL (an empty `[]`, a scalar list,
                // and a `List[String]`/scalar-aggregate list are all handled above), so this only
                // rejects the genuinely-unmaterializable case.
                if let IrExprKind::List { elements } = &value.kind {
                    use almide_lang::types::constructor::TypeConstructorId;
                    let heap_elem_list = matches!(ty,
                        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
                    if heap_elem_list && !elements.is_empty() {
                        // A List[Record] literal materializes via the record-list builder (drop →
                        // $__drop_list_<R>); other nested-ownership element lists stay walled.
                        if let Some(dst) = self.try_lower_record_list_literal(value) {
                            self.value_of.insert(var, dst);
                            return Ok(());
                        }
                        return Err(LowerError::Unsupported(
                            "non-empty List[heap] literal with nested-ownership elements \
                             (a heap-field record/tuple, a list, a call result) cannot be \
                             faithfully materialized in this brick (walled, not emitted empty)"
                                .into(),
                        ));
                    }
                }
                let dst = self.fresh_value();
                let repr = repr_of(ty)?;
                let init = alloc_init(value);
                // An all-literal `Init::IntList` is a REAL, POPULATED block (every slot a constant) —
                // admit a direct `xs[i]` bounds-checked load over it. An `Init::Opaque` (a deferred /
                // unsupported value) is NOT tracked: indexing it would trap on cap 0.
                let real_list = matches!(init, Init::IntList(_));
                self.value_of.insert(var, dst);
                self.ops.push(Op::Alloc { dst, repr, init });
                self.live_heap_handles.push(dst);
                if real_list {
                    self.materialized_lists.insert(dst);
                }
                self.record_elided_calls(value);
                Ok(())
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
            | IrExprKind::TupleIndex { .. } => {
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
            // `var x = f(...)` — a USER call returning a heap value. The result is
            // a FRESH OWNED heap value (the callee's return-mode signature, read
            // from the bind's heap type — the checker need not open the callee).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                // A custom-variant CONSTRUCTOR `let t = Num(9)` (ADT brick 2) is NOT a call —
                // build the tagged value-model block (tag@slot0 + scalar fields@slot1..), bound
                // + dropped at scope end (cert `i` + `d`, like the scalar-record bind). Must
                // precede the CallFn emission, which would emit a dangling `(call $Num)`. A
                // heap/recursive ctor field is ADT brick 5 → WALL (never a wrong-bytes block).
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) {
                    if let Some(dst) = self.try_lower_variant_ctor(value) {
                        self.value_of.insert(var, dst);
                        self.live_heap_handles.push(dst);
                        return Ok(());
                    }
                    return Err(LowerError::Unsupported(format!(
                        "variant constructor `{}` bound to a let/var cannot be faithfully \
                         materialized in this brick (a heap/recursive field — ADT brick 5)",
                        name.as_str()
                    )));
                }
                let lowered = self.lower_call_args(args)?;
                let dst = self.fresh_value();
                // A function-VALUED result (`let f = mk()`) is a CLOSURE BLOCK — the uniform
                // heap representation (`repr_of(Ty::Fn)` = Ptr), owned + dropped at scope end
                // like any heap result; `closure_values` (below) makes a later `f(args)`
                // dispatch through it.
                let repr = repr_of(ty)?;
                self.value_of.insert(var, dst);
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                self.live_heap_handles.push(dst);
                if crate::lower::is_res_intlist_strlist_ty(ty) {
                    // `result.collect` — Result[List[Int], List[String]]: the TAG-AWARE
                    // generated `$__drop_res_ilsl` (Err → recursive string free, Ok → flat;
                    // either flat class would leak or double-free one side).
                    self.variant_drop_handles.insert(dst, "res_ilsl".to_string());
                    self.materialized_results_str.insert(dst);
                } else if crate::lower::is_list_list_str_ty(ty) {
                    self.list_list_str_lists.insert(dst);
                } else if crate::lower::is_list_str_str_ty(ty) {
                    // `List[(String,String)]` (map.entries) — DropListStrStr frees each tuple's two
                    // Strings; the flat heap_elem_lists DropListStr would leak them (a render loop OOMs).
                    self.str_str_elem_lists.insert(dst);
                } else if crate::lower::is_list_int_str_ty(ty) {
                    // `List[(Int,String)]` (list.enumerate) — recursive `$__drop_list_int_str` (rc_dec
                    // each tuple's String); the flat heap_elem_lists DropListStr would leak them.
                    self.variant_drop_handles.insert(dst, "list_int_str".to_string());
                } else if crate::lower::is_map_ivh_ty(ty) {
                    // `Map[Int, String]` — `$__drop_map_ivh` rc_decs each OWNED value slot.
                    self.variant_drop_handles.insert(dst, "map_ivh".to_string());
                } else if crate::lower::is_map_hval_ty(ty) {
                    // `Map[String, List[scalar]]` — `$__drop_map_hval` rc_decs all 2n slots.
                    self.variant_drop_handles.insert(dst, "map_hval".to_string());
                } else if let Some(hname) = self.map_named_value_drop(ty) {
                    // `Map[String, <record/variant>]` — the desugared map literal's
                    // from_list result (type-driven sweep; see `map_named_value_drop`).
                    self.variant_drop_handles.insert(dst, hname);
                } else if crate::lower::is_map_msv_ty(ty) {
                    // `Map[String, Map[String, String]]` — `$__drop_map_msv` sweeps each
                    // last-ref inner map's String slots (a flat rc_dec would leak them).
                    self.variant_drop_handles.insert(dst, "map_msv".to_string());
                } else if crate::lower::is_map_mlo_ty(ty) {
                    // `Map[String, List[Option[Int]]]` — `$__drop_map_mlo` sweeps each
                    // last-ref value list's Option slots (a flat rc_dec would leak them).
                    self.variant_drop_handles.insert(dst, "map_mlo".to_string());
                } else if crate::lower::is_lenlist_list_ty(ty) {
                    // `List[Result[_, String]]`/`List[Option[String]]` — the len-loop drop; the
                    // flat DropListStr would leak each element's owned payload slots.
                    self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
                } else if crate::lower::is_opt_list_str_ty(ty) {
                    // `Option[List[String]]` (the heap-acc fold value) — physically a 0/1-element
                    // List[List[String]]; the nested DropListListStr sweep is its exact free (the
                    // flat DropListStr would leak the stack Strings).
                    self.list_list_str_lists.insert(dst);
                } else if matches!(ty,
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
                {
                    // `Map[String, <scalar>]` (split layout, @4 = n): the DropListStr sweep
                    // rc_decs exactly the n deep-copied key Strings (scalar value slots
                    // untouched) — the bare flat rc_dec LEAKED every key copy per bind (a
                    // latent leak the map.fold heap-acc loop made observable at a 4MB cap).
                    self.heap_elem_lists.insert(dst);
                } else if is_heap_elem_list_ty(ty) {
                    self.heap_elem_lists.insert(dst);
                }
                // A `Value` result from a user fn (`let v = parse_number(c, raw)`) drops via the
                // runtime-tag-dispatched `DropValue` — the SAME marking the Module-call bind path does
                // (was missing here, so a let-bound Named-call Value leaked: a parse loop OOMs).
                if crate::lower::is_value_ty(ty) {
                    self.value_handles.insert(dst);
                }
                // A user fn RETURNING a function value (`let f = mk()` / `let f = adder(3)`)
                // yields a CLOSURE BLOCK — a fresh owned heap value (already in the scope-end
                // set like any heap result): track it so a later `f(args)` dispatches through
                // `Op::CallIndirect` via `emit_closure_call`.
                if matches!(ty, Ty::Fn { .. }) {
                    self.closure_values.insert(dst);
                }
                // A user function returning Option/Result yields a REAL same-layout variant block
                // (the v1 calling convention — `seed_variant_param`'s contract). SEED its READ-shape
                // so a later `match x { … }` / `x ?? d` over the LET-BOUND var EXECUTES (reads the
                // tag) exactly as the direct-call-arg position already does (`lower_call_args`'s
                // Named arm). Adds ONLY layout knowledge — `dst` is already an owned heap value
                // dropped at scope end, so no ownership/cert change. This is what made
                // `let parsed = parse_oct(d); match parsed { … }` (num_signed_base, after the
                // let-bound-heap-`if` tail-duplication) lower instead of wall.
                if is_variant_ty(ty) {
                    self.seed_variant_param(dst, ty);
                } else if let Some((_, tys)) = self.aggregate_field_tys(ty) {
                    // A user function returning a RECORD/TUPLE yields a REAL same-layout block (the
                    // callee built it via try_lower_record_construct). Seed its READ-shape
                    // (materialized_aggregates) so a field read `p.y` loads the real slot instead of
                    // falling back to the container-grain Dup (which returns the whole record — the
                    // `mk(5).y` empty-string miscompile), AND its heap-slot MASK (record_masks) so the
                    // OWNED scope-end drop frees exactly the heap fields (no leak, no double-free).
                    let heap_slots: Vec<usize> =
                        (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                    self.materialized_aggregates.insert(dst);
                    self.record_masks.insert(dst, heap_slots);
                    // A record with a Map/List[heap]/record/Value field drops RECURSIVELY ($__drop_<R>),
                    // not the flat masked DropListStr (which would leak the nested heap) — route it. An
                    // ANONYMOUS record return whose flat one-level mask would leak a nested heap field
                    // (`{ data: Bytes, state: Cfb8State }` — aes cfb8) routes to its synthesized
                    // `__drop_anonrec_<hash>` (so `state` is freed through `__drop_Cfb8State`).
                    if let Some(name) = self.record_or_anon_drop_type_name(ty) {
                        self.variant_drop_handles.insert(dst, name);
                    }
                }
                Ok(())
            }
            // `var x = string.trim(s)` — a stdlib MODULE call returning a heap
            // value. Admitted only when first-order + pure (else walled); the
            // fresh owned result is bound and dropped at scope end, exactly like
            // the `Named` case above.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let dst =
                    self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, ty)?;
                self.value_of.insert(var, dst);
                // A SCALAR-element `List[Int/Float/Bool]` result from a self-host list call is a REAL,
                // POPULATED block — admit a direct `xs[i]` — ONLY when the call is FAITHFULLY executable:
                //  (1) every closure arg LIFTED (an unlifted `list.map(fns, (f) => f(10))` runs the
                //      combinator with a missing slot → empty/garbage), AND
                //  (2) no DATA argument carries an UN-REPRESENTABLE function type (this comment
                //      historically said "list.map(fns, …) over fns: List[(Int)->Int] — a list of
                //      closures the v1 model cannot represent" — no longer true: B36 shipped
                //      `List[<Fn>]` literal construction + a generated per-element `$__drop_
                //      list_closure`, so a `List[Fn]` DATA arg is now a REAL, populated,
                //      correctly-freed block — excluded below). A Fn buried in some OTHER shape
                //      (a record/tuple field, a nested nested-List[List[Fn]]) is still unrepresented
                //      and stays walled. The combinator's OWN closure arg (a `Lambda`/`FnRef`,
                //      function-typed by construction) is EXCLUDED too — it is handled by (1), and
                //      `(p) => p.x` over `points: List[Point]` is the faithful case that must stay
                //      tracked.
                // Otherwise the result is unmaterialized and a `xs[i]` over it would TRAP on cap 0, so
                // it is left deferring to `Const 0` (mis-valued, never a new runtime crash).
                let data_arg_has_fn = args.iter().any(|a| {
                    // A let-bound lambda passed BY NAME (`let g = (x) => …; xs |> list.map(g)`) is a
                    // CLOSURE arg — try_lower_defunc_list_hof resolves it via lambda_bindings and inlines
                    // it faithfully (calls.rs). Without recognizing the `Var` here it is misread as a
                    // fn-typed DATA arg (a `list.map(fns, …)` over a list-of-closures the v1 model can't
                    // represent) and the guard below WALLS it — even though the inline succeeded. This is
                    // the bind-vs-tail discrepancy: the tail/value position has no such data-arg guard, so
                    // `let g = …; xs |> map(g)` lowered as a TAIL but walled as a `let r = xs |> map(g)`.
                    let is_closure_arg = matches!(
                        &a.kind,
                        IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } | IrExprKind::ClosureCreate { .. }
                    ) || matches!(&a.kind, IrExprKind::Var { id } if self.lambda_bindings.contains_key(id));
                    // `List[<Fn>]` is B36's representable shape — excluded from the wall.
                    let is_representable_closure_list = matches!(&a.ty,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, e)
                            if e.len() == 1 && matches!(e[0], Ty::Fn { .. }));
                    !is_closure_arg && !is_representable_closure_list && crate::lower::ty_contains_fn(&a.ty)
                });
                let faithful = !self.last_call_had_unlifted_closure && !data_arg_has_fn;
                // WALL the UNFAITHFUL higher-order combinator instead of silently
                // mis-valuing it. A HOF call (`list.map`/`filter`/`fold`…) over a
                // CAPTURING/param-invoking lambda (no liftable slot) or a fn-typed DATA
                // arg (`list.map(fns, (f) => f(10))` over `fns: List[(Int)->Int]` — a
                // list of closures the v1 model cannot represent) runs the self-host
                // combinator with a missing/garbage closure and produces a zero-filled
                // result. Leaving the result deferred (a `Const 0` `xs[i]`) emits WRONG
                // BYTES — a silent miscompile. Walling the whole function here is the
                // honest outcome (render discards it cleanly; no invalid wasm, no wrong
                // output). The FAITHFUL case (every closure lifted, no fn-typed data —
                // `list.map(xs, (x) => x + 1)`, `(p) => p.x` over `List[Point]`) is
                // UNTOUCHED, so the in-scope HOF byte-matches stay materialized.
                if crate::lower::is_higher_order(args) && !faithful {
                    return Err(LowerError::Unsupported(format!(
                        "{}.{} with an unliftable/closure-list higher-order argument \
                         cannot execute faithfully in this brick (walled, not mis-valued)",
                        module.as_str(),
                        func.as_str()
                    )));
                }
                if is_scalar_elem_list_ty(ty) && faithful {
                    self.materialized_lists.insert(dst);
                }
                // A faithful `List[heap]` result (`string.split`/`chars`/`lines` → `List[String]`,
                // or a heap-element list combinator) is ALSO a REAL, POPULATED nested-ownership block
                // whose slots hold owned element HANDLES — so a value-position `xs[i]` over the bound
                // var can LoadHandle element i at `$elem_addr` (the heap-element borrow path in
                // `try_lower_heap_field_borrow`, gated on `materialized_lists`). Without registering
                // it, `parts[i]` fell to the container-grain `Dup` of the WHOLE list → a String
                // consumer read the list HEADER bytes (the `string.split`-subscript miscompiles).
                // Narrowed to `List[heap]` (NOT the broader Option/Result/Map that
                // `is_heap_elem_list_ty` also matches) — only a real list is `[i]`-indexable here.
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                        if a.len() == 1 && is_heap_ty(&a[0]))
                    && faithful
                {
                    self.materialized_lists.insert(dst);
                }
                // A BORROW result (`prim.load_str` of a list slot — the list still owns it) is NOT
                // added to the scope-end drop set; everything else is a fresh owned value.
                if !self.param_values.contains(&dst) {
                    self.live_heap_handles.push(dst);
                }
                // A self-host Option fn (`list.get`) returns a real materialized Option —
                // track the bound result so a later `match` over the var EXECUTES.
                if is_self_host_option_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_options.insert(dst);
                }
                // A self-host Result fn (`int.parse`) returns a real materialized Result — track it
                // so a later `match r { Ok(v) => …, Err(e) => … }` over the var EXECUTES.
                if is_self_host_result_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_results.insert(dst);
                }
                // A self-host HEAP-Ok Result fn (`value.as_string`/`value.as_array`) — track it in the
                // cap-as-tag set so a `match` reads tag @16 + binds the @12 payload. The DROP differs
                // by Ok-arm: a `List[Value]` Ok (`value.as_array`) frees recursively
                // (`value_result_lists` → `DropResultListValue`), else a String Ok flat (`DropListStr`).
                if crate::lower::is_self_host_result_str_module_fn(module.as_str(), func.as_str()) {
                    self.materialized_results_str.insert(dst);
                    if crate::lower::is_result_listval_ty(ty) {
                        self.value_result_lists.insert(dst);
                    } else if crate::lower::is_value_result_ty(ty) {
                        // `Result[Value, String]` (value.get) — a single dynamic Value Ok, freed
                        // recursively by `Op::DropResultValue` (Ok → `$__drop_value`).
                        self.value_result_results.insert(dst);
                    } else {
                        self.heap_elem_lists.insert(dst);
                    }
                }
                // A `List[String]` result (string.split / a List[String] combinator) is a
                // nested-ownership list — its scope-end drop must recursively free elements.
                if crate::lower::is_res_intlist_strlist_ty(ty) {
                    // `result.collect` — Result[List[Int], List[String]]: the TAG-AWARE
                    // generated `$__drop_res_ilsl` (Err → recursive string free, Ok → flat;
                    // either flat class would leak or double-free one side).
                    self.variant_drop_handles.insert(dst, "res_ilsl".to_string());
                    self.materialized_results_str.insert(dst);
                } else if crate::lower::is_list_list_str_ty(ty) {
                    self.list_list_str_lists.insert(dst);
                } else if crate::lower::is_list_str_str_ty(ty) {
                    // `List[(String,String)]` (map.entries) — DropListStrStr frees each tuple's two
                    // Strings; the flat heap_elem_lists DropListStr would leak them (a render loop OOMs).
                    self.str_str_elem_lists.insert(dst);
                } else if crate::lower::is_list_int_str_ty(ty) {
                    // `List[(Int,String)]` (list.enumerate) — recursive `$__drop_list_int_str`; the flat
                    // heap_elem_lists DropListStr would leak each tuple's String (a 10⁴ loop OOMs).
                    self.variant_drop_handles.insert(dst, "list_int_str".to_string());
                } else if crate::lower::is_map_ivh_ty(ty) {
                    // `Map[Int, String]` — `$__drop_map_ivh` rc_decs each OWNED value slot.
                    self.variant_drop_handles.insert(dst, "map_ivh".to_string());
                } else if crate::lower::is_map_hval_ty(ty) {
                    // `Map[String, List[scalar]]` — `$__drop_map_hval` rc_decs all 2n slots.
                    self.variant_drop_handles.insert(dst, "map_hval".to_string());
                } else if let Some(hname) = self.map_named_value_drop(ty) {
                    // `Map[String, <record/variant>]` — the desugared map literal's
                    // from_list result (type-driven sweep; see `map_named_value_drop`).
                    self.variant_drop_handles.insert(dst, hname);
                } else if crate::lower::is_map_msv_ty(ty) {
                    // `Map[String, Map[String, String]]` — `$__drop_map_msv` sweeps each
                    // last-ref inner map's String slots (a flat rc_dec would leak them).
                    self.variant_drop_handles.insert(dst, "map_msv".to_string());
                } else if crate::lower::is_map_mlo_ty(ty) {
                    // `Map[String, List[Option[Int]]]` — `$__drop_map_mlo` sweeps each
                    // last-ref value list's Option slots (a flat rc_dec would leak them).
                    self.variant_drop_handles.insert(dst, "map_mlo".to_string());
                } else if crate::lower::is_lenlist_list_ty(ty) {
                    // `List[Result[_, String]]`/`List[Option[String]]` — the len-loop drop; the
                    // flat DropListStr would leak each element's owned payload slots.
                    self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
                } else if crate::lower::is_opt_list_str_ty(ty) {
                    // `Option[List[String]]` (the heap-acc fold value) — physically a 0/1-element
                    // List[List[String]]; the nested DropListListStr sweep is its exact free (the
                    // flat DropListStr would leak the stack Strings).
                    self.list_list_str_lists.insert(dst);
                } else if matches!(ty,
                    Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                        if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
                {
                    // `Map[String, <scalar>]` (split layout, @4 = n): the DropListStr sweep
                    // rc_decs exactly the n deep-copied key Strings (scalar value slots
                    // untouched) — the bare flat rc_dec LEAKED every key copy per bind (a
                    // latent leak the map.fold heap-acc loop made observable at a 4MB cap).
                    self.heap_elem_lists.insert(dst);
                } else if is_heap_elem_list_ty(ty) {
                    self.heap_elem_lists.insert(dst);
                }
                // A `Value` result (value.str/int/… or a Value-returning combinator) drops via the
                // runtime-tag-dispatched DropValue (a heap-payload Value owns one handle).
                if crate::lower::is_value_ty(ty) {
                    self.value_handles.insert(dst);
                }
                Ok(())
            }
            // `var o = f(x)` where `f` is a lifted lambda / function-typed param returning a
            // HEAP value (`(Int) -> Option[Int]` / `-> List[Int]`): EXECUTE the closure via a
            // heap-result `Op::CallIndirect`. The result is a FRESH OWNED value (the closure
            // moves it out — cert `i`, dropped at scope end — the foundation for filter_map /
            // flat_map). A Computed callee that is NOT a known funcref falls through to the
            // deferred Opaque below.
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if self.closure_value_of(callee).is_some()
                    || Self::is_fn_member_callee(callee) =>
            {
                // A tracked closure VAR — or a RECORD-SLOT closure (`h.run("hello")` —
                // B8's Computed(Member); `closure_block_of_mut` loads the slot borrow).
                let blk = match self.closure_block_of_mut(callee) {
                    Some(b) => b,
                    None => {
                        return Err(LowerError::Unsupported(
                            "heap-result record-slot closure call over an unresolvable \
                             container not in this brick"
                                .into(),
                        ))
                    }
                };
                let repr = repr_of(ty)?;
                let lowered = self.lower_call_args(args)?;
                let dst = self.fresh_value();
                self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                // The funcref returns its Result/Option in the SAME materialized layout an `ok()`/
                // `err()` ctor builds (a lifted lambda's body goes through `materialize_result_*`), so
                // SEED its read-shape — a later `match o { ok/err }` over the bound var then reads its
                // real tag instead of walling (the higher-order-Result-callback path `fan.map` needs).
                self.seed_variant_param(dst, ty);
                // An `Option[List[String]]` closure result (the heap-acc fold's per-iteration
                // acc): the flat `heap_elem_lists` seed above would free ONE level only,
                // leaking the inner list's Strings every iteration (a fold loop OOMs) — route
                // its scope-end drop to the nested `DropListListStr` sweep instead.
                if crate::lower::is_opt_list_str_ty(ty) {
                    self.heap_elem_lists.remove(&dst);
                    self.list_list_str_lists.insert(dst);
                }
                // A MAP closure result (the map.fold heap-acc's per-iteration acc — the
                // `(a, k, v) => ["fresh": v]` fresh-map closure): the bare
                // `live_heap_handles` default is a FLAT rc_dec, which frees the map block
                // but LEAKS its key Strings every iteration (the 100k fold loop OOMs at a
                // 4MB cap). Route the scope-end drop to the DropListStr sweep — exact for
                // BOTH map layouts: `Map[String, String]` (interleaved, @4 = 2n, every
                // slot a String handle) and `Map[String, <scalar>]` (split, @4 = n, the
                // sweep rc_decs exactly the n key slots; the scalar value slots beyond
                // are untouched).
                if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                    if a.len() == 2 && matches!(a[0], Ty::String)
                        && (!is_heap_ty(&a[1]) || matches!(a[1], Ty::String)))
                {
                    self.heap_elem_lists.insert(dst);
                }
                Ok(())
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
            IrExprKind::Match { subject, arms } => {
                // `let e = match <Option[(s1,s2)]> { some(p) => p, none => (f1,f2) }` — the
                // tuple-unwrap_or desugar output: EXECUTE via component merges + ONE owned
                // block (no per-arm alloc — cert-clean single object).
                if let Some(dst) = self.try_lower_scalar_tuple_option_match_bind(subject, arms) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    self.materialized_aggregates.insert(dst);
                    return Ok(());
                }
                // A single-arm tuple-destructure `let offs = match pair { (o, _) => o }` extracting a
                // HEAP component — semantically `let offs = pair.<i>` (the non-tail tuple-accumulator
                // `fold` extraction). BORROW the slot handle (the tuple keeps ownership) then ACQUIRE
                // an OWNED reference (`Op::Dup`, cert `a`) the binding holds + drops at scope end — so
                // both the tuple's masked drop and this binding's drop are balanced (no double-free, no
                // leak). Mirrors the `Member`/`TupleIndex` heap-extraction bind arm.
                if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
                    if is_heap_ty(&elem_ty) {
                        let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                        if let Some(borrow) = self.try_lower_heap_field_borrow(&synth) {
                            let dst = self.fresh_value();
                            self.ops.push(Op::Dup { dst, src: borrow });
                            self.value_of.insert(var, dst);
                            self.live_heap_handles.push(dst);
                            return Ok(());
                        }
                    }
                }
                Err(LowerError::Unsupported(
                    "heap-result `match` bound to a let/var cannot be faithfully \
                     computed in this brick (would bind an empty deferred heap value); \
                     the merged result has no sound scope-end drop in the flat certificate"
                        .into(),
                ))
            }
            IrExprKind::If { else_, .. } => {
                // STRAIGHT-LINE identity-else shadow rebind `let acc = if cond then acc + [x] else acc`
                // (porta `serialize_opts`' 7 stacked optional-arg appends on one `args` slot). The ELSE
                // arm is EXACTLY the accumulator var — the PROVEN loop-carried `i(id)m` append slot,
                // UNROLLED straight-line. Drop-old + `SetLocal` the slot in place (the THEN arm only);
                // the new shadow ALIASES the same slot (NOT re-pushed to live_heap_handles — one
                // scope-end drop / tail move-out covers it). Each rebind folds to a `(id)` CLoop body
                // in the certificate (check_line_unroll_sound, the same unit the loop slot proves).
                if let IrExprKind::Var { id: acc_id } = &else_.kind {
                    if let Some(&acc_local) = self.value_of.get(acc_id) {
                        // The slot must be an OWNED, scope-tracked heap handle (the seed's `[]`/`""`) —
                        // NOT a borrowed param field (`param_values`), whose drop-old would release a
                        // reference we do not own. A borrow falls through to the wall.
                        if self.live_heap_handles.contains(&acc_local)
                            && !self.param_values.contains(&acc_local)
                        {
                            let mark = self.ops.len();
                            if self.try_lower_line_cond_acc(value, *acc_id, acc_local) {
                                self.value_of.insert(var, acc_local);
                                return Ok(());
                            }
                            self.ops.truncate(mark);
                        }
                    }
                }
                Err(LowerError::Unsupported(
                    "heap-result `if` bound to a let/var cannot be faithfully \
                     computed in this brick (would bind an empty deferred heap value); \
                     the merged result has no sound scope-end drop in the flat certificate"
                        .into(),
                ))
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

    /// `let (a, b) = …` — a TUPLE destructuring bind. Two sound shapes:
    ///
    /// 1. From a tuple LITERAL `(x, y)` of the same arity — lowered COMPONENT-WISE
    ///    as ordinary binds (`lower_bind` reused: a `Var` is an alias `Dup`, a
    ///    literal an `Alloc`/`Const`, a call a real `CallFn` whose caps are
    ///    captured, NOT elided). The tuple is never materialized.
    /// 2. From a tracked heap VAR `t` — each HEAP component aliases the WHOLE
    ///    container `t` (an `Op::Dup`, the container-grain field access of the
    ///    field-access op), each SCALAR component is a `Const` copy. Aliasing the
    ///    container keeps it alive for each component's lifetime (a conservative
    ///    lifetime widening, never a UAF); component-PRECISE identity (`a == t.0`)
    ///    is deferred to the layout brick.
    ///
    /// A `Wildcard` component is ignored. Anything else — a non-tuple/nested/
    /// constructor/record pattern, or a value that is neither a matching tuple
    /// literal nor a tracked heap var — stays an explicit `Unsupported` (totality).
    pub(crate) fn lower_destructure(&mut self, pattern: &IrPattern, value: &IrExpr) -> Result<(), LowerError> {
        // Shape 1: component-wise from a same-arity tuple LITERAL — each component is
        // bound to the ACTUAL element (a fresh value / alias, not a container alias),
        // the most precise lowering. The element's call caps are captured, not elided.
        if let (IrPattern::Tuple { elements: pats }, IrExprKind::Tuple { elements: vals }) =
            (pattern, &value.kind)
        {
            if pats.len() == vals.len() {
                for (p, v) in pats.iter().zip(vals) {
                    match p {
                        IrPattern::Bind { var, ty } => self.lower_bind(*var, ty, v)?,
                        IrPattern::Wildcard => {}
                        // A NESTED tuple sub-pattern `(b, c)` binds against the
                        // corresponding element value `v` — recurse (the same two sound
                        // shapes: a same-arity tuple literal binds component-wise, a
                        // tracked heap var aliases the container).
                        IrPattern::Tuple { .. } => self.lower_destructure(p, v)?,
                        _ => {
                            return Err(LowerError::Unsupported(
                                "destructure sub-pattern (only a bound var, `_`, or nested tuple) not in this brick"
                                    .into(),
                            ))
                        }
                    }
                }
                return Ok(());
            }
        }
        // Shape 2 (general): materialize/borrow the value as a SUBJECT (a tracked heap
        // var is borrowed, a fresh heap value is materialized + dropped at scope end),
        // then bind the pattern CONTAINER-GRAIN (each heap binding aliases the whole
        // subject — `bind_pattern`). Handles tuple-from-var, constructor, record, and
        // option/result destructuring; the bound vars drop at scope end.
        let subject: Option<ValueId> = if is_heap_ty(&value.ty) {
            match self.lower_call_args(std::slice::from_ref(value))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(value);
            None
        };
        // PRECISE tuple field extraction (the layout brick): a tuple value is a block
        // [rc][len][cap][f0@12, f1@20, ...]; a destructure (`let (a, b) = t`) loads each field at
        // its OWN slot instead of the container-grain alias. A SCALAR field is a value COPY; a HEAP
        // field (`let (inner, z) = n` over `((Int,Int), Int)`) is the BORROWED slot handle (the
        // tuple keeps ownership through its masked scope-end drop). Without this, `bind_pattern`
        // aliased the WHOLE container for a heap field and emitted `Const 0` for a scalar field
        // alongside it = the `8192:2000:0` miscompile.
        if let IrPattern::Tuple { elements } = pattern {
            if let Some(subj) = subject {
                // A CALL-RESULT tuple (`let (v, n) = dispatch(..)`) is a real OWNED block the
                // callee built (the `lower_tail` Tuple materialize) but `materialized_call_arg`
                // tracked it only flatly (a plain Drop would LEAK its heap slot, and it is not a
                // `materialized_aggregate` so the precise destructure below bails to the `Const 0`
                // container-alias garbage). SEED it as a masked aggregate: record the heap-slot
                // mask (so the scope-end drop is the recursive `DropListStr` that frees the owned
                // String/Value slot) + mark it `materialized_aggregates` (so per-slot borrow reads
                // execute). Only for an owned, still-live result (in `live_heap_handles`) — a
                // borrowed param/var already carries its own tracking.
                if !self.materialized_aggregates.contains(&subj)
                    && self.live_heap_handles.contains(&subj)
                {
                    // The tuple's element types: from value.ty when it is a Tuple, ELSE (brick 5) — an
                    // effect-fn `let (v,p) = f()!` whose `!` Unwrap render_program strips to a Call, so
                    // value.ty is the effect Result, NOT a Ty::Tuple — from the PATTERN's bound types.
                    // Without the pattern fallback the seed misses and the destructure container-grains
                    // (reads slot 0 as the whole handle + slot 1 as Const 0 — the `8212 / 0` garbage).
                    let elem_tys: Option<Vec<Ty>> = if let Ty::Tuple(tys) = &value.ty {
                        Some(tys.clone())
                    } else if matches!(
                        &value.kind,
                        IrExprKind::Unwrap { .. } | IrExprKind::Call { .. }
                    ) {
                        Some(
                            elements
                                .iter()
                                .map(|p| match p {
                                    IrPattern::Bind { ty, .. } => ty.clone(),
                                    _ => Ty::Unit,
                                })
                                .collect(),
                        )
                    } else {
                        None
                    };
                    if let Some(tys) = elem_tys {
                        // A (Value, scalar) tuple's Value slot needs the RECURSIVE __drop_value_tuple
                        // (a flat record_masks rc_dec leaks the Value's nested payload → 10⁴ OOM) — the
                        // same routing brick 3's construct uses.
                        let value_tuple = tys.len() == 2
                            && crate::lower::is_value_ty(&tys[0])
                            && !is_heap_ty(&tys[1]);
                        let heap_slots: Vec<usize> =
                            (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                        if value_tuple {
                            self.variant_drop_handles.insert(subj, "value_tuple".to_string());
                            self.materialized_aggregates.insert(subj);
                        } else if !heap_slots.is_empty() {
                            self.record_masks.insert(subj, heap_slots);
                            self.materialized_aggregates.insert(subj);
                        }
                    }
                }
                if self.try_lower_tuple_destructure(elements, subj) {
                    return Ok(());
                }
            }
        }
        // PRECISE record field extraction (`let { x, y } = p`) — the record sibling of the tuple
        // path above. Load each field from its OWN layout slot instead of the container-grain alias
        // (`bind_pattern` bound every field to the record pointer → `i64.add` on two ptrs / NUL
        // Strings). A CALL-RESULT record (`let { … } = mk()`) is seeded as a masked aggregate first
        // (so heap fields borrow + the scope-end drop frees them), exactly like the tuple seed.
        if let IrPattern::RecordPattern { fields, .. } = pattern {
            if let Some(subj) = subject {
                if !self.materialized_aggregates.contains(&subj)
                    && self.live_heap_handles.contains(&subj)
                {
                    if let Some((_, tys)) = self.aggregate_field_tys(&value.ty) {
                        let heap_slots: Vec<usize> =
                            (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                        if !heap_slots.is_empty() {
                            self.record_masks.insert(subj, heap_slots);
                        }
                        self.materialized_aggregates.insert(subj);
                    }
                }
                if self.try_lower_record_destructure(fields, &value.ty, subj) {
                    return Ok(());
                }
            }
        }
        self.bind_pattern(pattern, subject)
    }
}
