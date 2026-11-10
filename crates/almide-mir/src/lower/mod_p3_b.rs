impl LowerCtx {

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
                if !self.value_of.contains_key(&target) {
                    if let Some(g) = crate::lower::mutable_global_info(target) {
                        return self.lower_mutable_global_index_assign(target, index, value, g);
                    }
                }
                if let Some(&cell) = self.cell_of.get(&target).filter(|_| crate::lower::owned_tco_slots() && !is_heap_ty(&value.ty)) {
                    return self.lower_cell_index_assign(target, cell, index, value);
                }
                // A HEAP String/Value element (`xs[0] = "Z"`, the C-136 case-5 shape) takes
                // the functional REBIND `xs = list.set(xs, i, v)` BEFORE any COW guard (#2260):
                // the rc-correct `_str`/`_value` twin copies and the Assign path swaps the
                // local. NO MakeUnique first — as for the heap FIELD write below, the guard
                // composed with the Assign's drop-old into an rc-underflow trap on an alias
                // (`let ys = xs; xs[1] = "B"` trapped on the wasm test lane, `List[Int]` passed).
                if matches!(&value.ty, Ty::String) || crate::lower::is_value_ty(&value.ty)
                    || (crate::lower::owned_tco_slots() && is_heap_ty(&value.ty)) {
                    return self.lower_heap_elem_index_rebind(target, index, value);
                }
                // COW-guard the buffer (rebinds the local to a unique copy if shared), then ACTUALLY
                // STORE: `xs[i] = v` → `i64.store($elem_addr(handle(xs), i), v)`. WITHOUT the store the
                // assignment lowered to ONLY the MakeUnique guard (a silent no-op — `xs[1] = 99` never
                // wrote; v1-spine hole #29). The `$elem_addr` is bounds-checked (traps OOB, matching
                // native's panic). The store runs AFTER MakeUnique so it writes the unique copy.
                self.lower_place_mutation(target, value.span)?;
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

    /// A mutable-GLOBAL place target: `g[i] = v` routes through the slot as
    /// TAKE (the slot's owned ref transfers to us) → `MakeUnique` (COW if a
    /// reader's Dup is still live — the mutation must touch no alias) →
    /// bounds-checked element store → STORE-BACK (+`Consume`) of the possibly-
    /// copied block. Going through `lower_place_mutation` instead would COW the
    /// read-Dup and write the COPY — the global would silently keep the old
    /// value. SCALAR-element lists only this round (the #29 store subset);
    /// heap-element / non-scalar shapes WALL, as does a modeled frame (the
    /// write is an effect the model would elide).
    fn lower_mutable_global_index_assign(
        &mut self,
        target: VarId,
        index: &IrExpr,
        value: &IrExpr,
        global: (u32, Ty),
    ) -> Result<(), LowerError> {
        let (gindex, gty) = global;
        if self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
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
        let (idx, val) = match (self.lower_scalar_value(index), self.lower_scalar_value(value)) {
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
        self.ops.push(Op::ConstInt { dst: addr, value: crate::mg_slot_addr(gindex) as i64 });
        self.store_scalar_elem_through_slot(addr, (repr, &gty), idx, val);
        Ok(())
    }

    /// The shared-cell twin of [`Self::lower_mutable_global_index_assign`]
    /// (native owned slots): `xs[i] = v` over a cell holding a scalar-element
    /// list takes the cell's block, makes it unique, stores the element and
    /// stores the block back — the cell stays the one place the list lives.
    fn lower_cell_index_assign(
        &mut self,
        target: VarId,
        cell: ValueId,
        index: &IrExpr,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        let wall = |what: &str| LowerError::at(value.span, format!("index-assign to shared-cell var {target:?} {what}"));
        if self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
            return Err(wall("inside a modeled (non-executable) frame"));
        }
        let ty = self.var_decl_tys.get(&target).cloned().ok_or_else(|| wall("with no recorded type"))?;
        if !crate::lower::is_heap_ty(&ty) || crate::lower::is_heap_elem_list_ty(&ty) {
            return Err(wall("outside the scalar-element subset"));
        }
        let (Some(idx), Some(val)) = (self.lower_scalar_value(index), self.lower_scalar_value(value)) else {
            return Err(wall("with a non-lowerable index/value"));
        };
        let repr = repr_of(&ty)?;
        let addr = self.cell_slot_addr(cell);
        self.store_scalar_elem_through_slot(addr, (repr, &ty), idx, val);
        Ok(())
    }

    /// TAKE the list out of the storage slot at `addr`, make it unique, store
    /// the scalar element, and STORE the (possibly copied) block BACK.
    fn store_scalar_elem_through_slot(&mut self, addr: ValueId, (repr, ty): (crate::Repr, &Ty), idx: ValueId, val: ValueId) {
        let taken = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(taken),
            name: "__mg_take".to_string(),
            args: vec![crate::CallArg::Scalar(addr)],
            result: Some(repr),
        });
        self.materialized_call_arg(taken, repr, ty);
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
    }

    /// Heap element index-assign (String/Value on wasm, typed heaps on native):
    /// emit the C-067 bounds guard
    /// then desugar to the functional rebind `xs = list.set(xs, i, v)`.
    ///
    /// C-067 (#807): the index-operator WRITE must abort on OOB
    /// exactly like the read — but the functional `list.set` rebind
    /// is contractually a NO-OP on OOB (C-054), so the wasm
    /// leg silently kept the old list where native aborted. Emit
    /// the user-facing bounds guard FIRST: `$elem_addr_chk` traps
    /// with the native-identical message + exit 1; its address
    /// result is unused — the trap is the point. A non-lowerable
    /// index cannot be guarded: STRICT mode refuses (this exact
    /// silent-divergence class), the permissive classifier keeps
    /// the prior behavior.
    fn lower_heap_elem_index_rebind(
        &mut self,
        target: VarId,
        index: &IrExpr,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        // A shared-cell list (native owned slots) is read out of its cell for the
        // guard, as every other reference to it is; the rebind below writes the cell.
        let guarded = if crate::lower::owned_tco_slots() && self.cell_of.contains_key(&target) {
            self.value_or_global(target)
        } else {
            self.value_for(target)
        };
        match (guarded, self.lower_scalar_value(index)) {
            (Ok(list), Some(idx)) => {
                let h = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Handle,
                    dst: Some(h),
                    args: vec![list],
                });
                let addr = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::ElemAddr,
                    dst: Some(addr),
                    args: vec![h, idx],
                });
            }
            _ if crate::lower::strict_values() => {
                return Err(LowerError::Unsupported(
                    "heap-element index-assign with a non-lowerable \
                     index — the C-067 bounds guard cannot be emitted"
                        .into(),
                ));
            }
            _ => {}
        }
        let list_ty = Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::List,
            vec![value.ty.clone()].into(),
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
        self.lower_stmt(&assign)
    }


    /// The `FieldAssign` arm of [`Self::lower_stmt`] — verbatim move (#781).
    fn lower_stmt_field_assign(&mut self, target: VarId, field: almide_lang::intern::Sym, value: &IrExpr) -> Result<(), LowerError> {
                // Mutable-GLOBAL target: the functional rebind through its storage slot.
                if !self.value_of.contains_key(&target) {
                    if let Some(global) = crate::lower::mutable_global_info(target) {
                        return self.lower_mutable_global_field_assign(target, global, field, value);
                    }
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
                //
                // A SCALAR write into a record that HOLDS a heap field takes the same
                // rebind (#2656): the in-place path below COW-guards with `MakeUnique`,
                // whose `$list_copy` is a raw slot copy — on an aliased record
                // (`var m = it; m.a = 41`) the copy and the original then share the
                // heap child without co-owning it, and the two scope-end `__drop_Rec`s
                // release it twice (rc_dec trap). The spread construct `Dup`s every heap
                // slot it carries over, so each record owns its children. The in-place
                // store stays for all-scalar records, where the raw copy IS the value.
                let rebind_ty = self.var_decl_tys.get(&target).cloned().filter(|rec_ty| {
                    self.aggregate_field_tys(rec_ty).is_some_and(|(_, tys)| {
                        is_heap_ty(&value.ty) || tys.iter().any(is_heap_ty)
                            || self.cell_of.contains_key(&target)
                    })
                });
                if let Some(rec_ty) = rebind_ty {
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
                // COW-guard the buffer, then ACTUALLY STORE the field: `r.f = v` →
                // `ListSetScalar(block, slot(f), v)` on the uniform 8-byte-slot aggregate
                // block (the rung-5 layout; `ListGetScalar` is the read side). WITHOUT the
                // store, the assignment lowered to ONLY the MakeUnique guard — EVERY record
                // field-assign was a silent no-op on the verified default (v1 read back the
                // pre-assign value while native mutated: the recassign wrong-value class).
                self.lower_place_mutation(target, value.span)?;
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


    /// `g.f = v` on a mutable module-level record `var`: the functional rebind
    /// `g = { ...g, f: v }` through the global's storage slot. The slot assign builds
    /// the new record FIRST (the spread reads the old one through its own `Dup`), then
    /// takes, drops and replaces the old block — value semantics, so a copy bound
    /// before the write keeps the old field, as native's `RcCow` does. A global that
    /// is not a record/tuple has no field to write and walls.
    fn lower_mutable_global_field_assign(
        &mut self,
        target: VarId,
        global: (u32, Ty),
        field: almide_lang::intern::Sym,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        let (index, gty) = global;
        if self.aggregate_field_tys(&gty).is_none() {
            return Err(LowerError::Unsupported(format!(
                "field-assign to mutable module-level var {target:?} whose type has no \
                 record layout is not in this brick"
            )));
        }
        let base = IrExpr { kind: IrExprKind::Var { id: target }, ty: gty.clone(), span: None, def_id: None };
        let spread = IrExpr {
            kind: IrExprKind::SpreadRecord { base: Box::new(base), fields: vec![(field, value.clone())] },
            ty: gty.clone(),
            span: None,
            def_id: None,
        };
        self.lower_mutable_global_assign(target, index, &gty, &spread)
    }

    /// The `MapInsert` arm of [`Self::lower_stmt`] — verbatim move (#781).
    fn lower_stmt_map_insert(&mut self, target: VarId, key: &IrExpr, value: &IrExpr) -> Result<(), LowerError> {
                // Mutable-GLOBAL target: an in-place insert through the slot would be the
                // COW-copy silent-miscompile class of the IndexAssign guard above, so it
                // takes the same functional REBIND as a local below — `g = map.set(g, k, v)`
                // through the global's storage slot (`lower_mutable_global_assign`: the new
                // map is built first, then the old block is taken and dropped), typed by
                // the global's declared type (#2739).
                let global_ty = if self.value_of.contains_key(&target) {
                    None
                } else {
                    crate::lower::mutable_global_info(target).map(|(_, ty)| ty)
                };
                // Functional REBIND: `m[k] = v` ≡ `m = map.set(m, k, v)` (value
                // semantics) — the SAME treatment the other in-place mutators
                // already get below; the repr dispatch suffixes the self-host
                // (set_skv/str/…) exactly like a source-level call. Both legs take this
                // path (one shared rewrite, mir==ir symmetric); classify credits the
                // MapInsert node with the one synthetic call. Needs the declared map ty
                // for the Var reference — a target without one (a param, not a local
                // bind) keeps the historic wall below.
                if let Some(map_ty) = global_ty.or_else(|| self.var_decl_tys.get(&target).cloned()) {
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
                self.lower_place_mutation(target, value.span)?;
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
        // STMT-position `f()!` under the R2 probe: propagate-and-discard is the
        // SAME one bind rule with a synthetic discard binding (`let _ = f()!`
        // semantics — the ok payload is a scalar copy nobody reads). Probe-off
        // keeps the pre-rule dispatch byte-identical.
        if crate::lower::bang_return_probe() && matches!(&expr.kind, IrExprKind::Unwrap { .. }) {
            let discard = VarId(crate::lower::desugar_var_seed());
            if self.try_lower_bind_unwrap_return(discard, &expr.ty, expr)? {
                return Ok(());
            }
        }
        match &expr.kind {
                // A Unit `if` statement EXECUTES (only the taken arm's effects run) when
                // its cond is a scalar; otherwise it falls back to the linearization.
                IrExprKind::If { cond, then, else_ } => {
                    if self.try_lower_unit_if(cond, then, else_) {
                        Ok(())
                    } else {
                        self.lower_branch(expr)
                    }
                }
                IrExprKind::Match { subject, arms } => {
                    self.lower_unit_match_stmt(expr, subject, arms)
                }
                IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                    self.lower_for_in(*var, var_tuple, iterable, body)
                }
                IrExprKind::While { cond, body } => self.lower_while(cond, body),
                // A BLOCK expression statement (`{ stmts; e }` for its effect): lower
                // its statements (locals ride to the enclosing scope), then its tail —
                // a Unit effect call, a nested branch, or a deferred value whose calls
                // we capture (its value is discarded in statement position).
                IrExprKind::Block { stmts, expr: tail } => {
                    if almide_base::env::flag("ALMIDE_DBG_WHILE") {
                        eprintln!("STMT-BLOCK lowering ({} stmts, tail={})", stmts.len(), tail.is_some());
                    }
                    for s in stmts {
                        self.lower_stmt(s)?;
                    }
                    match tail {
                        Some(t) => self.lower_block_stmt_tail(t),
                        None => Ok(()),
                    }
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
                // The v0 IN-PLACE mutators (`bytes.push`, `list.push`,
                // `map.clear`, …) rewrite to their functional rebinds and re-dispatch
                // through the Assign/FieldAssign path — see `rewrite_inplace_mutation`.
                IrExprKind::Call { target: CallTarget::Module { .. }, .. } => {
                    match rewrite_inplace_mutation(expr) {
                        Some(stmt) => self.lower_stmt(&stmt),
                        None => self.lower_effect_call(expr),
                    }
                }
                _ => match self.lower_discarded_value(expr) {
                    Some(result) => result,
                    None => self.lower_effect_call(expr),
                },
        }
    }

    /// A Unit `match` in STATEMENT position. Over INT literal patterns it
    /// EXECUTES: desugar to a nested `if subject == lit then arm else …` and
    /// run it via try_lower_unit_if (only the matched arm's effects run). A
    /// TUPLE subject of scalar elements — the heap-branch tail-duplication
    /// rewrites `let s = match (…) {…}; use(s)` into this Unit form — takes
    /// the refinement-chain unit sibling (real IfThen/Else/EndIf markers; only
    /// the taken arm's effects run). Non-literal patterns / guards / a
    /// non-scalar subject fall back to the linearization.
    fn lower_unit_match_stmt(
        &mut self,
        expr: &IrExpr,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Result<(), LowerError> {
        if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
            if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                if self.try_lower_unit_if(cond, then, else_) {
                    return Ok(());
                }
            }
        }
        if self.try_lower_tuple_refinement_unit_match(subject, arms) {
            return Ok(());
        }
        self.lower_branch(expr)
    }

    /// One BLOCK-statement TAIL. Same statement-dispatcher routing as the
    /// arm-tail (control.rs): a Block-tail in-place mutator must take the
    /// functional-rebind interceptions (#782).
    ///
    /// A Block-TAIL `if` (the TCO loop body is `{ if … }`, so the base-check
    /// arrives HERE, not via the bare-If statement arm) EXECUTES via
    /// try_lower_unit_if (real branch — only the taken arm runs) so a loop
    /// base-check actually conditionally sets `rk`. Only if that declines do
    /// we consider linearization — and inside a scalar loop linearizing both
    /// arms runs the loop ONCE (the heap-`let`-in-body silent miscompile), so
    /// it walls there. Outside a loop, linearize as before. A LOOP tail is a
    /// Unit EFFECT that must RUN — eliding it silently drops the whole loop
    /// (see lower_branch_arm's twin).
    fn lower_block_stmt_tail(&mut self, t: &IrExpr) -> Result<(), LowerError> {
        match &t.kind {
            IrExprKind::Call { .. } if matches!(t.ty, Ty::Unit | Ty::Never) => self.lower_stmt_expr(t),
            IrExprKind::If { cond, then, else_ } => {
                if !self.try_lower_unit_if(cond, then, else_) {
                    self.lower_branch(t)?;
                }
                Ok(())
            }
            IrExprKind::Match { subject, arms } => {
                let mut done = false;
                if let Some(if_expr) = self.desugar_match_to_if(subject, arms, &Ty::Unit) {
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
                Ok(())
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.lower_for_in(*var, var_tuple, iterable, body)
            }
            IrExprKind::While { cond, body } => self.lower_while(cond, body),
            // A BLOCK tail executes: statements in order, then its own tail
            // through this same dispatcher. Without this arm the
            // never-err match rewrite's ok-arm body (a Block landed as the
            // rewritten block's tail) fell to the elision below and a
            // match-on-effect-call inside a while body ran its arms ZERO
            // times, silently (#1571).
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                match expr.as_deref() {
                    Some(t2) => self.lower_block_stmt_tail(t2),
                    None => Ok(()),
                }
            }
            _ => {
                if let Some(result) = self.lower_discarded_value(t) { return result; }
                // NO THIRD STATE (#810): a call-bearing tail this dispatcher
                // cannot execute must WALL under strict mode — eliding it
                // drops its effects entirely (the #1571 class). An
                // effect-free tail (Unit, a bare var, a literal) keeps the
                // no-op elide: there is nothing to drop.
                if crate::lower::strict_values() && crate::lower::expr_contains_call(t) {
                    return Err(LowerError::Unsupported(format!(
                        "call-bearing `{}` tail of a statement block cannot be \
                         faithfully executed in this brick (eliding it would drop \
                         its effects — a silently wrong value)",
                        kind_name(&t.kind)
                    )));
                }
                self.record_elided_calls(t);
                Ok(())
            }
        }
    }

    /// Discarding a value does not discard its evaluation: an integer
    /// division, index, or call can still fail or have effects. Use the IR's
    /// shared speculation predicate and the ordinary typed binding path,
    /// including ownership accounting for any temporary heap result.
    fn lower_discarded_value(&mut self, expr: &IrExpr) -> Option<Result<(), LowerError>> {
        if matches!(expr.ty, Ty::Unit | Ty::Never) || almide_ir::speculation::is_speculation_safe(expr) {
            return None;
        }
        let discard = self.fresh_synth_var();
        Some(self.lower_bind(discard, &expr.ty, expr))
    }
}

include!("mod_p3_mutators.rs");
