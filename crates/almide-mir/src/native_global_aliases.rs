//! Resolve imported global uses to their declaration after VarId unification.
//! Reads and writes share the same cell; initializers are never copied into
//! the importing function. The shared storage analysis owns name resolution.
use almide_ir::{IrExpr, IrExprKind, IrMutVisitor, IrProgram, IrStmt, IrStmtKind, VarId};
use std::collections::HashMap;

pub(crate) fn resolve(ir: &mut IrProgram) {
    use almide_ir::top_let_storage::{build_global_tables, top_let_inputs};
    let inputs: Vec<_> = ir.top_lets.iter().map(top_let_inputs).collect();
    let (_, aliases, _) = build_global_tables(&inputs, &ir.var_table);
    // Unresolvable references remain unbound and are rejected by lowering.
    // Entries for pruned bodies alone must not add an executable root.
    let mut pass = Resolve(&aliases);
    for f in &mut ir.functions {
        pass.visit_expr_mut(&mut f.body);
        for p in &mut f.params {
            if let Some(default) = &mut p.default { pass.visit_expr_mut(default); }
        }
    }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
    super::defaults(&mut ir.type_decls, |e| pass.visit_expr_mut(e));
}

struct Resolve<'a>(&'a HashMap<VarId, VarId>);
impl Resolve<'_> {
    fn place(&self, id: &mut VarId) { if let Some(decl) = self.0.get(id) { *id = *decl; } }
}
impl IrMutVisitor for Resolve<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Var { id } => self.place(id),
            IrExprKind::ClosureCreate { captures, .. } => for (id, _) in captures { self.place(id); },
            _ => {}
        }
        almide_ir::walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        match &mut s.kind {
            IrStmtKind::Assign { var, .. }
            | IrStmtKind::IndexAssign { target: var, .. }
            | IrStmtKind::FieldAssign { target: var, .. }
            | IrStmtKind::MapInsert { target: var, .. }
            | IrStmtKind::ListSwap { target: var, .. }
            | IrStmtKind::ListReverse { target: var, .. }
            | IrStmtKind::ListRotateLeft { target: var, .. } => self.place(var),
            IrStmtKind::ListCopySlice { dst, src, .. } => { self.place(dst); self.place(src); }
            _ => {}
        }
        almide_ir::walk_stmt_mut(self, s);
    }
}
