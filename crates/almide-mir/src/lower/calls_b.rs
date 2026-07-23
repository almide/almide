impl LowerCtx {

    /// The TWO-LEVEL record+field COW (#794): before an in-place mutator writes through
    /// `r.f`, make BOTH levels uniquely owned so no alias observes the write.
    ///
    /// Level 1 — the RECORD: an UNCONDITIONAL spread-copy (the proven
    /// `try_lower_spread_record_construct` discipline): a fresh block, each scalar slot
    /// value-copied, each heap slot `Dup`'d (CO-OWNED — cert `a`) then moved in (cert
    /// `m`). The var rebinds to the copy; the OLD block's owned reference is released
    /// NOW by its type-routed drop (masked/recursive — at rc>1 it only decs, the alias
    /// keeps its block; at rc=1 it frees the old block and each Dup'd child drops back
    /// to one owner: the copy). Unconditional-copy is value-semantics-exact — an
    /// unshared receiver pays a copy but observes nothing. NOTE `Op::MakeUnique` CANNOT
    /// serve level 1: its `$list_copy` is a raw slot copy that aliases the children
    /// WITHOUT co-owning them (sound only for flat blocks — the bare-var bytes/list
    /// receivers it ships on).
    ///
    /// Level 2 — the FIELD: load the (possibly shared) field handle, `Op::MakeUnique`
    /// it (flat Bytes/list block — exactly the raw-copy shape $list_copy handles), and
    /// store the unique handle back into the record's slot. The mutator's own receiver
    /// arg then borrows the slot and writes the uniquely-owned block.
    ///
    /// Returns None (nothing emitted — the ops are appended only after every gate
    /// passes) when the receiver is not a LOCAL var bound to a materialized aggregate
    /// with a resolvable layout — the caller walls, unchanged.
    fn two_level_field_cow(
        &mut self,
        object: &IrExpr,
        field: almide_lang::intern::Sym,
    ) -> Option<()> {
        use crate::{Init, PrimKind};
        let IrExprKind::Var { id } = &object.kind else { return None };
        let old = self.value_for(*id).ok()?;
        if self.param_values.contains(&old) || !self.materialized_aggregates.contains(&old) {
            return None;
        }
        let (names, tys) = self.aggregate_field_tys(&object.ty)?;
        let fidx = names.iter().position(|n| n.as_str() == field.as_str())?;
        if !is_heap_ty(&tys[fidx]) {
            return None; // a scalar field is not an in-place heap receiver
        }
        // Level 1: the record spread-copy.
        let n = tys.len();
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let new = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: new,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len },
        });
        let old_h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(old_h), args: vec![old] });
        let new_h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(new_h), args: vec![new] });
        for (i, fty) in tys.iter().enumerate() {
            let off = crate::lower::layout::slot_offset(i) as i64;
            let src_addr = self.addr_at(old_h, off);
            let dst_addr = self.addr_at(new_h, off);
            if is_heap_ty(fty) {
                let child = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::LoadHandle,
                    dst: Some(child),
                    args: vec![src_addr],
                });
                let owned = self.fresh_value();
                self.ops.push(Op::Dup { dst: owned, src: child });
                let handle = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::Handle,
                    dst: Some(handle),
                    args: vec![owned],
                });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![dst_addr, handle],
                });
                self.ops.push(Op::Consume { v: owned });
            } else {
                let val = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::Load { width: 8 },
                    dst: Some(val),
                    args: vec![src_addr],
                });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![dst_addr, val],
                });
            }
        }
        // Rebind the var to the copy; transfer the read-shape/drop tracking; release the
        // old block's owned reference by its type route (masked/recursive).
        self.value_of.insert(*id, new);
        self.materialized_aggregates.insert(new);
        if let Some(mask) = self.record_masks.get(&old).cloned() {
            self.record_masks.insert(new, mask);
        }
        if let Some(route) = self.variant_drop_handles.get(&old).cloned() {
            self.variant_drop_handles.insert(new, route);
        }
        if self.heap_elem_lists.contains(&old) {
            self.heap_elem_lists.insert(new);
        }
        let old_drop = self.drop_op_for(old);
        self.ops.push(old_drop);
        self.live_heap_handles.retain(|h| *h != old);
        self.live_heap_handles.push(new);
        // Level 2: the FIELD COW — load the (possibly shared) child handle, make it
        // unique (flat raw copy), store it back into the copied record's slot.
        let foff = crate::lower::layout::slot_offset(fidx) as i64;
        let faddr = self.addr_at(new_h, foff);
        let buf = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(buf), args: vec![faddr] });
        self.ops.push(Op::MakeUnique { v: buf });
        let faddr2 = self.addr_at(new_h, foff);
        let bh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(bh), args: vec![buf] });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 8 },
            dst: None,
            args: vec![faddr2, bh],
        });
        Some(())
    }

    /// `base + off` as a fresh address value (a ConstInt + IntBinOp Add pair).
    fn addr_at(&mut self, base: ValueId, off: i64) -> ValueId {
        let o = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: o, value: off });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: base, b: o });
        addr
    }

    /// Make the CALLS hidden inside a value whose CONTENT is deferred to
    /// `Init::Opaque` / `Const` VISIBLE to the transitive capability fold. An
    /// Opaque/Const value lowers NONE of its sub-expressions, so a call buried in a
    /// list element, constructor payload, operand, or scalar value (`[f()]`,
    /// `Some(g(x))`, `a ++ h()`, `var n = list.len(xs)`) vanishes from the MIR —
    /// invisible to the caps fold over `Op::CallFn` edges, forcing the corpus gate
    /// to conservatively TAINT the whole function. This appends a bare EFFECT MARKER
    /// `Op::CallFn { dst: None, args: [], result: None }` per such call: the
    /// existing handlers already treat a result-less, dst-less call as a PURE EFFECT
    /// — `ownership_certificate` emits no event (no `+1`/drop), `name_witness`
    /// references nothing (no dangling ref), the `+1`-backing gate ignores it — yet
    /// `reachable_caps_or_tainted` matches it by NAME and folds the callee
    /// transitively. So the EFFECT becomes analyzable while the value CONTENT stays
    /// deferred: the same Opaque deferral, now extended to the capability axis.
    ///
    /// Only calls whose capabilities the fold models SOUNDLY are recorded: a
    /// first-order `Named` call (the fold opens an in-profile callee or honestly
    /// taints an unknown one) and a first-order PURE `Module` call (a dotted name
    /// the gate treats as Stdout-free — sound because it IS pure). A higher-order
    /// call (unmodelled closure caps), an effectful/impure `Module` call (its dotted
    /// name would be WRONGLY treated as free), and a `Method`/`Computed` target are
    /// SKIPPED — left elided, so the `ir_calls > mir_calls` gate keeps the function
    /// tainted (no FALSE de-taint). This never errors and never walls — it only adds
    /// effect markers, so it can never turn an in-profile function `Unsupported`.
    ///
    /// SOUNDNESS BACKSTOP: a marker is recorded ONLY at a wholesale-elided position
    /// (the caller emits one `Opaque`/`Const` op for the whole `value`, lowering
    /// none of its sub-calls), so the MIR call-op count can only rise TOWARD the
    /// IR's, never past it. The corpus gate asserts `mir_calls <= ir_calls` — a
    /// double-count (the one way a marker could mask a real elision and FALSELY
    /// de-taint a function) then fails the gate, structurally impossible to ship.
    pub(crate) fn record_elided_calls(&mut self, value: &IrExpr) {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct Collector<'a> {
            names: Vec<String>,
            registry: &'a crate::lower::RecordLayouts,
        }
        impl IrVisitor for Collector<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    IrExprKind::Call { target, args, .. } => {
                        if !is_higher_order(args) {
                            match target {
                                CallTarget::Named { name } => {
                                    self.names.push(name.as_str().to_string())
                                }
                                CallTarget::Module { module, func, .. }
                                    if purity::is_pure(module.as_str(), func.as_str()) =>
                                {
                                    self.names
                                        .push(format!("{}.{}", module.as_str(), func.as_str()))
                                }
                                _ => {}
                            }
                        }
                    }
                    // A string `+` OPERATOR (`BinOp::ConcatStr`) lowers, where reachable,
                    // to a real `__str_concat` CallFn (`try_lower_concat_str`); in a
                    // DEFERRED position — a heap-result match/if arm tail, an Opaque
                    // call/branch — it is elided exactly like a call. Surface it as an
                    // elided `__str_concat` marker so the caps gate's `mir_calls` matches
                    // the `ir_calls` ConcatStr count (else the enclosing function falsely
                    // taints caps-unverified — `ir_calls > mir_calls`). SOUND: `__str_concat`
                    // is pure (empty capability witness — an `Op::CallFn` contributes zero
                    // caps), and the marker carries NO value (`dst: None`, no leak). The
                    // marker maps 1:1 to the counted ConcatStr node, so `mir_calls <=
                    // ir_calls` is preserved.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        self.names.push("__str_concat".to_string());
                    }
                    // A SCALAR-element list `+` OPERATOR (`BinOp::ConcatList` over List[Int/Float/Bool])
                    // lowers, where reachable, to a real `__list_concat` CallFn; in a DEFERRED position
                    // (a statement reassignment `c = c + [10]`, an Opaque branch/arg) it is elided like
                    // a call. Surface a `__list_concat` marker so the caps gate's `mir_calls` matches the
                    // `ir_calls` ConcatList count (the gate counts the SAME scalar-element shape). SOUND:
                    // `__list_concat` is pure (prim memory ops, empty capability witness), the marker
                    // carries no value (`dst: None`). A HEAP-element list concat is NOT counted by the
                    // gate and emits NO marker here (the `is_heap_ty` element guard mirrors the count).
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                        use almide_lang::types::constructor::TypeConstructorId;
                        let scalar_elem = matches!(&e.ty,
                            Ty::Applied(TypeConstructorId::List, a)
                                if a.len() == 1 && !crate::lower::is_heap_ty(&a[0]));
                        if scalar_elem {
                            self.names.push("__list_concat".to_string());
                        }
                    }
                    // A STRING INTERPOLATION in a DEFERRED position — a heap-result match/if
                    // arm where the WHOLE branch fell back to Opaque, or any Opaque value/arg.
                    // `count_ir_calls` credits a desugarable interp the call NODES of its
                    // desugared tree REGARDLESS of position (the gate's visitor walks every
                    // subtree); when the interp does NOT get folded by `try_lower_string_interp`
                    // (its enclosing branch is Opaque), surface the SAME synthetic calls as
                    // elided markers so `mir_calls` keeps pace with `ir_calls` (else the function
                    // falsely taints — the −32 caps regression). Every synthetic callee
                    // (`__str_concat`, `<module>.to_string`) is pure (no Stdout), so the markers
                    // add no capability; a NON-desugarable interp is credited 0 and emits 0
                    // markers here. The SYNTHETIC names are the ConcatStr + to_string wrappers
                    // ONLY — the operands' OWN calls (a `${g(x)}` callee) are reached by the
                    // `walk_expr` below over the ORIGINAL parts, so there is no double-count.
                    IrExprKind::StringInterp { parts } => {
                        for name in crate::lower::interp_synthetic_call_names(parts, self.registry) {
                            self.names.push(name);
                        }
                    }
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        let names = {
            let mut c = Collector { names: Vec::new(), registry: &self.record_layouts };
            c.visit_expr(value);
            c.names
        };
        for name in names {
            self.ops.push(Op::CallFn { dst: None, name, args: Vec::new(), result: None });
        }
    }

    /// The heap-typed STATEMENT-position call-result drop-route classification for
    /// [`Self::lower_effect_call`] — routes `dst`'s scope-end drop based on its static type
    /// `ty`. Verbatim extraction (guard-clause flattening) of the former inline else-if
    /// chain, no behavior change — see docs/roadmap/active/code-health-codopsy.md.
    fn classify_stmt_call_heap_drop(&mut self, dst: ValueId, ty: &Ty) {
        if crate::lower::is_result_listval_ty(ty) {
            self.value_result_lists.insert(dst);
            return;
        }
        if crate::lower::is_value_result_ty(ty) {
            self.value_result_results.insert(dst);
            return;
        }
        if crate::lower::is_lenlist_list_ty(ty) {
            self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
            return;
        }
        if crate::lower::is_heap_elem_list_ty(ty) {
            self.heap_elem_lists.insert(dst);
        }
    }

    /// Lower an EFFECT call (a Unit-typed `Call`) to a runtime [`Op::Call`].
    /// Today the recognized set is `println(s)` for a heap string → [`RtFn::PrintStr`],
    /// which BORROWS the string handle (no refcount change; the value stays live
    /// and is dropped at scope end) and reaches [`crate::Capability::Stdout`] (so a
    /// real printing program's capability witness is derived from real source).
    /// Anything outside the set is an explicit `Unsupported` (totality).
    pub(crate) fn lower_effect_call(&mut self, call: &IrExpr) -> Result<(), LowerError> {
        // An effect-fn call in STATEMENT position carries the auto-`?` of effect-Result
        // propagation: `g()` where `g` returns `Result[Unit, _]` is lowered by the
        // frontend as `Try { g() }` (or `Unwrap` for an explicit `g()!`). In statement
        // position the Result is DISCARDED (Unit), so there is no value to compute wrong —
        // the call simply runs for effect, and Err-propagation is the same loop-completion
        // model the heap-`Unwrap` tail already relies on (see `lower_tail`). Strip the
        // wrapper and lower the inner call. (A value-position `Unwrap` is still walled —
        // there the unwrapped value is load-bearing; here it is thrown away.)
        if let IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } = &call.kind {
            return self.lower_effect_call(expr);
        }
        // A primitive-floor STATEMENT (`prim.store32(...)` / a discarded `prim.*`):
        // `@intrinsic` lowers it to a `RuntimeCall`; map the `almide_rt_prim_*` symbol
        // to an `Op::Prim` (a store is Unit, so the dst is None — nothing to discard).
        if let IrExprKind::RuntimeCall { symbol, args } = &call.kind {
            if let Some(func) = symbol.as_str().strip_prefix("almide_rt_prim_") {
                self.lower_prim_call(func, args)?;
                return Ok(());
            }
        }
        let (target, args) = match &call.kind {
            IrExprKind::Call { target, args, .. } => (target, args),
            other => {
                return Err(LowerError::Unsupported(format!(
                    "effect statement {} is not a call",
                    kind_name(other)
                )))
            }
        };
        let name = match target {
            CallTarget::Named { name } => name.as_str(),
            // A pure Module COMBINATOR applied for side effects (`list.each(xs, f)`):
            // the effect is the CLOSURE's. Capture the closure's capabilities, borrow
            // the regular args, and emit the Unit-result call — exactly the value-
            // position higher-order handling, minus the result. An effectful/impure
            // Module call reaches a host capability of its OWN that the model cannot
            // yet name, so it stays walled (`purity::is_pure` gates inside).
            CallTarget::Module { module, func, .. } => {
                return self.lower_effect_module_call(module.as_str(), func.as_str(), args, &call.ty)
            }
            CallTarget::Method { method, .. } => {
                return Err(LowerError::Unsupported(format!(
                    "effect Method call .{} (unresolved dispatch) not in this brick",
                    method.as_str()
                )))
            }
            // A Computed effect call `(g)()` — the callee is a closure VALUE we cannot
            // name. DEFER it exactly like a Computed VALUE call: the callee's and args'
            // analyzable sub-calls are captured (`record_elided_calls`), the Computed
            // call itself is ELIDED (no nameable `CallFn`). Since `count_ir_calls` counts
            // the Computed `Call` node but the lowering emits no marker for it,
            // `ir_calls > mir_calls` TAINTS the function caps-unverified — honest (the
            // closure's invocation capabilities are unknown), never falsely caps-verified.
            // A discarded HEAP result is a fresh `Alloc{Opaque}` dropped at scope end;
            // a Unit/scalar result carries no ownership.
            CallTarget::Computed { callee } => {
                // C1 UNIT DIRECT-CALL INLINE — the statement-position twin of
                // `try_inline_direct_lambda_call`: `let inc = () => { count = count + 1 };
                // inc()` (the escape_analysis counter shape). The body's statements lower
                // AT THE CALL SITE — a MUTABLE capture is an ordinary in-scope Assign, so
                // no closure object and no lift is needed. Zero-param calls only in this
                // brick, and the body must not re-enter the same callee (a recursive
                // lambda would inline forever); failure rolls back to the paths below.
                // Guard-clause flattening of the former 5-deep nested-if (no `else` anywhere:
                // any unmet condition falls through to the code after this block, unchanged —
                // `break` exits the labeled block and resumes there, exactly as the original
                // fell out of the if-pyramid). No behavior change — see
                // docs/roadmap/active/code-health-codopsy.md.
                'inline_lambda_call: {
                    if !args.is_empty() {
                        break 'inline_lambda_call;
                    }
                    let IrExprKind::Var { id } = &callee.kind else {
                        break 'inline_lambda_call;
                    };
                    let id = *id;
                    let Some((params, body)) = self.lambda_bindings.get(&id).cloned() else {
                        break 'inline_lambda_call;
                    };
                    let recurses = {
                        struct R {
                            id: almide_ir::VarId,
                            found: bool,
                        }
                        impl almide_ir::visit::IrVisitor for R {
                            fn visit_expr(&mut self, e: &IrExpr) {
                                if matches!(&e.kind, IrExprKind::Var { id } if *id == self.id) {
                                    self.found = true;
                                }
                                almide_ir::visit::walk_expr(self, e);
                            }
                        }
                        let mut r = R { id, found: false };
                        almide_ir::visit::IrVisitor::visit_expr(&mut r, &body);
                        r.found
                    };
                    if params.is_empty() && !recurses {
                        let ops_mark = self.ops.len();
                        let lhh_mark = self.live_heap_handles.len();
                        let stmt = almide_ir::IrStmt {
                            kind: almide_ir::IrStmtKind::Expr { expr: body },
                            span: None,
                        };
                        match self.lower_stmt(&stmt) {
                            Ok(()) => return Ok(()),
                            Err(e) => {
                                if std::env::var("ALMIDE_DBG_ANF").is_ok() {
                                    eprintln!(
                                        "[c1-stmt-inline] body failed in {}: {e:?}",
                                        self.fn_name
                                    );
                                }
                            }
                        }
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                    }
                }
                // A Unit-result call THROUGH a lifted lambda value EXECUTES via CallIndirect
                // (e.g. `let f = (x) => print_it(x); f(3)`). Otherwise — a dynamic closure
                // value we cannot name — DEFER as before (calls captured, the Computed call
                // elided ⇒ honest caps taint).
                if let Some(blk) = self.closure_block_of_mut(callee) {
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Ok(lowered) = self.lower_call_args(args) {
                        // The CallIndirect's declared RESULT selects the wasm func TYPE
                        // (none/i64/i32 — render_wasm's sig classes), and the lifted
                        // lambda's own table type comes from its RETURN repr. A
                        // result-less dispatch to a VALUE-returning closure (`drain()`
                        // where `drain = () => { list.pop(xs) }` returns Option[Int])
                        // therefore declared the WRONG type — "indirect call type
                        // mismatch" at runtime — and leaked the returned block. Derive
                        // the result from the callee's Fn RETURN type: a discarded HEAP
                        // result is a fresh owned value dropped at scope end (its
                        // type-routed recursive drop registered); a discarded scalar
                        // binds an unused dst; Unit keeps the result-less dispatch.
                        let ret_ty = match &callee.ty {
                            Ty::Fn { ret, .. } => (**ret).clone(),
                            _ => Ty::Unit,
                        };
                        if matches!(ret_ty, Ty::Unit) {
                            self.emit_closure_call(blk, None, lowered, None);
                        } else {
                            let repr = repr_of(&ret_ty)?;
                            let dst = self.fresh_value();
                            self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
                            if is_heap_ty(&ret_ty) {
                                self.live_heap_handles.push(dst);
                                self.register_owned_heap_eq_drop(dst, &ret_ty);
                            }
                        }
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
                // STRICT value mode (the real render path — pipeline.rs sets it): eliding a
                // dynamic closure INVOCATION drops its side effects entirely (`run3(() => {
                // p = p + 10 })` printed p=0 — a silent wrong value, worse than the honest
                // caps taint the elision was designed around). REFUSE instead: the function
                // walls and `--verified` falls back to v0. The permissive caps-counting
                // classifier path keeps the elision (its only consumer is call accounting).
                if crate::lower::strict_values() {
                    return Err(LowerError::Unsupported(
                        "computed closure call outside the liftable subset cannot be \
                         faithfully executed (eliding it would drop the invocation's \
                         effects — a silently wrong value) not in this brick"
                            .into(),
                    ));
                }
                self.record_elided_calls(call);
                if is_heap_ty(&call.ty) {
                    let dst = self.fresh_value();
                    let repr = repr_of(&call.ty)?;
                    self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                    self.live_heap_handles.push(dst);
                }
                return Ok(());
            }
        };
        match (name, args.as_slice()) {
            // println(s) — the heap-string argument is BORROWED for a Stdout write.
            // A non-var arg (a literal `println("x")`, a concat `println(a ++ b)`,
            // an interpolation `println("${x}")`, or a call result `println(f())`)
            // is materialized into an owned temp by `lower_call_args` (the same
            // arg machinery as a normal call), then borrowed; the temp is dropped
            // at scope end. The Stdout effect makes the function caps-unverified
            // (it reaches Stdout, which `declared_caps` is empty for) — honest, not
            // claimed caps-safe.
            ("println", [arg]) if is_heap_ty(&arg.ty) => {
                let lowered = self.lower_call_args(std::slice::from_ref(arg))?;
                self.ops.push(Op::Call { dst: None, func: RtFn::PrintStr, args: lowered, result: None });
                Ok(())
            }
            // A USER function call (Unit result, e.g. `beep()`) → Op::CallFn. The
            // call BORROWS its heap-handle args (no refcount change here). The
            // callee's capabilities are accounted for at the CALL SITE against
            // its signature (the per-call-site subset rule), so a program is
            // rejected for a capability a CALLEE reaches — transitively — even
            // with no direct effect (closes the direct-only caps gap).
            (callee, call_args) => {
                let lowered = self.lower_call_args(call_args)?;
                // A callee whose (post-never-err-rewrite) call type is HEAP returns a
                // real block — a DECLARED-Result effect fn in statement position
                // (`write_message(..)!`, porta) or a discarded heap value. A bare
                // void `(call $f)` left that block ON THE WASM STACK (invalid wasm:
                // "values remaining on stack") and leaked it. Receive it into an
                // owned temp dropped at scope end; the by-type drop classes match
                // the bind path. A genuinely void callee (Unit / a never-err LIFTED
                // effect fn, whose call type was already rewritten to raw Unit)
                // keeps the void call.
                if is_heap_ty(&call.ty) {
                    let dst = self.fresh_value();
                    let pr = repr_of(&call.ty)?;
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: callee.to_string(),
                        args: lowered,
                        result: Some(pr),
                    });
                    self.classify_stmt_call_heap_drop(dst, &call.ty);
                    self.live_heap_handles.push(dst);
                } else {
                    self.ops.push(Op::CallFn {
                        dst: None,
                        name: callee.to_string(),
                        args: lowered,
                        result: None,
                    });
                }
                Ok(())
            }
        }
    }
}

include!("calls_p2.rs");
include!("calls_p3.rs");
include!("calls_p4.rs");
include!("calls_p4_b.rs");
include!("calls_p4_c.rs");
