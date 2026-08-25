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
            // This host passes NO program arguments (the harness defines
            // the boundary): the empty List[String].
            ("env", "args", []) => {
                self.f.instructions().i32_const(0).call(F_ALLOC);
                Some(SliceTy::List(self.types.intern(STR)))
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
            ("math", "sqrt", [x]) => {
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
        // select(v1, v2, cond) = cond ? v1 : v2 — push v1 first
        i.local_get(hb).local_get(ha);
        i.local_get(ha).local_get(hb);
        if is_max {
            i.f64_lt();
        } else {
            i.f64_gt();
        }
        i.select();
        i.end();
        i.end();
        let _ = i;
        self.release_f64();
        self.release_f64();
        Ok(FLOAT)
    }
}
