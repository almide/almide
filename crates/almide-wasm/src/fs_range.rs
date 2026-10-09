//! `fs.fold_lines_range` / `fs.fold_lines_chunked` (#2744): the byte-range
//! walkers of the streaming family, as native arms generic over the
//! accumulator.
//!
//! The self-host bodies (stdlib/fs_fold_lines.almd) cannot be linked on this
//! leg: they read the INCUMBENT's Result tag and payload raw
//! (`load32(rh + 16)`, `load_handle(rh + 12)`) and come in one twin per
//! accumulator class. These arms read the file through their own host op
//! (61 / 62 — op 1's body under the writer's call name, so a failure says
//! `fs.fold_lines_range(...)` as native does, #2090) and walk its bytes with
//! the native runtime's ownership rule (runtime/rs/src/fs.rs
//! `fold_lines_range_impl`): a line belongs to the range holding the byte
//! BEFORE its first byte (line 0 to the range holding byte 0). `start > 0`
//! begins at `start - 1` and discards through the first `\n`; lines then
//! fold while their start offset is `< end`, and the last may read past
//! `end` to finish. A line drops its `\n` and then one `\r` before it,
//! exactly as native's `read_line` + pop pair does.
//!
//! `fold_lines_chunked` computes native's byte partition (`chunk = size /
//! max(workers, 1) + 1`; range `i` = `[i * chunk, min(+chunk, size))` while
//! its start is in the file) and folds the ranges SEQUENTIALLY in chunk
//! order: the partials list is native's, and parallelism is a wall-clock
//! property, never an observable (C-220).
//!
//! Native's discard is a `read_line` into a String, so a range whose
//! `start - 1` falls INSIDE a multi-byte character errs with the invalid
//! UTF-8 message (a chunked fold over non-ASCII text hits it whenever a
//! chunk boundary splits a character); the arms reproduce that error and
//! its bytes. One divergence is inherent to reading the file whole: a file
//! whose invalid UTF-8 lies OUTSIDE the folded range errs here and not
//! natively.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

pub(crate) const OP_FOLD_LINES_RANGE: i32 = 61;
pub(crate) const OP_FOLD_LINES_CHUNKED: i32 = 62;

fn byte_at() -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// Lower `p` and read the whole file through `op`: leaves the three
    /// frame holds (raw, len, err) of `fs_frames_or_err` plus the path hold.
    fn fs_range_read(&mut self, p: &IrExpr, op: i32) -> Result<u32, EmitError> {
        self.note_host_op(op);
        self.lower_arg(p, Some(STR), ArgMode::Borrow)?;
        let hp = self.hold_i32()?;
        self.f.instructions().local_set(hp);
        Ok(hp)
    }

    fn fs_range_call(&mut self, hp: u32, op: i32) -> Result<(u32, u32, u32), EmitError> {
        {
            let mut i = self.f.instructions();
            i.i32_const(op);
            i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hp).i32_load(len_memarg());
            i.i32_const(0).i32_const(0);
            i.call(F_FS_CALL);
        }
        self.fs_frames_or_err()
    }

    /// Advance local `q` to the first `\n` at or after it, or to `len`.
    fn emit_scan_nl(&mut self, hraw: u32, hlen: u32, q: u32) {
        let mut i = self.f.instructions();
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(q).local_get(hlen).i32_ge_u().br_if(1);
        i.local_get(hraw).local_get(q).i32_add().i32_load8_u(byte_at()).i32_const(10).i32_eq().br_if(1);
        i.local_get(q).i32_const(1).i32_add().local_set(q);
        i.br(0).end().end();
    }

    /// Fold the lines of the text in `hraw` (`hlen` bytes) owned by the
    /// range [`hs`, `he`) into `params[0]`. `hs` is 0, an in-file offset, or
    /// `hlen + 1` (past the file: no line); `he` is clamped to [0, hlen].
    /// A discard that starts inside a character sets `hbad` and folds
    /// nothing (native's `read_line` fails there).
    #[allow(clippy::too_many_arguments)]
    fn emit_range_walk(
        &mut self,
        hraw: u32,
        hlen: u32,
        hs: u32,
        he: u32,
        hbad: u32,
        params: &[u32],
        cb: &IrExpr,
        body: &IrExpr,
        acc_ty: SliceTy,
    ) -> Result<(), EmitError> {
        let hpos = self.hold_i32()?;
        let hq = self.hold_i32()?;
        let hle = self.hold_i32()?;
        // The first owned line: 0, or one past the first `\n` at or after
        // `start - 1` (none = past the file).
        {
            let mut i = self.f.instructions();
            i.local_get(hs).i32_eqz().if_(BlockType::Empty);
            i.i32_const(0).local_set(hpos);
            i.else_();
            i.local_get(hs).i32_const(1).i32_sub().local_set(hq);
            // A UTF-8 continuation byte (0b10xx_xxxx) at `start - 1`.
            i.local_get(hq).local_get(hlen).i32_lt_u().if_(BlockType::Result(ValType::I32));
            i.local_get(hraw).local_get(hq).i32_add().i32_load8_u(byte_at());
            i.i32_const(0xC0).i32_and().i32_const(0x80).i32_eq();
            i.else_();
            i.i32_const(0);
            i.end();
            i.if_(BlockType::Empty);
            i.i32_const(1).local_set(hbad);
            i.local_get(hlen).i32_const(1).i32_add().local_set(hpos);
            i.else_();
        }
        self.emit_scan_nl(hraw, hlen, hq);
        {
            let mut i = self.f.instructions();
            i.local_get(hq).i32_const(1).i32_add().local_set(hpos);
            i.local_get(hq).local_get(hlen).i32_ge_u().if_(BlockType::Empty);
            i.local_get(hlen).i32_const(1).i32_add().local_set(hpos);
            i.end();
            i.end();
            i.end();
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hpos).local_get(he).i32_ge_u().br_if(1);
            i.local_get(hpos).local_get(hlen).i32_ge_u().br_if(1);
            i.local_get(hpos).local_set(hq);
        }
        self.emit_scan_nl(hraw, hlen, hq);
        {
            // The line ends at the `\n` (exclusive), minus one `\r` before
            // it; a last line with no `\n` keeps every byte.
            let mut i = self.f.instructions();
            i.local_get(hq).local_set(hle);
            i.local_get(hq).local_get(hlen).i32_lt_u();
            i.local_get(hq).local_get(hpos).i32_gt_u();
            i.i32_and();
            i.if_(BlockType::Empty);
            i.local_get(hraw).local_get(hq).i32_add().i32_const(1).i32_sub().i32_load8_u(byte_at());
            i.i32_const(13).i32_eq().if_(BlockType::Empty);
            i.local_get(hq).i32_const(1).i32_sub().local_set(hle);
            i.end();
            i.end();
            // A fresh String block of the line's bytes.
            i.local_get(hle).local_get(hpos).i32_sub().call(F_ALLOC).local_set(params[1]);
            i.local_get(params[1]).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hraw).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(hpos).i32_add();
            i.local_get(hle).local_get(hpos).i32_sub();
            i.memory_copy(0, 0);
        }
        let line = self.witness_line_open(cb, crate::fs::witness_walkers::WalkAcc::Carried(Some(params[0])));
        self.lower_fold_body(cb, body, acc_ty)?;
        // The list.fold discipline: the accumulator owns one credit on every
        // step — a borrowed body result takes its share, and the previous
        // accumulator is released before the rebind.
        self.rc_share_guard(body, acc_ty);
        if let Some(dec) = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty)) {
            self.f.instructions().local_get(params[0]).call(dec);
        }
        self.witness_fold_step(body, params[0], acc_ty);
        self.f.instructions().local_set(params[0]);
        // The line was this walk's fresh block; the body only borrowed it.
        let dec_str = self.dec_fn_of(STR);
        self.f.instructions().local_get(params[1]).call(dec_str);
        self.witness_line_close(line);
        {
            let mut i = self.f.instructions();
            i.local_get(hq).i32_const(1).i32_add().local_set(hpos);
            i.br(0).end().end();
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// When `hbad` is set: release the text block and put native's
    /// `<name>("<path>"): stream did not contain valid UTF-8` err into `herr`
    /// (the quoting is runtime/rs/src/fs.rs `fs_q`: a bare pair of quotes).
    fn emit_utf8_err(&mut self, hbad: u32, hraw: u32, hp: u32, herr: u32, name: &str) -> Result<(), EmitError> {
        let pre = format!("{name}(\"");
        let suf = format!("\"): {}", almide_base::fs_errno::INVALID_UTF8_TEXT);
        let (pre_at, suf_at) = (self.pool.intern(&pre), self.pool.intern(&suf));
        let (pre_len, suf_len) = (pre.len() as i32, suf.len() as i32);
        let pay = almide_layout::PAYLOAD as i32;
        let dec_str = self.dec_fn_of(STR);
        let hm = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hbad).if_(BlockType::Empty);
        i.local_get(hraw).call(dec_str);
        i.local_get(hp).i32_load(len_memarg()).i32_const(pre_len + suf_len).i32_add().call(F_ALLOC).local_set(hm);
        i.local_get(hm).i32_const(pay).i32_add();
        i.i32_const(pre_at as i32 + pay).i32_const(pre_len).memory_copy(0, 0);
        i.local_get(hm).i32_const(pay + pre_len).i32_add();
        i.local_get(hp).i32_const(pay).i32_add();
        i.local_get(hp).i32_load(len_memarg()).memory_copy(0, 0);
        i.local_get(hm).i32_const(pay + pre_len).i32_add().local_get(hp).i32_load(len_memarg()).i32_add();
        i.i32_const(suf_at as i32 + pay).i32_const(suf_len).memory_copy(0, 0);
        i.i32_const(16).call(F_ALLOC).local_tee(herr).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(herr).local_get(hm).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.end();
        self.release_i32();
        Ok(())
    }

    /// `fs.fold_lines_range(path, start, end, init, f)`.
    pub(crate) fn lower_fs_fold_lines_range(
        &mut self,
        p: &IrExpr,
        s: &IrExpr,
        e: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("fs-fold-range-acc:{}", ty_name(&init.ty)));
        };
        // Native evaluates every argument before the call opens the file.
        let hp = self.fs_range_read(p, OP_FOLD_LINES_RANGE)?;
        self.lower_arg(s, Some(INT), ArgMode::Borrow)?;
        let hs64 = self.hold_i64()?;
        self.f.instructions().local_set(hs64);
        self.lower_arg(e, Some(INT), ArgMode::Borrow)?;
        let he64 = self.hold_i64()?;
        self.f.instructions().local_set(he64);
        self.lower_arg(init, Some(acc_ty), ArgMode::Retain)?;
        self.f.instructions().local_set(params[0]);
        let (hraw, hlen, herr) = self.fs_range_call(hp, OP_FOLD_LINES_RANGE)?;
        let hs = self.hold_i32()?;
        let he = self.hold_i32()?;
        let hbad = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hbad);
        {
            let mut i = self.f.instructions();
            // start: <= 0 → 0; past the file → len + 1; else itself.
            i.local_get(hs64).i64_const(0).i64_le_s().if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
            i.local_get(hs64).local_get(hlen).i64_extend_i32_u().i64_gt_s().if_(BlockType::Result(ValType::I32));
            i.local_get(hlen).i32_const(1).i32_add();
            i.else_();
            i.local_get(hs64).i32_wrap_i64();
            i.end();
            i.end();
            i.local_set(hs);
            // end: clamped to [0, len].
            i.local_get(he64).i64_const(0).i64_le_s().if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
            i.local_get(he64).local_get(hlen).i64_extend_i32_u().i64_gt_s().if_(BlockType::Result(ValType::I32));
            i.local_get(hlen);
            i.else_();
            i.local_get(he64).i32_wrap_i64();
            i.end();
            i.end();
            i.local_set(he);
        }
        self.emit_range_walk(hraw, hlen, hs, he, hbad, &params, cb, body, acc_ty)?;
        self.emit_utf8_err(hbad, hraw, hp, herr, "fs.fold_lines_range")?;
        self.witness_walk_result(acc_ty);
        let acc_dec = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty));
        self.fs_range_result(hraw, herr, params[0], acc_ty, acc_dec)?;
        for _ in 0..7 {
            self.release_i32();
        }
        self.release_i64();
        self.release_i64();
        Ok(SliceTy::Result(self.types.intern(acc_ty), self.types.intern(STR)))
    }

    /// `err` passthrough (releasing the value `hv` held) or `ok(hv)`,
    /// releasing the text block on the ok path (on the err path the block
    /// IS the err payload).
    fn fs_range_result(
        &mut self,
        hraw: u32,
        herr: u32,
        hv: u32,
        ty: SliceTy,
        dec: Option<u32>,
    ) -> Result<(), EmitError> {
        let dec_str = self.dec_fn_of(STR);
        let hsr = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(herr).if_(BlockType::Result(ValType::I32));
            if let Some(d) = dec {
                i.local_get(hv).call(d);
            }
            i.local_get(herr);
            i.else_();
            i.local_get(hraw).call(dec_str);
            i.i32_const(16)
                .call(F_ALLOC)
                .local_tee(hsr)
                .i32_const(0)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
            i.local_get(hsr).local_get(hv);
        }
        self.store_ty_slot(ty, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hsr).end();
        self.release_i32();
        Ok(())
    }

    /// `fs.fold_lines_chunked(path, workers, init, f)`.
    pub(crate) fn lower_fs_fold_lines_chunked(
        &mut self,
        p: &IrExpr,
        w: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let Some(acc_ty) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("fs-fold-chunked-acc:{}", ty_name(&init.ty)));
        };
        let stride = acc_ty.slot_size() as i32;
        let hp = self.fs_range_read(p, OP_FOLD_LINES_CHUNKED)?;
        self.lower_arg(w, Some(INT), ArgMode::Borrow)?;
        let hw64 = self.hold_i64()?;
        self.f.instructions().local_set(hw64);
        let hinit = self.hold_for(acc_ty)?;
        self.lower_arg(init, Some(acc_ty), ArgMode::Retain)?;
        self.f.instructions().local_set(hinit);
        // Native's order: the size probe (`metadata`, named for the chunked
        // call), then — for a non-empty file only — the workers' reads, which
        // ARE fold_lines_range_impl and name `fs.fold_lines_range` (so a
        // directory path errs under the range name, a missing one under the
        // chunked name, and an empty file answers `[]` without a read).
        let hstat = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(OP_FOLD_LINES_CHUNKED);
            i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hp).i32_load(len_memarg());
            i.i32_const(0).i32_const(0);
            i.call(F_FS_CALL);
        }
        self.fs_result_i64()?;
        let stat_dec = self.dec_fn_of(SliceTy::Result(self.types.intern(INT), self.types.intern(STR)));
        {
            let mut i = self.f.instructions();
            i.local_set(hstat);
            i.local_get(hstat).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_eqz();
            i.local_get(hstat).i64_load(slot_memarg(almide_layout::SUM_FIELD)).i64_const(0).i64_ne();
            i.i32_and();
            i.if_(BlockType::Empty);
            i.local_get(hstat).call(stat_dec);
        }
        self.note_host_op(OP_FOLD_LINES_RANGE);
        let (hraw, hlen, herr) = self.fs_range_call(hp, OP_FOLD_LINES_RANGE)?;
        {
            let mut i = self.f.instructions();
            i.else_();
            i.i32_const(0).local_set(hlen);
            i.local_get(hstat).i32_load(slot_memarg(almide_layout::SUM_TAG)).if_(BlockType::Empty);
            // The probe's err IS the result (the text block stays unset).
            i.local_get(hstat).local_set(herr);
            i.i32_const(0).local_set(hraw);
            i.else_();
            i.local_get(hstat).call(stat_dec);
            i.i32_const(0).local_set(herr);
            i.i32_const(0).call(F_ALLOC).local_set(hraw);
            i.end();
            i.end();
        }
        let hchunk = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hs = self.hold_i32()?;
        let he = self.hold_i32()?;
        let hlist = self.hold_i32()?;
        let hbad = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hbad);
            // w = max(workers, 1); chunk = len / w + 1 (<= len + 1).
            i.local_get(hw64).i64_const(1).i64_lt_s().if_(BlockType::Empty);
            i.i64_const(1).local_set(hw64);
            i.end();
            i.local_get(hlen).i64_extend_i32_u().local_get(hw64).i64_div_s().i64_const(1).i64_add();
            i.i32_wrap_i64().local_set(hchunk);
            // n = min(w, ceil(len / chunk)) — the ranges whose start is in
            // the file (0 for an empty file).
            i.local_get(hlen).local_get(hchunk).i32_add().i32_const(1).i32_sub();
            i.local_get(hchunk).i32_div_u().local_set(hn);
            i.local_get(hw64).local_get(hn).i64_extend_i32_u().i64_lt_s().if_(BlockType::Empty);
            i.local_get(hw64).i32_wrap_i64().local_set(hn);
            i.end();
            i.local_get(hn).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hlist);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(hi).local_get(hchunk).i32_mul().local_set(hs);
            i.local_get(hs).local_get(hchunk).i32_add().local_set(he);
            i.local_get(he).local_get(hlen).i32_gt_u().if_(BlockType::Empty);
            i.local_get(hlen).local_set(he);
            i.end();
            // Each range folds from its own credit on `init`.
            i.local_get(hinit);
        }
        if self.elem_is_handle(acc_ty) {
            self.rc_inc_top();
        }
        self.f.instructions().local_set(params[0]);
        // One range per outer iteration: its accumulator leaves the line
        // walk owned and moves into the partials list's slot.
        self.witness_loop_open();
        self.emit_range_walk(hraw, hlen, hs, he, hbad, &params, cb, body, acc_ty)?;
        {
            let mut i = self.f.instructions();
            i.local_get(hlist).local_get(hi).i32_const(stride).i32_mul().i32_add();
            i.local_get(params[0]);
        }
        self.store_ty_slot(acc_ty, 0);
        self.witness_walk_stored(acc_ty);
        self.witness_loop_close();
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0).end().end();
        }
        if let Some(dec) = self.elem_is_handle(acc_ty).then(|| self.dec_fn_of(acc_ty)) {
            self.f.instructions().local_get(hinit).call(dec);
        }
        let el = self.types.intern(acc_ty);
        let list_ty = SliceTy::List(el);
        let list_dec = Some(self.dec_fn_of(list_ty));
        // Native's workers ARE fold_lines_range_impl, so a worker's failed
        // discard names `fs.fold_lines_range` (only the size probe before
        // the workers names the chunked call).
        self.emit_utf8_err(hbad, hraw, hp, herr, "fs.fold_lines_range")?;
        self.fs_range_result(hraw, herr, hlist, list_ty, list_dec)?;
        for _ in 0..12 {
            self.release_i32();
        }
        self.release_for(acc_ty);
        self.release_i64();
        Ok(SliceTy::Result(self.types.intern(list_ty), self.types.intern(STR)))
    }
}
