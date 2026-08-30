//! Named-fn values and map literals — split from emitter.rs for the
//! file budget.

use almide_ir::{IrExpr, IrExprKind};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// A named fn as a VALUE (split from lower for the complexity budget).
    pub(crate) fn lower_fn_ref(
        &mut self,
        e: &IrExpr,
        name: &almide_base::intern::Sym,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {

                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Fn(sig) = ty else {
                    return unsup(&format!("fnref-vs-{ty:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                let resolved = self
                    .cur_module
                    .and_then(|m| self.table.by_name.get(&format!("{m}.{}", name.as_str())))
                    .or_else(|| self.table.by_name.get(name.as_str()))
                    .copied();
                let Some(idx) = resolved else {
                    return unsup(&format!("fnref:{name}"));
                };
                let info = &self.table.infos[idx];
                if info.refuse.is_some() {
                    return unsup("fnref-refused-target");
                }
                if info.params != def.params {
                    return unsup("fnref-param-mismatch");
                }
                let entry = if info.ret == def.ret {
                    TableEntry::Fn(idx)
                } else {
                    match (info.ret, def.ret) {
                        (Some(raw), Some(SliceTy::Result(o, er)))
                            if def.effect
                                && self.types.el(o) == raw
                                && self.types.el(er) == STR =>
                        {
                            TableEntry::Adapter { target: idx, raw }
                        }
                        _ => return unsup("fnref-ret-mismatch"),
                    }
                };
                let slot = self.work.slot(entry);
                // Fn value = closure BLOCK [slot@0]; capture-free blocks
                // are pool statics (dedup by content, zero runtime alloc).
                let block = self.pool.intern_block(&(slot).to_le_bytes());
                self.f.instructions().i32_const(block as i32);
                Ok(ty)
    }

    /// A lambda as a VALUE (split from lower for the complexity budget).
    pub(crate) fn lower_lambda_value(
        &mut self,
        e: &IrExpr,
        params: &[(VarId, almide_types::types::Ty)],
        body: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {

                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Fn(sig) = ty else {
                    return unsup(&format!("lambda-vs-{ty:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                if params.len() != def.params.len() {
                    return unsup("lambda-arity");
                }
                let param_ids: std::collections::HashSet<VarId> =
                    params.iter().map(|(v, _)| *v).collect();
                let mut captured = self.captured_vars(&param_ids, body);
                captured.sort_by_key(|v| v.0);
                let ps: Vec<(VarId, SliceTy)> = params
                    .iter()
                    .map(|(v, _)| *v)
                    .zip(def.params.iter().copied())
                    .collect();
                let effect_raw = if def.effect {
                    match def.ret {
                        Some(SliceTy::Result(o, _)) => Some(self.types.el(o)),
                        _ => None,
                    }
                } else {
                    None
                };
                // Closure block layout: [slot:i32][captures packed...].
                // A C-319 cell travels as its 4-byte ADDRESS.
                let widths: Vec<u32> = std::iter::once(4)
                    .chain(captured.iter().map(|(v, t)| {
                        if self.cells.contains(v) { 4 } else { t.slot_size() }
                    }))
                    .collect();
                let (offsets, size) = almide_layout::pack_fields(&widths);
                let captures: Vec<(VarId, SliceTy, u32, bool)> = captured
                    .iter()
                    .zip(offsets.iter().skip(1))
                    .map(|(&(v, t), &off)| (v, t, off, self.cells.contains(&v)))
                    .collect();
                let j = self.work.register_closure_lambda(
                    ps, def.ret, effect_raw, body.clone(), captures.clone(), self.var_space,
                );
                let slot = self.work.slot(TableEntry::Lambda(j));
                if captures.is_empty() {
                    let block = self.pool.intern_block(&(slot).to_le_bytes());
                    self.f.instructions().i32_const(block as i32);
                } else {
                    let hb = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hb)
                        .i32_const(slot as i32)
                        .i32_store(slot_memarg(0));
                    for (v, t, off, is_cell) in &captures {
                        let (idx, _) = self.locals[v];
                        self.f.instructions().local_get(hb).local_get(idx);
                        if *is_cell {
                            // the local already holds the cell address
                            self.f.instructions().i32_store(slot_memarg(*off));
                        } else {
                            self.store_ty_slot(*t, *off);
                        }
                    }
                    self.f.instructions().local_get(hb);
                    self.release_i32();
                }
        Ok(ty)
    }
}

impl Emitter<'_> {
    /// `["k": v, …]` — the map literal, split from `lower` for the
    /// complexity budget. Desugars to the SAME insertion-ordered upsert
    /// `map.from_list` runs (last write wins on duplicate keys).
    pub(crate) fn lower_map_literal(
        &mut self,
        e: &IrExpr,
        entries: &[(IrExpr, IrExpr)],
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Map(kh, vh) = ty else {
                    return unsup(&format!("ty-mismatch:map-literal-vs-{ty:?}"));
                };
                let (kt, vt) = (self.types.el(kh), self.types.el(vh));
                let _ = (kt, vt);
                let (k_ty, v_ty) = match &e.ty {
                    Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 =>
                    {
                        (a[0].clone(), a[1].clone())
                    }
                    other => return unsup(&format!("map-literal-ty:{}", ty_name(other))),
                };
                let pair_ty = Ty::Tuple(vec![k_ty, v_ty]);
                let list_ty = Ty::Applied(
                    TypeConstructorId::List,
                    vec![pair_ty.clone()],
                );
                let pairs = IrExpr {
                    kind: IrExprKind::List {
                        elements: entries
                            .iter()
                            .map(|(k, v)| IrExpr {
                                kind: IrExprKind::Tuple {
                                    elements: vec![k.clone(), v.clone()],
                                },
                                ty: pair_ty.clone(),
                                span: e.span,
                                def_id: None,
                            })
                            .collect(),
                    },
                    ty: list_ty,
                    span: e.span,
                    def_id: None,
                };
                match self.lower_map_call("from_list", &[pairs], Some(ty))? {
                    Some(t) => Ok(t),
                    None => unsup("map-literal-unit"),
                }
    }
}
