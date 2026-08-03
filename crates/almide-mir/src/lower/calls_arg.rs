// ── Call-argument dispatch groups ────────────────────────────────
//
// The four groups `lower_call_arg_into` routes between, split out of
// `calls_p2.rs` so neither half is oversized. `include!`d from there, so imports
// and the `ArgOutcome` type come from that file.

impl LowerCtx {
    /// Variables, literals, lambdas and string interpolation — the argument forms
    /// that materialize without lowering a sub-expression tree first.
    ///
    /// One group of `lower_call_arg_into`'s dispatch table, arms verbatim and in
    /// source order. `Ok(None)` means "not my group" — the router tries the groups
    /// in that order, so which rule an argument takes is unchanged, and an `Err`
    /// still aborts the whole call as before.
    /// A lambda ARGUMENT (`list.map(xs, (x) => x + n)`): LIFT it to a fresh
    /// `__lambda_*` function and pass its CLOSURE BLOCK (a borrowed heap
    /// handle — fnidx in slot 0, captures in slots 1…); the callee invokes it
    /// via `Op::CallIndirect`. A lambda outside the liftable subset is
    /// rejected (an empty deferred closure env would be a silent miscompile).
    fn lambda_call_arg(&mut self, params: &[(VarId, Ty)], body: &IrExpr) -> Result<CallArg, LowerError> {
        Ok(
    match self.lift_lambda(params, body) {
        Some(blk) => CallArg::Handle(blk),
        // A lambda OUTSIDE the liftable subset would materialize a
        // deferred `Init::Opaque` (an EMPTY closure env) and pass it to
        // the callee, which would invoke garbage = a SILENT MISCOMPILE.
        // Reject.
        None => {
            return Err(LowerError::Unsupported(
                "lambda outside the liftable subset in a call-argument \
                 position (would pass an empty deferred closure env)"
                    .into(),
            ))
        }
    })
    }

    /// A STRING INTERPOLATION argument over the executable subset — a fresh
    /// owned String via the __str_concat chain, borrowed into the call and
    /// dropped at scope end; a non-lowerable interp is rejected (the callee
    /// would read an empty deferred String). Comments verbatim.
    fn string_interp_call_arg(&mut self, parts: &[almide_ir::IrStringPart], ty: &Ty) -> Result<CallArg, LowerError> {
        let repr = repr_of(ty)?;
        Ok(
    match self.try_lower_string_interp(parts) {
        Some(dst) => self.materialized_call_arg(dst, repr, ty),
        // A non-lowerable interp as a call ARGUMENT would materialize a
        // deferred `Init::Opaque` (an EMPTY String) and BORROW it into the
        // call — the callee reads zero bytes = a SILENT MISCOMPILE. Reject
        // explicitly so the enclosing function walls cleanly instead of
        // emitting wrong output.
        None => {
            return Err(LowerError::Unsupported(
                "non-lowerable string interpolation in a call-argument position \
                 (would borrow an empty deferred String)"
                    .into(),
            ))
        }
    })
    }

    fn lower_call_arg_leaf(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<Option<ArgOutcome>, LowerError> {
        let _ = &out;
        Ok(Some(ArgOutcome::Value(match &a.kind {
            IrExprKind::Var { id } if matches!(a.ty, Ty::Fn { .. }) => {
                CallArg::Scalar(self.value_or_global(*id)?)
            }
            IrExprKind::Var { id } if is_heap_ty(&a.ty) => {
                let v = self.value_or_global(*id)?;
                // F2 pass-2 consumer gate (#790): a DEFERRED Opaque bind passed as a call
                // argument hands the callee an EMPTY block it reads executably (the same
                // class as the eq-operand and interp-bind holes). Strict mode REFUSES;
                // the permissive classifier keeps the borrow for call accounting.
                if crate::lower::strict_values() && self.deferred_opaque_binds.contains(&v) {
                    return Err(LowerError::Unsupported(
                        "deferred (Opaque) value passed as a call argument — the callee \
                         would read an empty block not in this brick"
                            .into(),
                    ));
                }
                CallArg::Handle(v)
            }
            IrExprKind::Var { id } => CallArg::Scalar(self.value_or_global(*id)?),
            IrExprKind::LitInt { value } => CallArg::Imm(*value),
            // A lambda ARGUMENT (`list.map(xs, (x) => x + n)`): LIFT it to a fresh
            // `__lambda_*` function and pass its CLOSURE BLOCK (a borrowed heap
            // handle — fnidx in slot 0, captures in slots 1…) — the callee invokes
            // it via `Op::CallIndirect` through its function-typed param. This is
            // the call-site half of higher-order self-host (`list.map`/`filter`/
            // `fold`), capturing lambdas included (scalar captures).
            IrExprKind::Lambda { params, body, .. } => self.lambda_call_arg(params, body)?,
            // A STRING INTERPOLATION argument (`println("x=${n}")`, `f("hi ${s}")`)
            // over the executable subset — lowered to a fresh owned String via the
            // __str_concat chain, borrowed into the call and dropped at scope end
            // (cert `i` + `d`, identical to a materialized heap-literal arg). A
            // compound/call-operand interp returns None and falls through to the
            // deferred `Alloc{Opaque}` below (its inner calls recorded as elided),
            // unchanged. (This is the highest-traffic interp position — every
            // `println("…${x}…")` real program uses it.)
            IrExprKind::StringInterp { parts } => self.string_interp_call_arg(parts, &a.ty)?,
            // An Option/Result CONSTRUCTOR argument (`f(Some(8))`, `g(Ok(y))`,
            // `h(Err("e"))`, `k(None)`) materializes a REAL tagged block via
            // `try_lower_option_ctor` — the SAME `OptSome`/`OptNone`/DynListStr-Result
            // blocks a `let o = Some(8)` builds (len-as-tag, scalar payload moved in /
            // owned heap Err) — borrowed into the call and dropped at scope end via
            // `materialized_call_arg`: cert `i` (alloc) + `d` (drop), identical to the
            // verified fresh-heap bind. Outside that subset (a heap payload it declines,
            // e.g. a borrowed-param `Some(p)`) it WALLs — never the `Init::Opaque` empty
            // value the grouped arm below would build (which a callee reads as zero
            // bytes = a silent miscompile).
            IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_option_ctor(a, &a.ty) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(format!(
                            "{} argument cannot be faithfully materialized in this brick \
                             (a heap payload outside the executable subset)",
                            kind_name(&a.kind)
                        )))
                    }
                }
            }
            // A RECORD literal argument (`f(P { x: 3, y: 4 })`) materializes the real
            // layout block via `try_lower_record_construct` (the SAME block a `let p =
            // P{..}` builds — scalar fields stored, heap fields moved in), borrowed into
            // the call and dropped at scope end via `materialized_call_arg`: cert `i`
            // (alloc) + `d` (drop), identical to the verified fresh-heap bind. Outside the
            // subset (a heap-returning-call field) it WALLs — never an `Init::Opaque` empty.
            IrExprKind::Record { .. } => {
                let repr = repr_of(&a.ty)?;
                // heap-field records via `try_lower_record_construct`; all-scalar-field
                // records (`Point { x, y }`) via `try_lower_scalar_record_construct`.
                match self
                    .try_lower_record_construct(a)
                    .or_else(|| self.try_lower_scalar_record_construct(a))
                {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "record argument cannot be faithfully materialized in this \
                             brick (a field outside the executable subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A SPREAD-record argument (`upd({ ...opts, entry: next }, …)` — the dominant
            // porta recursive-parser shape `parse_options(args, idx+2, {...opts, field: v})`):
            // materialize the fresh same-layout block via `try_lower_spread_record_construct`
            // (each non-overridden field copied from the materialized base — a scalar Load / a
            // borrowed-handle Dup — and the overrides stored), then BORROW it into the call +
            // drop at scope end via `materialized_call_arg` (which seeds its heap-slot
            // `record_masks` + recursive `$__drop_<R>`): cert `i` (alloc + per-field moves) + `d`
            // (recursive drop), identical to the verified `let r = {...base, …}; f(r)` bind. The
            // SAME producer + drop wiring the Record arm uses — only the base-copy differs.
            // Outside the subset (a non-materialized base, an override field outside the
            // executable subset) it returns None → WALL (never an `Init::Opaque` empty record).
            IrExprKind::SpreadRecord { .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_spread_record_construct(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "spread-record argument cannot be faithfully materialized in this \
                             brick (a non-materialized base or a field outside the subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A fresh HEAP literal argument (`f("x")`, `f([1, 2, 3])`):
            // materialized into an owned temp via `Alloc`, borrowed into the
            // call, dropped at scope end — cert `i` (alloc) + `d` (drop), both
            // backed, identical to the verified fresh-heap bind.
            // A TUPLE argument (`f((slice, pos))`): the same masked nested-ownership block as a
            // record arg — heap elements via `try_lower_tuple_construct` (each `lower_owned_heap_field`
            // moved in), all-scalar via `try_lower_scalar_tuple_construct`, borrowed into the call +
            // dropped at scope end. `materialized_call_arg` already seeds the Tuple's `record_masks`
            // + recursive `$__drop_<R>` (calls_p4.rs), so the leaf fields then the block are freed —
            // no leak, no double-free. An unlowerable element returns `None` and WALLs (never Opaque).
            _ => return Ok(None),
        })))
    }

    /// Tuples, records, spreads and the branching forms whose value is an owned
    /// block built from their parts.
    ///
    /// One group of `lower_call_arg_into`'s dispatch table, arms verbatim and in
    /// source order. `Ok(None)` means "not my group" — the router tries the groups
    /// in that order, so which rule an argument takes is unchanged, and an `Err`
    /// still aborts the whole call as before.
    /// A heap-result `if` operand in call-argument position — materialize
    /// via the proven heap-result-if path, then borrow into the call through
    /// `materialized_call_arg` (tracking + destructure-mask seeding, B52).
    /// Comments verbatim from the former inline arm.
    fn heap_if_call_arg(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        ty: &Ty,
    ) -> Result<CallArg, LowerError> {
        Ok(    match self.try_lower_heap_result_if(cond, then, else_, ty) {
        // Route through `materialized_call_arg` (the sibling `Match` arm's own
        // B52 fix) rather than a bare `CallArg::Handle`: it tracks `dst` in
        // `live_heap_handles` AND, for a Tuple/Record `a.ty`, seeds the
        // precise-destructure masks `lower_destructure` (binds_p2.rs) needs —
        // without it, `let (a,b) = if c then (..) else (..)` materialized the
        // tuple fine but its scalar-component destructure fell to the generic
        // container-grain fallback (STRICT mode's `Const-0` wall). Verified
        // SAFE for the pre-existing plain-value case too (`println(if c then
        // "x" else "y")` in a 10,000× loop, 4MB cap, no leak/double-free before
        // OR after this change — `live_heap_handles` is the SAME scope-end-drop
        // list either path already respects, so adding the tracking here closes
        // a genuine latent gap rather than introducing a double-free).
        Some(dst) => {
            let repr = repr_of(ty)?;
            self.materialized_call_arg(dst, repr, ty)
        }
        None => {
            return Err(LowerError::Unsupported(
                "heap-result `if` in a call-argument position outside the executable \
                 subset"
                    .into(),
            ))
        }
    })
    }

    /// A heap-result `match` operand in call-argument position — desugared
    /// to the equivalent `if` chain via the proven `desugar_match_to_if`,
    /// hoisted `let`s lowered first, then the heap-result-if call-arg path.
    /// Comments verbatim from the former inline arm.
    fn heap_match_call_arg(
        &mut self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
        ty: &Ty,
    ) -> Result<CallArg, LowerError> {
        let value = {
            // `desugar_match_to_if` wraps its result in a `Block` (hoisted `let`
            // bindings PRECEDING the `If`) whenever the subject isn't one of the
            // freely-substitutable KINDS `build_match_chain`'s `subject_pure` admits
            // (`Var`/`LitInt`/`LitBool`/`LitFloat` — a `LitStr` subject, e.g. a
            // single-use `let x = "hello"; match x {...}` after an EARLIER inlining
            // pass propagates `x`'s literal value into the subject position, is NOT
            // in that list, so it takes the conservative `bind_subject` path instead
            // of inline substitution). This site only ever pattern-matched a BARE
            // `If`, declining outright on the Block-wrapped form — closing the ENTIRE
            // "match value in a call-argument position" class for any subject shape
            // needing the hoist, not just the LitStr case (`match arms returning
            // tuples`'s `let (label, len) = match x {s if .. => (..), s => (..)}`).
            // Lower the hoisted `let`s first (their own scope-end drops apply
            // normally), THEN unwrap to the inner `If` and proceed exactly as before.
            let lifted = self.desugar_match_to_if(subject, arms, ty).and_then(|e| {
                let (stmts, if_expr) = match e.kind {
                    IrExprKind::If { .. } => (Vec::new(), e),
                    IrExprKind::Block { stmts, expr: Some(tail) } => (stmts, *tail),
                    _ => return None,
                };
                let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return None };
                for s in &stmts {
                    self.lower_stmt(s).ok()?;
                }
                self.try_lower_heap_result_if(cond, then, else_, ty)
            });
            match lifted {
                // Route through `materialized_call_arg` (not a bare `CallArg::Handle`):
                // it tracks `dst` in `live_heap_handles` AND, for a Tuple/Record `a.ty`,
                // seeds `record_masks`/`variant_drop_handles` from `aggregate_field_tys`
                // — the SAME seeding `lower_destructure`'s OWN precise-tuple-extraction
                // path (binds_p2.rs) needs to find already done (it only seeds when
                // `live_heap_handles.contains(&subj)`, which a bare `CallArg::Handle`
                // never satisfies). WITHOUT this, `let (label, len) = match x {...}`
                // materialized the tuple fine but its DESTRUCTURE fell to the generic
                // container-grain `bind_pattern` fallback, which WALLS a scalar
                // component in STRICT mode (a Const-0 would silently corrupt `len`).
                Some(dst) => {
                    let repr = repr_of(ty)?;
                    self.materialized_call_arg(dst, repr, ty)
                }
                None => {
                    return Err(LowerError::Unsupported(
                        "heap-result `match` in a call-argument position outside the \
                         executable subset"
                            .into(),
                    ))
                }
            }
        };
        Ok(value)
    }

    fn lower_call_arg_aggregate(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<Option<ArgOutcome>, LowerError> {
        let _ = &out;
        Ok(Some(ArgOutcome::Value(match &a.kind {
            IrExprKind::Tuple { elements } => {
                let repr = repr_of(&a.ty)?;
                match self
                    .try_lower_tuple_construct(elements)
                    .or_else(|| self.try_lower_scalar_tuple_construct(elements))
                {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "Tuple argument cannot be faithfully materialized in this brick \
                             (an element outside the executable subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A heap-result `if` operand (`"a=" + (if c then "x" else "y")` — the StringInterp
            // `${if …}` desugar; `f(if c then a else b)`): materialize it via try_lower_heap_result_if
            // into an OWNED+TRACKED value (cert `i`, scope-end `d`) — EXACTLY the let-bound heap-if
            // path (`let s = if …; f(s)` already lowers) — then BORROW it into the call
            // (`CallArg::Handle`). Without this a heap-result-`if` operand fell to the deferred Opaque
            // arm below (rejected → the function walled). Closes the StringInterp-with-`${if}` wall
            // (porta `format_tool_log`) + any call/concat with a heap-result-`if` arg.
            IrExprKind::If { cond, then, else_ } if is_heap_ty(&a.ty) => {
                self.heap_if_call_arg(cond, then, else_, &a.ty)?
            }
            // A heap-result `match` operand (`let (label, len) = match x { s if .. => (..), s
            // => (..) }` — a tuple-pattern-let desugar that routes the match's VALUE through a
            // call argument): desugar the match to an equivalent `if`/`else if` chain via the
            // EXISTING, PROVEN `desugar_match_to_if` (the same transform tail/bind positions
            // already use), then lower it through the EXISTING, PROVEN heap-result-`if`
            // call-arg path directly above — no new lowering machinery, just wiring Match into
            // the If arm's already-working call-arg handling. Without this, EVERY heap-result
            // match operand fell straight to the generic wall below.
            IrExprKind::Match { subject, arms } if is_heap_ty(&a.ty) => {
                self.heap_match_call_arg(subject, arms, &a.ty)?
            }
            IrExprKind::LitStr { .. }
            | IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            // A CLOSURE value argument (`register((x) => …)`): a fresh heap env,
            // materialized + borrowed into the call. The callee borrows it per the
            // borrow-by-default convention; its body's calls are elided ⇒ the gate
            // taints the function caps-unverified (invocation caps unknown).
            // (A NON-CAPTURING `Lambda` arg is intercepted BELOW and lifted to a scalar
            // FuncRef slot passed by value — `list.map(xs, (x) => x + 1)`; only a
            // capturing one reaches this deferred Opaque arm.)
            | IrExprKind::ClosureCreate { .. } => {
                let repr = repr_of(&a.ty)?;
                // A NON-EMPTY `List[String]` (or scalar-aggregate-element) LITERAL arg
                // (`f(["a", "b"])`) materializes the REAL nested-ownership DynListStr via the
                // same builder the RETURN position uses (each element moved/Dup'd in), borrowed
                // into the call + dropped at scope end by DropListStr (cert `i` + recursive `d`).
                // Without this it fell to `alloc_init` → `Init::Opaque` empty list = rejected as
                // a silent miscompile below. (An empty/`List[Value]`/computed list still defers
                // to `alloc_init`, unchanged — the foundation for heap-element-list call args.)
                // A NON-EMPTY heap-element `List[String]`/aggregate literal → the nested-ownership
                // builder; a SCALAR-element `List[Int/Float/Bool]` with NON-literal elements
                // (`[pos]`, `[a, b]`) → the flat `DynList` + `store64` builder (a scalar list owns
                // no heap, so the scope-end drop is a flat `Drop`). Both yield a REAL populated
                // block, vs the `alloc_init` `Init::Opaque` that an all-literal-only path leaves
                // (rejected below). Closes `f([pos])` / the `acc + [pos]` append-accumulator element.
                // An EMPTY-map arg (`fold(xs, [:], …)` seed, `takes([:])` with ascription):
                // the SAME layout-agnostic 0-length block an empty-map BIND builds (a v1
                // Map is a paired-slot List; len 0 ⇒ the drop frees only the block) —
                // REAL (never Opaque), borrowed into the call + dropped at scope end.
                if matches!(&a.kind, IrExprKind::EmptyMap)
                    || matches!(&a.kind, IrExprKind::MapLiteral { entries } if entries.is_empty())
                {
                    if let Some(dst) = self.try_lower_scalar_list_slots(&[]) {
                        out.push(self.materialized_call_arg(dst, repr, &a.ty));
                        return Ok(Some(ArgOutcome::Pushed));
                    }
                }
                if matches!(&a.kind, IrExprKind::List { .. }) {
                    // A List[Record] literal arg (`group([rect(…), …])`): the record-list builder
                    // already pushes it to live_heap_handles + routes its drop to $__drop_list_<R>,
                    // so pass the handle directly (a second materialized_call_arg would double-track).
                    if let Some(dst) = self.try_lower_record_list_literal(a) {
                        out.push(CallArg::Handle(dst));
                        return Ok(Some(ArgOutcome::Pushed));
                    }
                    if let Some(dst) = self
                        .try_lower_str_list_literal(a)
                        .or_else(|| self.try_lower_scalar_list_construct(a))
                    {
                        out.push(self.materialized_call_arg(dst, repr, &a.ty));
                        return Ok(Some(ArgOutcome::Pushed));
                    }
                    // `f([a, b])` where a/b are TRACKED heap Vars with FLAT content
                    // (`list.flatten([first, second])` — the fft two-accumulator merge;
                    // `matrix.from_rows([r0, r1])`): materialize a fresh list CO-OWNING
                    // each element (Dup +1), dropped flat at scope end (per-slot rc_dec
                    // + block — a flat-content element's rc_dec IS its full free).
                    if let Some(dst) = self.try_lower_heap_var_list_literal(a) {
                        // A BORROW-VIEW list (slots are borrowed handles; the block-only
                        // plain Drop is already tracked inside the builder) — pass the
                        // handle directly, NO materialized_call_arg re-track.
                        out.push(CallArg::Handle(dst));
                        return Ok(Some(ArgOutcome::Pushed));
                    }
                }
                let init = alloc_init(a);
                // `alloc_init` faithfully materializes a string literal and a scalar-
                // literal list/tuple; every other constructor (Map/Record/Result/Option/
                // closure, a computed-element list) yields `Init::Opaque` — an EMPTY heap
                // value. Borrowing an empty value into the call lets the callee read zero
                // bytes = a SILENT MISCOMPILE, so reject the unfaithful case explicitly.
                if matches!(init, Init::Opaque) {
                    return Err(LowerError::Unsupported(format!(
                        "{} argument cannot be faithfully materialized in this brick \
                         (would borrow an empty deferred heap value)",
                        kind_name(&a.kind)
                    )));
                }
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc { dst, repr, init });
                self.record_elided_calls(a);
                self.materialized_call_arg(dst, repr, &a.ty)
            }
            // A Bool literal argument (`f(true)`): the real value 1/0 (the `if` cond
            // a callee branches on). `LitInt` is already an `Imm` above.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: if *value { 1 } else { 0 } });
                CallArg::Scalar(dst)
            }
            // A Float literal arg (`f(2.5)`): the i64-uniform value carries the f64 BITS
            // (the float-floor render reinterprets), so `2.5` materializes as ConstInt.
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: crate::lower::float_lit_bits(*value, &a.ty) });
                CallArg::Scalar(dst)
            }
            // `f(a + b)` — a string concat in a CALL-ARG position (also a NESTED `a + b + c`,
            // where `a + b` is the left operand arg). Lower it to the __str_concat call; its
            // fresh owned String is borrowed into the outer call and dropped at scope end.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_concat_str(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    // A non-lowerable string concat as a call ARGUMENT would borrow an
                    // empty deferred String into the callee = a SILENT MISCOMPILE. Reject.
                    None => {
                        return Err(LowerError::Unsupported(
                            "non-lowerable string concat in a call-argument position \
                             (would borrow an empty deferred String)"
                                .into(),
                        ))
                    }
                }
            }
            // `f(xs + [7])` — a SCALAR-element list concat in a CALL-ARG position. Lower it to
            // the __list_concat call; its fresh owned list is borrowed into the outer call and
            // dropped at scope end. A heap-element list concat (or a non-lowerable operand)
            // returns None and falls to the deferred Opaque.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_concat_list(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    // A non-lowerable list concat (heap-element / non-lowerable operand) as a
                    // call ARGUMENT would borrow an empty deferred list = a SILENT MISCOMPILE.
                    None => {
                        return Err(LowerError::Unsupported(
                            "non-lowerable list concat in a call-argument position \
                             (would borrow an empty deferred list)"
                                .into(),
                        ))
                    }
                }
            }
            // `f(opt ?? default)` — a `??` over a self-host materialized Option in a CALL-ARG
            // position (`int.to_string(list.get(xs, i) ?? 0)` / `println(list.get(ss, i) ?? "d")`
            // — extremely common). The let-bind path executes this via
            // `try_lower_option_unwrap_or`; the arg position must too, else the Option call
            // deferred to a bare elided-call marker (wrong arity → invalid wasm). A SCALAR result
            // passes by value; a HEAP-String result (`option.unwrap_or_str` — a fresh owned
            // String, tracked for scope-end drop by the helper) passes as a borrowed Handle. A
            // non-String-heap / non-Option operand returns None and defers below.
            _ => return Ok(None),
        })))
    }
}

include!("calls_arg_b.rs");
