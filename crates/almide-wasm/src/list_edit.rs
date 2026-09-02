//! Structural list edits (remove_at/reverse) — split from list_order.rs
//! for the complexity budget; list_order's dispatcher forwards here.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Native `if (i as usize) < len { xs.remove(i) }`: OOB — negative
    /// wraps huge — is a NO-OP (C-034), never a trap. Two spans copied
    /// around the removed slot; OOB degenerates to span1 = whole,
    /// span2 = empty.
    pub(crate) fn lower_list_remove_at(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-remove-at-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(idx, Some(INT))?;
        let hn = self.hold_i64()?;
        let hl = self.hold_i32()?;
        let hin = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        // in_bounds = 0 <= idx < len/stride (index domain — the
        // byte product would wrap for a huge idx)
        i.local_get(hn).i64_const(0).i64_ge_s();
        i.local_get(hn);
        i.local_get(hl).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_lt_s().i32_and().local_set(hin);
        // k = in_bounds ? idx*stride : len  (select: v1 first)
        i.local_get(hn).i64_const(i64::from(stride)).i64_mul().i32_wrap_i64();
        i.local_get(hl);
        i.local_get(hin).select().local_set(hk);
        // skip = in_bounds ? stride : 0; out = alloc(len - skip)
        i.local_get(hl);
        i.i32_const(stride).i32_const(0).local_get(hin).select();
        i.i32_sub().call(F_ALLOC).local_set(ho);
        // span 1: [0, k)
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk);
        i.memory_copy(0, 0);
        // span 2: [k+skip, len) lands at k
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hk).i32_add();
        i.i32_const(stride).i32_const(0).local_get(hin).select().i32_add();
        i.local_get(hl).local_get(hk).i32_sub();
        i.i32_const(stride).i32_const(0).local_get(hin).select().i32_sub();
        i.memory_copy(0, 0);
        i.local_get(ho);
        let _ = i;
        for _ in 0..5 {
            self.release_i32();
        }
        self.release_i64();
        Ok(Some(SliceTy::List(h)))
    }

    /// Fresh block, elements copied back-to-front (native `iter().rev()`);
    /// slot-width raw moves carry f64 bits and heap handles alike.
    pub(crate) fn lower_list_reverse(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-reverse-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        let hl = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        i.local_get(hl).call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hc);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hc).local_get(hl).i32_ge_u().br_if(1);
        // dst = out + (len - stride - c); src elem at c —
        // slot_memarg supplies the PAYLOAD offset on both sides.
        i.local_get(ho)
            .local_get(hl)
            .i32_add()
            .i32_const(stride)
            .i32_sub()
            .local_get(hc)
            .i32_sub();
        i.local_get(hb).local_get(hc).i32_add();
        if stride == 8 {
            i.i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
        } else {
            i.i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
        }
        i.local_get(hc).i32_const(stride).i32_add().local_set(hc);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(h)))
    }

    /// Native `if let Some(s) = r.get_mut(i) { *s = x }` over a fresh
    /// copy: OOB — negative wraps huge — leaves the copy untouched. The
    /// value ALWAYS evaluates (native argument order).
    pub(crate) fn lower_list_set(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
        v: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-set-of:{other:?}")),
        };
        let et = self.types.el(h);
        let stride = et.slot_size() as i32;
        self.f.instructions().call(F_BLOCK_COPY);
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(idx, Some(INT))?;
        let hn = self.hold_i64()?;
        self.f.instructions().local_set(hn);
        self.lower(v, Some(et))?;
        self.rc_map_value_share(v, et);
        enum Hv {
            I64(u32),
            F64(u32),
            I32(u32),
        }
        let hv = match et.val_type() {
            ValType::I64 => Hv::I64(self.hold_i64()?),
            ValType::F64 => Hv::F64(self.hold_f64()?),
            _ => Hv::I32(self.hold_i32()?),
        };
        let (Hv::I64(hvi) | Hv::F64(hvi) | Hv::I32(hvi)) = hv;
        self.f.instructions().local_set(hvi);
        let mut i = self.f.instructions();
        i.local_get(hn).i64_const(0).i64_ge_s();
        i.local_get(hn);
        i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_lt_s().i32_and().if_(BlockType::Empty);
        i.local_get(hb)
            .local_get(hn)
            .i32_wrap_i64()
            .i32_const(stride)
            .i32_mul()
            .i32_add()
            .local_get(hvi);
        let _ = i;
        self.store_ty_slot(et, 0);
        self.f.instructions().end().local_get(hb);
        match hv {
            Hv::I64(_) => self.release_i64(),
            Hv::F64(_) => self.release_f64(),
            Hv::I32(_) => self.release_i32(),
        }
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::List(h)))
    }

    /// Last n elements (native `n as usize >= len ? whole : tail`):
    /// a NEGATIVE n reinterprets huge and takes the WHOLE list.
    pub(crate) fn lower_list_take_end(
        &mut self,
        xs: &IrExpr,
        n: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-take-end-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(n, Some(INT))?;
        let hn = self.hold_i64()?;
        let hl = self.hold_i32()?;
        let hst = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        // start = big ? 0 : len - n*stride (index-domain big test
        // first — the byte product wraps for a huge n)
        i.i32_const(0);
        i.local_get(hl);
        i.local_get(hn).i64_const(i64::from(stride)).i64_mul().i32_wrap_i64();
        i.i32_sub();
        i.local_get(hn).i64_const(0).i64_lt_s();
        i.local_get(hn);
        i.local_get(hl).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_ge_s().i32_or();
        i.select().local_set(hst);
        i.local_get(hl).local_get(hst).i32_sub().call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hst)
            .i32_add();
        i.local_get(hl).local_get(hst).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(ho);
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        self.release_i64();
        Ok(Some(SliceTy::List(h)))
    }

    /// All but the last n (native `n as usize >= len ? empty : head`):
    /// a NEGATIVE n reinterprets huge and drops EVERYTHING.
    pub(crate) fn lower_list_drop_end(
        &mut self,
        xs: &IrExpr,
        n: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-drop-end-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(n, Some(INT))?;
        let hn = self.hold_i64()?;
        let hl = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hb).i32_load(len_memarg()).local_set(hl);
        // end = big ? 0 : len - n*stride (index-domain big test first)
        i.i32_const(0);
        i.local_get(hl);
        i.local_get(hn).i64_const(i64::from(stride)).i64_mul().i32_wrap_i64();
        i.i32_sub();
        i.local_get(hn).i64_const(0).i64_lt_s();
        i.local_get(hn);
        i.local_get(hl).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_ge_s().i32_or();
        i.select().local_set(hend);
        i.local_get(hend).call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hend);
        i.memory_copy(0, 0);
        i.local_get(ho);
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        self.release_i64();
        Ok(Some(SliceTy::List(h)))
    }

    /// First position of an equal element (native `position(== x)`):
    /// some(i) or none. Scalar and String elements; Float is IEEE ==
    /// (NaN never matches), exactly the native PartialEq.
    pub(crate) fn lower_list_index_of(
        &mut self,
        xs: &IrExpr,
        x: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-index-of-of:{other:?}")),
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
            return unsup(&format!("list-index-of-elem:{elem:?}"));
        }
        let stride = elem.slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(x, Some(elem))?;
        let hx = self.hold_val(elem)?;
        self.f.instructions().local_set(hx);
        let hc = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hc);
        i.i32_const(0).local_set(hi);
        i.i32_const(0).local_set(hr);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hc).i32_ge_u().br_if(1);
        i.local_get(hb).local_get(hi).i32_const(stride).i32_mul().i32_add();
        let _ = i;
        self.load_ty_slot(elem, 0);
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
            // Compound handles: the type-directed deep `==`.
            _ => {
                let _ = i;
                self.emit_val_eq(elem)?;
                i = self.f.instructions();
            }
        }
        i.if_(BlockType::Empty);
        i.i32_const(8).call(F_ALLOC).local_tee(hr);
        i.local_get(hi).i64_extend_i32_u();
        i.i64_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.br(2);
        i.end();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(hr);
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        self.release_val(elem);
        self.release_i32();
        Ok(Some(SliceTy::Option(self.types.intern(INT))))
    }

    /// Native `if let Some(s) = get_mut(i) { *s = f(s.clone()) }` over a
    /// fresh copy: the callback runs ONLY in bounds, once.
    pub(crate) fn lower_list_update(
        &mut self,
        xs: &IrExpr,
        idx: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-update-of:{other:?}")),
        };
        let et = self.types.el(h);
        let stride = et.slot_size() as i32;
        self.f.instructions().call(F_BLOCK_COPY);
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(idx, Some(INT))?;
        let hn = self.hold_i64()?;
        let ha = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hn).i64_const(0).i64_ge_s();
        i.local_get(hn);
        i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_lt_s().i32_and().if_(BlockType::Empty);
        i.local_get(hb)
            .local_get(hn)
            .i32_wrap_i64()
            .i32_const(stride)
            .i32_mul()
            .i32_add()
            .local_set(ha);
        i.local_get(ha);
        let _ = i;
        self.load_ty_slot(et, 0);
        self.f.instructions().local_set(params[0]);
        self.lower(body, Some(et))?;
        let hv = self.hold_val(et)?;
        self.f.instructions().local_set(hv);
        self.f.instructions().local_get(ha).local_get(hv);
        self.store_ty_slot(et, 0);
        self.release_val(et);
        self.f.instructions().end().local_get(hb);
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::List(h)))
    }

    /// Native `if a < len && b < len { r.swap(a, b) }` over a fresh copy:
    /// EITHER index out of range (negative wraps huge) is a whole no-op.
    pub(crate) fn lower_list_swap(
        &mut self,
        xs: &IrExpr,
        ia: &IrExpr,
        ib: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let h = match self.lower(xs, None)? {
            SliceTy::List(h) => h,
            other => return unsup(&format!("list-swap-of:{other:?}")),
        };
        let stride = self.types.el(h).slot_size() as i32;
        self.f.instructions().call(F_BLOCK_COPY);
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(ia, Some(INT))?;
        let hi = self.hold_i64()?;
        self.f.instructions().local_set(hi);
        self.lower(ib, Some(INT))?;
        let hj = self.hold_i64()?;
        let hc = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let hq = self.hold_i32()?;
        let ht = self.hold_i64()?;
        let mut i = self.f.instructions();
        i.local_set(hj);
        i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hc);
        i.local_get(hi).i64_const(0).i64_ge_s();
        i.local_get(hi).local_get(hc).i64_extend_i32_u().i64_lt_s().i32_and();
        i.local_get(hj).i64_const(0).i64_ge_s().i32_and();
        i.local_get(hj).local_get(hc).i64_extend_i32_u().i64_lt_s().i32_and();
        i.if_(BlockType::Empty);
        i.local_get(hb).local_get(hi).i32_wrap_i64().i32_const(stride).i32_mul().i32_add();
        i.local_set(hp);
        i.local_get(hb).local_get(hj).i32_wrap_i64().i32_const(stride).i32_mul().i32_add();
        i.local_set(hq);
        // tmp = *p; *p = *q; *q = tmp — raw slot-width moves
        if stride == 8 {
            i.local_get(hp).i64_load(slot_memarg(0)).local_set(ht);
            i.local_get(hp).local_get(hq).i64_load(slot_memarg(0)).i64_store(slot_memarg(0));
            i.local_get(hq).local_get(ht).i64_store(slot_memarg(0));
        } else {
            i.local_get(hp).i32_load(slot_memarg(0)).i64_extend_i32_u().local_set(ht);
            i.local_get(hp).local_get(hq).i32_load(slot_memarg(0)).i32_store(slot_memarg(0));
            i.local_get(hq).local_get(ht).i32_wrap_i64().i32_store(slot_memarg(0));
        }
        i.end();
        i.local_get(hb);
        let _ = i;
        self.release_i64();
        for _ in 0..3 {
            self.release_i32();
        }
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::List(h)))
    }
}
