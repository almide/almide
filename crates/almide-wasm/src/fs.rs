//! The fs HOST BOUNDARY (guest side): every `fs.*` call crosses
//! `almide.fs_call(op, a_ptr, a_len, b_ptr, b_len) -> i64` — the host
//! (the harness) runs the SAME std::fs code the native runtime runs, so
//! error strings match verbatim — then the guest pulls result bytes
//! through `almide.host_read(dst)`. Return packing: status = ret >> 32
//! (0 ok / 1 err / 2 ok-none), len = low 32. List-of-strings results
//! arrive as u32-LE length-prefixed frames.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

const OP_READ_TEXT: i32 = 1;
const OP_WRITE: i32 = 2;
const OP_WRITE_BYTES: i32 = 3;
const OP_EXISTS: i32 = 4;
const OP_IS_DIR: i32 = 5;
const OP_IS_FILE: i32 = 6;
const OP_MKDIR_P: i32 = 7;
const OP_REMOVE: i32 = 8;
const OP_REMOVE_ALL: i32 = 9;
const OP_CREATE_TEMP_DIR: i32 = 10;
const OP_LIST_DIR: i32 = 11;
const OP_READ_LINES: i32 = 12;
const OP_READ_TEXT_IF_EXISTS: i32 = 13;
const OP_READ_BYTES: i32 = 14;
const OP_WRITE_BYTES_RAW: i32 = 15;
const OP_APPEND: i32 = 16;

impl Emitter<'_> {
    /// `fs.*` module calls. Ok(None) = not handled here.
    pub(crate) fn lower_fs_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("read_text", [p]) => {
                self.fs_call_1(p, OP_READ_TEXT)?;
                self.fs_result_string()?
            }
            ("create_temp_dir", [p]) => {
                self.fs_call_1(p, OP_CREATE_TEMP_DIR)?;
                self.fs_result_string()?
            }
            ("write", [p, c]) => {
                self.fs_call_str2(p, c, OP_WRITE)?;
                self.fs_result_unit()?
            }
            ("append", [p, c]) => {
                self.fs_call_str2(p, c, OP_APPEND)?;
                self.fs_result_unit()?
            }
            ("write_bytes", [p, xs]) => {
                // b = the List[Int] payload (8-byte i64 LE slots; the
                // host takes the low byte of each — native `x as u8`).
                self.lower(p, Some(STR))?;
                let hp = self.hold_i32()?;
                self.f.instructions().local_set(hp);
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == INT => {}
                    other => return unsup(&format!("fs-write-bytes-of:{other:?}")),
                }
                let hb = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hb);
                self.note_host_op(OP_WRITE_BYTES);
                let mut i = self.f.instructions();
                i.i32_const(OP_WRITE_BYTES);
                i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hp).i32_load(len_memarg());
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_load(len_memarg());
                i.call(F_FS_CALL);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.fs_result_unit()?
            }
            ("write_bytes_raw", [p, bs]) => {
                self.lower(p, Some(STR))?;
                let hp = self.hold_i32()?;
                self.f.instructions().local_set(hp);
                match self.lower(bs, None)? {
                    SliceTy::Scalar(Scalar::Bytes) => {}
                    other => return unsup(&format!("fs-write-bytes-raw-of:{other:?}")),
                }
                let hb = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hb);
                self.note_host_op(OP_WRITE_BYTES_RAW);
                let mut i = self.f.instructions();
                i.i32_const(OP_WRITE_BYTES_RAW);
                i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hp).i32_load(len_memarg());
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_load(len_memarg());
                i.call(F_FS_CALL);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.fs_result_unit()?
            }
            ("mkdir_p", [p]) => {
                self.fs_call_1(p, OP_MKDIR_P)?;
                self.fs_result_unit()?
            }
            ("remove", [p]) => {
                self.fs_call_1(p, OP_REMOVE)?;
                self.fs_result_unit()?
            }
            ("remove_all", [p]) => {
                self.fs_call_1(p, OP_REMOVE_ALL)?;
                self.fs_result_unit()?
            }
            ("exists" | "is_dir" | "is_file", [p]) => {
                let op = match func {
                    "exists" => OP_EXISTS,
                    "is_dir" => OP_IS_DIR,
                    _ => OP_IS_FILE,
                };
                self.fs_call_1(p, op)?;
                // flag rides the len half; never errs.
                self.f.instructions().i32_wrap_i64();
                BOOL
            }
            _ => return self.lower_fs_call_b(func, args),
        };
        Ok(Some(Some(out)))
    }


    /// fs.read_bytes: raw host bytes → List[Int] (one i64 slot per byte).
    fn lower_fs_read_bytes(&mut self, p: &IrExpr) -> Result<SliceTy, EmitError> {
        Ok({

                self.fs_call_1(p, OP_READ_BYTES)?;
                // raw bytes → List[Int] (one i64 slot per byte)
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
                    // err(msg): hraw IS the message string
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hout)
                        .i32_const(1)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hout).local_get(hraw).i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.local_get(hout);
                    i.else_();
                    // decode: n bytes → n i64 slots
                    i.local_get(hlen).i32_const(3).i32_shl().call(F_ALLOC).local_set(hout);
                    i.i32_const(0).local_set(hk);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hk).local_get(hlen).i32_ge_u().br_if(1);
                    i.local_get(hout).local_get(hk).i32_const(3).i32_shl().i32_add();
                    i.local_get(hraw).local_get(hk).i32_add();
                    i.i32_load8_u(byte_memarg()).i64_extend_i32_u();
                    i.i64_store(slot_memarg(0));
                    i.local_get(hk).i32_const(1).i32_add().local_set(hk);
                    i.br(0).end().end();
                    // ok(list)
                    let hs = self.tmp_i32_local;
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_set(hs);
                    i.local_get(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hs).local_get(hout).i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.local_get(hs);
                    i.end();
                }
                for _ in 0..4 {
                    self.release_i32();
                }
                self.release_i64();
                let ih = self.types.intern(INT);
                let lh = self.types.intern(SliceTy::List(ih));
                SliceTy::Result(lh, self.types.intern(STR))
        })
    }

    /// path → fs_call(op, path, 0, 0): the i64 ret is on the stack.
    pub(crate) fn fs_call_1(&mut self, p: &IrExpr, op: i32) -> Result<(), EmitError> {
        self.note_host_op(op);
        self.lower(p, Some(STR))?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hp);
        i.i32_const(op);
        i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hp).i32_load(len_memarg());
        i.i32_const(0).i32_const(0);
        i.call(F_FS_CALL);
        let _ = i;
        self.release_i32();
        Ok(())
    }

    /// (path, content) both strings → fs_call(op, path, content).
    pub(crate) fn fs_call_str2(&mut self, p: &IrExpr, c: &IrExpr, op: i32) -> Result<(), EmitError> {
        self.note_host_op(op);
        self.lower(p, Some(STR))?;
        let hp = self.hold_i32()?;
        self.f.instructions().local_set(hp);
        self.lower(c, Some(STR))?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hc);
        i.i32_const(op);
        i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hp).i32_load(len_memarg());
        i.local_get(hc).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hc).i32_load(len_memarg());
        i.call(F_FS_CALL);
        let _ = i;
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// ret on stack → Result[String, String] block on stack.
    pub(crate) fn fs_result_string(&mut self) -> Result<SliceTy, EmitError> {
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        // the payload string (ok text or err message)
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
        let sh = self.types.intern(STR);
        Ok(SliceTy::Result(sh, sh))
    }

    /// ret on stack → Result[Unit, String] block on stack.
    pub(crate) fn fs_result_unit(&mut self) -> Result<SliceTy, EmitError> {
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(1).i32_eq();
        i.if_(BlockType::Result(ValType::I32));
        // err(msg)
        i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        let hs = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs);
        i.else_();
        i.i32_const(16).call(F_ALLOC).local_tee(hb).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hb).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hb);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        let uh = self.types.intern(SliceTy::Unit);
        let sh = self.types.intern(STR);
        Ok(SliceTy::Result(uh, sh))
    }

    /// ret on stack → Result[List[String], String] from the frames buffer.
    pub(crate) fn fs_result_string_list(&mut self) -> Result<SliceTy, EmitError> {
        let (hraw, hlen, herr) = self.fs_frames_or_err()?;
        let hlist = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hlist);
        self.fs_frames_foreach(hraw, hlen, |em| {
            let hline = em.tmp_i32_local;
            em.f.instructions().local_set(hline);
            em.f.instructions().local_get(hlist).local_get(hline).call(F_LIST_PUSH_4).local_set(hlist);
            Ok(())
        })?;
        let hs = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            i.local_get(herr);
            i.else_();
            i.i32_const(16).call(F_ALLOC).local_tee(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hs).local_get(hlist).i32_store(slot_memarg(almide_layout::SUM_FIELD));
            i.local_get(hs);
            i.end();
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        let sh = self.types.intern(STR);
        let lh = self.types.intern(SliceTy::List(sh));
        Ok(SliceTy::Result(lh, sh))
    }

    /// ret on stack → pull the buffer into a fresh raw block; if status
    /// is err, build the err Result into a hold and set the flag hold;
    /// leaves THREE holds live (raw, rawlen, err-result) for the frame
    /// walker + result assembly. Returns the err-result hold (0 = ok).
    pub(crate) fn fs_frames_or_err(&mut self) -> Result<(u32, u32, u32), EmitError> {
        let hret = self.hold_i64()?;
        let hraw = self.hold_i32()?;
        let hlen = self.hold_i32()?;
        let herr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().local_set(hlen);
        i.local_get(hlen).call(F_ALLOC).local_set(hraw);
        i.local_get(hraw).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        i.i32_const(0).local_set(herr);
        i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(1).i32_eq();
        i.if_(BlockType::Empty);
        let hs = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hraw).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs).local_set(herr);
        i.i32_const(0).local_set(hlen);
        i.end();
        let _ = i;
        self.release_i64();
        Ok((hraw, hlen, herr))
    }

    /// Walk the u32-LE length-prefixed frames in the raw block (the
    /// three holds from fs_frames_or_err are live: raw, len, err), the
    /// per-frame STRING block on the stack for `body`.
    pub(crate) fn fs_frames_foreach(
        &mut self,
        hraw: u32,
        hlen: u32,
        body: impl FnOnce(&mut Self) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let hoff = self.hold_i32()?;
        let hfl = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hoff);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hoff).local_get(hlen).i32_ge_u().br_if(1);
            i.local_get(hraw).local_get(hoff).i32_add().i32_load(slot_memarg(0)).local_set(hfl);
            i.local_get(hfl).call(F_ALLOC);
            i.local_tee(self.scr_i32_local);
            i.i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hraw)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(hoff)
                .i32_add()
                .i32_const(4)
                .i32_add();
            i.local_get(hfl);
            i.memory_copy(0, 0);
            i.local_get(self.scr_i32_local);
        }
        body(self)?;
        {
            let mut i = self.f.instructions();
            i.local_get(hoff).i32_const(4).i32_add().local_get(hfl).i32_add().local_set(hoff);
            i.br(0).end().end();
        }
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}

fn byte_memarg() -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// The second half of the `fs.*` dispatch — split from `lower_fs_call`
    /// for the complexity budget (the module-call twin pattern).
    fn lower_fs_call_b(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("list_dir", [p]) => {
                self.fs_call_1(p, OP_LIST_DIR)?;
                self.fs_result_string_list()?
            }
            ("read_lines", [p]) => {
                self.fs_call_1(p, OP_READ_LINES)?;
                self.fs_result_string_list()?
            }
            ("read_text_if_exists", [p]) => {
                self.fs_call_1(p, OP_READ_TEXT_IF_EXISTS)?;
                // status 2 = ok(none); else the string result some-wraps.
                let hret = self.hold_i64()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hret);
                    i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(2).i32_eq();
                    i.if_(BlockType::Result(ValType::I32));
                    // ok(none)
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
                let _ = self.fs_result_string()?;
                {
                    // stack: Result[String, String] — rewrap the ok side
                    // as some(payload).
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
                let sh = self.types.intern(STR);
                let oh = self.types.intern(SliceTy::Option(sh));
                SliceTy::Result(oh, sh)
            }
            ("read_bytes", [p]) => self.lower_fs_read_bytes(p)?,
            ("fold_lines", [p, init, cb]) => {
                let (params, body) = self.hof_lambda(cb, 2)?;
                let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
                    return unsup(&format!("fs-fold-acc:{}", ty_name(&init.ty)));
                };
                self.lower(init, Some(acc_ty))?;
                self.f.instructions().local_set(params[0]);
                self.fs_call_1(p, OP_READ_LINES)?;
                let (hraw, hlen, herr) = self.fs_frames_or_err()?;
                // walk the frames, folding
                self.fs_frames_foreach(hraw, hlen, |em| {
                    em.f.instructions().local_set(params[1]);
                    em.lower(body, Some(acc_ty))?;
                    em.f.instructions().local_set(params[0]);
                    Ok(())
                })?;
                // ok(acc) / err passthrough
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(herr).if_(BlockType::Result(ValType::I32));
                    i.local_get(herr);
                    i.else_();
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hs)
                        .i32_const(0)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hs).local_get(params[0]);
                }
                self.store_ty_slot(acc_ty, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hs).end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                SliceTy::Result(self.types.intern(acc_ty), self.types.intern(STR))
            }
            ("for_each_line", [p, cb]) => {
                let (params, body) = self.hof_lambda(cb, 1)?;
                self.fs_call_1(p, OP_READ_LINES)?;
                let (hraw, hlen, herr) = self.fs_frames_or_err()?;
                self.fs_frames_foreach(hraw, hlen, |em| {
                    em.f.instructions().local_set(params[0]);
                    em.lower_stmt_expr(body)?;
                    Ok(())
                })?;
                let hs = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(herr).if_(BlockType::Result(ValType::I32));
                    i.local_get(herr);
                    i.else_();
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hs)
                        .i32_const(0)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.local_get(hs);
                    i.end();
                }
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                SliceTy::Result(self.types.intern(SliceTy::Unit), self.types.intern(STR))
            }
            _ => return self.lower_fs_meta_call(func, args),
        };
        Ok(Some(Some(out)))
    }
}
