impl LowerCtx {
    pub(crate) fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    /// A fresh SYNTHETIC temp VarId, allocated descending from `u32::MAX` so it can never collide
    /// with a frontend-assigned source VarId. Used to ANF-lift a Call-result whose heap field /
    /// element / tuple component is extracted directly (`f(x).field`): bind the call to this temp
    /// (materialized + tracked exactly like a source `let`), then extract from the temp.
    pub(crate) fn fresh_synth_var(&mut self) -> almide_ir::VarId {
        let id = almide_ir::VarId(u32::MAX - self.synth_var_count);
        self.synth_var_count += 1;
        id
    }

    /// Seed the parameters: each param's VarId maps to a fresh MIR value (so uses
    /// in the body resolve) and becomes a [`MirParam`] carrying its [`Repr`] (so
    /// the name-totality witness counts it as DEFINED — every param use must have
    /// a defining param). A HEAP param is BORROWED (the caller owns the reference
    /// — it contributes no owned `+1` to the ownership certificate; the cert and
    /// verifier guard on `repr.is_heap()`) and is recorded in `param_values` so a
    /// later move-out/mutation of a bare borrowed param is walled, not faked. A
    /// scalar param carries no ownership but is still a defined value.
    pub(crate) fn bind_params(&mut self, params: &[IrParam]) -> Result<Vec<MirParam>, LowerError> {
        let mut out = Vec::new();
        for p in params {
            let v = self.fresh_value();
            self.value_of.insert(p.var, v);
            // A FUNCTION-typed param (`f: (Int) -> Int`, the closures machinery) is a
            // CLOSURE BLOCK — the uniform heap representation: the caller passes the
            // block (borrowed, like every heap param) and it joins `closure_values` —
            // a `f(x)` call in the body then lowers to `Op::CallIndirect` through it
            // (fnidx from slot 0, the block forwarded as the callee's env; cap_witness
            // taints it conservatively, so a higher-order function stays honestly
            // caps-unverified). This is what lets `list.map`/`filter`/`fold` be
            // self-hosted in Almide.
            if matches!(p.ty, Ty::Fn { .. }) {
                self.closure_values.insert(v);
            }
            let repr = repr_of(&p.ty)?; // Ptr (heap) / Scalar; Unsupported if Unknown or non-value
            if repr.is_heap() {
                self.param_values.insert(v);
                // A heap variant param (`Option[T]` / `Result[T, String]`) is passed by the caller
                // as a REAL materialized block of the SAME layout the constructors build (the v1
                // calling convention — see `param_values` in `try_lower_option_unwrap_or`). SEED its
                // variant-tracking so a `match`/`??` over the PARAM inside the callee EXECUTES (reads
                // the real tag/payload) instead of LINEARIZING (running both arms = garbage). Without
                // this, `fn show(r: Result[Int,String]) = match r { Ok=>…, Err=>… }` ran both arms.
                // SOUND: a borrowed variant param owns nothing here (it stays `param_values`,
                // un-dropped — the caller owns it), so seeding it only changes how the match READS
                // the tag/payload (scalar prims, no ownership event), never the drop discipline.
                self.seed_variant_param(v, &p.ty);
            }
            out.push(MirParam { value: v, repr });
        }
        Ok(out)
    }

    /// Seed the variant-tracking sets for a heap `Option`/`Result` PARAM so a `match`/`??` over
    /// it executes (the caller passes a real same-layout block — the v1 calling convention). The
    /// classification MIRRORS the let-bind call-result tracking in `lower_bind` exactly:
    ///   - `Option[scalar]`        → `materialized_options`            (len-as-tag, scalar payload)
    ///   - `Option[heap]`          → `materialized_options` + `heap_elem_lists` (borrowed handle)
    ///   - `Result[scalar, heap]`  → `materialized_results`            (len-as-tag, scalar Ok)
    ///   - `Result[heap, heap]`    → `materialized_results_str` + `heap_elem_lists` (cap-as-tag)
    /// `param_values` already holds the borrowed handle (the caller owns it), so this adds only the
    /// READ-shape knowledge, no ownership change.
    fn seed_variant_param(&mut self, v: ValueId, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                self.materialized_options.insert(v);
                if is_heap_ty(&a[0]) {
                    self.heap_elem_lists.insert(v);
                }
            }
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => {
                if is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    // Both arms heap — the cap-as-tag 1-slot DynListStr. The DROP differs by Ok-arm:
                    // a `List[Value]` Ok (`value.as_array`) frees recursively (`value_result_lists`),
                    // else a String Ok (`value.as_string`) frees flat (`heap_elem_lists`).
                    self.materialized_results_str.insert(v);
                    if is_result_listval_ty(ty) {
                        self.value_result_lists.insert(v);
                    } else if is_value_result_ty(ty) {
                        self.value_result_results.insert(v);
                    } else {
                        self.heap_elem_lists.insert(v);
                    }
                } else {
                    // Scalar Ok (`Result[Int, String]`) — len-as-tag, scalar Ok payload. A heap Err
                    // payload is owned by the Result block (DropListStr frees it); mark the nested-
                    // ownership so an `Err(e)` arm binds the borrowed slot-0 handle.
                    self.materialized_results.insert(v);
                    if is_heap_ty(&a[1]) {
                        self.heap_elem_lists.insert(v);
                    }
                }
            }
            // A RECORD / TUPLE param (`fn f(r: R)`, `fn f(t: (Int, String))`, and the closure
            // params of a lifted lambda — `(r) => r.name` over a `List[R]`) is passed by the
            // caller as a REAL materialized block of the SAME uniform-slot layout the
            // constructors build (the v1 calling convention). SEED it as a materialized
            // aggregate so a `r.field` / `t.i` access inside the callee READS its real slot
            // (a scalar `Load`, a heap `LoadHandle` BORROW) instead of returning the empty
            // deferred value. Gated to a type the layout registry can RESOLVE (a registered
            // `Ty::Named` record or a structural `Ty::Record`/`Ty::Tuple`) — a String/List/
            // Map heap param is NOT an aggregate (`aggregate_field_tys` is `None`) so it is
            // never mis-seeded.
            //
            // SOUNDNESS: a record/tuple param is BORROWED (it stays in `param_values`,
            // un-dropped — the caller owns it). Seeding `materialized_aggregates` adds ONLY
            // the READ-shape knowledge (scalar/handle prim loads of its real slots), NEVER an
            // ownership event or a drop — exactly the variant-param reasoning above. A heap
            // FIELD read is a `LoadHandle` BORROW (recorded in `param_values`, not a second
            // owner), so the field's owner (the caller's block) frees it once — no leak / no
            // double-free.
            Ty::Record { .. } | Ty::Tuple(_) | Ty::Named(..)
                if self.aggregate_field_tys(ty).is_some() =>
            {
                self.materialized_aggregates.insert(v);
            }
            _ => {}
        }
    }

    /// Lower a function body (statements + tail + scope-end drops) into `self` —
    /// the shared core of `lower_function` (params pre-seeded) and `lower_body`.
    ///
    /// An expression-bodied function (`fn f() = expr`) is the SAME value-semantics
    /// subset as a block body — just an empty statement list whose tail IS the
    /// expression. The tail lowering walls anything outside the subset, so the
    /// wrapping never weakens the boundary (control-flow / unsupported tails still
    /// become an explicit `Unsupported`).
    pub(crate) fn lower_body_into(&mut self, body: &IrExpr) -> Result<Option<ValueId>, LowerError> {
        // TAIL-DUPLICATION desugar: a `let s = <heap-result if/match>; <rest>` (which `lower_bind`
        // walls — the merged-dst has no sound flat-cert scope-end drop) is rewritten PURELY in the
        // IR to push the continuation `<rest>` into each arm (`if c then { let s = A; <rest> } else
        // …`), turning the branch into the block TAIL. The rewritten body then lowers through the
        // ordinary statements+tail path — no special dispatch — so each branch independently binds +
        // drops its own `s` (the per-arm `i…d` balance the proven checker already accepts). The
        // SAME rewrite runs in the caps `count_ir_calls` gate ("desugar-before-both"), so the
        // duplicated calls stay 1:1 between MIR and IR by construction. `lower_tail`'s per-position
        // `if` machinery (Unit/scalar/heap) walls any unfaithful arm explicitly.
        // ANF-LIFT a heap-result `if`/`match` out of a call ARGUMENT first (`println(if c then
        // "a" else "b")` → `let tmp = if..; println(tmp)`), so the tail-duplication below then
        // recovers it. Same rewrite runs in the count gate (desugar-before-both).
        // EFFECT-MONAD desugar FIRST: a statement/let-bind effect-`!` (`let x = f()!; rest` / `f()!;
        // rest`) becomes a NESTED-MATCH continuation (`match f() { err(e) => err(e), ok(x) => { rest } }`)
        // — err-propagation WITHOUT a mid-function Return op. Re-enter so a later `!` in the continuation
        // also desugars, then desugar_heap_branches handles any heap-`if` continuations. Call-count-
        // invariant (no duplication), so `count_ir_calls` stays exact without re-running it.
        // GUARD-ELSE → conditional FIRST (Phase A): restructure `guard cond else E; rest`
        // into `if cond then { rest } else E` so the proven `if`/tail machinery runs the
        // early-return / loop-continue. Re-enter so the other desugars then process the
        // resulting `if`. Call-count-invariant (no duplication), so the caps gate stays exact.
        // METHOD/UFCS RESOLUTION FIRST (B-1): rewrite `obj.method(a)` (an unresolved
        // `CallTarget::Method`) to the concrete free fn it names — `p.encode()` →
        // `Person.encode(p)` — so the proven Named-call machinery lowers it. Must precede
        // the other desugars, which operate on resolved call structure. Call-count-invariant
        // (a Method Call and its resolved Named Call both count as one), so the caps gate stays
        // exact; the SAME step runs in `desugar_all` for the `count_ir_calls` side.
        if let Some(rewritten) = crate::lower::desugar_method_calls(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_guard(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_beta_reduce(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_tuple_unwrap_or(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = desugar_effect_unwrap(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = desugar_heap_branches(body) {
            return self.lower_body_into(&rewritten);
        }
        // DEBUG (env `DBG_LOWER_FN`): the FULLY-desugared body this function actually lowers — the
        // real lowering path (`desugar_heap_branches → TCO → here`), distinct from `desugar_all`.
        // Diff two functions' dumps to see why an identical `desugar_all` yields different MIR.
        if std::env::var("DBG_LOWER_FN").is_ok_and(|v| v == self.fn_name) {
            eprintln!(
                "=== LOWER-BODY {} ===\n{}",
                self.fn_name,
                crate::lower::dump_ir(body)
            );
        }
        // The set of vars reassigned INSIDE a loop (option-C slots) — gates the mutable
        // `var x = r.field` owned-field-`Dup` (a loop-reassigned such var would leak; see
        // `lower_heap_extraction`). Computed once over this (possibly tail-duplicated) body; a later
        // recompute over a rewritten body only adds (never removes) entries, so the gate stays sound.
        for v in crate::lower::loop_reassigned_vars(body) {
            self.loop_reassigned_vars.insert(v);
        }
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &body.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(body)),
        };
        for stmt in stmts {
            self.lower_stmt(stmt)?;
        }
        // The tail expression is the function's return value. A HEAP tail is MOVED
        // OUT to the caller (recorded as `ret`, not dropped at scope end); a scalar
        // tail carries no ownership; a Unit/absent tail is a Unit-returning body.
        let ret = self.lower_tail(tail)?;
        // Scope end: release every still-live heap handle (the moved-out return is
        // already removed). Aliases share a ValueId, so one Drop per HANDLE
        // balances the Alloc(+1) and each aliasing Dup(+1).
        self.emit_scope_end_drops();
        Ok(ret)
    }

    pub(crate) fn lower_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        // (The Try/Unwrap early-return-over-a-live-heap-local wall is LIFTED: the v0 wasm
        // codegen now frees the live heap locals before the Err-path `return_`
        // [emit_wasm: emit_early_return_decs], so the deferred-continue cert is faithful
        // on both targets — no leak. See docs/roadmap/active/v0-unwrap-early-return-leak.md.)
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, mutability } => {
                // A MUTABLE (`var`) binding may be COW-mutated later, so a heap-field
                // extraction (`var b = r.items`) must take an OWNED copy (container-grain
                // `Dup`), NOT a precise borrow (which cannot be mutated in place). Flag it so
                // `lower_heap_extraction` skips the borrow optimization for this bind.
                let prev = self.binding_is_mutable;
                let prev_var = self.binding_var;
                self.binding_is_mutable = matches!(mutability, almide_ir::Mutability::Var);
                self.binding_var = Some(*var);
                let r = self.lower_bind(*var, ty, value);
                self.binding_is_mutable = prev;
                self.binding_var = prev_var;
                r
            }
            // `x = value` — reassignment.
            //
            // At function TOP LEVEL: REBIND `x` to the new value (reusing
            // `lower_bind`). The OLD binding's handle stays in `live_heap_handles`
            // and is dropped at scope end — a conservative lifetime EXTENSION
            // (memory-safe, never a double-free: the old object is dropped exactly
            // once, at scope end, instead of at the reassignment). A read of the
            // old `x` inside `value` (e.g. `x = f(x)`) lowers BEFORE the rebind
            // overwrites `value_of[x]`, so it borrows the still-live old handle —
            // never a use-after-free.
            //
            // Inside a control-flow FRAME (`in_frame > 0`): a HEAP rebind would
            // repoint `value_of[x]` to a frame-local handle the per-iteration / per-arm
            // teardown drops, while `x` is read on the next iteration or after the
            // branch merges → UAF. So DEFER it — `x` keeps its still-live handle (the
            // loop/branch accumulator stays memory-safe), and the new value is carried
            // like every `Opaque`; capture its calls so the caps fold stays honest. A
            // SCALAR reassignment (`i = i + 1`) rebinds to a Copy `Const` with no handle
            // to dangle, so it is admitted unchanged (e.g. a loop counter).
            IrStmtKind::Assign { var, value } => {
                // Inside a scalar-marker loop, a reassignment mutates the var's STABLE
                // local (the loop-carried state) — `SetLocal`, not a fresh rebind. A heap
                // reassignment cannot run this way (the accumulator would need real heap
                // merge): ERROR to abort the attempt → `lower_while` falls back to its
                // sound model-one-iteration form.
                if self.scalar_loop_depth > 0 {
                    if is_heap_ty(&value.ty) {
                        // APPEND ACCUMULATOR (option C): `slot = slot + [x]` → alloc the new list, DROP
                        // the old slot, rebind the slot IN PLACE (`SetLocal`). The slot is an OWNED
                        // loop-carried list (initialized to an owned copy of the param before the loop by
                        // the TCO); each iteration drops the previous object + acquires the new one — the
                        // cert-`i(id)m` loop-carried slot PROVED leak/double-free-free for any iteration
                        // count (OwnershipChecker.v `check_line_unroll_sound`). Only a SELF-append
                        // (`Var(slot) + …`) qualifies; any other heap reassign still defers below.
                        if let IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatList,
                            left,
                            ..
                        } = &value.kind
                        {
                            if matches!(&left.kind, IrExprKind::Var { id } if id == var) {
                                if let Some(&slot_local) = self.value_of.get(var) {
                                    if let Some(new) = self.try_lower_concat_list(value) {
                                        let drop_op = self.drop_op_for(slot_local);
                                        self.ops.push(drop_op);
                                        self.ops
                                            .push(Op::SetLocal { local: slot_local, src: new });
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        // RESET to a fresh EMPTY heap value (`cur = []` / `acc = ""` — the parser
                        // resets the current-row accumulator after a delimiter): materialize the empty
                        // block, drop the old slot, rebind IN PLACE. Not a ConcatList (fast-path) nor
                        // a `lower_owned_heap_field` shape, so handle it here. Cert: drop-old (`d`) +
                        // alloc (`i`) = the same loop-carried `i(id)` the append slot proves.
                        if let Some(&slot_local) = self.value_of.get(var) {
                            let empty = match &value.kind {
                                IrExprKind::List { elements } if elements.is_empty() => Some(
                                    crate::Init::IntList(vec![]),
                                ),
                                IrExprKind::LitStr { value: s } if s.is_empty() => {
                                    Some(crate::Init::Str(String::new()))
                                }
                                _ => None,
                            };
                            if let Some(init) = empty {
                                let new = self.fresh_value();
                                self.ops.push(Op::Alloc {
                                    dst: new,
                                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                                    init,
                                });
                                let drop_op = self.drop_op_for(slot_local);
                                self.ops.push(drop_op);
                                self.ops.push(Op::SetLocal { local: slot_local, src: new });
                                return Ok(());
                            }
                        }
                        // GENERAL loop-carried heap slot — `slot = <any fresh-owned heap expr>`: a
                        // non-self list/string concat (`result = rows + [cur]`), or a call result
                        // (`result = paf(text, np, rows, cur + [field])` — the TCO RESULT ACCUMULATOR
                        // that carries a base case out of the loop, where its loop-body-local inputs
                        // like a destructured `field` are still live). Each builds a FRESH owned value
                        // (cert `i`); drop the old slot (`d`) and rebind in place (`m`) — the SAME
                        // loop-carried `i(id)m` the self-append/reset slots prove (OwnershipChecker.v
                        // `check_line_unroll_sound`), generalized to any fresh-owned producer.
                        if let Some(&slot_local) = self.value_of.get(var) {
                            let new = match &value.kind {
                                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                                    self.try_lower_concat_list(value)
                                }
                                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                                    self.try_lower_concat_str(value)
                                }
                                // TCO RESULT-ACCUMULATOR base delivery: `result = ok(acc)` / `result =
                                // err(e)` (the unwrap-`!` desugar's TCO over a `match` — base64
                                // decode_chunks). lower_result_str_piece DUPs a Var payload (rc_inc,
                                // cert `a`) so the loop-carried `acc` / borrowed `e` stays valid for its
                                // OWN scope-end drop — `result` owns a FRESH cap-tag Result block, so the
                                // slot's `i(id)m` + the payload's rc stay balanced (no double-free, no
                                // leak). `is_err` picks the @16 tag; `value.ty`'s Result repr is the
                                // 1-slot DynListStr block materialize_result_str builds.
                                IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                                    let is_err = matches!(&value.kind, IrExprKind::ResultErr { .. });
                                    match (self.lower_result_str_piece(expr), repr_of(&value.ty)) {
                                        (Some(piece), Ok(repr)) => {
                                            Some(self.materialize_result_str(piece, repr, is_err, false))
                                        }
                                        _ => None,
                                    }
                                }
                                // CLOSURE-CALL accumulator: `acc = f(acc, x)` where `f` is a
                                // first-class lifted combinator (the self-host `list_reduce_str` /
                                // `list_fold` loop). The CallIndirect yields a FRESH OWNED heap result
                                // (cert `i`, exactly the value-position closure call in binds_p2) — the
                                // loop-carried slot then drops-old (`d`) + SetLocals (`m`) it: the SAME
                                // proven `i(id)m` slot, generalized to a CallIndirect producer
                                // (OwnershipChecker.v `check_line_unroll_sound` — any fresh-owned
                                // producer). NOT pushed to live_heap_handles (the slot owns it).
                                IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                                    if self.closure_value_of(callee).is_some() =>
                                {
                                    let blk = self.closure_value_of(callee).unwrap();
                                    match (repr_of(&value.ty), self.lower_call_args(args)) {
                                        (Ok(repr), Ok(lowered)) => {
                                            let new = self.fresh_value();
                                            self.emit_closure_call(blk, Some(new), lowered, Some(repr));
                                            Some(new)
                                        }
                                        _ => None,
                                    }
                                }
                                _ => self.lower_owned_heap_field(value),
                            };
                            if let Some(new) = new {
                                if new != slot_local {
                                    let drop_op = self.drop_op_for(slot_local);
                                    self.ops.push(drop_op);
                                    self.ops.push(Op::SetLocal { local: slot_local, src: new });
                                    self.live_heap_handles.retain(|&v| v != new);
                                    return Ok(());
                                }
                            }
                        }
                        return Err(LowerError::Unsupported(
                            "heap reassignment in a scalar loop body".into(),
                        ));
                    }
                    let local = *self.value_of.get(var).ok_or_else(|| {
                        LowerError::Unsupported("scalar loop reassigns an unbound var".into())
                    })?;
                    // The reassigned value is a SCALAR: a literal/arithmetic (lower_scalar_value) OR a
                    // scalar-returning CALL (`last = string.len(e)` / `list.len(xs)`). Without the call
                    // fallback the whole `while` rolls back to model-one-iteration (runs the body ONCE
                    // → wrong accumulation AND — worse — it MASKS per-iteration leaks: a body that
                    // leaks each turn looks clean when run once). A heap value was already rejected
                    // above, so this only admits a scalar; the call's caps stay in the cert (a real
                    // CallFn). Faithful-execution by design: this surfaces real leaks, it does not hide
                    // them (see the set.from_list/string.split in-loop known-hole).
                    let src = self
                        .lower_scalar_value(value)
                        .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                        .ok_or_else(|| {
                            LowerError::Unsupported(
                                "non-scalar value in a scalar loop reassignment".into(),
                            )
                        })?;
                    self.ops.push(Op::SetLocal { local, src });
                    return Ok(());
                }
                // Inside an EXECUTABLE Unit (statement) arm, a SCALAR reassignment of a var
                // that ALREADY has a stable local (declared outside the arm) mutates that
                // local IN PLACE via `SetLocal` — exactly as v0 does — instead of a fresh
                // rebind. A rebind is frame-local: `value_of[var]` would end up pointing at
                // whichever arm lowered LAST, so a read after the branch sees a local only
                // that arm's `local.set` wrote, while at runtime the OTHER arm ran (the
                // `match n { 0 => {r=100}, x => {r=999} }` silent miscompile). The value must
                // be a SCALAR lowerable to a single value (literal/arithmetic/scalar call);
                // a heap reassignment keeps the existing branch-arm DEFER below. The local
                // is the var's own already-defined slot, so SetLocal carries no new heap
                // ownership (cert-neutral, like the loop-carried SetLocal above).
                if self.unit_arm_depth > 0 && !is_heap_ty(&value.ty) {
                    if let Some(&local) = self.value_of.get(var) {
                        if let Some(src) = self
                            .lower_scalar_value(value)
                            .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                        {
                            self.ops.push(Op::SetLocal { local, src });
                            return Ok(());
                        }
                    }
                }
                if self.in_frame > 0 && is_heap_ty(&value.ty) {
                    self.record_elided_calls(value);
                    Ok(())
                } else {
                    self.lower_bind(*var, &value.ty, value)
                }
            }
            // `let (a, b) = (x, y)` — a TUPLE destructuring bind.
            IrStmtKind::BindDestructure { pattern, value } => {
                self.lower_destructure(pattern, value)
            }
            // In-place mutation of a place: `xs[i] = v` and `r.field = v` both
            // require the buffer to be UNIQUELY owned (copy-on-write) → `MakeUnique`.
            // The written value (and an index expression) are deferred — record any
            // call inside them so the caps fold is not blind to their effects.
            IrStmtKind::IndexAssign { target, index, value } => {
                // COW-guard the buffer (rebinds the local to a unique copy if shared), then ACTUALLY
                // STORE: `xs[i] = v` → `i64.store($elem_addr(handle(xs), i), v)`. WITHOUT the store the
                // assignment lowered to ONLY the MakeUnique guard (a silent no-op — `xs[1] = 99` never
                // wrote; v1-spine hole #29). The `$elem_addr` is bounds-checked (traps OOB, matching
                // native's panic). The store runs AFTER MakeUnique so it writes the unique copy.
                self.lower_place_mutation(*target)?;
                // The SCALAR-element store subset (`List[Int/Float/Bool]`, a lowerable scalar index +
                // value) — the #29 shape. Attempt it; on a miss (a heap-element store, or a non-scalar
                // index/value) ROLL BACK to the prior behavior (record the operands' calls for caps,
                // no store) rather than walling — so a corpus IndexAssign that lowered before keeps
                // lowering (no coverage regression). The heap-element / complex case is the recursive-
                // ownership frontier, left exactly as it was (NOT made worse).
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                let stored = if !is_heap_ty(&value.ty) {
                    if let (Ok(list), Some(idx), Some(val)) = (
                        self.value_for(*target),
                        self.lower_scalar_value(index),
                        self.lower_scalar_value(value),
                    ) {
                        let h = self.fresh_value();
                        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
                        let addr = self.fresh_value();
                        self.ops.push(Op::Prim { kind: crate::PrimKind::ElemAddr, dst: Some(addr), args: vec![h, idx] });
                        self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![addr, val] });
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if !stored {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    self.record_elided_calls(index);
                    self.record_elided_calls(value);
                }
                Ok(())
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(value);
                Ok(())
            }
            // `m[k] = v` — map insertion/update, in-place on the buffer. Like
            // `IndexAssign` it requires the map to be UNIQUELY owned (copy-on-write) →
            // `MakeUnique`. The key and value are deferred — record their calls so the
            // caps fold is not blind to their effects.
            IrStmtKind::MapInsert { target, key, value } => {
                self.lower_place_mutation(*target)?;
                self.record_elided_calls(key);
                self.record_elided_calls(value);
                Ok(())
            }
            // A bare expression statement: an `if`/`match` in statement position is
            // LINEARIZED (control flow), an EFFECT call (`println(s)`) is lowered as a
            // runtime effect. Other non-call expr statements stay Unsupported (the
            // lower_effect_call guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => match &expr.kind {
                // A Unit `if` statement EXECUTES (only the taken arm's effects run) when
                // its cond is a scalar; otherwise it falls back to the linearization.
                IrExprKind::If { cond, then, else_ }
                    if self.try_lower_unit_if(cond, then, else_) =>
                {
                    Ok(())
                }
                // A Unit `match` over INT literal patterns EXECUTES: desugar to a nested
                // `if subject == lit then arm else …` and run it via try_lower_unit_if
                // (only the matched arm's effects run). Non-literal patterns / guards / a
                // non-scalar subject fall back to the linearization below.
                IrExprKind::Match { subject, arms } => {
                    if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
                        if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                            if self.try_lower_unit_if(cond, then, else_) {
                                return Ok(());
                            }
                        }
                    }
                    self.lower_branch(expr)
                }
                IrExprKind::If { .. } => self.lower_branch(expr),
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body),
                // A BLOCK expression statement (`{ stmts; e }` for its effect): lower
                // its statements (locals ride to the enclosing scope), then its tail —
                // a Unit effect call, a nested branch, or a deferred value whose calls
                // we capture (its value is discarded in statement position).
                IrExprKind::Block { stmts, expr: tail } => {
                    for s in stmts {
                        self.lower_stmt(s)?;
                    }
                    if let Some(t) = tail {
                        match &t.kind {
                            IrExprKind::Call { .. } if matches!(t.ty, Ty::Unit) => {
                                self.lower_effect_call(t)?
                            }
                            // A Block-TAIL `if` (the TCO loop body is `{ if … }`, so the base-check
                            // arrives HERE, not via the bare-If statement arm): EXECUTE it via
                            // try_lower_unit_if (real branch — only the taken arm runs) so a loop
                            // base-check actually conditionally sets `rk`. Only if that declines do
                            // we consider linearization — and inside a scalar loop linearizing both
                            // arms runs the loop ONCE (the heap-`let`-in-body silent miscompile), so
                            // wall it there. Outside a loop, linearize as before.
                            IrExprKind::If { cond, then, else_ } => {
                                if !self.try_lower_unit_if(cond, then, else_) {
                                    self.lower_branch(t)?;
                                }
                            }
                            IrExprKind::Match { subject, arms } => {
                                let mut done = false;
                                if let Some(if_expr) =
                                    self.desugar_match_to_if(subject, arms, &Ty::Unit)
                                {
                                    if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                                        done = self.try_lower_unit_if(cond, then, else_);
                                    }
                                }
                                if !done {
                                    self.lower_branch(t)?;
                                }
                            }
                            // A LOOP tail is a Unit EFFECT that must RUN — eliding it
                            // silently drops the whole loop (see lower_branch_arm's twin).
                            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                                self.lower_for_in(*var, var_tuple, iterable, body)?
                            }
                            IrExprKind::While { cond, body } => self.lower_while(cond, body)?,
                            _ => self.record_elided_calls(t),
                        }
                    }
                    Ok(())
                }
                // `break` / `continue` — a Unit-typed, value-less, label-less early exit
                // (Almide has no `break x`, no labels, no `return`). It adds NO ownership
                // op: the cert models the loop running to completion, with the
                // per-iteration frame's Drops intact. This is leak-safe ONLY when the
                // frame holds no heap handle a real early exit could skip — the loop
                // lowerers enforce that with a post-lowering frame check (a heap-frame
                // loop with break/continue is WALLED, because the v0 wasm backend frees
                // AFTER the break branch target and would leak).
                IrExprKind::Break | IrExprKind::Continue => Ok(()),
                // `bytes.push(buf, x)` — the v0 intrinsic is an IN-PLACE mutation (`mut b -> Unit`).
                // v1 has value semantics, so rewrite it to a functional rebind `buf = bytes.append(buf,
                // x)` and re-dispatch — the Assign path then handles it (a scalar-loop accumulator
                // SetLocal via the general heap-reassign, or a top-level rebind). `bytes.append` is the
                // self-hosted functional append (bytes_core). Only a bare `Var` first arg qualifies; any
                // other receiver keeps the (walling) effect-call path. Unblocks bigint.from_int / rsa.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "bytes"
                        && func.as_str() == "push"
                        && args.len() == 2
                        && matches!(&args[0].kind, IrExprKind::Var { .. }) =>
                {
                    let IrExprKind::Var { id } = &args[0].kind else { unreachable!() };
                    let append = IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module {
                                module: sym("bytes"),
                                func: sym("append"),
                                def_id: None,
                            },
                            args: vec![args[0].clone(), args[1].clone()],
                            type_args: vec![],
                        },
                        ty: args[0].ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let assign = IrStmt {
                        kind: IrStmtKind::Assign { var: *id, value: append },
                        span: None,
                    };
                    self.lower_stmt(&assign)
                }
                // `list.push(entries, e)` — same treatment as bytes.push: v0's in-place
                // mutation is observation-equal to the functional `entries = entries + [e]`
                // under value semantics, and the ConcatList Assign path (the proven
                // append-accumulator slot in a loop, the rebind at top level) handles it.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "list"
                        && func.as_str() == "push"
                        && args.len() == 2
                        && matches!(&args[0].kind, IrExprKind::Var { .. }) =>
                {
                    let IrExprKind::Var { id } = &args[0].kind else { unreachable!() };
                    let one_elem = IrExpr {
                        kind: IrExprKind::List { elements: vec![args[1].clone()] },
                        ty: args[0].ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let concat = IrExpr {
                        kind: IrExprKind::BinOp {
                            op: almide_ir::BinOp::ConcatList,
                            left: Box::new(args[0].clone()),
                            right: Box::new(one_elem),
                        },
                        ty: args[0].ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let assign = IrStmt {
                        kind: IrStmtKind::Assign { var: *id, value: concat },
                        span: None,
                    };
                    self.lower_stmt(&assign)
                }
                _ => self.lower_effect_call(expr),
            },
            // A source comment carries no ownership — skip it (it is not a
            // "silent drop": Comment is a no-op by definition, not an unhandled op).
            IrStmtKind::Comment { .. } => Ok(()),
            // `guard cond else { body }` — a CONDITIONAL early exit. The guard adds NO
            // ownership: the model takes the always-CONTINUE path (success), which is
            // self-consistent and memory-safe; the failure path's early exit and the
            // `else` body's effects are DEFERRED, like every Opaque (the guard's job is
            // functional, not a safety property). Capture the caps of any call in the
            // condition or the else body so a printing/effectful guard taints honestly.
            IrStmtKind::Guard { cond, else_ } => {
                // `guard cond else E` is a CONDITIONAL EARLY RETURN: when `!cond`, `E` is the
                // function's result. The old model DEFERRED it (always-continue), which SILENTLY
                // MISCOMPILES every call with `!cond` — `guard len(s)>0 else err("empty"); ok(x)`
                // returned `ok` for the empty input (validated(""), error_test). v1 has no
                // early-return control flow yet, so WALL it (honest) rather than emit wrong output.
                // (A guard whose `else` is a pure no-op continue would be safe to defer, but the
                // corpus guards all early-RETURN a value — none is a no-op — so an unconditional
                // wall matches the real shapes without a false-negative.)
                self.record_elided_calls(cond);
                self.record_elided_calls(else_);
                let _ = (cond, else_);
                Err(LowerError::Unsupported(
                    "guard-else early return cannot be faithfully lowered (v1 has no early-return                      control flow; deferring it silently miscompiles the !cond path) not in this brick"
                        .into(),
                ))
            }
            other => Err(LowerError::Unsupported(format!(
                "statement {} not in the value-semantics subset",
                stmt_kind_name(other)
            ))),
        }
    }

    /// In-place mutation of a place (`xs[i] = v` / `r.field = v`): the write must
    /// land on a UNIQUELY-owned buffer, so emit `Op::MakeUnique` (copy-on-write if
    /// the buffer is shared). The written value is copied (value semantics; its
    /// content is deferred, and any call in it is caps-tainted by the elided-call
    /// gate, not silently dropped). A borrowed-param target is walled — mutating
    /// the caller's data needs the move-mode calling convention.
    pub(crate) fn lower_place_mutation(&mut self, target: VarId) -> Result<(), LowerError> {
        let v = self.value_for(target)?;
        if self.param_values.contains(&v) {
            return Err(LowerError::Unsupported(
                "in-place mutation of a borrowed param not in this brick".into(),
            ));
        }
        self.ops.push(Op::MakeUnique { v });
        Ok(())
    }

    pub(crate) fn value_for(&self, var: VarId) -> Result<ValueId, LowerError> {
        self.value_of
            .get(&var)
            .copied()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))
    }

    /// Resolve a value-position variable reference, admitting a reference to a
    /// module-level `let` GLOBAL. A function-local var is in `value_of`. A miss is a
    /// global IFF it is in the DECLARED global set (`self.globals`) — the frontend
    /// guarantees every non-global reference is bound by a preceding local form, so a
    /// miss that is NOT a declared global is a genuine lowering gap and stays WALLED.
    ///
    /// A confirmed global is bound ONCE (cached in `value_of`, so repeated references
    /// reuse the one handle) as a fresh EXTERNAL value: a scalar global is a Copy
    /// `Const`; a heap global is a fresh owned `Alloc{Opaque}` dropped at scope end —
    /// we model an owned COPY rather than an alias of the module's object, which is
    /// memory-safe by construction (alloc once / drop once, the real global untouched)
    /// and its content deferred like every `Opaque`. Referencing a global does NOT
    /// re-run its initializer, so this adds no call/cap obligation.
    pub(crate) fn value_or_global(&mut self, var: VarId) -> Result<ValueId, LowerError> {
        if let Some(&v) = self.value_of.get(&var) {
            return Ok(v);
        }
        let ty = self
            .globals
            .get(&var)
            .cloned()
            .ok_or_else(|| LowerError::Unsupported(format!("use of unbound var {var:?}")))?;
        if is_heap_ty(&ty) {
            // A HEAP module-level global (the base64 alphabet, the aes S-box): MATERIALIZE a FRESH
            // OWNED copy of its CONST initializer as a DIRECT `Alloc` — a string literal (`Init::Str`),
            // an int-list literal (`Init::IntList`), or `bytes.from_list([int literals])` (`Init::Bytes`).
            // CRITICAL: only a CONST-foldable init (NO runtime call) is admitted, so the materialization
            // injects ZERO `CallFn` ops — the gate's IR-side `count_ir_calls` stays exact (`mir == ir`).
            // A COMPUTED init (`string.from_codepoint(10)`, a user call) would inject a call the IR-body
            // count never sees (mir>ir = a false caps de-taint), so it keeps WALLING (no regression).
            // The fresh owned copy is dropped at scope end like any literal (cert: one `i` + one `d`);
            // `value_of[var]` caches it so repeated references in the SAME function reuse the one copy.
            if let Some(init) = self.global_inits.get(&var) {
                if let Some(const_init) = const_global_init(init) {
                    let repr = repr_of(&ty)?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc { dst, repr, init: const_init });
                    self.live_heap_handles.push(dst);
                    self.value_of.insert(var, dst);
                    return Ok(dst);
                }
                // A NESTED-OWNERSHIP heap global with no flat CONST-data form but a PURE
                // (call-free) LITERAL initializer — the `let DIFFICULTIES = ["basic", …]`
                // shape: materialize a FRESH OWNED copy via the SAME `DynListStr` builder a
                // local `let xs = [..]` uses (`try_lower_str_list_literal`). GATED to a
                // call-free literal list (`is_pure_literal_list`) so the materialization
                // injects ZERO `CallFn` — the IR reference is a single `Var` (0 calls), so the
                // gate's `mir == ir` count stays exact. A COMPUTED element (`[f(x)]`,
                // `string.repeat(..)`) is NOT pure → keeps walling (no mir>ir de-taint). The
                // builder registers the right recursive drop set (`heap_elem_lists` →
                // `DropListStr`); we add it to `live_heap_handles` so the fresh owned copy is
                // freed at scope end (cert one `i` + one `d`), the real module global untouched.
                if is_pure_literal_list(init) {
                    let init = init.clone();
                    if let Some(dst) = self.try_lower_str_list_literal(&init) {
                        self.live_heap_handles.push(dst);
                        self.value_of.insert(var, dst);
                        return Ok(dst);
                    }
                }
            }
            return Err(LowerError::Unsupported(format!(
                "reference to a heap module-level global {var:?} cannot be faithfully \
                 materialized in this brick (no CONST initializer — a computed init would \
                 inject an uncounted call)"
            )));
        }
        // A SCALAR module-level global: materialize its CONST (call-free)
        // initializer's REAL value — a literal, const arithmetic, or a
        // reference to another const global (`let SOLAR_MASS = 4.0 * PI * PI`,
        // which recurses back through value_or_global). This used to fall to
        // the deferred `Const` = 0 — every USE of a scalar top-level `let` read
        // zero (top_let_test printed `PI = 0`, a silent miscompile). A
        // call-bearing init would inject CallFn ops the gate's IR-side count
        // never sees, so it WALLS instead (honest, never wrong).
        if let Some(init) = self.global_inits.get(&var) {
            fn init_has_call(e: &IrExpr) -> bool {
                use almide_ir::visit::{walk_expr, IrVisitor};
                struct C(bool);
                impl IrVisitor for C {
                    fn visit_expr(&mut self, e: &IrExpr) {
                        if matches!(
                            e.kind,
                            IrExprKind::Call { .. }
                                | IrExprKind::TailCall { .. }
                                | IrExprKind::RuntimeCall { .. }
                        ) {
                            self.0 = true;
                        }
                        walk_expr(self, e);
                    }
                }
                let mut c = C(false);
                c.visit_expr(e);
                c.0
            }
            if !init_has_call(init) {
                let init = init.clone();
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(&init) {
                    self.value_of.insert(var, dst);
                    return Ok(dst);
                }
                self.ops.truncate(mark);
            }
            return Err(LowerError::Unsupported(format!(
                "scalar module-level global {var:?} has a non-const-foldable initializer                  (a call would be uncounted; a deferred Const-0 would be silently wrong)                  not in this brick"
            )));
        }
        if crate::lower::strict_values() {
            return Err(crate::lower::strict_const_wall("module-level global"));
        }
        let dst = self.fresh_value();
        self.ops.push(Op::Const { dst });
        self.value_of.insert(var, dst);
        Ok(dst)
    }

    /// The correct release op for a heap value at scope/frame end, by its tracking set (the SINGLE
    /// source of truth for drop-op selection — used by `emit_scope_end_drops`, `drop_arm_locals`, and
    /// the variant-match subject drop). Order matters: the recursive value-drops are checked BEFORE
    /// the flat `DropListStr`, since a `value.as_array` Result / a `List[Value]` is ALSO a
    /// `heap_elem_list`, but a flat per-slot `rc_dec` there would leak the nested element Values.
    /// The NAMED record type of `ty` iff it needs the recursive `$__drop_<R>` (some field is a
    /// `Map`/`Value`/record/`List[heap]` — [`record_field_needs_recursive_drop`]). A record VALUE of
    /// such a type is registered in `variant_drop_handles` so `drop_op_for` routes it to the recursive
    /// `Op::DropVariant` instead of the flat `DropListStr` (which would leak its nested heap fields).
    pub(crate) fn record_drop_type_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let name = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        // Return the CANONICAL registry key (a bare cross-module spelling resolves to the
        // qualified decl name) — this string is the drop-routing identity, and the
        // generators name `$__drop_<R>` from the QUALIFIED decl.
        let canonical =
            crate::lower::canonical_record_key(&self.record_layouts, &name)?.to_string();
        let (_, tys) = self.aggregate_field_tys(ty)?;
        tys.iter()
            .any(record_field_needs_recursive_drop)
            .then_some(canonical)
    }

    /// The recursive-drop handle name for a record VALUE of type `ty` — a NAMED recursive record's
    /// `<name>` (→ `$__drop_<name>`, generated from its `type` decl) OR a synthesized
    /// `anonrec_<hash>` for an ANONYMOUS record that owns any heap field whose nested heap a flat
    /// one-level mask would LEAK (`{ data: Bytes, state: Cfb8State }` — aes cfb8;
    /// `__drop_anonrec_<hash>` is emitted by `generate_record_drop_sources` from
    /// `collect_recursive_anon_records`). `None` for a non-record / scalar-only record (the flat
    /// masked `DropListStr` is sound). The synthesis predicate is structural
    /// (`record_field_needs_recursive_drop`), so this lowering-side decision matches the
    /// generation-side one exactly.
    pub(crate) fn record_or_anon_drop_type_name(&self, ty: &Ty) -> Option<String> {
        if let Some(name) = self.record_drop_type_name(ty) {
            return Some(name);
        }
        if let Ty::Record { fields } = ty {
            if crate::lower::anon_record_needs_recursive_drop(fields) {
                return Some(crate::lower::anon_record_drop_name(fields));
            }
        }
        None
    }

    pub(crate) fn drop_op_for(&self, v: ValueId) -> Op {
        if let Some(ty) = self.variant_drop_handles.get(&v) {
            // `List[(Int, String)]` was routed here as a pseudo-"variant" but has no generated
            // `$__drop_list_int_str` ADT helper (the `DropVariant` render emitted a dangling call →
            // invalid wat). Route it to the dedicated INLINE `DropListIntStr` (frees each tuple's
            // String slot + block, then the list). Every real user-ADT variant keeps `DropVariant`.
            if ty == "list_int_str" {
                Op::DropListIntStr { v }
            } else if ty == "list_str_int" {
                Op::DropListStrInt { v }
            } else if let Some(drop_fn) = ty.strip_prefix("optrec:") {
                // An Option WRAPPER holding a heap RECORD payload (`some({key, val})`): recurse into
                // the @12 record via `$__drop_<drop_fn>` at the wrapper's last ref, then free the
                // wrapper block. The `optrec:` prefix is injected by `materialize_opt_aggregate_some`.
                Op::DropWrapperRec { v, drop_fn: drop_fn.to_string(), is_result: false }
            } else if let Some(drop_fn) = ty.strip_prefix("resrec:") {
                // A Result WRAPPER holding a heap RECORD Ok payload (`ok({val, next})`): recurse into
                // the @12 record (tag@16==0) via `$__drop_<drop_fn>`, else `rc_dec` the @12 Err
                // String, then free the wrapper. Injected by `materialize_result_aggregate`.
                Op::DropWrapperRec { v, drop_fn: drop_fn.to_string(), is_result: true }
            } else {
                Op::DropVariant { v, ty: ty.clone() }
            }
        } else if self.value_result_lists.contains(&v) {
            Op::DropResultListValue { v }
        } else if self.value_result_results.contains(&v) {
            Op::DropResultValue { v }
        } else if self.str_int_result_results.contains(&v) {
            Op::DropResultStrInt { v }
        } else if self.value_int_result_results.contains(&v) {
            Op::DropResultValueInt { v }
        } else if self.list_value_int_result_results.contains(&v) {
            Op::DropResultListValueInt { v }
        } else if self.list_str_int_result_results.contains(&v) {
            Op::DropResultListStrInt { v }
        } else if self.list_str_result_results.contains(&v) {
            Op::DropResultListStr { v }
        } else if self.value_elem_lists.contains(&v) {
            Op::DropListValue { v }
        } else if self.str_value_elem_lists.contains(&v) {
            Op::DropListStrValue { v }
        } else if self.str_str_elem_lists.contains(&v) {
            Op::DropListStrStr { v }
        } else if self.list_list_str_lists.contains(&v) {
            // `List[List[String]]` — checked BEFORE heap_elem_lists (it also matches
            // is_heap_elem_list_ty): the nested loop frees each inner row's cell Strings, which a
            // flat DropListStr would leak.
            Op::DropListListStr { v }
        } else if self.heap_elem_lists.contains(&v) || self.record_masks.contains_key(&v) {
            Op::DropListStr { v }
        } else if self.value_handles.contains(&v) {
            Op::DropValue { v }
        } else {
            Op::Drop { v }
        }
    }

    pub(crate) fn emit_scope_end_drops(&mut self) {
        // Reverse binding order (LIFO scope teardown). A `List[String]` value is released by a
        // RECURSIVE `DropListStr` (frees its owned element Strings); every other heap value by
        // a flat `Drop`.
        let drops: Vec<Op> =
            self.live_heap_handles.iter().rev().map(|v| self.drop_op_for(*v)).collect();
        self.ops.extend(drops);
    }
}
