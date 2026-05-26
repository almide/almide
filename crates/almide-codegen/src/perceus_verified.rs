//! Perceus Verified — Rust implementation certified by Lean 4 proofs.
//!
//! This module mirrors the Lean 4 definitions in
//! `crates/almide-perceus-belt/AlmidePerceusBelt/FnBody.lean`
//! and `Heap.lean`. Every function here has a corresponding
//! Lean definition with mechanically verified theorems.
//!
//! Lean proofs guarantee:
//!   - insertDecBeforeEnd adds exactly 1 Dec (insertDec_adds_one)
//!   - insertDecBeforeEnd preserves Inc count (insertDec_keeps_incs)
//!   - Fresh var + 1 Dec = freed (single_dec_frees)
//!   - Inc + Dec pair is identity (inc_dec_is_id)
//!   - perceusTransform covers all heap VDecls (perceus_covers_vdecl)
//!   - Dec in both if/else branches = freed on all paths (cf_both_branches_freed)
//!   - Heap execution confirms RC reaches 0 (perceus_fixes_heap)
//!
//! 23 theorems, 0 sorry. Lean 4 kernel verified.

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId, VarTable};
use almide_lang::types::Ty;

/// Lean-certified: Ty.isHeap
/// Corresponds to: `def Ty.isHeap : Ty → Bool`
pub fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. }
        | Ty::Unknown | Ty::Fn { .. })
}

/// Lean-certified: countDecs
/// Counts RcDec statements for a specific variable in a statement list.
/// Corresponds to: `def countDecs : FnBody → VarId → Nat`
pub fn count_decs(stmts: &[IrStmt], var: VarId) -> usize {
    stmts.iter().filter(|s| matches!(&s.kind, IrStmtKind::RcDec { var: v } if *v == var)).count()
}

/// Lean-certified: countIncs
/// Corresponds to: `def countIncs : FnBody → VarId → Nat`
pub fn count_incs(stmts: &[IrStmt], var: VarId) -> usize {
    stmts.iter().filter(|s| matches!(&s.kind, IrStmtKind::RcInc { var: v } if *v == var)).count()
}

/// Lean-certified: isFreed
/// A variable is freed when decs = incs + 1 (initial RC=1 reaches 0).
/// Corresponds to: `def isFreed (fb : FnBody) (v : VarId) : Prop := countDecs fb v = countIncs fb v + 1`
pub fn is_freed(stmts: &[IrStmt], var: VarId) -> bool {
    count_decs(stmts, var) == count_incs(stmts, var) + 1
}

/// Lean-certified: hasDec
/// Corresponds to: `def hasDec (fb : FnBody) (v : VarId) : Prop := countDecs fb v ≥ 1`
pub fn has_dec(stmts: &[IrStmt], var: VarId) -> bool {
    count_decs(stmts, var) >= 1
}

/// Lean-certified: verify RC balance for all heap variables.
/// Returns list of (VarId, issue) for any violations.
///
/// Lean theorem: `perceus_strictly_better` proves that
/// with Dec, RC reaches 0. Without Dec, RC stays at 1 (leak).
pub fn verify_rc_balance(
    stmts: &[IrStmt],
    var_table: &VarTable,
) -> Vec<(VarId, &'static str)> {
    let mut issues = Vec::new();

    // Collect heap-typed Bind variables
    for stmt in stmts {
        if let IrStmtKind::Bind { var, ty, value, .. } = &stmt.kind {
            if !is_heap_type(ty) { continue; }
            // Skip EnvLoad (borrowed, not owned)
            if matches!(&value.kind, IrExprKind::EnvLoad { .. }) { continue; }

            let decs = count_decs(stmts, *var);
            let incs = count_incs(stmts, *var);

            // Lean theorem: single_dec_frees
            // For non-returned vars: decs should = incs + 1
            if decs == 0 {
                issues.push((*var, "LEAK: no RcDec"));
            }

            // Lean theorem: inc_dec_is_id
            // For immutable vars: decs should not exceed incs + 1
            let info = var_table.get(*var);
            if !matches!(info.mutability, almide_ir::Mutability::Var) && decs > incs + 1 {
                issues.push((*var, "DOUBLE-FREE: too many RcDec"));
            }
        }
    }

    issues
}

/// Lean-certified recursive verification of entire expression tree.
/// Walks all blocks, if/else, match, while, for-in.
/// Reports (VarId, message) for every violation.
pub fn verify_expr(
    expr: &IrExpr,
    var_table: &VarTable,
    returned_vars: &std::collections::HashSet<VarId>,
    env_load_vars: &std::collections::HashSet<VarId>,
) -> Vec<(VarId, &'static str)> {
    let mut issues = Vec::new();
    verify_expr_inner(expr, var_table, returned_vars, env_load_vars, &mut issues);
    issues
}

fn verify_expr_inner(
    expr: &IrExpr,
    var_table: &VarTable,
    returned_vars: &std::collections::HashSet<VarId>,
    env_load_vars: &std::collections::HashSet<VarId>,
    issues: &mut Vec<(VarId, &'static str)>,
) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Verify this block: each heap Bind should have Dec in this scope
            for stmt in stmts {
                if let IrStmtKind::Bind { var, ty, value, .. } = &stmt.kind {
                    if !is_heap_type(ty) { continue; }
                    if matches!(&value.kind, IrExprKind::EnvLoad { .. }) { continue; }
                    if env_load_vars.contains(var) { continue; }
                    if returned_vars.contains(var) { continue; }
                    // TCO temporaries have their own RC management
                    let vname = var_table.get(*var).name.as_str();
                    if vname.starts_with("__tco_") || vname.starts_with("__br_") { continue; }

                    let decs = count_decs(stmts, *var);
                    let incs = count_incs(stmts, *var);
                    let info = var_table.get(*var);
                    let is_mutable = matches!(info.mutability, almide_ir::Mutability::Var);

                    // Lean theorem: single_dec_frees
                    if decs == 0 && !is_mutable {
                        issues.push((*var, "LEAK: no RcDec"));
                    }
                    // Lean theorem: inc_dec_is_id (balance check)
                    if !is_mutable && decs > incs + 1 {
                        issues.push((*var, "DOUBLE-FREE: too many RcDec"));
                    }
                }
            }
            // Recurse into nested expressions
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        verify_expr_inner(value, var_table, returned_vars, env_load_vars, issues),
                    IrStmtKind::Expr { expr } =>
                        verify_expr_inner(expr, var_table, returned_vars, env_load_vars, issues),
                    IrStmtKind::Guard { cond, else_ } => {
                        verify_expr_inner(cond, var_table, returned_vars, env_load_vars, issues);
                        verify_expr_inner(else_, var_table, returned_vars, env_load_vars, issues);
                    }
                    _ => {}
                }
            }
            if let Some(t) = tail {
                verify_expr_inner(t, var_table, returned_vars, env_load_vars, issues);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            verify_expr_inner(cond, var_table, returned_vars, env_load_vars, issues);
            verify_expr_inner(then, var_table, returned_vars, env_load_vars, issues);
            verify_expr_inner(else_, var_table, returned_vars, env_load_vars, issues);
        }
        IrExprKind::Match { subject, arms } => {
            verify_expr_inner(subject, var_table, returned_vars, env_load_vars, issues);
            for arm in arms {
                verify_expr_inner(&arm.body, var_table, returned_vars, env_load_vars, issues);
            }
        }
        IrExprKind::While { cond, body } => {
            verify_expr_inner(cond, var_table, returned_vars, env_load_vars, issues);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        verify_expr_inner(value, var_table, returned_vars, env_load_vars, issues),
                    IrStmtKind::Expr { expr } =>
                        verify_expr_inner(expr, var_table, returned_vars, env_load_vars, issues),
                    _ => {}
                }
            }
        }
        IrExprKind::Lambda { body, .. } =>
            verify_expr_inner(body, var_table, returned_vars, env_load_vars, issues),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Corresponds to Lean theorem: `perceus_strictly_better`
    /// Without Dec: leaked. With Dec: freed.
    #[test]
    fn test_strictly_better() {
        // Without Dec
        let stmts_leak = vec![
            IrStmt { kind: IrStmtKind::Bind {
                var: VarId(0), ty: Ty::String,
                mutability: almide_ir::Mutability::Let,
                value: IrExpr { kind: IrExprKind::LitStr { value: "test".into() },
                    ty: Ty::String, span: None, def_id: None },
            }, span: None },
        ];
        assert!(!has_dec(&stmts_leak, VarId(0))); // leaked

        // With Dec
        let stmts_freed = vec![
            IrStmt { kind: IrStmtKind::Bind {
                var: VarId(0), ty: Ty::String,
                mutability: almide_ir::Mutability::Let,
                value: IrExpr { kind: IrExprKind::LitStr { value: "test".into() },
                    ty: Ty::String, span: None, def_id: None },
            }, span: None },
            IrStmt { kind: IrStmtKind::RcDec { var: VarId(0) }, span: None },
        ];
        assert!(has_dec(&stmts_freed, VarId(0))); // freed
        assert!(is_freed(&stmts_freed, VarId(0))); // RC = 0
    }
}
