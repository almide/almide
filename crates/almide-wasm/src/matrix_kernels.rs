//! Matrix compute kernels on the FLAT layout — transcribed from the
//! self-hosted stdlib bodies (matrix_arith/core), which are
//! the cross-target oracle: arithmetic op ORDER is copied exactly.

use almide_ir::IrExpr;
use wasm_encoder::MemArg;

use crate::emitter::Emitter;
use crate::*;

/// f64 element at handle + PAYLOAD + 8 + (computed byte offset).
pub(crate) fn mat_elem() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD) + 8, align: 2, memory_index: 0 }
}

impl Emitter<'_> {
    /// Lower a Matrix expr; returns (handle, rows i32, cols i32) holds.
    pub(crate) fn mat_open(&mut self, m: &IrExpr) -> Result<(u32, u32, u32), EmitError> {
        match self.lower_arg(m, None, ArgMode::Borrow)? {
            SliceTy::Matrix => {}
            other => return unsup(&format!("matrix-kernel-of:{other:?}")),
        }
        let hm = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hm);
        i.local_get(hm).i32_load(slot_memarg(0)).local_set(hr);
        i.local_get(hm).i32_load(slot_memarg(4)).local_set(hc);
        let _ = i;
        Ok((hm, hr, hc))
    }

    /// Alloc a flat matrix with header (rows, cols) from i32 locals.
    /// Leaves NOTHING on the stack; returns the handle hold.
    pub(crate) fn mat_alloc_out(&mut self, hr: u32, hc: u32) -> Result<u32, EmitError> {
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hr).local_get(hc).i32_mul().i32_const(8).i32_mul();
        i.i32_const(8).i32_add().call(F_ALLOC).local_set(ho);
        i.local_get(ho).local_get(hr).i32_store(slot_memarg(0));
        // rows = 0 stores cols = 0: a rowless matrix has no columns (the
        // `mat_alloc_out64` invariant, docs/stdlib/matrix.md).
        i.local_get(ho);
        i.i32_const(0).local_get(hc).local_get(hr).i32_eqz().select();
        i.i32_store(slot_memarg(4));
        let _ = i;
        Ok(ho)
    }

}
