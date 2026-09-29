//! Speculation safety: may an expression be evaluated EARLIER, LATER, MORE OFTEN
//! or NOT AT ALL without changing what the program does?
//!
//! Several rewrites ask this. Native LICM (almide-codegen `pass_licm_purity`)
//! hoists a loop-invariant expression above a loop that might run zero times.
//! The small-scalar-fn inliner (almide-mir `inline_small_scalar_fns`, which the
//! wasm leg and the v1 native renderer both run) substitutes an argument for a
//! param that the callee might read once, twice, only in one branch, or never.
//! Each used to answer the question with its own predicate, and they
//! disagreed: the inliner counted only calls, so `pick(false, 10 / z)` lost its
//! division-by-zero error on the legs that inline and kept it on the classic
//! native codegen (#2947). The trap rules live HERE, once, and both rewrites
//! read them.
//!
//! The runtime errors a call-free expression can raise: integer `/` and `%`
//! (a zero divisor, or `MIN / -1` overflow), integer `**` (a negative
//! exponent), and index/map access out of range. Float arithmetic is total
//! (IEEE inf/NaN), and so are comparison, boolean and wrapping integer ops.

use crate::{BinOp, IrExpr, IrExprKind};

/// May this BinOp raise a runtime error? Integer division and modulo fail on a
/// zero divisor and overflow on `MIN / -1`, so only an integer-literal divisor
/// other than `0` and `-1` is statically safe. `PowInt` fails on a negative
/// exponent. Their float duals and every other op are total.
pub fn binop_may_trap(op: BinOp, right: &IrExpr) -> bool {
    match op {
        BinOp::DivInt | BinOp::ModInt => {
            !matches!(&right.kind, IrExprKind::LitInt { value } if *value != 0 && *value != -1)
        }
        BinOp::PowInt => true,
        _ => false,
    }
}

/// Is `e` a CALL-FREE expression that cannot raise a runtime error, so that a
/// rewrite may evaluate it at a different time, several times, or not at all?
///
/// Deliberately narrow: literals, variables, and operators, `if` and
/// member/tuple projections over such operands. A call, an index or map access,
/// or a trapping operator (see [`binop_may_trap`]) answers `false`. Callers
/// that know more about calls (LICM's pure-fn set) layer that on top; the
/// trap rules themselves are only here.
pub fn is_speculation_safe(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::OptionNone
        | IrExprKind::Var { .. } => true,
        IrExprKind::BinOp { op, left, right } => {
            !binop_may_trap(*op, right) && is_speculation_safe(left) && is_speculation_safe(right)
        }
        IrExprKind::UnOp { operand, .. } => is_speculation_safe(operand),
        IrExprKind::If { cond, then, else_ } => {
            is_speculation_safe(cond) && is_speculation_safe(then) && is_speculation_safe(else_)
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            is_speculation_safe(object)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ty, VarId};

    fn int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None }
    }
    fn var(n: u32) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id: VarId(n) }, ty: Ty::Int, span: None, def_id: None }
    }
    fn bin(op: BinOp, l: IrExpr, r: IrExpr) -> IrExpr {
        IrExpr {
            kind: IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) },
            ty: Ty::Int,
            span: None,
            def_id: None,
        }
    }

    #[test]
    fn integer_division_is_safe_only_by_a_literal_other_than_zero_and_minus_one() {
        assert!(is_speculation_safe(&bin(BinOp::DivInt, var(0), int(2))));
        assert!(!is_speculation_safe(&bin(BinOp::DivInt, var(0), var(1))));
        assert!(!is_speculation_safe(&bin(BinOp::DivInt, var(0), int(0))));
        assert!(!is_speculation_safe(&bin(BinOp::ModInt, var(0), int(-1))));
        assert!(!is_speculation_safe(&bin(BinOp::PowInt, var(0), int(2))));
        assert!(is_speculation_safe(&bin(BinOp::AddInt, var(0), var(1))));
    }

    #[test]
    fn a_trap_nested_under_a_total_op_is_still_a_trap() {
        let nested = bin(BinOp::AddInt, int(1), bin(BinOp::DivInt, int(10), var(0)));
        assert!(!is_speculation_safe(&nested));
    }

    #[test]
    fn index_access_is_not_speculation_safe() {
        let idx = IrExpr {
            kind: IrExprKind::IndexAccess { object: Box::new(var(0)), index: Box::new(int(7)) },
            ty: Ty::Int,
            span: None,
            def_id: None,
        };
        assert!(!is_speculation_safe(&idx));
    }
}
