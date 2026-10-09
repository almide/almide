//! A `match` over `string.split_once(s, sep)` that destructures the pair
//! in place never builds the pair (front_desugar.rs pass 5).
//!
//! On this leg `(String, String)?` is two heap blocks — the option cell and
//! the tuple — around the two pieces, and a `match` that immediately takes
//! the pieces apart released both blocks again: four allocations per call
//! where the pieces are the only values the program ever reads (onebrc's
//! `step` / `parse_tenths`: 8 of its 17 allocations per line). The shape is
//! the scalar replacement of an immediately-scrutinised constructor result
//! — the case-of-known-constructor / unboxed-sum return of GHC's worker /
//! wrapper split and Koka's and Lean's value-type returns (references
//! `../almide-references/`: Koka passes small value types in registers,
//! Lean's IR `ctor`-elimination on a projected result) — done here at the
//! one call whose constructor this leg knows statically:
//!
//!   match string.split_once(s, sep) { some((a, b)) => A, none => B }
//!   =>
//!   { let __at = string."split_once#at"(s, sep)
//!     if __at >= 0 then { let a = string."split_once#head"(s, __at)
//!                         let b = string."split_once#tail"(s, sep, __at)
//!                         A }
//!     else B }
//!
//! The three ops are the split_once kernel cut in three (string_ext.rs,
//! byte-for-byte the self-hosted body: the FIRST byte occurrence, an empty
//! separator hits at 0). Their names carry a `#`, which no source can
//! spell, so no program reaches them but through this rewrite. `s` and
//! `sep` are read more than once, so any operand that is not a plain read
//! is bound first, in operand order. `a` / `b` become ordinary `let`s that
//! own their piece (the frame releases them like any local); a wildcard
//! element builds no piece. Arms with a guard, a nested pattern, or a
//! third arm keep the call.

use almide_base::intern::sym;
use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, IrStmtKind, Mutability, VarTable};
use almide_types::types::Ty;

/// The op names the rewrite emits (string_ext.rs lowers them).
pub(crate) const AT: &str = "split_once#at";
pub(crate) const HEAD: &str = "split_once#head";
pub(crate) const TAIL: &str = "split_once#tail";

/// The two binders of the `some((a, b))` arm (None = wildcard), its body,
/// and the `none` arm's body — when `arms` is the destructuring shape.
fn shape(arms: &[IrMatchArm]) -> Option<([Option<(almide_ir::VarId, Ty)>; 2], &IrExpr, &IrExpr)> {
    let [x, y] = arms else { return None };
    if x.guard.is_some() || y.guard.is_some() {
        return None;
    }
    let (some, none) = match (&x.pattern, &y.pattern) {
        (IrPattern::Some { .. }, IrPattern::None | IrPattern::Wildcard) => (x, y),
        (IrPattern::None, IrPattern::Some { .. }) => (y, x),
        _ => return None,
    };
    let IrPattern::Some { inner } = &some.pattern else { return None };
    let IrPattern::Tuple { elements } = inner.as_ref() else { return None };
    let [p, q] = elements.as_slice() else { return None };
    let piece = |p: &IrPattern| match p {
        IrPattern::Bind { var, ty } => Some(Some((*var, ty.clone()))),
        IrPattern::Wildcard => Some(None),
        _ => None,
    };
    Some(([piece(p)?, piece(q)?], &some.body, &none.body))
}

fn expr(kind: IrExprKind, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    IrExpr { kind, ty, span, def_id: None }
}

fn op(func: &str, args: Vec<IrExpr>, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    let target = CallTarget::Module { module: sym("string"), func: sym(func), def_id: None };
    expr(IrExprKind::Call { target, args, type_args: vec![] }, ty, span)
}

fn bind(var: almide_ir::VarId, ty: Ty, value: IrExpr) -> IrStmt {
    let span = value.span;
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span }
}

/// A plain read: evaluating it twice is evaluating it once.
fn plain(e: &IrExpr) -> bool {
    matches!(e.kind, IrExprKind::Var { .. } | IrExprKind::LitStr { .. })
}

/// Rewrite `e` in place when it is the shape; `true` when it was.
pub(crate) fn rewrite(vars: &mut VarTable, e: &mut IrExpr) -> bool {
    let IrExprKind::Match { subject, arms } = &e.kind else { return false };
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &subject.kind else {
        return false;
    };
    if module.as_str() != "string" || func.as_str() != "split_once" || args.len() != 2 {
        return false;
    }
    let Some((pieces, some_body, none_body)) = shape(arms) else { return false };
    let (some_body, none_body) = (some_body.clone(), none_body.clone());
    let (span, ty) = (e.span, e.ty.clone());
    let mut stmts = Vec::new();
    // The operands, each read in place when plain, else bound once.
    let mut operand = |a: &IrExpr, stmts: &mut Vec<IrStmt>| -> IrExpr {
        if plain(a) {
            return a.clone();
        }
        let id = vars.alloc(sym("__split_arg"), a.ty.clone(), Mutability::Let, a.span);
        stmts.push(bind(id, a.ty.clone(), a.clone()));
        expr(IrExprKind::Var { id }, a.ty.clone(), a.span)
    };
    let s = operand(&args[0], &mut stmts);
    let sep = operand(&args[1], &mut stmts);
    let at = vars.alloc(sym("__split_at"), Ty::Int, Mutability::Let, span);
    stmts.push(bind(at, Ty::Int, op(AT, vec![s.clone(), sep.clone()], Ty::Int, span)));
    let at_read = expr(IrExprKind::Var { id: at }, Ty::Int, span);
    let mut hit = Vec::new();
    if let Some((a, aty)) = &pieces[0] {
        hit.push(bind(*a, aty.clone(), op(HEAD, vec![s.clone(), at_read.clone()], Ty::String, span)));
    }
    if let Some((b, bty)) = &pieces[1] {
        hit.push(bind(*b, bty.clone(), op(TAIL, vec![s, sep, at_read.clone()], Ty::String, span)));
    }
    let then = expr(IrExprKind::Block { stmts: hit, expr: Some(Box::new(some_body)) }, ty.clone(), span);
    let zero = expr(IrExprKind::LitInt { value: 0 }, Ty::Int, span);
    let cond = expr(IrExprKind::BinOp { op: BinOp::Gte, left: Box::new(at_read), right: Box::new(zero) }, Ty::Bool, span);
    let branch = IrExprKind::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(none_body) };
    let tail = expr(branch, ty.clone(), span);
    *e = expr(IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty, span);
    true
}
