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
                let SliceTy::Scalar(_) = e else { return unsup("set-elem-nonscalar") };
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
            _ => unsup(&format!("call:set.{func}")),
        }
    }

    /// Evaluate set + needle, run the scan. Returns ALL holds explicitly
    /// — (set, needle, entry, elem ty). Release: entry, needle, set.
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
