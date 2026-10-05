//! Statement-position lowering (binds, assigns, index COW stores, loops,
//! statement markers) — split from emitter.rs for the complexity budget.

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

// Loop statements and break/continue (#2745), split for the file budget.
#[path = "stmts_loop.rs"]
mod stmts_loop;

// Does an Assign's rhs spend the var's own credit (#2616, #3127), split for
// the file budget.
#[path = "stmts_spend.rs"]
mod stmts_spend;

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
                self.lower_block_stmts(stmts, expr.as_deref())?;
                if let Some(tail) = expr {
                    self.lower_stmt_expr(tail)?;
                }
                Ok(())
            }
            IrExprKind::Call { target, args, .. } => {
                // Unit-position call: a value-returning callee's result is
                // discarded (a bare non-Unit call statement is legal IR).
                if let Some(ty) = self.lower_call(target, args)? {
                    self.discard_result(e, ty);
                }
                Ok(())
            }
            IrExprKind::If { cond, then, else_ } => self.lower_stmt_if(cond, then, else_),
            IrExprKind::While { cond, body } => self.lower_while(cond, body),
            // A statement-position match: the arm chain counts its own
            // if_ labels into the loop context (#2745), so a `break` /
            // `continue` in an arm reaches the right depth.
            IrExprKind::Match { subject, arms } => self.lower_match(subject, arms, None).map(|_| ()),
            IrExprKind::Continue => self.lower_loop_jump(false),
            IrExprKind::Break => self.lower_loop_jump(true),
            // for x in <list> / for i in a..b — extracted for complexity.
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.lower_forin(*var, var_tuple.as_deref(), iterable, body)
            }
            IrExprKind::Unit => Ok(()),
            // A Unit var read in statement position does nothing. Its local
            // holds an i32 placeholder that `lower` pushes and types Unit, so
            // the catch-all below would leave it on the stack (#2945: the tail
            // of C-132's `{ let (__mp_res, __mp_buf) = f(r)!; r = __mp_buf;
            // __mp_res }` for a `-> Result[Unit, String]` callee).
            IrExprKind::Var { id } if matches!(self.locals.get(id), Some(&(_, SliceTy::Unit))) => Ok(()),
            // Statement-position `f()!` / `f()?`: the marker machinery
            // runs (propagation/abort), the ok payload is discarded — and
            // RELEASED when the extraction handed this frame its credit
            // (#2509: an owned carrier's payload moves out, so a bare
            // `drop` here would leak exactly what the carrier stopped
            // holding). A borrowed extraction drops as before.
            IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } => {
                let ty = self.lower(e, None)?;
                self.discard_result(e, ty);
                Ok(())
            }
            // Any other value expression in statement position: evaluate
            // and discard (a bare `ok(x)` statement is legal IR) — an OWNED
            // droppable value released, as a discarded call result is.
            // A Unit VALUE is on the stack too: `lower` materializes Unit
            // as an i32 placeholder (a `()` literal, a void call under a
            // Unit want, a `r ?? ()` join over a `Result[Unit, _]`), so it
            // is dropped like any scalar — skipping it left the i32 on the
            // stack and failed validation (#3105: `fs.remove(p) ?? ()` as
            // the tail of a Unit arm).
            _ => {
                let ty = self.lower(e, None)?;
                self.discard_result(e, ty);
                Ok(())
            }
        }
    }

    /// A value-returning call's result in statement position. An OWNED
    /// droppable result arrived with its one credit (the callee-owned
    /// convention, #1986; a native arm's declared `Owned`, #2004) — the
    /// route releases it here, never leaks it (the witness records the
    /// pair, `id`). A View or a scalar carries no credit: plain drop.
    fn discard_result(&mut self, e: &IrExpr, ty: SliceTy) {
        if self.rc_droppable(ty) && self.rc_owned_result(e) {
            let dec = self.dec_fn_of(ty);
            self.f.instructions().call(dec);
            self.witness_discard();
        } else {
            self.f.instructions().drop();
        }
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
        self.witness_branch_open();
        let arms = (|| {
            self.witness_branch_arm();
            self.lower_stmt_expr(then)?;
            self.f.instructions().else_();
            self.witness_branch_arm();
            self.lower_stmt_expr(else_)
        })();
        self.witness_branch_close();
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
        if self.try_lower_guard_loop_ctl(cond, else_)? {
            return Ok(());
        }
        if self.region_repair.is_some() {
            return unsup("guard-in-region-arm");
        }
        self.lower(cond, Some(BOOL))?;
        self.f.instructions().i32_eqz().if_(BlockType::Empty);
        // #2755: a one-arm site whose arm leaves the frame (`emit_exit`).
        self.witness_branch_open();
        self.witness_branch_arm();
        match self.fn_ret {
            Some(want) => {
                // `guard c else err(m)!` in an effect fn: the `!` over a
                // Result whose type IS this fn's Result is propagation —
                // the else-arm's value is the fn's return, not its unwrapped
                // payload. Lowering the Unwrap as an unwrap produced the
                // payload type and walled with `ty-mismatch:Scalar(Int)-vs-
                // Result` (#1968), routing a correct program to the
                // incumbent (#1967). The native walker strips the same
                // wrapper (#1926).
                let bare = crate::data::err_channel::through_empty_blocks(else_);
                let ret_direct = match &bare.kind {
                    IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }
                        if matches!(want, SliceTy::Result(..))
                            && slice_ty_of(&expr.ty, self.types) == Some(want) =>
                    {
                        Some(&**expr)
                    }
                    _ => None,
                };
                let ret_e = ret_direct.unwrap_or(else_);
                match self.raw_effect_else(want, ret_direct, else_) {
                    // #3042: an effect fn's plain-value else (`guard c else
                    // ()` / `else n`) is the fn's RAW return — ok-wrapped
                    // exactly like a raw tail (func.rs), not lowered as the
                    // Result it is not.
                    Some(raw) => {
                        self.lower_raw_effect_exit(else_, raw, want)?;
                        if raw != SliceTy::Unit {
                            self.witness_exit_value(else_, raw);
                        }
                        // The ok carrier is born here and moves out.
                        self.witness_tail_owned();
                    }
                    None => {
                        self.lower(ret_e, Some(want))?;
                        // The guard's early return is an exit like the tail: a
                        // droppable value that may BORROW a local takes +1 before
                        // the frame's owners are released (#2001).
                        if self.rc_droppable(want) && !self.rc_owned_result(ret_e) {
                            self.rc_inc_top();
                        }
                        self.witness_exit_value(ret_e, want);
                    }
                }
                let plan = self.exit_plan(crate::exit_plan::Continuation::GuardReturn);
                self.emit_exit(&plan);
                self.f.instructions().return_();
            }
            // main / Unit fn: the else IS the return — evaluate it in
            // statement position (a `process.exit` else never returns).
            // A RESULT else in main is the err channel, not a discard:
            // `guard c else err(…)` must print `Error: {msg}` and exit 1
            // (#1734 — the discard silently swallowed the err). The
            // early return releases the frame like the epilogue (#2001).
            None => {
                if !self.try_lower_main_err_carrier(else_)? {
                    self.lower_stmt_expr(else_)?;
                }
                let plan = self.exit_plan(crate::exit_plan::Continuation::GuardReturn);
                self.emit_exit(&plan);
                self.f.instructions().return_();
            }
        }
        self.f.instructions().end();
        self.witness_branch_arm();
        self.witness_branch_close();
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
        // by, so a borrowed rhs takes +1 — cells included).
        // Every DROPPABLE shape shares on a borrowed rhs — the flat
        // Option / Result / tuple blocks included (`let n1: Int? = n ?? none`
        // read the nested option's payload as a view and, owning it
        // without the +1, double-freed it beside `n2`). Maps and Sets
        // included (#2010 Map stage b): their rc is a live count, the
        // in-place `map.set` window judges `rc == 1` exactly, and every
        // other mutation is a functional rebind — the bind-time copy
        // (which left a fresh rhs at rc 1 forever) is gone.
        if self.rc_droppable(declared) && !self.rc_owned_result(value) {
            self.rc_inc_top();
        }
        if self.cells.contains(var) {
            // C-319: the bind allocates the shared cell; the
            // local holds its ADDRESS from here on.
            let hv = self.hold_val(declared)?;
            self.f.instructions().local_set(hv);
            // #2010: the cell is refcounted — the frame holds one credit
            // (released at its exits, and here at a loop rebind: the
            // previous pass's cell lives on only in the envs that captured
            // it), each capturing env one more.
            let dec_cell = self.dec_cell_fn(declared);
            self.f.instructions().local_get(idx).call(dec_cell);
            self.rc_own(idx, declared);
            self.witness_cell_bind(idx, declared, value);
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
                let dec = self.dec_fn_of(declared);
                self.f.instructions().local_get(idx).call(dec);
                self.rc_own(idx, declared);
                if self.witness.is_some() {
                    self.witness_bind(idx, declared, value);
                }
            }
            self.f.instructions().local_set(idx);
        }
        Ok(())
    }

    /// `m[k] = v` on a C-319 cell (#3339) — the `map.insert` mut form,
    /// whose write-back reads the occupant through the cell, releases it
    /// and stores the functional `set`'s fresh block back. It walled before:
    /// a captured-and-written var is such a cell (a module `var` written
    /// from a callback too), so `seen[k] = v` in a closure was E082.
    fn lower_cell_map_insert(&mut self, target: &VarId, key: &IrExpr, value: &IrExpr) -> Result<(), EmitError> {
        let var_expr = IrExpr { kind: IrExprKind::Var { id: *target }, ty: Ty::Unit, span: None, def_id: None };
        let args = self.owned_call_marks.pin_args(vec![var_expr, key.clone(), value.clone()]);
        self.arm_scope(|em| em.lower_map_call("insert", &args, None))?;
        Ok(())
    }

    pub(crate) fn rc_own(&mut self, idx: u32, ty: SliceTy) {
        self.rc_owned.insert(idx);
        self.owned_ty.insert(idx, ty);
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
                self.lower_field_assign(target, std::slice::from_ref(field), value)
            }
            // `m[k] = v` on a map var — the in-place window when the var
            // owns its block (#1219), else the same write-back the
            // `map.insert` mut form runs (functional `set`, rebind).
            IrStmtKind::MapInsert { target, key, value } => {
                if self.try_map_set_in_place(target, key, value)? {
                    return Ok(());
                }
                if self.cells.contains(target) {
                    return self.lower_cell_map_insert(target, key, value);
                }
                let Some(&(var_idx, _)) = self.locals.get(target) else {
                    return unsup("map-insert:unmapped");
                };
                // #2758: this write-back records no rebind of the var's block.
                self.witness_decline("map-insert:functional");
                let var_expr = IrExpr {
                    kind: IrExprKind::Var { id: *target },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                };
                // Pinned: clones whose marks are keyed by address (#3143).
                let args = self.owned_call_marks.pin_args(vec![var_expr, key.clone(), value.clone()]);
                self.arm_scope(|em| em.lower_map_call("set", &args, None))?;
                self.f.instructions().local_set(var_idx);
                Ok(())
            }
            IrStmtKind::Assign { var, value } => self.lower_assign(var, value),
            IrStmtKind::IndexAssign { target, index, value } => {
                self.lower_index_assign(target, index, value)
            }
            IrStmtKind::Expr { expr } => self.lower_stmt_expr(expr),
            // let (a, b) = e — evaluate once, load each bound position.
            IrStmtKind::BindDestructure { pattern, value } => {
                let ty = self.lower(value, None)?;
                let scr = self.scr_i32_local;
                self.f.instructions().local_set(scr);
                self.emit_pattern_binds(pattern, ty, scr)?;
                self.witness_pattern_views(pattern);
                Ok(())
            }
            IrStmtKind::Comment { .. } => Ok(()),
            IrStmtKind::Guard { cond, else_ } => self.lower_stmt_guard(cond, else_),
            other => unsup(&format!("stmt:{}", stmt_kind_name(other))),
        }
    }

    /// Push an i32 "leave the counting loop" for a Range head: `var >= stop`
    /// (exclusive) / `var > stop` (inclusive). The inclusive test also leaves
    /// when `var < floor` (the range's start): a range ending at i64::MAX
    /// steps its index past the last element to i64::MIN, which `var > stop`
    /// alone never catches (#2679). An exclusive loop cannot wrap.
    fn range_exit_test(&mut self, var_idx: u32, floor: u32, stop: u32, inclusive: bool) {
        let mut i = self.f.instructions();
        i.local_get(var_idx).local_get(stop);
        if inclusive {
            i.i64_gt_s();
            i.local_get(var_idx).local_get(floor).i64_lt_s();
            i.i32_or();
        } else {
            i.i64_ge_s();
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
                    // The inclusive loop's no-wrap floor (#2679): `start`.
                    let floor = self.hold_i64()?;
                    self.f.instructions().local_get(var_idx).local_set(floor);
                    let flags = self.hoist_cow_flags(None, body)?;
                    let incl = *inclusive;
                    let pre = self.prejudge_first_stores(None, body, &|e: &mut Self| {
                        e.range_exit_test(var_idx, floor, stop, incl);
                        e.f.instructions().i32_eqz();
                        Ok(())
                    })?;
                    let ptrs = self.hoist_payload_ptrs(None, body, &pre)?; // #3345
                    self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                    self.emit_det_charge_const(1);
                    self.range_exit_test(var_idx, floor, stop, *inclusive);
                    self.f.instructions().br_if(1);
                    self.witness_loop_open();
                    self.lower_loop_body(body, true)?;
                    self.witness_loop_close();
                    self.f
                        .instructions()
                        .local_get(var_idx)
                        .i64_const(1)
                        .i64_add()
                        .local_set(var_idx)
                        .br(0)
                        .end()
                        .end();
                    self.drop_payload_ptrs(ptrs);
                    self.drop_prejudged(pre);
                    self.drop_cow_flags(flags);
                    self.release_i64();
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
                        let flags = self.hoist_cow_flags(None, body)?;
                        let pre = self.prejudge_first_stores(None, body, &|e: &mut Self| {
                            e.range_exit_test(var_idx, sl, el, inclusive);
                            e.f.instructions().i32_eqz();
                            Ok(())
                        })?;
                        let ptrs = self.hoist_payload_ptrs(None, body, &pre)?; // #3345
                        self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                        self.emit_det_charge_const(1);
                        self.range_exit_test(var_idx, sl, el, inclusive);
                        self.f.instructions().br_if(1);
                        self.witness_loop_open();
                        self.lower_loop_body(body, true)?;
                        self.witness_loop_close();
                        self.f
                            .instructions()
                            .local_get(var_idx)
                            .i64_const(1)
                            .i64_add()
                            .local_set(var_idx)
                            .br(0)
                            .end()
                            .end();
                        self.drop_payload_ptrs(ptrs);
                    self.drop_prejudged(pre);
                        self.drop_cow_flags(flags);
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
                        // witnesses a second holder (a borrowed subject
                        // takes +1; an owned one is the cursor's own), and
                        // the cursor releases its credit after the loop.
                        let owned = self.rc_owned_result(iterable);
                        if !owned {
                            self.rc_inc_top();
                        }
                        let drop_map = self.dec_fn_of(SliceTy::Map(kh, vh));
                        let bh = self.hold_i32()?;
                        let cur = self.hold_i32()?;
                        let end = self.hold_i32()?;
                        if let Some(w) = self.witness.as_mut() {
                            w.cursor_take(owned, bh);
                        }
                        // #3374: for the body's duration the cursor's credit
                        // is a FRAME credit, so every exit edge out of the
                        // body (a `!`, a guard return) releases it through
                        // its exit plan, as the loop end below does. A
                        // `break` / `continue` stays in the frame and reaches
                        // that release itself.
                        self.rc_owned.insert(bh);
                        self.owned_ty.insert(bh, SliceTy::Map(kh, vh));
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
                        // One activation per entry; the key and the value are
                        // VIEWS of the entry's slots.
                        self.witness_loop_open();
                        self.load_ty_slot_at(k);
                        self.f.instructions().local_set(ki);
                        self.witness_view_local(ki, k);
                        self.f.instructions().local_get(cur).i32_const(voff as i32).i32_add();
                        self.load_ty_slot_at(v);
                        self.f.instructions().local_set(vi);
                        self.witness_view_local(vi, v);
                        self.lower_loop_body(body, true)?;
                        self.witness_loop_close();
                        self.f
                            .instructions()
                            .local_get(cur)
                            .i32_const(esz as i32)
                            .i32_add()
                            .local_set(cur)
                            .br(0)
                            .end()
                            .end();
                        self.rc_owned.remove(&bh);
                        self.owned_ty.remove(&bh);
                        self.f.instructions().local_get(bh).call(drop_map);
                        if let Some(w) = self.witness.as_mut() {
                            w.cursor_release(bh);
                        }
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
                self.witness_loop_open();
                self.load_ty_slot(elem, 0);
                self.f.instructions().local_set(var_idx);
                self.witness_view_local(var_idx, elem);
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
                        self.witness_view_local(tidx, fty);
                    }
                }
                self.lower_loop_body(body, true)?;
                self.witness_loop_close();
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

}

impl Emitter<'_> {
    /// `Assign` lowering — the share/dec discipline (RC-3/RC-5) plus the
    /// growing-accumulator window, split from `lower_stmt` for the
    /// complexity budget.
    fn lower_assign(&mut self, var: &almide_ir::VarId, value: &IrExpr) -> Result<(), EmitError> {
                let moved = self.take_moved_temp(value);
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
                // #1688 once refused a droppable PARAM reassigned under an
                // if/match arm ("one path releases the caller's block, the
                // other keeps it"). Under the credit discipline the local
                // holds exactly ONE credit on every path — the assign
                // releases the old occupant and makes the local an owner,
                // the epilogue releases the local once whichever arm ran —
                // and the exit validator (E083) checks it; the refusal is
                // retired (stage 2c-ii: records made the mut_port cell hit it).
                self.lower(value, Some(declared))?;
                // RC-5: same share discipline as Bind — except a MOVED
                // temp (#3104), whose one credit becomes the var's.
                if self.rc_droppable(declared) && !self.rc_owned_result(value) && moved.is_none() {
                    self.rc_inc_top();
                }
                // RC-3: same ownership settlement as Bind — here for locals
                // (a global's is at the store below), never through a cell, and
                // NEVER when the rhs SPENDS the assigned var's own credit
                // (`assign_rhs_spends_var`): then the callee already released
                // or reallocated the old block, and a dec here double-frees
                // (mut_heap_param exit-1'd on exactly this). Every other rhs
                // leaves the old occupant this local's to release. Aliasing
                // rhs shapes (`xs = if c then xs else ys`) stay safe by order:
                // the RC-5 inc above runs before this dec, so a same-block
                // result nets to zero.
                let rhs_spends_var = self.assign_rhs_spends_var(value, *var);
                if let Some(idx) = local
                    && !self.cells.contains(var)
                    && self.rc_droppable(declared)
                    && !rhs_spends_var
                {
                    let dec = self.dec_fn_of(declared);
                    self.f.instructions().local_get(idx).call(dec);
                    self.rc_own(idx, declared);
                }
                // The witness (#2757): a droppable local's occupant changes
                // here — or a global's / a cell's, an outer holder the frame
                // borrows (#2755, `witness_holder`), settled the same way below.
                if self.rc_droppable(declared)
                    && let Some(idx) = self.witness_holder(*var, local.is_none())
                {
                    match moved {
                        Some(src) => self.witness_transfer(idx, !rhs_spends_var, src),
                        None => self.witness_assign(idx, !rhs_spends_var, rhs_spends_var, value),
                    }
                }
                // #2010: a C-319 cell's occupant is released as it is
                // replaced (the cell holds exactly one credit on it) — the
                // same settlement, one load deeper.
                if let Some(idx) = local
                    && self.cells.contains(var)
                    && self.rc_droppable(declared)
                    && !rhs_spends_var
                {
                    let dec = self.dec_fn_of(declared);
                    self.f.instructions().local_get(idx);
                    self.load_ty_slot(declared, 0);
                    self.f.instructions().call(dec);
                }
                match local {
                    Some(idx) => self.emit_store_var(*var, idx, declared)?,
                    None => {
                        let gidx = self.globals[&(self.var_space, *var)].0;
                        // #2992: a top-let global holds ONE credit on its
                        // occupant for the program's life (its initializer
                        // takes it, func.rs), so replacing the occupant
                        // releases it — the local settlement above, read
                        // through the global. Without it every `g = …` in a
                        // loop kept the previous block alive.
                        if self.rc_droppable(declared) && !rhs_spends_var {
                            let dec = self.dec_fn_of(declared);
                            self.f.instructions().global_get(gidx).call(dec);
                        }
                        self.f.instructions().global_set(gidx);
                    }
                }
                self.empty_moved_temp(moved);
                Ok(())
    }
}

impl Emitter<'_> {
    /// `p.field = v` on a record var: copy-on-write write-back — fresh
    /// block, one slot replaced, rebound. Split from `lower_stmt` for the
    /// complexity budget; the core is `field_assign_with` (list_mut.rs),
    /// which the record-field forms of the mut list/map ops share (#2411).
    pub(crate) fn lower_field_assign(
        &mut self,
        target: &almide_ir::VarId,
        path: &[almide_base::intern::Sym],
        value: &IrExpr,
    ) -> Result<(), EmitError> {
        let moved = self.take_moved_temp(value);
        let spends_var = self.assign_rhs_spends_var(value, *target);
        self.field_assign_with(target, path, spends_var, |s, fty| {
            s.lower(value, Some(fty))?;
            if moved.is_none() {
                s.rc_share_guard(value, fty);
            }
            s.witness_field_value(value, fty, moved, spends_var);
            Ok(())
        })?;
        self.empty_moved_temp(moved);
        Ok(())
    }
}

