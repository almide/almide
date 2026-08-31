//! env / io / process host surfaces over the fs_call boundary — the
//! ops live in fs_meta.rs; this file is the guest-side dispatch.
//! `io.write`/`write_bytes` append RAW to the same stdout sink println
//! uses (PROGRAM order is the contract); `io.read_n_bytes` reads up to
//! n stdin bytes (n <= 0 is the empty list, no allocation).

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::fs_meta::{
    OP_ARGS, OP_CWD, OP_ENV_GET, OP_ENV_OS, OP_STDIN_TAKE, OP_STDOUT_RAW, OP_STDIN_READ, OP_TEMP_DIR,
};
use crate::*;

impl Emitter<'_> {
    /// env./io./process. calls. Ok(None) = not handled here.
    pub(crate) fn lower_host_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (module, func, args) {
            // Option[String]: status 2 = unset → none.
            ("env", "get", [name]) => {
                self.fs_call_1(name, OP_ENV_GET)?;
                let hret = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hret);
                i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(2).i32_eq();
                i.if_(BlockType::Result(wasm_encoder::ValType::I32));
                i.i32_const(almide_layout::NULL_ADDR as i32);
                i.else_();
                let _ = i;
                let hb = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_get(hret)
                    .i64_const(0xFFFF_FFFF)
                    .i64_and()
                    .i32_wrap_i64()
                    .call(F_ALLOC)
                    .local_set(hb);
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .call(F_HOST_READ);
                // some(str): a 4-byte option cell holding the handle
                let hs = self.tmp_i32_local;
                i.i32_const(4).call(F_ALLOC).local_set(hs);
                i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::OPTION_FIELD));
                i.local_get(hs);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i64();
                Some(SliceTy::Option(self.types.intern(STR)))
            }
            // Result[Unit, String]: the overlay set — key in a, value in
            // b, the fs.write two-string convention (#1423 bucket C).
            ("env", "set", [k, v]) => {
                self.fs_call_str2(k, v, crate::fs_meta::OP_ENV_SET)?;
                Some(self.fs_result_unit()?)
            }
            // The http string family (#1710 increment 1): every fn is
            // Result[String, String] — exactly the fs.read_text decode.
            ("http", "get", [u]) => {
                self.fs_call_1(u, crate::fs_meta::OP_HTTP_GET)?;
                Some(self.fs_result_string()?)
            }
            ("http", "delete", [u]) => {
                self.fs_call_1(u, crate::fs_meta::OP_HTTP_DELETE)?;
                Some(self.fs_result_string()?)
            }
            ("http", "post", [u, b]) => {
                self.fs_call_str2(u, b, crate::fs_meta::OP_HTTP_POST)?;
                Some(self.fs_result_string()?)
            }
            ("http", "put", [u, b]) => {
                self.fs_call_str2(u, b, crate::fs_meta::OP_HTTP_PUT)?;
                Some(self.fs_result_string()?)
            }
            ("http", "patch", [u, b]) => {
                self.fs_call_str2(u, b, crate::fs_meta::OP_HTTP_PATCH)?;
                Some(self.fs_result_string()?)
            }
            ("env", "os", []) => {
                self.fs_call_0(OP_ENV_OS)?;
                self.fs_take_text()?;
                Some(STR)
            }
            // Result[String, String] — the surface is fallible (matched
            // with ok/err), unlike the never-err os/temp_dir texts.
            ("env", "cwd", []) => {
                self.fs_call_0(OP_CWD)?;
                Some(self.fs_result_string()?)
            }
            ("env", "temp_dir", []) => {
                self.fs_call_0(OP_TEMP_DIR)?;
                self.fs_take_text()?;
                Some(STR)
            }
            ("env" | "process", "args", []) => {
                self.fs_call_0(OP_ARGS)?;
                let (hraw, hlen, _herr) = self.fs_frames_or_err()?;
                let hlist = self.hold_i32()?;
                self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hlist);
                self.fs_frames_foreach(hraw, hlen, |em| {
                    let hline = em.tmp_i32_local;
                    em.f.instructions().local_set(hline);
                    em.f
                        .instructions()
                        .local_get(hlist)
                        .local_get(hline)
                        .call(F_LIST_PUSH_4)
                        .local_set(hlist);
                    Ok(())
                })?;
                self.f.instructions().local_get(hlist);
                for _ in 0..4 {
                    self.release_i32();
                }
                Some(SliceTy::List(self.types.intern(STR)))
            }
            // stdin read-to-end (#1598's io half): the host's op-31 drain
            // parks the stream, and the raw-text builder collects it. RAW
            // String, not a Result block: the frontend absorbs the `!` on
            // this @intrinsic effect call (the probe showed the Bind's
            // value is the bare Call typed String), the same convention
            // env.os / env.temp_dir ride — and op 31 cannot fail.
            ("io", "read_all", []) => {
                self.fs_call_0(OP_STDIN_READ)?;
                self.fs_take_text()?;
                Some(STR)
            }
            // One byte off the stdin CURSOR (op 35): parked len 0 = EOF
            // -> -1, else the byte zero-extended — the native intrinsic's
            // exact contract, and the read composes with read_line /
            // read_n_bytes on the same cursor.
            ("io", "read_byte", []) => {
                self.fs_call_stdin_take(1)?;
                let hret = self.hold_i64()?;
                let hb = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hret);
                i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i64_eqz();
                i.if_(BlockType::Result(wasm_encoder::ValType::I64));
                i.i64_const(-1);
                i.else_();
                i.i32_const(1).call(F_ALLOC).local_set(hb);
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .call(F_HOST_READ);
                i.local_get(hb).i64_load8_u(crate::bytes::byte_k(0));
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i64();
                Some(INT)
            }
            // Byte-at-a-time off the stdin cursor until '\n' (excluded)
            // or EOF, trailing '\r' stripped — native
            // read_line().trim_end_matches and the incumbent leg's fd-0
            // cadence, on the SAME 4096 line cap as its scratch. RAW
            // String like read_all (the frontend absorbs the `!` on this
            // @intrinsic effect call; the read itself cannot fail).
            ("io", "read_line", []) => {
                let hbuf = self.hold_i32()?;
                let hn = self.hold_i32()?;
                let hret = self.hold_i64()?;
                {
                    let mut i = self.f.instructions();
                    i.i32_const(4096).call(F_ALLOC).local_set(hbuf);
                    i.i32_const(0).local_set(hn);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hn).i32_const(4096).i32_ge_u().br_if(1);
                }
                self.fs_call_stdin_take(1)?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hret);
                    // EOF -> done with what we have.
                    i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i64_eqz().br_if(1);
                    // park byte -> buf[n].
                    i.local_get(hbuf)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_get(hn)
                        .i32_add()
                        .call(F_HOST_READ);
                    // newline -> done (NOT counted).
                    i.local_get(hbuf).local_get(hn).i32_add();
                    i.i64_load8_u(crate::bytes::byte_k(0));
                    i.i64_const(10).i64_eq().br_if(1);
                    i.local_get(hn).i32_const(1).i32_add().local_set(hn);
                    i.br(0).end().end();
                    // strip one trailing '\r' (CRLF endings).
                    i.local_get(hn).i32_const(0).i32_gt_u().if_(BlockType::Empty);
                    i.local_get(hbuf).local_get(hn).i32_add().i32_const(1).i32_sub();
                    i.i64_load8_u(crate::bytes::byte_k(0)).i64_const(13).i64_eq();
                    i.if_(BlockType::Empty);
                    i.local_get(hn).i32_const(1).i32_sub().local_set(hn);
                    i.end();
                    i.end();
                    // the block was allocated len=4096; the LINE's length
                    // is what the string observes.
                    i.local_get(hbuf).local_get(hn).i32_store(len_memarg());
                    i.local_get(hbuf);
                }
                self.release_i64();
                self.release_i32();
                self.release_i32();
                Some(STR)
            }
            ("io", "write", [b]) => {
                self.lower(b, Some(SliceTy::Scalar(Scalar::Bytes)))?;
                self.io_stdout_raw()?;
                None
            }
            // String bytes to the same raw sink (print is write minus the
            // Bytes spelling — Strings share the len=bytes layout), then
            // the always-ok unit carrier: print has no failure channel,
            // but the effect ABI still hands the caller a Result block.
            ("io", "print", [s]) => {
                self.lower(s, Some(STR))?;
                self.io_stdout_raw()?;
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hb)
                        .i32_const(0)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hb).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.local_get(hb);
                }
                self.release_i32();
                let uh = self.types.intern(SliceTy::Unit);
                let sh = self.types.intern(STR);
                Some(SliceTy::Result(uh, sh))
            }
            // Unit effect with no failure channel and no observable value:
            // sleep on the host (the ms count rides the a_len slot with a
            // null a_ptr — the op-35 scalar discipline; clamped to
            // [0, i32::MAX]), the i64 status dropped, then the always-ok
            // unit carrier io.print builds (#1423 bucket A).
            ("env", "sleep_ms", [ms]) => {
                self.lower(ms, Some(INT))?;
                let hm = self.hold_i64()?;
                self.note_host_op(crate::fs_meta::OP_SLEEP_MS);
                {
                    let mut i = self.f.instructions();
                    i.local_set(hm);
                    i.i32_const(crate::fs_meta::OP_SLEEP_MS);
                    i.i32_const(0);
                    i.local_get(hm).i64_const(0).i64_lt_s();
                    i.if_(BlockType::Result(wasm_encoder::ValType::I64));
                    i.i64_const(0);
                    i.else_();
                    i.local_get(hm).i64_const(0x7FFF_FFFF).i64_lt_s();
                    i.if_(BlockType::Result(wasm_encoder::ValType::I64));
                    i.local_get(hm);
                    i.else_();
                    i.i64_const(0x7FFF_FFFF);
                    i.end();
                    i.end();
                    i.i32_wrap_i64();
                    i.i32_const(0).i32_const(0);
                    i.call(F_FS_CALL);
                    i.drop();
                }
                self.release_i64();
                let hb = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.i32_const(16)
                        .call(F_ALLOC)
                        .local_tee(hb)
                        .i32_const(0)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    i.local_get(hb).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_FIELD));
                    i.local_get(hb);
                }
                self.release_i32();
                let uh = self.types.intern(SliceTy::Unit);
                let sh = self.types.intern(STR);
                Some(SliceTy::Result(uh, sh))
            }
            // List[Int] → low bytes, then the same raw sink.
            ("io", "write_bytes", [xs]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == INT => {}
                    other => return unsup(&format!("io-write-bytes-of:{other:?}")),
                }
                let hl = self.hold_i32()?;
                let hb = self.hold_i32()?;
                let hk = self.hold_i32()?;
                let hn = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hl);
                i.local_get(hl).i32_load(len_memarg()).i32_const(3).i32_shr_u().local_set(hn);
                i.local_get(hn).call(F_ALLOC).local_set(hb);
                i.i32_const(0).local_set(hk);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
                i.local_get(hb).local_get(hk).i32_add();
                i.local_get(hl).local_get(hk).i32_const(3).i32_shl().i32_add();
                i.i64_load(slot_memarg(0)).i32_wrap_i64();
                i.i32_store8(crate::bytes::byte_k(0));
                i.local_get(hk).i32_const(1).i32_add().local_set(hk);
                i.br(0).end().end();
                i.local_get(hb);
                let _ = i;
                for _ in 0..4 {
                    self.release_i32();
                }
                self.io_stdout_raw()?;
                None
            }
            // n <= 0 → []; else read up to n stdin bytes (harness: none)
            // and decode one i64 slot per byte.
            ("io", "read_n_bytes", [n]) => {
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                i.local_get(hn).i64_const(0).i64_le_s();
                i.if_(BlockType::Result(wasm_encoder::ValType::I32));
                i.i32_const(0).call(F_ALLOC);
                i.else_();
                // The count rides the a_len SLOT of op 35 (never a guest
                // buffer — a len slot makes the host read guest memory;
                // i64::MAX once pulled 4 GiB out of a 17-page instance).
                // The host serves UP TO n bytes off the stdin CURSOR, so
                // sequential reads compose with read_byte/read_line
                // exactly as native's shared stdin handle does.
                self.note_host_op(OP_STDIN_TAKE);
                let mut i = self.f.instructions();
                i.i32_const(OP_STDIN_TAKE);
                i.i32_const(0);
                i.local_get(hn).i64_const(0x7FFF_FFFF).i64_lt_s();
                i.if_(BlockType::Result(wasm_encoder::ValType::I64));
                i.local_get(hn);
                i.else_();
                i.i64_const(0x7FFF_FFFF);
                i.end();
                i.i32_wrap_i64();
                i.i32_const(0).i32_const(0);
                i.call(F_FS_CALL);
                let _ = i;
                // decode: n bytes → n i64 slots (never errs)
                let hret = self.hold_i64()?;
                let hraw = self.hold_i32()?;
                let hlen = self.hold_i32()?;
                let hout = self.hold_i32()?;
                let hk = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hret);
                i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().local_set(hlen);
                i.local_get(hlen).call(F_ALLOC).local_set(hraw);
                i.local_get(hraw)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .call(F_HOST_READ);
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
                i.local_get(hout);
                i.end();
                let _ = i;
                for _ in 0..4 {
                    self.release_i32();
                }
                self.release_i64();
                self.release_i64();
                Some(SliceTy::List(self.types.intern(INT)))
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// Bytes handle on the stack → op 30 (raw stdout append).
    fn io_stdout_raw(&mut self) -> Result<(), EmitError> {
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        self.note_host_op(OP_STDOUT_RAW);
        let mut i = self.f.instructions();
        i.i32_const(OP_STDOUT_RAW);
        i.i32_const(0).i32_const(0);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_load(len_memarg());
        i.call(F_FS_CALL);
        i.drop();
        let _ = i;
        self.release_i32();
        Ok(())
    }
}
