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
                        // Guard-clause flattening (`return` targets `seed_variant_param` — sound
                        // because this is the tail of its `match ty { .. }`, the function's last
                        // statement, so returning early here is identical to falling through to
                        // the function's end). No behavior change.
                        let Some(vn) = self.custom_variant_type_name(&a[1]) else {
                            return;
                        };
                        if !self.variant_layouts.needs_recursive_drop(&vn, &|rn| {
                            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                        }) {
                            return;
                        }
                        self.variant_drop_handles.insert(v, format!("reserr:{vn}"));
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
}
