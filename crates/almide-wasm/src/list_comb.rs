//! List combinator surfaces (last/contains/product/flatten/all/
//! take_while/reduce/scan/zip/zip_with/unique/intersperse) — split for
//! the file budget; semantics verbatim from runtime/rs.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

#[path = "list_zip.rs"]
mod list_zip;

impl Emitter<'_> {
    /// some(last) or none (native `xs.last().cloned()`).
    pub(crate) fn lower_list_last(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-last-of:{other:?}")),
        };
        let elem = self.types.el(h);
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(hb).i32_load(len_memarg()).i32_eqz();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(0);
        i.else_();
        i.i32_const(stride).call(F_ALLOC).local_set(hr);
        i.local_get(hr);
        i.local_get(hb).local_get(hb).i32_load(len_memarg()).i32_add().i32_const(stride).i32_sub();
        let _ = i;
        self.load_ty_slot(elem, 0);
        // The element handle inside the Option block takes +1
        // (leak-not-dangle until the Option's typed drop, stage 2c).
        self.share_handle_top(elem);
        self.store_ty_slot(elem, almide_layout::OPTION_FIELD);
        self.f.instructions().local_get(hr).end();
        self.release_i32();
        self.release_i32();
        Ok(Some(Lowered::owned(SliceTy::Option(self.types.intern(elem)))))
    }

    /// Sequential product (native: Int wrapping_mul fold from 1;
    /// Float `iter().product()` — the same left fold from 1.0).
    pub(crate) fn lower_list_product(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-product-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(elem, INT | FLOAT) {
            return unsup(&format!("list-product-elem:{elem:?}"));
        }
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = if elem == INT { self.hold_i64()? } else { self.hold_f64()? };
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.i32_const(0).local_set(hc);
        if elem == INT {
            i.i64_const(1).local_set(hacc);
        } else {
            i.f64_const(1.0.into()).local_set(hacc);
        }
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
        i.local_get(hacc);
        i.local_get(hb).local_get(hc).i32_add();
        let _ = i;
        self.load_ty_slot(elem, 0);
        let mut i = self.f.instructions();
        if elem == INT {
            i.i64_mul();
        } else {
            i.f64_mul();
        }
        i.local_set(hacc);
        i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(hacc);
        let _ = i;
        if elem == INT {
            self.release_i64();
        } else {
            self.release_f64();
        }
        self.release_i32();
        self.release_i32();
        Ok(Some(Lowered::scalar(elem)))
    }

    /// Concat of the inner lists, in order.
    pub(crate) fn lower_list_flatten(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-flatten-of:{other:?}")),
        };
        let SliceTy::List(inner) = self.types.el(h) else {
            return unsup("list-flatten-nonnested");
        };
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hprev = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.i32_const(0).call(F_ALLOC).local_set(hacc);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
        i.local_get(hacc).local_tee(hprev);
        i.local_get(hb).local_get(hc).i32_add().i32_load(slot_memarg(0));
        i.call(F_CONCAT).local_set(hacc);
        // The outgrown accumulator is a bare spine no credit was taken
        // through yet (#2977 — one leaked per inner list).
        i.local_get(hprev).call(F_DEC_FLAT);
        i.local_get(hc).i32_const(4).i32_add().local_set(hc);
        i.br(0).end().end();
        let _ = i;
        // The inner lists' handles were COPIED through the concats: the
        // result takes one credit per element, once, on the final spine.
        self.emit_inc_elems(hacc, self.types.el(inner));
        self.f.instructions().local_get(hacc);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(inner))))
    }

    /// Sequential sum (native: Int wrapping fold from 0; Float
    /// `iter().sum()` — the same left fold from 0.0).
    pub(crate) fn lower_list_sum(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-sum-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(elem, INT | FLOAT) {
            return unsup(&format!("list-sum-elem:{elem:?}"));
        }
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = if elem == INT { self.hold_i64()? } else { self.hold_f64()? };
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.i32_const(0).local_set(hc);
        if elem == INT {
            i.i64_const(0).local_set(hacc);
        } else {
            i.f64_const(0.0.into()).local_set(hacc);
        }
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
        i.local_get(hacc);
        i.local_get(hb).local_get(hc).i32_add();
        let _ = i;
        self.load_ty_slot(elem, 0);
        let mut i = self.f.instructions();
        if elem == INT {
            i.i64_add();
        } else {
            i.f64_add();
        }
        i.local_set(hacc);
        i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(hacc);
        let _ = i;
        if elem == INT {
            self.release_i64();
        } else {
            self.release_f64();
        }
        self.release_i32();
        self.release_i32();
        Ok(Some(Lowered::scalar(elem)))
    }

    /// CONSECUTIVE dedup (native `r.last() != Some(x)` — unlike unique,
    /// only adjacent equals fold).
    pub(crate) fn lower_list_dedup(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-dedup-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(
            elem,
            INT | FLOAT
                | STR
                | BOOL
                | SliceTy::Tuple(_)
                | SliceTy::Named(_)
                | SliceTy::List(_)
                | SliceTy::Option(_)
        ) {
            return unsup(&format!("list-dedup-elem:{elem:?}"));
        }
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hx = self.hold_val(elem)?;
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hc);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
            i.local_get(hb).local_get(hc).i32_add();
        }
        self.load_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_set(hx);
            // keep unless equal to the LAST kept element
            i.local_get(hacc).i32_load(len_memarg()).i32_eqz();
            i.if_(BlockType::Result(ValType::I32));
            i.i32_const(1);
            i.else_();
            i.local_get(hacc)
                .local_get(hacc)
                .i32_load(len_memarg())
                .i32_add()
                .i32_const(stride)
                .i32_sub();
        }
        self.load_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hx);
            match elem {
                INT => {
                    i.i64_ne();
                }
                FLOAT => {
                    i.f64_ne();
                }
                STR => {
                    i.call(F_STR_EQ).i32_eqz();
                }
                BOOL => {
                    i.i32_ne();
                }
                _ => {
                    let _ = i;
                    self.emit_val_eq(elem)?;
                    i = self.f.instructions();
                    i.i32_eqz();
                }
            }
            i.end();
            i.if_(BlockType::Empty);
            i.local_get(hacc).local_get(hx);
            if elem.val_type() == ValType::F64 {
                i.i64_reinterpret_f64();
            }
            i.call(push).local_set(hacc);
            i.end();
            i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
            i.br(0).end().end();
        }
        // The kept elements are COPIES of the source's handles.
        self.emit_inc_elems(hacc, elem);
        self.f.instructions().local_get(hacc);
        self.release_val(elem);
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(h))))
    }

    /// Suffix after the first false (native skip_while: the callback
    /// runs through the first failing element).
    pub(crate) fn lower_list_drop_while(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        let hd = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().i32_const(0).local_set(hd);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        // #2755: the callback runs on the dropping arm only; a kept element
        // is shared into the suffix.
        self.witness_callback_open(cb, None);
        self.witness_branch_open();
        self.witness_branch_arm();
        // still dropping? run the callback; a false flips to keeping
        self.f.instructions().local_get(hd).i32_eqz().if_(BlockType::Empty);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().i32_eqz().local_set(hd);
        self.f.instructions().end();
        self.witness_branch_arm();
        self.witness_branch_close();
        self.witness_hit(&[(params[0], elem)]);
        self.f.instructions().local_get(hd).if_(BlockType::Empty);
        self.f.instructions().local_get(hacc).local_get(params[0]);
        if elem.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        // The pushed element is a COPY of the source's handle: the result
        // takes its share (#2010).
        self.share_handle_top(elem);
        self.f.instructions().call(push).local_set(hacc);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(elem)))))
    }

    /// Exists (native `iter().any`): the first true wins, empty = false.
    pub(crate) fn lower_list_any(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower_inlined_predicate(cb, body)?;
        self.f.instructions().if_(BlockType::Empty);
        self.f.instructions().i32_const(1).local_set(hr);
        self.f.instructions().br(2);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hr);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(BOOL)))
    }

    /// Forall (native `iter().all`): the first false wins, empty = true.
    pub(crate) fn lower_list_all(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(1).local_set(hr);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower_inlined_predicate(cb, body)?;
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        self.f.instructions().i32_const(0).local_set(hr);
        self.f.instructions().br(2);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hr);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(BOOL)))
    }

    /// Matching-element count (native `filter().count()` — every element
    /// runs through the callback, no early exit).
    pub(crate) fn lower_list_count(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hn = self.hold_i64()?;
        self.f.instructions().i64_const(0).local_set(hn);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower_inlined_predicate(cb, body)?;
        self.f.instructions().if_(BlockType::Empty);
        self.f.instructions().local_get(hn).i64_const(1).i64_add().local_set(hn);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hn);
        self.release_i64();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(INT)))
    }

    /// Prefix while true (the callback runs through the FIRST false).
    pub(crate) fn lower_list_take_while(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.witness_callback_open(cb, None);
        self.lower(body, Some(BOOL))?;
        // #2755: a false verdict leaves the walk; a true one shares the
        // element into the prefix.
        self.witness_filter_skip();
        self.witness_view_shares(&[(params[0], elem)]);
        self.witness_loop_close();
        self.f.instructions().i32_eqz().br_if(1);
        self.f.instructions().local_get(hacc).local_get(params[0]);
        if elem.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        // The pushed element is a COPY of the source's handle: the result
        // takes its share (#2010).
        self.share_handle_top(elem);
        self.f.instructions().call(push).local_set(hacc);
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(elem)))))
    }

    /// some(fold-from-first) or none (native `into_iter().reduce`).
    pub(crate) fn lower_list_reduce(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(ch).i32_eqz().if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
            // acc = xs[0] (into the ACC param), walk from 1
            i.local_get(bh);
        }
        // #2755: the empty list answers none; the other arm seeds the
        // accumulator with a share of the head, moved into the walk, whose
        // activations are list.fold's.
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_branch_arm();
        self.load_ty_slot(elem, 0);
        // The accumulator OWNS one credit on every step, as `list.fold`'s
        // does (#2977): the first element takes its share, each step's
        // borrowed result takes its share and the replaced accumulator is
        // released, and the last one MOVES into the `some` cell.
        self.share_handle_top(elem);
        self.witness_payload_share(elem);
        self.f.instructions().local_set(params[0]);
        self.f.instructions().i32_const(1).local_set(ih);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[1]);
        self.witness_callback_open(cb, Some(params[0]));
        self.lower(body, Some(elem))?;
        self.rc_share_guard(body, elem);
        if let Some(dec) = self.elem_is_handle(elem).then(|| self.dec_fn_of(elem)) {
            self.f.instructions().local_get(params[0]).call(dec);
        }
        self.witness_fold_step(body, params[0], elem);
        self.f.instructions().local_set(params[0]);
        self.witness_loop_close();
        self.hof_step(ih);
        // some(acc)
        self.f
            .instructions()
            .i32_const(elem.slot_size() as i32)
            .call(F_ALLOC)
            .local_tee(hr)
            .local_get(params[0]);
        self.store_ty_slot(elem, almide_layout::OPTION_FIELD);
        self.f.instructions().local_get(hr).end();
        self.witness_branch_close();
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Option(self.types.intern(elem)))))
    }

    /// Running-accumulator list (native scan: push EVERY new acc).
    pub(crate) fn lower_list_scan(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let b_ty = self.infer(body)?;
        // The seed is only READ (never pushed): a borrow, released by the
        // scope after the loop. Every pushed accumulator is a value the
        // result list holds — a borrowed body result (the seed, the
        // previous accumulator) takes its share before the push (fuzz
        // 20260910: `scan(xs, s, (a, x) => a)` pushed one block twice).
        self.lower_arg(init, Some(b_ty), ArgMode::Borrow)?;
        self.f.instructions().local_set(params[0]);
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[1]);
        self.witness_callback_open(cb, None);
        self.lower(body, Some(b_ty))?;
        self.rc_share_guard(body, b_ty);
        // #2755: the guarded value moves into the result list; the
        // accumulator param is a view of it for the next element.
        self.witness_store(body, b_ty);
        self.witness_loop_close();
        self.f.instructions().local_set(params[0]);
        self.f.instructions().local_get(hacc).local_get(params[0]);
        if b_ty.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match b_ty.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        self.f.instructions().call(push).local_set(hacc);
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(b_ty)))))
    }

    /// First-seen order dedup (native nested contains walk).
    pub(crate) fn lower_list_unique(&mut self, xs: &IrExpr) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-unique-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(
            elem,
            INT | FLOAT
                | STR
                | BOOL
                | SliceTy::Tuple(_)
                | SliceTy::Named(_)
                | SliceTy::List(_)
                | SliceTy::Option(_)
        ) {
            return unsup(&format!("list-unique-elem:{elem:?}"));
        }
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hf = self.hold_i32()?;
        let hx = self.hold_val(elem)?;
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hc);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
            i.local_get(hb).local_get(hc).i32_add();
        }
        self.load_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_set(hx);
            // seen? scan the OUT list
            i.i32_const(0).local_set(hf);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hacc).i32_load(len_memarg()).i32_ge_u().br_if(1);
            i.local_get(hacc).local_get(hj).i32_add();
        }
        self.load_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hx);
            match elem {
                INT => {
                    i.i64_eq();
                }
                FLOAT => {
                    i.f64_eq();
                }
                STR => {
                    i.call(F_STR_EQ);
                }
                BOOL => {
                    i.i32_eq();
                }
                _ => {
                    let _ = i;
                    self.emit_val_eq(elem)?;
                    i = self.f.instructions();
                }
            }
            i.if_(BlockType::Empty);
            i.i32_const(1).local_set(hf);
            i.br(2);
            i.end();
            i.local_get(hj).i32_const(stride).i32_add().local_set(hj);
            i.br(0).end().end();
            i.local_get(hf).i32_eqz().if_(BlockType::Empty);
            i.local_get(hacc).local_get(hx);
            if elem.val_type() == ValType::F64 {
                i.i64_reinterpret_f64();
            }
        }
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        {
            let mut i = self.f.instructions();
            i.call(push).local_set(hacc);
            i.end();
            i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
            i.br(0).end().end();
        }
        // The kept elements are COPIES of the source's handles.
        self.emit_inc_elems(hacc, elem);
        self.f.instructions().local_get(hacc);
        self.release_val(elem);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(h))))
    }

    /// sep between every pair (native intersperse).
    pub(crate) fn lower_list_intersperse(
        &mut self,
        xs: &IrExpr,
        sep: &IrExpr,
    ) -> ArmResult {
        let h = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-intersperse-of:{other:?}")),
        };
        let elem = self.types.el(h);
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower_arg(sep, Some(elem), ArgMode::Borrow)?;
        let hx = self.hold_val(elem)?;
        self.f.instructions().local_set(hx);
        let hc = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hc);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
            i.local_get(hc).if_(BlockType::Empty);
            i.local_get(hacc).local_get(hx);
            if elem.val_type() == ValType::F64 {
                i.i64_reinterpret_f64();
            }
            i.call(push).local_set(hacc);
            i.end();
            i.local_get(hacc);
            i.local_get(hb).local_get(hc).i32_add();
        }
        self.load_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            if elem.val_type() == ValType::F64 {
                i.i64_reinterpret_f64();
            }
            i.call(push).local_set(hacc);
            i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
            i.br(0).end().end();
        }
        // Every slot — the copied elements and each occurrence of the
        // separator — holds its own credit.
        self.emit_inc_elems(hacc, elem);
        self.f.instructions().local_get(hacc);
        self.release_i32();
        self.release_i32();
        self.release_val(elem);
        self.release_i32();
        Ok(Some(Lowered::owned(SliceTy::List(h))))
    }
}
