//! Induction-variable strength reduction for a fused chain over a counted
//! range (#4610): when the first map is AFFINE in its element,
//! `(x) => a * x + b` built from literals with `+ - *` and negation on plain
//! Int, its value over `cur, cur + 1, ...` is carried in one i64 local that
//! starts at `a * start + b` and steps by `a`, instead of a multiply per
//! element.
//!
//! WHY IT HOLDS: Int `+ - *` WRAP (C-056), i.e. they are the ring Z/2^64, where
//! `a * (s + i) + b = (a * s + b) + i * a` exactly, for every start `s`
//! (i64::MIN included) and every step count. The body reads only its element
//! and literals, can neither abort nor observe anything, so not evaluating it
//! per element is unobservable.

use almide_ir::{BinOp, IrExpr, IrExprKind, UnOp, VarId};
use almide_types::types::Ty;

use crate::emitter::Emitter;
use crate::*;

/// `e` as `a * p + b` (wrapping), and whether it multiplies. `None` when it
/// reads anything but `p` and literals, or leaves plain Int.
pub(crate) fn affine_in(e: &IrExpr, p: VarId) -> Option<(i64, i64, bool)> {
    if !matches!(e.ty, Ty::Int) {
        return None;
    }
    match &e.kind {
        IrExprKind::Var { id } if *id == p => Some((1, 0, false)),
        IrExprKind::LitInt { value } => Some((0, *value, false)),
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => {
            let (a, b, m) = affine_in(operand, p)?;
            Some((a.wrapping_neg(), b.wrapping_neg(), m))
        }
        IrExprKind::BinOp { op, left, right } => {
            let (la, lb, lm) = affine_in(left, p)?;
            let (ra, rb, rm) = affine_in(right, p)?;
            match op {
                BinOp::AddInt => Some((la.wrapping_add(ra), lb.wrapping_add(rb), lm || rm)),
                BinOp::SubInt => Some((la.wrapping_sub(ra), lb.wrapping_sub(rb), lm || rm)),
                // A product stays affine when one side is a constant.
                BinOp::MulInt if la == 0 => Some((lb.wrapping_mul(ra), lb.wrapping_mul(rb), true)),
                BinOp::MulInt if ra == 0 => Some((rb.wrapping_mul(la), rb.wrapping_mul(lb), true)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The carried value: its local and its per-element step.
#[derive(Clone, Copy)]
pub(crate) struct Iv {
    pub(crate) local: u32,
    pub(crate) step: i64,
}

impl Emitter<'_> {
    /// Open the induction variable for map `f` over the counter `cur`, which
    /// holds the range's start here. `None` = not an affine map that
    /// multiplies, or no i64 hold is free; the chain lowers as before.
    pub(crate) fn open_affine_iv(&mut self, f: &IrExpr, cur: u32) -> Option<Iv> {
        let IrExprKind::Lambda { params, body, .. } = &f.kind else { return None };
        let [(p, _)] = params.as_slice() else { return None };
        let (a, b, multiplies) = affine_in(body, *p)?;
        if !multiplies {
            return None;
        }
        let local = self.hold_i64().ok()?;
        let mut i = self.f.instructions();
        i.local_get(cur).i64_const(a).i64_mul();
        if b != 0 {
            i.i64_const(b).i64_add();
        }
        i.local_set(local);
        Some(Iv { local, step: a })
    }

    /// Advance the induction variable with the counter.
    pub(crate) fn step_affine_iv(&mut self, iv: Iv) {
        self.f.instructions().local_get(iv.local).i64_const(iv.step).i64_add().local_set(iv.local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Int, span: None, def_id: None }
    }
    fn lit(v: i64) -> IrExpr {
        e(IrExprKind::LitInt { value: v })
    }
    fn var(n: u32) -> IrExpr {
        e(IrExprKind::Var { id: VarId(n) })
    }
    fn bin(op: BinOp, l: IrExpr, r: IrExpr) -> IrExpr {
        e(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) })
    }
    fn eval(e: &IrExpr, x: i64) -> i64 {
        match &e.kind {
            IrExprKind::Var { .. } => x,
            IrExprKind::LitInt { value } => *value,
            IrExprKind::UnOp { operand, .. } => eval(operand, x).wrapping_neg(),
            IrExprKind::BinOp { op, left, right } => {
                let (l, r) = (eval(left, x), eval(right, x));
                match op {
                    BinOp::AddInt => l.wrapping_add(r),
                    BinOp::SubInt => l.wrapping_sub(r),
                    _ => l.wrapping_mul(r),
                }
            }
            _ => unreachable!(),
        }
    }

    /// The carried value equals the body at every element, wrapping included.
    #[test]
    fn the_carried_value_equals_the_body_at_every_element() {
        let bodies = [
            bin(BinOp::MulInt, var(0), lit(3)),
            bin(BinOp::AddInt, bin(BinOp::MulInt, lit(-7), var(0)), lit(5)),
            bin(BinOp::SubInt, lit(1), bin(BinOp::MulInt, bin(BinOp::AddInt, var(0), lit(2)), lit(i64::MAX))),
            e(IrExprKind::UnOp { op: UnOp::NegInt, operand: Box::new(bin(BinOp::MulInt, var(0), lit(i64::MIN))) }),
        ];
        for body in &bodies {
            let (a, b, m) = affine_in(body, VarId(0)).expect("affine");
            assert!(m);
            for start in [0, -5, i64::MIN, i64::MAX - 40, 1 << 62] {
                let mut y = a.wrapping_mul(start).wrapping_add(b);
                for i in 0..40i64 {
                    assert_eq!(y, eval(body, start.wrapping_add(i)));
                    y = y.wrapping_add(a);
                }
            }
        }
    }

    #[test]
    fn another_variable_or_a_product_of_two_elements_is_not_affine() {
        assert!(affine_in(&bin(BinOp::MulInt, var(0), var(1)), VarId(0)).is_none());
        assert!(affine_in(&bin(BinOp::MulInt, var(0), var(0)), VarId(0)).is_none());
        assert!(affine_in(&bin(BinOp::ModInt, var(0), lit(3)), VarId(0)).is_none());
        let mut narrow = bin(BinOp::MulInt, var(0), lit(3));
        narrow.ty = Ty::Int32;
        assert!(affine_in(&narrow, VarId(0)).is_none());
    }
}
