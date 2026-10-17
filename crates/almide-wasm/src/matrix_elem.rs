//! Matrix element-wise and row-wise kernels on the FLAT layout (#1423
//! stage 4): the zip family (add / sub / div), the unary family
//! (neg / scale / map) and broadcast_add_row. Transcribed op for op from native
//! runtime/rs/src/matrix.rs + matrix_p2.rs (the self-host bodies in
//! stdlib/matrix_core.almd / matrix_arith.almd / matrix_ext.almd agree on
//! the sane domain):
//!
//! - ZIP: rows and cols truncate to the shorter operand (`a.iter().zip(b)`
//!   twice); a 0-row result has 0 cols — native's `from_iter` reads the
//!   width off row 0, and this layout's header must say what `shape` says.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::*;

/// The zip-family element operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZipOp {
    Add,
    Sub,
    Div,
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

    /// add / sub / div — the zip family.
    pub(crate) fn lower_matrix_zip(&mut self, op: ZipOp, a: &IrExpr, b: &IrExpr) -> ArmResult {
        let (ha, ra, ca) = self.mat_open(a)?;
        let (hb, rb, cb) = self.mat_open(b)?;
        let hr = self.hold_i32()?;
        let hc = self.hold_i32()?;
        // `hsame` (and `hx` below) are no longer read by any zip op; they
        // stay so the emitted body is byte-identical to the pinned size
        // goldens — drop them together with a golden regen.
        let hsame = self.hold_i32()?;
        self.min_u32(hr, ra, rb);
        self.min_u32(hc, ca, cb);
        self.zero_cols_if_no_rows(hr, hc);
        {
            let mut i = self.f.instructions();
            i.local_get(ra).local_get(rb).i32_eq();
            i.local_get(ca).local_get(cb).i32_eq().i32_and().local_set(hsame);
        }
        let ho = self.mat_alloc_out(hr, hc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let _hx = self.hold_f64()?;
        self.loop_begin(hi, hr);
        self.loop_begin(hj, hc);
        self.elem_at(ho, hi, hc, hj);
        self.elem_at(ha, hi, ca, hj);
        self.f.instructions().f64_load(mat_elem());
        self.elem_at(hb, hi, cb, hj);
        {
            let mut i = self.f.instructions();
            i.f64_load(mat_elem());
            match op {
                ZipOp::Add => i.f64_add(),
                ZipOp::Sub => i.f64_sub(),
                ZipOp::Div => i.f64_div(),
            };
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
                // #2755: one activation per element, all scalars.
                if let Some(cb) = arg {
                    self.witness_callback_open(cb, None);
                }
                self.elem_at(ho, hi, hc, hj);
                self.lower(body, Some(FLOAT))?;
                self.witness_loop_close();
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

    /// broadcast_add_row (x + bias[j], cols = min(cols, len(bias))).
    pub(crate) fn lower_matrix_row_bias(&mut self, m: &IrExpr, bias: &IrExpr) -> ArmResult {
        let (hm, hr, hc) = self.mat_open(m)?;
        // Two f64 holds no longer read; kept so the emitted body stays
        // byte-identical to the pinned size goldens.
        let _hv = self.hold_f64()?;
        let _hx = self.hold_f64()?;
        let (hb, hblen) = self.float_list_open(bias)?;
        let hoc = self.hold_i32()?;
        self.min_u32(hoc, hc, hblen);
        self.zero_cols_if_no_rows(hr, hoc);
        let ho = self.mat_alloc_out(hr, hoc)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        self.loop_begin(hi, hr);
        self.loop_begin(hj, hoc);
        self.elem_at(ho, hi, hoc, hj);
        self.elem_at(hm, hi, hc, hj);
        self.f.instructions().f64_load(mat_elem());
        self.float_slot(hb, hj);
        self.f.instructions().f64_add();
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
}
