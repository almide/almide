//! PerceusPass: Insert RcInc/RcDec IR nodes for automatic memory management.
//!
//! Implements Perceus-style reference counting (ICFP 2021) at the IR level.
//! All RC operations are derived from types alone — no user annotation.
//!
//! Rules:
//!   1. RcInc: Bind(y, Var(x)) where is_heap(typeof(x)) → insert RcInc(x) after bind
//!   2. RcDec: last use of heap-typed variable in a block → insert RcDec after last use
//!   3. RcDec: Assign(x, _) where is_heap(typeof(x)) → insert RcDec(x) before assign
//!   4. RcDec: function exit → insert RcDec for all heap locals not returned
//!   5. RcInc: ClosureCreate captures heap var → insert RcInc
//!
//! Target: WASM only (Rust handles ownership natively).

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use std::collections::{HashMap, HashSet};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct PerceusPass;

impl NanoPass for PerceusPass {
    fn name(&self) -> &str { "Perceus" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if insert_rc_ops(func, &program.var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if insert_rc_ops(func, &program.var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Insert RC operations into a function body.
fn insert_rc_ops(func: &mut IrFunction, var_table: &VarTable) -> bool {
    if func.name.as_str() == "main" || func.is_test { return false; }

    let mut changed = false;

    // Process the body block
    if let IrExprKind::Block { stmts, expr } = &mut func.body.kind {
        // Rule 1: RcInc for Bind aliases
        // Rule 3: RcDec for Assign old values
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            match &stmt.kind {
                // Rule 1: Bind(y, Var(x)) where heap → RcInc(x)
                IrStmtKind::Bind { var: _, ty, value, .. } => {
                    if is_heap_type(ty) {
                        let id_opt = match &value.kind {
                            IrExprKind::Var { id } => Some(*id),
                            IrExprKind::Clone { expr } => {
                                if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                            }
                            IrExprKind::Deref { expr } => {
                                if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                            }
                            _ => None,
                        };
                        if let Some(id) = id_opt {
                            new_stmts.push(IrStmt {
                                kind: IrStmtKind::RcInc { var: id },
                                span: stmt.span,
                            });
                            changed = true;
                        }
                    }
                    new_stmts.push(stmt);
                }
                // Rule 3: Assign(x, _) where heap → RcDec(x) before assign
                IrStmtKind::Assign { var, .. } => {
                    let ty = &var_table.get(*var).ty;
                    if is_heap_type(ty) {
                        new_stmts.push(IrStmt {
                            kind: IrStmtKind::RcDec { var: *var },
                            span: stmt.span,
                        });
                        changed = true;
                    }
                    new_stmts.push(stmt);
                }
                _ => new_stmts.push(stmt),
            }
        }
        *stmts = new_stmts;

        // Rule 2: Last-use RcDec for block locals
        insert_last_use_drops(stmts, expr.as_deref(), var_table);

        // Rule 4: Function-exit RcDec for remaining heap locals
        // Collect exit drops from the function body (before mutating stmts)
        let mut exit_vars = Vec::new();
        collect_heap_binds_from_stmts(stmts, var_table, &mut exit_vars);
        for var in exit_vars {
            let use_count = var_table.get(var).use_count;
            if use_count <= 1 { continue; }
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
        }
    }

    // Rule 5: RcInc for closure captures
    insert_closure_capture_incs(&mut func.body);

    changed
}

/// Rule 2: Insert RcDec after the last use of each heap-typed local in a block.
fn insert_last_use_drops(stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>, var_table: &VarTable) {
    // Collect heap-typed locals bound in this block
    let mut block_locals: HashMap<VarId, Ty> = HashMap::new();
    for stmt in stmts.iter() {
        if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
            if is_heap_type(ty) {
                block_locals.insert(*var, ty.clone());
            }
        }
    }
    if block_locals.is_empty() { return; }

    // Variables used in tail expression — don't drop (they're returned)
    let mut tail_vars: HashSet<VarId> = HashSet::new();
    if let Some(tail) = tail {
        collect_var_refs_expr(tail, &mut tail_vars);
    }

    // For each block-local, find the last statement that references it
    let mut last_use: HashMap<VarId, usize> = HashMap::new();
    for (i, stmt) in stmts.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_var_refs_stmt(stmt, &mut refs);
        for var in &refs {
            if block_locals.contains_key(var) {
                last_use.insert(*var, i);
            }
        }
    }

    // Build insertion map: after stmt_idx, insert RcDec for these vars
    let mut insertions: HashMap<usize, Vec<VarId>> = HashMap::new();
    for (var, stmt_idx) in &last_use {
        if tail_vars.contains(var) { continue; }
        // Don't drop at the bind statement itself
        let bind_idx = stmts.iter().position(|s| {
            matches!(&s.kind, IrStmtKind::Bind { var: v, .. } if *v == *var)
        });
        if bind_idx == Some(*stmt_idx) { continue; }
        insertions.entry(*stmt_idx).or_default().push(*var);
    }

    // Apply insertions (iterate in reverse to preserve indices)
    let mut sorted_indices: Vec<usize> = insertions.keys().copied().collect();
    sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted_indices {
        if let Some(vars) = insertions.get(&idx) {
            for var in vars {
                stmts.insert(idx + 1, IrStmt {
                    kind: IrStmtKind::RcDec { var: *var },
                    span: None,
                });
            }
        }
    }
}

/// Collect heap-typed Bind variables from a statement list.
fn collect_heap_binds_from_stmts(stmts: &[IrStmt], _var_table: &VarTable, out: &mut Vec<VarId>) {
    for stmt in stmts {
        if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
            if is_heap_type(ty) {
                out.push(*var);
            }
        }
    }
}

/// Rule 5: Insert RcInc before ClosureCreate for heap-typed captures.
fn insert_closure_capture_incs(expr: &mut IrExpr) {
    // Walk the expression tree looking for ClosureCreate with heap captures
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts = Vec::new();
            for stmt in stmts.drain(..) {
                // Check if this stmt's value contains a ClosureCreate
                if let IrStmtKind::Bind { value, .. } = &stmt.kind {
                    if let IrExprKind::ClosureCreate { captures, .. } = &value.kind {
                        for (vid, ty) in captures {
                            if is_heap_type(ty) {
                                new_stmts.push(IrStmt {
                                    kind: IrStmtKind::RcInc { var: *vid },
                                    span: stmt.span,
                                });
                            }
                        }
                    }
                }
                new_stmts.push(stmt);
            }
            *stmts = new_stmts;
            if let Some(tail) = tail {
                insert_closure_capture_incs(tail);
            }
        }
        IrExprKind::If { then, else_, .. } => {
            insert_closure_capture_incs(then);
            insert_closure_capture_incs(else_);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                insert_closure_capture_incs(&mut arm.body);
            }
        }
        _ => {}
    }
}

fn collect_var_refs_expr(expr: &IrExpr, refs: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct VarCollector<'a> { refs: &'a mut HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    VarCollector { refs }.visit_expr(expr);
}

fn collect_var_refs_stmt(stmt: &IrStmt, refs: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_var_refs_expr(value, refs),
        IrStmtKind::Assign { var, value } => { refs.insert(*var); collect_var_refs_expr(value, refs); }
        IrStmtKind::Expr { expr } => collect_var_refs_expr(expr, refs),
        IrStmtKind::Guard { cond, else_ } => { collect_var_refs_expr(cond, refs); collect_var_refs_expr(else_, refs); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { refs.insert(*var); }
        _ => {}
    }
}
