//! Var-slot access (locals, C-319 cells, top-let globals) and the
//! abort frame — split from emitter.rs for the file budget — and the
//! effect-fn ok wrap of a raw exit value.

use almide_ir::{IrExpr, VarId};

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
            .or_else(|| self.globals.get(&(self.var_space, *id)).map(|&(i, t)| (i, t, true)))
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

    /// The COW form of the mut-var read (RC-5): every in-place mutation
    /// route reads through here — a shared block copies first and the
    /// var (or cell) is repointed at the unique copy before the route
    /// touches it. Lists and Bytes only; strings mutate functionally and
    /// maps have their own judge (map_inplace.rs, #1219).
    pub(crate) fn emit_read_mut_var_cow(
        &mut self,
        id: &VarId,
        idx: u32,
        ty: SliceTy,
        global: bool,
    ) -> Result<(), EmitError> {
        self.emit_read_mut_var(id, idx, ty, global);
        // Params are exempt: they are borrowed views of the caller's
        // block, and writes through them must stay caller-visible.
        if matches!(ty, SliceTy::List(_) | SliceTy::Scalar(Scalar::Bytes))
            && (global || idx >= self.rc_param_ceiling)
        {
            let scr = self.scr_i32_local;
            let cow = self.cow_fn_of(ty);
            self.f.instructions().call(cow).local_set(scr);
            self.f.instructions().local_get(scr);
            self.emit_store_mut_var(*id, idx, ty, global)?;
            self.f.instructions().local_get(scr);
        }
        Ok(())
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

    /// Rebind the mut var to the FRESH block on the stack — a mutator the
    /// leg lowers functionally (`string.push`'s concat, `list.clear`'s empty
    /// list, the bytes window writers' copy): the var held one credit on
    /// the block it no longer names, and that credit goes with the rebind,
    /// exactly as an `Assign` settles its old occupant (#2968 — every such
    /// write-back orphaned the old block). A PARAMETER receiver holds the
    /// site's credit the same way (param_borrow.rs marks a param a module
    /// op touches owned), so no receiver is exempt. The value must not BE
    /// the old block: an in-place helper that may answer with its operand
    /// (`$list_push`, `$bytes_push`) settles through
    /// `settle_outgrown_receiver` instead.
    pub(crate) fn emit_rebind_mut_var_fresh(
        &mut self,
        id: VarId,
        idx: u32,
        ty: SliceTy,
        global: bool,
    ) -> Result<(), EmitError> {
        if self.rc_droppable(ty) {
            let hn = self.hold_i32()?;
            self.f.instructions().local_set(hn);
            self.emit_read_mut_var(&id, idx, ty, global);
            let dec = self.dec_fn_of(ty);
            self.f.instructions().call(dec).local_get(hn);
            self.release_i32();
        }
        self.emit_store_mut_var(id, idx, ty, global)
    }

    /// The raw ok type a guard's else yields when it is a PLAIN value of an
    /// effect fn's Result return — the else type is the Result's ok side and
    /// not the Result itself. `None` when the else already is the Result
    /// (`ok(..)` / `err(..)` / a propagated `r!`): that lowers at `want`.
    pub(crate) fn raw_effect_else(&self, want: SliceTy, ret_direct: Option<&IrExpr>, else_: &IrExpr) -> Option<SliceTy> {
        let SliceTy::Result(o, _) = want else { return None };
        if ret_direct.is_some() {
            return None;
        }
        let raw = self.types.el(o);
        let ety = slice_ty_of(&else_.ty, self.types)?;
        (ety == raw && ety != want).then_some(raw)
    }

    /// `[] -> [ok(else) Result block]`: a raw effect-fn exit value, wrapped
    /// the way `lower_fn` wraps a raw tail — a Unit else runs as a
    /// statement and the ok payload materializes after it.
    pub(crate) fn lower_raw_effect_exit(&mut self, else_: &IrExpr, raw: SliceTy, want: SliceTy) -> Result<(), EmitError> {
        if raw == SliceTy::Unit {
            self.lower_stmt_expr(else_)?;
            self.f.instructions().i32_const(0);
        } else {
            self.lower(else_, Some(raw))?;
            if self.rc_droppable(raw) && !self.rc_owned_result(else_) {
                self.rc_inc_top();
            }
        }
        self.wrap_ok(raw, want)
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

    /// #3101: make every block on a `mut` argument's place path uniquely
    /// held before the callee writes the leaf in place — the field-place
    /// twin of the bare-var `$cow` read (`emit_read_mut_var_cow`, #2503).
    /// The callee never judges sharing (its param is a borrowed view), so
    /// the site must: `o.a.xs` copies `o`'s record when another binding
    /// holds it (`let alias = o`), then `o.a`'s record the same way
    /// (`let alias = o.a`), then the `xs` block (`let alias_xs = o.a.xs`),
    /// each copy stored back into its (already unique) holder. An unshared
    /// path costs one rc test per level and copies nothing. Before this,
    /// the leaf block was written through while an alias still held it, so
    /// the alias showed the callee's write (C-033). The C-132 write-back
    /// (mut_param_place.rs) then stores the returned buffer into the same
    /// place.
    ///
    /// The leaf takes the judge only for Lists and Bytes, as the bare-var
    /// read does: a String mutates functionally, a Map has its own
    /// (monotone) judge. A path that is not a chain of fields and tuple
    /// slots over a mutable local or global is left to the plain read.
    pub(crate) fn make_mut_place_unique(&mut self, arg: &IrExpr) -> Result<(), EmitError> {
        use almide_ir::IrExprKind;
        let mut steps: Vec<Result<almide_base::intern::Sym, usize>> = Vec::new();
        let mut cur = arg;
        let id = loop {
            match &cur.kind {
                IrExprKind::Member { object, field } => {
                    steps.push(Ok(*field));
                    cur = object;
                }
                IrExprKind::TupleIndex { object, index } => {
                    steps.push(Err(*index));
                    cur = object;
                }
                IrExprKind::Var { id } => break *id,
                _ => return Ok(()),
            }
        };
        if steps.is_empty() {
            return Ok(());
        }
        steps.reverse();
        let Some((idx, root_ty, global)) = self.mut_var(&id) else { return Ok(()) };
        // A PARAMETER root is exempt, as the var arm and the record-field
        // bytes receiver (bytes_recv.rs) exempt it: its block is the
        // caller's, the site's argument credit keeps its count above one,
        // and its writes must stay caller-visible.
        if !self.rc_droppable(root_ty) || (!global && idx < self.rc_param_ceiling) {
            return Ok(());
        }
        let cow = self.cow_fn_of(root_ty);
        let root = self.hold_i32()?;
        self.emit_read_mut_var(&id, idx, root_ty, global);
        self.f.instructions().call(cow).local_tee(root);
        self.emit_store_mut_var(id, idx, root_ty, global)?;
        let mut holds = vec![root];
        let mut parent_ty = root_ty;
        for (k, step) in steps.iter().enumerate() {
            let (fty, off) = match step {
                Ok(field) => self.record_field_slot(parent_ty, field)?,
                Err(slot) => {
                    let SliceTy::Tuple(ti) = parent_ty else { break };
                    match self.types.tuple_def(ti).fields.get(*slot) {
                        Some(&f) => f,
                        None => break,
                    }
                }
            };
            let leaf = k + 1 == steps.len();
            let judged = if leaf {
                matches!(fty, SliceTy::List(_) | SliceTy::Scalar(Scalar::Bytes))
            } else {
                self.rc_droppable(fty) && !matches!(fty, SliceTy::Map(..) | SliceTy::Set(_))
            };
            if !judged {
                break;
            }
            let parent = holds[holds.len() - 1];
            let cow = self.cow_fn_of(fty);
            let h = self.hold_i32()?;
            self.f.instructions().local_get(parent);
            self.load_ty_slot(fty, off);
            self.f.instructions().call(cow).local_set(h).local_get(parent).local_get(h);
            self.store_ty_slot(fty, off);
            holds.push(h);
            parent_ty = fty;
        }
        for _ in &holds {
            self.release_i32();
        }
        Ok(())
    }
}
