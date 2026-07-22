impl LowerCtx {

    /// Drop every heap handle the current scope frame added beyond `mark` (LIFO),
    /// restoring `live_heap_handles` to its pre-frame length — the per-arm teardown.
    pub(crate) fn drop_arm_locals(&mut self, mark: usize) {
        // Release the handles created SINCE `mark`. Clamp to the current length: a sub-lowering
        // (e.g. a match that drops its own subject) can leave `live_heap_handles` SHORTER than
        // `mark`, in which case nothing new is live and this is a no-op (a bare `split_off(mark)`
        // would panic). Semantically "drop everything from mark onward, nothing if none".
        let mark = mark.min(self.live_heap_handles.len());
        for v in self.live_heap_handles.split_off(mark).into_iter().rev() {
            let op = self.drop_op_for(v);
            self.ops.push(op);
        }
    }

    /// Lower a `for v in iterable { body }` by modeling ONE iteration with a
    /// PER-ITERATION SCOPE FRAME. Each iteration is internally balanced (its loop
    /// variable + body locals are all dropped at iteration end), so N runtime
    /// iterations are N balanced episodes — no cross-iteration leak or double-free,
    /// and the flat cert (one iteration) is sound for any N (including 0: every op is
    /// in a balanced frame). NO loop op — the iteration discipline lives entirely in
    /// the lowering, the checker stays a flat fold.
    ///
    /// The ITERABLE is evaluated once: a heap iterable is lowered by `lower_call_args`
    /// — an already-tracked `Var` is BORROWED, a FRESH heap value (a call/literal
    /// result) is MATERIALIZED into an owned temp released at the OUTER scope; a scalar
    /// iterable (a `Range`) carries no ownership. The LOOP VARIABLE binds one element per
    /// iteration: a HEAP element ALIASES the whole container (`Op::Dup`, container-
    /// grain like field extraction — it keeps the container alive for the iteration,
    /// dropped at its end; element-precise identity needs the layout brick), a SCALAR
    /// element is a `Const`. A `break`/`continue` is a no-op admitted ONLY over a
    /// SCALAR-only frame (`wall_break_over_heap_frame`); over a heap frame it is WALLED
    /// (a real early exit would skip a per-iteration heap Drop = a wasm leak). A HEAP
    /// reassignment (the accumulator, `acc = acc + [x]`) is DEFERRED, not walled: the
    /// `in_frame` discipline keeps `acc` pinned to its still-live handle across
    /// iterations (memory-safe; the accumulation is deferred like every `Opaque`) and it
    /// is not a frame handle. A scalar reassignment (`i = i + 1`) is a Copy `Const`,
    /// harmless, admitted.
    pub(crate) fn lower_for_in(
        &mut self,
        var: VarId,
        var_tuple: &Option<Vec<VarId>>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), LowerError> {
        // First try to EXECUTE a scalar `for i in start..end` as a real loop; out of that
        // subset it rolls back and we keep the model-one-iteration form below.
        if self.try_lower_scalar_for_range(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // Then try to EXECUTE `for x in xs` over a List[Int] as a real element loop.
        if self.try_lower_scalar_for_list(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // Then `for (k, v) in m` / `for k in m` over a self-hosted Map layout as a
        // real entry loop.
        if self.try_lower_scalar_for_map(var, var_tuple, iterable, body) {
            return Ok(());
        }
        // The iterable is evaluated ONCE before the loop. A heap iterable goes through
        // `lower_call_args` — an already-tracked `Var` is borrowed (no new ownership),
        // a fresh heap value is materialized into an owned temp dropped at the OUTER
        // scope (its caps captured by the lowering). A scalar iterable (a `Range`)
        // carries no ownership; capture any call in it for caps.
        let container: Option<ValueId> = if is_heap_ty(&iterable.ty)
            // A Range ITERABLE stays on the no-container model path (it carries no
            // ownership; the call-arg Range materialization emits a `list.range` CallFn
            // the caps ledger only accounts for in ARGUMENT position).
            && !matches!(&iterable.kind, IrExprKind::Range { .. })
        {
            match self.lower_call_args(std::slice::from_ref(iterable))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(iterable);
            None
        };
        let mark = self.live_heap_handles.len();
        let vars: Vec<VarId> = match var_tuple {
            Some(vs) => vs.clone(),
            None => vec![var],
        };
        for v in vars {
            // A heap element aliases the whole container; a scalar element is a Const.
            let elem_ty = find_var_ty(body, v);
            let elem_heap = elem_ty.as_ref().map(|t| is_heap_ty(t)).unwrap_or(false);
            if elem_heap {
                // The model-one-iteration fallback aliases the element to the WHOLE container (a `Dup`
                // of the list handle) — the real per-element loop (`try_lower_scalar_for_list`) only
                // covers List[scalar]/List[String], which never reach here. A heap-AGGREGATE element
                // (tuple/record) DOES reach here, and reading a field of it (`for p in ps { p.0 }`)
                // would read the CONTAINER's slot, not the element's — a SILENT MISCOMPILE. Now that
                // the producer side materializes List[tuple]/List[record] returns, that consumer is
                // reachable, so WALL it (honest) until the heap-aggregate per-element borrow lands.
                if elem_ty.as_ref().is_some_and(|t| self.aggregate_field_tys(t).is_some())
                    && body_reads_var_field(body, v)
                {
                    return Err(LowerError::Unsupported(
                        "for-in over a List of heap aggregates (tuple/record) whose element is read \
                         via a direct field/index (`for p in ps { p.0 }`) is not in this brick: the \
                         model-one-iteration fallback aliases the element to the whole container, so \
                         the projection would read the container — walled to avoid a silent \
                         miscompile (needs the heap-aggregate per-element borrow)"
                            .into(),
                    ));
                }
                let src = container.ok_or_else(|| {
                    LowerError::Unsupported(
                        "for-in heap loop variable over a non-container iterable not in this brick".into(),
                    )
                })?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                self.value_of.insert(v, dst);
                self.live_heap_handles.push(dst);
            } else {
                if crate::lower::strict_values() {
                    return Err(crate::lower::strict_const_wall("for-in loop variable"));
                }
                let dst = self.fresh_value();
                self.ops.push(Op::Const { dst });
                self.value_of.insert(v, dst);
            }
        }
        // A heap reassignment in the body is the loop ACCUMULATOR (`acc = acc + [x]`):
        // it is DEFERRED, not rebound (the `in_frame` discipline) — `acc` keeps its
        // still-live handle across iterations, so the next iteration never dereferences
        // a freed handle. Memory-safe; the accumulation itself is deferred like `Opaque`.
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.wall_break_over_heap_frame(body, "for-in", mark)?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Lower a `while cond { body }` like a `for-in` body — a PER-ITERATION SCOPE
    /// FRAME makes one modeled iteration balanced, sound for any N. The condition is
    /// evaluated each iteration (its caps captured); the body's locals are dropped at
    /// iteration end. Same as `for-in`: a `break`/`continue` over a HEAP frame is walled
    /// (post-lowering), a heap reassignment (accumulator) deferred by `in_frame`.
    /// Try to lower `while cond { body }` as a REAL scalar-state loop that EXECUTES N
    /// times (the `LoopStart`/`LoopBreakUnless`/`LoopEnd` markers), reassigning scalar
    /// loop-carried state via [`Op::SetLocal`]. Restricted to the sound + runnable subset:
    /// an Int/Bool cond, NO `break`/`continue` (a no-op early-exit would be wrong inside a
    /// real loop), and a body with NO heap reassignment (the `scalar_loop_depth` Assign
    /// rule errors on one) and NO net heap handle escaping the per-iteration frame. The
    /// cond ops sit INSIDE the loop (re-evaluated each iteration); per-iteration heap (a
    /// string literal in `println`) is dropped before the back-edge. SOUNDNESS by REUSE:
    /// the markers are no-ops in verify_ownership and the body is a per-iteration-balanced
    /// frame — the cert verifies ONE balanced iteration, sound for any N (the existing
    /// model-one-iteration argument), the markers only make wasm actually run it N times.
    /// Returns false (and rolls back) when out of subset; `lower_while` then falls back.
    /// Pre-loop OWNED COPY for every borrowed-param slot the loop body HEAP-REASSIGNS
    /// (the C-132 callee shape after the move-mode rewrite: `fn addc(mut s: String, n)
    /// = { while k < n { s = s + "x"; … } … }` — the rebind arrives via the string.push
    /// functional rewrite). The loop rebind's uniform drop-old would free the CALLER's
    /// buffer on iteration 1 (the mut_heap_param rc-underflow trap); starting the slot
    /// as a `Dup` (+1) of the borrowed param makes iteration 1 drop the copy (the
    /// caller's reference stays live) and later iterations drop the owned
    /// intermediates — the same accounting the TCO pre-copy proves. The Dup joins
    /// `live_heap_handles` (scope-end drops the FINAL object through the same local);
    /// its cert `a` is backed by the real `Op::Dup` (the borrow-by-default gate).
    pub(crate) fn precopy_borrowed_reassign_slots(&mut self, body: &[IrStmt]) {
        let mut vars: Vec<VarId> = Vec::new();
        collect_heap_reassign_vars(body, &mut vars);
        for var in vars {
            if let Some(&val) = self.value_of.get(&var) {
                if self.param_values.contains(&val) {
                    let owned = self.fresh_value();
                    self.ops.push(Op::Dup { dst: owned, src: val });
                    self.value_of.insert(var, owned);
                    self.live_heap_handles.push(owned);
                }
            }
        }
    }

    pub(crate) fn try_lower_scalar_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> bool {
        if !matches!(cond.ty, Ty::Int | Ty::Bool) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();
        self.precopy_borrowed_reassign_slots(body);

        self.ops.push(Op::LoopStart);
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
                return false;
            }
        };
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.scalar_loop_depth += 1;
        let mut ok = true;
        for stmt in body {
            if self.lower_while_body_stmt(stmt).is_err() {
                ok = false;
                break;
            }
        }
        self.scalar_loop_depth -= 1;
        self.in_frame -= 1;

        if !ok {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
            return false;
        }
        // Per-iteration heap (a string literal in a body `println`) is released before the
        // back-edge, INSIDE the loop, so each iteration is balanced.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::LoopEnd);
        true
    }

    /// One while-body statement, admitting the CONDITIONAL-BREAK forms the real loop
    /// can execute with the EXISTING marker vocabulary (no new op):
    ///   `if c then <rest> else break`  (the guard-else-break desugar) →
    ///       `LoopBreakUnless(c)` then <rest> emitted linearly — on the broken path the
    ///       `br` already exited, exactly like the loop-head condition;
    ///   `if c then break else ()`      (the do-block shape) →
    ///       `LoopBreakUnless(1 - c)`.
    /// Any OTHER Break/Continue in the statement ERRS (aborting the attempt → the
    /// model-one-iteration fallback then WALLS): `lower_stmt` silently swallows a bare
    /// Break (mod_p3), so delegating one would silently drop the early exit.
    /// `{ A…; break }` → `Some({ A… })` — the then-arm of a mid-body conditional break
    /// with statements BEFORE the break. `None` when the expr does not end in a bare
    /// trailing break (or has nothing before it — the simpler cases own that shape).
    fn strip_trailing_break_expr(e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Block { stmts, expr } = &e.kind else { return None };
        // Trailing break as the block TAIL.
        if let Some(t) = expr.as_deref() {
            if matches!(t.kind, IrExprKind::Break) && !stmts.is_empty() {
                return Some(IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: None },
                    ty: almide_lang::types::Ty::Unit,
                    span: e.span.clone(),
                    def_id: e.def_id,
                });
            }
            return None;
        }
        // Trailing break as the LAST statement.
        if stmts.len() >= 2 {
            if let IrStmtKind::Expr { expr: last } = &stmts[stmts.len() - 1].kind {
                if matches!(last.kind, IrExprKind::Break) {
                    return Some(IrExpr {
                        kind: IrExprKind::Block {
                            stmts: stmts[..stmts.len() - 1].to_vec(),
                            expr: None,
                        },
                        ty: almide_lang::types::Ty::Unit,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    });
                }
            }
        }
        None
    }

    fn lower_while_body_stmt(&mut self, stmt: &IrStmt) -> Result<(), LowerError> {
        fn is_break(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Break => true,
                IrExprKind::Block { stmts, expr } => {
                    (stmts.is_empty() && expr.as_deref().is_some_and(is_break))
                        || (expr.is_none()
                            && stmts.len() == 1
                            && matches!(&stmts[0].kind,
                                IrStmtKind::Expr { expr } if is_break(expr)))
                }
                _ => false,
            }
        }
        fn is_unit(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Unit => true,
                IrExprKind::Block { stmts, expr } => {
                    stmts.is_empty() && expr.as_deref().map_or(true, is_unit)
                }
                _ => false,
            }
        }
        if let IrStmtKind::Expr { expr } = &stmt.kind {
            // A BARE break statement (`if true then break else ()` const-folds to it):
            // break unconditionally — LoopBreakUnless over a 0 cond.
            if matches!(expr.kind, IrExprKind::Break) {
                let z = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: z, value: 0 });
                self.ops.push(Op::LoopBreakUnless { cond: z });
                return Ok(());
            }
            if let IrExprKind::If { cond, then, else_ } = &expr.kind {
                if is_break(else_) {
                    let c = self.lower_scalar_value(cond).ok_or_else(|| {
                        LowerError::Unsupported("while conditional-break cond".into())
                    })?;
                    self.ops.push(Op::LoopBreakUnless { cond: c });
                    return self.lower_while_body_inline(then);
                }
                if is_break(then) && is_unit(else_) {
                    let c = self.lower_scalar_value(cond).ok_or_else(|| {
                        LowerError::Unsupported("while conditional-break cond".into())
                    })?;
                    let one = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: one, value: 1 });
                    let nc = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: nc, op: IntOp::Sub, a: one, b: c });
                    self.ops.push(Op::LoopBreakUnless { cond: nc });
                    return Ok(());
                }
                // `if c then { A…; break } else B` (find_factor: `if n % i == 0 then
                // { result = i; break } else { i = i + 1 }`): CAPTURE the (call-free,
                // pure-scalar) cond once, run the ordinary unit `if` with the trailing
                // break STRIPPED (both arms then break-free — the statement-if machinery
                // branches the arm assigns correctly), and break on the CAPTURED value
                // after. The capture keeps the break test the value the branch dispatched
                // on even when an arm mutates the cond's operands (`i = i + 1`).
                if let Some(then_stripped) = Self::strip_trailing_break_expr(then) {
                    if !body_breaks_or_continues(std::slice::from_ref(&IrStmt {
                        kind: IrStmtKind::Expr { expr: then_stripped.clone() },
                        span: stmt.span.clone(),
                    })) && !body_breaks_or_continues(std::slice::from_ref(&IrStmt {
                        kind: IrStmtKind::Expr { expr: (**else_).clone() },
                        span: stmt.span.clone(),
                    })) && !crate::lower::expr_contains_call(cond)
                    {
                        let c = self.lower_scalar_value(cond).ok_or_else(|| {
                            LowerError::Unsupported("while conditional-break cond".into())
                        })?;
                        let unit_if = IrStmt {
                            kind: IrStmtKind::Expr {
                                expr: IrExpr {
                                    kind: IrExprKind::If {
                                        cond: cond.clone(),
                                        then: Box::new(then_stripped),
                                        else_: else_.clone(),
                                    },
                                    ty: almide_lang::types::Ty::Unit,
                                    span: expr.span.clone(),
                                    def_id: expr.def_id,
                                },
                            },
                            span: stmt.span.clone(),
                        };
                        self.lower_stmt(&unit_if)?;
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        let nc = self.fresh_value();
                        self.ops.push(Op::IntBinOp { dst: nc, op: IntOp::Sub, a: one, b: c });
                        self.ops.push(Op::LoopBreakUnless { cond: nc });
                        return Ok(());
                    }
                }
            }
        }
        if body_breaks_or_continues(std::slice::from_ref(stmt)) {
            return Err(LowerError::Unsupported(
                "unrecognized break/continue in a while body (lower_stmt would silently \
                 swallow it) not in this brick"
                    .into(),
            ));
        }
        self.lower_stmt(stmt)
    }

    /// Inline the surviving arm of a while conditional-break (`if c then <rest> else
    /// break`): a Block's statements re-enter [`Self::lower_while_body_stmt`] (a nested
    /// conditional break chains), a unit tail is nothing, any other tail lowers as an
    /// effect statement.
    fn lower_while_body_inline(&mut self, e: &IrExpr) -> Result<(), LowerError> {
        match &e.kind {
            IrExprKind::Unit => Ok(()),
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_while_body_stmt(s)?;
                }
                match expr.as_deref() {
                    Some(t) => self.lower_while_body_inline(t),
                    None => Ok(()),
                }
            }
            _ => self.lower_while_body_stmt(&IrStmt {
                kind: IrStmtKind::Expr { expr: e.clone() },
                span: e.span.clone(),
            }),
        }
    }

    /// Roll back a scalar-loop ATTEMPT (`try_lower_scalar_while` / `_for_range` / `_for_list`),
    /// restoring EVERY side-effect the partial body lowering may have produced — not only `ops`
    /// but the LAMBDA-LIFTED auxiliaries (`self.lifted`). A lambda call-arg in the body (`for x in
    /// xs { … list.map([y], (u) => …) … }`) lifts a `__lambda_*` MirFunction into `self.lifted`;
    /// if the attempt then rolls back (a heap reassignment aborts it → the model-one-iteration
    /// fallback re-lowers the SAME body, re-lifting the lambda), the abandoned first copy would
    /// survive and DOUBLE-COUNT its inner calls (a `mir > ir` wall breach). Truncating `lifted` to
    /// `lifted_mark` (captured at THIS attempt's start, threaded as a local so NESTED loop attempts
    /// each roll back to their own floor) makes the rollback total.
    fn rollback_scalar_loop(
        &mut self,
        ops_mark: usize,
        lhh_mark: usize,
        lifted_mark: usize,
        value_of_snapshot: std::collections::HashMap<almide_ir::VarId, ValueId>,
    ) {
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        self.lifted.truncate(lifted_mark);
        self.value_of = value_of_snapshot;
    }

    /// Try to lower a HEAP-RESULT `if cond then A else B` (a String/data-returning branch)
    /// to EXECUTABLE control flow — only the taken arm allocates, and its value is the
    /// function result. SOUNDNESS by PER-ARM BALANCE (no Coq change — see
    /// docs/roadmap/active/v1-heap-result-control-flow.md): each arm `Alloc`s its value
    /// (cert `i`) AND `Consume`s it (cert `m`) so the arm is internally `"im"` balanced
    /// exactly like a scalar arm carries none; the `IfThen` result `dst` is NEVER an
    /// `Alloc`, so it is not in the ownership cert's object set and `func.ret = dst` emits
    /// no second move-out (no double-free). The render selects one arm at runtime
    /// (`(if (result i32) …)`), so exactly one `Alloc` happens and is returned rc=1 to the
    /// caller — the untaken arm never allocates (no leak). FIRST version: both arms are
    /// direct string LITERALS (the common `if c then "a" else "b"`); other arm kinds fall
    /// back to today's sound Opaque form. Returns the result `dst`, or `None` (rolled
    /// back) when out of subset. Arms may be string LITERALS or a NESTED heap-result `if`
    /// (the else-if chain a desugared `match` produces — `match n { 0 => "a", _ => "b" }`),
    /// recursively. Other arm kinds fall back to today's sound Opaque form.
    pub(crate) fn try_lower_heap_result_if(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        if !is_heap_ty(result_ty) {
            return None;
        }
        // The whole attempt rolls back as a unit: the recursion below truncates nothing,
        // so the OUTERMOST call restores the op stream AND the live-handle set on any
        // out-of-subset arm (a call arm may have materialized + tracked a temp).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let result = self.lower_heap_result_if_inner(cond, then, else_, result_ty);
        if result.is_none() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        result
    }

    /// Materialize the CONDITION of a heap-result `if` to a scalar (Bool = i64 0/1)
    /// BEFORE the `IfThen` marker. The common shape is a pure `lower_scalar_value` cond
    /// (a comparison, a Var, a literal) — tried first, no ownership. When that defers, a
    /// Bool/Int-returning PURE call WITH HEAP ARGS (`if string.contains(s, x) then …`,
    /// `if list.is_empty(xs) then …`) is materialized via `try_lower_scalar_call`: the
    /// call's heap arg temps are pushed to `live_heap_handles`, and a per-cond frame
    /// (`drop_arm_locals`) frees them IMMEDIATELY after the call — they are transient to
    /// the condition, not owned by either arm. The scalar result is not a heap handle, so
    /// it survives the frame teardown. SOUND: the cond eval is internally balanced (each
    /// arg temp alloc'd `i` + dropped `d` within the frame), exactly the per-arm
    /// discipline; outside the pure-scalar-call subset it walls (`None` → Opaque). The
    /// gate keeping a heap-arg call OUT of `lower_scalar_value` (its rollback-safe, no-
    /// ownership contract) does not bind here — this position freely emits ownership ops.
    fn lower_heap_result_cond(&mut self, cond: &IrExpr) -> Option<ValueId> {
        // A scalar cond can itself MATERIALIZE a transient heap temp — `if c == "#"` lowers to
        // `string.eq(c, "#")` whose `"#"` literal is a fresh owned String. That temp is dead the
        // instant the Bool is computed, so it MUST be freed HERE, within a cond-local frame, BEFORE
        // the `IfThen` marker — never deferred to the enclosing arm's `drop_arm_locals`. The cond
        // of a NESTED heap-result `if` (the else-of-an-else parser shape) sits inside one arm's
        // wasm branch; deferring its temp's `Drop` to the OUTER block scope emits an UNCONDITIONAL
        // `rc_dec` of a local that the sibling arm never initialized (garbage/0 → trap). The frame
        // keeps the cond internally `i…d`-balanced exactly where it executes.
        let frame = self.live_heap_handles.len();
        if let Some(v) = self.lower_scalar_value(cond) {
            self.drop_arm_locals(frame);
            return Some(v);
        }
        // A scalar-returning (Bool/Int) PURE call with heap args — materialize it, then
        // free the transient arg temps within a cond-local frame.
        if let IrExprKind::Call { .. } = &cond.kind {
            if !is_heap_ty(&cond.ty) {
                if let Some(v) = self.try_lower_scalar_call(cond, &cond.ty) {
                    self.drop_arm_locals(frame);
                    return Some(v);
                }
            }
        }
        // A Bool-returning `==`/`!=` over HEAP operands neither of which is a tracked Var
        // (`list.first(xs) == some("")`, `string.first(s) == some(c)` — the toml `emit_sections`
        // blocker). `lower_scalar_value` above already covers the case where BOTH operands are
        // materialized Vars (its Option/Result eq reads them via `materialized_option_handle`);
        // here the operands are a heap-returning CALL and/or an Option/Result CTOR, which it
        // declines. Materialize EACH operand into an owned block in the SAME cond-local frame, run
        // the typed eq over their handles, then `drop_arm_locals(frame)` frees the operand temps —
        // they are transient to the condition, owned by NEITHER arm (the per-cond `i…d` balance,
        // exactly the pure-call-cond path's discipline). A non-materializable operand returns None
        // → the `if` keeps its sound Opaque wall (no invalid wasm, no leak).
        if let IrExprKind::BinOp { op: op @ (almide_ir::BinOp::Eq | almide_ir::BinOp::Neq), left, right } = &cond.kind {
            if matches!(cond.ty, Ty::Bool) && is_heap_ty(&left.ty) {
                if let Some(v) = self.lower_heap_eq_cond(left, right, matches!(op, almide_ir::BinOp::Neq)) {
                    self.drop_arm_locals(frame);
                    return Some(v);
                }
                // On decline, the partial ops/handles a half-materialized operand left are restored
                // by the OUTER `try_lower_heap_result_if` rollback (ops + live_heap_handles truncated
                // to its marks on any `None`), so the caller's Opaque fallback starts clean — no
                // extra teardown needed here.
            }
        }
        None
    }

    /// Lower a HEAP `left == right` / `left != right` cond (Bool) whose operands are NOT both
    /// tracked Vars — by MATERIALIZING each operand into an owned block (tracked in the cond
    /// frame so `drop_arm_locals` frees it after the compare) and running the typed eq over the
    /// two block handles. Reuses the SAME eq cores the Var-operand path uses
    /// (`option_scalar_eq_from_handles` / `option_heap_eq_from_handles` /
    /// `result_scalar_eq_from_handles`, `string.eq`, `value.eq`, `list.eq_*`) — no new eq op.
    /// `negate` wraps the result as `1 - eq` (for `!=`). SELF-CONTAINED ROLLBACK: on any decline a
    /// half-materialized operand may have emitted ops + pushed an owned temp to `live_heap_handles`;
    /// this restores BOTH to the pre-attempt marks so the method leaves NO trace on `None` (the
    /// rollback-safe contract the cond position requires — `lower_heap_result_cond` is reached from
    /// paths that do NOT all wrap it in an outer truncate, e.g. the defunc HOF body). Returns None
    /// (the caller walls) for an operand shape we cannot materialize, or a type whose eq is unhandled.
    fn lower_heap_eq_cond(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let ty = &left.ty;
        let eq = match self.lower_heap_eq_typed_materialized(left, right, ty) {
            Some(eq) => eq,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        if !negate {
            return Some(eq);
        }
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::Sub, a: one, b: eq });
        Some(dst)
    }
}
