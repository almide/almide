//! List-op lowering (module surface + inlined HOF machinery) — split
//! from calls.rs for the complexity budget.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `list.*` special forms over the runtime helpers.
    pub(crate) fn lower_list_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
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
            // `list.push` MUTATES through its `mut` param on the oracle
            // (the growth fixture pushes as bare statements). Lowered as a
            // write-back: var = $push(var, v). Requires a plain var arg.
            ("push", [xs, v]) => {
                let IrExprKind::Var { id } = &xs.kind else {
                    return unsup("list-push-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let SliceTy::List(h) = var_ty else {
                    return unsup(&format!("list-push-of:{var_ty:?}"));
                };
                let elem = self.types.el(h);
                self.f.instructions().local_get(var_idx);
                self.lower(v, Some(elem))?;
                // The 8-byte helper's value param is i64; an f64 element
                // crosses the call boundary as its BIT PATTERN (memory is
                // bytes — the consumer reloads the slot as f64).
                if elem.val_type() == wasm_encoder::ValType::F64 {
                    self.f.instructions().i64_reinterpret_f64();
                }
                let helper = match elem.slot_size() {
                    8 => F_LIST_PUSH_8,
                    _ => F_LIST_PUSH_4,
                };
                self.f.instructions().call(helper).local_set(var_idx);
                Ok(None)
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
            ("filter", [xs, cb]) => self.lower_list_filter(xs, cb),
            ("fold", [xs, init, cb]) => self.lower_list_fold(xs, init, cb),
            // ONE allocation, zero copies: the linked self-host impl binds
            // its buffer, and the bind deep-copy doubles the footprint —
            // the C-169 boundary (2^28 slots = 2^31 bytes) then needs 4 GiB
            // and traps where the contract requires success. Semantics
            // verbatim from stdlib/list_make.almd list_repeat: over-ceiling
            // dies in the T6 form, a negative count clamps to empty (C-054),
            // and a block-typed element repeats as the SHARED word (vec![x; n]
            // clones the handle; no-in-place-mutation makes it unobservable).
            ("repeat", [x, n]) => {
                let elem = self.infer(x)?;
                let stride = elem.slot_size();
                self.lower(x, Some(elem))?;
                enum Hx {
                    I64(u32),
                    F64(u32),
                    I32(u32),
                }
                let hx = match elem.val_type() {
                    ValType::I64 => {
                        let h = self.hold_i64()?;
                        self.f.instructions().local_set(h);
                        Hx::I64(h)
                    }
                    ValType::F64 => {
                        let h = self.hold_f64()?;
                        self.f.instructions().local_set(h);
                        Hx::F64(h)
                    }
                    _ => {
                        let h = self.hold_i32()?;
                        self.f.instructions().local_set(h);
                        Hx::I32(h)
                    }
                };
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let hb = self.hold_i32()?;
                let hc = self.hold_i32()?;
                let he = self.hold_i32()?;
                let msg = self.pool.intern("repeat result too large");
                {
                    let mut i = self.f.instructions();
                    i.local_set(hn);
                    i.local_get(hn).i64_const(268435456).i64_gt_s();
                    i.if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                {
                    let mut i = self.f.instructions();
                    i.end();
                    i.i64_const(0)
                        .local_get(hn)
                        .local_get(hn)
                        .i64_const(0)
                        .i64_lt_s()
                        .select()
                        .local_set(hn);
                    i.local_get(hn)
                        .i64_const(i64::from(stride))
                        .i64_mul()
                        .i32_wrap_i64()
                        .call(F_ALLOC)
                        .local_tee(hb)
                        .local_set(hc);
                    i.local_get(hb)
                        .local_get(hn)
                        .i32_wrap_i64()
                        .i32_const(stride as i32)
                        .i32_mul()
                        .i32_add()
                        .local_set(he);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hc).local_get(he).i32_ge_u().br_if(1);
                    i.local_get(hc);
                    match hx {
                        Hx::I64(h) | Hx::F64(h) | Hx::I32(h) => i.local_get(h),
                    };
                }
                self.store_ty_slot(elem, 0);
                {
                    let mut i = self.f.instructions();
                    i.local_get(hc).i32_const(stride as i32).i32_add().local_set(hc);
                    i.br(0);
                    i.end();
                    i.end();
                    i.local_get(hb);
                }
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i64();
                match hx {
                    Hx::I64(_) => self.release_i64(),
                    Hx::F64(_) => self.release_f64(),
                    Hx::I32(_) => self.release_i32(),
                }
                Ok(Some(SliceTy::List(self.types.intern(elem))))
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
    fn hof_loop_open(
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
    fn hof_elem_into(&mut self, elem: SliceTy, bh: u32, ch: u32, ih: u32, param: u32) {
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

    fn hof_step(&mut self, ih: u32) {
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
        let rh = self.hold_i32()?;
        self.f.instructions().i32_const(0).call(F_ALLOC).local_set(rh); // []
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        self.f
            .instructions()
            .local_get(rh)
            .local_get(params[0])
            .call(push)
            .local_set(rh);
        self.f.instructions().end();
        self.hof_step(ih);
        self.f.instructions().local_get(rh);
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
