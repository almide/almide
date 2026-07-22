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
//! The pass propagates `expected_void` through ALL control flow:
//!   - Block: demote tail to Expr stmt
//!   - If/Match: propagate to branches, update block type to Unit
//!   - Leaf expressions (Var, Call, Lit): wrap in void Block
//!
//! Invariant (post-pass):
//!   In every void context, no expression pushes a value onto the
//!   WASM stack. Blocks have no tails, If/Match have BlockType::Empty,
//!   and leaf expressions are wrapped in Expr statements that drop.

use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

/// A type is void if it produces no WASM stack value.
fn is_void(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit | Ty::Never)
}

/// Wrap a non-Unit expression in `Block { Expr(original) }` so its
/// value is emitted-and-dropped rather than left on the stack.
fn void_wrap(expr: &mut IrExpr) -> bool {
    if is_void(&expr.ty) { return false; }
    let inner = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None,
    });
    *expr = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: inner }, span: None }],
            expr: None,
        },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    true
}

/// `IrExprKind::Block` case of `balance_expr`, extracted verbatim (cog>30
/// decomposition, pattern 2: uniform match arms, mirrors the
/// `lower_expr`/`infer_expr_inner` extraction shape). Every
/// `if X() { changed = true; }` became `changed |= X()` — provably
/// equivalent (see `pass_capture_clone::transform_expr`'s precedent for
/// the same rewrite) — except the unconditional `changed = true;` after
/// the tail-demotion, which stays literal since it doesn't depend on the
/// recursive call's own result.
fn balance_expr_block(expr: &mut IrExpr, expected_void: bool) -> bool {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    let mut changed = false;
    // Process existing statements
    for stmt in stmts.iter_mut() {
        changed |= balance_stmt(stmt);
    }

    if expected_void {
        if let Some(tail_expr) = tail.take() {
            // Recurse into tail with void context before demoting.
            // This handles nested If/Match/Block inside the tail.
            let mut t = *tail_expr;
            changed |= balance_expr(&mut t, true);
            // Demote: move tail to Expr statement.
            stmts.push(IrStmt {
                kind: IrStmtKind::Expr { expr: t },
                span: None,
            });
            expr.ty = Ty::Unit;
            changed = true;
        }
    } else if let Some(t) = tail {
        // Non-void context: recurse into tail for nested structures
        changed |= balance_expr(t, false);
    }
    changed
}

/// `IrExprKind::If` case of `balance_expr`, extracted verbatim.
fn balance_expr_if(expr: &mut IrExpr, expected_void: bool) -> bool {
    let IrExprKind::If { cond, then, else_ } = &mut expr.kind else { unreachable!() };
    let mut changed = balance_expr(cond, false);
    // Propagate void context into branches
    changed |= balance_expr(then, expected_void);
    changed |= balance_expr(else_, expected_void);
    // Update If type so WASM emitter uses BlockType::Empty
    if expected_void && !is_void(&expr.ty) {
        expr.ty = Ty::Unit;
        changed = true;
    }
    changed
}

/// `IrExprKind::Match` case of `balance_expr`, extracted verbatim.
fn balance_expr_match(expr: &mut IrExpr, expected_void: bool) -> bool {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    let mut changed = balance_expr(subject, false);
    for arm in arms.iter_mut() {
        changed |= balance_expr(&mut arm.body, expected_void);
    }
    if expected_void && !is_void(&expr.ty) {
        expr.ty = Ty::Unit;
        changed = true;
    }
    changed
}

/// Balance an expression given its context.
///
/// `expected_void`: true if the enclosing context expects no stack value
/// (void function body, Expr statement, If/Match branch in void context).
///
/// Returns true if any IR was modified.
fn balance_expr(expr: &mut IrExpr, expected_void: bool) -> bool {
    match &mut expr.kind {
        IrExprKind::Block { .. } => balance_expr_block(expr, expected_void),
        IrExprKind::If { .. } => balance_expr_if(expr, expected_void),
        IrExprKind::Match { .. } => balance_expr_match(expr, expected_void),
        IrExprKind::While { cond, body } => {
            let mut changed = balance_expr(cond, false);
            for stmt in body.iter_mut() {
                changed |= balance_stmt(stmt);
            }
            changed
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            let mut changed = balance_expr(iterable, false);
            for stmt in body.iter_mut() {
                changed |= balance_stmt(stmt);
            }
            changed
        }
        IrExprKind::Lambda { body, .. } => {
            // Lambda bodies are expression context (return their value)
            balance_expr(body, false)
        }
        _ => {
            // Leaf expression in void context that produces a value:
            // wrap in Block { Expr(original) } so the emitter drops it.
            if expected_void {
                void_wrap(expr)
            } else {
                false
            }
        }
    }
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
    // #559: the StackBalance→Perceus ordering is load-bearing (Perceus RC
    // insertion assumes a balanced stack); declared as a before-dep since
    // Perceus does not name the reverse. Vacuous on the Rust arm where
    // StackBalance does not run.
    fn run_before(&self) -> Vec<&'static str> { vec!["Perceus"] }

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
