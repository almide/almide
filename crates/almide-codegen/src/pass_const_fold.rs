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
    v.map(|v| IrExprKind::LitInt { value: narrow_to_width(v, &left.ty) })
}

/// Wrap a folded value into the two's-complement range of `ty`.
///
/// The fold runs at i64 width, but the result is EMITTED as a literal of the
/// operand's declared type: `let a: Int8 = 127; let b: Int8 = 1; a + b` folded
/// to `128` and rendered `128i8`, which rustc rejects outright — a program that
/// `check` accepted and could not build, while the wasm leg (which never sees a
/// Rust literal) printed the correct `-128` (#901). The RUNTIME path already
/// wraps (#889); this is the same rule at fold time, so the two agree instead of
/// disagreeing on whether the program exists. Canonical `Int` is i64-wide
/// already and is returned unchanged.
fn narrow_to_width(v: i64, ty: &almide_lang::types::Ty) -> i64 {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int8 => v as i8 as i64,
        Ty::Int16 => v as i16 as i64,
        Ty::Int32 => v as i32 as i64,
        Ty::UInt8 => v as u8 as i64,
        Ty::UInt16 => v as u16 as i64,
        Ty::UInt32 => v as u32 as i64,
        _ => v,
    }
}

// The ±0 split matters (#1542): `x + 0.0 -> x` is NOT a valid IEEE 754
// identity — under round-to-nearest `-0.0 + 0.0 = +0.0`, so folding it made
// native print -0.0 where wasm (which never folds) printed 0.0: a
// cross-target divergence. The float identities are SIGN-precise:
// `x + (-0.0) -> x` and `x - (+0.0) -> x` (LLVM folds exactly these two).
// `== 0.0` compares true for BOTH zeros, so the Add/Sub arms key on the sign.
fn is_pos_zero_f(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 0.0 && !value.is_sign_negative())
}
fn is_neg_zero_f(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::LitFloat { value } if *value == 0.0 && value.is_sign_negative())
}
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
        // FLOAT: only the sign-precise identities (#1542, see is_pos/neg_zero_f):
        // x + (-0.0) / (-0.0) + x → x; x - (+0.0) → x. `x + 0.0` and `0.0 + x`
        // must EVALUATE (they normalize -0.0 to +0.0); `x - (-0.0)` likewise.
        BinOp::AddFloat if is_neg_zero_f(right) => Some(left.kind.clone()),
        BinOp::AddFloat if is_neg_zero_f(left) => Some(right.kind.clone()),
        // INT: x + 0 / 0 + x → x (exact, no signed zero).
        BinOp::AddInt if is_zero_i(right) => Some(left.kind.clone()),
        BinOp::AddInt if is_zero_i(left) => Some(right.kind.clone()),
        // x - 0 → x  (not 0 - x; that's negation, leave alone)
        BinOp::SubFloat if is_pos_zero_f(right) => Some(left.kind.clone()),
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
