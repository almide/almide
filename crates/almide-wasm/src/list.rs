//! List-op lowering (module surface + inlined HOF machinery) — split
//! from calls.rs for the complexity budget.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `list.*` special forms over the runtime helpers.
    pub(crate) fn lower_list_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        let _ = &ret_hint;
        if let Some(out) = self.lower_list_order_call(func, args)? {
            return Ok(out);
        }
        if let Some(out) = self.lower_list_mut_call(func, args, ret_hint)? {
            return Ok(out);
        }
        match (func, args) {
            ("len", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("list-len-of:{other:?}")),
                };
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(elem.slot_size() as i32)
                    .i32_div_u()
                    .i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("get", [xs, idx]) => {
                let h = match self.lower(xs, None)? {
                    SliceTy::List(h) => h,
                    other => return unsup(&format!("list-get-of:{other:?}")),
                };
                self.lower(idx, Some(INT))?;
                let helper = match self.types.el(h).slot_size() {
                    8 => F_LIST_GET_8,
                    _ => F_LIST_GET_4,
                };
                self.f.instructions().call(helper);
                Ok(Some(SliceTy::Option(h)))
            }
            ("get_or", [xs, idx, default]) => self.lower_list_get_or(xs, idx, default),
            // first = get(xs, 0) — the same Option-returning helper.
            ("first", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("list-first-of:{other:?}")),
                };
                self.f.instructions().i64_const(0);
                let helper = match elem.slot_size() {
                    8 => F_LIST_GET_8,
                    _ => F_LIST_GET_4,
                };
                self.f.instructions().call(helper);
                Ok(Some(SliceTy::Option(self.types.intern(elem))))
            }
            ("join", [xs, sep]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == STR => {}
                    other => return unsup(&format!("list-join-of:{other:?}")),
                }
                self.lower(sep, Some(STR))?;
                self.f.instructions().call(F_LIST_JOIN);
                Ok(Some(STR))
            }
            ("enumerate", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("list-enumerate-of:{other:?}")),
                };
                let pair_ti = self.types.tuple(vec![INT, elem]);
                let pdef = self.types.tuple_def(pair_ti);
                let (ioff, eoff, psize) =
                    (pdef.fields[0].1, pdef.fields[1].1, pdef.size);
                let stride = elem.slot_size();
                let bh = self.hold_i32()?;
                let ch = self.hold_i32()?;
                let ih = self.hold_i32()?;
                let rh = self.hold_i32()?;
                let ph = self.hold_i32()?;
                self.f.instructions().local_tee(bh);
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(ch)
                    .i32_const(0)
                    .local_set(ih);
                // result list of pair addresses
                self.f
                    .instructions()
                    .local_get(ch)
                    .i32_const(4)
                    .i32_mul()
                    .call(F_ALLOC)
                    .local_set(rh);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                // pair block
                self.f.instructions().i32_const(psize as i32).call(F_ALLOC).local_set(ph);
                self.f
                    .instructions()
                    .local_get(ph)
                    .local_get(ih)
                    .i64_extend_i32_u();
                self.store_ty_slot(INT, ioff);
                self.f.instructions().local_get(ph);
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(elem, 0);
                self.store_ty_slot(elem, eoff);
                // store pair addr into result
                self.f
                    .instructions()
                    .local_get(rh)
                    .local_get(ih)
                    .i32_const(4)
                    .i32_mul()
                    .i32_add()
                    .local_get(ph);
                self.store_ty_slot(SliceTy::Tuple(pair_ti), 0);
                self.f
                    .instructions()
                    .local_get(ih)
                    .i32_const(1)
                    .i32_add()
                    .local_set(ih)
                    .br(0)
                    .end()
                    .end();
                self.f.instructions().local_get(rh);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::List(self.types.intern(SliceTy::Tuple(pair_ti)))))
            }
            ("slice", [xs, a, b]) => {
                let (h, elem) = match self.lower(xs, None)? {
                    SliceTy::List(h) => (h, self.types.el(h)),
                    other => return unsup(&format!("list-slice-of:{other:?}")),
                };
                let stride = elem.slot_size() as i64;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(a, Some(INT))?;
                let ah = self.hold_i64()?;
                self.f.instructions().local_set(ah);
                self.lower(b, Some(INT))?;
                let eh = self.hold_i64()?;
                // e = min(b, count); s = a; s < 0 or s >= e → []
                let mut ins = self.f.instructions();
                ins.local_tee(eh);
                ins.local_get(bh)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_const(stride)
                    .i64_div_s();
                ins.local_get(eh);
                ins.local_get(bh)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_const(stride)
                    .i64_div_s();
                ins.i64_lt_s().select().local_set(eh);
                // empty when a < 0 (usize-wrap semantics) or a >= e
                ins.local_get(ah).i64_const(0).i64_lt_s();
                ins.local_get(ah).local_get(eh).i64_ge_s();
                ins.i32_or().if_(BlockType::Result(ValType::I32));
                ins.i32_const(0).call(F_ALLOC);
                ins.else_();
                // alloc (e-a)*stride; copy from base + a*stride
                ins.local_get(eh)
                    .local_get(ah)
                    .i64_sub()
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64()
                    .call(F_ALLOC)
                    .local_tee(self.tmp_i32_local)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add();
                ins.local_get(bh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(ah)
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64()
                    .i32_add();
                ins.local_get(eh)
                    .local_get(ah)
                    .i64_sub()
                    .i64_const(stride)
                    .i64_mul()
                    .i32_wrap_i64();
                ins.memory_copy(0, 0);
                ins.local_get(self.tmp_i32_local);
                ins.end();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(SliceTy::List(h)))
            }
            ("map", [xs, cb]) => self.lower_list_map(xs, cb),
            ("find", [xs, cb]) => {
                let (params, body) = self.hof_lambda(cb, 1)?;
                let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
                let rh = self.hold_i32()?;
                self.f.instructions().i32_const(0).local_set(rh);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.hof_elem_into(elem, bh, ch, ih, params[0]);
                self.lower(body, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Empty);
                // some(x): the first match wins, then break the scan
                self.f
                    .instructions()
                    .i32_const(elem.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(rh)
                    .local_get(params[0]);
                self.store_ty_slot(elem, almide_layout::OPTION_FIELD);
                self.f.instructions().br(2);
                self.f.instructions().end();
                self.hof_step(ih);
                self.f.instructions().local_get(rh);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::Option(self.types.intern(elem))))
            }
            // sort_by: the key fn runs ONCE PER ELEMENT (native
            // sort_by_cached_key, #560 — per-comparison was an observable
            // divergence for side-effectful keys), then the same stable
            // insertion sort as list.sort moves keys and values in
            // lockstep.
            ("filter", [xs, cb]) => self.lower_list_filter(xs, cb),
            // Option-returning map: none (null handle) skips, some's
            // payload collects in order — fan.map's collect idiom minus
            // the short-circuit.
            ("filter_map", [xs, cb]) => {
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
                self.f.instructions().call(push).local_set(hacc).end();
                self.hof_step(ih);
                self.f.instructions().local_get(hacc);
                for _ in 0..5 {
                    self.release_i32();
                }
                Ok(Some(SliceTy::List(self.types.intern(b))))
            }
            ("fold", [xs, init, cb]) => {
                if let Some(out) = self.lower_list_fold_fused(xs, init, cb)? {
                    return Ok(out);
                }
                self.lower_list_fold(xs, init, cb)
            }
            _ => unsup(&format!("call:list.{func}")),
        }
    }

    /// A literal-lambda HOF callback: (param locals, body). Fn-typed
    /// VALUES are a later mechanism — the direct-lambda form is the
    /// dominant idiom (153:31 in the corpus) and inlines with zero
    /// closure machinery: captures are just enclosing locals in scope.
    pub(crate) fn hof_lambda<'e>(
        &mut self,
        cb: &'e IrExpr,
        arity: usize,
    ) -> Result<(Vec<u32>, &'e IrExpr), EmitError> {
        let IrExprKind::Lambda { params, body, .. } = &cb.kind else {
            return unsup("list-hof-nonlambda");
        };
        if params.len() != arity {
            return unsup("list-hof-arity");
        }
        let mut idxs = Vec::new();
        for (var, _) in params {
            let Some(&(idx, _)) = self.locals.get(var) else {
                return unsup("bind:unmapped");
            };
            idxs.push(idx);
        }
        Ok((idxs, body))
    }

    /// Shared loop header: xs → holds (base, count, idx); returns them.
    pub(crate) fn hof_loop_open(
        &mut self,
        xs: &IrExpr,
    ) -> Result<(SliceTy, u32, u32, u32), EmitError> {
        let elem = match self.lower(xs, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-hof-of:{other:?}")),
        };
        let bh = self.hold_i32()?;
        let ch = self.hold_i32()?;
        let ih = self.hold_i32()?;
        self.f.instructions().local_tee(bh);
        self.f
            .instructions()
            .i32_load(len_memarg())
            .i32_const(elem.slot_size() as i32)
            .i32_div_u()
            .local_set(ch)
            .i32_const(0)
            .local_set(ih);
        Ok((elem, bh, ch, ih))
    }

    /// Loop-body prologue: guard + load current element into `param`.
    pub(crate) fn hof_elem_into(&mut self, elem: SliceTy, bh: u32, ch: u32, ih: u32, param: u32) {
        self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(elem.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.load_ty_slot(elem, 0);
        self.f.instructions().local_set(param);
    }

    pub(crate) fn hof_step(&mut self, ih: u32) {
        self.f.instructions().local_get(ih).i32_const(1).i32_add().local_set(ih).br(0).end().end();
    }


    fn lower_list_map(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let Some(u) = slice_ty_of(&body.ty, self.types) else {
            return unsup(&format!("list-map-ret:{}", ty_name(&body.ty)));
        };
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        // result = alloc(count * stride_u), same element count as source
        let rh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(ch)
            .i32_const(u.slot_size() as i32)
            .i32_mul()
            .call(F_ALLOC)
            .local_set(rh);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        // dest addr, then value, then store
        self.f
            .instructions()
            .local_get(rh)
            .local_get(ih)
            .i32_const(u.slot_size() as i32)
            .i32_mul()
            .i32_add();
        self.lower(body, Some(u))?;
        self.store_ty_slot(u, 0);
        self.hof_step(ih);
        self.f.instructions().local_get(rh);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(u))))
    }

    fn lower_list_filter(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let stride = elem.slot_size() as i32;
        // ONE upper-bound allocation, then a kept-counter and a final len
        // rewrite — the push-per-kept form re-copied the block per element
        // (quadratic bytes; the perf probe caught it at 3.8x).
        let rh = self.hold_i32()?;
        let hw = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(ch)
            .i32_const(stride)
            .i32_mul()
            .call(F_ALLOC)
            .local_set(rh);
        self.f.instructions().i32_const(0).local_set(hw);
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        self.f
            .instructions()
            .local_get(rh)
            .local_get(hw)
            .i32_const(stride)
            .i32_mul()
            .i32_add()
            .local_get(params[0]);
        self.store_ty_slot(elem, 0);
        self.f.instructions().local_get(hw).i32_const(1).i32_add().local_set(hw);
        self.f.instructions().end();
        self.hof_step(ih);
        // len = cap = kept*stride
        {
            let mut i = self.f.instructions();
            i.local_get(rh).local_get(hw).i32_const(stride).i32_mul().i32_store(len_memarg());
            i.local_get(rh)
                .local_get(hw)
                .i32_const(stride)
                .i32_mul()
                .i32_store(MemArg {
                    offset: u64::from(almide_layout::CAP.offset),
                    align: 2,
                    memory_index: 0,
                });
            i.local_get(rh);
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }

    fn lower_list_fold(
        &mut self,
        xs: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (acc_p, x_p) = (params[0], params[1]);
        let Some(b) = slice_ty_of(&init.ty, self.types) else {
            return unsup(&format!("list-fold-acc:{}", ty_name(&init.ty)));
        };
        self.lower(init, Some(b))?;
        self.f.instructions().local_set(acc_p);
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, x_p);
        self.lower(body, Some(b))?;
        self.f.instructions().local_set(acc_p);
        self.hof_step(ih);
        self.f.instructions().local_get(acc_p);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(b))
    }

    /// `list.get_or(xs, i, d)`: (xs.get(i)) ?? d, inlined via the get
    /// helper — extracted for complexity budget.
    pub(crate) fn lower_list_get_or(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
        default: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let elem = match self.lower(xs, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-get-of:{other:?}")),
        };
        self.lower(idx, Some(INT))?;
        let helper = match elem.slot_size() {
            8 => F_LIST_GET_8,
            _ => F_LIST_GET_4,
        };
        self.f
            .instructions()
            .call(helper)
            .local_tee(self.scr_i32_local)
            .i32_eqz()
            .if_(BlockType::Result(elem.val_type()));
        self.lower(default, Some(elem))?;
        self.f.instructions().else_().local_get(self.scr_i32_local);
        self.load_ty_slot(elem, almide_layout::OPTION_FIELD);
        self.f.instructions().end();
        Ok(Some(elem))
    }
}
