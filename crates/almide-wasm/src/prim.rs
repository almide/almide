//! The prim floor — the self-host stdlib's raw-memory/bit/float ops.
//! Every op is a DIRECT wasm mapping; addresses are absolute byte
//! addresses (the interp's heap-slice model: `prim.handle` returns the
//! block BASE, payload begins at base + PAYLOAD), so load/store use
//! align-hint 0 — the ops do arbitrary byte arithmetic by design.

use almide_ir::IrExpr;
use wasm_encoder::MemArg;

use crate::emitter::Emitter;
use crate::*;

fn raw(align_unused: ()) -> MemArg {
    let () = align_unused;
    MemArg { offset: 0, align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    pub(crate) fn lower_prim_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            // handle: any BLOCK value's base address as Int.
            ("handle", [x]) => {
                match self.lower(x, None)? {
                    SliceTy::Scalar(Scalar::Str)
                    | SliceTy::List(_)
                    | SliceTy::Map(..)
                    | SliceTy::Set(_)
                    | SliceTy::Tuple(_)
                    | SliceTy::Named(_)
                    | SliceTy::Option(_)
                    | SliceTy::Result(..) => {}
                    other => return unsup(&format!("prim-handle-of:{other:?}")),
                }
                self.f.instructions().i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("load8", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_load8_u(raw(())).i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("load32", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_load(raw(())).i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("load64", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i64_load(raw(()));
                Ok(Some(INT))
            }
            ("store8", [a, v]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64();
                self.lower(v, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_store8(raw(()));
                Ok(None)
            }
            ("store32", [a, v]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64();
                self.lower(v, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_store(raw(()));
                Ok(None)
            }
            ("store64", [a, v]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64();
                self.lower(v, Some(INT))?;
                self.f.instructions().i64_store(raw(()));
                Ok(None)
            }
            ("alloc_str", [n]) => {
                self.lower(n, Some(INT))?;
                self.f.instructions().i32_wrap_i64().call(F_ALLOC);
                Ok(Some(STR))
            }
            ("alloc_list", [n]) => {
                // List[Int]: n slots of 8 bytes.
                self.lower(n, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_const(8).i32_mul().call(F_ALLOC);
                Ok(Some(SliceTy::List(self.types.intern(INT))))
            }
            ("band", [a, b]) | ("bor", [a, b]) | ("bxor", [a, b]) | ("bshl", [a, b])
            | ("bshr", [a, b]) | ("bshr_u", [a, b]) => {
                self.lower(a, Some(INT))?;
                self.lower(b, Some(INT))?;
                let mut i = self.f.instructions();
                match func {
                    "band" => i.i64_and(),
                    "bor" => i.i64_or(),
                    "bxor" => i.i64_xor(),
                    "bshl" => i.i64_shl(),
                    "bshr" => i.i64_shr_s(),
                    _ => i.i64_shr_u(),
                };
                Ok(Some(INT))
            }
            ("i2f", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().f64_convert_i64_s();
                Ok(Some(FLOAT))
            }
            // Rust `as i64` semantics = saturating truncation.
            ("f2i", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().i64_trunc_sat_f64_s();
                Ok(Some(INT))
            }
            ("fbits", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().i64_reinterpret_f64();
                Ok(Some(INT))
            }
            ("ffrombits", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().f64_reinterpret_i64();
                Ok(Some(FLOAT))
            }
            ("fadd", [a, b]) | ("fsub", [a, b]) | ("fmul", [a, b]) | ("fdiv", [a, b]) => {
                self.lower(a, Some(FLOAT))?;
                self.lower(b, Some(FLOAT))?;
                let mut i = self.f.instructions();
                match func {
                    "fadd" => i.f64_add(),
                    "fsub" => i.f64_sub(),
                    "fmul" => i.f64_mul(),
                    _ => i.f64_div(),
                };
                Ok(Some(FLOAT))
            }
            ("fceil", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f64_ceil();
                Ok(Some(FLOAT))
            }
            ("ffloor", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f64_floor();
                Ok(Some(FLOAT))
            }
            ("fneg", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f64_neg();
                Ok(Some(FLOAT))
            }
            ("fabs", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f64_abs();
                Ok(Some(FLOAT))
            }
            ("feq", [a, b]) | ("fne", [a, b]) | ("flt", [a, b]) | ("fle", [a, b])
            | ("fgt", [a, b]) | ("fge", [a, b]) => {
                self.lower(a, Some(FLOAT))?;
                self.lower(b, Some(FLOAT))?;
                let mut i = self.f.instructions();
                match func {
                    "feq" => i.f64_eq(),
                    "fne" => i.f64_ne(),
                    "flt" => i.f64_lt(),
                    "fle" => i.f64_le(),
                    "fgt" => i.f64_gt(),
                    _ => i.f64_ge(),
                };
                Ok(Some(BOOL))
            }
            // die(msg_handle): the guarded-abort floor — surface the line
            // on stderr, then trap (abort parity is its own gate class).
            ("die", [msg]) => {
                self.lower(msg, Some(INT))?;
                self.f.instructions().i32_wrap_i64().call(F_EPRINTLN_BLOCK).unreachable();
                Ok(None)
            }
            // Bump world: refcounts are inert — evaluate for effect order,
            // drop the value.
            ("rc_inc", [x]) | ("rc_dec", [x]) => {
                self.lower(x, None)?;
                self.f.instructions().drop();
                Ok(None)
            }
            _ => unsup(&format!("call:prim.{func}")),
        }
    }
}
