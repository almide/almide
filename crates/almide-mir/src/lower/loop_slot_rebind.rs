// ── loop rebind of a slot the frame does not own (#3298) ──
// A heap reassignment in an executing loop body drops the slot's old object and
// rebinds the slot in place (`Drop old; SetLocal old = new`, the proven `i(id)m`
// loop slot). That is only right when the frame owns the old object. A borrowed
// `mut` param (or a payload loaded out of a borrowed record) owned by the caller
// must have been copied before the loop (`precopy_borrowed_reassign_slots`), or
// the first iteration's drop releases the caller's reference — a double free the
// caller's own release then completes. A value the loop body itself loaded this
// iteration (a field read off a global record) is a per-iteration borrow: the
// rebind is a plain `value_of` rebind of the var to the fresh value, which the
// iteration's frame then owns and drops, and the borrow is left alone.

impl LowerCtx {
    /// `Ok(true)`: rebound in place (the slot owns `new`); `Ok(false)`: the var
    /// now names `new`, which the frame owns.
    pub(crate) fn rebind_loop_slot(
        &mut self,
        var: VarId,
        slot_local: ValueId,
        new: ValueId,
    ) -> Result<bool, LowerError> {
        if self.live_heap_handles.contains(&slot_local) || !self.borrowed_slot(slot_local) {
            let drop_op = self.drop_op_for(slot_local);
            self.ops.push(drop_op);
            self.ops.push(Op::SetLocal { local: slot_local, src: new });
            return Ok(true);
        }
        if self.defined_in_current_loop(slot_local) {
            self.value_of.insert(var, new);
            if !self.live_heap_handles.contains(&new) {
                self.live_heap_handles.push(new);
            }
            return Ok(false);
        }
        Err(LowerError::Unsupported(
            "heap rebind in a loop of a slot the frame does not own (a borrowed param or \
             payload not copied before the loop)"
                .into(),
        ))
    }

    /// A value the frame holds no reference of its own to: a borrowed param, or a
    /// handle loaded out of another block (`LoadHandle`).
    fn borrowed_slot(&self, v: ValueId) -> bool {
        if self.param_values.contains(&v) {
            return true;
        }
        self.ops.iter().rev().any(|op| {
            matches!(op, Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(d), .. } if *d == v)
        })
    }

    /// Is `v` defined by an op inside the innermost open loop (after its
    /// unmatched `LoopStart`)?
    fn defined_in_current_loop(&self, v: ValueId) -> bool {
        let mut depth = 0usize;
        for op in self.ops.iter().rev() {
            match op {
                Op::LoopEnd => depth += 1,
                Op::LoopStart if depth == 0 => return false,
                Op::LoopStart => depth -= 1,
                _ => {
                    if crate::mir_ops::defined_value(op) == Some(v) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Collect every var a loop body HEAP-REASSIGNS — directly (`Assign` with a heap
/// value, `FieldAssign`/`MapInsert` targets whose write lowers to a rebind) or via
/// the functional-rebind rewrites (`list.push(v, x)` / `string.push(s, x)` /
/// `bytes.push(b, x)` and their Member-receiver forms, which rewrite into
/// Assign/FieldAssign during lowering). Descends into nested control flow but NOT
/// into nested loops (each loop pre-copies its own borrowed slots when reached).
pub(crate) fn collect_heap_reassign_vars(stmts: &[IrStmt], out: &mut Vec<VarId>) {
    collect_heap_reassign_vars_from(stmts, None, out);
}

/// [`collect_heap_reassign_vars`] over an expression — a callback body the
/// defunctionalized loops inline (#3298).
pub(crate) fn collect_heap_reassign_vars_in_expr(e: &IrExpr, out: &mut Vec<VarId>) {
    collect_heap_reassign_vars_from(&[], Some(e), out);
}

fn collect_heap_reassign_vars_from(stmts: &[IrStmt], tail: Option<&IrExpr>, out: &mut Vec<VarId>) {
    use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
    struct Scan<'a> {
        out: &'a mut Vec<VarId>,
    }
    impl Scan<'_> {
        fn push(&mut self, v: VarId) {
            if !self.out.contains(&v) {
                self.out.push(v);
            }
        }
        fn push_receiver(&mut self, recv: &IrExpr) {
            match &recv.kind {
                IrExprKind::Var { id } => self.push(*id),
                IrExprKind::Member { object, .. } => {
                    if let IrExprKind::Var { id } = &object.kind {
                        self.push(*id);
                    }
                }
                _ => {}
            }
        }
    }
    impl IrVisitor for Scan<'_> {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Assign { var, value } => {
                    if is_heap_ty(&value.ty) {
                        self.push(*var);
                    }
                }
                IrStmtKind::FieldAssign { target, value, .. } => {
                    if is_heap_ty(&value.ty) {
                        self.push(*target);
                    }
                }
                IrStmtKind::MapInsert { target, .. } => {
                    self.push(*target);
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if func.as_str() == "push"
                        && matches!(module.as_str(), "list" | "string" | "bytes")
                        && !args.is_empty() =>
                {
                    self.push_receiver(&args[0]);
                    walk_expr(self, e);
                }
                // Every other in-place mutator the statement lowering rewrites
                // to its functional rebind (`map.insert(m, k, v)` → `m =
                // map.set(m, k, v)`, the byte appends): read the rebind off the
                // same rewrite, so the two cannot drift (#3298).
                // Only the rebind TARGET is read off the rewrite: its value can be
                // the same call under its functional name (`bytes.append_u16`).
                IrExprKind::Call { target: CallTarget::Module { .. }, .. } => {
                    match rewrite_inplace_mutation(e).map(|s| s.kind) {
                        Some(IrStmtKind::Assign { var, .. }) => self.push(var),
                        Some(IrStmtKind::FieldAssign { target, .. }) => self.push(target),
                        _ => {}
                    }
                    walk_expr(self, e);
                }
                // A nested loop pre-copies its OWN borrowed slots — do not descend.
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan { out };
    for stmt in stmts {
        s.visit_stmt(stmt);
    }
    if let Some(e) = tail {
        s.visit_expr(e);
    }
}
