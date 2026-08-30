//! The fs metadata / rename / walk / if-exists surfaces + the raw-bytes
//! readers — the host-boundary ops past the first sixteen (fs.rs holds
//! the protocol doctrine). Same packing: status<<32 | len, frames for
//! string lists, an 8-byte LE buffer for i64 results.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

pub(crate) const OP_FILE_SIZE: i32 = 17;
pub(crate) const OP_MODIFIED_AT: i32 = 18;
pub(crate) const OP_COPY: i32 = 19;
pub(crate) const OP_RENAME: i32 = 20;
pub(crate) const OP_CREATE_TEMP_FILE: i32 = 21;
pub(crate) const OP_IS_SYMLINK: i32 = 22;
pub(crate) const OP_WALK: i32 = 23;
pub(crate) const OP_READ_LINES_IF_EXISTS: i32 = 24;
pub(crate) const OP_READ_BYTES_IF_EXISTS: i32 = 25;
pub(crate) const OP_ENV_GET: i32 = 26;
pub(crate) const OP_ENV_OS: i32 = 27;
pub(crate) const OP_TEMP_DIR: i32 = 28;
pub(crate) const OP_ARGS: i32 = 29;
pub(crate) const OP_STDOUT_RAW: i32 = 30;
pub(crate) const OP_STDIN_READ: i32 = 31;
pub(crate) const OP_STDIN_TAKE: i32 = 35;
pub(crate) const OP_RANDOM_GET: i32 = 32;
pub(crate) const OP_CWD: i32 = 33;
pub(crate) const OP_WALL_NOW: i32 = 34;
/// env.sleep_ms (#1423 bucket A): the millisecond count rides the a_len
/// slot with a null a_ptr (the op-35 scalar discipline — never a guest
/// buffer), handled by the host BEFORE its buffer reads. No observable
/// value: the guest builds the always-ok unit carrier itself.
pub(crate) const OP_SLEEP_MS: i32 = 36;
const OP_READ_BYTES: i32 = 14;

impl Emitter<'_> {
    /// The fs surfaces past fs.rs's set. Ok(None) = not handled.
    pub(crate) fn lower_fs_meta_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("file_size" | "modified_at", [p]) => {
                let op = if func == "file_size" { OP_FILE_SIZE } else { OP_MODIFIED_AT };
                self.fs_call_1(p, op)?;
                self.fs_result_i64()?
            }
            ("copy" | "rename", [a, b]) => {
                let op = if func == "copy" { OP_COPY } else { OP_RENAME };
                self.fs_call_str2(a, b, op)?;
                self.fs_result_unit()?
            }
            ("create_temp_file", [p]) => {
                self.fs_call_1(p, OP_CREATE_TEMP_FILE)?;
                self.fs_result_string()?
            }
            ("is_symlink", [p]) => {
                self.fs_call_1(p, OP_IS_SYMLINK)?;
                self.f.instructions().i32_wrap_i64();
                BOOL
            }
            ("walk", [p]) => {
                self.fs_call_1(p, OP_WALK)?;
                self.fs_result_string_list()?
            }
            ("temp_dir", []) => {
                self.fs_call_0(OP_TEMP_DIR)?;
                self.fs_take_text()?;
                STR
            }
            ("read_lines_if_exists", [p]) => {
                self.fs_call_1(p, OP_READ_LINES_IF_EXISTS)?;
                let sh = self.types.intern(STR);
                let lh = self.types.intern(SliceTy::List(sh));
                self.fs_if_exists_wrap(lh, |em| {
                    let _ = em.fs_result_string_list()?;
                    Ok(())
                })?
            }
            ("read_bytes_if_exists", [p]) => {
                self.fs_call_1(p, OP_READ_BYTES_IF_EXISTS)?;
                let ih = self.types.intern(INT);
                let lh = self.types.intern(SliceTy::List(ih));
                self.fs_if_exists_wrap(lh, |em| {
                    em.fs_decode_byte_list()?;
                    Ok(())
                })?
            }
            ("read_bytes_raw", [p]) => {
                self.fs_call_1(p, OP_READ_BYTES)?;
                self.fs_result_bytes()?
            }
            ("read_bytes_raw_if_exists", [p]) => {
                self.fs_call_1(p, OP_READ_BYTES_IF_EXISTS)?;
                let bh = self.types.intern(SliceTy::Scalar(Scalar::Bytes));
                self.fs_if_exists_wrap(bh, |em| {
                    let _ = em.fs_result_bytes()?;
                    Ok(())
                })?
            }
            // ADR-0006's fallible walker carrier: the callback answers
            // Result and the FIRST err ends the walk (the file is already
            // in memory — C-220's streaming carve-out keeps reader
            // position out of the wasm observables).
            (f, [p, init, cb]) if f.starts_with("__fallible_fold_lines") => {
                self.lower_fs_fallible_fold(p, init, cb)?
            }
            _ => return Ok(None),
        };
        Ok(Some(Some(out)))
    }

    /// no-arg host op: fs_call(op, 0,0,0,0) — ret on the stack.
    pub(crate) fn fs_call_0(&mut self, op: i32) -> Result<(), EmitError> {
        let mut i = self.f.instructions();
        i.i32_const(op);
        i.i32_const(0).i32_const(0).i32_const(0).i32_const(0);
        i.call(F_FS_CALL);
        Ok(())
    }

    /// Incremental stdin (op 35): up to `count` bytes off the stream's
    /// cursor. The count rides in the a_len SLOT with a null a_ptr — a
    /// scalar, never a guest buffer (the op-31 comment's 4 GiB trap) —
    /// and the host special-cases the op before its buffer reads.
    pub(crate) fn fs_call_stdin_take(&mut self, count: i32) -> Result<(), EmitError> {
        let mut i = self.f.instructions();
        i.i32_const(OP_STDIN_TAKE);
        i.i32_const(0).i32_const(count).i32_const(0).i32_const(0);
        i.call(F_FS_CALL);
        Ok(())
    }

    /// ret on stack → the text payload as a fresh String block (no
    /// Result wrap — for the never-err text ops).
    pub(crate) fn fs_take_text(&mut self) -> Result<(), EmitError> {
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        i.local_get(hb);
        let _ = i;
        self.release_i32();
        Ok(())
    }

    /// ret on stack → Result[Int, String]: ok rides an 8-byte LE buffer.
    fn fs_result_i64(&mut self) -> Result<SliceTy, EmitError> {
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(1).i32_eq();
        i.if_(BlockType::Result(ValType::I32));
        i.local_get(hret)
            .i64_const(0xFFFF_FFFF)
            .i64_and()
            .i32_wrap_i64()
            .call(F_ALLOC)
            .local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        let hs = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs);
        i.else_();
        // ok: pull 8 LE bytes into a scratch block, load the i64
        i.i32_const(8).call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        let hs2 = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs2);
        i.local_get(hs2).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs2);
        i.local_get(hb).i64_load(slot_memarg(0));
        i.i64_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs2);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        Ok(SliceTy::Result(self.types.intern(INT), self.types.intern(STR)))
    }

    /// ret on stack → Result[Bytes, String]: the raw buffer IS the ok
    /// payload (Bytes and the message share the block layout).
    pub(crate) fn fs_result_bytes(&mut self) -> Result<SliceTy, EmitError> {
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        let hs = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs);
        i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64();
        i.i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs);
        let _ = i;
        self.release_i32();
        self.release_i64();
        Ok(SliceTy::Result(
            self.types.intern(SliceTy::Scalar(Scalar::Bytes)),
            self.types.intern(STR),
        ))
    }

    /// Shared *_if_exists wrapper: status 2 → ok(none); else the inner
    /// builder produces Result[T, String] whose ok side gets some-boxed.
    /// The inner builder must CONSUME the i64 ret from the stack.
    fn fs_if_exists_wrap(
        &mut self,
        inner: ETy,
        build: impl FnOnce(&mut Self) -> Result<(), EmitError>,
    ) -> Result<SliceTy, EmitError> {
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hret);
            i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(2).i32_eq();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hb)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hb).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.local_get(hb);
            i.else_();
            i.local_get(hret);
        }
        build(self)?;
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            i.local_get(hb).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.if_(BlockType::Empty);
            let hs = self.tmp_i32_local;
            i.i32_const(4).call(F_ALLOC).local_set(hs);
            i.local_get(hs);
            i.local_get(hb).i32_load(slot_memarg(almide_layout::SUM_FIELD));
            i.i32_store(slot_memarg(almide_layout::OPTION_FIELD));
            i.local_get(hb).local_get(hs).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.end();
            i.local_get(hb);
            i.end();
        }
        self.release_i32();
        self.release_i64();
        let oh = self.types.intern(SliceTy::Option(inner));
        Ok(SliceTy::Result(oh, self.types.intern(STR)))
    }

    /// ret on stack → Result[List[Int], String] (the read_bytes decode,
    /// shared with the if-exists twin).
    fn fs_decode_byte_list(&mut self) -> Result<(), EmitError> {
        let hret = self.hold_i64()?;
        let hraw = self.hold_i32()?;
        let hlen = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hk = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hret);
            i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().local_set(hlen);
            i.local_get(hlen).call(F_ALLOC).local_set(hraw);
            i.local_get(hraw).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
            i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(1).i32_eq();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hout)
                .i32_const(1)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hout).local_get(hraw).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.local_get(hout);
            i.else_();
            i.local_get(hlen).i32_const(3).i32_shl().call(F_ALLOC).local_set(hout);
            i.i32_const(0).local_set(hk);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hk).local_get(hlen).i32_ge_u().br_if(1);
            i.local_get(hout).local_get(hk).i32_const(3).i32_shl().i32_add();
            i.local_get(hraw).local_get(hk).i32_add();
            i.i64_load8_u(crate::bytes::byte_k(0));
            i.i64_store(slot_memarg(0));
            i.local_get(hk).i32_const(1).i32_add().local_set(hk);
            i.br(0).end().end();
            let hs = self.tmp_i32_local;
            i.i32_const(16).call(F_ALLOC).local_set(hs);
            i.local_get(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).local_get(hout).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.local_get(hs);
            i.end();
        }
        for _ in 0..4 {
            self.release_i32();
        }
        self.release_i64();
        Ok(())
    }

    /// The fallible fold: read_lines frames; per line the callback yields
    /// Result[acc, String] — the first err becomes the WHOLE result and
    /// later lines never see the callback (a done-flag guards the walk).
    fn lower_fs_fallible_fold(
        &mut self,
        p: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("fs-fallible-fold-acc:{}", ty_name(&init.ty)));
        };
        self.lower(init, Some(acc_ty))?;
        self.f.instructions().local_set(params[0]);
        self.fs_call_1(p, 12)?; // OP_READ_LINES
        let (hraw, hlen, herr) = self.fs_frames_or_err()?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        self.fs_frames_foreach(hraw, hlen, |em| {
            let hline = em.tmp_i32_local;
            em.f.instructions().local_set(hline);
            // once an err landed, the callback never runs again
            em.f.instructions().local_get(hr).i32_eqz().if_(BlockType::Empty);
            em.f.instructions().local_get(hline).local_set(params[1]);
            em.lower(body, None)?;
            let hres = em.tmp_i32_local;
            let mut i = em.f.instructions();
            i.local_set(hres);
            i.local_get(hres).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.if_(BlockType::Empty);
            i.local_get(hres);
            let _ = i;
            em.load_ty_slot(acc_ty, almide_layout::SUM_FIELD);
            let mut i = em.f.instructions();
            i.local_set(params[0]);
            i.else_();
            i.local_get(hres).local_set(hr);
            i.end();
            i.end();
            Ok(())
        })?;
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            i.local_get(herr);
            i.else_();
            i.local_get(hr).if_(BlockType::Result(ValType::I32));
            i.local_get(hr);
            i.else_();
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hs)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).local_get(params[0]);
        }
        self.store_ty_slot(acc_ty, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hs).end().end();
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(SliceTy::Result(self.types.intern(acc_ty), self.types.intern(STR)))
    }
}
