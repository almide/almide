//! ConstFoldPass: replace arithmetic on constant numeric literals with
//! their evaluated result. Mostly cleans up artifacts from earlier passes
//! (e.g. MatrixFusionPass emits `(kb * -1.0)` for sub→fma rewrites; once
//! kb is itself a literal we want a single LitFloat).
//!
//! Conservative — only folds when both operands are LitFloat or LitInt and
//! the operation is trivially safe (no divide-by-zero, no overflow on Int).
//!
//! Traversal goes through the canonical `IrMutVisitor`/`walk_expr_mut`
//! (exhaustive, wildcard-free) rather than a hand-rolled `match expr.kind { …;
//! _ => {} }`, so a foldable subtree under any wrapper / future node kind is
//! reached — no silent drop (see docs/roadmap/active/codegen-traversal-totality.md).

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ConstFoldPass;

impl NanoPass for ConstFoldPass {
    fn name(&self) -> &str { "ConstFold" }
    fn targets(&self) -> Option<Vec<Target>> { None }
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut folder = ConstFolder { changed: false };
        for func in &mut program.functions {
            folder.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            folder.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                folder.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                folder.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: folder.changed }
    }
}

/// Bottom-up fold: descend into every child via the exhaustive `walk_expr_mut`,
/// then fold this node if it is a constant arithmetic op (so a parent sees its
/// already-folded children).
struct ConstFolder {
    changed: bool,
}

impl IrMutVisitor for ConstFolder {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);

        if let IrExprKind::BinOp { op, left, right } = &expr.kind {
            if let Some(folded) = try_fold(*op, left, right) {
                expr.kind = folded;
                self.changed = true;
            }
        }
        if let IrExprKind::UnOp { op: UnOp::NegFloat, operand } = &expr.kind {
            if let IrExprKind::LitFloat { value } = &operand.kind {
                expr.kind = IrExprKind::LitFloat { value: -*value };
                self.changed = true;
            }
        }
        if let IrExprKind::UnOp { op: UnOp::NegInt, operand } = &expr.kind {
            if let IrExprKind::LitInt { value } = &operand.kind {
                expr.kind = IrExprKind::LitInt { value: -*value };
                self.changed = true;
            }
        }
    }
}

fn try_fold(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    try_fold_float(op, left, right)
        .or_else(|| try_fold_int(op, left, right))
        .or_else(|| try_fold_identity(op, left, right))
}

/// Float-arithmetic phase of `try_fold`, extracted verbatim (cog>30
/// decomposition, pattern 1 — the three phases share no state and each
/// independently returns `Some`/`None`).
fn try_fold_float(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    let (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b })
        = (&left.kind, &right.kind) else { return None };
    let v = match op {
        BinOp::AddFloat => Some(a + b),
        BinOp::SubFloat => Some(a - b),
        BinOp::MulFloat => Some(a * b),
        // Avoid 0/0; let it stay as IR so runtime gets NaN.
        BinOp::DivFloat if *b != 0.0 => Some(a / b),
        _ => None,
    };
    v.map(|v| IrExprKind::LitFloat { value: v })
}

/// Int-arithmetic phase of `try_fold`, extracted verbatim (cog>30
/// decomposition) — checked to avoid silent wrap.
fn try_fold_int(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    let (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b })
        = (&left.kind, &right.kind) else { return None };
    let v = match op {
        BinOp::AddInt => a.checked_add(*b),
        BinOp::SubInt => a.checked_sub(*b),
        BinOp::MulInt => a.checked_mul(*b),
        BinOp::DivInt if *b != 0 => a.checked_div(*b),
        BinOp::ModInt if *b != 0 => a.checked_rem(*b),
        _ => None,
    };
    v.map(|v| IrExprKind::LitInt { value: v })
}

fn is_zero_f(e: &IrExpr) -> bool { matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 0.0) }
fn is_one_f(e: &IrExpr) -> bool { matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 1.0) }
fn is_zero_i(e: &IrExpr) -> bool { matches!(&e.kind, IrExprKind::LitInt { value } if *value == 0) }
fn is_one_i(e: &IrExpr) -> bool { matches!(&e.kind, IrExprKind::LitInt { value } if *value == 1) }

/// Identity / annihilator simplification phase of `try_fold`, extracted
/// verbatim (cog>30 decomposition) — keeps types intact via `left.ty`. The
/// three groups (add/sub, mul/div — split by operator family) share no
/// state, so `.or_else()`-chained same as `try_fold` itself.
fn try_fold_identity(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    try_fold_identity_add_sub(op, left, right)
        .or_else(|| try_fold_identity_mul_div(op, left, right))
}

/// `+`/`-` identities of `try_fold_identity`, extracted verbatim (further
/// split of the same decomposition).
fn try_fold_identity_add_sub(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    match op {
        // x + 0 / 0 + x → x
        BinOp::AddFloat if is_zero_f(right) => Some(left.kind.clone()),
        BinOp::AddFloat if is_zero_f(left) => Some(right.kind.clone()),
        BinOp::AddInt if is_zero_i(right) => Some(left.kind.clone()),
        BinOp::AddInt if is_zero_i(left) => Some(right.kind.clone()),
        // x - 0 → x  (not 0 - x; that's negation, leave alone)
        BinOp::SubFloat if is_zero_f(right) => Some(left.kind.clone()),
        BinOp::SubInt if is_zero_i(right) => Some(left.kind.clone()),
        _ => None,
    }
}

/// `*`/`/` identities of `try_fold_identity`, extracted verbatim (further
/// split of the same decomposition).
fn try_fold_identity_mul_div(op: BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
    match op {
        // x * 1 / 1 * x → x
        BinOp::MulFloat if is_one_f(right) => Some(left.kind.clone()),
        BinOp::MulFloat if is_one_f(left) => Some(right.kind.clone()),
        BinOp::MulInt if is_one_i(right) => Some(left.kind.clone()),
        BinOp::MulInt if is_one_i(left) => Some(right.kind.clone()),
        // x / 1 → x
        BinOp::DivFloat if is_one_f(right) => Some(left.kind.clone()),
        BinOp::DivInt if is_one_i(right) => Some(left.kind.clone()),
        _ => None,
    }
}
