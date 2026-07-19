// SHARED-CELL locals (closures Rung 6): a `var` that is captured by a lambda AND
// mutated must be shared storage, not an env value-copy — the closure's functional
// rebind (`acc = acc + [1]`, the `list.push` desugar) otherwise writes a copy the
// enclosing scope never sees (probe-confirmed silent wrong value: native 1 / wasm 0
// for a closure stored in a list/tuple/record and then called). The cell is a 1-slot
// DynList block holding the var's CURRENT value (a raw i64 for a scalar, an OWNED
// handle for heap); every read loads slot 0 fresh and every assign stores through it
// — the LOCAL analogue of the proven mutable-global slot machinery
// (`value_or_global`'s slot load / `__mg_take` + type-routed drop + Store), reusing
// the same runtime accessor. `lift_lambda` captures the CELL handle (an rc-shared
// co-own), so closure and enclosing scope address the same storage — both directions
// of mutation (inside the closure, or in the enclosing scope after capture) stay
// visible to the other side.

/// The in-place mutator surface the statement dispatcher rewrites to FUNCTIONAL
/// REBINDS (`lower_stmt_expr`'s bytes/map/list/string arms): a call statement
/// `m.f(var, …)` from this table becomes `var = …` during lowering. The cell scan
/// must treat these as assignments — at IR-scan time the rebind has not happened
/// yet, so an `Assign`-only scan misses exactly the `list.push(acc, 1)`-through-a-
/// closure class this machinery exists for.
///
/// EXACT-MIRROR contract: this table must match the rebind arms and NOTHING more.
/// A genuine in-place effect call that never rebinds (`bytes.set_at` — an index
/// store with no realloc) mutates the SHARED block through the handle, so a plain
/// env value-copy capture is already correct; forcing a cell for it regressed the
/// working shape to a wall (wasm_bytes_set_at_shared_through_closure).
pub(crate) fn inplace_mutated_receiver(e: &IrExpr) -> Option<VarId> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
    else {
        return None;
    };
    let mutates = match (module.as_str(), func.as_str()) {
        ("list", "push") | ("list", "clear") => true,
        ("string", "push") => true,
        ("map", "insert") | ("map", "delete") | ("map", "clear") => true,
        ("bytes", f) => {
            matches!(f, "push" | "append_u8")
                || f.starts_with("append_")
                || matches!(f, "write_uint16" | "write_uint32" | "write_int32" | "write_float32")
        }
        _ => false,
    };
    if !mutates {
        return None;
    }
    match args.first().map(|a| &a.kind) {
        Some(IrExprKind::Var { id }) => Some(*id),
        _ => None,
    }
}

/// Compute the SHARED-CELL var set for a function body: every var that is (a)
/// captured free by some lambda in the body and (b) mutated anywhere — an
/// `Assign`/`IndexAssign`/`FieldAssign`/`MapInsert` target or an in-place mutator
/// receiver, in the enclosing scope OR inside any lambda body. Globals are excluded
/// (the module slot machinery is already shared storage) and params cannot be
/// reassigned (`mutated_params` fns wall earlier). Over-approximation is safe: a
/// cell for a var whose closure only ever C1-inlines still reads/writes consistently
/// through the cell.
pub(crate) fn collect_cell_vars(
    body: &IrExpr,
    globals: &HashMap<VarId, Ty>,
    params: &[almide_ir::IrParam],
) -> HashSet<VarId> {
    struct Scan<'a> {
        globals: &'a HashMap<VarId, Ty>,
        captured: HashSet<VarId>,
        mutated: HashSet<VarId>,
    }
    impl almide_ir::visit::IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &e.kind {
                let mut bound: HashSet<VarId> = HashSet::new();
                for (v, _) in params {
                    bound.insert(*v);
                }
                for v in almide_ir::free_vars::free_vars(body, &bound) {
                    if !self.globals.contains_key(&v)
                        && crate::lower::mutable_global_info(v).is_none()
                    {
                        self.captured.insert(v);
                    }
                }
            }
            if let Some(v) = inplace_mutated_receiver(e) {
                self.mutated.insert(v);
            }
            almide_ir::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. } => {
                    self.mutated.insert(*var);
                }
                IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::FieldAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. } => {
                    self.mutated.insert(*target);
                }
                _ => {}
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut scan = Scan { globals, captured: HashSet::new(), mutated: HashSet::new() };
    almide_ir::visit::IrVisitor::visit_expr(&mut scan, body);
    let param_ids: HashSet<VarId> = params.iter().map(|p| p.var).collect();
    let out: HashSet<VarId> = scan
        .captured
        .intersection(&scan.mutated)
        .filter(|v| !param_ids.contains(v))
        .copied()
        .collect();
    if std::env::var("ALMIDE_DBG_CELLS").is_ok() {
        eprintln!(
            "[cells] captured={:?} mutated={:?} cells={:?}",
            scan.captured, scan.mutated, out
        );
    }
    out
}

/// The cell INNER classes this brick admits, chosen so the cell rides EXISTING drop
/// machinery end to end: a scalar inner leaves the slot raw (the cell block itself
/// frees flat); a FLAT-heap inner (one `rc_dec` fully frees it — the same
/// `one_level_exact` family the env capture classes use, plus `Bytes`) makes the
/// 1-slot cell physically a `DynListStr`-class block, so both the scope-end drop
/// (`heap_elem_lists` → `DropListStr`: dec slot 0, free the cell) and the env
/// capture (the nested-heap class → `$__drop_closure`'s `__drop_list_str` walk) are
/// already correct. A NESTED inner (`List[String]`, a Map, a record) is NOT admitted
/// — the bind falls through to the plain local path and `lift_lambda`'s
/// mutated-capture gate refuses the lift, an honest wall instead of the value-copy
/// miscompile.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CellClass {
    Scalar,
    FlatHeap,
}

pub(crate) fn cell_class_of(ty: &Ty) -> Option<CellClass> {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Int | Ty::Bool => Some(CellClass::Scalar),
        Ty::String | Ty::Bytes => Some(CellClass::FlatHeap),
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Float) =>
        {
            Some(CellClass::FlatHeap)
        }
        _ => None,
    }
}

impl LowerCtx {
    /// The address of a cell's single value slot (slot 0 of the 1-slot block).
    fn cell_slot_addr(&mut self, cell: ValueId) -> ValueId {
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![cell] });
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: layout::slot_offset(0) as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
        addr
    }

    /// `var x = init` for a CELL var: lower the initializer as an owned value, alloc
    /// the 1-slot cell, move the value in (store + `Consume` — the record-slot
    /// move-in pattern), and register the cell's own scope-end drop (flat for a
    /// scalar inner; the `heap_elem_lists` walk for a flat-heap inner, which decs
    /// slot 0 then frees the cell). The var maps into `cell_of` ONLY — never
    /// `value_of` — so every read takes the fresh-load path.
    pub(crate) fn lower_cell_bind(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
        class: CellClass,
    ) -> Result<(), LowerError> {
        let inner: ValueId = match class {
            CellClass::Scalar => self
                .lower_scalar_value(value)
                .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                .ok_or_else(|| {
                    LowerError::Unsupported(format!(
                        "cell var {var:?} scalar initializer outside the value subset"
                    ))
                })?,
            CellClass::FlatHeap => self.lower_owned_heap_field(value).ok_or_else(|| {
                LowerError::Unsupported(format!(
                    "cell var {var:?} heap initializer outside the value subset"
                ))
            })?,
        };
        let len_c = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len_c, value: 1 });
        let cell = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: cell,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len: len_c },
        });
        let addr = self.cell_slot_addr(cell);
        match class {
            CellClass::Scalar => {
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, inner],
                });
            }
            CellClass::FlatHeap => {
                let handle = self.fresh_value();
                self.ops
                    .push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(handle), args: vec![inner] });
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, handle],
                });
                self.ops.push(Op::Consume { v: inner });
                self.live_heap_handles.retain(|x| *x != inner);
                self.heap_elem_lists.insert(cell);
            }
        }
        self.live_heap_handles.push(cell);
        self.cell_of.insert(var, cell);
        self.var_decl_tys.insert(var, ty.clone());
        Ok(())
    }

    /// A value-position read of a CELL var: load slot 0 FRESH on every reference
    /// (never cached — an intervening closure call may have written the cell). A
    /// scalar is a plain slot `Load`; a heap inner borrows the slot handle then
    /// `Dup`s it (the same borrow-then-Dup the mutable-global heap read uses — the
    /// function owns a real reference a later cell write cannot invalidate), routed
    /// for its type-correct scope-end drop.
    pub(crate) fn lower_cell_read(&mut self, var: VarId, cell: ValueId) -> Result<ValueId, LowerError> {
        let ty = self.var_decl_tys.get(&var).cloned().ok_or_else(|| {
            LowerError::Unsupported(format!("cell var {var:?} has no recorded type"))
        })?;
        let class = cell_class_of(&ty).ok_or_else(|| {
            LowerError::Unsupported(format!("cell var {var:?} inner class not in this brick"))
        })?;
        let addr = self.cell_slot_addr(cell);
        match class {
            CellClass::Scalar => {
                let dst = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Load { width: 8 },
                    dst: Some(dst),
                    args: vec![addr],
                });
                Ok(dst)
            }
            CellClass::FlatHeap => {
                let repr = repr_of(&ty)?;
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
                Ok(dst)
            }
        }
    }

    /// ASSIGN through a CELL var's slot — the local mirror of
    /// `lower_mutable_global_assign`: a scalar stores directly; a heap inner builds
    /// the NEW value FIRST (the RHS may read the cell — `acc = acc + [x]`), takes
    /// the OLD slot handle via the same `$__mg_take` accessor (the slot's owned
    /// reference transfers out), drops it by its type route, then stores+`Consume`s
    /// the new value in. A modeled (non-executable) frame must WALL — eliding a
    /// shared-cell write diverges (the write is an effect the closure observes).
    pub(crate) fn lower_cell_assign(
        &mut self,
        var: VarId,
        cell: ValueId,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        if self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0 {
            return Err(LowerError::Unsupported(format!(
                "assignment to shared-cell var {var:?} inside a modeled (non-executable) \
                 frame — the cell write is an effect the model would elide"
            )));
        }
        let ty = self.var_decl_tys.get(&var).cloned().ok_or_else(|| {
            LowerError::Unsupported(format!("cell var {var:?} has no recorded type"))
        })?;
        let class = cell_class_of(&ty).ok_or_else(|| {
            LowerError::Unsupported(format!("cell var {var:?} inner class not in this brick"))
        })?;
        match class {
            CellClass::Scalar => {
                let src = self
                    .lower_scalar_value(value)
                    .or_else(|| self.try_lower_scalar_call(value, &value.ty))
                    .ok_or_else(|| {
                        LowerError::Unsupported(format!(
                            "non-scalar value assigned to cell var {var:?} outside the \
                             executable subset"
                        ))
                    })?;
                let addr = self.cell_slot_addr(cell);
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, src],
                });
                Ok(())
            }
            CellClass::FlatHeap => {
                let new = self.lower_owned_heap_field(value).ok_or_else(|| {
                    LowerError::Unsupported(format!(
                        "heap value assigned to cell var {var:?} outside the executable subset"
                    ))
                })?;
                let repr = repr_of(&ty)?;
                let addr = self.cell_slot_addr(cell);
                let old = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(old),
                    name: "__mg_take".to_string(),
                    args: vec![crate::CallArg::Scalar(addr)],
                    result: Some(repr),
                });
                self.materialized_call_arg(old, repr, &ty);
                let drop_old = self.drop_op_for(old);
                self.ops.push(drop_old);
                self.live_heap_handles.retain(|v| *v != old);
                let handle = self.fresh_value();
                self.ops
                    .push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(handle), args: vec![new] });
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![addr, handle],
                });
                self.ops.push(Op::Consume { v: new });
                self.live_heap_handles.retain(|v| *v != new);
                Ok(())
            }
        }
    }
}
