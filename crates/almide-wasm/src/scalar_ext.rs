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
            // T6: an inverted range (float: NaN bounds too, via !(lo<=hi))
            // dies with the one-line clamp message; the clamp itself is
            // Rust's COMPARISON chain, not wasm min/max — f64::clamp
            // keeps -0.0 when lo is +0.0 (compares, never sign-joins).
            ("int" | "float", "clamp", [n, lo, hi]) => {
                let is_int = module.as_str() == "int";
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
                if is_int {
                    self.release_i64();
                    self.release_i64();
                    self.release_i64();
                } else {
                    self.release_f64();
                    self.release_f64();
                    self.release_f64();
                }
                Some(want)
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
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}
