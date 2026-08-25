//! Matrix byte-loaders and row selectors on the FLAT layout — f32/f16
//! full loaders, the f32/plain/Q1_0/Q8_0 row-subset selectors, and the
//! Q1_0 full loader. Transcribed from matrix_core/matrix_activations
//! self-hosts: negative dims/offsets clamp (#1503), OOB rows are the
//! defined all-zero edge (C-229 family), the q family carries the 2^28
//! dims guard (#1525), and dequant zeros normalize -0.0 → +0.0.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::*;

const Q_MAX: i64 = 268_435_456;

fn raw16() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}
fn raw32() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// Four-arg loader/selector family, split by name.
    pub(crate) fn lower_matrix_loader(
        &mut self,
        func: &str,
        a: &IrExpr,
        b: &IrExpr,
        c: &IrExpr,
        d: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        match func {
            "from_bytes_f32_le" => self.lower_matrix_from_bytes(false, a, b, c, d),
            "from_bytes_f16_le" => self.lower_matrix_from_bytes(true, a, b, c, d),
            "select_rows_f32" => self.lower_matrix_select_f32(a, b, c, d),
            "from_q1_0_bytes" => self.lower_matrix_q1_0(false, a, b, c, d),
            "select_rows_q1_0" => self.lower_matrix_q1_0(true, a, b, c, d),
            _ => self.lower_matrix_q8_select(a, b, c, d),
        }
    }

    /// Clamp an i64 hold to `max(v, 0)` in place.
    fn clamp0(&mut self, h: u32) {
        let mut i = self.f.instructions();
        i.i64_const(0).local_get(h).local_get(h).i64_const(0).i64_lt_s().select().local_set(h);
    }

    /// The q-family constructor ceiling (#1525): rows > 2^28 dies, then
    /// rows > 0 and cols > 2^28/rows dies — the unified T6 abort.
    fn q_dims_guard(&mut self, hr: u32, hc: u32) {
        let msg = self.pool.intern("matrix dimensions too large");
        {
            let mut i = self.f.instructions();
            i.local_get(hr).i64_const(Q_MAX).i64_gt_s().if_(BlockType::Empty);
            i.i32_const(msg as i32);
        }
        self.emit_error_frame_abort();
        {
            let mut i = self.f.instructions();
            i.end();
            // `rows > 0 and cols > MAX/rows` SHORT-CIRCUITS in the
            // oracle — the division must not run for rows = 0.
            i.local_get(hr).i64_const(0).i64_gt_s().if_(BlockType::Result(ValType::I32));
            i.local_get(hc);
            i.i64_const(Q_MAX);
            i.local_get(hr);
            i.i64_div_s();
            i.i64_gt_s();
            i.else_();
            i.i32_const(0);
            i.end();
            i.if_(BlockType::Empty);
            i.i32_const(msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
    }

    /// Alloc a flat matrix from i64 dims with the structural OOM bound
    /// (C-197: past it the allocator's die fires, no chosen ceiling).
    fn mat_alloc_out64(&mut self, hr: u32, hc: u32) -> Result<u32, EmitError> {
        let ho = self.hold_i32()?;
        let oom = self.pool.intern("Error: out of memory");
        let mut i = self.f.instructions();
        i.local_get(hr).local_get(hc).i64_mul().i64_const(8).i64_mul().i64_const(8).i64_add();
        i.i64_const(0x7FFF_0000).i64_gt_s().if_(BlockType::Empty);
        i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
        i.local_get(hr).local_get(hc).i64_mul().i64_const(8).i64_mul().i64_const(8).i64_add();
        i.i32_wrap_i64().call(F_ALLOC).local_set(ho);
        i.local_get(ho).local_get(hr).i32_wrap_i64().i32_store(slot_memarg(0));
        i.local_get(ho).local_get(hc).i32_wrap_i64().i32_store(slot_memarg(4));
        let _ = i;
        Ok(ho)
    }

    /// from_bytes_f32_le / from_bytes_f16_le: negative dims clamp to the
    /// empty matrix; a negative offset or a short buffer is the all-zero
    /// matrix (the family's OOB→zeros edge).
    pub(crate) fn lower_matrix_from_bytes(
        &mut self,
        half: bool,
        data: &IrExpr,
        offset: &IrExpr,
        rows: &IrExpr,
        cols: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(data, Some(SliceTy::Scalar(Scalar::Bytes)))?;
        let hd = self.hold_i32()?;
        self.f.instructions().local_set(hd);
        self.lower(offset, Some(INT))?;
        let hoff = self.hold_i64()?;
        self.f.instructions().local_set(hoff);
        self.lower(rows, Some(INT))?;
        let hr = self.hold_i64()?;
        self.f.instructions().local_set(hr);
        self.lower(cols, Some(INT))?;
        let hc = self.hold_i64()?;
        self.f.instructions().local_set(hc);
        self.clamp0(hr);
        self.clamp0(hc);
        let ho = self.mat_alloc_out64(hr, hc)?;
        let hk = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let width: i64 = if half { 2 } else { 4 };
        let mut i = self.f.instructions();
        // in-bounds? offset >= 0 && offset + r*c*width <= len
        i.local_get(hoff).i64_const(0).i64_ge_s();
        i.local_get(hoff)
            .local_get(hr)
            .local_get(hc)
            .i64_mul()
            .i64_const(width)
            .i64_mul()
            .i64_add();
        i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_le_s().i32_and().if_(BlockType::Empty);
        i.i32_const(0).local_set(hk);
        i.local_get(hr).local_get(hc).i64_mul().i32_wrap_i64().local_set(hn);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(ho).local_get(hk).i32_const(8).i32_mul().i32_add();
        i.local_get(hd)
            .local_get(hoff)
            .i32_wrap_i64()
            .i32_add()
            .local_get(hk)
            .i32_const(width as i32)
            .i32_mul()
            .i32_add();
        if half {
            i.i32_load16_u(raw16()).call(F_F16_TO_F64);
        } else {
            i.i32_load(raw32()).f32_reinterpret_i32().f64_promote_f32();
        }
        i.f64_store(mat_elem());
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        i.end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        for _ in 0..3 {
            self.release_i64();
        }
        self.release_i32();
        Ok(Some(SliceTy::Matrix))
    }

    /// select_rows_f32: rid clamps to 0; a row whose f32 window leaves
    /// the buffer stays all-zero; offset/cols clamp (#1503).
    pub(crate) fn lower_matrix_select_f32(
        &mut self,
        data: &IrExpr,
        offset: &IrExpr,
        cols: &IrExpr,
        ids: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(data, Some(SliceTy::Scalar(Scalar::Bytes)))?;
        let hd = self.hold_i32()?;
        self.f.instructions().local_set(hd);
        self.lower(offset, Some(INT))?;
        let hoff = self.hold_i64()?;
        self.f.instructions().local_set(hoff);
        self.lower(cols, Some(INT))?;
        let hc = self.hold_i64()?;
        self.f.instructions().local_set(hc);
        match self.lower(ids, None)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("matrix-select-ids:{other:?}")),
        }
        let hids = self.hold_i32()?;
        self.f.instructions().local_set(hids);
        self.clamp0(hoff);
        self.clamp0(hc);
        let hn = self.hold_i64()?;
        self.f
            .instructions()
            .local_get(hids)
            .i32_load(len_memarg())
            .i32_const(8)
            .i32_div_u()
            .i64_extend_i32_u()
            .local_set(hn);
        let ho = self.mat_alloc_out64(hn, hc)?;
        let hi = self.hold_i32()?;
        let hrid = self.hold_i64()?;
        let hj = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hn).i32_wrap_i64().i32_ge_u().br_if(1);
        // rid = max(ids[i], 0)
        i.local_get(hids).local_get(hi).i32_const(8).i32_mul().i32_add();
        i.i64_load(slot_memarg(0)).local_set(hrid);
        let _ = i;
        self.clamp0(hrid);
        let mut i = self.f.instructions();
        // base = off + rid*c*4; fits? base + c*4 <= len
        i.local_get(hoff)
            .local_get(hrid)
            .local_get(hc)
            .i64_mul()
            .i64_const(4)
            .i64_mul()
            .i64_add()
            .local_set(hrid); // reuse: base
        i.local_get(hrid).local_get(hc).i64_const(4).i64_mul().i64_add();
        i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_le_s().if_(BlockType::Empty);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hc).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(ho);
        i.local_get(hi)
            .local_get(hc)
            .i32_wrap_i64()
            .i32_mul()
            .local_get(hj)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add();
        i.local_get(hd)
            .local_get(hrid)
            .i32_wrap_i64()
            .i32_add()
            .local_get(hj)
            .i32_const(4)
            .i32_mul()
            .i32_add();
        i.i32_load(raw32()).f32_reinterpret_i32().f64_promote_f32();
        i.f64_store(mat_elem());
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::Matrix))
    }

    /// from_q1_0_bytes / select_rows_q1_0: the Q1_0 decode on the
    /// global-k schedule via `$q10_val` (per-element bound, #1532); a
    /// row whose blocks leave the buffer stays all-zero; the #1525 dims
    /// guard fires before any alloc.
    pub(crate) fn lower_matrix_q1_0(
        &mut self,
        select: bool,
        data: &IrExpr,
        offset: &IrExpr,
        dim_a: &IrExpr,
        dim_b: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let qv = self.work.helper(crate::work::Helper::Q10Val);
        self.lower(data, Some(SliceTy::Scalar(Scalar::Bytes)))?;
        let hd = self.hold_i32()?;
        self.f.instructions().local_set(hd);
        self.lower(offset, Some(INT))?;
        let hoff = self.hold_i64()?;
        self.f.instructions().local_set(hoff);
        self.clamp0(hoff);
        // full loader: (rows, cols); selector: (cols, ids)
        let hr = self.hold_i64()?;
        let hc = self.hold_i64()?;
        let hids = self.hold_i32()?;
        if select {
            self.lower(dim_a, Some(INT))?;
            self.f.instructions().local_set(hc);
            self.clamp0(hc);
            match self.lower(dim_b, None)? {
                SliceTy::List(h) if self.types.el(h) == INT => {}
                other => return unsup(&format!("matrix-q1-ids:{other:?}")),
            }
            let mut i = self.f.instructions();
            i.local_set(hids);
            i.local_get(hids)
                .i32_load(len_memarg())
                .i32_const(8)
                .i32_div_u()
                .i64_extend_i32_u()
                .local_set(hr);
        } else {
            self.lower(dim_a, Some(INT))?;
            self.f.instructions().local_set(hr);
            self.lower(dim_b, Some(INT))?;
            self.f.instructions().local_set(hc);
            self.clamp0(hr);
            self.clamp0(hc);
        }
        self.q_dims_guard(hr, hc);
        let ho = self.mat_alloc_out64(hr, hc)?;
        let hi = self.hold_i32()?;
        let hrow = self.hold_i64()?; // rid (selector) / row index as i64
        let hj = self.hold_i32()?;
        let hrb = self.hold_i64()?; // row_bytes
        let mut i = self.f.instructions();
        i.local_get(hc).i64_const(128).i64_div_s().i64_const(18).i64_mul().local_set(hrb);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hr).i32_wrap_i64().i32_ge_u().br_if(1);
        if select {
            i.local_get(hids).local_get(hi).i32_const(8).i32_mul().i32_add();
            i.i64_load(slot_memarg(0)).local_set(hrow);
            let _ = i;
            self.clamp0(hrow);
            i = self.f.instructions();
        } else {
            i.local_get(hi).i64_extend_i32_u().local_set(hrow);
        }
        // in bounds? off + rid*row_bytes + row_bytes <= len
        i.local_get(hoff)
            .local_get(hrow)
            .local_get(hrb)
            .i64_mul()
            .i64_add()
            .local_get(hrb)
            .i64_add();
        i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_le_s().if_(BlockType::Empty);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hc).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(ho);
        i.local_get(hi)
            .local_get(hc)
            .i32_wrap_i64()
            .i32_mul()
            .local_get(hj)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add();
        // k = rid*cols + j (global schedule)
        i.local_get(hd).local_get(hoff);
        i.local_get(hrow).local_get(hc).i64_mul().local_get(hj).i64_extend_i32_u().i64_add();
        i.call(qv);
        i.f64_store(mat_elem());
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        self.release_i64();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::Matrix))
    }

    /// select_rows_q8_0_dq: 34-byte blocks of [fp16 scale][32 int8
    /// quants]; cols not divisible by 32 is the all-zero matrix; the
    /// dequant zero normalizes -0.0 → +0.0 per ELEMENT.
    pub(crate) fn lower_matrix_q8_select(
        &mut self,
        data: &IrExpr,
        offset: &IrExpr,
        cols: &IrExpr,
        ids: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(data, Some(SliceTy::Scalar(Scalar::Bytes)))?;
        let hd = self.hold_i32()?;
        self.f.instructions().local_set(hd);
        self.lower(offset, Some(INT))?;
        let hoff = self.hold_i64()?;
        self.f.instructions().local_set(hoff);
        self.clamp0(hoff);
        self.lower(cols, Some(INT))?;
        let hc = self.hold_i64()?;
        self.f.instructions().local_set(hc);
        self.clamp0(hc);
        match self.lower(ids, None)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("matrix-q8-ids:{other:?}")),
        }
        let hids = self.hold_i32()?;
        let hn = self.hold_i64()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hids);
            i.local_get(hids)
                .i32_load(len_memarg())
                .i32_const(8)
                .i32_div_u()
                .i64_extend_i32_u()
                .local_set(hn);
        }
        self.q_dims_guard(hn, hc);
        let ho = self.mat_alloc_out64(hn, hc)?;
        let hi = self.hold_i32()?;
        let hrow = self.hold_i64()?; // rid → row byte base
        let hj = self.hold_i32()?;
        let hrb = self.hold_i64()?;
        let hs = self.hold_f64()?;
        let mut i = self.f.instructions();
        // cols % 32 != 0 → all zeros (the fresh alloc IS the answer)
        i.local_get(hc).i64_const(32).i64_rem_s().i64_eqz().if_(BlockType::Empty);
        i.local_get(hc).i64_const(32).i64_div_s().i64_const(34).i64_mul().local_set(hrb);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hn).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(hids).local_get(hi).i32_const(8).i32_mul().i32_add();
        i.i64_load(slot_memarg(0)).local_set(hrow);
        let _ = i;
        self.clamp0(hrow);
        let mut i = self.f.instructions();
        // row_off = rid * row_bytes; fits? off + row_off + row_bytes <= len
        i.local_get(hrow).local_get(hrb).i64_mul().local_set(hrow);
        i.local_get(hoff).local_get(hrow).i64_add().local_get(hrb).i64_add();
        i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_le_s().if_(BlockType::Empty);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hc).i32_wrap_i64().i32_ge_u().br_if(1);
        // scale = f16(load16(data + off + row_off + (j>>5)*34))
        i.local_get(hd);
        i.local_get(hoff).local_get(hrow).i64_add().i32_wrap_i64().i32_add();
        i.local_get(hj).i32_const(5).i32_shr_u().i32_const(34).i32_mul().i32_add();
        i.i32_load16_u(raw16()).call(F_F16_TO_F64).local_set(hs);
        // v = scale * f64(q) with q = load8_s(... + 2 + (j&31))
        i.local_get(ho);
        i.local_get(hi)
            .local_get(hc)
            .i32_wrap_i64()
            .i32_mul()
            .local_get(hj)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add();
        i.local_get(hs);
        i.local_get(hd);
        i.local_get(hoff).local_get(hrow).i64_add().i32_wrap_i64().i32_add();
        i.local_get(hj).i32_const(5).i32_shr_u().i32_const(34).i32_mul().i32_add();
        i.local_get(hj).i32_const(31).i32_and().i32_add();
        i.i32_load8_s(MemArg {
            offset: u64::from(almide_layout::PAYLOAD) + 2,
            align: 0,
            memory_index: 0,
        });
        i.f64_convert_i32_s();
        i.f64_mul().local_set(hs);
        // dq_zero per element
        i.local_get(hs).f64_const(0.0f64.into()).f64_eq().if_(BlockType::Result(ValType::F64));
        i.f64_const(0.0f64.into());
        i.else_();
        i.local_get(hs);
        i.end();
        i.f64_store(mat_elem());
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.end();
        i.local_get(ho);
        let _ = i;
        self.release_f64();
        for _ in 0..5 {
            self.release_i64();
        }
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }

    /// select_rows (plain): rid clamps to 0; rid < rows copies the row,
    /// else the all-zero row.
    pub(crate) fn lower_matrix_select_rows(
        &mut self,
        m: &IrExpr,
        ids: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (hm, hr, hc) = self.mat_open(m)?;
        match self.lower(ids, None)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("matrix-select-ids:{other:?}")),
        }
        let hids = self.hold_i32()?;
        let hn = self.hold_i64()?;
        let hc64 = self.hold_i64()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hids);
            i.local_get(hids)
                .i32_load(len_memarg())
                .i32_const(8)
                .i32_div_u()
                .i64_extend_i32_u()
                .local_set(hn);
            i.local_get(hc).i64_extend_i32_u().local_set(hc64);
        }
        let ho = self.mat_alloc_out64(hn, hc64)?;
        let hi = self.hold_i32()?;
        let hrid = self.hold_i64()?;
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hn).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(hids).local_get(hi).i32_const(8).i32_mul().i32_add();
        i.i64_load(slot_memarg(0)).local_set(hrid);
        let _ = i;
        self.clamp0(hrid);
        let mut i = self.f.instructions();
        i.local_get(hrid).local_get(hr).i64_extend_i32_u().i64_lt_s().if_(BlockType::Empty);
        i.local_get(ho)
            .i32_const(almide_layout::PAYLOAD as i32 + 8)
            .i32_add()
            .local_get(hi)
            .local_get(hc)
            .i32_mul()
            .i32_const(8)
            .i32_mul()
            .i32_add();
        i.local_get(hm)
            .i32_const(almide_layout::PAYLOAD as i32 + 8)
            .i32_add()
            .local_get(hrid)
            .i32_wrap_i64()
            .local_get(hc)
            .i32_mul()
            .i32_const(8)
            .i32_mul()
            .i32_add();
        i.local_get(hc).i32_const(8).i32_mul();
        i.memory_copy(0, 0);
        i.end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }
}
