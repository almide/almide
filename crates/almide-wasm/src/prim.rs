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
    ) -> ArmResult {
        match (func, args) {
            // handle: any BLOCK value's base address as Int.
            ("handle", [x]) => {
                match self.lower_arg(x, None, ArgMode::Raw)? {
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
                Ok(Some(Lowered::scalar(INT)))
            }
            ("load8", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().i32_load8_u(raw(())).i64_extend_i32_u();
                Ok(Some(Lowered::scalar(INT)))
            }
            ("load32", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().i32_load(raw(())).i64_extend_i32_u();
                Ok(Some(Lowered::scalar(INT)))
            }
            ("load64", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().i64_load(raw(()));
                Ok(Some(Lowered::scalar(INT)))
            }
            ("store8", [a, v]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64();
                self.lower_arg(v, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().i32_store8(raw(()));
                Ok(None)
            }
            ("store32", [a, v]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64();
                self.lower_arg(v, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().i32_store(raw(()));
                Ok(None)
            }
            ("store64", [a, v]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64();
                self.lower_arg(v, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i64_store(raw(()));
                Ok(None)
            }
            // RawPtr <-> Int identity casts (both are the i64 address).
            ("int_to_ptr" | "ptr_to_int", [x]) => {
                self.lower_arg(x, Some(INT), ArgMode::Raw)?;
                Ok(Some(Lowered::scalar(INT)))
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
    ) -> ArmResult {
        match (func, args) {
            ("random_get", [p, n]) => {
                self.lower_arg(p, Some(INT), ArgMode::Raw)?;
                let hp = self.hold_i64()?;
                self.f.instructions().local_set(hp);
                self.lower_arg(n, Some(INT), ArgMode::Raw)?;
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
                Ok(Some(Lowered::scalar(INT)))
            }
            ("alloc_bytes", [n]) => {
                self.lower_arg(n, Some(INT), ArgMode::Raw)?;
                self.emit_alloc_size_guard(1)?;
                self.f.instructions().i32_wrap_i64().call(F_ALLOC);
                Ok(Some(Lowered::owned(SliceTy::Scalar(Scalar::Bytes))))
            }
            ("alloc_str", [n]) => {
                self.lower_arg(n, Some(INT), ArgMode::Raw)?;
                self.emit_alloc_size_guard(1)?;
                self.f.instructions().i32_wrap_i64().call(F_ALLOC);
                Ok(Some(Lowered::owned(STR)))
            }
            ("alloc_list", [n]) => {
                // List[Int]: n slots of 8 bytes.
                self.lower_arg(n, Some(INT), ArgMode::Raw)?;
                self.emit_alloc_size_guard(8)?;
                self.f.instructions().i32_wrap_i64().i32_const(8).i32_mul().call(F_ALLOC);
                Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(INT)))))
            }
            ("alloc_list_f64", [n]) => {
                // List[Float]: the same 8-byte slots, Float-typed.
                self.lower_arg(n, Some(INT), ArgMode::Raw)?;
                self.emit_alloc_size_guard(8)?;
                self.f.instructions().i32_wrap_i64().i32_const(8).i32_mul().call(F_ALLOC);
                Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(FLOAT)))))
            }
            ("band", [a, b]) | ("bor", [a, b]) | ("bxor", [a, b]) | ("bshl", [a, b])
            | ("bshr", [a, b]) | ("bshr_u", [a, b]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.lower_arg(b, Some(INT), ArgMode::Raw)?;
                let mut i = self.f.instructions();
                match func {
                    "band" => i.i64_and(),
                    "bor" => i.i64_or(),
                    "bxor" => i.i64_xor(),
                    "bshl" => i.i64_shl(),
                    "bshr" => i.i64_shr_s(),
                    _ => i.i64_shr_u(),
                };
                Ok(Some(Lowered::scalar(INT)))
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
                self.lower_arg(msg, Some(INT), ArgMode::Raw)?;
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
                self.lower_arg(x, None, ArgMode::Raw)?;
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
    ) -> ArmResult {
        match (func, args) {
            ("i2f", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().f64_convert_i64_s();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            // Rust `as i64` semantics = saturating truncation.
            ("f2i", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().i64_trunc_sat_f64_s();
                Ok(Some(Lowered::scalar(INT)))
            }
            ("fbits", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().i64_reinterpret_f64();
                Ok(Some(Lowered::scalar(INT)))
            }
            ("ffrombits", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().f64_reinterpret_i64();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("fadd", [a, b]) | ("fsub", [a, b]) | ("fmul", [a, b]) | ("fdiv", [a, b]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.lower_arg(b, Some(FLOAT), ArgMode::Raw)?;
                let mut i = self.f.instructions();
                match func {
                    "fadd" => i.f64_add(),
                    "fsub" => i.f64_sub(),
                    "fmul" => i.f64_mul(),
                    _ => i.f64_div(),
                };
                Ok(Some(Lowered::scalar(FLOAT)))
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
    ) -> ArmResult {
        match (func, args) {
            ("f2f32", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f32_demote_f64().f64_promote_f32();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("f32_2f", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            // i64 → f32 DIRECTLY (single rounding — native's `n as f32`
            // and the incumbent's f32.convert_i64_s), then widened onto
            // the f64 carrier. The i2f-then-demote spelling double-rounds:
            // 2^60 + 2^36 + 1 loses its +1 to the f64 step and then sits
            // exactly on the f32 tie, rounding to even (2^60) where the
            // single rounding reads the true value above the tie (2^60 +
            // 2^37).
            ("i2f32", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().f32_convert_i64_s().f64_promote_f32();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("f32bits", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
                Ok(Some(Lowered::scalar(INT)))
            }
            ("bits_to_f32", [a]) => {
                self.lower_arg(a, Some(INT), ArgMode::Raw)?;
                self.f.instructions().i32_wrap_i64().f32_reinterpret_i32().f64_promote_f32();
                Ok(Some(Lowered::scalar(FLOAT)))
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
    ) -> ArmResult {
        match (func, args) {
            ("fceil", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_ceil();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("ffloor", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_floor();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("fneg", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_neg();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("fabs", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_abs();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            // f64.sqrt is IEEE-correctly-rounded on every target — the
            // one transcendental wasm itself guarantees bit-exact.
            ("fsqrt", [a]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_sqrt();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("fcopysign", [a, b]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.lower_arg(b, Some(FLOAT), ArgMode::Raw)?;
                self.f.instructions().f64_copysign();
                Ok(Some(Lowered::scalar(FLOAT)))
            }
            ("feq", [a, b]) | ("fne", [a, b]) | ("flt", [a, b]) | ("fle", [a, b])
            | ("fgt", [a, b]) | ("fge", [a, b]) => {
                self.lower_arg(a, Some(FLOAT), ArgMode::Raw)?;
                self.lower_arg(b, Some(FLOAT), ArgMode::Raw)?;
                let mut i = self.f.instructions();
                match func {
                    "feq" => i.f64_eq(),
                    "fne" => i.f64_ne(),
                    "flt" => i.f64_lt(),
                    "fle" => i.f64_le(),
                    "fgt" => i.f64_gt(),
                    _ => i.f64_ge(),
                };
                Ok(Some(Lowered::scalar(BOOL)))
            }
            _ => unsup(&format!("call:prim.{func}")),
        }
    }
}

impl Emitter<'_> {
    /// The prim allocators' size judgment, in i64 BEFORE the i32 wrap
    /// (the `bytes.new` / `bytes.repeat` shape, C-197): an element count
    /// whose byte total lies past the structural bound is the defined
    /// `Error: out of memory` abort, never a wrapped small request that
    /// the allocator satisfies and the element stores then overrun
    /// (#1908's sibling: `bytes.read_f16_le_array(b, 0, i64::MAX)` wrapped
    /// `n * 8` to a small size and trapped out of bounds on the first
    /// store past it, where native died in the C-197 form). The count is
    /// on the stack (i64) and stays there.
    pub(crate) fn emit_alloc_size_guard(&mut self, elem_bytes: i64) -> Result<(), EmitError> {
        let h = self.hold_i64()?;
        let oom = self.pool.intern("Error: out of memory");
        let mut i = self.f.instructions();
        i.local_tee(h);
        i.local_get(h).i64_const(0x7FFF_0000 / elem_bytes).i64_gt_s().if_(BlockType::Empty);
        i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
        let _ = i;
        self.release_i64();
        Ok(())
    }
}
