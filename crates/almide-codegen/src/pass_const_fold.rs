//! ConstFoldPass: replace arithmetic on constant numeric literals with
//! their evaluated result. Mostly cleans up artifacts from earlier passes
//! (e.g. MatrixFusionPass emits `(kb * -1.0)` for sub→fma rewrites; once
//! kb is itself a literal we want a single LitFloat).
//!
//! Conservative — only folds when both operands are LitFloat or LitInt and
//! the operation is trivially safe (no divide-by-zero, no overflow on Int).

use almide_ir::*;
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ConstFoldPass;

impl NanoPass for ConstFoldPass {
    fn name(&self) -> &str { "ConstFold" }
    fn targets(&self) -> Option<Vec<Target>> { None }
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if rewrite_expr(&mut func.body) { changed = true; }
        }
        for tl in &mut program.top_lets {
            if rewrite_expr(&mut tl.value) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if rewrite_expr(&mut func.body) { changed = true; }
            }
            for tl in &mut module.top_lets {
                if rewrite_expr(&mut tl.value) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

fn rewrite_expr(expr: &mut IrExpr) -> bool {
    let mut changed = false;

    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
            if let Some(e) = tail { if rewrite_expr(e) { changed = true; } }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_expr(then) { changed = true; }
            if rewrite_expr(else_) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_expr(subject) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard { if rewrite_expr(g) { changed = true; } }
                if rewrite_expr(&mut arm.body) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_expr(iterable) { changed = true; }
            for stmt in body.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_expr(cond) { changed = true; }
            for stmt in body.iter_mut() { if rewrite_stmt(stmt) { changed = true; } }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_expr(body) { changed = true; }
        }
        IrExprKind::Call { args, .. } => {
            for a in args.iter_mut() { if rewrite_expr(a) { changed = true; } }
        }
        IrExprKind::BinOp { left, right, .. } => {
            if rewrite_expr(left) { changed = true; }
            if rewrite_expr(right) { changed = true; }
        }
        IrExprKind::UnOp { operand, .. } => {
            if rewrite_expr(operand) { changed = true; }
        }
        _ => {}
    }

    if let IrExprKind::BinOp { op, left, right } = &expr.kind {
        if let Some(folded) = try_fold(*op, left, right) {
            expr.kind = folded;
            changed = true;
        }
    }
    if let IrExprKind::UnOp { op: UnOp::NegFloat, operand } = &expr.kind {
        if let IrExprKind::LitFloat { value } = &operand.kind {
            expr.kind = IrExprKind::LitFloat { value: -*value };
            changed = true;
        }
    }
    if let IrExprKind::UnOp { op: UnOp::NegInt, operand } = &expr.kind {
        if let IrExprKind::LitInt { value } = &operand.kind {
            expr.kind = IrExprKind::LitInt { value: -*value };
            changed = true;
        }
    }

    changed
}

fn try_fold(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    // Float arithmetic
    if let (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b })
        = (&left.kind, &right.kind) {
        let v = match op {
            BinOp::AddFloat => Some(a + b),
            BinOp::SubFloat => Some(a - b),
            BinOp::MulFloat => Some(a * b),
            // Avoid 0/0; let it stay as IR so runtime gets NaN.
            BinOp::DivFloat if *b != 0.0 => Some(a / b),
            _ => None,
        };
        if let Some(v) = v {
            return Some(IrExprKind::LitFloat { value: v });
        }
    }
    // Int arithmetic — checked to avoid silent wrap.
    if let (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b })
        = (&left.kind, &right.kind) {
        let v = match op {
            BinOp::AddInt => a.checked_add(*b),
            BinOp::SubInt => a.checked_sub(*b),
            BinOp::MulInt => a.checked_mul(*b),
            BinOp::DivInt if *b != 0 => a.checked_div(*b),
            BinOp::ModInt if *b != 0 => a.checked_rem(*b),
            _ => None,
        };
        if let Some(v) = v {
            return Some(IrExprKind::LitInt { value: v });
        }
    }
    // Identity / annihilator simplifications (keeps types intact via left.ty)
    let is_zero_f = |e: &IrExpr| matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 0.0);
    let is_one_f  = |e: &IrExpr| matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 1.0);
    let is_zero_i = |e: &IrExpr| matches!(&e.kind, IrExprKind::LitInt { value } if *value == 0);
    let is_one_i  = |e: &IrExpr| matches!(&e.kind, IrExprKind::LitInt { value } if *value == 1);
    match op {
        // x + 0 / 0 + x → x
        BinOp::AddFloat if is_zero_f(right) => return Some(left.kind.clone()),
        BinOp::AddFloat if is_zero_f(left) => return Some(right.kind.clone()),
        BinOp::AddInt if is_zero_i(right) => return Some(left.kind.clone()),
        BinOp::AddInt if is_zero_i(left) => return Some(right.kind.clone()),
        // x - 0 → x  (not 0 - x; that's negation, leave alone)
        BinOp::SubFloat if is_zero_f(right) => return Some(left.kind.clone()),
        BinOp::SubInt if is_zero_i(right) => return Some(left.kind.clone()),
        // x * 1 / 1 * x → x
        BinOp::MulFloat if is_one_f(right) => return Some(left.kind.clone()),
        BinOp::MulFloat if is_one_f(left) => return Some(right.kind.clone()),
        BinOp::MulInt if is_one_i(right) => return Some(left.kind.clone()),
        BinOp::MulInt if is_one_i(left) => return Some(right.kind.clone()),
        // x / 1 → x
        BinOp::DivFloat if is_one_f(right) => return Some(left.kind.clone()),
        BinOp::DivInt if is_one_i(right) => return Some(left.kind.clone()),
        _ => {}
    }
    None
}

fn rewrite_stmt(stmt: &mut IrStmt) -> bool {
    let mut changed = false;
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. }
        | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. }
        | IrStmtKind::FieldAssign { value, .. } => {
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            if rewrite_expr(index) { changed = true; }
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            if rewrite_expr(key) { changed = true; }
            if rewrite_expr(value) { changed = true; }
        }
        IrStmtKind::Guard { cond, else_ } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_expr(else_) { changed = true; }
        }
        IrStmtKind::Expr { expr } => {
            if rewrite_expr(expr) { changed = true; }
        }
        _ => {}
    }
    changed
}

// Suppress unused import warning.
#[allow(dead_code)]
fn _ty_marker(_: Ty) {}
