//! Sorting machinery: the bottom-up merge sort, the per-element `<=`
//! flag, and the recursive type-directed total-order compare — split
//! from list_order.rs for the file budget.

use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Type-directed total-order compare: consumes (a, b) of `t`'s wasm
    /// type, leaves an i32 whose SIGN is the verdict (only <0 / 0 / >0
    /// is promised). Tuples chain fields, lists are lexicographic with
    /// the shorter-first prefix tiebreak, none < some; floats order by
    /// the sign-flipped bit key (the total order the scalar path uses).
    pub(crate) fn emit_val_cmp(&mut self, t: SliceTy) -> Result<(), EmitError> {
        match t {
            INT | FLOAT => {
                let hb = self.hold_i64()?;
                let ha = self.hold_i64()?;
                {
                    let mut i = self.f.instructions();
                    if t == FLOAT {
                        i.i64_reinterpret_f64().local_set(hb);
                        i.i64_reinterpret_f64().local_set(ha);
                        for l in [ha, hb] {
                            i.local_get(l);
                            i.local_get(l).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                            i.i64_xor().local_set(l);
                        }
                    } else {
                        i.local_set(hb).local_set(ha);
                    }
                    i.local_get(ha).local_get(hb).i64_lt_s().if_(BlockType::Result(ValType::I32));
                    i.i32_const(-1);
                    i.else_();
                    i.local_get(ha).local_get(hb).i64_gt_s();
                    i.end();
                }
                self.release_i64();
                self.release_i64();
            }
            BOOL => {
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hb).local_set(ha);
                i.local_get(ha).local_get(hb).i32_lt_u().if_(BlockType::Result(ValType::I32));
                i.i32_const(-1);
                i.else_();
                i.local_get(ha).local_get(hb).i32_gt_u();
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i32();
            }
            STR => {
                self.f.instructions().call(F_STR_CMP);
            }
            SliceTy::Tuple(ti) => {
                let fields = self.types.tuple_def(ti).fields;
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                let hc = self.hold_i32()?;
                self.f.instructions().local_set(hb).local_set(ha);
                // nested chain: cmp f_k, 0 → next field, else the verdict
                for (n, (fty, off)) in fields.iter().enumerate() {
                    self.f.instructions().local_get(ha);
                    self.load_ty_slot(*fty, *off);
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(*fty, *off);
                    self.emit_val_cmp(*fty)?;
                    let mut i = self.f.instructions();
                    i.local_tee(hc);
                    if n + 1 < fields.len() {
                        i.i32_eqz().if_(BlockType::Result(ValType::I32));
                    }
                }
                let mut i = self.f.instructions();
                for _ in 0..fields.len().saturating_sub(1) {
                    i.else_();
                    i.local_get(hc);
                    i.end();
                }
                if fields.is_empty() {
                    i.i32_const(0);
                }
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i32();
            }
            SliceTy::List(h) => {
                let el = self.types.el(h);
                let stride = el.slot_size() as i32;
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                let hc = self.hold_i32()?;
                let hk = self.hold_i32()?;
                let hn = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hb).local_set(ha);
                    i.i32_const(0).local_set(hc);
                    // n = min(len_a, len_b) in BYTES (same stride)
                    i.local_get(ha).i32_load(len_memarg());
                    i.local_get(hb).i32_load(len_memarg());
                    i.local_get(ha)
                        .i32_load(len_memarg())
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_lt_u();
                    i.select().local_set(hn);
                    i.i32_const(0).local_set(hk);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hk).local_get(hn).i32_ge_u().br_if(1);
                    i.local_get(ha)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_get(hk)
                        .i32_add();
                }
                self.load_ty_slot_at(el);
                self.f
                    .instructions()
                    .local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hk)
                    .i32_add();
                self.load_ty_slot_at(el);
                self.emit_val_cmp(el)?;
                {
                    let mut i = self.f.instructions();
                    i.local_tee(hc).i32_const(0).i32_ne().br_if(1);
                    i.local_get(hk).i32_const(stride).i32_add().local_set(hk);
                    i.br(0).end().end();
                    // prefix equal → shorter first
                    i.local_get(hc).i32_eqz().if_(BlockType::Result(ValType::I32));
                    i.local_get(ha)
                        .i32_load(len_memarg())
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_lt_u()
                        .if_(BlockType::Result(ValType::I32));
                    i.i32_const(-1);
                    i.else_();
                    i.local_get(ha)
                        .i32_load(len_memarg())
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_gt_u();
                    i.end();
                    i.else_();
                    i.local_get(hc);
                    i.end();
                }
                for _ in 0..5 {
                    self.release_i32();
                }
            }
            SliceTy::Option(h) => {
                let et = self.types.el(h);
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hb).local_set(ha);
                    i.local_get(ha).i32_eqz().if_(BlockType::Result(ValType::I32));
                    // none vs (none | some)
                    i.i32_const(0).i32_const(-1).local_get(hb).i32_eqz().select();
                    i.else_();
                    i.local_get(hb).i32_eqz().if_(BlockType::Result(ValType::I32));
                    i.i32_const(1);
                    i.else_();
                    i.local_get(ha);
                }
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hb);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.emit_val_cmp(et)?;
                self.f.instructions().end().end();
                self.release_i32();
                self.release_i32();
            }
            other => return unsup(&format!("list-cmp-elem:{other:?}")),
        }
        Ok(())
    }

    /// `[block]` -> `[sorted block]`: ping-pong bottom-up merge sort.
    /// Comparison is take-from-left on `left <= right` in the scalar
    /// orders (Int/Bool signed-or-flag i64, Float via the totalOrder key
    /// transform, Str via $str_cmp).
    pub(crate) fn emit_merge_sort(&mut self, elem: SliceTy) -> Result<(), EmitError> {
        let stride = elem.slot_size() as i32;
        let ha = self.hold_i32()?;
        let hb2 = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hlo = self.hold_i32()?;
        let hmid = self.hold_i32()?;
        let hhi = self.hold_i32()?;
        let hi_ = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hsrc = self.scr_i32_local;
        let htmp = self.tmp_i32_local;
        {
            let mut i = self.f.instructions();
            i.local_set(ha);
            i.local_get(ha).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hn);
            i.local_get(hn).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hb2);
            i.i32_const(1).local_set(hw);
            // while w < n
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hw).local_get(hn).i32_ge_u().br_if(1);
            i.i32_const(0).local_set(hlo);
            // for lo in steps of 2w
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hlo).local_get(hn).i32_ge_u().br_if(1);
            // mid = min(lo + w, n); hi = min(lo + 2w, n)
            i.local_get(hlo).local_get(hw).i32_add().local_set(hmid);
            i.local_get(hn)
                .local_get(hmid)
                .local_get(hmid)
                .local_get(hn)
                .i32_gt_u()
                .select()
                .local_set(hmid);
            i.local_get(hlo).local_get(hw).i32_const(1).i32_shl().i32_add().local_set(hhi);
            i.local_get(hn)
                .local_get(hhi)
                .local_get(hhi)
                .local_get(hn)
                .i32_gt_u()
                .select()
                .local_set(hhi);
            i.local_get(hlo).local_set(hi_);
            i.local_get(hmid).local_set(hj);
            i.local_get(hlo).local_set(hk);
            // merge [lo,mid) + [mid,hi) from A into B
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hk).local_get(hhi).i32_ge_u().br_if(1);
            // take_left = j >= hi || (i < mid && a[i] <= a[j])
            i.local_get(hj).local_get(hhi).i32_ge_u();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(1);
            i.else_();
            i.local_get(hi_).local_get(hmid).i32_ge_u();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
        }
        self.emit_le_flag(elem, ha, hi_, hj)?;
        {
            let mut i = self.f.instructions();
            i.end();
            i.end();
            // src = take_left ? i++ : j++
            i.if_(BlockType::Result(ValType::I32));
            i.local_get(hi_);
            i.local_get(hi_).i32_const(1).i32_add().local_set(hi_);
            i.else_();
            i.local_get(hj);
            i.local_get(hj).i32_const(1).i32_add().local_set(hj);
            i.end();
            i.local_set(hsrc);
            // B[k] = A[src]
            i.local_get(hb2).local_get(hk).i32_const(stride).i32_mul().i32_add();
            i.local_get(ha).local_get(hsrc).i32_const(stride).i32_mul().i32_add();
            if stride == 8 {
                i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
            } else {
                i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
            }
            i.local_get(hk).i32_const(1).i32_add().local_set(hk);
            i.br(0);
            i.end();
            i.end();
            i.local_get(hlo).local_get(hw).i32_const(1).i32_shl().i32_add().local_set(hlo);
            i.br(0);
            i.end();
            i.end();
            // swap A <-> B; w *= 2
            i.local_get(ha).local_set(htmp);
            i.local_get(hb2).local_set(ha);
            i.local_get(htmp).local_set(hb2);
            i.local_get(hw).i32_const(1).i32_shl().local_set(hw);
            i.br(0);
            i.end();
            i.end();
            i.local_get(ha);
        }
        for _ in 0..10 {
            self.release_i32();
        }
        Ok(())
    }

    /// Push `A[i] <= A[j]` as an i32 flag.
    pub(crate) fn emit_le_flag(&mut self, elem: SliceTy, ha: u32, hi_: u32, hj: u32) -> Result<(), EmitError> {
        let stride = elem.slot_size() as i32;
        let mut i = self.f.instructions();
        let addr = |i: &mut wasm_encoder::InstructionSink<'_>, idx: u32| {
            i.local_get(ha).local_get(idx).i32_const(stride).i32_mul().i32_add();
        };
        match elem {
            FLOAT => {
                let t = self.scr_i64_local;
                for idx in [hi_, hj] {
                    addr(&mut i, idx);
                    i.i64_load(slot_memarg(0)).local_set(t);
                    i.local_get(t);
                    i.local_get(t).i64_const(63).i64_shr_s().i64_const(1).i64_shr_u();
                    i.i64_xor();
                }
                i.i64_le_s();
            }
            INT => {
                addr(&mut i, hi_);
                i.i64_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i64_load(slot_memarg(0));
                i.i64_le_s();
            }
            BOOL => {
                addr(&mut i, hi_);
                i.i32_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i32_load(slot_memarg(0));
                i.i32_le_u();
            }
            STR => {
                addr(&mut i, hi_);
                i.i32_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i32_load(slot_memarg(0));
                i.call(F_STR_CMP).i32_const(0).i32_le_s();
            }
            // Compound handles: the recursive type-directed total order.
            _ => {
                addr(&mut i, hi_);
                i.i32_load(slot_memarg(0));
                addr(&mut i, hj);
                i.i32_load(slot_memarg(0));
                let _ = i;
                self.emit_val_cmp(elem)?;
                self.f.instructions().i32_const(0).i32_le_s();
            }
        }
        Ok(())
    }

}
