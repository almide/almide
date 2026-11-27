// ── part of pass_capture_clone.rs, include!-spliced (split for the 800-line budget) ──

fn replace_vars_stmt(stmt: &mut IrStmt, renames: &std::collections::HashMap<VarId, VarId>) {
    // A captured var that is WRITTEN inside the closure (Assign/IndexAssign/...)
    // must have its write *target* renamed too, not just its read sites —
    // otherwise the closure mutates the original var (capturing/moving it) instead
    // of its `__cap` clone. (Closure v2, P3.)
    fn rn(v: &mut VarId, renames: &std::collections::HashMap<VarId, VarId>) {
        if let Some(&new) = renames.get(v) { *v = new; }
    }
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            replace_vars(value, renames);
        }
        IrStmtKind::Assign { var, value } => { rn(var, renames); replace_vars(value, renames); }
        IrStmtKind::FieldAssign { target, value, .. } => { rn(target, renames); replace_vars(value, renames); }
        IrStmtKind::IndexAssign { target, index, value } => {
            rn(target, renames); replace_vars(index, renames); replace_vars(value, renames);
        }
        IrStmtKind::MapInsert { target, key, value } => {
            rn(target, renames); replace_vars(key, renames); replace_vars(value, renames);
        }
        IrStmtKind::ListSwap { target, a, b } => {
            rn(target, renames); replace_vars(a, renames); replace_vars(b, renames);
        }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
            rn(target, renames); replace_vars(end, renames);
        }
        IrStmtKind::ListCopySlice { dst, src, len } => {
            rn(dst, renames); rn(src, renames); replace_vars(len, renames);
        }
        IrStmtKind::Guard { cond, else_ } => {
            replace_vars(cond, renames); replace_vars(else_, renames);
        }
        IrStmtKind::Expr { expr } => replace_vars(expr, renames),
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { rn(var, renames); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn needs_clone_type(ty: &Ty) -> bool {
    // §4 stage 2c (#531): derived from THE copy-ness classifier — see the
    // projection table in almide_ir::top_let_storage (note the tuple cell).
    almide_ir::top_let_storage::capture_clone_wrap(ty)
}
