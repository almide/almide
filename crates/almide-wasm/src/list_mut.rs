//! Mutating-form and construction list surfaces (push/pop/clear/
//! repeat/with_capacity/is_empty) — split from list.rs for the
//! complexity budget; the dispatcher falls through here second.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {

    /// mut pop: some(last) + shrunken-copy write-back.
    fn lower_list_pop(&mut self, xs: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        {

                let IrExprKind::Var { id } = &xs.kind else {
                    return unsup("list-pop-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let SliceTy::List(h) = var_ty else {
                    return unsup(&format!("list-pop-of:{var_ty:?}"));
                };
                let elem = self.types.el(h);
                let stride = elem.slot_size() as i32;
                let hb = self.hold_i32()?;
                let hlen = self.hold_i32()?;
                let hres = self.hold_i32()?;
                let hnew = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(var_idx).local_set(hb);
                    i.local_get(hb).i32_load(len_memarg()).local_set(hlen);
                    i.local_get(hlen).i32_eqz().if_(BlockType::Empty);
                    i.i32_const(0).local_set(hres);
                    i.else_();
                    // some(last)
                    i.i32_const(stride).call(F_ALLOC).local_set(hres);
                    i.local_get(hres);
                    i.local_get(hb).local_get(hlen).i32_add().i32_const(stride).i32_sub();
                }
                self.load_ty_slot(elem, 0);
                self.store_ty_slot(elem, almide_layout::OPTION_FIELD);
                {
                    let mut i = self.f.instructions();
                    // shrunken copy, write-back
                    i.local_get(hlen).i32_const(stride).i32_sub().call(F_ALLOC).local_set(hnew);
                    i.local_get(hnew).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                    i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                    i.local_get(hlen).i32_const(stride).i32_sub();
                    i.memory_copy(0, 0);
                    i.local_get(hnew).local_set(var_idx);
                    i.end();
                    i.local_get(hres);
                }
                for _ in 0..4 {
                    self.release_i32();
                }
                Ok(Some(SliceTy::Option(self.types.intern(elem))))
        }
    }

    pub(crate) fn lower_list_mut_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let _ = &ret_hint;
        match (func, args) {
            ("pop", [xs]) => self.lower_list_pop(xs),
            // mut form (native xs.clear()): rebind to the empty list.
            ("clear", [xs]) => {
                let IrExprKind::Var { id } = &xs.kind else {
                    return unsup("list-clear-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let SliceTy::List(_) = var_ty else {
                    return unsup(&format!("list-clear-of:{var_ty:?}"));
                };
                self.f.instructions().i32_const(0).call(F_ALLOC).local_set(var_idx);
                Ok(None)
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
            // Capacity is a HINT (native clamps it and the backing
            // buffer is unobservable) — the value is the empty list.
            ("with_capacity", [n]) => {
                let Some(SliceTy::List(h)) = ret_hint else {
                    return unsup("list-with-capacity-no-hint");
                };
                self.lower(n, Some(INT))?;
                self.f.instructions().drop().i32_const(0).call(F_ALLOC);
                Ok(Some(SliceTy::List(h)))
            }
            ("is_empty", [xs]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(_) => {}
                    other => return unsup(&format!("list-is-empty-of:{other:?}")),
                }
                self.f.instructions().i32_load(len_memarg()).i32_eqz();
                Ok(Some(BOOL))
            }
            _ => return Ok(None),
        }
        .map(Some)
    }
}
