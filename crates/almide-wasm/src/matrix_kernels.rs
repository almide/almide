//! Matrix compute kernels on the FLAT layout — transcribed from the
//! self-hosted stdlib bodies (matrix_arith/core), which are
//! the cross-target oracle: arithmetic op ORDER is copied exactly.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg};

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

    /// The linked vendored-libm function index for a `math.*` name.
    pub(crate) fn linked_math(&mut self, name: &str) -> Result<u32, EmitError> {
        let Some(fi) = self.resolve_qualified(name) else {
            return unsup(&format!("matrix-kernel-needs:{name}"));
        };
        let info = &self.table.infos[fi];
        if info.refuse.is_some() || info.ret != Some(FLOAT) {
            return unsup(&format!("matrix-kernel-impl:{name}"));
        }
        let idx = info.wasm_index;
        self.calls.insert(fi);
        Ok(idx)
    }

    /// Element-wise pow: every element through the linked math.fpow.
    pub(crate) fn lower_matrix_elementwise_pow(&mut self, m: &IrExpr, e: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        let he = self.hold_f64()?;
        self.lower_arg(e, Some(FLOAT), ArgMode::Borrow)?;
        self.f.instructions().local_set(he);
        let target = self.linked_math("math.fpow")?;
        let ho = self.mat_alloc_out(hr, hc)?;
        let hk = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(hk);
        i.local_get(hr).local_get(hc).i32_mul().i32_const(8).i32_mul().local_set(hend);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hend).i32_ge_u().br_if(1);
        i.local_get(ho).local_get(hk).i32_add();
        i.local_get(hm).local_get(hk).i32_add().f64_load(mat_elem());
        i.local_get(he);
        i.call(target);
        i.f64_store(mat_elem());
        i.local_get(hk).i32_const(8).i32_add().local_set(hk);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        self.release_f64();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }
}
