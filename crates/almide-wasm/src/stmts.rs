//! Statement-position lowering (binds, assigns, index COW stores, loops,
//! statement markers) — split from emitter.rs for the complexity budget.

use std::collections::HashMap;

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Statement position: Unit-typed shapes only (blocks, calls, control).
    pub(crate) fn lower_stmt_expr(&mut self, e: &IrExpr) -> Result<(), EmitError> {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                if let Some(tail) = expr {
                    self.lower_stmt_expr(tail)?;
                }
                Ok(())
            }
            IrExprKind::Call { target, args, .. } => {
                // Unit-position call: a value-returning callee's result is
                // dropped (a bare non-Unit call statement is legal IR).
                if self.lower_call(target, args)?.is_some() {
                    self.f.instructions().drop();
                }
                Ok(())
            }
            // Unit-position `if`: both arms are statement bodies.
            IrExprKind::If { cond, then, else_ } => {
                self.lower(cond, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Empty);
                self.lower_stmt_expr(then)?;
                self.f.instructions().else_();
                self.lower_stmt_expr(else_)?;
                self.f.instructions().end();
                Ok(())
            }
            // `while`: block { loop { !cond → br out; body; br loop } }.
            // Break/Continue would need label-depth tracking — they surface
            // as their own honest reasons until that lands.
            IrExprKind::While { cond, body } => {
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                // Deterministic meter: one loop-head charge per condition
                // CHECK (n iterations = n+1 checks), ALS-DT2.
                self.emit_det_charge_const(1);
                self.lower(cond, Some(BOOL))?;
                self.f.instructions().i32_eqz().br_if(1);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.f.instructions().br(0).end().end();
                Ok(())
            }
            IrExprKind::Match { subject, arms } => self.lower_match(subject, arms, None).map(|_| ()),
            // for x in <list> / for i in a..b — extracted for complexity.
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.lower_forin(*var, var_tuple.as_deref(), iterable, body)
            }
            IrExprKind::Unit => Ok(()),
            // Statement-position `f()!` / `f()?`: the marker machinery
            // runs (propagation/abort), the ok payload is discarded.
            IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } => {
                self.lower(e, None)?;
                self.f.instructions().drop();
                Ok(())
            }
            // `{}` in statement position is a no-op value; in value
            // position the arm below builds the empty map.
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    pub(crate) fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                // Deferred head-only range (C-238): evaluate the bounds
                // ONCE, in source order, into the pair locals — no block.
                if let Some(&(sl, el, _)) = self.deferred_ranges.get(var) {
                    let IrExprKind::Range { start, end, .. } = &value.kind else {
                        return unsup("bind:deferred-non-range");
                    };
                    self.lower(start, Some(INT))?;
                    self.f.instructions().local_set(sl);
                    self.lower(end, Some(INT))?;
                    self.f.instructions().local_set(el);
                    return Ok(());
                }
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.lower(value, Some(declared))?;
                // Container value semantics: every bind owns a fresh
                // block, so in-place mutation (push growth, bytes.set_*)
                // can never be observed through aliases. Bytes joined
                // when the snapshot fixture showed `let snap = arena`
                // observing later set_at writes.
                if matches!(
                    declared,
                    SliceTy::List(_)
                        | SliceTy::Map(..)
                        | SliceTy::Set(_)
                        | SliceTy::Scalar(Scalar::Bytes)
                ) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            // `p.field = v` on a record var: copy-on-write write-back —
            // fresh block, one slot replaced, rebound. In-place mutation
            // stays unobservable (the alias_cow fixtures pin exactly
            // this: an alias captured before the assign keeps the old
            // value).
            IrStmtKind::FieldAssign { target, field, value } => {
                let (slot, declared) = match self.locals.get(target) {
                    Some(&(idx, d)) => (Ok(idx), d),
                    None => match self.globals.get(target) {
                        Some(&(gidx, d)) => (Err(gidx), d),
                        None => return unsup("field-assign:unmapped"),
                    },
                };
                let SliceTy::Named(ti) = declared else {
                    return unsup(&format!("field-assign-of:{declared:?}"));
                };
                let (fty, off) = {
                    let crate::types_table::NamedDef::Record(r) = self.types.def(ti) else {
                        return unsup("field-assign-nonrecord");
                    };
                    let Some(fi) = r.fields.iter().find(|f| f.name == field.as_str()) else {
                        return unsup(&format!("field-assign-unknown:{field}"));
                    };
                    (fi.ty, fi.offset)
                };
                let hb = self.hold_i32()?;
                match slot {
                    Ok(idx) => self.f.instructions().local_get(idx),
                    Err(gidx) => self.f.instructions().global_get(gidx),
                };
                self.f.instructions().call(F_BLOCK_COPY).local_tee(hb);
                self.lower(value, Some(fty))?;
                self.store_ty_slot(fty, off);
                self.f.instructions().local_get(hb);
                match slot {
                    Ok(idx) => self.f.instructions().local_set(idx),
                    Err(gidx) => self.f.instructions().global_set(gidx),
                };
                self.release_i32();
                Ok(())
            }
            // `m[k] = v` on a map var — the same write-back the
            // `map.insert` mut form runs (functional `set`, rebind).
            IrStmtKind::MapInsert { target, key, value } => {
                let Some(&(var_idx, _)) = self.locals.get(target) else {
                    return unsup("map-insert:unmapped");
                };
                let var_expr = IrExpr {
                    kind: IrExprKind::Var { id: *target },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                };
                let args = [var_expr, key.clone(), value.clone()];
                self.lower_map_call("set", &args, None)?;
                self.f.instructions().local_set(var_idx);
                Ok(())
            }
            IrStmtKind::Assign { var, value } => {
                let (local, declared) = match self.locals.get(var) {
                    Some(&(idx, d)) => (Some(idx), d),
                    None => match self.globals.get(var) {
                        Some(&(gidx, d)) => (None, {
                            let _ = gidx;
                            d
                        }),
                        None => return unsup("assign:unmapped"),
                    },
                };
                self.lower(value, Some(declared))?;
                if matches!(
                    declared,
                    SliceTy::List(_)
                        | SliceTy::Map(..)
                        | SliceTy::Set(_)
                        | SliceTy::Scalar(Scalar::Bytes)
                ) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                match local {
                    Some(idx) => {
                        self.f.instructions().local_set(idx);
                    }
                    None => {
                        let gidx = self.globals[var].0;
                        self.f.instructions().global_set(gidx);
                    }
                }
                Ok(())
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                self.lower_index_assign(target, index, value)
            }
            IrStmtKind::Expr { expr } => self.lower_stmt_expr(expr),
            // let (a, b) = e — evaluate once, load each bound position.
            IrStmtKind::BindDestructure { pattern, value } => {
                let ty = self.lower(value, None)?;
                let scr = self.scr_i32_local;
                self.f.instructions().local_set(scr);
                self.emit_pattern_binds(pattern, ty, scr)
            }
            IrStmtKind::Comment { .. } => Ok(()),
            other => unsup(&format!("stmt:{}", stmt_kind_name(other))),
        }
    }

    /// A call in any position. Returns the callee's slice return type
    /// (None = Unit). `println`/`eprintln` are the special forms.
    /// `for` loops: a Range iterates Int directly; a List walks its
    /// element array. The loop variable is a pre-collected local.
    pub(crate) fn lower_forin(
        &mut self,
        var: VarId,
        var_tuple: Option<&[VarId]>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), EmitError> {
        let Some(&(var_idx, var_ty)) = self.locals.get(&var) else {
            return unsup("bind:unmapped");
        };

                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    // Range: var runs start..end directly, no list at all.
                    if var_ty != INT {
                        return unsup("forin-range-nonint");
                    }
                    self.lower(start, Some(INT))?;
                    self.f.instructions().local_set(var_idx);
                    self.lower(end, Some(INT))?;
                    let stop = self.hold_i64()?;
                    self.f.instructions().local_set(stop);
                    self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                    self.emit_det_charge_const(1);
                    self.f.instructions().local_get(var_idx).local_get(stop);
                    if *inclusive {
                        self.f.instructions().i64_gt_s();
                    } else {
                        self.f.instructions().i64_ge_s();
                    }
                    self.f.instructions().br_if(1);
                    for st in body {
                        self.lower_stmt(st)?;
                    }
                    self.f
                        .instructions()
                        .local_get(var_idx)
                        .i64_const(1)
                        .i64_add()
                        .local_set(var_idx)
                        .br(0)
                        .end()
                        .end();
                    self.release_i64();
                    return Ok(());
                }
                // A deferred range var: the same counting loop, bounds
                // from the bind-time pair locals.
                if let IrExprKind::Var { id } = &iterable.kind
                    && let Some(&(sl, el, inclusive)) = self.deferred_ranges.get(id)
                {
                    {
                        if var_ty != INT {
                            return unsup("forin-range-nonint");
                        }
                        self.f.instructions().local_get(sl).local_set(var_idx);
                        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                        self.emit_det_charge_const(1);
                        self.f.instructions().local_get(var_idx).local_get(el);
                        if inclusive {
                            self.f.instructions().i64_gt_s();
                        } else {
                            self.f.instructions().i64_ge_s();
                        }
                        self.f.instructions().br_if(1);
                        for st in body {
                            self.lower_stmt(st)?;
                        }
                        self.f
                            .instructions()
                            .local_get(var_idx)
                            .i64_const(1)
                            .i64_add()
                            .local_set(var_idx)
                            .br(0)
                            .end()
                            .end();
                        return Ok(());
                    }
                }
                let elem = match self.lower(iterable, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("forin-iter:{other:?}")),
                };
                if var_ty != elem {
                    return unsup("forin-var-ty");
                }
                let stride = elem.slot_size();
                let base = self.hold_i32()?;
                let count = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(base);
                self.f
                    .instructions()
                    .local_get(base)
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(count)
                    .i32_const(0)
                    .local_set(cur);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.emit_det_charge_const(1);
                self.f.instructions().local_get(cur).local_get(count).i32_ge_u().br_if(1);
                self.f
                    .instructions()
                    .local_get(base)
                    .local_get(cur)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(elem, 0);
                self.f.instructions().local_set(var_idx);
                // for (a, b) in pairs — the loop var holds the tuple base;
                // load each position into its destructured local.
                if let Some(tvars) = var_tuple {
                    let SliceTy::Tuple(ti) = elem else {
                        return unsup("forin-tuple-nontuple");
                    };
                    let def = self.types.tuple_def(ti);
                    if def.fields.len() != tvars.len() {
                        return unsup("forin-tuple-arity");
                    }
                    for (tv, (fty, off)) in tvars.iter().zip(def.fields) {
                        let Some(&(tidx, _)) = self.locals.get(tv) else {
                            return unsup("bind:unmapped");
                        };
                        self.f.instructions().local_get(var_idx);
                        self.load_ty_slot(fty, off);
                        self.f.instructions().local_set(tidx);
                    }
                }
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.f
                    .instructions()
                    .local_get(cur)
                    .i32_const(1)
                    .i32_add()
                    .local_set(cur)
                    .br(0)
                    .end()
                    .end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(())
                }

    /// `xs[i] = v` — copy-on-write (split from lower_stmt for the complexity budget).
    fn lower_index_assign(
        &mut self,
        target: &VarId,
        index: &IrExpr,
        value: &IrExpr,
    ) -> Result<(), EmitError> {

                let (is_local, declared) = match self.locals.get(target) {
                    Some(&(_, d)) => (true, d),
                    None => match self.globals.get(target) {
                        Some(&(_, d)) => (false, d),
                        None => return unsup("index-assign:unmapped"),
                    },
                };
                let SliceTy::List(h) = declared else {
                    return unsup(&format!("index-assign-ty:{declared:?}"));
                };
                let el = self.types.el(h);
                let stride = el.slot_size() as i64;
                // Interp order: index, then value, then the bounds check.
                self.lower(index, Some(INT))?;
                let hi = self.hold_i64()?;
                self.f.instructions().local_set(hi);
                self.lower(value, Some(el))?;
                let hv = self.hold_val(el)?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                let get_target = |f: &mut wasm_encoder::Function, locals: &HashMap<VarId, (u32, SliceTy)>, globals: &HashMap<VarId, (u32, SliceTy)>| {
                    if is_local {
                        f.instructions().local_get(locals[target].0);
                    } else {
                        f.instructions().global_get(globals[target].0);
                    }
                };
                // OOB → the exact native frame + exit 1.
                let msg = self.pool.intern("index out of bounds");
                get_target(self.f, self.locals, self.globals);
                {
                    let mut i = self.f.instructions();
                    i.i32_load(len_memarg())
                        .i64_extend_i32_u()
                        .i64_const(stride)
                        .i64_div_s();
                    i.local_get(hi).i64_le_s();
                    i.local_get(hi).i64_const(0).i64_lt_s();
                    i.i32_or().if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                self.f.instructions().end();
                // COW: the binding gets a fresh block, then the store.
                get_target(self.f, self.locals, self.globals);
                self.f.instructions().call(F_BLOCK_COPY).local_set(hb);
                if is_local {
                    let idx = self.locals[target].0;
                    self.f.instructions().local_get(hb).local_set(idx);
                } else {
                    let g = self.globals[target].0;
                    self.f.instructions().local_get(hb).global_set(g);
                }
                {
                    let mut i = self.f.instructions();
                    i.local_get(hb)
                        .i64_extend_i32_u()
                        .local_get(hi)
                        .i64_const(stride)
                        .i64_mul()
                        .i64_add()
                        .i32_wrap_i64()
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add();
                    i.local_get(hv);
                }
                self.store_ty_slot_raw(el);
                self.release_i32();
                self.release_val(el);
                self.release_i64();
                Ok(())
    }
}
