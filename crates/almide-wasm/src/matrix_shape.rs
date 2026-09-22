//! Matrix row/column movers and byte encoders on the FLAT layout (#1423
//! stage 4): slice_rows, gather_rows, concat_cols / concat_cols_many,
//! split_cols_even, to_bytes_f64_le, to_bytes_f32_le. Each is native's
//! rule (runtime/rs/src/matrix.rs + matrix_p2.rs) read on a layout whose
//! rows are contiguous, so a row move is one `memory.copy`:
//!
//! - slice_rows / gather_rows read their indices as native's `as usize`
//!   does: a negative start or index is past every row (the empty slice,
//!   the all-zero gathered row), a negative end is "to the last row".
//! - concat_cols of the empty list is the 0-row matrix; a first matrix with
//!   no rows is ONE empty row (native's `vec![vec![]]`); each output row is
//!   the concatenation of the operands that HAVE that row (a shorter
//!   operand's missing rows zero-fill the tail — the flat reading of the
//!   ragged rows native would build, as from_lists reads them).
//! - split_cols_even: an empty matrix or a count below 1 is the empty list;
//!   otherwise `n` parts of width cols / n.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::bytes::BYTES;
use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::*;

/// A 4-byte element at a Bytes payload offset (unaligned-safe).
fn raw_f32() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// Push the address of the first cell of row `row` of a flat block
    /// `cols` wide (the `mat_elem` payload offset folded in).
    fn row_base(&mut self, h: u32, row: u32, cols: u32) {
        let mut i = self.f.instructions();
        i.local_get(h).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
        i.local_get(row).local_get(cols).i32_mul().i32_const(3).i32_shl().i32_add();
    }

    /// slice_rows(m, start, end): rows start..min(end, rows); a start at or
    /// past that end is the empty matrix.
    pub(crate) fn lower_matrix_slice_rows(&mut self, m: &IrExpr, start: &IrExpr, end: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        let mut se = [0u32; 2];
        for (slot, e) in se.iter_mut().zip([start, end]) {
            self.lower_arg(e, Some(INT), ArgMode::Borrow)?;
            *slot = self.hold_i64()?;
            self.f.instructions().local_set(*slot);
        }
        let [hs, he] = se;
        let hn = self.hold_i32()?;
        let hoc = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            // e = end < 0 || end > rows ? rows : end
            i.local_get(he).i64_const(0).i64_lt_s();
            i.local_get(he).local_get(hr).i64_extend_i32_u().i64_gt_s().i32_or();
            i.if_(BlockType::Result(ValType::I64));
            i.local_get(hr).i64_extend_i32_u();
            i.else_().local_get(he).end().local_set(he);
            // n = 0 <= start < e ? e - start : 0
            i.local_get(hs).i64_const(0).i64_ge_s();
            i.local_get(hs).local_get(he).i64_lt_s().i32_and();
            i.if_(BlockType::Result(ValType::I32));
            i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
            i.else_().i32_const(0).end().local_set(hn);
            i.local_get(hc).local_set(hoc);
        }
        self.zero_cols_if_no_rows(hn, hoc);
        let ho = self.mat_alloc_out(hn, hoc)?;
        let hstart = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(hn).if_(BlockType::Empty);
            i.local_get(hs).i32_wrap_i64().local_set(hstart);
        }
        self.f.instructions().local_get(ho).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
        self.row_base(hm, hstart, hc);
        {
            let mut i = self.f.instructions();
            i.local_get(hn).local_get(hc).i32_mul().i32_const(3).i32_shl();
            i.memory_copy(0, 0);
            i.end();
            i.local_get(ho);
        }
        // hm hr hc | hn hoc | ho | hstart
        for _ in 0..7 {
            self.release_i32();
        }
        self.release_i64();
        self.release_i64();
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// gather_rows(m, indices): the empty matrix for a rowless m; else one
    /// row per index — the indexed row, or the all-zero row out of range.
    pub(crate) fn lower_matrix_gather_rows(&mut self, m: &IrExpr, ids: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        match self.lower_arg(ids, None, ArgMode::Borrow)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("matrix-gather-ids:{other:?}")),
        }
        let hids = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hoc = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hids);
            i.local_get(hids).i32_load(len_memarg()).i32_const(3).i32_shr_u();
            i.i32_const(0).local_get(hr).select().local_set(hn);
            i.local_get(hc).local_set(hoc);
        }
        self.zero_cols_if_no_rows(hn, hoc);
        let ho = self.mat_alloc_guarded(hn, hoc)?;
        let hi = self.hold_i32()?;
        let hid = self.hold_i64()?;
        self.loop_begin(hi, hn);
        {
            let mut i = self.f.instructions();
            i.local_get(hids).local_get(hi).i32_const(3).i32_shl().i32_add();
            i.i64_load(slot_memarg(0)).local_tee(hid);
            // native `idx as usize < rows`: a negative index is out of range
            i.local_get(hr).i64_extend_i32_u().i64_lt_u().if_(BlockType::Empty);
        }
        self.row_base(ho, hi, hoc);
        {
            let mut i = self.f.instructions();
            i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
            i.local_get(hid).i32_wrap_i64().local_get(hc).i32_mul().i32_const(3).i32_shl().i32_add();
            i.local_get(hc).i32_const(3).i32_shl().memory_copy(0, 0);
            i.end();
        }
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        self.release_i64();
        // hm hr hc | hids hn hoc | ho | hi
        for _ in 0..8 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// Lower a List[Matrix] argument (borrowed): (handle, count) holds.
    fn matrix_list_open(&mut self, e: &IrExpr) -> Result<(u32, u32), EmitError> {
        match self.lower_arg(e, None, ArgMode::Borrow)? {
            SliceTy::List(h) if self.types.el(h) == SliceTy::Matrix => {}
            other => return unsup(&format!("matrix-list:{other:?}")),
        }
        let hl = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hl);
        i.local_get(hl).i32_load(len_memarg()).i32_const(2).i32_shr_u().local_set(hk);
        Ok((hl, hk))
    }

    /// concat_cols / concat_cols_many (one intrinsic natively).
    pub(crate) fn lower_matrix_concat_cols(&mut self, ms: &IrExpr) -> ArmResult {
        let (hl, hk) = self.matrix_list_open(ms)?;
        let hrows0 = self.hold_i32()?;
        let hrows = self.hold_i32()?;
        let hcols = self.hold_i32()?;
        let hmi = self.hold_i32()?;
        let hsub = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            // rows0 = k == 0 ? 0 : rows(ms[0]); rows = k == 0 ? 0 : max(rows0, 1)
            i.i32_const(0).local_set(hrows0);
            i.local_get(hk).if_(BlockType::Empty);
            i.local_get(hl).i32_load(slot_memarg(0)).i32_load(slot_memarg(0)).local_set(hrows0);
            i.end();
            i.local_get(hrows0).i32_const(1).local_get(hrows0).select();
            i.i32_const(0).local_get(hk).select().local_set(hrows);
            i.i32_const(0).local_set(hcols);
        }
        // cols = Σ cols(m) over the operands that have a row
        self.loop_begin(hmi, hk);
        {
            let mut i = self.f.instructions();
            i.local_get(hl).local_get(hmi).i32_const(2).i32_shl().i32_add();
            i.i32_load(slot_memarg(0)).local_tee(hsub).i32_load(slot_memarg(0));
            i.if_(BlockType::Empty);
            i.local_get(hcols).local_get(hsub).i32_load(slot_memarg(4)).i32_add().local_set(hcols);
            i.end();
        }
        self.loop_end(hmi);
        self.f.instructions().local_get(hrows0).i32_eqz().if_(BlockType::Empty).i32_const(0).local_set(hcols).end();
        let ho = self.mat_alloc_guarded(hrows, hcols)?;
        let hr = self.hold_i32()?;
        let hpos = self.hold_i32()?;
        self.loop_begin(hr, hrows0);
        self.f.instructions().i32_const(0).local_set(hpos);
        self.loop_begin(hmi, hk);
        {
            let mut i = self.f.instructions();
            i.local_get(hl).local_get(hmi).i32_const(2).i32_shl().i32_add();
            i.i32_load(slot_memarg(0)).local_set(hsub);
            i.local_get(hr).local_get(hsub).i32_load(slot_memarg(0)).i32_lt_u().if_(BlockType::Empty);
            // out[r, pos..] <- sub[r, ..]
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
            i.local_get(hr).local_get(hcols).i32_mul().local_get(hpos).i32_add();
            i.i32_const(3).i32_shl().i32_add();
            i.local_get(hsub).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
            i.local_get(hr).local_get(hsub).i32_load(slot_memarg(4)).i32_mul();
            i.i32_const(3).i32_shl().i32_add();
            i.local_get(hsub).i32_load(slot_memarg(4)).i32_const(3).i32_shl();
            i.memory_copy(0, 0);
            i.local_get(hpos).local_get(hsub).i32_load(slot_memarg(4)).i32_add().local_set(hpos);
            i.end();
        }
        self.loop_end(hmi);
        self.loop_end(hr);
        self.f.instructions().local_get(ho);
        // hl hk | hrows0 hrows hcols hmi hsub | ho | hr hpos
        for _ in 0..10 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// split_cols_even(m, n) → List[Matrix]: `n` parts of width cols / n
    /// (column block h = [h·w, h·w + w)), the empty list for a rowless m or
    /// a count below 1.
    pub(crate) fn lower_matrix_split_cols(&mut self, m: &IrExpr, n: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        self.lower_arg(n, Some(INT), ArgMode::Borrow)?;
        let hn = self.hold_i64()?;
        let oom = self.pool.intern("Error: out of memory");
        let hparts = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hl = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hn);
            // parts = rows == 0 || n < 1 ? 0 : n, judged in i64 before the wrap
            i.i64_const(0).local_get(hn);
            i.local_get(hr).i32_eqz().local_get(hn).i64_const(1).i64_lt_s().i32_or().select();
            i.local_tee(hn).i64_const(0x7FFF_0000 / 4).i64_gt_s().if_(BlockType::Empty);
            i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
            i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
            i.end();
            i.local_get(hn).i32_wrap_i64().local_set(hparts);
            // w = parts == 0 ? 0 : cols / parts
            i.local_get(hparts).if_(BlockType::Result(ValType::I32));
            i.local_get(hc).local_get(hparts).i32_div_u();
            i.else_().i32_const(0).end().local_set(hw);
            i.local_get(hparts).i32_const(2).i32_shl().call(F_ALLOC).local_set(hl);
        }
        let hh = self.hold_i32()?;
        let hi = self.hold_i32()?;
        self.loop_begin(hh, hparts);
        let hsub = self.mat_alloc_out(hr, hw)?;
        self.loop_begin(hi, hr);
        self.row_base(hsub, hi, hw);
        self.row_base(hm, hi, hc);
        {
            let mut i = self.f.instructions();
            i.local_get(hh).local_get(hw).i32_mul().i32_const(3).i32_shl().i32_add();
            i.local_get(hw).i32_const(3).i32_shl().memory_copy(0, 0);
        }
        self.loop_end(hi);
        {
            let mut i = self.f.instructions();
            i.local_get(hl).local_get(hh).i32_const(2).i32_shl().i32_add();
            i.local_get(hsub).i32_store(slot_memarg(0));
        }
        self.release_i32();
        self.loop_end(hh);
        self.f.instructions().local_get(hl);
        // hm hr hc | hparts hw hl | hh hi
        for _ in 0..8 {
            self.release_i32();
        }
        self.release_i64();
        let el = self.types.intern(SliceTy::Matrix);
        Ok(Some(Lowered::owned(SliceTy::List(el))))
    }

    /// to_bytes_f64_le / to_bytes_f32_le: the cells row-major, little
    /// endian (wasm memory IS little endian, so an f64 cell's bytes are its
    /// encoding; the f32 form rounds each cell to nearest, native `as f32`).
    pub(crate) fn lower_matrix_to_bytes(&mut self, f32: bool, m: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        let hn = self.hold_i32()?;
        let hb = self.hold_i32()?;
        let width = if f32 { 4 } else { 8 };
        {
            let mut i = self.f.instructions();
            i.local_get(hr).local_get(hc).i32_mul().local_set(hn);
            i.local_get(hn).i32_const(width).i32_mul().call(F_ALLOC).local_set(hb);
        }
        if f32 {
            let hk = self.hold_i32()?;
            self.loop_begin(hk, hn);
            {
                let mut i = self.f.instructions();
                i.local_get(hb).local_get(hk).i32_const(2).i32_shl().i32_add();
                i.local_get(hm).local_get(hk).i32_const(3).i32_shl().i32_add();
                i.f64_load(mat_elem()).f32_demote_f64().f32_store(raw_f32());
            }
            self.loop_end(hk);
            self.release_i32();
        } else {
            let mut i = self.f.instructions();
            i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
            i.local_get(hn).i32_const(3).i32_shl().memory_copy(0, 0);
        }
        self.f.instructions().local_get(hb);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(BYTES)))
    }
}
