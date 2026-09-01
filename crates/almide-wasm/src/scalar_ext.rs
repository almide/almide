//! Scalar surface extensions (clamp family, float.sign, env.args) —
//! native intrinsic cells, semantics verbatim from runtime/rs.

use almide_ir::{CallTarget, IrExpr};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_scalar_ext(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let CallTarget::Module { module, func, .. } = target else {
            return Ok(None);
        };
        let out = match (module.as_str(), func.as_str(), args) {
            ("int" | "float", "clamp", [n, lo, hi]) => {
                Some(self.lower_scalar_clamp(module.as_str() == "int", n, lo, hi)?)
            }
            // f64::signum: ±1 by SIGN BIT (so sign(-0) = -1, sign(+0) = 1),
            // NaN stays NaN.
            ("float", "sign", [n]) => {
                self.lower(n, Some(FLOAT))?;
                let h = self.hold_f64()?;
                let mut i = self.f.instructions();
                i.local_set(h);
                i.local_get(h).local_get(h).f64_ne();
                i.if_(BlockType::Result(ValType::F64));
                i.local_get(h);
                i.else_();
                i.f64_const(1.0.into()).local_get(h).f64_copysign();
                i.end();
                let _ = i;
                self.release_f64();
                Some(FLOAT)
            }
            // C-210: NaN OBSERVATION IS CANONICAL — to_bits collapses every
            // NaN to 0x7FF8000000000000; non-NaN bits stay raw.
            ("float", "to_bits", [x]) => {
                self.lower(x, Some(FLOAT))?;
                let h = self.hold_f64()?;
                let mut i = self.f.instructions();
                i.local_set(h);
                i.local_get(h).local_get(h).f64_ne();
                i.if_(BlockType::Result(ValType::I64));
                i.i64_const(0x7FF8_0000_0000_0000_u64 as i64);
                i.else_();
                i.local_get(h).i64_reinterpret_f64();
                i.end();
                let _ = i;
                self.release_f64();
                Some(INT)
            }
            // The smuggling door C-210 tolerates: bits go in RAW (payload
            // NaNs live internally; only observation canonicalizes).
            ("int", "bits_to_float", [x]) => {
                self.lower(x, Some(INT))?;
                self.f.instructions().f64_reinterpret_i64();
                Some(FLOAT)
            }
            // IEEE-754 requires sqrt correctly rounded: wasm f64.sqrt and
            // Rust's `f64::sqrt` are the SAME function, bit for bit.
            ("math" | "float", "sqrt", [x]) => {
                self.lower(x, Some(FLOAT))?;
                self.f.instructions().f64_sqrt();
                Some(FLOAT)
            }
            ("float", "max" | "min", [a, b]) => {
                Some(self.lower_float_min_max(func.as_str() == "max", a, b)?)
            }
            // Same square-and-multiply (wrapping) + negative-exponent
            // abort as the `**` operator — one definition, two spellings.
            ("math", "pow", [b, e]) => Some(self.lower_pow_int(b, e)?),
            // `n as f64` IS f64.convert_i64_s (IEEE round-to-nearest-even)
            // — int.to_float with the module spelled the other way.
            ("float" | "float64", "from_int", [n]) | ("int", "to_float64", [n]) => {
                self.lower(n, Some(INT))?;
                self.f.instructions().f64_convert_i64_s();
                Some(FLOAT)
            }
            ("int", "band" | "bor" | "bxor" | "bshl" | "bshr" | "wrap_add" | "wrap_mul"
                | "bnot" | "to_u32" | "to_u8", _) => {
                return self.lower_int_bitops(func.as_str(), args).map(Some);
            }
            // f64.ceil is IEEE-exact on both targets.
            ("float", "ceil", [x]) => {
                self.lower(x, Some(FLOAT))?;
                self.f.instructions().f64_ceil();
                Some(FLOAT)
            }
            ("float", "is_infinite", [x]) => {
                self.lower(x, Some(FLOAT))?;
                let mut i = self.f.instructions();
                i.f64_abs().f64_const(f64::INFINITY.into()).f64_eq();
                let _ = i;
                Some(BOOL)
            }
            // Branchless (x ^ (x>>63)) - (x>>63): i64::MIN stays i64::MIN,
            // the release-build native wrap.
            ("int", "abs", [n]) => {
                self.lower(n, Some(INT))?;
                let h = self.hold_i64()?;
                let hm = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_tee(h).i64_const(63).i64_shr_s().local_set(hm);
                i.local_get(h).local_get(hm).i64_xor().local_get(hm).i64_sub();
                let _ = i;
                self.release_i64();
                self.release_i64();
                Some(INT)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// T6: an inverted range (float: NaN bounds too, via !(lo<=hi))
    /// dies with the one-line clamp message; the clamp itself is
    /// Rust's COMPARISON chain, not wasm min/max — f64::clamp
    /// keeps -0.0 when lo is +0.0 (compares, never sign-joins).
    fn lower_scalar_clamp(
        &mut self,
        is_int: bool,
        n: &IrExpr,
        lo: &IrExpr,
        hi: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let want = if is_int { INT } else { FLOAT };
        self.lower(n, Some(want))?;
        let hn = if is_int { self.hold_i64()? } else { self.hold_f64()? };
        self.f.instructions().local_set(hn);
        self.lower(lo, Some(want))?;
        let hlo = if is_int { self.hold_i64()? } else { self.hold_f64()? };
        self.f.instructions().local_set(hlo);
        self.lower(hi, Some(want))?;
        let hhi = if is_int { self.hold_i64()? } else { self.hold_f64()? };
        let msg = self.pool.intern("Error: clamp requires min <= max");
        {
            let mut i = self.f.instructions();
            i.local_set(hhi);
            i.local_get(hlo).local_get(hhi);
            if is_int {
                i.i64_le_s();
            } else {
                i.f64_le();
            }
            i.i32_eqz().if_(BlockType::Empty);
            i.i32_const(msg as i32).call(F_EPRINTLN_BLOCK).unreachable();
            i.end();
            // if n < lo { lo } else if n > hi { hi } else { n }
            let vt = if is_int { ValType::I64 } else { ValType::F64 };
            i.local_get(hn).local_get(hlo);
            if is_int {
                i.i64_lt_s();
            } else {
                i.f64_lt();
            }
            i.if_(BlockType::Result(vt));
            i.local_get(hlo);
            i.else_();
            i.local_get(hn).local_get(hhi);
            if is_int {
                i.i64_gt_s();
            } else {
                i.f64_gt();
            }
            i.if_(BlockType::Result(vt));
            i.local_get(hhi);
            i.else_();
            i.local_get(hn);
            i.end();
            i.end();
        }
        for _ in 0..3 {
            if is_int {
                self.release_i64();
            } else {
                self.release_f64();
            }
        }
        Ok(want)
    }

    /// NaN-IGNORING min/max (native chain verbatim; C-306 side): one NaN
    /// yields the OTHER operand, equal operands yield `a` — so
    /// f64.min/max (NaN-propagating, -0-sign-joining) is wrong on both
    /// counts and a comparison+select is used instead.
    fn lower_float_min_max(
        &mut self,
        is_max: bool,
        a: &IrExpr,
        b: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        self.lower(a, Some(FLOAT))?;
        let ha = self.hold_f64()?;
        self.f.instructions().local_set(ha);
        self.lower(b, Some(FLOAT))?;
        let hb = self.hold_f64()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(ha).local_get(ha).f64_ne();
        i.if_(BlockType::Result(ValType::F64));
        i.local_get(hb);
        i.else_();
        i.local_get(hb).local_get(hb).f64_ne();
        i.if_(BlockType::Result(ValType::F64));
        i.local_get(ha);
        i.else_();
        // Strict winner, else the TIE branch. IEEE-754-2019 zero ordering
        // (C-049, ALS-T23): a ±0 tie resolves by SIGN — min = -0.0, max =
        // +0.0, commutative — computed as the bitwise OR (min: either
        // sign bit wins) / AND (max: both must be negative) of the two
        // payloads, which is the identity on non-zero equal ties.
        i.local_get(ha).local_get(hb);
        if is_max {
            i.f64_gt();
        } else {
            i.f64_lt();
        }
        i.if_(BlockType::Result(ValType::F64));
        i.local_get(ha);
        i.else_();
        i.local_get(hb).local_get(ha);
        if is_max {
            i.f64_gt();
        } else {
            i.f64_lt();
        }
        i.if_(BlockType::Result(ValType::F64));
        i.local_get(hb);
        i.else_();
        i.local_get(ha).i64_reinterpret_f64();
        i.local_get(hb).i64_reinterpret_f64();
        if is_max {
            i.i64_and();
        } else {
            i.i64_or();
        }
        i.f64_reinterpret_i64();
        i.end();
        i.end();
        i.end();
        i.end();
        let _ = i;
        self.release_f64();
        self.release_f64();
        Ok(FLOAT)
    }

    /// The int bit family + the width-wrap pair — split from
    /// lower_scalar_ext for the complexity budget.
    fn lower_int_bitops(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("band" | "bor" | "bxor" | "bshl" | "bshr", [a, b]) => {
                self.lower(a, Some(INT))?;
                self.lower(b, Some(INT))?;
                let mut i = self.f.instructions();
                match func {
                    "band" => i.i64_and(),
                    "bor" => i.i64_or(),
                    "bxor" => i.i64_xor(),
                    "bshl" => i.i64_shl(),
                    _ => i.i64_shr_s(),
                };
                Ok(Some(INT))
            }
            // #1423 stage 4: the pure bit family's unary/masking trio —
            // semantics verbatim from stdlib/int_wrap.almd (`bnot(n) =
            // bxor(n, -1)`, `to_u32(n) = n & 0xFFFFFFFF`, `to_u8(n) =
            // n & 0xFF`, low bits zero-extended).
            ("bnot", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i64_const(-1).i64_xor();
                Ok(Some(INT))
            }
            ("to_u32", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i64_const(0xFFFF_FFFF).i64_and();
                Ok(Some(INT))
            }
            ("to_u8", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i64_const(0xFF).i64_and();
                Ok(Some(INT))
            }
            ("wrap_add" | "wrap_mul", [a, b, bits]) => {
                let mul = func == "wrap_mul";
                self.lower(a, Some(INT))?;
                self.lower(b, Some(INT))?;
                if mul {
                    self.f.instructions().i64_mul();
                } else {
                    self.f.instructions().i64_add();
                }
                self.lower(bits, Some(INT))?;
                let hb = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hb);
                // mask = bits >= 64 ? -1 : (1<<bits)-1  (select: v1 first)
                i.i64_const(-1);
                i.i64_const(1).local_get(hb).i64_shl().i64_const(1).i64_sub();
                i.local_get(hb).i64_const(64).i64_ge_s();
                i.select().i64_and();
                let _ = i;
                self.release_i64();
                Ok(Some(INT))
            }
            _ => unsup(&format!("call:int.{func}")),
        }
    }
}
