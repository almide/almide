//! Var-slot access (locals, C-319 cells, top-let globals) and the
//! abort frame — split from emitter.rs for the file budget.

use almide_ir::VarId;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Store the value on the stack into `var`'s storage: a plain local
    /// set, or a store through the C-319 cell address.
    pub(crate) fn emit_store_var(
        &mut self,
        id: VarId,
        idx: u32,
        ty: SliceTy,
    ) -> Result<(), EmitError> {
        if self.cells.contains(&id) {
            let hv = self.hold_val(ty)?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(idx).local_get(hv);
            self.store_ty_slot(ty, 0);
            self.release_val(ty);
        } else {
            self.f.instructions().local_set(idx);
        }
        Ok(())
    }

    /// A MUTABLE var slot: a local first, else a top-let global —
    /// (index, ty, is_global). The mut-convention arms (bytes.push,
    /// set_*, list.push, string.push …) write back through this.
    pub(crate) fn mut_var(&self, id: &VarId) -> Option<(u32, SliceTy, bool)> {
        self.locals
            .get(id)
            .map(|&(i, t)| (i, t, false))
            .or_else(|| self.globals.get(id).map(|&(i, t)| (i, t, true)))
    }

    /// Push the mut var's current VALUE (cells deref for locals).
    pub(crate) fn emit_read_mut_var(&mut self, id: &VarId, idx: u32, ty: SliceTy, global: bool) {
        if global {
            self.f.instructions().global_get(idx);
        } else {
            self.f.instructions().local_get(idx);
            if self.cells.contains(id) {
                self.load_ty_slot(ty, 0);
            }
        }
    }

    /// Store the value on the stack back into the mut var's slot.
    pub(crate) fn emit_store_mut_var(
        &mut self,
        id: VarId,
        idx: u32,
        ty: SliceTy,
        global: bool,
    ) -> Result<(), EmitError> {
        if global {
            self.f.instructions().global_set(idx);
            Ok(())
        } else {
            self.emit_store_var(id, idx, ty)
        }
    }

    /// `[raw value]` -> `[ok(..) Result block]` (the effect-fn return wrap).
    pub(crate) fn wrap_ok(&mut self, raw: SliceTy, ret: SliceTy) -> Result<(), EmitError> {
        let SliceTy::Result(o, _) = ret else {
            return unsup("effect-wrap-non-result");
        };
        let side = self.types.el(o);
        if side != raw {
            return unsup("effect-wrap-ty-mismatch");
        }
        let hv = self.hold_val(raw)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(hb)
            .i32_const(0)
            .i32_store(slot_memarg(almide_layout::SUM_TAG));
        self.f.instructions().local_get(hb).local_get(hv);
        self.store_ty_slot(raw, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hb);
        self.release_i32();
        self.release_val(raw);
        Ok(())
    }

}
