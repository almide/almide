//! The numeric instructions, as data. Every row is one opcode of the closed
//! set (REQ-VM-2): its byte, its text name, its signature, and its semantics
//! over raw 64-bit cells (an i32 or f32 lives zero-extended in the low half).
//! The validator reads the signature, the interpreter calls the function, and
//! nothing else names a numeric opcode — so this table IS the numeric half of
//! the allowlist, and `every_row_has_a_distinct_opcode` keeps it one.

use crate::error::Trap;
use crate::types::ValType::{self, F32, F64, I32, I64};

pub type Unary = fn(u64) -> u64;
pub type Binary = fn(u64, u64) -> Result<u64, Trap>;

#[derive(Clone, Copy)]
pub enum Semantics {
    Unary(Unary),
    Binary(Binary),
}

#[derive(Clone, Copy)]
pub struct NumericOp {
    /// The opcode byte, or `0xFC00 | sub` for a 0xFC-prefixed instruction.
    pub code: u16,
    pub name: &'static str,
    pub operand: ValType,
    pub result: ValType,
    pub semantics: Semantics,
}

const fn un(code: u16, name: &'static str, operand: ValType, result: ValType, f: Unary) -> NumericOp {
    NumericOp { code, name, operand, result, semantics: Semantics::Unary(f) }
}

const fn bin(code: u16, name: &'static str, operand: ValType, result: ValType, f: Binary) -> NumericOp {
    NumericOp { code, name, operand, result, semantics: Semantics::Binary(f) }
}

fn w32(x: u64) -> u32 {
    x as u32
}
fn c32(x: u32) -> u64 {
    u64::from(x)
}
fn f64v(x: u64) -> f64 {
    f64::from_bits(x)
}
fn cf64(x: f64) -> u64 {
    x.to_bits()
}
fn f32v(x: u64) -> f32 {
    f32::from_bits(x as u32)
}
fn cf32(x: f32) -> u64 {
    u64::from(x.to_bits())
}
fn bool_cell(b: bool) -> u64 {
    u64::from(b)
}

// ── i32 ───────────────────────────────────────────────────────────────────
fn i32_eqz(a: u64) -> u64 {
    bool_cell(w32(a) == 0)
}
fn i32_clz(a: u64) -> u64 {
    c32(w32(a).leading_zeros())
}
fn i32_add(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a).wrapping_add(w32(b))))
}
fn i32_sub(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a).wrapping_sub(w32(b))))
}
fn i32_mul(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a).wrapping_mul(w32(b))))
}
fn i32_div_u(a: u64, b: u64) -> Result<u64, Trap> {
    w32(a).checked_div(w32(b)).map(c32).ok_or(Trap::IntegerDivideByZero)
}
fn i32_and(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a) & w32(b)))
}
fn i32_or(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a) | w32(b)))
}
fn i32_xor(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a) ^ w32(b)))
}
fn i32_shl(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a).wrapping_shl(w32(b))))
}
fn i32_shr_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(c32(w32(a).wrapping_shr(w32(b))))
}
fn i32_eq(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) == w32(b)))
}
fn i32_ne(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) != w32(b)))
}
fn i32_lt_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell((w32(a) as i32) < w32(b) as i32))
}
fn i32_lt_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) < w32(b)))
}
fn i32_gt_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) as i32 > w32(b) as i32))
}
fn i32_gt_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) > w32(b)))
}
fn i32_le_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) as i32 <= w32(b) as i32))
}
fn i32_le_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) <= w32(b)))
}
fn i32_ge_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) as i32 >= w32(b) as i32))
}
fn i32_ge_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(w32(a) >= w32(b)))
}

// ── i64 ───────────────────────────────────────────────────────────────────
fn i64_eqz(a: u64) -> u64 {
    bool_cell(a == 0)
}
fn i64_add(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a.wrapping_add(b))
}
fn i64_sub(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a.wrapping_sub(b))
}
fn i64_mul(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a.wrapping_mul(b))
}
fn i64_div_s(a: u64, b: u64) -> Result<u64, Trap> {
    let (a, b) = (a as i64, b as i64);
    if b == 0 {
        return Err(Trap::IntegerDivideByZero);
    }
    a.checked_div(b).map(|q| q as u64).ok_or(Trap::IntegerOverflow)
}
fn i64_div_u(a: u64, b: u64) -> Result<u64, Trap> {
    a.checked_div(b).ok_or(Trap::IntegerDivideByZero)
}
fn i64_rem_s(a: u64, b: u64) -> Result<u64, Trap> {
    let (a, b) = (a as i64, b as i64);
    if b == 0 {
        return Err(Trap::IntegerDivideByZero);
    }
    // MIN % -1 is 0 in wasm (it does not trap, unlike the division)
    Ok(a.wrapping_rem(b) as u64)
}
fn i64_rem_u(a: u64, b: u64) -> Result<u64, Trap> {
    a.checked_rem(b).ok_or(Trap::IntegerDivideByZero)
}
fn i64_and(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a & b)
}
fn i64_or(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a | b)
}
fn i64_xor(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a ^ b)
}
fn i64_shl(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a.wrapping_shl(b as u32))
}
fn i64_shr_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok((a as i64).wrapping_shr(b as u32) as u64)
}
fn i64_shr_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(a.wrapping_shr(b as u32))
}
fn i64_eq(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a == b))
}
fn i64_ne(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a != b))
}
fn i64_lt_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell((a as i64) < b as i64))
}
fn i64_lt_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a < b))
}
fn i64_gt_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a as i64 > b as i64))
}
fn i64_gt_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a > b))
}
fn i64_le_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a as i64 <= b as i64))
}
fn i64_le_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a <= b))
}
fn i64_ge_s(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a as i64 >= b as i64))
}
fn i64_ge_u(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(a >= b))
}

// ── f64 ───────────────────────────────────────────────────────────────────
fn f64_eq(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) == f64v(b)))
}
fn f64_ne(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) != f64v(b)))
}
fn f64_lt(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) < f64v(b)))
}
fn f64_gt(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) > f64v(b)))
}
fn f64_le(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) <= f64v(b)))
}
fn f64_ge(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(bool_cell(f64v(a) >= f64v(b)))
}
fn f64_abs(a: u64) -> u64 {
    a & !(1u64 << 63)
}
fn f64_neg(a: u64) -> u64 {
    a ^ (1u64 << 63)
}
fn f64_ceil(a: u64) -> u64 {
    cf64(f64v(a).ceil())
}
fn f64_floor(a: u64) -> u64 {
    cf64(f64v(a).floor())
}
fn f64_nearest(a: u64) -> u64 {
    cf64(f64v(a).round_ties_even())
}
fn f64_sqrt(a: u64) -> u64 {
    cf64(f64v(a).sqrt())
}
fn f64_add(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(cf64(f64v(a) + f64v(b)))
}
fn f64_sub(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(cf64(f64v(a) - f64v(b)))
}
fn f64_mul(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(cf64(f64v(a) * f64v(b)))
}
fn f64_div(a: u64, b: u64) -> Result<u64, Trap> {
    Ok(cf64(f64v(a) / f64v(b)))
}
/// wasm `f64.min`: a NaN operand gives NaN, and -0 is below +0 (Rust's
/// `f64::min` returns the other operand for a NaN and treats the zeros as
/// equal, so it is not used).
fn f64_min(a: u64, b: u64) -> Result<u64, Trap> {
    let (x, y) = (f64v(a), f64v(b));
    Ok(if x.is_nan() || y.is_nan() {
        cf64(f64::NAN)
    } else if x == y {
        a | b // equal values: the zeros differ only in sign, and -0 wins
    } else {
        cf64(if x < y { x } else { y })
    })
}
fn f64_max(a: u64, b: u64) -> Result<u64, Trap> {
    let (x, y) = (f64v(a), f64v(b));
    Ok(if x.is_nan() || y.is_nan() {
        cf64(f64::NAN)
    } else if x == y {
        a & b // equal values: +0 wins
    } else {
        cf64(if x > y { x } else { y })
    })
}
fn f64_copysign(a: u64, b: u64) -> Result<u64, Trap> {
    Ok((a & !(1u64 << 63)) | (b & (1u64 << 63)))
}

// ── conversions ───────────────────────────────────────────────────────────
fn i32_wrap_i64(a: u64) -> u64 {
    c32(a as u32)
}
fn i64_extend_i32_s(a: u64) -> u64 {
    i64::from(w32(a) as i32) as u64
}
fn i64_extend_i32_u(a: u64) -> u64 {
    c32(w32(a))
}
fn f32_convert_i64_s(a: u64) -> u64 {
    cf32(a as i64 as f32)
}
fn f32_demote_f64(a: u64) -> u64 {
    cf32(f64v(a) as f32)
}
fn f64_convert_i32_s(a: u64) -> u64 {
    cf64(f64::from(w32(a) as i32))
}
fn f64_convert_i64_s(a: u64) -> u64 {
    cf64(a as i64 as f64)
}
fn f64_convert_i64_u(a: u64) -> u64 {
    cf64(a as f64)
}
fn f64_promote_f32(a: u64) -> u64 {
    cf64(f64::from(f32v(a)))
}
fn reinterpret(a: u64) -> u64 {
    a // the cell already holds the bits; only the static type changes
}
fn i64_extend8_s(a: u64) -> u64 {
    i64::from(a as u8 as i8) as u64
}
fn i64_extend16_s(a: u64) -> u64 {
    i64::from(a as u16 as i16) as u64
}
fn i64_extend32_s(a: u64) -> u64 {
    i64::from(a as u32 as i32) as u64
}
/// Saturating: NaN is 0, and out-of-range values clamp — exactly Rust's `as`.
fn i64_trunc_sat_f64_s(a: u64) -> u64 {
    f64v(a) as i64 as u64
}

pub static NUMERIC: &[NumericOp] = &[
    un(0x45, "i32.eqz", I32, I32, i32_eqz),
    bin(0x46, "i32.eq", I32, I32, i32_eq),
    bin(0x47, "i32.ne", I32, I32, i32_ne),
    bin(0x48, "i32.lt_s", I32, I32, i32_lt_s),
    bin(0x49, "i32.lt_u", I32, I32, i32_lt_u),
    bin(0x4A, "i32.gt_s", I32, I32, i32_gt_s),
    bin(0x4B, "i32.gt_u", I32, I32, i32_gt_u),
    bin(0x4C, "i32.le_s", I32, I32, i32_le_s),
    bin(0x4D, "i32.le_u", I32, I32, i32_le_u),
    bin(0x4E, "i32.ge_s", I32, I32, i32_ge_s),
    bin(0x4F, "i32.ge_u", I32, I32, i32_ge_u),
    un(0x50, "i64.eqz", I64, I32, i64_eqz),
    bin(0x51, "i64.eq", I64, I32, i64_eq),
    bin(0x52, "i64.ne", I64, I32, i64_ne),
    bin(0x53, "i64.lt_s", I64, I32, i64_lt_s),
    bin(0x54, "i64.lt_u", I64, I32, i64_lt_u),
    bin(0x55, "i64.gt_s", I64, I32, i64_gt_s),
    bin(0x56, "i64.gt_u", I64, I32, i64_gt_u),
    bin(0x57, "i64.le_s", I64, I32, i64_le_s),
    bin(0x58, "i64.le_u", I64, I32, i64_le_u),
    bin(0x59, "i64.ge_s", I64, I32, i64_ge_s),
    bin(0x5A, "i64.ge_u", I64, I32, i64_ge_u),
    bin(0x61, "f64.eq", F64, I32, f64_eq),
    bin(0x62, "f64.ne", F64, I32, f64_ne),
    bin(0x63, "f64.lt", F64, I32, f64_lt),
    bin(0x64, "f64.gt", F64, I32, f64_gt),
    bin(0x65, "f64.le", F64, I32, f64_le),
    bin(0x66, "f64.ge", F64, I32, f64_ge),
    un(0x67, "i32.clz", I32, I32, i32_clz),
    bin(0x6A, "i32.add", I32, I32, i32_add),
    bin(0x6B, "i32.sub", I32, I32, i32_sub),
    bin(0x6C, "i32.mul", I32, I32, i32_mul),
    bin(0x6E, "i32.div_u", I32, I32, i32_div_u),
    bin(0x71, "i32.and", I32, I32, i32_and),
    bin(0x72, "i32.or", I32, I32, i32_or),
    bin(0x73, "i32.xor", I32, I32, i32_xor),
    bin(0x74, "i32.shl", I32, I32, i32_shl),
    bin(0x76, "i32.shr_u", I32, I32, i32_shr_u),
    bin(0x7C, "i64.add", I64, I64, i64_add),
    bin(0x7D, "i64.sub", I64, I64, i64_sub),
    bin(0x7E, "i64.mul", I64, I64, i64_mul),
    bin(0x7F, "i64.div_s", I64, I64, i64_div_s),
    bin(0x80, "i64.div_u", I64, I64, i64_div_u),
    bin(0x81, "i64.rem_s", I64, I64, i64_rem_s),
    bin(0x82, "i64.rem_u", I64, I64, i64_rem_u),
    bin(0x83, "i64.and", I64, I64, i64_and),
    bin(0x84, "i64.or", I64, I64, i64_or),
    bin(0x85, "i64.xor", I64, I64, i64_xor),
    bin(0x86, "i64.shl", I64, I64, i64_shl),
    bin(0x87, "i64.shr_s", I64, I64, i64_shr_s),
    bin(0x88, "i64.shr_u", I64, I64, i64_shr_u),
    un(0x99, "f64.abs", F64, F64, f64_abs),
    un(0x9A, "f64.neg", F64, F64, f64_neg),
    un(0x9B, "f64.ceil", F64, F64, f64_ceil),
    un(0x9C, "f64.floor", F64, F64, f64_floor),
    un(0x9E, "f64.nearest", F64, F64, f64_nearest),
    un(0x9F, "f64.sqrt", F64, F64, f64_sqrt),
    bin(0xA0, "f64.add", F64, F64, f64_add),
    bin(0xA1, "f64.sub", F64, F64, f64_sub),
    bin(0xA2, "f64.mul", F64, F64, f64_mul),
    bin(0xA3, "f64.div", F64, F64, f64_div),
    bin(0xA4, "f64.min", F64, F64, f64_min),
    bin(0xA5, "f64.max", F64, F64, f64_max),
    bin(0xA6, "f64.copysign", F64, F64, f64_copysign),
    un(0xA7, "i32.wrap_i64", I64, I32, i32_wrap_i64),
    un(0xAC, "i64.extend_i32_s", I32, I64, i64_extend_i32_s),
    un(0xAD, "i64.extend_i32_u", I32, I64, i64_extend_i32_u),
    un(0xB4, "f32.convert_i64_s", I64, F32, f32_convert_i64_s),
    un(0xB6, "f32.demote_f64", F64, F32, f32_demote_f64),
    un(0xB7, "f64.convert_i32_s", I32, F64, f64_convert_i32_s),
    un(0xB9, "f64.convert_i64_s", I64, F64, f64_convert_i64_s),
    un(0xBA, "f64.convert_i64_u", I64, F64, f64_convert_i64_u),
    un(0xBB, "f64.promote_f32", F32, F64, f64_promote_f32),
    un(0xBC, "i32.reinterpret_f32", F32, I32, reinterpret),
    un(0xBD, "i64.reinterpret_f64", F64, I64, reinterpret),
    un(0xBE, "f32.reinterpret_i32", I32, F32, reinterpret),
    un(0xBF, "f64.reinterpret_i64", I64, F64, reinterpret),
    un(0xC2, "i64.extend8_s", I64, I64, i64_extend8_s),
    un(0xC3, "i64.extend16_s", I64, I64, i64_extend16_s),
    un(0xC4, "i64.extend32_s", I64, I64, i64_extend32_s),
    un(0xFC06, "i64.trunc_sat_f64_s", F64, I64, i64_trunc_sat_f64_s),
];

/// The row for an opcode, if the closed set has one.
pub fn lookup(code: u16) -> Option<&'static NumericOp> {
    NUMERIC.iter().find(|op| op.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_has_a_distinct_opcode_and_name() {
        let mut codes: Vec<u16> = NUMERIC.iter().map(|o| o.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), NUMERIC.len(), "an opcode appears twice");
        let mut names: Vec<&str> = NUMERIC.iter().map(|o| o.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), NUMERIC.len(), "a name appears twice");
    }

    fn b(code: u16, a: u64, b_: u64) -> Result<u64, Trap> {
        match lookup(code).unwrap().semantics {
            Semantics::Binary(f) => f(a, b_),
            Semantics::Unary(_) => panic!("unary"),
        }
    }
    fn u(code: u16, a: u64) -> u64 {
        match lookup(code).unwrap().semantics {
            Semantics::Unary(f) => f(a),
            Semantics::Binary(_) => panic!("binary"),
        }
    }

    #[test]
    fn integer_edges_follow_the_spec() {
        assert_eq!(b(0x7F, i64::MIN as u64, (-1i64) as u64), Err(Trap::IntegerOverflow));
        assert_eq!(b(0x81, i64::MIN as u64, (-1i64) as u64), Ok(0), "rem_s MIN -1 is 0");
        assert_eq!(b(0x7F, 7, 0), Err(Trap::IntegerDivideByZero));
        assert_eq!(b(0x6E, 7, 0), Err(Trap::IntegerDivideByZero));
        assert_eq!(b(0x74, 1, 33), Ok(2), "i32 shift count is taken mod 32");
        assert_eq!(b(0x87, (-8i64) as u64, 65), Ok((-4i64) as u64), "i64 shift count mod 64");
        assert_eq!(u(0x67, 0), 32);
        assert_eq!(b(0x6A, 0xffff_ffff, 1), Ok(0), "i32 add wraps within 32 bits");
        assert_eq!(u(0xAC, 0x8000_0000), 0xffff_ffff_8000_0000);
        assert_eq!(u(0xC2, 0x80), (-128i64) as u64);
    }

    #[test]
    fn float_edges_follow_the_spec() {
        let nz = (-0.0f64).to_bits();
        let pz = 0.0f64.to_bits();
        assert_eq!(b(0xA4, pz, nz), Ok(nz), "min(+0, -0) = -0");
        assert_eq!(b(0xA5, nz, pz), Ok(pz), "max(-0, +0) = +0");
        assert!(f64::from_bits(b(0xA4, 1.0f64.to_bits(), f64::NAN.to_bits()).unwrap()).is_nan());
        assert_eq!(u(0x9E, 2.5f64.to_bits()), 2.0f64.to_bits(), "nearest ties to even");
        assert_eq!(u(0x9E, (-0.5f64).to_bits()), (-0.0f64).to_bits());
        assert_eq!(u(0xFC06, f64::NAN.to_bits()), 0, "trunc_sat NaN is 0");
        assert_eq!(u(0xFC06, 1e300f64.to_bits()), i64::MAX as u64);
        assert_eq!(u(0xFC06, (-1e300f64).to_bits()), i64::MIN as u64);
        assert_eq!(f64::from_bits(u(0xBA, u64::MAX)), 18446744073709551616.0);
    }
}
