//! Matrix products on the FLAT layout (#1423 stage 4): mul, linear_row /
//! linear_row_no_bias, swiglu_gate and conv1d. Every dot product is the
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
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::*;

impl Emitter<'_> {
    /// C-353: two extents a kernel indexes against each other must be EQUAL,
    /// or the call aborts in the unified T6 form — the same abort the head
    /// count (C-198), the head geometry (C-278) and the index domain (C-282)
    /// take. `applies` is a 0/1 i32 local: the EMPTY-operand short-circuit
    /// each kernel already has answers first (C-278's "the empty matrix has
    /// no row to violate"), so the guard only judges a call that will read.
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

    /// `acc = 0.0; for k < n { acc = acc + A[ar, k]·B[br, k] }` with
    /// per-operand row strides — the shared row·row dot (linear, swiglu).
    fn row_row_dot(&mut self, acc: u32, a: (u32, u32, u32), b: (u32, u32, u32), k: u32, n: u32) {
        let ((ha, ar, astride), (hb, br, bstride)) = (a, b);
        self.f.instructions().f64_const(0.0f64.into()).local_set(acc);
        self.loop_begin(k, n);
        self.f.instructions().local_get(acc);
        self.elem_at(ha, ar, astride, k);
        self.f.instructions().f64_load(mat_elem());
        self.elem_at(hb, br, bstride, k);
        self.f.instructions().f64_load(mat_elem()).f64_mul().f64_add().local_set(acc);
        self.loop_end(k);
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

    /// linear_row / linear_row_no_bias: y[i][j] = Σ_k x[i][k]·w[j][k]
    /// (+ bias[j], added last); x.rows × w.rows, the empty matrix when
    /// either operand is empty.
    pub(crate) fn lower_matrix_linear(&mut self, x: &IrExpr, w: &IrExpr, bias: Option<&IrExpr>) -> ArmResult {
        let (hx, xr, xc) = self.mat_open(x)?;
        let (hw, wr, wc) = self.mat_open(w)?;
        let (hb, hblen) = match bias {
            Some(b) => self.float_list_open(b)?,
            None => (self.hold_i32()?, self.hold_i32()?),
        };
        let hrows = self.hold_i32()?;
        let hcols = self.hold_i32()?;
        let hnin = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(xr).i32_const(0).local_get(wr).select().local_set(hrows);
            i.local_get(wr).local_set(hcols);
        }
        self.zero_cols_if_no_rows(hrows, hcols);
        // C-353: the weight is (n_out, n_in) against x's width, and the bias
        // has one entry per output — checked after the empty short-circuit
        // native takes. The missing bias entries this skipped read as "no
        // bias" where native panicked on the index (#2483).
        let happ = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(xr).i32_eqz().local_get(wr).i32_eqz().i32_or().i32_eqz().local_set(happ);
        }
        self.matrix_shape_eq(happ, wc, xc);
        if bias.is_some() {
            self.matrix_shape_eq(happ, hblen, wr);
        }
        self.min_u32(hnin, xc, wc);
        let ho = self.mat_alloc_guarded(hrows, hcols)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hacc = self.hold_f64()?;
        self.loop_begin(hi, hrows);
        self.loop_begin(hj, hcols);
        self.row_row_dot(hacc, (hx, hi, xc), (hw, hj, wc), hk, hnin);
        if bias.is_some() {
            {
                let mut i = self.f.instructions();
                i.local_get(hj).local_get(hblen).i32_lt_u().if_(BlockType::Empty);
                i.local_get(hacc);
            }
            self.float_slot(hb, hj);
            let mut i = self.f.instructions();
            i.f64_add().local_set(hacc);
            i.end();
        }
        self.elem_at(ho, hi, hcols, hj);
        self.f.instructions().local_get(hacc).f64_store(mat_elem());
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        self.release_f64();
        for _ in 0..16 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// swiglu_gate: g = x[i]·w_gate[j], u = x[i]·w_up[j], out = g·σ(g)·u
    /// with σ(g) = 1 / (1 + exp(−g)) through the vendored libm exp —
    /// native's scalar loop (matrix.rs almide_rt_matrix_swiglu_gate).
    pub(crate) fn lower_matrix_swiglu(&mut self, x: &IrExpr, wg: &IrExpr, wu: &IrExpr) -> ArmResult {
        let exp = self.linked_math("math.exp")?;
        let (hx, xr, xc) = self.mat_open(x)?;
        let (hg, gr, gc) = self.mat_open(wg)?;
        let (hu, ur, uc) = self.mat_open(wu)?;
        let hrows = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hin = self.hold_i32()?;
        {
            // rows = 0 when any operand is empty (native's early return)
            let mut i = self.f.instructions();
            i.local_get(xr).i32_const(0);
            i.local_get(gr).i32_eqz().local_get(ur).i32_eqz().i32_or().i32_eqz();
            i.select().local_set(hrows);
        }
        self.min_u32(hout, gr, ur);
        self.zero_cols_if_no_rows(hrows, hout);
        // C-353: both weights are (d_out, d_in) against cols(x), and w_up
        // carries w_gate's row count — checked after the empty short-circuit.
        let happ = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(xr).i32_eqz().local_get(gr).i32_eqz().i32_or();
            i.local_get(ur).i32_eqz().i32_or().i32_eqz().local_set(happ);
        }
        self.matrix_shape_eq(happ, gc, xc);
        self.matrix_shape_eq(happ, uc, xc);
        self.matrix_shape_eq(happ, ur, gr);
        self.min_u32(hin, xc, gc);
        self.min_u32(hin, hin, uc);
        let ho = self.mat_alloc_guarded(hrows, hout)?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hgs = self.hold_f64()?;
        let hus = self.hold_f64()?;
        self.loop_begin(hi, hrows);
        self.loop_begin(hj, hout);
        self.row_row_dot(hgs, (hx, hi, xc), (hg, hj, gc), hk, hin);
        self.row_row_dot(hus, (hx, hi, xc), (hu, hj, uc), hk, hin);
        self.elem_at(ho, hi, hout, hj);
        {
            let mut i = self.f.instructions();
            // g * (1 / (1 + exp(-g))) * u, left to right
            i.local_get(hgs);
            i.f64_const(1.0f64.into()).f64_const(1.0f64.into());
            i.local_get(hgs).f64_neg().call(exp);
            i.f64_add().f64_div().f64_mul();
            i.local_get(hus).f64_mul().f64_store(mat_elem());
        }
        self.loop_end(hj);
        self.loop_end(hi);
        self.f.instructions().local_get(ho);
        self.release_f64();
        self.release_f64();
        for _ in 0..17 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// conv1d(input (T, in_ch), weight (out_ch, in_ch·K), bias, K, S, P) →
    /// (T_out, out_ch), T_out = (T + 2P − K)/S + 1; taps outside [0, T) of
    /// the zero-padded input contribute nothing; the sum starts at bias[o].
    /// An empty operand or T + 2P < K is the empty matrix.
    pub(crate) fn lower_matrix_conv1d(&mut self, args: &[IrExpr]) -> ArmResult {
        let [input, weight, bias, kernel, stride, padding] = args else {
            return unsup("matrix-conv1d-arity");
        };
        let (hin, tin, inch) = self.mat_open(input)?;
        let (hw, outch, wc) = self.mat_open(weight)?;
        let (hb, hblen) = self.float_list_open(bias)?;
        let mut sc = [0u32; 3];
        for (slot, e) in sc.iter_mut().zip([kernel, stride, padding]) {
            self.lower_arg(e, Some(INT), ArgMode::Borrow)?;
            *slot = self.hold_i64()?;
            self.f.instructions().local_set(*slot);
        }
        let [hk, hs, hp] = sc;
        // The count domain (C-354), in native's order: the STRIDE is a step
        // and below 1 aborts (0 was a wasm `integer divide by zero` trap here
        // against native's `Error: stride must be positive`), the kernel and
        // padding are WIDTHS and clamp at 0 like every C-161 dimension, and a
        // padding past the shared element ceiling aborts rather than wrapping
        // `T + 2P` into "no rows" while native computed a length. All of it
        // only when the call will read: an empty input or weight is the empty
        // matrix on every leg.
        let happ = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(tin).i32_eqz().local_get(outch).i32_eqz().i32_or().i32_eqz().local_set(happ);
        }
        let stride_msg = self.pool.intern("stride must be positive");
        let dims_msg = self.pool.intern("matrix dimensions too large");
        {
            let mut i = self.f.instructions();
            i.local_get(happ);
            i.local_get(hs).i64_const(1).i64_lt_s().i32_and().if_(BlockType::Empty);
            i.i32_const(stride_msg as i32);
        }
        self.emit_error_frame_abort();
        {
            let mut i = self.f.instructions();
            i.end();
            // kernel / padding clamp at 0
            for h in [hk, hp] {
                i.i64_const(0).local_get(h).local_get(h).i64_const(0).i64_lt_s().select().local_set(h);
            }
            i.local_get(happ);
            i.local_get(hp).i64_const(1 << 28).i64_gt_s().i32_and().if_(BlockType::Empty);
            i.i32_const(dims_msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
        // C-353: the weight row is `in_ch * kernel` taps and the bias has one
        // entry per output channel. The product is tested by DIVISION so a
        // huge kernel cannot wrap it (native takes `checked_mul`): with
        // in_ch > 0 the row fits iff `wc % in_ch == 0 && wc / in_ch == k`,
        // and at in_ch == 0 the row must be empty.
        let htaps = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(inch).if_(BlockType::Result(ValType::I32));
            i.local_get(wc).i64_extend_i32_u().local_get(inch).i64_extend_i32_u().i64_rem_u().i64_eqz();
            i.local_get(wc).i64_extend_i32_u().local_get(inch).i64_extend_i32_u().i64_div_u();
            i.local_get(hk).i64_eq().i32_and();
            i.else_();
            i.local_get(wc).i32_eqz();
            i.end().local_set(htaps);
            // reuse the shared abort: taps_ok == 1 is the "equal" side
            i.local_get(happ);
            i.local_get(htaps).i32_eqz().i32_and().if_(BlockType::Empty);
            let msg = self.pool.intern("matrix shape mismatch");
            i.i32_const(msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
        self.matrix_shape_eq(happ, hblen, outch);
        let htp = self.hold_i64()?;
        let htout = self.hold_i32()?;
        self.conv1d_out_rows(htout, (tin, outch), (hk, hs, hp), htp);
        let hcols = self.hold_i32()?;
        self.f.instructions().local_get(outch).local_set(hcols);
        self.zero_cols_if_no_rows(htout, hcols);
        let ho = self.mat_alloc_guarded(htout, hcols)?;
        let ht = self.hold_i32()?;
        let hoo = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hki = self.hold_i32()?;
        let hsum = self.hold_f64()?;
        self.loop_begin(ht, htout);
        self.loop_begin(hoo, hcols);
        {
            let mut i = self.f.instructions();
            i.local_get(hoo).local_get(hblen).i32_lt_u().if_(BlockType::Result(ValType::F64));
        }
        self.float_slot(hb, hoo);
        self.f.instructions().else_().f64_const(0.0f64.into()).end().local_set(hsum);
        self.loop_begin(hc, inch);
        let tap = ConvTap { hin, inch, hw, wc, tin, hk, hstride: hs, hp, ht, hoo, hc, hki, htp, hsum };
        self.conv1d_taps(&tap)?;
        self.loop_end(hc);
        self.elem_at(ho, ht, hcols, hoo);
        self.f.instructions().local_get(hsum).f64_store(mat_elem());
        self.loop_end(hoo);
        self.loop_end(ht);
        self.f.instructions().local_get(ho);
        self.release_f64();
        for _ in 0..4 {
            self.release_i64();
        }
        // hin tin inch | hw outch wc | hb hblen | happ htaps | htout hcols ho
        // | ht hoo hc hki
        for _ in 0..17 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Matrix)))
    }

    /// T_out into `dst` (i32): 0 when T or out_ch is 0 or T + 2P < K, else
    /// (T + 2P − K)/S + 1 in i64 (a zero stride traps, as the incumbent's
    /// division does; native panics — both a crash). A count below 1 (a
    /// negative stride — native reads it as a huge usize) is the empty
    /// matrix, never a wrapped extent.
    fn conv1d_out_rows(&mut self, dst: u32, (tin, outch): (u32, u32), (hk, hs, hp): (u32, u32, u32), scratch: u32) {
        let mut i = self.f.instructions();
        i.local_get(tin).i32_eqz().local_get(outch).i32_eqz().i32_or();
        i.local_get(tin).i64_extend_i32_u().local_get(hp).i64_const(2).i64_mul().i64_add();
        i.local_get(hk).i64_lt_s().i32_or();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.local_get(tin).i64_extend_i32_u().local_get(hp).i64_const(2).i64_mul().i64_add();
        i.local_get(hk).i64_sub().local_get(hs).i64_div_s().i64_const(1).i64_add();
        i.local_tee(scratch).i64_const(0).i64_gt_s();
        i.if_(BlockType::Result(ValType::I32));
        i.local_get(scratch).i32_wrap_i64();
        i.else_();
        i.i32_const(0);
        i.end();
        i.end();
        i.local_set(dst);
    }

    /// The inner `for ki < K` of one (t, o, c): tp = t·S + ki; inside
    /// [P, P + T) it adds w[o][c·K + ki]·input[tp − P][c] (a weight index
    /// past the row contributes nothing — the memory-safe reading).
    fn conv1d_taps(&mut self, v: &ConvTap) -> Result<(), EmitError> {
        let hkn = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            // the tap count K, clamped to [0, i32::MAX]
            i.local_get(v.hk).i64_const(0).local_get(v.hk).i64_const(0).i64_gt_s().select();
            i.local_tee(v.htp).i64_const(0x7FFF_FFFF).local_get(v.htp).i64_const(0x7FFF_FFFF).i64_lt_s();
            i.select().i32_wrap_i64().local_set(hkn);
        }
        self.loop_begin(v.hki, hkn);
        let mut i = self.f.instructions();
        i.local_get(v.ht).i64_extend_i32_u().local_get(v.hstride).i64_mul();
        i.local_get(v.hki).i64_extend_i32_u().i64_add().local_set(v.htp);
        // P <= tp < P + T
        i.local_get(v.htp).local_get(v.hp).i64_ge_s();
        i.local_get(v.htp).local_get(v.hp).local_get(v.tin).i64_extend_i32_u().i64_add().i64_lt_s();
        i.i32_and();
        // weight index c·K + ki inside the row
        i.local_get(v.hc).i64_extend_i32_u().local_get(v.hk).i64_mul();
        i.local_get(v.hki).i64_extend_i32_u().i64_add();
        i.local_get(v.wc).i64_extend_i32_u().i64_lt_u().i32_and();
        i.if_(BlockType::Empty);
        i.local_get(v.hsum);
        // w[o][c*K + ki]
        i.local_get(v.hw);
        i.local_get(v.hoo).local_get(v.wc).i32_mul();
        i.local_get(v.hc).i64_extend_i32_u().local_get(v.hk).i64_mul().i32_wrap_i64();
        i.i32_add().local_get(v.hki).i32_add().i32_const(3).i32_shl().i32_add();
        i.f64_load(mat_elem());
        // input[tp - P][c]
        i.local_get(v.hin);
        i.local_get(v.htp).local_get(v.hp).i64_sub().i32_wrap_i64().local_get(v.inch).i32_mul();
        i.local_get(v.hc).i32_add().i32_const(3).i32_shl().i32_add();
        i.f64_load(mat_elem());
        i.f64_mul().f64_add().local_set(v.hsum);
        i.end();
        let _ = i;
        self.loop_end(v.hki);
        self.release_i32();
        Ok(())
    }
}

/// The locals one conv1d tap loop reads (grouped for the parameter budget).
struct ConvTap {
    hin: u32,
    inch: u32,
    hw: u32,
    wc: u32,
    tin: u32,
    hk: u32,
    hstride: u32,
    hp: u32,
    ht: u32,
    hoo: u32,
    hc: u32,
    hki: u32,
    htp: u32,
    hsum: u32,
}
