//! List combinator surfaces (last/contains/product/flatten/all/
//! take_while/reduce/scan/zip/zip_with/unique/intersperse) — split for
//! the file budget; semantics verbatim from runtime/rs.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// some(last) or none (native `xs.last().cloned()`).
    pub(crate) fn lower_list_last(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
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
        self.store_ty_slot(elem, almide_layout::OPTION_FIELD);
        self.f.instructions().local_get(hr).end();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::Option(self.types.intern(elem))))
    }

    /// Sequential product (native: Int wrapping_mul fold from 1;
    /// Float `iter().product()` — the same left fold from 1.0).
    pub(crate) fn lower_list_product(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
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
        Ok(Some(elem))
    }

    /// Concat of the inner lists, in order.
    pub(crate) fn lower_list_flatten(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-flatten-of:{other:?}")),
        };
        let SliceTy::List(inner) = self.types.el(h) else {
            return unsup("list-flatten-nonnested");
        };
        let hb = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.i32_const(0).call(F_ALLOC).local_set(hacc);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hb).i32_load(len_memarg()).i32_ge_u().br_if(1);
        i.local_get(hacc);
        i.local_get(hb).local_get(hc).i32_add().i32_load(slot_memarg(0));
        i.call(F_CONCAT).local_set(hacc);
        i.local_get(hc).i32_const(4).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(hacc);
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(inner)))
    }

    /// Sequential sum (native: Int wrapping fold from 0; Float
    /// `iter().sum()` — the same left fold from 0.0).
    pub(crate) fn lower_list_sum(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
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
        Ok(Some(elem))
    }

    /// CONSECUTIVE dedup (native `r.last() != Some(x)` — unlike unique,
    /// only adjacent equals fold).
    pub(crate) fn lower_list_dedup(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-dedup-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(elem, INT | FLOAT | STR | BOOL) {
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
                _ => {
                    i.i32_ne();
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
            i.local_get(hacc);
        }
        self.release_val(elem);
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(h)))
    }

    /// Suffix after the first false (native skip_while: the callback
    /// runs through the first failing element).
    pub(crate) fn lower_list_drop_while(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        let hd = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().i32_const(0).local_set(hd);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        // still dropping? run the callback; a false flips to keeping
        self.f.instructions().local_get(hd).i32_eqz().if_(BlockType::Empty);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().i32_eqz().local_set(hd);
        self.f.instructions().end();
        self.f.instructions().local_get(hd).if_(BlockType::Empty);
        self.f.instructions().local_get(hacc).local_get(params[0]);
        if elem.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        self.f.instructions().call(push).local_set(hacc);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }

    /// Exists (native `iter().any`): the first true wins, empty = false.
    pub(crate) fn lower_list_any(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(hr);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        self.f.instructions().i32_const(1).local_set(hr);
        self.f.instructions().br(2);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hr);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(BOOL))
    }

    /// Forall (native `iter().all`): the first false wins, empty = true.
    pub(crate) fn lower_list_all(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hr = self.hold_i32()?;
        self.f.instructions().i32_const(1).local_set(hr);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        self.f.instructions().i32_const(0).local_set(hr);
        self.f.instructions().br(2);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hr);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(BOOL))
    }

    /// Matching-element count (native `filter().count()` — every element
    /// runs through the callback, no early exit).
    pub(crate) fn lower_list_count(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hn = self.hold_i64()?;
        self.f.instructions().i64_const(0).local_set(hn);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        self.f.instructions().local_get(hn).i64_const(1).i64_add().local_set(hn);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(hn);
        self.release_i64();
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(Some(INT))
    }

    /// Prefix while true (the callback runs through the FIRST false).
    pub(crate) fn lower_list_take_while(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().i32_eqz().br_if(1);
        self.f.instructions().local_get(hacc).local_get(params[0]);
        if elem.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        self.f.instructions().call(push).local_set(hacc);
        self.hof_step(ih);
        self.f.instructions().local_get(hacc);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }

    /// some(fold-from-first) or none (native `into_iter().reduce`).
    pub(crate) fn lower_list_reduce(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
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
        self.load_ty_slot(elem, 0);
        self.f.instructions().local_set(params[0]);
        self.f.instructions().i32_const(1).local_set(ih);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[1]);
        self.lower(body, Some(elem))?;
        self.f.instructions().local_set(params[0]);
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
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Option(self.types.intern(elem))))
    }

    /// Running-accumulator list (native scan: push EVERY new acc).
    pub(crate) fn lower_list_scan(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let b_ty = self.infer(body)?;
        self.lower(init, Some(b_ty))?;
        self.f.instructions().local_set(params[0]);
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let hacc = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(hacc);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[1]);
        self.lower(body, Some(b_ty))?;
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
        Ok(Some(SliceTy::List(self.types.intern(b_ty))))
    }

    /// Pairs to the SHORTER length (native zip); zip_with maps the pair
    /// through the callback instead of building tuples.
    pub(crate) fn lower_list_zip(
        &mut self,
        a: &IrExpr,
        b: &IrExpr,
        cb: Option<&IrExpr>,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = match cb {
            Some(cb) => {
                let (p, bd) = self.hof_lambda(cb, 2)?;
                (p, Some(bd))
            }
            None => (Vec::new(), None),
        };
        let ea = match self.lower(a, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-zip-of:{other:?}")),
        };
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(ha);
        let eb = match self.lower(b, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-zip-of:{other:?}")),
        };
        let hb = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let (sa, sb) = (ea.slot_size() as i32, eb.slot_size() as i32);
        let out_ty = match body {
            Some(body) => self.infer(body)?,
            None => SliceTy::Tuple(self.types.tuple(vec![ea, eb])),
        };
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            // n = min(count_a, count_b)  (select: v1 first)
            i.local_get(ha).i32_load(len_memarg()).i32_const(sa).i32_div_u();
            i.local_get(hb).i32_load(len_memarg()).i32_const(sb).i32_div_u();
            i.local_get(ha).i32_load(len_memarg()).i32_const(sa).i32_div_u();
            i.local_get(hb).i32_load(len_memarg()).i32_const(sb).i32_div_u();
            i.i32_lt_u().select().local_set(hn);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
        }
        let val_ty = if let Some(body) = body {
            // load both elems into the callback params, run the body
            self.f.instructions().local_get(ha).local_get(hi).i32_const(sa).i32_mul().i32_add();
            self.load_ty_slot(ea, 0);
            self.f.instructions().local_set(params[0]);
            self.f.instructions().local_get(hb).local_get(hi).i32_const(sb).i32_mul().i32_add();
            self.load_ty_slot(eb, 0);
            self.f.instructions().local_set(params[1]);
            self.lower(body, Some(out_ty))?;
            out_ty
        } else {
            // build the (A, B) pair block
            let SliceTy::Tuple(ti) = out_ty else { unreachable!() };
            let def = self.types.tuple_def(ti);
            let hp = self.hold_i32()?;
            self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hp);
            self.f.instructions().local_get(hp);
            self.f.instructions().local_get(ha).local_get(hi).i32_const(sa).i32_mul().i32_add();
            self.load_ty_slot(ea, 0);
            self.store_ty_slot(ea, def.fields[0].1);
            self.f.instructions().local_get(hp);
            self.f.instructions().local_get(hb).local_get(hi).i32_const(sb).i32_mul().i32_add();
            self.load_ty_slot(eb, 0);
            self.store_ty_slot(eb, def.fields[1].1);
            self.f.instructions().local_get(hp);
            self.release_i32();
            out_ty
        };
        if val_ty.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        if val_ty.slot_size() == 8 {
            let hv = self.hold_i64()?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(hacc).local_get(hv).call(F_LIST_PUSH_8);
            self.f.instructions().local_set(hacc);
            self.release_i64();
        } else {
            let hv = self.hold_i32()?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(hacc).local_get(hv).call(F_LIST_PUSH_4);
            self.f.instructions().local_set(hacc);
            self.release_i32();
        }
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0).end().end();
            i.local_get(hacc);
        }
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(out_ty))))
    }

    /// First-seen order dedup (native nested contains walk).
    pub(crate) fn lower_list_unique(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-unique-of:{other:?}")),
        };
        let elem = self.types.el(h);
        if !matches!(elem, INT | FLOAT | STR | BOOL) {
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
                _ => {
                    i.i32_eq();
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
            i.local_get(hacc);
        }
        self.release_val(elem);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(h)))
    }

    /// sep between every pair (native intersperse).
    pub(crate) fn lower_list_intersperse(
        &mut self,
        xs: &IrExpr,
        sep: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-intersperse-of:{other:?}")),
        };
        let elem = self.types.el(h);
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(sep, Some(elem))?;
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
            i.local_get(hacc);
        }
        self.release_i32();
        self.release_i32();
        self.release_val(elem);
        self.release_i32();
        Ok(Some(SliceTy::List(h)))
    }
}
