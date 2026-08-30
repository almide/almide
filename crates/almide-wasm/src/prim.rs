//! The prim floor — the self-host stdlib's raw-memory/bit/float ops.
//! Every op is a DIRECT wasm mapping; addresses are absolute byte
//! addresses (the interp's heap-slice model: `prim.handle` returns the
//! block BASE, payload begins at base + PAYLOAD), so load/store use
//! align-hint 0 — the ops do arbitrary byte arithmetic by design.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

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
                    | SliceTy::Scalar(Scalar::Bytes)
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
            // RawPtr <-> Int identity casts (both are the i64 address).
            ("int_to_ptr" | "ptr_to_int", [x]) => {
                self.lower(x, Some(INT))?;
                Ok(Some(INT))
            }
            // Host entropy (C-112): n bytes written at address p via the
            // fs_call boundary (op 32) + host_read; returns 0.
            _ => self.lower_prim_call_b(func, args),
        }
    }

    /// The alloc / bitop / float-delegate half of the prim dispatch —
    /// split from `lower_prim_call` for the complexity budget.
    fn lower_prim_call_b(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("random_get", [p, n]) => {
                self.lower(p, Some(INT))?;
                let hp = self.hold_i64()?;
                self.f.instructions().local_set(hp);
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                self.note_host_op(crate::fs_meta::OP_RANDOM_GET);
                let mut i = self.f.instructions();
                i.i32_const(crate::fs_meta::OP_RANDOM_GET);
                i.i32_const(0).i32_const(0).i32_const(0);
                i.local_get(hn).i32_wrap_i64();
                i.call(F_FS_CALL).drop();
                i.local_get(hp).i32_wrap_i64().call(F_HOST_READ);
                i.i64_const(0);
                let _ = i;
                self.release_i64();
                self.release_i64();
                Ok(Some(INT))
            }
            ("alloc_bytes", [n]) => {
                self.lower(n, Some(INT))?;
                self.f.instructions().i32_wrap_i64().call(F_ALLOC);
                Ok(Some(SliceTy::Scalar(Scalar::Bytes)))
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
            ("alloc_list_f64", [n]) => {
                // List[Float]: the same 8-byte slots, Float-typed.
                self.lower(n, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_const(8).i32_mul().call(F_ALLOC);
                Ok(Some(SliceTy::List(self.types.intern(FLOAT))))
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
            ("f2f32" | "f32_2f" | "i2f32" | "f32bits" | "bits_to_f32", _) => {
                self.lower_prim_f32(func, args)
            }
            ("i2f" | "f2i" | "fbits" | "ffrombits" | "fadd" | "fsub" | "fmul"
            | "fdiv" | "fceil" | "ffloor" | "fneg" | "fabs" | "fsqrt" | "fcopysign"
            | "feq" | "fne" | "flt" | "fle" | "fgt" | "fge", _) => {
                self.lower_prim_float(func, args)
            }
            // die(msg_handle): the guarded-abort floor — surface the line
            // on stderr, then trap (abort parity is its own gate class).
            ("die", [msg]) => {
                // The die convention carries its own trailing "\n" in the
                // message block, and the host print appends one — print
                // ptr/len directly with the trailing newline stripped so
                // stderr is the interp's line VERBATIM, not doubled.
                self.lower(msg, Some(INT))?;
                let b = self.tmp_i32_local;
                let mut i = self.f.instructions();
                i.i32_wrap_i64().local_set(b);
                i.local_get(b).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(b).i32_load(len_memarg());
                // len -= (len > 0 && payload[len-1] == '\n')
                i.local_get(b).i32_load(len_memarg());
                i.if_(BlockType::Result(ValType::I32));
                i.local_get(b)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(b)
                    .i32_load(len_memarg())
                    .i32_add()
                    .i32_const(1)
                    .i32_sub()
                    .i32_load8_u(raw(()))
                    .i32_const(10)
                    .i32_eq();
                i.else_().i32_const(0).end();
                i.i32_sub();
                i.call(F_EPRINTLN_IMPORT).unreachable();
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

    /// The float half of the prim floor — split for the complexity budget.
    fn lower_prim_float(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
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
            _ => self.lower_prim_float_b(func, args),
        }
    }

    /// The f32 lane over the widened carrier — split from
    /// lower_prim_float for the complexity budget.
    fn lower_prim_f32(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("f2f32", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f32_demote_f64().f64_promote_f32();
                Ok(Some(FLOAT))
            }
            ("f32_2f", [a]) => {
                self.lower(a, Some(FLOAT))?;
                Ok(Some(FLOAT))
            }
            ("i2f32", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().f64_convert_i64_s().f32_demote_f64().f64_promote_f32();
                Ok(Some(FLOAT))
            }
            ("f32bits", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("bits_to_f32", [a]) => {
                self.lower(a, Some(INT))?;
                self.f.instructions().i32_wrap_i64().f32_reinterpret_i32().f64_promote_f32();
                Ok(Some(FLOAT))
            }
            _ => unsup(&format!("call:prim.{func}")),
        }
    }
}

impl Emitter<'_> {
    /// The unary-rounding / sign / compare half of the prim float ops —
    /// split from `lower_prim_float` for the complexity budget.
    fn lower_prim_float_b(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
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
            // f64.sqrt is IEEE-correctly-rounded on every target — the
            // one transcendental wasm itself guarantees bit-exact.
            ("fsqrt", [a]) => {
                self.lower(a, Some(FLOAT))?;
                self.f.instructions().f64_sqrt();
                Ok(Some(FLOAT))
            }
            ("fcopysign", [a, b]) => {
                self.lower(a, Some(FLOAT))?;
                self.lower(b, Some(FLOAT))?;
                self.f.instructions().f64_copysign();
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
            _ => unsup(&format!("call:prim.{func}")),
        }
    }
}
