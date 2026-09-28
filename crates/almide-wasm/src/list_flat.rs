//! `list.flat_map` / `list.filter_map` — split from list.rs for the
//! 800-line file discipline (the arms are unchanged) — and `list.partition`
//! (#2744), the two-sided filter.

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

    /// `list.partition(xs, f)` (#2744): ONE predicate call per element, in
    /// order (native's `Iterator::partition`), each element copied into the
    /// yes or the no side. The filter doctrine on both sides: one
    /// upper-bound allocation each, a kept counter, and a final LEN rewrite
    /// (CAP stays what `$alloc` wrote); the kept slots are copies of the
    /// source's handles, so each spine takes its own element credits once
    /// LEN is final. The result is the `(yes, no)` tuple of two owned lists
    /// (the result.partition shape).
    pub(crate) fn lower_list_partition(&mut self, xs: &IrExpr, cb: &IrExpr) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let stride = elem.slot_size() as i32;
        let hy = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hwy = self.hold_i32()?;
        let hwn = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(ch).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hy);
            i.local_get(ch).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hn);
            i.i32_const(0).local_set(hwy);
            i.i32_const(0).local_set(hwn);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        for (side, w) in [(hy, hwy), (hn, hwn)] {
            self.f
                .instructions()
                .local_get(side)
                .local_get(w)
                .i32_const(stride)
                .i32_mul()
                .i32_add()
                .local_get(params[0]);
            self.store_ty_slot(elem, 0);
            self.f.instructions().local_get(w).i32_const(1).i32_add().local_set(w);
            if side == hy {
                self.f.instructions().else_();
            }
        }
        self.f.instructions().end();
        self.hof_step(ih);
        for (side, w) in [(hy, hwy), (hn, hwn)] {
            self.f
                .instructions()
                .local_get(side)
                .local_get(w)
                .i32_const(stride)
                .i32_mul()
                .i32_store(len_memarg());
            self.emit_inc_elems(side, elem);
        }
        let el = self.types.intern(elem);
        let ti = self.types.tuple(vec![SliceTy::List(el), SliceTy::List(el)]);
        let def = self.types.tuple_def(ti);
        let (off_y, off_n, size) = (def.fields[0].1, def.fields[1].1, def.size);
        {
            let hr = self.tmp_i32_local;
            let mut i = self.f.instructions();
            i.i32_const(size as i32).call(F_ALLOC).local_set(hr);
            i.local_get(hr).local_get(hy).i32_store(slot_memarg(off_y));
            i.local_get(hr).local_get(hn).i32_store(slot_memarg(off_n));
            i.local_get(hr);
        }
        for _ in 0..7 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Tuple(ti))))
    }
}
