impl LowerCtx {
    /// Try to lower a SCALAR-result call to a REAL executable `CallFn` (arguments
    /// materialized via [`Self::lower_call_args`], the scalar result bound to a fresh
    /// `dst`), returning `Some(dst)`. Mirrors the heap Named/pure-`Module` call
    /// lowering MINUS the live-heap-handle — a scalar result carries no ownership
    /// (`Repr::Scalar` is not heap), so it is bound but never dropped.
    ///
    /// Returns `None` for a non-call value, an unresolvable `Method`/`Computed`
    /// callee, or a call whose args / module-purity are not resolvably executable —
    /// the caller then DEFERS it (a `Const` + an elided-caps marker), exactly as
    /// before. A partial-then-failed lowering rolls back its pushed ops/handles, so
    /// the deferred path starts clean. This can NEVER turn an in-profile function
    /// `Unsupported` (the deferral is always available) — the in-profile set and the
    /// caps fold are preserved: a real `CallFn` replaces the elided marker 1:1 (same
    /// callee NAME, so `reachable_caps` is unchanged; same op count, so the
    /// `mir_calls <= ir_calls` gate cannot falsely de-taint).
    /// If `callee` names a local bound to a CLOSURE BLOCK (a lifted lambda, a
    /// function-typed param, or a function-valued call result — recorded in
    /// `closure_values`), return that block value — what a `CallIndirect` dispatches
    /// through. Returns `None` for any other computed callee (an unanalyzable value),
    /// so the caller keeps the sound deferred model for those.
    pub(crate) fn closure_value_of(&self, callee: &IrExpr) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &callee.kind {
            if let Some(v) = self.value_of.get(id) {
                if self.closure_values.contains(v) {
                    return Some(*v);
                }
            }
        }
        None
    }

    /// The MUTABLE widening of [`Self::closure_value_of`]: additionally loads a
    /// RECORD-SLOT closure (`(h.run)(...)` — the record_fn_field class, B8's
    /// `Computed(Member)` desugar) as a BORROW of the slot handle. The record keeps
    /// ownership (its generated `__drop_<R>` frees the block via `__drop_closure`);
    /// the borrow joins `param_values` (never dropped here) + `closure_values` (the
    /// dispatch tracking). Emits the LoadHandle, so `&mut self`.
    /// PURE guard twin of [`Self::closure_block_of_mut`]'s Member arm — usable in
    /// match-arm guards (no emission): is the callee a Fn-typed record-slot Member?
    pub(crate) fn is_fn_member_callee(callee: &IrExpr) -> bool {
        matches!(&callee.kind, IrExprKind::Member { .. })
            && matches!(callee.ty, almide_lang::types::Ty::Fn { .. })
    }

    pub(crate) fn closure_block_of_mut(&mut self, callee: &IrExpr) -> Option<ValueId> {
        if let Some(v) = self.closure_value_of(callee) {
            return Some(v);
        }
        if let IrExprKind::Member { object, field } = &callee.kind {
            if matches!(callee.ty, almide_lang::types::Ty::Fn { .. }) {
                let offset = self.aggregate_field_offset_any(&object.ty, field.as_str())?;
                let h = self.resolve_aggregate_container_handle(object)?;
                let p = self.load_at_offset(h, offset as i64, crate::PrimKind::LoadHandle);
                self.param_values.insert(p);
                self.closure_values.insert(p);
                return Some(p);
            }
        }
        None
    }

    /// Load a closure block's table index (slot 0) — the scalar a `call_indirect`
    /// wraps to its i32 table offset. Rung-5 closures slab: the read goes through the
    /// TARGET-NEUTRAL `Op::ListGetScalar` (wasm: the bounds-checked element load;
    /// native: `blk[0]`) — no ownership event (the block is live — the caller holds it).
    pub(crate) fn emit_closure_fnidx(&mut self, blk: ValueId) -> ValueId {
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let idx = self.fresh_value();
        self.ops.push(Op::ListGetScalar { dst: idx, list: blk, idx: zero });
        idx
    }

    /// Emit a `CallIndirect` THROUGH a closure block: fnidx from slot 0, the block
    /// itself as the leading BORROWED env argument (the lifted lambda's prologue
    /// reads its captures back out of it), then the user args.
    pub(crate) fn emit_closure_call(
        &mut self,
        blk: ValueId,
        dst: Option<ValueId>,
        user_args: Vec<CallArg>,
        result: Option<crate::Repr>,
    ) {
        let table_idx = self.emit_closure_fnidx(blk);
        let mut args = vec![CallArg::Handle(blk)];
        args.extend(user_args);
        self.ops.push(Op::CallIndirect { dst, table_idx, args, result });
    }

    pub(crate) fn try_lower_scalar_call(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match &value.kind {
            // A scalar call THROUGH a lifted lambda value (`let y = f(5)` where `f` bound a
            // non-capturing lambda ⇒ an `Op::FuncRef`). The callee resolves to a funcref
            // value, so this lowers to `Op::CallIndirect` and the closure EXECUTES — args
            // materialized like any call, the scalar result bound. A Computed callee that is
            // NOT a known funcref returns `None` and DEFERS (the existing model). The MIR
            // CallIndirect is a genuine call (the corpus gate counts it), so it replaces the
            // elided Computed 1:1 — no spurious caps taint, no `mir > ir` breach.
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } => {
                // C1 DIRECT-CALL INLINE: a `f(args)` whose callee `f` is a statically-known
                // let-bound INLINE lambda is DEFUNCTIONALIZED — the body is lowered inline with
                // params bound to args, captures resolved through `value_of`. Tried FIRST (a
                // capturing lambda has no FuncRef slot; even a liftable one prefers inline — a
                // direct call edge is more sound for caps than a CallIndirect). Returns None →
                // the CallIndirect / defer path below.
                if !is_heap_ty(ty) {
                    if let Some(v) = self.try_inline_direct_lambda_call(callee, args, ty) {
                        return Some(v);
                    }
                    // The inline attempt rolls itself back on failure (its own marks), so the
                    // op stream is clean here for the CallIndirect fallback.
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                }
                let blk = self.closure_value_of(callee)?;
                let repr = repr_of(ty).ok()?;
                match self.lower_call_args(args) {
                    Ok(lowered) => {
                        let dst = self.fresh_value();
                        self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
                        Some(dst)
                    }
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            // A scalar `Named` user call (`fn f() = g()`, `let n = add(2, 3)`).
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let repr = repr_of(ty).ok()?;
                match self.lower_call_args(args) {
                    Ok(lowered) => {
                        let dst = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(dst),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(repr),
                        });
                        Some(dst)
                    }
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            // A scalar first-order PURE `Module` call (`let n = string.len(s)`): the
            // purity / higher-order gate is inside `lower_pure_module_value_call`; an
            // impure/HO/unsupported call errors → roll back and defer (no new wall).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                match self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, ty) {
                    Ok(dst) => Some(dst),
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// C1 DIRECT-CALL INLINE — defunctionalize a `f(args)` whose callee `f` is a
    /// statically-known let-bound INLINE lambda (`let f = (x) => body`). The body is
    /// lowered INLINE with each param bound to its lowered argument value; the lambda's
    /// CAPTURES (free vars like the `s` in `(x) => string.len(s) + x`) resolve through the
    /// EXISTING `value_of` map — they are in scope at the call site, so no env block and no
    /// substitution are needed. NO runtime closure, NO `CallIndirect`, NO lifted function:
    /// a static call graph, the inlined body's calls are REAL IR call nodes the caps fold
    /// and `count_ir_calls` see in place.
    ///
    /// SCALAR result only (this slice): the body lowers via `lower_scalar_value` (a
    /// Var/literal/arith/scalar-call/`string.len`-style pure-module call), which is
    /// rollback-safe (it restores `ops` + `live_heap_handles` on a partial miss). A heap
    /// result, or a body the scalar subset cannot lower (a side effect, a heap op), returns
    /// `None` and the caller keeps the existing CallIndirect / deferred path. Each ARGUMENT
    /// is lowered as a scalar value (a literal/Var/arith); a non-scalar-lowerable arg →
    /// `None` (defer). Self-contained marks make a partial attempt fully reversible.
    ///
    /// SOUNDNESS: param binding is `value_of[param] = arg_value` (a pure local rebind, no
    /// ownership event — a SCALAR arg carries none); the body lowers exactly as if its
    /// statements/expr were written at the call site. The captures are BORROWED through
    /// `value_of` (no new owner — the enclosing binding still owns `s`, dropped once at its
    /// own scope end), so no double-free. NB: a parameter VarId is UNIQUE per lambda (the
    /// frontend assigns fresh VarIds), so binding it cannot clobber a live caller local.
    fn try_inline_direct_lambda_call(
        &mut self,
        callee: &IrExpr,
        args: &[IrExpr],
        ty: &Ty,
    ) -> Option<ValueId> {
        // The callee must be a Var statically bound to a recorded inline lambda.
        let callee_var = match &callee.kind {
            IrExprKind::Var { id } => *id,
            _ => return None,
        };
        let (params, body) = self.lambda_bindings.get(&callee_var)?.clone();
        if params.len() != args.len() {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Lower each ARGUMENT to a scalar value, then bind the param to it. (A heap arg is
        // out of this slice — it would need owned/borrow tracking; defer.)
        for ((pvar, pty), arg) in params.iter().zip(args.iter()) {
            if is_heap_ty(pty) {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
            match self.lower_scalar_value(arg) {
                Some(v) => {
                    self.value_of.insert(*pvar, v);
                }
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        // Lower the lambda BODY inline as a scalar value (captures resolve through value_of).
        match self.lower_scalar_value(&body) {
            Some(v) if !is_heap_ty(ty) => Some(v),
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// C1 HEAP DIRECT-CALL INLINE — the heap-result twin of [`Self::try_inline_direct_lambda_call`].
    /// Defunctionalize a `f(args)` whose callee `f` is a statically-known let-bound INLINE lambda
    /// RETURNING A HEAP value (a String, …). The body is lowered INLINE to a FRESH OWNED heap value
    /// tracked in `live_heap_handles` for a single scope-end drop (cert `i…d`), via
    /// [`Self::lower_inline_lambda_heap_body`] over the existing owned-heap-value machinery — exactly
    /// as if the lambda's body expression were written at the call site. Returns the tracked owned
    /// `ValueId`, or `None` (fully rolled back: ops + handles restored) when a param or the body is
    /// outside the executable subset — the caller then keeps its sound defer/wall.
    ///
    /// SOUNDNESS: each PARAM is BOUND through `value_of` to the lowered argument — a SCALAR arg is a
    /// value (no ownership), a HEAP-Var arg is BORROWED (`value_for` — the caller still owns it,
    /// dropped once at its own scope end), so no new owner and no double-free; this is the same
    /// borrow the captures already use. A param VarId is UNIQUE per lambda, so binding it cannot
    /// clobber a live caller local. The body produces ONE distinct owned heap value (a `Dup` of a
    /// borrowed param/Var is a fresh independent reference), identical to the proven owned-heap-field
    /// lowering. A heap arg that is not a simple Var (a fresh call / literal) is out of this slice →
    /// `None` (defer), conservative.
    pub(crate) fn try_inline_direct_lambda_call_heap(
        &mut self,
        callee: &IrExpr,
        args: &[IrExpr],
        ty: &Ty,
    ) -> Option<ValueId> {
        if !is_heap_ty(ty) {
            return None;
        }
        let callee_var = match &callee.kind {
            IrExprKind::Var { id } => *id,
            _ => return None,
        };
        let (params, body) = self.lambda_bindings.get(&callee_var)?.clone();
        if params.len() != args.len() {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Bind each param to its argument value: a SCALAR arg is lowered as a value; a HEAP arg is
        // admitted ONLY as a Var, BORROWED via `value_for` (no ownership event). Anything else defers.
        for ((pvar, pty), arg) in params.iter().zip(args.iter()) {
            let bound = if is_heap_ty(pty) {
                match &arg.kind {
                    IrExprKind::Var { id } => self.value_for(*id).ok(),
                    _ => None,
                }
            } else {
                self.lower_scalar_value(arg)
            };
            match bound {
                Some(v) => {
                    self.value_of.insert(*pvar, v);
                }
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        match self.lower_inline_lambda_heap_body(&body) {
            Some(v) => Some(v),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Lower a let-lambda BODY (inlined at a call site) to a FRESH OWNED heap value tracked in
    /// `live_heap_handles` for a single scope-end drop. A BLOCK body lowers its statements as effects
    /// in a per-block frame (their heap let-locals ride into the frame), then its tail recursively,
    /// then DROPS the block's own inner heap lets (everything tracked since the frame mark) while
    /// KEEPING the tail's owned value — exactly the per-arm `drop_arm_locals` discipline, but the
    /// tail VALUE survives (it is a distinct reference, freed once at the caller's scope end). Any
    /// other body kind delegates to [`Self::lower_owned_heap_field`] (LitStr / concat / `${interp}` /
    /// a Dup'd Var / a Member borrow / a Named or pure-Module call / a heap-result `if` / `match` /
    /// Option·Result ctor — all of which produce a fresh owned tracked value). `None` rolls the
    /// caller back to its sound defer/wall.
    fn lower_inline_lambda_heap_body(&mut self, body: &IrExpr) -> Option<ValueId> {
        match &body.kind {
            IrExprKind::Block { stmts, expr } => {
                let tail = expr.as_deref()?;
                let mark = self.live_heap_handles.len();
                self.in_frame += 1;
                let mut ok = true;
                for stmt in stmts {
                    if self.lower_stmt(stmt).is_err() {
                        ok = false;
                        break;
                    }
                }
                let obj = if ok { self.lower_inline_lambda_heap_body(tail) } else { None };
                self.in_frame -= 1;
                let obj = obj?;
                // Drop the block's inner heap lets (LIFO) EXCEPT the tail value `obj`, then re-track
                // `obj` so the caller's scope-end drop frees it exactly once.
                let frame = self.live_heap_handles.split_off(mark.min(self.live_heap_handles.len()));
                for v in frame.into_iter().rev() {
                    if v != obj {
                        let op = self.drop_op_for(v);
                        self.ops.push(op);
                    }
                }
                self.live_heap_handles.push(obj);
                Some(obj)
            }
            _ => self.lower_owned_heap_field(body),
        }
    }

    /// Lower a SCALAR `Int` expression to a `ValueId` holding its REAL value (the
    /// scalar-value foundation): a Var/param, an `Int` literal (`ConstInt`), or an
    /// `Int` Add/Sub/Mul (`IntBinOp` over recursively-lowered operands). Returns
    /// `None` for anything outside this subset (Div/Mod/Pow, comparisons, logic,
    /// Float, calls, …) — the caller then DEFERS the value (`Const`). It pushes only
    /// `ConstInt`/`IntBinOp` (never a heap handle / ownership event), so a caller can
    /// roll back a partial attempt by truncating `self.ops`. The cert is unaffected:
    /// `IntBinOp`/`ConstInt` are no-ops for ownership and already define their `dst` /
    /// use their operands for the name witness.
    /// Lower a SCALAR field/element PROJECTION (`r.x`, `t.0`) to a real `Prim::Load`
    /// at the field's layout slot — the v1 VALUE MODEL read side. Returns the loaded
    /// scalar `dst`, or `None` (defer/wall) when the projection is not in the
    /// materialized subset:
    ///   - the container is not a TRACKED heap var (`f().x`, a nested `a.b.c` — no
    ///     single block to load from),
    ///   - the container's type is not a SCALAR-only record/tuple (a heap-field
    ///     aggregate is constructed as a deferred `Opaque`, whose slots are NOT the
    ///     layout offsets, so loading would read garbage — walled instead),
    ///   - the field is heap-typed (a String field — handled by the container-grain
    ///     `lower_heap_extraction`, not a scalar load).
    ///
    /// SOUNDNESS: a pure `Prim::Load` reads a copy of the scalar — no ownership event
    /// (the container keeps its single reference, dropped once at scope end). The gate
    /// on a MATERIALIZED scalar-aggregate container is what makes the offset correct:
    /// a deferred `Opaque` record never reaches here (its type would still be a
    /// scalar-aggregate, but it was never built with field stores — see below).
    /// The DECLARATION-ordered scalar field types of an aggregate container type, for
    /// the VALUE MODEL: a `Ty::Record`/`Ty::Tuple` is structural (used directly), a
    /// `Ty::Named(name, args)` is resolved via the [`LowerCtx::record_layouts`] registry,
    /// substituting the declared generic params with `args` (so a `Box[Int]` field
    /// `value: T` is sized as `Int` — the #650 instantiated-layout concern). Returns
    /// `None` for a non-aggregate / unregistered / arity-mismatched type (the caller
    /// then walls). The field NAMES are returned alongside so a `.field` access can find
    /// its index; a tuple has positional "fields" so its names are empty.
    pub(crate) fn aggregate_field_tys(&self, ty: &Ty) -> Option<(Vec<almide_lang::intern::Sym>, Vec<Ty>)> {
        match ty {
            Ty::Record { fields } => {
                Some((fields.iter().map(|(n, _)| *n).collect(), fields.iter().map(|(_, t)| t.clone()).collect()))
            }
            Ty::Tuple(elems) => Some((Vec::new(), elems.clone())),
            Ty::Named(name, args) => {
                // A cross-module type may arrive with its BARE spelling (`Lin`) while the
                // registry keys the QUALIFIED decl name (`types_mod.Lin`) — resolve through
                // the unique-suffix canonicalizer (ambiguous bare names stay unresolved).
                let key = crate::lower::canonical_record_key(&self.record_layouts, name.as_str())?;
                let (generics, decl_fields) = self.record_layouts.get(key)?;
                // Substitute the declared generic params (`T`, `A`, …) with the concrete
                // `args` from the instantiated type. A param with no supplied arg (arity
                // mismatch) is a resolution failure → wall.
                let mut subst: std::collections::HashMap<almide_lang::intern::Sym, Ty> =
                    std::collections::HashMap::new();
                for (g, a) in generics.iter().zip(args.iter()) {
                    subst.insert(*g, a.clone());
                }
                let names = decl_fields.iter().map(|(n, _)| *n).collect();
                let tys = decl_fields
                    .iter()
                    .map(|(_, t)| subst_type_var(t, &subst))
                    .collect();
                Some((names, tys))
            }
            _ => None,
        }
    }

    /// The uniform-slot BYTE OFFSET of a named field, resolving the concrete field types
    /// first — NOT walling a heap-field aggregate (the layout is one i64 slot per field
    /// regardless of field-ness, so a heap field's slot is at the same
    /// `BLOCK_HEADER + idx*SLOT_SIZE` a scalar field's is). A SCALAR read at this offset
    /// (`r.n` of `{name: String, n: Int}`) loads its value; a heap read (`b.label`) loads
    /// the slot's owned handle. `None` if `ty` is unresolvable or has no such field.
    pub(crate) fn aggregate_field_offset_any(&self, ty: &Ty, field: &str) -> Option<u32> {
        let (names, _tys) = self.aggregate_field_tys(ty)?;
        let idx = names.iter().position(|n| n.as_str() == field)?;
        Some(layout::slot_offset(idx))
    }

    /// The uniform-slot BYTE OFFSET of a tuple element by index, NOT walling a heap-element
    /// tuple (the tuple sibling of [`Self::aggregate_field_offset_any`]).
    pub(crate) fn aggregate_index_offset_any(&self, ty: &Ty, index: usize) -> Option<u32> {
        if !matches!(ty, Ty::Tuple(_)) {
            return None;
        }
        let (_, tys) = self.aggregate_field_tys(ty)?;
        if index >= tys.len() {
            return None;
        }
        Some(layout::slot_offset(index))
    }

    /// Resolve an aggregate CONTAINER expression to the i64 BYTE-ADDRESS of its block (the base
    /// for a `base + slot_offset` field load). A `Var` bound to a tracked heap aggregate (or a
    /// param-bound aggregate) is `Prim::Handle`'d directly. A NESTED aggregate field (`o.p` in
    /// `o.p.x`) is borrowed via `try_lower_heap_field_borrow` (the loaded inner-block handle) then
    /// `Prim::Handle`'d — so field access composes to arbitrary depth over materialized blocks.
    /// `None` for a non-resolvable container (`f().x`, a non-materialized var) → the caller defers.
    pub(crate) fn resolve_aggregate_container_handle(&mut self, container: &IrExpr) -> Option<ValueId> {
        use crate::PrimKind;
        let block = self.resolve_aggregate_container_block(container)?;
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![block] });
        Some(h)
    }

    /// The container's BLOCK value (pre-`Handle`) — the form the target-neutral
    /// list/record ops take (`Op::ListGetScalar` resolves its own address).
    pub(crate) fn resolve_aggregate_container_block(&mut self, container: &IrExpr) -> Option<ValueId> {
        let block = match &container.kind {
            IrExprKind::Var { id } if is_heap_ty(&container.ty) => self.value_or_global(*id).ok()?,
            // A nested aggregate field — borrow its loaded inner-block handle. Gated on the
            // OUTER container being materialized (inside `try_lower_heap_field_borrow`), so a
            // garbage slot is never dereferenced.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if is_heap_ty(&container.ty) =>
            {
                self.try_lower_heap_field_borrow(container)?
            }
            // A list-ELEMENT aggregate (`line.items[ii].idx` — the chained scalar read off
            // an indexed record): borrow the element block via the same bounds-checked
            // `$elem_addr` LoadHandle the for-in element borrow uses (gated on a tracked/
            // field-borrowable list container at each level).
            IrExprKind::IndexAccess { .. } if is_heap_ty(&container.ty) => {
                self.try_lower_heap_field_borrow(container)?
            }
            // A CALL-result container (`mk_paren().name` — the paren-ctor scalar field read):
            // ANF-materialize the call to a synthetic temp via the SAME `lower_bind` path a
            // `let tmp = mk_paren()` takes (tracked, recursive scope-end drop, read shapes
            // seeded), then resolve the temp — the exact mirror of `lower_heap_extraction`'s
            // Call arm on the scalar-field side.
            IrExprKind::Call { .. } if is_heap_ty(&container.ty) => {
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &container.ty, container).ok()?;
                self.value_for(tmp).ok()?
            }
            _ => return None,
        };
        Some(block)
    }

    pub(crate) fn lower_scalar_field_access(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        // Scalar result only (the caller's contract; a heap field defers to the
        // container-grain extraction).
        if is_heap_ty(&expr.ty) {
            return None;
        }
        // Use the NON-WALLING offset: a SCALAR field of a MIXED heap-field record/tuple is
        // at the same uniform slot a scalar-only record's is (one i64 slot per field), so
        // `R { name: String, n: Int }.n` reads slot 1 correctly. The result is scalar
        // (guarded above), so loading it (load64) is right regardless of the OTHER fields'
        // heap-ness; the only requirement is the container is materialized with this layout
        // (the tracked-heap-var guard below), which a heap-field record now is.
        let (container, offset) = match &expr.kind {
            IrExprKind::Member { object, field } => {
                (object, self.aggregate_field_offset_any(&object.ty, field.as_str())?)
            }
            IrExprKind::TupleIndex { object, index } => {
                (object, self.aggregate_index_offset_any(&object.ty, *index)?)
            }
            _ => return None,
        };
        // Resolve the container to a block handle: a TRACKED heap var (a `try_lower_*_construct`
        // block or a param-bound aggregate), OR a NESTED aggregate field (`o.p` of `o.p.x`) whose
        // borrowed handle points to the inner block. A non-resolvable container (`f().x`) → defer.
        let block = self.resolve_aggregate_container_block(container)?;
        // Rung-5 records slab: the aggregate block IS a scalar list, so the slot read
        // is the TARGET-NEUTRAL `Op::ListGetScalar` with the DECLARATION-order slot
        // index (byte offset 12 + 8*slot ⇒ slot) — wasm renders the bounds-checked
        // element load (always in range: len = field count), native `rec[slot]`.
        // The stored scalar (any width) round-trips losslessly through the i64 slot.
        let slot = (offset - crate::lower::layout::BLOCK_HEADER) / crate::lower::layout::SLOT_SIZE;
        let idx = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: idx, value: slot as i64 });
        let dst = self.fresh_value();
        self.ops.push(Op::ListGetScalar { dst, list: block, idx });
        Some(dst)
    }

    /// Lower a SCALAR direct index `xs[i]` (`xs: List[Int/Float/Bool]`, scalar i64 element slots)
    /// to a bounds-checked element load: `prim.handle(xs)` → `$elem_addr(list, i)` (the preamble
    /// helper that TRAPs on a negative / `>= cap` index — v0's `a[i]` likewise halts on OOB) →
    /// `Load { width: 8 }` of the i64 slot. The element round-trips losslessly (a narrow Int8 / a
    /// Float's f64 bits read back exact). GATED to a SCALAR result element AND a resolvable heap
    /// container var (a tracked List) AND a lowerable scalar index; a heap-element list (an
    /// i32-handle slot) or an unresolvable container defers to the caller's safe fallback. The
    /// container is BORROWED (read-only handle), no ownership — `lower_scalar_value`'s contract
    /// (only rollback-safe value ops, never an ownership event) holds.
    pub(crate) fn lower_scalar_index_access(
        &mut self,
        object: &IrExpr,
        index: &IrExpr,
        elem_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // Scalar element only — a heap element (List[String]) needs a borrowing LoadHandle path,
        // handled by the heap-extraction lowering, not here.
        if is_heap_ty(elem_ty) {
            return None;
        }
        // The container must be a tracked heap list VAR that is a REAL, POPULATED block (in
        // `materialized_lists` — a literal / heap param / fully-lifted self-host list result) OR a
        // borrowed heap PARAM (the caller passes a genuine list). An Opaque/deferred list (a
        // `list.map` whose param-invoking lambda could not lift → an empty block, cap 0) is NOT
        // admitted: a bounds-checked `$elem_addr` load would TRAP at `xs[0]` (cap 0), a new runtime
        // crash. Such a list defers to the caller's safe `Const 0` fallback (mis-valued, never a trap).
        let list = match &object.kind {
            IrExprKind::Var { id } if is_heap_ty(&object.ty) => {
                let v = self.value_or_global(*id).ok()?;
                if !self.materialized_lists.contains(&v) && !self.param_values.contains(&v) {
                    return None;
                }
                v
            }
            // `line.items[ii]` — the scalar-element list is ITSELF a heap field/element
            // of a materialized aggregate (the ceangal resolve_line_flex class): borrow
            // the list block through the same gated field-borrow chain the heap-element
            // read uses (materialization checked at every level).
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. }
                if is_heap_ty(&object.ty) =>
            {
                self.try_lower_heap_field_borrow(object)?
            }
            _ => return None,
        };
        let idx = self.lower_scalar_value(index)?;
        // ONE target-neutral bounds-checked element load (rung 4): the wasm render
        // expands to the exact `$elem_addr_chk` + `i64.load` the inline
        // Handle/ElemAddr/Load sequence produced; the native leg maps to `v[i]`.
        let dst = self.fresh_value();
        self.ops.push(Op::ListGetScalar { dst, list, idx });
        Some(dst)
    }

    /// Lower a SCALAR (Int/Bool/Float) value expression to a `ValueId` holding its REAL
    /// value, or `None` (the caller then DEFERS to `Const`). SELF-ROLLBACK contract: on a
    /// `None` return this restores BOTH `self.ops` AND `self.live_heap_handles` to their
    /// entry length, so the function leaves NO net side effect when it fails — a caller may
    /// roll back with an `ops`-only truncate (the historic discipline) and still be correct
    /// even though a sub-lowering (a scalar CALL OPERAND, `5 + string.len("abc")`) may
    /// MATERIALIZE a fresh heap argument temp (an `Alloc` registered for a scope-end drop).
    /// On SUCCESS, any such temp stays tracked (it is a genuine value to free at scope end),
    /// exactly as a direct `let _ = string.len("abc")` bind tracks it. The actual lowering
    /// is [`Self::lower_scalar_value_inner`].
    pub(crate) fn lower_scalar_value(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => Some(v),
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Lower a scalar operand of an EAGER `UnOp` (`not e`) or logical `And`/`Or`, FREEING any
    /// transient heap temp the operand materializes WITHIN a local frame. The canonical case is
    /// `c == "'"` (→ `string.eq(c, "'")`): the `"'"` literal is a fresh owned String that is dead
    /// the instant the `Bool` is computed, so it is `Alloc`'d (cert `i`) and `Drop`'d (cert `d`)
    /// LOCALLY here — the operand is internally balanced and registers NO temp in the enclosing
    /// frame. This is SOUND precisely because `and`/`or`/`not` are EAGER in v0 (both operands /
    /// the operand always evaluate, NO short-circuit), so the `Drop` always runs on the same path
    /// as the `Alloc`; the scalar `Bool` result survives the frame teardown (it is not a heap
    /// handle). Returns `None` (fully rolled back) if the operand is not scalar-lowerable. (Before,
    /// a heap-materializing operand was GATED OUT to `None` → the caller fell back to a silent
    /// `Const 0` / a `WALL` — the `not (c == "'" or c == "\"")` miscompile.)
    fn lower_scalar_operand(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let frame = self.live_heap_handles.len();
        match self.lower_scalar_value_inner(expr) {
            Some(v) => {
                // Free any transient temp the operand allocated (e.g. a string-eq literal),
                // keeping the operand internally `i…d`-balanced — the scalar `v` is not among them.
                self.drop_arm_locals(frame);
                Some(v)
            }
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(frame);
                None
            }
        }
    }
}
