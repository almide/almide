//! Matrix products on the FLAT layout (#1423 stage 4): mul. Every dot
//! product is the
//! native scalar loop — accumulate from 0.0, k ascending, mul-then-add, no
//! fused step (runtime/rs/src/matrix.rs; the self-host bodies in
//! stdlib/matrix_core.almd / matrix_ext.almd spell the same order). The
//! output size is a PRODUCT of input extents, so every result goes through
//! the C-197 structural bound (`mat_alloc_guarded`).
//!
//! Outside the sane domain (operands whose inner extents disagree) these
//! kernels read only inside their operands — the inner extent is the smaller
//! of the two, a missing bias entry adds nothing — so a mismatch can never
//! read another block. It is still OUT OF DOMAIN: C-353 makes a shape
//! precondition a defined abort (`Error: matrix shape mismatch`, exit 1) on
//! every leg, so the truncated product this would otherwise answer — where
//! native panicked past its row — never reaches the program (#2481, #2483).

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::*;

impl Emitter<'_> {
    /// C-353: two extents a kernel indexes against each other must be EQUAL,
    /// or the call aborts in the unified T6 form — the same abort the index
    /// domain (C-282) takes. `applies` is a 0/1 i32 local: the EMPTY-operand
    /// short-circuit each kernel already has answers first (the empty matrix
    /// has no row to violate), so the guard only judges a call that will read.
    pub(crate) fn matrix_shape_eq(&mut self, applies: u32, a: u32, b: u32) {
        let msg = self.pool.intern("matrix shape mismatch");
        {
            let mut i = self.f.instructions();
            i.local_get(applies);
            i.local_get(a).local_get(b).i32_ne();
            i.i32_and().if_(BlockType::Empty);
            i.i32_const(msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
    }

    /// A flat r×c block (i32 extents) through the C-197 bound, zero-filled
    /// (`mat_alloc_out64`).
    pub(crate) fn mat_alloc_guarded(&mut self, hr: u32, hc: u32) -> Result<u32, EmitError> {
        let r64 = self.hold_i64()?;
        let c64 = self.hold_i64()?;
        {
            let mut i = self.f.instructions();
            i.local_get(hr).i64_extend_i32_u().local_set(r64);
            i.local_get(hc).i64_extend_i32_u().local_set(c64);
        }
        let ho = self.mat_alloc_out64(r64, c64)?;
        self.release_i64();
        self.release_i64();
        Ok(ho)
    }

    /// mul: out[i][j] = Σ_{k < min(a.cols, b.rows)} a[i][k]·b[k][j]; a.rows
    /// rows, b's width (0 for a rowless b) wide.
    pub(crate) fn lower_matrix_mul(&mut self, a: &IrExpr, b: &IrExpr) -> ArmResult {
        let (ha, ra, ca) = self.mat_open(a)?;
        let (hb, rb, cb) = self.mat_open(b)?;
        let hn = self.hold_i32()?;
        let hk = self.hold_i32()?;
        self.zero_cols_if_no_rows(rb, cb);
        self.f.instructions().local_get(cb).local_set(hn);
        self.zero_cols_if_no_rows(ra, hn);
        // C-353: the inner dimension is a PRECONDITION — `cols(a) == rows(b)`
        // — once the degenerate shapes are out (m, k or n zero answers the
        // m×n zero matrix, native's early return). Without it this summed
        // over min(cols(a), rows(b)) and printed a product of the overlap
        // where native indexed past `b`'s data (#2481).
        let happ = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(ra).i32_eqz().local_get(ca).i32_eqz().i32_or();
            i.local_get(hn).i32_eqz().i32_or().i32_eqz().local_set(happ);
        }
        self.matrix_shape_eq(happ, ca, rb);
        self.min_u32(hk, ca, rb);
        let ho = self.mat_alloc_guarded(ra, hn)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hkk = self.hold_i32()?;
        let hacc = self.hold_f64()?;
        self.loop_begin(hi, ra);
        self.loop_begin(hj, hn);
        self.f.instructions().f64_const(0.0f64.into()).local_set(hacc);
        self.loop_begin(hkk, hk);
        self.f.instructions().local_get(hacc);
        self.elem_at(ha, hi, ca, hkk);
        self.f.instructions().f64_load(mat_elem());
        self.elem_at(hb, hkk, cb, hj);
        self.f.instructions().f64_load(mat_elem()).f64_mul().f64_add().local_set(hacc);
        self.loop_end(hkk);
        self.elem_at(ho, hi, hn, hj);
        self.f.instructions().local_get(hacc).f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        self.release_f64();
        for _ in 0..13 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }
}

