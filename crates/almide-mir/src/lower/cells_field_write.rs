// A FIELD write through a native shared cell (`h.xs = …`, the `list.push(h.xs, v)`
// rebind, a captured record's `h.n = h.n + 1`). The general path reads the
// payload twice (once for the value, once as the spread's base), each read a
// whole-record copy, then takes and drops the old payload. When the written
// value cannot run program code and cannot leave early, nothing can look at
// the cell while the write is under way: the payload is taken out of the cell
// FIRST, the field write lands on that owned record exactly as it does on a
// plain local (the field moves, an append extends in place), and the record is
// stored back.
//
// "Cannot run program code": a cell is reachable only from its own function
// and the closures that capture it, so only a call that is handed a closure,
// or that calls one, can look at it. The value admits literals, var and member
// reads, operators, constructors, and module calls whose every argument is
// plain data (no closure anywhere in its type). "Cannot leave early": no `!`,
// `?`, `return`, `break`, block or match — every path of the value reaches the
// store back.

/// `ty` holds no closure anywhere (a named record's fields resolved through
/// the registry; a type that does not resolve is not plain).
fn cell_plain_ty(ctx: &LowerCtx, ty: &Ty, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    match ty {
        Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16
        | Ty::UInt32 | Ty::UInt64 | Ty::Float32 | Ty::Float64 | Ty::String | Ty::Bool | Ty::Unit
        | Ty::Bytes => true,
        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::UserDefined(_), _) | Ty::Named(..) => ctx
            .aggregate_field_tys(ty)
            .is_some_and(|(_, tys)| tys.iter().all(|t| cell_plain_ty(ctx, t, depth + 1))),
        Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().all(|t| cell_plain_ty(ctx, t, depth + 1)),
        Ty::Record { fields } => fields.iter().all(|(_, t)| cell_plain_ty(ctx, t, depth + 1)),
        _ => false,
    }
}

/// `e` runs no program code and reaches its end on every path (see above).
fn cell_write_value_inert(ctx: &LowerCtx, e: &IrExpr) -> bool {
    let inert = |x: &IrExpr| cell_write_value_inert(ctx, x);
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::EmptyMap
        | IrExprKind::OptionNone => true,
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => inert(object),
        IrExprKind::UnOp { operand, .. } => inert(operand),
        IrExprKind::BinOp { left, right, .. } => inert(left) && inert(right),
        IrExprKind::IndexAccess { object, index } => inert(object) && inert(index),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter().all(inert),
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, x)| inert(x)),
        IrExprKind::SpreadRecord { base, fields } => inert(base) && fields.iter().all(|(_, x)| inert(x)),
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
            inert(expr)
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            almide_ir::IrStringPart::Lit { .. } => true,
            almide_ir::IrStringPart::Expr { expr } => inert(expr),
        }),
        IrExprKind::Call { target: CallTarget::Module { .. }, args, .. } => {
            args.iter().all(|a| inert(a) && cell_plain_ty(ctx, &a.ty, 0))
        }
        _ => false,
    }
}

impl LowerCtx {
    /// The take-first field write (module docs). `Ok(false)` leaves the write
    /// to the general cell rebind.
    pub(crate) fn try_lower_cell_field_assign(
        &mut self,
        target: VarId,
        field: almide_lang::intern::Sym,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        let Some(&cell) = self.cell_of.get(&target) else { return Ok(false) };
        if !crate::lower::owned_tco_slots() || (self.in_frame > 0 && self.unit_arm_depth == 0 && self.scalar_loop_depth == 0) {
            return Ok(false);
        }
        let Some(ty) = self.var_decl_tys.get(&target).cloned() else { return Ok(false) };
        if self.cell_class_of_ctx(&ty) != Some(CellClass::NativeOwned)
            || self.aggregate_field_tys(&ty).is_none()
            || !cell_write_value_inert(self, value)
        {
            return Ok(false);
        }
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
        self.materialized_aggregates.insert(old);
        self.materialized_lists.insert(old);
        // The taken payload is an ordinary owned local the new record is
        // built from (`{ ...local, field: value }`), then released.
        let local = self.fresh_synth_var();
        self.value_of.insert(local, old);
        self.var_decl_tys.insert(local, ty.clone());
        let local_ref = IrExpr { kind: IrExprKind::Var { id: local }, ty: ty.clone(), span: None, def_id: None };
        let value = almide_ir::substitute::substitute_var_in_expr(value, target, &local_ref);
        let spread = IrExpr {
            kind: IrExprKind::SpreadRecord { base: Box::new(local_ref), fields: vec![(field, value)] },
            ty: ty.clone(),
            span: None,
            def_id: None,
        };
        let new = self.lower_typed_cell_value(&ty, &spread)?;
        self.value_of.remove(&local);
        let drop_old = self.drop_op_for(old);
        self.ops.push(drop_old);
        self.live_heap_handles.retain(|v| *v != old);
        let handle = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(handle), args: vec![new] });
        self.ops.push(Op::Prim { kind: crate::PrimKind::Store { width: 8 }, dst: None, args: vec![addr, handle] });
        self.ops.push(Op::Consume { v: new });
        self.live_heap_handles.retain(|v| *v != new);
        Ok(true)
    }
}
