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
}
