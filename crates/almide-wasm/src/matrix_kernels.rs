//! Matrix compute kernels on the FLAT layout — transcribed from the
//! self-hosted stdlib bodies (matrix_activations/arith/core), which are
//! the cross-target oracle: arithmetic op ORDER is copied exactly
//! (fast-exp #1197, reciprocal-multiply softmax, halve-first gelu).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::work::Helper;
use crate::*;

/// f64 element at handle + PAYLOAD + 8 + (computed byte offset).
pub(crate) fn mat_elem() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD) + 8, align: 2, memory_index: 0 }
}

impl Emitter<'_> {
    /// Lower a Matrix expr; returns (handle, rows i32, cols i32) holds.
    pub(crate) fn mat_open(&mut self, m: &IrExpr) -> Result<(u32, u32, u32), EmitError> {
        match self.lower(m, None)? {
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
        i.local_get(ho).local_get(hc).i32_store(slot_memarg(4));
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

    /// Element-wise kernels: gelu (helper) and pow (linked math.fpow).
    pub(crate) fn lower_matrix_elementwise(
        &mut self,
        func: &str,
        m: &IrExpr,
        e_arg: Option<&IrExpr>,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (hm, hr, hc) = self.mat_open(m)?;
        let he = self.hold_f64()?;
        if let Some(e) = e_arg {
            self.lower(e, Some(FLOAT))?;
            self.f.instructions().local_set(he);
        }
        let target = if func == "gelu" {
            let fe = self.work.helper(Helper::FastExp);
            self.work.helper(Helper::GeluScalar { fast_exp: fe })
        } else {
            self.linked_math("math.fpow")?
        };
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
        if e_arg.is_some() {
            i.local_get(he);
        }
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
        Ok(Some(SliceTy::Matrix))
    }

    /// softmax_rows: per row — max scan (init row[0], `>` keeps NaN out),
    /// fast-exp(x − max) into out, LEFT-TO-RIGHT sum, then RECIPROCAL
    /// MULTIPLY (#1197); a bad sum (≤0 or NaN) yields the uniform 1/n row.
    /// A zero-width matrix loops over nothing per row — no special case.
    pub(crate) fn lower_matrix_softmax(&mut self, m: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let fe = self.work.helper(Helper::FastExp);
        let (hm, hr, hc) = self.mat_open(m)?;
        let ho = self.mat_alloc_out(hr, hc)?;
        let hp = self.hold_i32()?; // row index
        let hj = self.hold_i32()?; // col byte cursor
        let hrow = self.hold_i32()?; // row byte base (shared src/dst)
        let hwidth = self.hold_i32()?; // row bytes
        let hmx = self.hold_f64()?;
        let hs = self.hold_f64()?;
        let he = self.hold_f64()?;
        let mut i = self.f.instructions();
        i.local_get(hc).i32_const(8).i32_mul().local_set(hwidth);
        i.i32_const(0).local_set(hp);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hp).local_get(hr).i32_ge_u().br_if(1);
        i.local_get(hp).local_get(hwidth).i32_mul().local_set(hrow);
        i.local_get(hc).i32_const(0).i32_ne().if_(BlockType::Empty);
        // mx = row[0]; scan from 1 with `>` (select: v then mx, cond v>mx)
        i.local_get(hm).local_get(hrow).i32_add().f64_load(mat_elem()).local_set(hmx);
        i.i32_const(8).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hwidth).i32_ge_u().br_if(1);
        i.local_get(hm).local_get(hrow).i32_add().local_get(hj).i32_add().f64_load(mat_elem());
        i.local_set(he);
        i.local_get(he).local_get(hmx);
        i.local_get(he).local_get(hmx).f64_gt();
        i.select().local_set(hmx);
        i.local_get(hj).i32_const(8).i32_add().local_set(hj);
        i.br(0).end().end();
        // out[j] = fast_exp(row[j] - mx); s = left-to-right sum
        i.f64_const(0.0f64.into()).local_set(hs);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hwidth).i32_ge_u().br_if(1);
        i.local_get(hm).local_get(hrow).i32_add().local_get(hj).i32_add().f64_load(mat_elem());
        i.local_get(hmx).f64_sub().call(fe).local_set(he);
        i.local_get(ho).local_get(hrow).i32_add().local_get(hj).i32_add();
        i.local_get(he).f64_store(mat_elem());
        i.local_get(hs).local_get(he).f64_add().local_set(hs);
        i.local_get(hj).i32_const(8).i32_add().local_set(hj);
        i.br(0).end().end();
        // bad sum → uniform 1/n; else reciprocal multiply
        i.local_get(hs).f64_const(0.0f64.into()).f64_le();
        i.local_get(hs).local_get(hs).f64_ne();
        i.i32_or().if_(BlockType::Empty);
        i.f64_const(1.0f64.into()).local_get(hc).f64_convert_i32_s().f64_div().local_set(he);
        i.else_();
        i.f64_const(1.0f64.into()).local_get(hs).f64_div().local_set(he);
        i.end();
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hwidth).i32_ge_u().br_if(1);
        i.local_get(ho).local_get(hrow).i32_add().local_get(hj).i32_add();
        i.local_get(hs).f64_const(0.0f64.into()).f64_le();
        i.local_get(hs).local_get(hs).f64_ne();
        i.i32_or().if_(BlockType::Result(ValType::F64));
        i.local_get(he);
        i.else_();
        i.local_get(ho).local_get(hrow).i32_add().local_get(hj).i32_add().f64_load(mat_elem());
        i.local_get(he).f64_mul();
        i.end();
        i.f64_store(mat_elem());
        i.local_get(hj).i32_const(8).i32_add().local_set(hj);
        i.br(0).end().end();
        i.end();
        i.local_get(hp).i32_const(1).i32_add().local_set(hp);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..3 {
            self.release_f64();
        }
        for _ in 0..8 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }

    /// rms_norm_rows: inv = 1/√(Σx²/c + eps) over the FULL row; the
    /// output row truncates to min(cols, len(gamma)) like the zip, and
    /// out[j] = (x[j]·inv)·g[j] — left association exactly.
    pub(crate) fn lower_matrix_rms_norm(
        &mut self,
        m: &IrExpr,
        gamma: &IrExpr,
        eps: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (hm, hr, hc) = self.mat_open(m)?;
        match self.lower(gamma, None)? {
            SliceTy::List(h) if self.types.el(h) == FLOAT => {}
            other => return unsup(&format!("matrix-rms-gamma:{other:?}")),
        }
        let hg = self.hold_i32()?;
        self.f.instructions().local_set(hg);
        self.lower(eps, Some(FLOAT))?;
        let heps = self.hold_f64()?;
        let hout_c = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(heps);
            // outn = min(cols, glen)
            i.local_get(hc);
            i.local_get(hg).i32_load(len_memarg()).i32_const(8).i32_div_u();
            i.local_get(hc)
                .local_get(hg)
                .i32_load(len_memarg())
                .i32_const(8)
                .i32_div_u()
                .i32_lt_u();
            i.select().local_set(hout_c);
        }
        let ho = self.mat_alloc_out(hr, hout_c)?;
        let hp = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hsq = self.hold_f64()?;
        let hx = self.hold_f64()?;
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(hp);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hp).local_get(hr).i32_ge_u().br_if(1);
        // sq over the FULL source row
        i.f64_const(0.0f64.into()).local_set(hsq);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hc).i32_ge_u().br_if(1);
        i.local_get(hm);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj).i32_add().i32_const(8).i32_mul();
        i.i32_add().f64_load(mat_elem()).local_set(hx);
        i.local_get(hsq).local_get(hx).local_get(hx).f64_mul().f64_add().local_set(hsq);
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        // inv = 1 / sqrt(sq/c + eps) — reuse hsq as inv
        i.f64_const(1.0f64.into());
        i.local_get(hsq).local_get(hc).f64_convert_i32_s().f64_div();
        i.local_get(heps).f64_add().f64_sqrt();
        i.f64_div().local_set(hsq);
        // out[j] = (x[j] * inv) * g[j] for j < outn
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hout_c).i32_ge_u().br_if(1);
        i.local_get(ho);
        i.local_get(hp)
            .local_get(hout_c)
            .i32_mul()
            .local_get(hj)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add();
        i.local_get(hm);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj).i32_add().i32_const(8).i32_mul();
        i.i32_add().f64_load(mat_elem());
        i.local_get(hsq).f64_mul();
        i.local_get(hg).local_get(hj).i32_const(8).i32_mul().i32_add().f64_load(slot_memarg(0));
        i.f64_mul();
        i.f64_store(mat_elem());
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.local_get(hp).i32_const(1).i32_add().local_set(hp);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        self.release_f64();
        self.release_f64();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_f64();
        self.release_i32();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }
}
