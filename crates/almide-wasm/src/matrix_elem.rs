//! Matrix element-wise and row-wise kernels on the FLAT layout (#1423
//! stage 4): the zip family (add / sub / div / silu_mul), the unary family
//! (neg / scale / map), broadcast_add_row, causal_mask_add and
//! layer_norm_rows. Transcribed op for op from native
//! runtime/rs/src/matrix.rs + matrix_p2.rs (the self-host bodies in
//! stdlib/matrix_core.almd / matrix_arith.almd / matrix_ext.almd agree on
//! the sane domain):
//!
//! - ZIP: rows and cols truncate to the shorter operand (`a.iter().zip(b)`
//!   twice); a 0-row result has 0 cols — native's `from_iter` reads the
//!   width off row 0, and this layout's header must say what `shape` says.
//! - The row reductions accumulate left to right from 0.0, mul-then-add,
//!   no fused step, no reassociation.
//! - silu_mul on equal shapes is native's almide-kernel path: the CANONICAL
//!   fast-exp (#1197, the `$fast_exp` helper every kernel here shares); a
//!   shape mismatch is native's ragged fallback, which calls libm `exp`.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::work::Helper;
use crate::*;

/// The zip-family element operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZipOp {
    Add,
    Sub,
    Div,
    SiluMul,
}

impl Emitter<'_> {
    /// `ctr = 0; while ctr < limit {` — the body follows; `loop_end` closes.
    pub(crate) fn loop_begin(&mut self, ctr: u32, limit: u32) {
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(ctr);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(ctr).local_get(limit).i32_ge_u().br_if(1);
    }

    /// `ctr += 1 }` — closes `loop_begin`.
    pub(crate) fn loop_end(&mut self, ctr: u32) {
        let mut i = self.f.instructions();
        i.local_get(ctr).i32_const(1).i32_add().local_set(ctr);
        i.br(0).end().end();
    }

    /// Push the address of element (row, col) of a flat block whose rows
    /// are `stride` elements wide — `h + (row*stride + col)*8`; load or
    /// store it through `mat_elem()`.
    pub(crate) fn elem_at(&mut self, h: u32, row: u32, stride: u32, col: u32) {
        let mut i = self.f.instructions();
        i.local_get(h).local_get(row).local_get(stride).i32_mul().local_get(col).i32_add();
        i.i32_const(3).i32_shl().i32_add();
    }

    /// Push `list[k]` of a List[Float] block (8-byte slots).
    pub(crate) fn float_slot(&mut self, list: u32, k: u32) {
        let mut i = self.f.instructions();
        i.local_get(list).local_get(k).i32_const(3).i32_shl().i32_add();
        i.f64_load(slot_memarg(0));
    }

    /// `dst = min(a, b)` over u32 locals.
    pub(crate) fn min_u32(&mut self, dst: u32, a: u32, b: u32) {
        let mut i = self.f.instructions();
        i.local_get(a).local_get(b).local_get(a).local_get(b).i32_lt_u().select().local_set(dst);
    }

    /// `cols = 0` when `rows == 0` (native `from_iter`: the width is row 0's).
    pub(crate) fn zero_cols_if_no_rows(&mut self, rows: u32, cols: u32) {
        let mut i = self.f.instructions();
        i.local_get(cols).i32_const(0).local_get(rows).select().local_set(cols);
    }

    /// Lower a List[Float] argument (borrowed); returns (handle, count) holds.
    pub(crate) fn float_list_open(&mut self, e: &IrExpr) -> Result<(u32, u32), EmitError> {
        match self.lower_arg(e, None, ArgMode::Borrow)? {
            SliceTy::List(h) if self.types.el(h) == FLOAT => {}
            other => return unsup(&format!("matrix-float-list:{other:?}")),
        }
        let hl = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hl);
        i.local_get(hl).i32_load(len_memarg()).i32_const(3).i32_shr_u().local_set(hn);
        Ok((hl, hn))
    }

    /// add / sub / div / silu_mul — the zip family.
    pub(crate) fn lower_matrix_zip(&mut self, op: ZipOp, a: &IrExpr, b: &IrExpr) -> ArmResult {
        let (ha, ra, ca) = self.mat_open(a)?;
        let (hb, rb, cb) = self.mat_open(b)?;
        let hr = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hsame = self.hold_i32()?;
        self.min_u32(hr, ra, rb);
        self.min_u32(hc, ca, cb);
        self.zero_cols_if_no_rows(hr, hc);
        let exps = if op == ZipOp::SiluMul {
            let fast = self.work.helper(Helper::FastExp);
            Some((fast, self.linked_math("math.exp")?))
        } else {
            None
        };
        {
            let mut i = self.f.instructions();
            i.local_get(ra).local_get(rb).i32_eq();
            i.local_get(ca).local_get(cb).i32_eq().i32_and().local_set(hsame);
        }
        let ho = self.mat_alloc_out(hr, hc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hx = self.hold_f64()?;
        self.loop_begin(hi, hr);
        self.loop_begin(hj, hc);
        self.elem_at(ho, hi, hc, hj);
        self.elem_at(ha, hi, ca, hj);
        self.f.instructions().f64_load(mat_elem());
        match exps {
            None => {
                self.elem_at(hb, hi, cb, hj);
                let mut i = self.f.instructions();
                i.f64_load(mat_elem());
                match op {
                    ZipOp::Add => i.f64_add(),
                    ZipOp::Sub => i.f64_sub(),
                    _ => i.f64_div(),
                };
            }
            Some((fast, libm)) => {
                // x * (1 / (1 + exp(-x))) * y, left to right (native's
                // silu_mul_naive and its ragged fallback alike).
                {
                    let mut i = self.f.instructions();
                    i.local_tee(hx);
                    i.f64_const(1.0f64.into()).f64_const(1.0f64.into());
                    i.local_get(hsame).if_(BlockType::Result(ValType::F64));
                    i.local_get(hx).f64_neg().call(fast);
                    i.else_().local_get(hx).f64_neg().call(libm).end();
                    i.f64_add().f64_div().f64_mul();
                }
                self.elem_at(hb, hi, cb, hj);
                self.f.instructions().f64_load(mat_elem()).f64_mul();
            }
        }
        self.f.instructions().f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        self.release_f64();
        // ha ra ca | hb rb cb | hr hc hsame | ho | hi hj
        for _ in 0..12 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// neg / scale / map — one input element, one output element, the
    /// shape unchanged. `map`'s callback is a lambda inlined per element
    /// (the list.map convention: `hof_lambda`).
    pub(crate) fn lower_matrix_unary(&mut self, func: &str, m: &IrExpr, arg: Option<&IrExpr>) -> ArmResult {
        let lambda = match (func, arg) {
            ("map", Some(cb)) => Some(self.hof_lambda(cb, 1)?),
            _ => None,
        };
        let (hm, hr, hc) = self.mat_open(m)?;
        let hs = self.hold_f64()?;
        if let (None, Some(s)) = (&lambda, arg) {
            self.lower_arg(s, Some(FLOAT), ArgMode::Borrow)?;
            self.f.instructions().local_set(hs);
        }
        let ho = self.mat_alloc_out(hr, hc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        self.loop_begin(hi, hr);
        self.loop_begin(hj, hc);
        match &lambda {
            Some((params, body)) => {
                self.elem_at(hm, hi, hc, hj);
                self.f.instructions().f64_load(mat_elem()).local_set(params[0]);
                self.elem_at(ho, hi, hc, hj);
                self.lower(body, Some(FLOAT))?;
            }
            None => {
                self.elem_at(ho, hi, hc, hj);
                self.elem_at(hm, hi, hc, hj);
                let mut i = self.f.instructions();
                i.f64_load(mat_elem());
                if func == "neg" {
                    // native `-x`: the sign flip, so neg(0.0) is -0.0
                    i.f64_neg();
                } else {
                    i.local_get(hs).f64_mul();
                }
            }
        }
        self.f.instructions().f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        for _ in 0..6 {
            self.release_i32();
        }
        self.release_f64();
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// broadcast_add_row (x + bias[j], cols = min(cols, len(bias))) and
    /// causal_mask_add (x + v strictly above the diagonal, else x).
    pub(crate) fn lower_matrix_row_bias(&mut self, func: &str, m: &IrExpr, arg: &IrExpr) -> ArmResult {
        let causal = func == "causal_mask_add";
        let (hm, hr, hc) = self.mat_open(m)?;
        let hv = self.hold_f64()?;
        let hx = self.hold_f64()?;
        let (hb, hblen) = if causal {
            self.lower_arg(arg, Some(FLOAT), ArgMode::Borrow)?;
            self.f.instructions().local_set(hv);
            (self.hold_i32()?, self.hold_i32()?)
        } else {
            self.float_list_open(arg)?
        };
        let hoc = self.hold_i32()?;
        if causal {
            self.f.instructions().local_get(hc).local_set(hoc);
        } else {
            self.min_u32(hoc, hc, hblen);
            self.zero_cols_if_no_rows(hr, hoc);
        }
        let ho = self.mat_alloc_out(hr, hoc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        self.loop_begin(hi, hr);
        self.loop_begin(hj, hoc);
        self.elem_at(ho, hi, hoc, hj);
        self.elem_at(hm, hi, hc, hj);
        self.f.instructions().f64_load(mat_elem());
        if causal {
            // strictly above the diagonal: x + v, else x (a select keeps x
            // in one place; the sum is computed either way, no side effect)
            let mut i = self.f.instructions();
            i.local_tee(hx).local_get(hv).f64_add();
            i.local_get(hx);
            i.local_get(hj).local_get(hi).i32_gt_u().select();
        } else {
            self.float_slot(hb, hj);
            self.f.instructions().f64_add();
        }
        self.f.instructions().f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        for _ in 0..9 {
            self.release_i32();
        }
        self.release_f64();
        self.release_f64();
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// layer_norm_rows: mean = Σx / c, var = Σ(x − mean)² / c over the FULL
    /// row, inv = 1 / √(var + eps); out[j] = (x − mean)·inv·g[j] + b[j] over
    /// min(cols, len(gamma), len(beta)) — native's double zip.
    pub(crate) fn lower_matrix_layer_norm(
        &mut self,
        m: &IrExpr,
        gamma: &IrExpr,
        beta: &IrExpr,
        eps: &IrExpr,
    ) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        let (hg, hglen) = self.float_list_open(gamma)?;
        let (hb, hblen) = self.float_list_open(beta)?;
        self.lower_arg(eps, Some(FLOAT), ArgMode::Borrow)?;
        let heps = self.hold_f64()?;
        self.f.instructions().local_set(heps);
        let hoc = self.hold_i32()?;
        self.min_u32(hoc, hc, hglen);
        self.min_u32(hoc, hoc, hblen);
        self.zero_cols_if_no_rows(hr, hoc);
        let ho = self.mat_alloc_out(hr, hoc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hmean = self.hold_f64()?;
        let hacc = self.hold_f64()?;
        self.loop_begin(hi, hr);
        // mean = (0.0 + x0 + x1 + …) / c
        self.f.instructions().f64_const(0.0f64.into()).local_set(hacc);
        self.loop_begin(hj, hc);
        self.f.instructions().local_get(hacc);
        self.elem_at(hm, hi, hc, hj);
        self.f.instructions().f64_load(mat_elem()).f64_add().local_set(hacc);
        self.loop_end(hj);
        {
            let mut i = self.f.instructions();
            i.local_get(hacc).local_get(hc).f64_convert_i32_u().f64_div().local_set(hmean);
            i.f64_const(0.0f64.into()).local_set(hacc);
        }
        // var = (0.0 + d0·d0 + d1·d1 + …) / c with d = x − mean; then
        // inv = 1 / √(var + eps) — native's `(var + eps).sqrt().recip()`.
        let hd = self.hold_f64()?;
        self.loop_begin(hj, hc);
        self.elem_at(hm, hi, hc, hj);
        {
            let mut i = self.f.instructions();
            i.f64_load(mat_elem()).local_get(hmean).f64_sub().local_set(hd);
            i.local_get(hacc).local_get(hd).local_get(hd).f64_mul().f64_add().local_set(hacc);
        }
        self.loop_end(hj);
        {
            let mut i = self.f.instructions();
            i.f64_const(1.0f64.into());
            i.local_get(hacc).local_get(hc).f64_convert_i32_u().f64_div();
            i.local_get(heps).f64_add().f64_sqrt().f64_div().local_set(hacc);
        }
        // out[j] = (x − mean)·inv·g[j] + b[j], left association exactly
        self.loop_begin(hj, hoc);
        self.elem_at(ho, hi, hoc, hj);
        self.elem_at(hm, hi, hc, hj);
        {
            let mut i = self.f.instructions();
            i.f64_load(mat_elem()).local_get(hmean).f64_sub().local_get(hacc).f64_mul();
        }
        self.float_slot(hg, hj);
        self.f.instructions().f64_mul();
        self.float_slot(hb, hj);
        self.f.instructions().f64_add().f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        for _ in 0..4 {
            self.release_f64();
        }
        // hm hr hc | hg hglen | hb hblen | hoc | ho | hi hj
        for _ in 0..11 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }
}
