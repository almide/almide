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
        // main's Result-typed statement/tail is the effect carrier —
        // err aborts with the native contract instead of discarding
        // (#1734; see try_lower_main_err_carrier).
        if !matches!(&e.kind, IrExprKind::Block { .. } | IrExprKind::If { .. } | IrExprKind::Match { .. })
            && self.try_lower_main_err_carrier(e)?
        {
            return Ok(());
        }
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
            IrExprKind::If { cond, then, else_ } => self.lower_stmt_if(cond, then, else_),
            IrExprKind::While { cond, body } => self.lower_while(cond, body),
            // Match opens labels the walker does not track — suspend the
            // loop context so a Continue inside an arm walls honestly
            // instead of branching to the wrong depth.
            IrExprKind::Match { subject, arms } => {
                let saved = self.loop_ctl.take();
                let r = self.lower_match(subject, arms, None).map(|_| ());
                self.loop_ctl = saved;
                r
            }
            IrExprKind::Continue => match self.loop_ctl {
                Some((extra, _)) => {
                    self.f.instructions().br(extra);
                    Ok(())
                }
                None => unsup("expr:Continue"),
            },
            IrExprKind::Break => match self.loop_ctl {
                Some((extra, delta)) => {
                    self.f.instructions().br(extra + delta);
                    Ok(())
                }
                None => unsup("expr:Break"),
            },
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
            // Any other value expression in statement position: evaluate
            // and discard (a bare `ok(x)` statement is legal IR).
            _ => {
                if self.lower(e, None)? != SliceTy::Unit {
                    self.f.instructions().drop();
                }
                Ok(())
            }
        }
    }

    /// `while`: block { loop { !cond → br out; body; br loop } }.
    /// `continue` brs to the loop head (the next cond CHECK, which
    /// charges — the interp's per-check meter), `break` to the block.
    fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), EmitError> {
        // Counted-shape fast lane (unroll.rs): on `true` the rolled loop
        // below drains the remainder iterations.
        let _ = self.try_unroll_while(cond, body)?;
        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
        // Deterministic meter: one loop-head charge per condition
        // CHECK (n iterations = n+1 checks), ALS-DT2.
        self.emit_det_charge_const(1);
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().i32_eqz().br_if(1);
        self.lower_loop_body(body, false)?;
        self.f.instructions().br(0).end().end();
        Ok(())
    }

    /// A loop body with break/continue wired. For-in bodies sit in an
    /// extra block so `continue` still reaches the STEP code after it;
    /// a while `continue` brs straight to the loop head (the next cond
    /// check). break_delta = labels from the continue target up to the
    /// exit block (while: 1; for-in: 2 — the inner block adds one).
    fn lower_loop_body(&mut self, body: &[IrStmt], for_in: bool) -> Result<(), EmitError> {
        let saved = self.loop_ctl.take();
        if for_in {
            self.f.instructions().block(BlockType::Empty);
            self.loop_ctl = Some((0, 2));
        } else {
            self.loop_ctl = Some((0, 1));
        }
        for st in body {
            self.lower_stmt(st)?;
        }
        if for_in {
            self.f.instructions().end();
        }
        self.loop_ctl = saved;
        Ok(())
    }

    /// Unit-position `if`: both arms are statement bodies. The if_
    /// label shifts break/continue targets one deeper.
    fn lower_stmt_if(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
    ) -> Result<(), EmitError> {
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        if let Some((extra, _)) = self.loop_ctl.as_mut() {
            *extra += 1;
        }
        self.branch_depth += 1;
        let arms = (|| {
            self.lower_stmt_expr(then)?;
            self.f.instructions().else_();
            self.lower_stmt_expr(else_)
        })();
        self.branch_depth -= 1;
        arms?;
        self.f.instructions().end();
        if let Some((extra, _)) = self.loop_ctl.as_mut() {
            *extra -= 1;
        }
        Ok(())
    }

    /// `guard cond else raise`: cond false → the else value IS the
    /// function's return (the interp's Flow::Return). Region arms
    /// would skip their exit bookkeeping on this early return, and
    /// main's raise-abort frame is a different shape — both wall.
    fn lower_stmt_guard(&mut self, cond: &IrExpr, else_: &IrExpr) -> Result<(), EmitError> {
        if self.region_repair.is_some() {
            return unsup("guard-in-region-arm");
        }
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        match self.fn_ret {
            Some(want) => {
                self.lower(else_, Some(want))?;
                self.f.instructions().return_();
            }
            // main / Unit fn: the else IS the return — evaluate it in
            // statement position (a `process.exit` else never returns).
            // A RESULT else in main is the err channel, not a discard:
            // `guard c else err(…)` must print `Error: {msg}` and exit 1
            // (#1734 — the discard silently swallowed the err). The
            // early return skips the RC epilogue: a leak, never a
            // dangle.
            None => {
                if !self.try_lower_main_err_carrier(else_)? {
                    self.lower_stmt_expr(else_)?;
                }
                self.f.instructions().return_();
            }
        }
        self.f.instructions().end();
        Ok(())
    }

    /// let/var bind: deferred ranges write their pair locals; container
    /// values deep-copy (bind-owns-its-block); C-319 cells allocate.
    fn lower_stmt_bind(&mut self, var: &VarId, value: &IrExpr) -> Result<(), EmitError> {
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
        // RC-5: Lists and Bytes SHARE at bind — the COW judge at every
        // in-place mutation entry moved the value-semantics copy from
        // bind time to mutation time (rc counts the holders it judges
        // by, so a borrowed rhs takes +1 — cells included). Maps and
        // Sets keep the bind copy: their mutations are functional
        // rebinds that never pass a COW gate.
        if matches!(declared, SliceTy::Map(..) | SliceTy::Set(_)) {
            self.f.instructions().call(F_BLOCK_COPY);
        }
        if matches!(declared, SliceTy::List(_) | SliceTy::Scalar(Scalar::Str | Scalar::Bytes))
            && !crate::rc_ownership::rc_certainly_fresh(&value.kind)
        {
            self.rc_inc_top();
        }
        if self.cells.contains(var) {
            // C-319: the bind allocates the shared cell; the
            // local holds its ADDRESS from here on.
            let hv = self.hold_val(declared)?;
            self.f.instructions().local_set(hv);
            self.f
                .instructions()
                .i32_const(declared.slot_size() as i32)
                .call(F_ALLOC)
                .local_tee(idx)
                .local_get(hv);
            self.store_ty_slot(declared, 0);
            self.release_val(declared);
        } else {
            // RC-3 ownership: a Str bind takes no copy, so a borrowed
            // rhs (var/element/field read, call into a native arm) gets
            // +1; copied containers arrive fresh. The previous occupant
            // (loop rebinds; zero on the first pass) is released, and
            // the local joins the epilogue's owner set.
            if self.rc_droppable(declared) {
                self.f.instructions().local_get(idx).call(F_DEC_FLAT);
                self.rc_own(idx);
                if self.witness.is_some() {
                    self.witness_bind(idx, declared, value);
                }
            }
            self.f.instructions().local_set(idx);
        }
        Ok(())
    }

    /// Register a local as an epilogue-released owner. A droppable PARAM
    /// can land here too (a mut-param writeback's Assign) — the epilogue's
    /// param pass skips locals in this set, so each local is released
    /// exactly once (#1770: both passes firing on one local double-freed
    /// the returned buffer, and its freelist link zeroed the first
    /// payload word).
    pub(crate) fn rc_own(&mut self, idx: u32) {
        self.rc_owned.insert(idx);
    }

    pub(crate) fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => self.lower_stmt_bind(var, value),
            // `p.field = v` on a record var: copy-on-write write-back —
            // fresh block, one slot replaced, rebound. In-place mutation
            // stays unobservable (the alias_cow fixtures pin exactly
            // this: an alias captured before the assign keeps the old
            // value).
            IrStmtKind::FieldAssign { target, field, value } => {
                self.lower_field_assign(target, field, value)
            }
            // `m[k] = v` on a map var — the in-place window when the var
            // owns its block (#1219), else the same write-back the
            // `map.insert` mut form runs (functional `set`, rebind).
            IrStmtKind::MapInsert { target, key, value } => {
                if self.try_map_set_in_place(target, key, value)? {
                    return Ok(());
                }
                if self.cells.contains(target) {
                    return unsup("cell-write:map-insert");
                }
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
            IrStmtKind::Assign { var, value } => self.lower_assign(var, value),
            IrStmtKind::IndexAssign { target, index, value } => {
                if self.cells.contains(target) {
                    return unsup("cell-write:index-assign");
                }
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
            IrStmtKind::Guard { cond, else_ } => self.lower_stmt_guard(cond, else_),
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
                    self.lower_loop_body(body, true)?;
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
                        self.lower_loop_body(body, true)?;
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
                    // `for (k, v) in map` — walk the insertion-ordered
                    // entry blocks (the same layout map.fold walks).
                    SliceTy::Map(kh, vh) => {
                        let (k, v) = (self.types.el(kh), self.types.el(vh));
                        let Some(&[tk, tv]) = var_tuple else {
                            return unsup("forin-map-nontuple");
                        };
                        let (Some(&(ki, _)), Some(&(vi, _))) =
                            (self.locals.get(&tk), self.locals.get(&tv))
                        else {
                            return unsup("bind:unmapped");
                        };
                        let (koff, voff, esz) = crate::collections::entry_layout(k, v);
                        // #1219: the cursor below holds the block across
                        // the body — a `map.insert(m, …)` there must not
                        // grow it in place under us, so the subject
                        // witnesses a second holder (the monotone Map rc;
                        // the window then takes the functional copy).
                        if !crate::rc_ownership::rc_certainly_fresh(&iterable.kind) {
                            self.rc_inc_top();
                        }
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
                            i.local_get(cur)
                                .local_get(bh)
                                .i32_load(len_memarg())
                                .i32_add()
                                .local_set(end);
                            i.block(BlockType::Empty).loop_(BlockType::Empty);
                            self.emit_det_charge_const(1);
                            let mut i = self.f.instructions();
                            i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                            i.local_get(cur).i32_const(koff as i32).i32_add();
                        }
                        self.load_ty_slot_at(k);
                        self.f.instructions().local_set(ki);
                        self.f.instructions().local_get(cur).i32_const(voff as i32).i32_add();
                        self.load_ty_slot_at(v);
                        self.f.instructions().local_set(vi);
                        self.lower_loop_body(body, true)?;
                        self.f
                            .instructions()
                            .local_get(cur)
                            .i32_const(esz as i32)
                            .i32_add()
                            .local_set(cur)
                            .br(0)
                            .end()
                            .end();
                        self.release_i32();
                        self.release_i32();
                        self.release_i32();
                        return Ok(());
                    }
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
                self.lower_loop_body(body, true)?;
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
                    None => match self.globals.get(&(self.var_space, *target)) {
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
                self.rc_map_value_share(value, el);
                let hv = self.hold_val(el)?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                let var_space = self.var_space;
                let get_target = |f: &mut wasm_encoder::Function, locals: &HashMap<VarId, (u32, SliceTy)>, globals: &HashMap<GVar, (u32, SliceTy)>| {
                    if is_local {
                        f.instructions().local_get(locals[target].0);
                    } else {
                        f.instructions().global_get(globals[&(var_space, *target)].0);
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
                // RC-5: the COW judge, not an unconditional copy — a
                // uniquely-held list takes the store IN PLACE, a shared one
                // copies and releases one source ref. The old
                // $block_copy-per-write materialized a fresh generation on
                // EVERY index write and never freed the outgrown one, so n
                // writes into a preallocated list retained O(n²) bytes
                // (#1729: the prealloc/fft rows OOM'd at 2^16 writes where
                // the live payload is 512 KiB).
                get_target(self.f, self.locals, self.globals);
                self.f.instructions().call(F_COW).local_set(hb);
                if is_local {
                    let idx = self.locals[target].0;
                    self.f.instructions().local_get(hb).local_set(idx);
                } else {
                    let g = self.globals[&(self.var_space, *target)].0;
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


impl Emitter<'_> {
    /// `Assign` lowering — the share/dec discipline (RC-3/RC-5) plus the
    /// growing-accumulator window, split from `lower_stmt` for the
    /// complexity budget.
    fn lower_assign(&mut self, var: &almide_ir::VarId, value: &IrExpr) -> Result<(), EmitError> {
                if self.try_str_append_assign(var, value)? {
                    return Ok(());
                }
                if self.try_list_append_assign(var, value)? {
                    return Ok(());
                }
                if self.try_map_set_assign(var, value)? {
                    return Ok(());
                }
                let (local, declared) = match self.locals.get(var) {
                    Some(&(idx, d)) => (Some(idx), d),
                    None => match self.globals.get(&(self.var_space, *var)) {
                        Some(&(gidx, d)) => (None, {
                            let _ = gidx;
                            d
                        }),
                        None => return unsup("assign:unmapped"),
                    },
                };
                // #1688: a droppable PARAM reassigned under an if/match
                // arm — one path releases the caller's block, the other
                // keeps it, and the epilogue's release set can't tell
                // which ran. The C-132 fold rewrites the provable shapes
                // away before lowering; whatever still reaches here is
                // refused, never silently emitted (native `A&B`, wasm
                // `\0\0\0` was this exact hole).
                if let Some(idx) = local
                    && idx < self.rc_param_ceiling
                    && self.branch_depth > 0
                    && self.rc_droppable(declared)
                    && !self.cells.contains(var)
                {
                    return unsup("assign:mut-param-in-branch-arm(#1688)");
                }
                self.lower(value, Some(declared))?;
                // RC-5: same share discipline as Bind.
                if matches!(declared, SliceTy::Map(..) | SliceTy::Set(_)) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                if matches!(
                    declared,
                    SliceTy::List(_) | SliceTy::Scalar(Scalar::Str | Scalar::Bytes)
                ) && !crate::rc_ownership::rc_certainly_fresh(&value.kind)
                {
                    self.rc_inc_top();
                }
                // RC-3: same ownership settlement as Bind — locals only
                // (globals are main-lifetime), never through a cell, and
                // NEVER when the rhs mentions the assigned var: a
                // self-referential assign (the C-132 write-back
                // `xs = f(xs)`, `xs = $push(xs, v)`) transfers ownership
                // through the call — the callee already released what it
                // outgrew, and a dec here double-frees (mut_heap_param
                // exit-1'd on exactly this).
                // The self-mention skip is CALL-shaped only: `xs = f(xs)` /
                // `xs = $push(xs, v)` transfer ownership through the callee
                // (a dec here double-freed — mut_heap_param). A NON-call
                // self-mentioning rhs (`data = data + [x]` — the append
                // loop's ConcatList) merely READS the old block and builds a
                // FRESH one; skipping the dec leaked every outgrown
                // generation, and a 65k-append loop exhausted the 4 GiB
                // address space in a quarter second (#1729). Aliasing rhs
                // shapes (`xs = if c then xs else ys`) stay safe by order:
                // the RC-5 inc above runs before this dec, so a same-block
                // result nets to zero.
                let call_core = match &value.kind {
                    IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => &expr.kind,
                    k => k,
                };
                let call_shaped_self = matches!(
                    call_core,
                    IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. }
                ) && crate::rc_ownership::rc_mentions_var(value, *var);
                if let Some(idx) = local
                    && !self.cells.contains(var)
                    && self.rc_droppable(declared)
                    && !call_shaped_self
                {
                    self.f.instructions().local_get(idx).call(F_DEC_FLAT);
                    self.rc_own(idx);
                }
                match local {
                    Some(idx) => self.emit_store_var(*var, idx, declared)?,
                    None => {
                        let gidx = self.globals[&(self.var_space, *var)].0;
                        self.f.instructions().global_set(gidx);
                    }
                }
                Ok(())
    }
}

impl Emitter<'_> {
    /// `p.field = v` on a record var: copy-on-write write-back — fresh
    /// block, one slot replaced, rebound. Split from `lower_stmt` for the
    /// complexity budget.
    fn lower_field_assign(
        &mut self,
        target: &almide_ir::VarId,
        field: &almide_base::intern::Sym,
        value: &IrExpr,
    ) -> Result<(), EmitError> {
                // C-319 residual: only the Assign form writes THROUGH a
                // shared cell — a field write against a cell var would land
                // in the raw local and silently diverge. Refuse honestly.
                if self.cells.contains(target) {
                    return unsup("cell-write:field-assign");
                }
                let (slot, declared) = match self.locals.get(target) {
                    Some(&(idx, d)) => (Ok(idx), d),
                    None => match self.globals.get(&(self.var_space, *target)) {
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
                self.rc_share_guard(value, fty);
                self.store_ty_slot(fty, off);
                self.f.instructions().local_get(hb);
                match slot {
                    Ok(idx) => self.f.instructions().local_set(idx),
                    Err(gidx) => self.f.instructions().global_set(gidx),
                };
                self.release_i32();
                Ok(())
    }
}
