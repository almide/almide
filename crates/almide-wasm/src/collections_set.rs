//! Set lowering — split from collections.rs for the file budget;
//! the insertion-order doctrine and entry machinery live there.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_set_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("union" | "intersection" | "difference", [a, b]) => {
                self.lower_set_algebra(func, a, b)
            }
            // filter: order-preserving subset — the list machinery applies
            // verbatim (hof_loop_open accepts Set), only the tag differs.
            ("filter", [s, cb]) => match self.lower_list_filter(s, cb)? {
                Some(SliceTy::List(h)) => Ok(Some(SliceTy::Set(h))),
                other => Ok(other),
            },
            // is_subset: every member of a found in b; is_disjoint: none.
            ("is_subset" | "is_disjoint", [a, b]) => self.lower_set_relation(func, a, b),
            // remove: the set minus one member (a plain copy when absent).
            ("remove", [s, x]) => self.lower_set_remove(s, x),
            // (a − b) ++ (b − a): the two filters are DISJOINT, so the
            // concatenation is already deduped.
            ("symmetric_difference", [a, b]) => self.lower_set_symdiff(a, b),
            // Transform + first-seen dedup (a set stays a set).
            ("map", [s, cb]) => self.lower_set_hof_map(s, cb),
            _ => self.lower_set_call_b(func, args, ret_hint),
        }
    }

    /// Evaluate set + needle, run the scan. Returns ALL holds explicitly
    /// — (set, needle, entry, elem ty). Release: entry, needle, set.
    /// union: a's order + b's new members appended (in b order);
    /// intersection/difference: a's members (dis)qualified by b, in a's
    /// order — the native filter/insert walks, verbatim.
    fn lower_set_algebra(
        &mut self,
        func: &str,
        a: &IrExpr,
        b: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let e = match self.lower(a, None)? {
            SliceTy::Set(h) => self.types.el(h),
            other => return unsup(&format!("set-op-of:{other:?}")),
        };
        let ah = self.hold_i32()?;
        self.f.instructions().local_set(ah);
        match self.lower(b, None)? {
            SliceTy::Set(h) if self.types.el(h) == e => {}
            other => return unsup(&format!("set-algebra-of:{other:?}")),
        }
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        let scan = self.scan_helper(e)?;
        let stride = e.slot_size() as i32;
        let union = func == "union";
        let keep_found = func == "intersection";
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hx = self.hold_for(e)?;
        {
            let mut i = self.f.instructions();
            if union {
                // count b's NEW members (bytes) into hw
                i.i32_const(0).local_set(hw);
                i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
                i.local_get(hcur).local_get(bh).i32_load(len_memarg()).i32_add().local_set(hend);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
                i.local_get(hcur);
            }
        }
        if union {
            self.load_ty_slot_at(e);
            let mut i = self.f.instructions();
            i.local_set(hx);
            i.local_get(ah).i32_const(stride).i32_const(0).local_get(hx);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hw).i32_const(stride).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
            i.br(0).end().end();
            // out = a wholesale + the new members appended
            i.local_get(ah).i32_load(len_memarg()).local_get(hw).i32_add();
            i.call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ah).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ah).i32_load(len_memarg());
            i.memory_copy(0, 0);
            i.local_get(ho)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(ah)
                .i32_load(len_memarg())
                .i32_add()
                .local_set(hw);
            i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur);
            let _ = i;
            self.load_ty_slot_at(e);
            let mut i = self.f.instructions();
            i.local_set(hx);
            i.local_get(ah).i32_const(stride).i32_const(0).local_get(hx);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hw).local_get(hcur).i32_const(stride);
            i.memory_copy(0, 0);
            i.local_get(hw).i32_const(stride).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(ho);
        } else {
            // intersection/difference: over-allocate a's len, keep the
            // (dis)qualified members, patch the len header down.
            let mut i = self.f.instructions();
            i.local_get(ah).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
            i.local_get(ah).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(ah).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur);
            let _ = i;
            self.load_ty_slot_at(e);
            let mut i = self.f.instructions();
            i.local_set(hx);
            i.local_get(bh).i32_const(stride).i32_const(0).local_get(hx);
            i.call(scan);
            if keep_found {
                i.i32_eqz();
            }
            i.i32_eqz().if_(BlockType::Empty);
            i.local_get(hw).local_get(hcur).i32_const(stride);
            i.memory_copy(0, 0);
            i.local_get(hw).i32_const(stride).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(ho);
            i.local_get(hw).local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_sub();
            i.i32_store(len_memarg());
            i.local_get(ho);
        }
        self.release_for(e);
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Set(self.types.intern(e))))
    }

    /// is_subset: every member of a found in b; is_disjoint: none.
    fn lower_set_relation(&mut self, func: &str, a: &IrExpr, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let want_found = i32::from(func == "is_subset");
        let e = match self.lower(a, None)? {
            SliceTy::Set(h) => self.types.el(h),
            other => return unsup(&format!("set-op-of:{other:?}")),
        };
        let ah = self.hold_i32()?;
        self.f.instructions().local_set(ah);
        match self.lower(b, None)? {
            SliceTy::Set(h) if self.types.el(h) == e => {}
            other => return unsup(&format!("set-rel-of:{other:?}")),
        }
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        let scan = self.scan_helper(e)?;
        let stride = e.slot_size() as i32;
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hres = self.hold_i32()?;
        let hx = self.hold_for(e)?;
        let mut i = self.f.instructions();
        i.i32_const(1).local_set(hres);
        i.local_get(ah)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_set(hcur);
        i.local_get(hcur).local_get(ah).i32_load(len_memarg()).i32_add().local_set(hend);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
        i.local_get(hcur);
        let _ = i;
        self.load_ty_slot_at(e);
        let mut i = self.f.instructions();
        i.local_set(hx);
        i.local_get(bh).i32_const(stride).i32_const(0).local_get(hx);
        i.call(scan).i32_const(0).i32_ne();
        i.i32_const(want_found).i32_ne().if_(BlockType::Empty);
        i.i32_const(0).local_set(hres);
        i.br(2);
        i.end();
        i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
        i.br(0).end().end();
        i.local_get(hres);
        let _ = i;
        self.release_for(e);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(BOOL))
    }

    /// remove: the set minus one member (a plain copy when absent).
    fn lower_set_remove(&mut self, s: &IrExpr, x: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let (sh, _xh, eh, e) = self.set_scan(s, x)?;
        let stride = e.slot_size() as i32;
        let ho = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(eh).i32_eqz().if_(BlockType::Empty);
        i.local_get(sh).call(F_BLOCK_COPY).local_set(ho);
        i.else_();
        i.local_get(eh)
            .local_get(sh)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .i32_sub()
            .local_set(hp);
        i.local_get(sh).i32_load(len_memarg()).i32_const(stride).i32_sub();
        i.call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(sh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hp);
        i.memory_copy(0, 0);
        i.local_get(ho)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hp)
            .i32_add();
        i.local_get(eh).i32_const(stride).i32_add();
        i.local_get(sh)
            .i32_load(len_memarg())
            .local_get(hp)
            .i32_sub()
            .i32_const(stride)
            .i32_sub();
        i.memory_copy(0, 0);
        i.end();
        i.local_get(ho);
        let _ = i;
        self.release_i32();
        self.release_i32();
        self.release_i32(); // eh
        self.release_for(e);
        self.release_i32(); // sh
        Ok(Some(SliceTy::Set(self.types.intern(e))))
    }

    /// (a - b) ++ (b - a): the two filters are DISJOINT, so the
    /// concatenation is already deduped.
    fn lower_set_symdiff(&mut self, a: &IrExpr, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let e = match self.lower(a, None)? {
            SliceTy::Set(h) => self.types.el(h),
            other => return unsup(&format!("set-op-of:{other:?}")),
        };
        let ah = self.hold_i32()?;
        self.f.instructions().local_set(ah);
        match self.lower(b, None)? {
            SliceTy::Set(h) if self.types.el(h) == e => {}
            other => return unsup(&format!("set-symdiff-of:{other:?}")),
        }
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        let scan = self.scan_helper(e)?;
        let stride = e.slot_size() as i32;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hx = self.hold_for(e)?;
        {
            let mut i = self.f.instructions();
            i.local_get(ah)
                .i32_load(len_memarg())
                .local_get(bh)
                .i32_load(len_memarg())
                .i32_add()
                .call(F_ALLOC)
                .local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
        }
        for (src, other) in [(ah, bh), (bh, ah)] {
            let mut i = self.f.instructions();
            i.local_get(src)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_set(hcur);
            i.local_get(hcur).local_get(src).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur);
            let _ = i;
            self.load_ty_slot_at(e);
            let mut i = self.f.instructions();
            i.local_set(hx);
            i.local_get(other).i32_const(stride).i32_const(0).local_get(hx);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hw).local_get(hcur).i32_const(stride);
            i.memory_copy(0, 0);
            i.local_get(hw).i32_const(stride).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
            i.br(0).end().end();
        }
        {
            let mut i = self.f.instructions();
            i.local_get(ho);
            i.local_get(hw)
                .local_get(ho)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .i32_sub();
            i.i32_store(len_memarg());
            i.local_get(ho);
        }
        self.release_for(e);
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Set(self.types.intern(e))))
    }

    /// Transform + first-seen dedup (a set stays a set).
    fn lower_set_hof_map(&mut self, s: &IrExpr, cb: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let e = match self.lower(s, None)? {
            SliceTy::Set(h) => self.types.el(h),
            other => return unsup(&format!("set-map-of:{other:?}")),
        };
        let b_ty = self.infer(body)?;
        let SliceTy::Scalar(_) = b_ty else { return unsup("set-map-elem-nonscalar") };
        let scan = self.scan_helper(b_ty)?;
        let stride = e.slot_size() as i32;
        let bstride = b_ty.slot_size() as i32;
        let sh = self.hold_i32()?;
        self.f.instructions().local_set(sh);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let hv = self.hold_for(b_ty)?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.local_get(sh)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_set(hcur);
            i.local_get(hcur).local_get(sh).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur);
        }
        self.load_ty_slot_at(e);
        self.f.instructions().local_set(params[0]);
        self.lower(body, Some(b_ty))?;
        {
            let mut i = self.f.instructions();
            i.local_set(hv);
            i.local_get(hacc).i32_const(bstride).i32_const(0).local_get(hv);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hacc).local_get(hv);
            if b_ty.val_type() == wasm_encoder::ValType::F64 {
                i.i64_reinterpret_f64();
            }
        }
        let push = match b_ty.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        {
            let mut i = self.f.instructions();
            i.call(push).local_set(hacc);
            i.end();
            i.local_get(hcur).i32_const(stride).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(hacc);
        }
        self.release_for(b_ty);
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::Set(self.types.intern(b_ty))))
    }

    fn set_scan(&mut self, s: &IrExpr, x: &IrExpr) -> Result<(u32, u32, u32, SliceTy), EmitError> {
        let e = match self.lower(s, None)? {
            SliceTy::Set(h) => self.types.el(h),
            other => return unsup(&format!("set-op-of:{other:?}")),
        };
        let sh = self.hold_i32()?;
        self.f.instructions().local_set(sh);
        let xh = self.hold_for(e)?;
        self.lower(x, Some(e))?;
        self.f.instructions().local_set(xh);
        let scan = self.scan_helper(e)?;
        let eh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(sh)
            .i32_const(e.slot_size() as i32)
            .i32_const(0)
            .local_get(xh)
            .call(scan)
            .local_set(eh);
        Ok((sh, xh, eh, e))
    }
}

impl Emitter<'_> {
    /// The second half of the `set.*` dispatch — split from
    /// `lower_set_call` for the complexity budget.
    fn lower_set_call_b(
        &mut self,
        func: &str,
        args: &[IrExpr],
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("new", []) => {
                let Some(ty @ SliceTy::Set(_)) = ret_hint else {
                    return unsup("set-new-needs-context");
                };
                self.f.instructions().i32_const(0).call(F_ALLOC);
                Ok(Some(ty))
            }
            ("len", [s]) => {
                let e = match self.lower(s, None)? {
                    SliceTy::Set(h) => self.types.el(h),
                    other => return unsup(&format!("set-op-of:{other:?}")),
                };
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(e.slot_size() as i32)
                    .i32_div_u()
                    .i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("to_list", [s]) => {
                // Layout-identical; sharing the base is unobservable
                // (no in-place list/set mutation exists, binds deep-copy).
                let e = match self.lower(s, None)? {
                    SliceTy::Set(h) => self.types.el(h),
                    other => return unsup(&format!("set-op-of:{other:?}")),
                };
                Ok(Some(SliceTy::List(self.types.intern(e))))
            }
            ("contains", [s, x]) => {
                let (_sh, _xh, eh, e) = self.set_scan(s, x)?;
                self.f.instructions().local_get(eh).i32_const(0).i32_ne();
                self.release_i32(); // eh
                self.release_for(e);
                self.release_i32(); // sh
                Ok(Some(BOOL))
            }
            ("insert", [s, x]) => {
                let (sh, xh, eh, e) = self.set_scan(s, x)?;
                self.f
                    .instructions()
                    .local_get(eh)
                    .i32_const(0)
                    .i32_ne()
                    .if_(BlockType::Result(wasm_encoder::ValType::I32));
                // already present: the functional result IS the input.
                self.f.instructions().local_get(sh);
                self.f.instructions().else_();
                let (len_h, rh) = self.emit_copy_grow(sh, e.slot_size())?;
                self.f
                    .instructions()
                    .local_get(rh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h)
                    .i32_add()
                    .local_get(xh);
                self.store_ty_slot_raw(e);
                self.f.instructions().local_get(rh);
                self.release_i32();
                self.release_i32();
                self.f.instructions().end();
                self.release_i32(); // eh
                self.release_for(e);
                self.release_i32(); // sh
                Ok(Some(SliceTy::Set(self.types.intern(e))))
            }
            // The set IS an insertion-ordered flat array (to_list is a
            // cast), so fold walks it exactly like map.fold walks entries.
            ("fold", [s, init, cb]) => {
                let (params, body) = self.hof_lambda(cb, 2)?;
                let (acc_p, x_p) = (params[0], params[1]);
                let Some(b) = slice_ty_of(&init.ty, self.types) else {
                    return unsup(&format!("set-fold-acc:{}", ty_name(&init.ty)));
                };
                self.lower(init, Some(b))?;
                self.f.instructions().local_set(acc_p);
                let e = match self.lower(s, None)? {
                    SliceTy::Set(h) => self.types.el(h),
                    other => return unsup(&format!("set-fold-of:{other:?}")),
                };
                let stride = e.slot_size() as i32;
                let bh = self.hold_i32()?;
                let cur = self.hold_i32()?;
                let end = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(bh);
                    i.local_get(bh)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur).local_get(bh).i32_load(len_memarg()).i32_add().local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(cur);
                }
                self.load_ty_slot_at(e);
                self.f.instructions().local_set(x_p);
                self.lower(body, Some(b))?;
                self.f.instructions().local_set(acc_p);
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                    i.local_get(acc_p);
                }
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(b))
            }
            ("from_list", [xs]) => {
                let e = match self.lower(xs, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("set-from-of:{other:?}")),
                };
                if !matches!(
                    e,
                    SliceTy::Scalar(_) | SliceTy::Tuple(_) | SliceTy::Named(_)
                ) {
                    return unsup("set-elem-nonscalar");
                }
                let stride = e.slot_size();
                let scan = self.scan_helper(e)?;
                let bh = self.hold_i32()?;
                let ch = self.hold_i32()?;
                let ih = self.hold_i32()?;
                let rh = self.hold_i32()?;
                let xh = self.hold_for(e)?;
                self.f.instructions().local_tee(bh);
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(ch)
                    .i32_const(0)
                    .local_set(ih)
                    .i32_const(0)
                    .call(F_ALLOC)
                    .local_set(rh);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(e, 0);
                self.f.instructions().local_set(xh);
                // dedup: append only when absent
                self.f
                    .instructions()
                    .local_get(rh)
                    .i32_const(stride as i32)
                    .i32_const(0)
                    .local_get(xh)
                    .call(scan)
                    .i32_eqz()
                    .if_(BlockType::Empty);
                let (len_h, nh) = self.emit_copy_grow(rh, stride)?;
                self.f
                    .instructions()
                    .local_get(nh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h)
                    .i32_add()
                    .local_get(xh);
                self.store_ty_slot_raw(e);
                self.f.instructions().local_get(nh).local_set(rh);
                self.release_i32();
                self.release_i32();
                self.f.instructions().end();
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
                self.release_for(e);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::Set(self.types.intern(e))))
            }
            _ => self.lower_linked_call("set", func, args, false),
        }
    }
}
