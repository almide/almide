//! A remainder by a power of two compared against zero (#4610).
//!
//! `x % c == 0` for a literal `c = ±2^k` (k >= 1) needs no division and no
//! signed fix-up: it is `x & (2^k - 1) == 0`. And a literal factor on the
//! dividend divides out of the test: `(d * y) % 2^k == 0` is
//! `y & ((2^k - 1) >> tz(d)) == 0`, so an odd `d` leaves `y & (2^k - 1)`.
//!
//! WHY IT HOLDS under Almide's Int (i64) semantics:
//! - `%` is the truncating remainder (docs/specs/als/text-and-numbers.md,
//!   C-003): `r = x - c * trunc(x / c)` with `|r| < |c|`, so `r == 0` iff `c`
//!   divides `x` in Z, whatever the signs. For `c = ±2^k` that is `2^k | x`,
//!   and because `2^k | 2^64` it is a fact about `x mod 2^64`, i.e. the low
//!   `k` bits of the two's-complement register: `x & (2^k - 1) == 0`.
//!   `i64::MIN` included (`MIN % 2 == 0`, and `x % MIN == 0` iff
//!   `x & (2^63 - 1) == 0` iff x is 0 or MIN).
//! - `*` WRAPS (C-056): the product is `d * y mod 2^64`, and again `2^k | 2^64`,
//!   so `2^k` divides it iff `2^k | d * y` in Z. Write `d = 2^j * o` with `o`
//!   odd: `o` is a unit mod `2^k`, so that holds iff `2^(k-j) | y` when
//!   `j < k`, and always when `j >= k` (`d = 0` included) — the mask
//!   `(2^k - 1) >> j`, zero once `j >= k`.
//! - Nothing observable is dropped: `x % c` aborts only for `c` in {0, -1}
//!   (C-001/C-002), both excluded here, and a wrapping product never aborts.
//!   The non-literal operand is still evaluated, exactly once, in order.
//!
//! Only plain `Int` operands take it: a narrow width re-wraps and a UInt64
//! reads its register unsigned, and neither is needed by the rule.

use almide_ir::{BinOp, IrExpr, IrExprKind};
use almide_types::types::Ty;

use crate::emitter::Emitter;
use crate::*;

/// The literal value of a plain-`Int` literal.
fn int_lit(e: &IrExpr) -> Option<i64> {
    match (&e.kind, &e.ty) {
        (IrExprKind::LitInt { value }, Ty::Int) => Some(*value),
        _ => None,
    }
}

/// `x % c` with `c = ±2^k`, k >= 1, all plain Int: the operand whose low
/// bits decide `== 0`, and the mask of those bits — after dividing out any
/// literal factors of the operand.
pub(crate) fn rem_zero_mask(e: &IrExpr) -> Option<(&IrExpr, u64)> {
    let IrExprKind::BinOp { op: BinOp::ModInt, left, right } = &e.kind else { return None };
    if !matches!(e.ty, Ty::Int) || !matches!(left.ty, Ty::Int) {
        return None;
    }
    let c = int_lit(right)?;
    let k = c.unsigned_abs().trailing_zeros();
    if c == 0 || !c.unsigned_abs().is_power_of_two() || k == 0 {
        return None;
    }
    let mut mask = u64::MAX >> (64 - k);
    let mut x: &IrExpr = left;
    while let IrExprKind::BinOp { op: BinOp::MulInt, left: l, right: r } = &x.kind {
        if !matches!(x.ty, Ty::Int) {
            break;
        }
        let (d, y) = match (int_lit(l), int_lit(r)) {
            (Some(d), _) if matches!(r.ty, Ty::Int) => (d, &**r),
            (_, Some(d)) if matches!(l.ty, Ty::Int) => (d, &**l),
            _ => break,
        };
        mask = mask.checked_shr(d.trailing_zeros()).unwrap_or(0);
        x = y;
    }
    Some((x, mask))
}

impl Emitter<'_> {
    /// `e % 2^k == 0` / `!= 0` (either side the zero): the low-bits test.
    /// `Ok(None)` = not this shape; the general comparison runs.
    pub(crate) fn try_lower_rem_zero_test(
        &mut self,
        op: BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        if !matches!(op, BinOp::Eq | BinOp::Neq) {
            return Ok(None);
        }
        let rem = match (int_lit(left), int_lit(right)) {
            (_, Some(0)) => left,
            (Some(0), _) => right,
            _ => return Ok(None),
        };
        let Some((x, mask)) = rem_zero_mask(rem) else { return Ok(None) };
        self.lower(x, Some(INT))?;
        let mut i = self.f.instructions();
        i.i64_const(mask as i64).i64_and().i64_eqz();
        if matches!(op, BinOp::Neq) {
            i.i32_eqz();
        }
        Ok(Some(BOOL))
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
    fn var() -> IrExpr {
        e(IrExprKind::Var { id: almide_ir::VarId(0) })
    }
    fn bin(op: BinOp, l: IrExpr, r: IrExpr) -> IrExpr {
        e(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) })
    }
    fn mask_of(e: &IrExpr) -> Option<u64> {
        rem_zero_mask(e).map(|(_, m)| m)
    }

    /// The rule's one claim, checked by brute force: for every operand in a
    /// window around zero and both ends, `(d * y) % c == 0` (truncating
    /// remainder, wrapping product) agrees with `y & mask == 0`.
    #[test]
    fn the_mask_test_agrees_with_the_truncating_remainder() {
        let ys: Vec<i64> = (-300..300).chain([i64::MIN, i64::MIN + 1, i64::MAX, i64::MAX - 1, 1 << 62]).collect();
        for c in [2, 4, 8, 64, 1 << 62, i64::MIN, -2, -4, -1024] {
            for d in [1, 3, -3, 5, 2, 6, -6, 12, 0, i64::MIN, i64::MAX, 7 << 40] {
                let m = mask_of(&bin(BinOp::ModInt, bin(BinOp::MulInt, lit(d), var()), lit(c))).unwrap();
                let m2 = mask_of(&bin(BinOp::ModInt, bin(BinOp::MulInt, var(), lit(d)), lit(c))).unwrap();
                assert_eq!(m, m2);
                for &y in &ys {
                    let truth = d.wrapping_mul(y).wrapping_rem(c) == 0;
                    assert_eq!(truth, y as u64 & m == 0, "c={c} d={d} y={y}");
                }
            }
        }
    }

    #[test]
    fn a_divisor_that_is_not_a_power_of_two_or_can_abort_does_not_fire() {
        for c in [0, 1, -1, 3, 6, -6, 12, i64::MAX] {
            assert_eq!(mask_of(&bin(BinOp::ModInt, var(), lit(c))), None, "c={c}");
        }
        // A variable divisor is a runtime value: the guarded path keeps it.
        assert_eq!(mask_of(&bin(BinOp::ModInt, var(), var())), None);
    }

    #[test]
    fn a_narrow_operand_does_not_fire() {
        let mut x = var();
        x.ty = Ty::Int8;
        assert_eq!(mask_of(&bin(BinOp::ModInt, x, lit(2))), None);
    }
}
