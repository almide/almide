//! StackBalancePass: guarantee WASM stack balance by demoting non-Unit
//! tail expressions in void contexts to Expr statements.
//!
//! Runs after ANF, before Perceus. This ordering is critical:
//!   ANF → **StackBalance** → Perceus → PerceusOpt → PerceusVerify
//!
//! Perceus converts Block tails to `FnBody::Ret`, then inserts RcDec
//! before the Ret. Variables used in the Ret expression are excluded
//! from Dec (ownership transfers to the caller). If the block is in a
//! void context, the "returned" value is discarded — but Perceus has
//! already skipped the Dec, causing a leak.
//!
//! By running before Perceus, this pass ensures void-context blocks
//! have no tails (Nop terminus in FnBody). Perceus then correctly
//! Dec's all heap vars at scope exit.
//!
//! Invariant (post-pass):
//!   For every Block B that is the body of a void function, or is
//!   nested inside an Expr statement: B.tail == None.
//!
//! This is the stack-balance analogue of Perceus's RC-balance guarantee:
//! Perceus ensures every heap alloc is freed exactly once; StackBalance
//! ensures every WASM block has the correct number of values on exit.

use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

/// A type is void if it produces no WASM stack value.
fn is_void(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit | Ty::Never)
}

/// Balance an expression given its context.
///
/// `expected_void`: true if the enclosing context expects no stack value
/// (void function body, Expr statement, etc.).
///
/// For Block expressions in void context: demotes the tail to an Expr
/// statement. The WASM emitter's Expr-stmt handler automatically drops
/// values produced by the expression.
///
/// Returns true if any IR was modified.
fn balance_expr(expr: &mut IrExpr, expected_void: bool) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Process existing statements (may contain void-context sub-blocks)
            for stmt in stmts.iter_mut() {
                if balance_stmt(stmt) { changed = true; }
            }

            if expected_void {
                if let Some(tail_expr) = tail.take() {
                    // Demote: move tail to Expr statement.
                    // The emitter's Expr-stmt handler will drop any produced value.
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Expr { expr: *tail_expr },
                        span: None,
                    });
                    expr.ty = Ty::Unit;
                    changed = true;
                }
            } else if let Some(t) = tail {
                // Non-void context: recurse into tail for nested blocks
                if balance_expr(t, false) { changed = true; }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if balance_expr(cond, false) { changed = true; }
            if balance_expr(then, false) { changed = true; }
            if balance_expr(else_, false) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if balance_expr(subject, false) { changed = true; }
            for arm in arms.iter_mut() {
                if balance_expr(&mut arm.body, false) { changed = true; }
            }
        }
        IrExprKind::While { cond, body } => {
            if balance_expr(cond, false) { changed = true; }
            for stmt in body.iter_mut() {
                if balance_stmt(stmt) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if balance_expr(iterable, false) { changed = true; }
            for stmt in body.iter_mut() {
                if balance_stmt(stmt) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambda bodies are expression context (return their value)
            if balance_expr(body, false) { changed = true; }
        }
        _ => {}
    }
    changed
}

/// Balance statements. Dispatches to balance_expr with appropriate context.
fn balance_stmt(stmt: &mut IrStmt) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
            // Value is consumed by local_set — expression context
            balance_expr(value, false)
        }
        IrStmtKind::Expr { expr } => {
            // Value is discarded — void context
            balance_expr(expr, true)
        }
        IrStmtKind::Guard { cond, else_ } => {
            let a = balance_expr(cond, false);
            let b = balance_expr(else_, false);
            a || b
        }
        _ => false,
    }
}

#[derive(Debug)]
pub struct StackBalancePass;

impl NanoPass for StackBalancePass {
    fn name(&self) -> &str { "StackBalance" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if func.is_test { continue; }
            let void_fn = is_void(&func.ret_ty);
            if balance_expr(&mut func.body, void_fn) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if func.is_test { continue; }
                let void_fn = is_void(&func.ret_ty);
                if balance_expr(&mut func.body, void_fn) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}
