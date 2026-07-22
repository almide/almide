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
            // The param's DECLARED ty, for the same type-directed rewrites a local
            // bind gets (the FieldAssign/MapInsert functional rebinds need the record/
            // map ty of a move-mode param target — the C-132 `list.push(b.xs, v)`
            // write-back shape). The scalar-slot store path is unaffected: its
            // borrowed-param wall (`lower_place_mutation`) fires before the read.
            self.var_decl_tys.insert(p.var, p.ty.clone());
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
                        // A RICH custom-variant Err payload (`Result[String, MathError]` —
                        // Overflow(String) owns nested heap): the flat DropListStr would
                        // free the variant BLOCK but leak its fields. Route the drop to the
                        // Err-side recursion (`reserr:` — DropWrapperRec `err_rec`); the
                        // heap_elem_lists membership stays for the bind gates (drop_op_for
                        // consults variant_drop_handles first). A PARAM is never dropped
                        // (it stays in param_values), so the entry is read-inert there.
                        if let Some(vn) = self.custom_variant_type_name(&a[1]) {
                            if self.variant_layouts.needs_recursive_drop(&vn, &|rn| {
                                crate::lower::canonical_record_key(&self.record_layouts, rn)
                                    .is_some()
                            }) {
                                self.variant_drop_handles.insert(v, format!("reserr:{vn}"));
                            }
                        }
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
        if let Some(rewritten) = crate::lower::desugar_method_calls(body, &self.record_layouts) {
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
        // `unit_main` (the void-main die-on-error convention) applies ONLY to a `main` that
        // declares the SYNTHETIC void return (bare `Unit`, no explicit Result/Option) — a
        // `main` that EXPLICITLY declares `-> Result[Unit, String]` is a REAL Result-returning
        // fn the caller inspects (cross_module_unit_effect_test), so its `!`-desugared Err arm
        // must reconstruct `err(e)` normally, never the abort-line shape. `decl_ret_is_result`
        // already draws exactly this line (tail.rs's Result[Unit] tail-voiding gate reuses it).
        let unit_main = self.fn_name == "main" && !self.decl_ret_is_result;
        if let Some(rewritten) =
            desugar_effect_unwrap(body, unit_main, self.ret_is_result_abi, &self.variant_layouts)
        {
            return self.lower_body_into(&rewritten);
        }
        if unit_main {
            if let Some(rewritten) = crate::lower::desugar_unit_main_err_arms(body) {
                return self.lower_body_into(&rewritten);
            }
        }
        if let Some(rewritten) = crate::lower::desugar_sort_by_cached_keys(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_to_option_calls(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_offtype_testing_asserts(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = desugar_heap_branches(body, &self.variant_layouts) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_scalar_tuple_literal_match(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_scalar_guard_match(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_tuple_variant_match(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) =
            crate::lower::desugar_tuple_variant_match_deep(body, &self.variant_layouts)
        {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_tuple_empty_list_match(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_fan_block(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_record_destructure_match(body) {
            return self.lower_body_into(&rewritten);
        }
        if let Some(rewritten) = crate::lower::desugar_list_pattern_match(body) {
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
                self.var_decl_tys.insert(*var, ty.clone());
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
            IrStmtKind::Assign { var, value } => self.lower_stmt_assign(*var, value),
            // `let (a, b) = (x, y)` — a TUPLE destructuring bind.
            IrStmtKind::BindDestructure { pattern, value } => {
                self.lower_destructure(pattern, value)
            }
            // In-place mutation of a place: `xs[i] = v` and `r.field = v` both
            // require the buffer to be UNIQUELY owned (copy-on-write) → `MakeUnique`.
            // The written value (and an index expression) are deferred — record any
            // call inside them so the caps fold is not blind to their effects.
            IrStmtKind::IndexAssign { target, index, value } => self.lower_stmt_index_assign(*target, index, value),
            IrStmtKind::FieldAssign { target, field, value } => self.lower_stmt_field_assign(*target, *field, value),
            // `m[k] = v` — map insertion/update, in-place on the buffer. Like
            // `IndexAssign` it requires the map to be UNIQUELY owned (copy-on-write) →
            // `MakeUnique`. The key and value are deferred — record their calls so the
            // caps fold is not blind to their effects.
            IrStmtKind::MapInsert { target, key, value } => self.lower_stmt_map_insert(*target, key, value),
            // A bare expression statement: an `if`/`match` in statement position is
            // LINEARIZED (control flow), an EFFECT call (`println(s)`) is lowered as a
            // runtime effect. Other non-call expr statements stay Unsupported (the
            // lower_effect_call guard rejects them — flight-grade totality).
            IrStmtKind::Expr { expr } => self.lower_stmt_expr(expr),
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

    /// The `Assign` statement arm of [`Self::lower_stmt`] — router (#781
    /// decomposition, continued): each guard below either returns directly or
    /// (for the fall-through-capable unit-arm guards) tries a helper and only
    /// returns if it reports a match; the helpers are verbatim moves of the
    /// original arm bodies.
    fn lower_stmt_assign(&mut self, var: VarId, value: &IrExpr) -> Result<(), LowerError> {
        // ASSIGN to a SHARED-CELL var (cells.rs): write through the cell slot —
        // rebinding a local copy would silently vanish for the sharing closure
        // (the same hazard as a mutable global, function-locally).
        if let Some(&cell) = self.cell_of.get(&var) {
            return self.lower_cell_assign(var, cell, value);
        }
        // ASSIGN to a MUTABLE module-level `var`: write through its STORAGE SLOT
        // (`lower_bind` below would rebind a function-LOCAL copy and the write
        // silently vanishes for every other function). The `value_of` gate skips a
        // CROSS-REGION VarId collision where the target really is a bound local
        // (a mutable global itself never enters `value_of`: reads are uncached).
        if !self.value_of.contains_key(&var) {
            if let Some((index, gty)) = crate::lower::mutable_global_info(var) {
                return self.lower_mutable_global_assign(var, index, &gty, value);
            }
        }
        // Inside a scalar-marker loop, a reassignment mutates the var's STABLE
        // local (the loop-carried state) — `SetLocal`, not a fresh rebind. A heap
        // reassignment cannot run this way (the accumulator would need real heap
        // merge): ERROR to abort the attempt → `lower_while` falls back to its
        // sound model-one-iteration form.
        if self.scalar_loop_depth > 0 {
            return self.lower_stmt_assign_scalar_loop(var, value);
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
            if let Some(result) = self.lower_stmt_assign_unit_scalar(var, value) {
                return result;
            }
        }
        // A HEAP reassignment inside an EXECUTING unit arm (`if let v = x {
        // out = int.to_string(v) }` — the statement if-let / variant-match
        // arms): the var already owns a stable scope-tracked heap local, so
        // the write is drop-old + `SetLocal` IN PLACE — the same rebind unit
        // the loop-carried slot proves (per-arm the `i` of the fresh value
        // and the `d` of the old one balance; the slot's scope-end drop
        // frees whichever object the taken arm left). A borrowed-param slot
        // is excluded (its drop-old would release the caller's reference).
        if self.unit_arm_depth > 0 && is_heap_ty(&value.ty) {
            if let Some(result) = self.lower_stmt_assign_unit_heap(var, value) {
                return result;
            }
        }
        if self.in_frame > 0 && is_heap_ty(&value.ty) {
            // STRICT value mode: this defer DROPS the write. In an EXECUTING frame
            // (a `try_lower_unit_if` arm) that is a silent wrong value on the
            // verified default — `if let v = x { out = int.to_string(v) }` left
            // `out` at its pre-branch value while native assigned. The executable
            // fix is a stable heap-handle slot (the loop-carried SetLocal shape,
            // branch-merged); until that lands, REFUSE — v0 emits correct bytes.
            if crate::lower::strict_values() {
                return Err(LowerError::Unsupported(
                    "heap reassignment inside a control-flow frame — deferring \
                     the write would be a silent no-op; the branch-merged handle \
                     slot is not in this brick"
                        .into(),
                ));
            }
            self.record_elided_calls(value);
            Ok(())
        } else {
            self.lower_bind(var, &value.ty, value)
        }
    }

    /// The scalar-loop-body reassignment arm of [`Self::lower_stmt_assign`]
    /// (`self.scalar_loop_depth > 0`) — verbatim move, always returns.
    fn lower_stmt_assign_scalar_loop(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
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
                        if matches!(&left.kind, IrExprKind::Var { id } if id == &var) {
                            if let Some(&slot_local) = self.value_of.get(&var) {
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
                    if let Some(&slot_local) = self.value_of.get(&var) {
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
                    if let Some(&slot_local) = self.value_of.get(&var) {
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
                                // Repr dispatch: a HEAP-Ok Result (`Result[String,_]` — base64
                                // decode_chunks) is the cap-tag block `materialize_result_str`
                                // builds; a SCALAR-Ok Result (`Result[Int,String]` — the
                                // early-return `res` accumulator) is the LEN-AS-TAG family, so
                                // routing it through the str builder emitted a scalar payload
                                // into a handle slot — invalid wasm (i32/i64 mismatch) that
                                // ESCAPED the render wall (probe-confirmed). Build len-tag:
                                // Ok → `materialize_result_ok` (len 0, scalar @12); Err →
                                // `materialize_opt_str_some` (len 1, owned String @12 — the
                                // same physical block `try_lower_result_err_variant_ctor`
                                // uses; the slot's bind-time tracking already frees slot-0
                                // on the Err path via DropListStr).
                                if Self::is_heap_ok_result(&value.ty) {
                                    match (self.lower_result_str_piece(expr), repr_of(&value.ty)) {
                                        (Some(piece), Ok(repr)) => {
                                            Some(self.materialize_result_str(piece, repr, is_err, false))
                                        }
                                        _ => None,
                                    }
                                } else if is_err {
                                    match (self.lower_result_str_piece(expr), repr_of(&value.ty)) {
                                        (Some(piece), Ok(repr)) => {
                                            Some(self.materialize_opt_str_some(piece, repr))
                                        }
                                        _ => None,
                                    }
                                } else {
                                    match (self.lower_scalar_value(expr), repr_of(&value.ty)) {
                                        (Some(payload), Ok(repr)) => {
                                            Some(self.materialize_result_ok(payload, repr))
                                        }
                                        _ => None,
                                    }
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
                let local = *self.value_of.get(&var).ok_or_else(|| {
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

    /// The unit-arm SCALAR reassignment arm of [`Self::lower_stmt_assign`]
    /// (`self.unit_arm_depth > 0 && !is_heap_ty`) — `None` means fall through
    /// to the next arm; verbatim move.
    fn lower_stmt_assign_unit_scalar(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Option<Result<(), LowerError>> {
                if let Some(&local) = self.value_of.get(&var) {
                    if let Some(src) = self
                        .lower_scalar_value(value)
                        .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                    {
                        self.ops.push(Op::SetLocal { local, src });
                        return Some(Ok(()));
                    }
                }
        None
    }

    /// The unit-arm HEAP reassignment arm of [`Self::lower_stmt_assign`]
    /// (`self.unit_arm_depth > 0 && is_heap_ty`) — `None` means fall through
    /// to the next arm; verbatim move.
    fn lower_stmt_assign_unit_heap(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Option<Result<(), LowerError>> {
                if let Some(&local) = self.value_of.get(&var) {
                    if self.live_heap_handles.contains(&local)
                        && !self.param_values.contains(&local)
                    {
                        let mark = self.ops.len();
                        let lhh_mark = self.live_heap_handles.len();
                        // A literal/concat/interp/Var value via the owned-field
                        // helper; a heap-returning CALL (`out = int.to_string(v)`)
                        // via the call-arg materialization (a fresh owned result).
                        let new = self.lower_owned_heap_field(value).or_else(|| {
                            if !matches!(&value.kind, IrExprKind::Call { .. }) {
                                return None;
                            }
                            match self.lower_call_args(std::slice::from_ref(value)) {
                                Ok(args) => match args.into_iter().next() {
                                    Some(crate::CallArg::Handle(v)) => Some(v),
                                    _ => None,
                                },
                                Err(_) => None,
                            }
                        });
                        if let Some(new) = new {
                            let drop_op = self.drop_op_for(local);
                            self.ops.push(drop_op);
                            self.ops.push(Op::SetLocal { local, src: new });
                            // ONLY the rebound value leaves the scope-drop set (the
                            // slot owns it; the local's own scope-end drop frees it).
                            // Any arg temp the value lowering tracked stays — the
                            // per-arm drop releases it (truncating it away left the
                            // arm +1 → a grouped seg → the {i|} poison cascade).
                            self.live_heap_handles.retain(|&v| v != new);
                            return Some(Ok(()));
                        }
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh_mark);
                    }
                }
        None
    }


    /// The `IndexAssign` arm of [`Self::lower_stmt`] — verbatim move (#781).
    fn lower_stmt_index_assign(&mut self, target: VarId, index: &IrExpr, value: &IrExpr) -> Result<(), LowerError> {
                // A mutable-GLOBAL place target: `g[i] = v` routes through the slot as
                // TAKE (the slot's owned ref transfers to us) → `MakeUnique` (COW if a
                // reader's Dup is still live — the mutation must touch no alias) →
                // bounds-checked element store → STORE-BACK (+`Consume`) of the possibly-
                // copied block. Going through `lower_place_mutation` instead would COW the
                // read-Dup and write the COPY — the global would silently keep the old
                // value. SCALAR-element lists only this round (the #29 store subset);
                // heap-element / non-scalar shapes WALL, as does a modeled frame (the
                // write is an effect the model would elide).
                if !self.value_of.contains_key(&target) {
                    if let Some((gindex, gty)) = crate::lower::mutable_global_info(target) {
                        if self.in_frame > 0
                            && self.unit_arm_depth == 0
                            && self.scalar_loop_depth == 0
                        {
                            return Err(LowerError::Unsupported(format!(
                                "index-assign to mutable module-level var {target:?} inside \
                                 a modeled (non-executable) frame"
                            )));
                        }
                        if is_heap_ty(&value.ty)
                            || !crate::lower::is_heap_ty(&gty)
                            || crate::lower::is_heap_elem_list_ty(&gty)
                        {
                            return Err(LowerError::Unsupported(format!(
                                "index-assign to mutable module-level var {target:?} outside \
                                 the scalar-element subset is not in this brick"
                            )));
                        }
                        let (idx, val) = match (
                            self.lower_scalar_value(index),
                            self.lower_scalar_value(value),
                        ) {
                            (Some(i), Some(v)) => (i, v),
                            _ => {
                                return Err(LowerError::Unsupported(format!(
                                    "index-assign to mutable module-level var {target:?} with \
                                     a non-lowerable index/value"
                                )))
                            }
                        };
                        let repr = repr_of(&gty)?;
                        let addr = self.fresh_value();
                        self.ops.push(Op::ConstInt {
                            dst: addr,
                            value: crate::mg_slot_addr(gindex) as i64,
                        });
                        let taken = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(taken),
                            name: "__mg_take".to_string(),
                            args: vec![crate::CallArg::Scalar(addr)],
                            result: Some(repr),
                        });
                        self.materialized_call_arg(taken, repr, &gty);
                        self.ops.push(Op::MakeUnique { v: taken });
                        self.ops.push(Op::ListSetScalar { list: taken, idx, val });
                        let h2 = self.fresh_value();
                        self.ops.push(Op::Prim {
                            kind: crate::PrimKind::Handle,
                            dst: Some(h2),
                            args: vec![taken],
                        });
                        self.ops.push(Op::Prim {
                            kind: crate::PrimKind::Store { width: 8 },
                            dst: None,
                            args: vec![addr, h2],
                        });
                        self.ops.push(Op::Consume { v: taken });
                        self.live_heap_handles.retain(|v| *v != taken);
                        return Ok(());
                    }
                }
                // COW-guard the buffer (rebinds the local to a unique copy if shared), then ACTUALLY
                // STORE: `xs[i] = v` → `i64.store($elem_addr(handle(xs), i), v)`. WITHOUT the store the
                // assignment lowered to ONLY the MakeUnique guard (a silent no-op — `xs[1] = 99` never
                // wrote; v1-spine hole #29). The `$elem_addr` is bounds-checked (traps OOB, matching
                // native's panic). The store runs AFTER MakeUnique so it writes the unique copy.
                self.lower_place_mutation(target)?;
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
                        self.value_for(target),
                        self.lower_scalar_value(index),
                        self.lower_scalar_value(value),
                    ) {
                        self.ops.push(Op::ListSetScalar { list, idx, val });
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
                    // A HEAP String/Value element (`xs[0] = "Z"` — the C-136 case-5
                    // shape): desugar to the FUNCTIONAL rebind `xs = list.set(xs, i, v)`
                    // — the router picks the registered rc-correct `_str`/`_value` twin
                    // (rc_dec the replaced element + own the new), and the ordinary
                    // Assign machinery swaps the local (the map-insert discipline).
                    if matches!(&value.ty, Ty::String) || crate::lower::is_value_ty(&value.ty) {
                        {
                            let list_ty = Ty::Applied(
                                almide_lang::types::constructor::TypeConstructorId::List,
                                vec![value.ty.clone()],
                            );
                            let xs_expr = IrExpr {
                                kind: IrExprKind::Var { id: target },
                                ty: list_ty.clone(),
                                span: value.span,
                                def_id: None,
                            };
                            let call = IrExpr {
                                kind: IrExprKind::Call {
                                    target: CallTarget::Module {
                                        module: almide_lang::intern::sym("list"),
                                        func: almide_lang::intern::sym("set"),
                                        def_id: None,
                                    },
                                    args: vec![xs_expr, index.clone(), value.clone()],
                                    type_args: Vec::new(),
                                },
                                ty: list_ty,
                                span: value.span,
                                def_id: None,
                            };
                            let assign = IrStmt {
                                kind: IrStmtKind::Assign { var: target, value: call },
                                span: value.span.clone(),
                            };
                            return self.lower_stmt(&assign);
                        }
                    }
                    // STRICT value mode: an elided element write is an EXECUTABLE silent
                    // no-op (`xs[0] = "Z"` left the list unchanged on the verified default
                    // while native stored). REFUSE — the fn walls, v0 emits correct bytes.
                    if crate::lower::strict_values() {
                        return Err(LowerError::Unsupported(
                            "index-assign outside the scalar-element store subset (heap \
                             element or non-scalar index/value) — eliding the write would \
                             be a silent no-op not in this brick"
                                .into(),
                        ));
                    }
                    self.record_elided_calls(index);
                    self.record_elided_calls(value);
                }
                Ok(())
    }


    /// The `FieldAssign` arm of [`Self::lower_stmt`] — verbatim move (#781).
    fn lower_stmt_field_assign(&mut self, target: VarId, field: almide_lang::intern::Sym, value: &IrExpr) -> Result<(), LowerError> {
                // Mutable-GLOBAL target: same COW-copy silent-miscompile class as the
                // IndexAssign guard above — WALL.
                if !self.value_of.contains_key(&target) && crate::lower::is_mutable_global(target) {
                    return Err(LowerError::Unsupported(format!(
                        "field-assign to mutable module-level var {target:?} (in-place \
                         mutation through the global slot) is not in this brick"
                    )));
                }
                // A HEAP-typed field write takes the functional REBIND `r.f = v` ≡
                // `r = { ...r, f: v }` — the same value-semantics treatment `m[k] = v`
                // gets: the spread construct reads the old record (a borrow), and the
                // Assign path owns the whole rebind protocol (drop-old + slot
                // accounting). BOTH legs take this path (one shared rewrite — the
                // permissive cert then witnesses the SAME ops the strict render emits;
                // a strict-only rewrite walled the permissive leg's mut-param shapes
                // and broke the walled-real ratchet). NO MakeUnique here — an aliased
                // record must NOT be uniquified first (the manual COW guard composed
                // with the Assign's drop-old into an rc-underflow trap: alias_cow
                // test_6). On Err the whole fn WALLS (ctx discarded), so no rollback
                // is needed.
                if is_heap_ty(&value.ty) {
                    if let Some(rec_ty) = self.var_decl_tys.get(&target).cloned() {
                        if self.aggregate_field_tys(&rec_ty).is_some() {
                            let base = IrExpr {
                                kind: IrExprKind::Var { id: target },
                                ty: rec_ty.clone(),
                                span: None,
                                def_id: None,
                            };
                            let spread = IrExpr {
                                kind: IrExprKind::SpreadRecord {
                                    base: Box::new(base),
                                    fields: vec![(field, value.clone())],
                                },
                                ty: rec_ty,
                                span: None,
                                def_id: None,
                            };
                            let assign = IrStmt {
                                kind: IrStmtKind::Assign { var: target, value: spread },
                                span: None,
                            };
                            return self.lower_stmt(&assign);
                        }
                    }
                }
                // COW-guard the buffer, then ACTUALLY STORE the field: `r.f = v` →
                // `ListSetScalar(block, slot(f), v)` on the uniform 8-byte-slot aggregate
                // block (the rung-5 layout; `ListGetScalar` is the read side). WITHOUT the
                // store, the assignment lowered to ONLY the MakeUnique guard — EVERY record
                // field-assign was a silent no-op on the verified default (v1 read back the
                // pre-assign value while native mutated: the recassign wrong-value class).
                self.lower_place_mutation(target)?;
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                let stored = if !is_heap_ty(&value.ty) {
                    let slot = self
                        .var_decl_tys
                        .get(&target)
                        .cloned()
                        .and_then(|ty| self.aggregate_field_offset_any(&ty, field.as_str()))
                        .map(|off| {
                            (off - crate::lower::layout::BLOCK_HEADER)
                                / crate::lower::layout::SLOT_SIZE
                        });
                    if let (Ok(list), Some(slot), Some(val)) =
                        (self.value_for(target), slot, self.lower_scalar_value(value))
                    {
                        let idx = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: idx, value: slot as i64 });
                        self.ops.push(Op::ListSetScalar { list, idx, val });
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
                    // STRICT value mode (the real render path): an elided field write is an
                    // EXECUTABLE silent no-op — REFUSE, so the fn walls and v0 emits the
                    // correct bytes. (The heap-typed value shape already took the spread
                    // REBIND above.) The permissive caps-counting classifier keeps the old
                    // elision (its only consumer is call accounting).
                    if crate::lower::strict_values() {
                        return Err(LowerError::Unsupported(format!(
                            "field-assign `.{} = …` outside the scalar-slot store subset \
                             (heap-typed value, unresolved layout, or non-scalar RHS) — \
                             eliding the write would be a silent no-op not in this brick",
                            field.as_str()
                        )));
                    }
                    self.record_elided_calls(value);
                }
                Ok(())
    }


    /// The `MapInsert` arm of [`Self::lower_stmt`] — verbatim move (#781).
    fn lower_stmt_map_insert(&mut self, target: VarId, key: &IrExpr, value: &IrExpr) -> Result<(), LowerError> {
                // Mutable-GLOBAL target: same COW-copy silent-miscompile class as the
                // IndexAssign guard above — WALL.
                if !self.value_of.contains_key(&target) && crate::lower::is_mutable_global(target) {
                    return Err(LowerError::Unsupported(format!(
                        "map-insert to mutable module-level var {target:?} (in-place \
                         mutation through the global slot) is not in this brick"
                    )));
                }
                // Functional REBIND: `m[k] = v` ≡ `m = map.set(m, k, v)` (value
                // semantics) — the SAME treatment the `map.insert(m, k, v)` CALL form
                // already gets below; the repr dispatch suffixes the self-host
                // (set_skv/str/…) exactly like a source-level call. Both legs take this
                // path (one shared rewrite, mir==ir symmetric); classify credits the
                // MapInsert node with the one synthetic call. Needs the declared map ty
                // for the Var reference — a target without one (a param, not a local
                // bind) keeps the historic wall below.
                if let Some(map_ty) = self.var_decl_tys.get(&target).cloned() {
                    let m_ref = IrExpr {
                        kind: IrExprKind::Var { id: target },
                        ty: map_ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let call = IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module {
                                module: sym("map"),
                                func: sym("set"),
                                def_id: None,
                            },
                            args: vec![m_ref, key.clone(), value.clone()],
                            type_args: vec![],
                        },
                        ty: map_ty,
                        span: None,
                        def_id: None,
                    };
                    let assign =
                        IrStmt { kind: IrStmtKind::Assign { var: target, value: call }, span: None };
                    return self.lower_stmt(&assign);
                }
                self.lower_place_mutation(target)?;
                // STRICT value mode: the insert itself was ELIDED (only the MakeUnique
                // guard emitted) — `m[k] = v` was a silent no-op on the verified default
                // (native inserted, v1 read the map unchanged). REFUSE so the fn walls
                // and v0 emits the correct bytes; the permissive classifier keeps the
                // old elision for call accounting.
                if crate::lower::strict_values() {
                    return Err(LowerError::Unsupported(
                        "map-insert `m[k] = v` (in-place map mutation) — eliding the \
                         write would be a silent no-op not in this brick"
                            .into(),
                    ));
                }
                self.record_elided_calls(key);
                self.record_elided_calls(value);
                Ok(())
    }


    /// The `Expr` (statement-position expression) arm of [`Self::lower_stmt`] — verbatim move (#781).
    pub(crate) fn lower_stmt_expr(&mut self, expr: &IrExpr) -> Result<(), LowerError> {
        match &expr.kind {
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
                    // A TUPLE subject of scalar elements in STATEMENT position — the
                    // heap-branch tail-duplication rewrites `let s = match (…) {…};
                    // use(s)` into this Unit form, so the refinement chain needs a
                    // unit sibling (real IfThen/Else/EndIf markers; only the taken
                    // arm's effects run — the linearization guard stays for the rest).
                    if self.try_lower_tuple_refinement_unit_match(subject, arms) {
                        return Ok(());
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
                            // Same statement-dispatcher routing as the arm-tail
                            // (control.rs): a Block-tail in-place mutator must
                            // take the functional-rebind interceptions (#782).
                            IrExprKind::Call { .. } if matches!(t.ty, Ty::Unit) => {
                                self.lower_stmt_expr(t)?
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
                                // The tuple-refinement unit chain (the Block-tail twin
                                // of the statement-Match hook — the heap-branch tail
                                // duplication lands the match HERE when the `let` was
                                // a block's last statement).
                                if !done {
                                    done = self.try_lower_tuple_refinement_unit_match(subject, arms);
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
                // `bytes.append_u8(buf, x)` is the SAME in-place byte push under another
                // name (`almide_rt_bytes_append_u8`) — identical rewrite.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "bytes"
                        && (func.as_str() == "push"
                            || func.as_str() == "append_u8"
                            // the MULTI-BYTE in-place appends: same rewrite, the
                            // functional twins live in bytes_append_multi.almd
                            || matches!(func.as_str(),
                                "append_u16_le" | "append_u16_be" | "append_i16_le"
                                | "append_i16_be" | "append_u32_le" | "append_u32_be"
                                | "append_i32_le" | "append_i32_be" | "append_i64_le"
                                | "append_i64_be" | "append_f32_le" | "append_f32_be"
                                | "append_f64_le" | "append_f64_be")
                            // the typed Endian-dispatch appends (bytes_typed.almd):
                            // same in-place v0 form, functional twin + rebind here.
                            // 3 args (buf, value, endian) — the receiver stays args[0].
                            || matches!(func.as_str(),
                                "write_uint16" | "write_uint32" | "write_int32"
                                | "write_float32"))
                        && matches!(args.len(), 2 | 3)
                        && matches!(&args[0].kind, IrExprKind::Var { .. }) =>
                {
                    let IrExprKind::Var { id } = &args[0].kind else { unreachable!() };
                    // push/append_u8 route to the 1-byte `bytes.append`; every
                    // multi-byte variant keeps its own name (its functional twin).
                    let fname = if matches!(func.as_str(), "push" | "append_u8") {
                        "append".to_string()
                    } else {
                        func.as_str().to_string()
                    };
                    let append = IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module {
                                module: sym("bytes"),
                                func: sym(&fname),
                                def_id: None,
                            },
                            // ALL args ride through — the typed Endian writes carry a
                            // third (endian) argument the functional twin dispatches on.
                            args: args.clone(),
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
                // `map.insert(m, k, v)` / `map.delete(m, k)` — v0 in-place map mutations:
                // same functional-rebind treatment as bytes.push (`m = map.set(m, k, v)` /
                // `m = map.remove(m, k)`); the repr dispatch then suffixes the self-host
                // (set_skv/msv/… , remove_skv/str) exactly like a source-level call.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "map"
                        && matches!(func.as_str(), "insert" | "delete")
                        && matches!(&args[0].kind, IrExprKind::Var { .. }) =>
                {
                    let IrExprKind::Var { id } = &args[0].kind else { unreachable!() };
                    let fname = if func.as_str() == "insert" { "set" } else { "remove" };
                    let call = IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module {
                                module: sym("map"),
                                func: sym(fname),
                                def_id: None,
                            },
                            args: args.clone(),
                            type_args: vec![],
                        },
                        ty: args[0].ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let assign =
                        IrStmt { kind: IrStmtKind::Assign { var: *id, value: call }, span: None };
                    self.lower_stmt(&assign)
                }
                // `list.push(xs, v)` / `string.push(s, x)` — v0 in-place appends: the
                // same functional-rebind treatment as bytes.push (`xs = xs + [v]` /
                // `s = s + x`); the ConcatList/ConcatStr lowering then emits the ONE
                // synthetic concat call the source Call node already credits (mir <= ir
                // holds). A `Var` receiver rebinds via Assign; a FIELD receiver
                // (`list.push(b.xs, v)` — the C-132 mut-param write-back shape) routes
                // through FieldAssign, whose spread rebind owns the write-back.
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if func.as_str() == "push"
                        && matches!(module.as_str(), "list" | "string")
                        && args.len() == 2
                        && (matches!(&args[0].kind, IrExprKind::Var { .. })
                            || matches!(&args[0].kind, IrExprKind::Member { object, .. }
                                if matches!(&object.kind, IrExprKind::Var { .. }))) =>
                {
                    let is_list = module.as_str() == "list";
                    let recv = args[0].clone();
                    let rhs = if is_list {
                        IrExpr {
                            kind: IrExprKind::List { elements: vec![args[1].clone()] },
                            ty: recv.ty.clone(),
                            span: None,
                            def_id: None,
                        }
                    } else {
                        args[1].clone()
                    };
                    let concat = IrExpr {
                        kind: IrExprKind::BinOp {
                            op: if is_list {
                                almide_ir::BinOp::ConcatList
                            } else {
                                almide_ir::BinOp::ConcatStr
                            },
                            left: Box::new(recv.clone()),
                            right: Box::new(rhs),
                        },
                        ty: recv.ty.clone(),
                        span: None,
                        def_id: None,
                    };
                    let stmt = match &recv.kind {
                        IrExprKind::Var { id } => {
                            IrStmt { kind: IrStmtKind::Assign { var: *id, value: concat }, span: None }
                        }
                        IrExprKind::Member { object, field } => {
                            let IrExprKind::Var { id } = &object.kind else { unreachable!() };
                            IrStmt {
                                kind: IrStmtKind::FieldAssign {
                                    target: *id,
                                    field: *field,
                                    value: concat,
                                },
                                span: None,
                            }
                        }
                        _ => unreachable!(),
                    };
                    self.lower_stmt(&stmt)
                }
                // `map.clear(m)` / `list.clear(xs)` — the in-place empty: rebind to the
                // EMPTY literal of the receiver's own type (adds no call; mir <= ir holds).
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if func.as_str() == "clear"
                        && matches!(module.as_str(), "map" | "list")
                        && args.len() == 1
                        && matches!(&args[0].kind, IrExprKind::Var { .. }) =>
                {
                    let IrExprKind::Var { id } = &args[0].kind else { unreachable!() };
                    let empty = if module.as_str() == "map" {
                        IrExpr {
                            kind: IrExprKind::EmptyMap,
                            ty: args[0].ty.clone(),
                            span: None,
                            def_id: None,
                        }
                    } else {
                        IrExpr {
                            kind: IrExprKind::List { elements: vec![] },
                            ty: args[0].ty.clone(),
                            span: None,
                            def_id: None,
                        }
                    };
                    let assign =
                        IrStmt { kind: IrStmtKind::Assign { var: *id, value: empty }, span: None };
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
        }
    }

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
    /// ASSIGN through a mutable module-level `var`'s storage slot. A scalar stores the
    /// value directly; a heap global builds the NEW value FIRST (so a self-referencing
    /// RHS — `items = items + [n]`, the #501 alias pin — reads the old block via its own
    /// owned `$__mg_get` Dup while the slot still holds it), then `$__mg_take`s the OLD
    /// block (the slot's owned reference transfers to us — a fresh-owned CallFn result,
    /// exactly the certificate's model), drops it by its type route, and stores+Consumes
    /// the new block (the record-slot move-in pattern). Inside the synthesized
    /// `__mg_init` the slot is still zero, so take+drop are SKIPPED (dropping handle 0
    /// would trap). A MODELED frame (the model-one-iteration `while` fallback / a
    /// non-executable branch arm) must WALL: both eliding and emitting a modeled global
    /// write diverge from v0 (the write is an EFFECT).
    fn lower_mutable_global_assign(
        &mut self,
        var: VarId,
        index: u32,
        gty: &Ty,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        use crate::PrimKind;
        if self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
            return Err(LowerError::Unsupported(format!(
                "assignment to mutable module-level var {var:?} inside a modeled (non-\
                 executable) frame — the global write is an effect the model would elide"
            )));
        }
        let in_mg_init = self.fn_name == "__mg_init";
        if !is_heap_ty(gty) {
            let src = self
                .lower_scalar_value(value)
                .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                .ok_or_else(|| {
                    LowerError::Unsupported(format!(
                        "non-scalar value assigned to mutable module-level var {var:?} \
                         outside the executable subset"
                    ))
                })?;
            let addr = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, src],
            });
            return Ok(());
        }
        // Heap global: NEW value first (RHS may read the global), as a fresh OWNED handle.
        let new = self.lower_owned_heap_field(value).ok_or_else(|| {
            LowerError::Unsupported(format!(
                "heap value assigned to mutable module-level var {var:?} outside the \
                 executable subset"
            ))
        })?;
        let repr = repr_of(gty)?;
        let addr = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
        if !in_mg_init {
            let old = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(old),
                name: "__mg_take".to_string(),
                args: vec![crate::CallArg::Scalar(addr)],
                result: Some(repr),
            });
            // Route the old block's drop by the global's TYPE (the same classification a
            // call-arg temp gets), then release it — its holders elsewhere (an in-flight
            // RHS `__mg_get` Dup) keep their own references, so this frees at last-ref only.
            self.materialized_call_arg(old, repr, gty);
            let drop_old = self.drop_op_for(old);
            self.ops.push(drop_old);
            self.live_heap_handles.retain(|v| *v != old);
        }
        let handle = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![new] });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 8 },
            dst: None,
            args: vec![addr, handle],
        });
        self.ops.push(Op::Consume { v: new });
        self.live_heap_handles.retain(|v| *v != new);
        Ok(())
    }

    pub(crate) fn value_or_global(&mut self, var: VarId) -> Result<ValueId, LowerError> {
        // A SHARED-CELL var (cells.rs) reads its cell slot FRESH on every reference —
        // checked BEFORE `value_of` (a cell var must never be cached: an intervening
        // closure call may have written the cell, exactly like a mutable global).
        if let Some(&cell) = self.cell_of.get(&var) {
            return self.lower_cell_read(var, cell);
        }
        if let Some(&v) = self.value_of.get(&var) {
            return Ok(v);
        }
        // A MUTABLE module-level `var`: read its STORAGE SLOT fresh on every reference
        // (never cached in `value_of` — an intervening write, ours or a callee's, must be
        // seen; materializing the const INITIALIZER instead was a probe-confirmed silent
        // miscompile: `5 3 0` vs native `5 8 8`). A scalar is a plain slot `Load`; a heap
        // global goes through `$__mg_get` (slot load + `rc_inc` — the returned handle is a
        // REAL owned reference, matching the certificate's fresh-owned CallFn result), is
        // routed for its type-correct scope-end drop, and its block is materialized-real
        // by construction (only real constructions are ever stored through `__mg_take`/
        // `Store`), so member reads and spreads work on it.
        if let Some((index, ty)) = crate::lower::mutable_global_info(var) {
            let addr = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(index) as i64 });
            if is_heap_ty(&ty) {
                let repr = repr_of(&ty)?;
                // BORROW the slot's handle then `Dup` it — the same borrow-then-Dup the
                // spread-record copy uses (cert `a`; the render's Dup IS the `rc_inc`),
                // so the function owns a real reference the slot's later reassignment
                // cannot invalidate. No call op is injected (the caps count stays exact).
                let borrowed = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::LoadHandle,
                    dst: Some(borrowed),
                    args: vec![addr],
                });
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src: borrowed });
                self.materialized_call_arg(dst, repr, &ty);
                self.materialized_aggregates.insert(dst);
                self.materialized_lists.insert(dst);
                return Ok(dst);
            }
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::Load { width: 8 },
                dst: Some(dst),
                args: vec![addr],
            });
            return Ok(dst);
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
            if let Some(init) = self.global_inits.get(&var).cloned() {
                // A global whose init is ANOTHER global (`let DIRECT = letlib.GREETING` —
                // the #632 alias-let): recurse through value_or_global on the SOURCE id (a
                // fresh owned copy of ITS const init, cached + dropped at scope end); this
                // reference aliases the same local copy (reads only — no second owner).
                // Zero calls injected (the source resolves through the same const-only
                // machinery), so the count gate stays exact.
                if let IrExprKind::Var { id: src } = &init.kind {
                    let src = *src;
                    // `let snapshot = counter` over a MUTABLE source: v0 evaluates the
                    // alias ONCE at startup; recursing here would read the slot's CURRENT
                    // value at each use — a divergence, so WALL the alias instead.
                    if crate::lower::is_mutable_global(src) {
                        return Err(LowerError::Unsupported(format!(
                            "global alias-let of a MUTABLE module-level var {src:?} (a \
                             startup snapshot) is not in this brick"
                        )));
                    }
                    if self.globals.contains_key(&src) {
                        let v = self.value_or_global(src)?;
                        self.value_of.insert(var, v);
                        return Ok(v);
                    }
                }
                if let Some(const_init) = const_global_init(&init) {
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
                if is_pure_literal_list(&init) {
                    if let Some(dst) = self.try_lower_str_list_literal(&init) {
                        self.live_heap_handles.push(dst);
                        self.value_of.insert(var, dst);
                        return Ok(dst);
                    }
                }
                // A RECORD-literal heap global (`let CFG = Cfg { name: "c" }` — the #502
                // spread/member base): call-free fields construct through the SAME builder
                // a local record `let` uses (`try_lower_record_construct` — allocs + stores
                // ONLY, zero `CallFn`, so the gate's `mir == ir` count stays exact), which
                // registers the record's own drop route. The fresh owned copy frees at
                // scope end; the real module global is untouched.
                if matches!(init.kind, IrExprKind::Record { .. })
                    && !crate::lower::expr_contains_call(&init)
                {
                    // A SCALAR-ONLY record global (`let _transparent = { r: 0.0, a: 0.0 }`)
                    // constructs through the scalar builder (the mixed builder defers it).
                    if let Some(dst) = self
                        .try_lower_record_construct(&init)
                        .or_else(|| self.try_lower_scalar_record_construct(&init))
                    {
                        if !self.live_heap_handles.contains(&dst) {
                            self.live_heap_handles.push(dst);
                        }
                        // The copy's slots are REAL — register it so member reads and
                        // `{ ...global, override }` spreads take the materialized path.
                        self.materialized_aggregates.insert(dst);
                        // A heap-nested record copy (`_default`'s `bg: Color` slot) frees
                        // via its recursive `$__drop_<R>` at scope end — the flat mask
                        // alone would leak a heap-IN-nested field on deeper shapes.
                        if let Some(name) = self.record_or_anon_drop_type_name(&ty) {
                            self.record_masks.remove(&dst);
                            self.variant_drop_handles.insert(dst, name);
                        }
                        self.value_of.insert(var, dst);
                        return Ok(dst);
                    }
                }
                // An OPTION-ctor heap global (`let MAYBE = some(Cfg { name: "opt" })` —
                // the crossmod option_record_toplet): a call-free `some(...)`/`none`
                // initializer builds through the SAME ctor builder a local `let o =
                // some(..)` uses (allocs + stores only, zero `CallFn` — the count gate
                // stays exact), which registers the Option's own drop route. Tracked in
                // `materialized_options` so a `match m.MAYBE { some(c) => … }` over the
                // fresh copy EXECUTES (reads the real len-as-tag).
                if matches!(init.kind, IrExprKind::OptionSome { .. } | IrExprKind::OptionNone)
                    && !crate::lower::expr_contains_call(&init)
                {
                    if let Some(dst) = self.try_lower_option_ctor(&init, &ty) {
                        if !self.live_heap_handles.contains(&dst) {
                            self.live_heap_handles.push(dst);
                        }
                        self.materialized_options.insert(dst);
                        self.value_of.insert(var, dst);
                        return Ok(dst);
                    }
                }
                // A LIST-OF-RECORDS heap global (`let CFGS = [Cfg { name: "a" }, Cfg { name:
                // "b" }]` — cross_module_toplet_byvalue's #486 list-of-records shape): a
                // call-free `List` initializer whose elements are all record ctors builds
                // through the SAME builder a local `let xs = [Cfg{..}, ..]` uses
                // (`try_lower_record_list_literal` — per-element `try_lower_record_construct`
                // MOVED into owned i64 slots, zero `CallFn`, so the gate's `mir == ir` count
                // stays exact), which registers the list's own recursive `$__drop_list_<R>`
                // drop route (each element freed via `$__drop_<R>`). The fresh owned copy
                // frees at scope end; the real module global is untouched.
                if matches!(init.kind, IrExprKind::List { .. })
                    && !crate::lower::expr_contains_call(&init)
                {
                    if let Some(dst) = self.try_lower_record_list_literal(&init) {
                        if !self.live_heap_handles.contains(&dst) {
                            self.live_heap_handles.push(dst);
                        }
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
        let (names, tys) = self.aggregate_field_tys(ty)?;
        // A GENERIC decl has no shared `$__drop_<R>` (the heap mask differs per
        // instantiation — `Pair[Int, String]` vs `Pair[String, Int]`): route to the
        // per-shape `__drop_anonrec_<hash>` over the SUBSTITUTED fields, the same
        // identity `collect_recursive_anon_records` registers for generation.
        if self.record_layouts.get(canonical.as_str()).is_some_and(|(gs, _)| !gs.is_empty()) {
            let pairs: Vec<(almide_lang::intern::Sym, Ty)> =
                names.into_iter().zip(tys.iter().cloned()).collect();
            if crate::lower::anon_record_needs_recursive_drop(&pairs) {
                return Some(crate::lower::anon_record_drop_name(&pairs));
            }
            return None;
        }
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
                Op::DropWrapperRec { v, drop_fn: drop_fn.to_string(), is_result: false, err_rec: false }
            } else if let Some(drop_fn) = ty.strip_prefix("resrec:") {
                // A Result WRAPPER holding a heap RECORD Ok payload (`ok({val, next})`): recurse into
                // the @12 record (tag@16==0) via `$__drop_<drop_fn>`, else `rc_dec` the @12 Err
                // String, then free the wrapper. Injected by `materialize_result_aggregate`.
                Op::DropWrapperRec { v, drop_fn: drop_fn.to_string(), is_result: true, err_rec: false }
            } else if let Some(drop_fn) = ty.strip_prefix("reserr:") {
                // The heap-Ok × variant-ERR wrapper (`Result[String, MathError]` — the
                // `err(NegativeInput(x))` class): recurse into the @12 VARIANT (tag@16==1)
                // via `$__drop_<drop_fn>`, else `rc_dec` the @12 Ok payload, then free the
                // wrapper. Injected by `try_lower_result_err_variant_ctor_heap_ok` and the
                // both-heap `seed_variant_param` branch (rich-variant Err types).
                Op::DropWrapperRec { v, drop_fn: drop_fn.to_string(), is_result: true, err_rec: true }
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
        } else if self.closure_values.contains(&v) {
            // A CLOSURE BLOCK frees through the uniform, SELF-DESCRIBING
            // `$__drop_closure` (fixed runtime): at the last ref it reads the drop
            // header (slot 1), recursively drops the captured-closure slots, rc_decs
            // the captured-heap slots, and NEVER touches slot 0 (the fnidx — a table
            // index, not a pointer). Works for any closure value regardless of where
            // it was created (a call-result's captures are unknowable here).
            Op::DropVariant { v, ty: "closure".to_string() }
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
