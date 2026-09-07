//! `list.flat_map` / `list.filter_map` — split from list.rs for the
//! 800-line file discipline (the arms are unchanged).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_list_flat_map(&mut self, xs: &IrExpr, cb: &IrExpr) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hs = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        let got = self.lower(body, None)?;
        let SliceTy::List(bi) = got else {
            return unsup(&format!("flat-map-body:{got:?}"));
        };
        if matches!(self.types.el(bi), SliceTy::Scalar(Scalar::Int | Scalar::Float)) {
            // 8-byte scalar chunks ride the amortized push window (#1729):
            // the per-element $concat re-copied the WHOLE accumulator and
            // freed neither side, so n chunks retained O(n²) bytes — 2^16
            // elements OOM'd where the append row's working set fits. A
            // borrowed chunk (a callback returning a captured list) takes
            // inc-before-dec, so the per-iteration dec balances fresh and
            // aliased alike; the move is by 8-byte bits, so Int and Float
            // share one loop and $dec_flat (shallow) is a full free.
            if !self.rc_owned_result(crate::rc_ownership::rc_tail(body)) {
                self.rc_inc_top();
            }
            self.f.instructions().local_set(hs);
            let hj = self.hold_i32()?;
            let he = self.hold_i32()?;
            self.f.instructions().local_get(hs).i32_load(len_memarg()).local_set(he);
            self.f.instructions().i32_const(0).local_set(hj);
            self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
            self.f.instructions().local_get(hj).local_get(he).i32_ge_u().br_if(1);
            self.f.instructions().local_get(hacc);
            self.f.instructions().local_get(hs).local_get(hj).i32_add();
            self.f.instructions().i64_load(slot_memarg(0));
            self.f.instructions().call(F_LIST_PUSH_8).local_set(hacc);
            self.f.instructions().local_get(hj).i32_const(8).i32_add().local_set(hj);
            self.f.instructions().br(0).end().end();
            self.f.instructions().local_get(hs).call(F_DEC_FLAT);
            self.release_i32();
            self.release_i32();
        } else {
            // Handle-element chunks keep the concat merge: their interiors
            // are co-owned by the chunk, and freeing after a raw byte copy
            // needs the Dup discipline (the C-186 trap) — outside this
            // window, as in the assign-site append gate.
            self.f.instructions().local_set(hs);
            self.f.instructions().local_get(hacc).local_get(hs).call(F_CONCAT);
            self.f.instructions().local_set(hacc);
        }
        self.hof_step(ih);
        // Handle elements were COPIED through the concats: the result takes
        // one credit per element (the intermediate spines leak, never
        // dangle — fuzz 20260912: a callback returning a captured list).
        if !matches!(self.types.el(bi), SliceTy::Scalar(Scalar::Int | Scalar::Float)) {
            self.emit_inc_elems(hacc, self.types.el(bi));
        }
        self.f.instructions().local_get(hacc);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(bi))))
    }

    pub(crate) fn lower_list_filter_map(&mut self, xs: &IrExpr, cb: &IrExpr) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        let got = self.lower(body, None)?;
        let SliceTy::Option(oi) = got else {
            return unsup(&format!("filter-map-body:{got:?}"));
        };
        let b = self.types.el(oi);
        self.f.instructions().local_tee(hr).if_(BlockType::Empty);
        self.f.instructions().local_get(hacc).local_get(hr);
        self.load_ty_slot(b, almide_layout::OPTION_FIELD);
        if b.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match b.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        // The Option's payload moves into the result as a SHARE: the Option
        // temporary keeps (and releases) its own credit.
        self.share_handle_top(b);
        self.f.instructions().call(push).local_set(hacc).end();
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(b)))))
    }
}
