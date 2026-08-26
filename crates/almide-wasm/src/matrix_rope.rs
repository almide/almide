//! RoPE rotation family + multi-head attention on the FLAT layout —
//! transcribed from matrix_activations self-hosts. The rope trig runs
//! through the LINKED vendored libm (math.fpow/sin/cos — 1/pow, NOT the
//! fast-exp: the last-ulp lesson of the rope_at burn-down), attention's
//! softmax runs the canonical `$fast_exp` (#1197). Guards: head count
//! < 1 and head geometry past the row width both die in the unified
//! `Error: …` + exit 1 form.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::matrix_kernels::mat_elem;
use crate::work::Helper;
use crate::*;

impl Emitter<'_> {
    /// rope_rotate / rope_rotate_at / rope_rotate_neox_at. Non-pair
    /// columns COPY THROUGH (#1419); position = start + row (start
    /// clamps at 0).
    pub(crate) fn lower_matrix_rope(
        &mut self,
        neox: bool,
        x: &IrExpr,
        n_heads: &IrExpr,
        head_dim: &IrExpr,
        theta: &IrExpr,
        start: Option<&IrExpr>,
    ) -> Result<Option<SliceTy>, EmitError> {
        let fpow = self.linked_math("math.fpow")?;
        let fsin = self.linked_math("math.sin")?;
        let fcos = self.linked_math("math.cos")?;
        let (hm, hr, hc) = self.mat_open(x)?;
        self.lower(n_heads, Some(INT))?;
        let hnh = self.hold_i64()?;
        self.f.instructions().local_set(hnh);
        self.lower(head_dim, Some(INT))?;
        let hhd = self.hold_i64()?;
        self.f.instructions().local_set(hhd);
        self.lower(theta, Some(FLOAT))?;
        let hth = self.hold_f64()?;
        self.f.instructions().local_set(hth);
        let hst = self.hold_i64()?;
        if let Some(s) = start {
            self.lower(s, Some(INT))?;
            let mut i = self.f.instructions();
            i.local_set(hst);
            i.i64_const(0).local_get(hst).local_get(hst).i64_const(0).i64_lt_s().select();
            i.local_set(hst);
        } else {
            self.f.instructions().i64_const(0).local_set(hst);
        }
        // head count / geometry guards
        let count_msg = self.pool.intern("head count must be positive");
        let geo_msg = self.pool.intern("head geometry exceeds row width");
        {
            let mut i = self.f.instructions();
            i.local_get(hnh).i64_const(1).i64_lt_s().if_(BlockType::Empty);
            i.i32_const(count_msg as i32);
        }
        self.emit_error_frame_abort();
        {
            let mut i = self.f.instructions();
            i.end();
            i.local_get(hr).i32_const(0).i32_gt_s();
            i.local_get(hhd).i64_const(0).i64_gt_s().i32_and();
            i.local_get(hhd).i64_const(0).i64_gt_s().if_(BlockType::Result(ValType::I32));
            i.local_get(hnh);
            i.local_get(hc).i64_extend_i32_u().local_get(hhd).i64_div_s();
            i.i64_gt_s();
            i.else_();
            i.i32_const(0);
            i.end();
            i.i32_and().if_(BlockType::Empty);
            i.i32_const(geo_msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
        let ho = self.mat_alloc_out(hr, hc)?;
        let hp = self.hold_i32()?;
        let hh = self.hold_i64()?;
        let hip = self.hold_i64()?;
        let hj0 = self.hold_i32()?;
        let hj1 = self.hold_i32()?;
        let hpos = self.hold_f64()?;
        let hang = self.hold_f64()?;
        let hx0 = self.hold_f64()?;
        let hx1 = self.hold_f64()?;
        let hsin = self.hold_f64()?;
        let hcos = self.hold_f64()?;
        let mut i = self.f.instructions();
        // copy-through the whole payload once
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
        i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32 + 8).i32_add();
        i.local_get(hr).local_get(hc).i32_mul().i32_const(8).i32_mul();
        i.memory_copy(0, 0);
        i.i32_const(0).local_set(hp);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hp).local_get(hr).i32_ge_u().br_if(1);
        i.local_get(hst).local_get(hp).i64_extend_i32_u().i64_add().f64_convert_i64_s();
        i.local_set(hpos);
        i.i64_const(0).local_set(hh);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hh).local_get(hnh).i64_ge_s().br_if(1);
        i.i64_const(0).local_set(hip);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hip).local_get(hhd).i64_const(2).i64_div_s().i64_ge_s().br_if(1);
        // pair columns
        if neox {
            i.local_get(hh)
                .local_get(hhd)
                .i64_mul()
                .local_get(hip)
                .i64_add()
                .i32_wrap_i64()
                .local_set(hj0);
            i.local_get(hh)
                .local_get(hhd)
                .i64_mul()
                .local_get(hhd)
                .i64_const(2)
                .i64_div_s()
                .i64_add()
                .local_get(hip)
                .i64_add()
                .i32_wrap_i64()
                .local_set(hj1);
        } else {
            i.local_get(hh)
                .local_get(hhd)
                .i64_mul()
                .local_get(hip)
                .i64_const(2)
                .i64_mul()
                .i64_add()
                .i32_wrap_i64()
                .local_set(hj0);
            i.local_get(hj0).i32_const(1).i32_add().local_set(hj1);
        }
        // x0/x1 from the SOURCE row
        i.local_get(hm);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj0).i32_add().i32_const(8).i32_mul();
        i.i32_add().f64_load(mat_elem()).local_set(hx0);
        i.local_get(hm);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj1).i32_add().i32_const(8).i32_mul();
        i.i32_add().f64_load(mat_elem()).local_set(hx1);
        // angle = pos * (1 / theta^(2i/dim))
        i.f64_const(1.0f64.into());
        i.local_get(hth);
        i.local_get(hip).i64_const(2).i64_mul().f64_convert_i64_s();
        i.local_get(hhd).f64_convert_i64_s();
        i.f64_div();
        i.call(fpow);
        i.f64_div();
        i.local_get(hpos).f64_mul().local_set(hang);
        i.local_get(hang).call(fsin).local_set(hsin);
        i.local_get(hang).call(fcos).local_set(hcos);
        // out[j0] = x0·c − x1·s; out[j1] = x0·s + x1·c
        i.local_get(ho);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj0).i32_add().i32_const(8).i32_mul();
        i.i32_add();
        i.local_get(hx0).local_get(hcos).f64_mul();
        i.local_get(hx1).local_get(hsin).f64_mul();
        i.f64_sub().f64_store(mat_elem());
        i.local_get(ho);
        i.local_get(hp).local_get(hc).i32_mul().local_get(hj1).i32_add().i32_const(8).i32_mul();
        i.i32_add();
        i.local_get(hx0).local_get(hsin).f64_mul();
        i.local_get(hx1).local_get(hcos).f64_mul();
        i.f64_add().f64_store(mat_elem());
        i.local_get(hip).i64_const(1).i64_add().local_set(hip);
        i.br(0).end().end();
        i.local_get(hh).i64_const(1).i64_add().local_set(hh);
        i.br(0).end().end();
        i.local_get(hp).i32_const(1).i32_add().local_set(hp);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..6 {
            self.release_f64();
        }
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_f64();
        self.release_i64();
        self.release_i64();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }

    /// multi_head_attention / masked_multi_head_attention: per (row,
    /// head) — scaled dot scores (+ the causal −1e9 mask), fast-exp
    /// softmax with the bad-sum uniform fallback, weighted V sum into
    /// disjoint head columns. K/V rows use their OWN column strides.
    pub(crate) fn lower_matrix_mha(
        &mut self,
        causal: bool,
        q: &IrExpr,
        k: &IrExpr,
        v: &IrExpr,
        n_heads: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let fe = self.work.helper(Helper::FastExp);
        let (hq, hsq, hdm) = self.mat_open(q)?;
        let (hk, hsk, hkc) = self.mat_open(k)?;
        let (hv, _hvr, hvc) = self.mat_open(v)?;
        self.lower(n_heads, Some(INT))?;
        let hnh = self.hold_i64()?;
        self.f.instructions().local_set(hnh);
        let count_msg = self.pool.intern("head count must be positive");
        {
            let mut i = self.f.instructions();
            i.local_get(hnh).i64_const(1).i64_lt_s().if_(BlockType::Empty);
            i.i32_const(count_msg as i32);
        }
        self.emit_error_frame_abort();
        self.f.instructions().end();
        let hdh = self.hold_i64()?; // head dim
        let hscale = self.hold_f64()?;
        let hsc = self.hold_i32()?; // scores scratch base
        {
            let mut i = self.f.instructions();
            i.local_get(hdm).i64_extend_i32_u().local_get(hnh).i64_div_s().local_set(hdh);
            i.f64_const(1.0f64.into());
            i.local_get(hdh).f64_convert_i64_s().f64_sqrt();
            i.f64_div().local_set(hscale);
            i.local_get(hsk).i32_const(8).i32_mul().call(F_ALLOC).local_set(hsc);
        }
        let ho = self.mat_alloc_out(hsq, hdm)?;
        let hi = self.hold_i32()?;
        let hh = self.hold_i64()?;
        let hj = self.hold_i32()?;
        let hkk = self.hold_i32()?;
        let hcol0 = self.hold_i32()?;
        let hmask = self.hold_i64()?;
        let hacc = self.hold_f64()?;
        let hsum = self.hold_f64()?;
        let hw = self.hold_f64()?;
        let mut i = self.f.instructions();
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hsq).i32_ge_u().br_if(1);
        i.i64_const(0).local_set(hh);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hh).local_get(hnh).i64_ge_s().br_if(1);
        i.local_get(hh).local_get(hdh).i64_mul().i32_wrap_i64().local_set(hcol0);
        // mask = causal ? (sk - sq) + i : -1
        if causal {
            i.local_get(hsk)
                .local_get(hsq)
                .i32_sub()
                .local_get(hi)
                .i32_add()
                .i64_extend_i32_s()
                .local_set(hmask);
        } else {
            i.i64_const(-1).local_set(hmask);
        }
        // scores
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hsk).i32_ge_u().br_if(1);
        i.f64_const(0.0f64.into()).local_set(hacc);
        i.i32_const(0).local_set(hkk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hkk).local_get(hdh).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(hacc);
        i.local_get(hq);
        i.local_get(hi)
            .local_get(hdm)
            .i32_mul()
            .local_get(hcol0)
            .i32_add()
            .local_get(hkk)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add().f64_load(mat_elem());
        i.local_get(hk);
        i.local_get(hj)
            .local_get(hkc)
            .i32_mul()
            .local_get(hcol0)
            .i32_add()
            .local_get(hkk)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add().f64_load(mat_elem());
        i.f64_mul().f64_add().local_set(hacc);
        i.local_get(hkk).i32_const(1).i32_add().local_set(hkk);
        i.br(0).end().end();
        i.local_get(hacc).local_get(hscale).f64_mul().local_set(hacc);
        // causal mask
        i.local_get(hmask).i64_const(0).i64_ge_s();
        i.local_get(hj).i64_extend_i32_s().local_get(hmask).i64_gt_s();
        i.i32_and().if_(BlockType::Empty);
        i.local_get(hacc).f64_const((-1_000_000_000.0f64).into()).f64_add().local_set(hacc);
        i.end();
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add();
        i.local_get(hacc).f64_store(slot_memarg(0));
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        // mx = scores[0]; scan from 1 with `>` — reuse hacc as mx
        i.local_get(hsc).f64_load(slot_memarg(0)).local_set(hacc);
        i.i32_const(1).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hsk).i32_ge_u().br_if(1);
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add().f64_load(slot_memarg(0));
        i.local_set(hw);
        i.local_get(hw).local_get(hacc);
        i.local_get(hw).local_get(hacc).f64_gt();
        i.select().local_set(hacc);
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        // exp + sum
        i.f64_const(0.0f64.into()).local_set(hsum);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hsk).i32_ge_u().br_if(1);
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add();
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add().f64_load(slot_memarg(0));
        i.local_get(hacc).f64_sub().call(fe).local_set(hw);
        i.local_get(hw).f64_store(slot_memarg(0));
        i.local_get(hsum).local_get(hw).f64_add().local_set(hsum);
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        // bad sum → scores all 1.0, sum = f64(sk)
        i.local_get(hsum).f64_const(0.0f64.into()).f64_le();
        i.local_get(hsum).local_get(hsum).f64_ne();
        i.i32_or().if_(BlockType::Empty);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hsk).i32_ge_u().br_if(1);
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add();
        i.f64_const(1.0f64.into()).f64_store(slot_memarg(0));
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.local_get(hsk).f64_convert_i32_s().local_set(hsum);
        i.end();
        // out[i, col0+kk] += (scores[j]/sum) * v[j, col0+kk]
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hsk).i32_ge_u().br_if(1);
        i.local_get(hsc).local_get(hj).i32_const(8).i32_mul().i32_add().f64_load(slot_memarg(0));
        i.local_get(hsum).f64_div().local_set(hw);
        i.i32_const(0).local_set(hkk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hkk).local_get(hdh).i32_wrap_i64().i32_ge_u().br_if(1);
        i.local_get(ho);
        i.local_get(hi)
            .local_get(hdm)
            .i32_mul()
            .local_get(hcol0)
            .i32_add()
            .local_get(hkk)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add();
        i.local_get(ho);
        i.local_get(hi)
            .local_get(hdm)
            .i32_mul()
            .local_get(hcol0)
            .i32_add()
            .local_get(hkk)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add().f64_load(mat_elem());
        i.local_get(hv);
        i.local_get(hj)
            .local_get(hvc)
            .i32_mul()
            .local_get(hcol0)
            .i32_add()
            .local_get(hkk)
            .i32_add()
            .i32_const(8)
            .i32_mul();
        i.i32_add().f64_load(mat_elem());
        i.local_get(hw).f64_mul().f64_add();
        i.f64_store(mat_elem());
        i.local_get(hkk).i32_const(1).i32_add().local_set(hkk);
        i.br(0).end().end();
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.local_get(hh).i64_const(1).i64_add().local_set(hh);
        i.br(0).end().end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..4 {
            self.release_f64();
        }
        for _ in 0..4 {
            self.release_i64();
        }
        for _ in 0..15 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Matrix))
    }
}
