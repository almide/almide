//! MC/DC independence-pair vectors for the multi-condition decisions in
//! `perceus_verified.rs` (proofs/mcdc-ledger.toml, #566 rung 2).
//!
//! Each test toggles EXACTLY ONE condition of its decision between two runs
//! and asserts the outcome flips — the MC/DC independent-effect argument,
//! written as pinned unit tests because rustc's MC/DC instrumentation was
//! removed upstream (rust-lang/rust#144999).

use almide_codegen::perceus_verified::{verify_expr, verify_rc_balance};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::types::Ty;
use std::collections::HashSet;

fn table(mutability: Mutability) -> (VarTable, VarId) {
    let mut vt = VarTable::new();
    let v = vt.alloc(almide_base::intern::sym("x"), Ty::String, mutability, None);
    (vt, v)
}

fn lit() -> IrExpr {
    IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::String, span: None, def_id: None }
}

fn bind(v: VarId, mutability: Mutability) -> IrStmt {
    IrStmt { kind: IrStmtKind::Bind { var: v, ty: Ty::String, mutability, value: lit() }, span: None }
}

fn dec(v: VarId) -> IrStmt { IrStmt { kind: IrStmtKind::RcDec { var: v }, span: None } }
fn inc(v: VarId) -> IrStmt { IrStmt { kind: IrStmtKind::RcInc { var: v }, span: None } }

fn has_double_free(issues: &[(VarId, &'static str)]) -> bool {
    issues.iter().any(|(_, m)| m.contains("DOUBLE-FREE"))
}
fn has_leak(issues: &[(VarId, &'static str)]) -> bool {
    issues.iter().any(|(_, m)| m.contains("LEAK"))
}

// ── Site perceus_verified.rs:87 — `!matches!(mutability, Var) && decs > incs + 1` ──

#[test]
fn site87_baseline_immutable_overdec_reports_double_free() {
    let (vt, v) = table(Mutability::Let);
    let stmts = vec![bind(v, Mutability::Let), dec(v), dec(v)];
    assert!(has_double_free(&verify_rc_balance(&stmts, &vt)));
}

#[test]
fn site87_c1_mutability_alone_suppresses_double_free() {
    // Only C1 (immutability) flips vs the baseline: same decs/incs.
    let (vt, v) = table(Mutability::Var);
    let stmts = vec![bind(v, Mutability::Var), dec(v), dec(v)];
    assert!(!has_double_free(&verify_rc_balance(&stmts, &vt)));
}

#[test]
fn site87_c2_balance_alone_suppresses_double_free() {
    // Only C2 (decs > incs + 1) flips vs the baseline: an extra inc rebalances.
    let (vt, v) = table(Mutability::Let);
    let stmts = vec![bind(v, Mutability::Let), inc(v), dec(v), dec(v)];
    assert!(!has_double_free(&verify_rc_balance(&stmts, &vt)));
}

// ── Sites perceus_verified.rs:207 (two operators) —
//    `decs == 0 && !is_mutable && !moved_out.contains(var)` — and
//    site 213 — `!is_mutable && decs > incs + 1` — via the public verify_expr.

fn block(stmts: Vec<IrStmt>) -> IrExpr {
    IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span: None, def_id: None }
}

fn run_verify(
    stmts: Vec<IrStmt>,
    vt: &VarTable,
    moved_out: &HashSet<VarId>,
) -> Vec<(VarId, &'static str)> {
    let empty = HashSet::new();
    verify_expr(&block(stmts), vt, &empty, moved_out, &empty)
}

#[test]
fn site207_baseline_undecced_immutable_unmoved_reports_leak() {
    let (vt, v) = table(Mutability::Let);
    assert!(has_leak(&run_verify(vec![bind(v, Mutability::Let)], &vt, &HashSet::new())));
}

#[test]
fn site207_c1_a_dec_alone_suppresses_the_leak() {
    let (vt, v) = table(Mutability::Let);
    assert!(!has_leak(&run_verify(vec![bind(v, Mutability::Let), dec(v)], &vt, &HashSet::new())));
}

#[test]
fn site207_c2_mutability_alone_suppresses_the_leak() {
    let (vt, v) = table(Mutability::Var);
    assert!(!has_leak(&run_verify(vec![bind(v, Mutability::Var)], &vt, &HashSet::new())));
}

#[test]
fn site207_c3_moved_out_alone_suppresses_the_leak() {
    let (vt, v) = table(Mutability::Let);
    let moved: HashSet<VarId> = [v].into_iter().collect();
    assert!(!has_leak(&run_verify(vec![bind(v, Mutability::Let)], &vt, &moved)));
}

#[test]
fn site213_baseline_immutable_overdec_reports_double_free() {
    let (vt, v) = table(Mutability::Let);
    assert!(has_double_free(&run_verify(vec![bind(v, Mutability::Let), dec(v), dec(v)], &vt, &HashSet::new())));
}

#[test]
fn site213_c1_mutability_alone_suppresses_double_free() {
    let (vt, v) = table(Mutability::Var);
    assert!(!has_double_free(&run_verify(vec![bind(v, Mutability::Var), dec(v), dec(v)], &vt, &HashSet::new())));
}

#[test]
fn site213_c2_balance_alone_suppresses_double_free() {
    let (vt, v) = table(Mutability::Let);
    assert!(!has_double_free(&run_verify(vec![bind(v, Mutability::Let), inc(v), dec(v), dec(v)], &vt, &HashSet::new())));
}
