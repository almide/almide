//! Matrix byte-loaders and row selectors on the FLAT layout — f32/f16/f64
//! full loaders and the f32/plain row-subset selectors. Transcribed from
//! the matrix_core self-host: negative dims/offsets clamp (#1503), OOB
//! rows are the defined all-zero edge (C-229 family), and the byte
//! loaders carry the 2^28 dims guard (#1525).

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
    /// Four-arg loader family, split by name.
    pub(crate) fn lower_matrix_loader(
        &mut self,
        func: &str,
        a: &IrExpr,
        b: &IrExpr,
        c: &IrExpr,
        d: &IrExpr,
    ) -> ArmResult {
        match func {
            "from_bytes_f32_le" => self.lower_matrix_from_bytes(4, a, b, c, d),
            "from_bytes_f16_le" => self.lower_matrix_from_bytes(2, a, b, c, d),
            _ => self.lower_matrix_from_bytes(8, a, b, c, d),
        }
    }

    /// Clamp an i64 hold to `max(v, 0)` in place.
    fn clamp0(&mut self, h: u32) {
        let mut i = self.f.instructions();
        i.i64_const(0).local_get(h).local_get(h).i64_const(0).i64_lt_s().select().local_set(h);
    }

    /// The byte-loader constructor ceiling (#1525): rows > 2^28 dies, then
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
    pub(crate) fn mat_alloc_out64(&mut self, hr: u32, hc: u32) -> Result<u32, EmitError> {
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
        // A rowless matrix has NO columns (docs/stdlib/matrix.md: `shape` is
        // (0, 0) with no rows): the header's cols is 0 whenever rows is 0,
        // the invariant `zeros`/`ones` and native's first-row-derived width
        // already keep. Storing the requested width here made
        // `from_bytes_f32_le(b, 0, 0, 3)` / `select_rows_f32(b, 0, 3, [])`
        // answer (0, 3) on wasm against native's (0, 0) (differential fuzz,
        // seed 576831800136 index 11987). Only the header changes — with
        // rows = 0 there are no cells, so the caller's own width is inert.
        i.local_get(ho);
        i.i64_const(0).local_get(hc).local_get(hr).i64_eqz().select();
        i.i32_wrap_i64().i32_store(slot_memarg(4));
        // The cells start ZERO by contract (the OOB→zeros edge of the byte
        // loaders fills nothing): `$alloc` hands back reused blocks
        // unzeroed, so the constructor zeroes — the `bytes.zeros` lesson
        // (#2004), met again the moment list spines were freed (#2010).
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
        i.i32_const(0);
        i.local_get(hr).local_get(hc).i64_mul().i64_const(8).i64_mul().i32_wrap_i64();
        i.memory_fill(0);
        let _ = i;
        Ok(ho)
    }

    /// from_bytes_f32_le / from_bytes_f16_le / from_bytes_f64_le (`width`
    /// = the element's byte count) answer `Option[Matrix]` (C-341):
    /// negative dims clamp to 0 and the C-161 ceiling aborts first; then a
    /// negative offset, an offset past the end or a short buffer is `none`
    /// (NULL_ADDR), anything else `some` of the decoded matrix.
    pub(crate) fn lower_matrix_from_bytes(
        &mut self,
        width: i64,
        data: &IrExpr,
        offset: &IrExpr,
        rows: &IrExpr,
        cols: &IrExpr,
    ) -> ArmResult {
        self.lower_arg(data, Some(SliceTy::Scalar(Scalar::Bytes)), ArgMode::Borrow)?;
        let hd = self.hold_i32()?;
        self.f.instructions().local_set(hd);
        self.lower_arg(offset, Some(INT), ArgMode::Borrow)?;
        let hoff = self.hold_i64()?;
        self.f.instructions().local_set(hoff);
        self.lower_arg(rows, Some(INT), ArgMode::Borrow)?;
        let hr = self.hold_i64()?;
        self.f.instructions().local_set(hr);
        self.lower_arg(cols, Some(INT), ArgMode::Borrow)?;
        let hc = self.hold_i64()?;
        self.f.instructions().local_set(hc);
        self.clamp0(hr);
        self.clamp0(hc);
        // The shared element ceiling BEFORE any product is formed: with
        // `cols = i64::MAX - 1` the `r*c*8` in `mat_alloc_out64` WRAPPED
        // negative, passed its bound, allocated a tiny block and the fill
        // loop trapped out of bounds where native took the C-161 abort
        // (`Error: matrix dimensions too large` — differential fuzz, seed
        // 544202078563 index 874). The division-form guard cannot wrap.
        self.q_dims_guard(hr, hc);
        {
            let mut i = self.f.instructions();
            // fits? offset >= 0 && offset <= len && r*c*width <= len - offset
            // — subtraction on the buffer side, never `offset + r*c*width`:
            // an offset near i64::MAX wrapped that sum negative, passed the
            // test and read a subnormal out of the wrong bytes (#1909). r and
            // c are clamped >= 0 and `q_dims_guard` has already refused a
            // count past the element ceiling, so `r*c*width` cannot wrap.
            i.local_get(hoff).i64_const(0).i64_ge_s();
            i.local_get(hoff);
            i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
            i.i64_le_s().i32_and();
            i.local_get(hr).local_get(hc).i64_mul().i64_const(width).i64_mul();
            i.local_get(hd).i32_load(len_memarg()).i64_extend_i32_u();
            i.local_get(hoff).i64_sub();
            i.i64_le_s().i32_and();
            i.if_(BlockType::Result(ValType::I32));
        }
        let ho = self.mat_alloc_out64(hr, hc)?;
        let hk = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hbox = self.hold_i32()?;
        let mut i = self.f.instructions();
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
        match width {
            2 => i.i32_load16_u(raw16()).call(F_F16_TO_F64),
            4 => i.i32_load(raw32()).f32_reinterpret_i32().f64_promote_f32(),
            // the 8 raw bytes ARE the f64 (wasm memory is little endian)
            _ => i.i64_load(raw32()).f64_reinterpret_i64(),
        };
        i.f64_store(mat_elem());
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        // `some(m)`: a one-slot box owning the fresh matrix.
        i.i32_const(SliceTy::Matrix.slot_size() as i32).call(F_ALLOC).local_tee(hbox);
        i.local_get(ho);
        let _ = i;
        self.store_ty_slot(SliceTy::Matrix, almide_layout::OPTION_FIELD);
        {
            let mut i = self.f.instructions();
            i.local_get(hbox);
            i.else_();
            i.i32_const(almide_layout::NULL_ADDR as i32);
            i.end();
        }
        for _ in 0..4 {
            self.release_i32();
        }
        for _ in 0..3 {
            self.release_i64();
        }
        self.release_i32();
        Ok(Some(Lowered::owned(SliceTy::Option(self.types.intern(SliceTy::Matrix)))))
    }

}
