impl LowerCtx {
    /// Try to EXECUTE `<materialized Option> ?? <scalar fallback>` to a SCALAR value: read
    /// the tag (len) and yield the payload (`data[0]`) when Some, else the fallback. Gated
    /// to a DIRECT self-host Option call — every such fn returns `Option[Int]`, so the
    /// payload is a scalar (no element alias), and its result is a real materialized Option
    /// dropped at scope end. Returns the scalar `dst`, or `None` (rolled back) when not in
    /// this subset (a non-call expr, or a heap fallback) — the caller defers to `Opaque`.
    ///
    /// SOUND: the Option's `Alloc` (the now-MATERIALIZED call, no longer elided) is `i`,
    /// dropped at scope end `d` = balanced; the tag/payload reads are scalar prims, the
    /// markers no-op, the payload is an i64 value COPY (not an alias), so dropping the
    /// Option after is safe. The call becoming real only improves caps (analyzed, not
    /// elided) and stays 1:1 with its IR call-node (no mir>ir issue).
    /// `track_result` governs the HEAP-String result only: `true` (a let-bind / call-arg temp)
    /// pushes the fresh owned String to `live_heap_handles` so it is dropped at scope end; `false`
    /// (a RETURN/tail position) leaves it untracked because it is MOVED OUT to the caller (tracking
    /// it would double-free). The scalar path is unaffected (a scalar result owns nothing).
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // The Option operand's handle: either a VAR already bound to a materialized Option
        // (`let o = list.get(xs, i); o ?? d` — the most common form, BORROWED, dropped by its
        // own let-bind at scope end), a function PARAM of Option type (`fn f(o: Option[String]) =
        // o ?? d` — passed by the caller as a real materialized Option block, BORROWED, not dropped
        // here), or a DIRECT self-host Option call (materialized here). The param case is sound by
        // the same evidence as `materialized_options`: an Option-typed param is a real 0-or-1-
        // element block (the calling convention), so its len-as-tag read is correct — NOT a deferred
        // Opaque (those are never params), which is why the bare-Var gate excludes non-Option Vars.
        //
        // A `??` operand is EITHER an Option (`o ?? d` → Some-payload / fallback) OR a scalar Result
        // (`int.parse(s) ?? -1` → Ok-payload / fallback). They share the len-as-tag layout but read
        // INVERSELY: Option Some = `tag != 0` (take payload), Result Ok = `tag == 0` (take payload).
        // `is_result` selects the arm arrangement below; a Result operand also skips the Option-only
        // `option.unwrap_or_str` String branch (a `Result[String,String] ?? "d"` is a later case).
        let is_named_variant_call = matches!(
            &expr.kind,
            IrExprKind::Call { target: CallTarget::Named { .. }, .. }
        ) && is_variant_ty(&expr.ty);
        let is_result = match &expr.kind {
            IrExprKind::Var { id } => self
                .value_for(*id)
                .ok()
                .map(|v| {
                    self.materialized_results.contains(&v)
                        && !self.materialized_options.contains(&v)
                })
                .unwrap_or(false),
            // A USER function returning Result — read its tag INVERSELY (Ok = tag 0).
            _ if is_named_variant_call => is_result_ty(&expr.ty),
            _ => is_self_host_result_call(expr),
        };
        let handle = if let IrExprKind::Var { id } = &expr.kind {
            match self.value_for(*id) {
                // A bare-Var operand must be a tracked materialized Option/Result OR a borrowed
                // variant PARAM (`param_values` — same calling-convention soundness as the match):
                // a deferred Opaque Var (len 0) would MISREAD as None/Err, so it is excluded.
                Ok(v)
                    if self.materialized_options.contains(&v)
                        || self.materialized_results.contains(&v)
                        // a Value/List-Ok Result Var (`value.get`/`value.as_array` result) — its `??`
                        // routes to the value_unwrap helper below. A String-Ok Result (heap_elem_lists)
                        // is NOT admitted here: it keeps its original path (the String branch is for
                        // OPTION[String], and counting a str-Result there falsely taints mir>ir).
                        || self.value_result_results.contains(&v)
                        || self.value_result_lists.contains(&v)
                        || self.param_values.contains(&v) =>
                {
                    v
                }
                _ => return None,
            }
        } else if is_self_host_option_call(expr)
            || is_self_host_result_call(expr)
            || (is_self_host_result_str_call(expr)
                && (crate::lower::is_value_result_ty(&expr.ty)
                    || crate::lower::is_result_listval_ty(&expr.ty)
                    || crate::lower::is_result_str_str_ty(&expr.ty)))
            || is_named_variant_call
        {
            // A self-host OR user-function call returning Option/Result — materialize it (the
            // Named-call arm seeds the READ-shape into `materialized_options/results`, so the
            // tag read below is over a KNOWN-layout block) and read its tag, exactly like a
            // tracked Var. The owned result is dropped at scope end by `materialized_call_arg`.
            match self.lower_call_args(std::slice::from_ref(expr)) {
                Ok(args) => match args.into_iter().next() {
                    Some(CallArg::Handle(v)) => v,
                    _ => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                },
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        } else {
            return None;
        };
        // HEAP-Value Result `??` (`value.get(o,k) ?? value.null()` — Result[Value,String]): route to
        // the self-hosted `result.value_unwrap_or`, which reuses the Value-Ok match read (its
        // `ok(v) => v` arm Dup's the @12 payload; the Result is freed by its scope-end DropResultValue).
        // A call returning a FRESH owned Value sidesteps the bind-position heap-result rc bookkeeping,
        // exactly like `option.unwrap_or_str` for the String payload.
        // The Ok payload selects the helper: a single Value (`value.get`) → `result.value_unwrap_or`;
        // a `List[Value]` (`value.as_array`) → `result.list_value_unwrap_or` (recursive list drop).
        // Both reuse the working heap-Ok match read; a call returning a fresh owned heap value
        // sidesteps the bind-position rc bookkeeping, like `option.unwrap_or_str` for the String case.
        let value_unwrap_helper = if crate::lower::is_result_listval_ty(&expr.ty) {
            Some("result.list_value_unwrap_or")
        } else if crate::lower::is_value_result_ty(&expr.ty) {
            Some("result.value_unwrap_or")
        } else if crate::lower::is_result_str_str_ty(&expr.ty) {
            Some("result.str_unwrap_or")
        } else if crate::lower::is_option_value_ty(&expr.ty) {
            Some("option.value_unwrap_or")
        } else if crate::lower::is_option_liststr_ty(&expr.ty) {
            Some("option.liststr_unwrap_or")
        } else if crate::lower::is_option_listvalue_ty(&expr.ty) {
            Some("option.listvalue_unwrap_or")
        } else {
            None
        };
        if let Some(helper) = value_unwrap_helper {
            if is_heap_ty(&fallback.ty) {
                let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
                    Ok(a) => a,
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                };
                let repr = match repr_of(&expr.ty) {
                    Ok(r) => r,
                    Err(_) => {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                };
                let mut call_args = vec![CallArg::Handle(handle)];
                call_args.extend(fb_args);
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: helper.to_string(),
                    args: call_args,
                    result: Some(repr),
                });
                if track_result {
                    self.live_heap_handles.push(dst);
                }
                return Some(dst);
            }
        }
        // HEAP-String result (`Option[String] ?? "default"` — the most common heap `??`): the scalar
        // unwrap below can't carry a heap payload (it would mis-read the slot-0 String HANDLE as an
        // i64 scalar). Route to the self-host `option.unwrap_or_str` CALL — a call returning a FRESH
        // owned String (cert `i`, bound + dropped like any heap value), which sidesteps the
        // bind-position heap-result-`if` cert problem entirely. The Option is BORROWED (the callee
        // reads + copies it); the fallback is materialized/borrowed by `lower_call_args`. Gated to
        // `Ty::String` (a `List`/other-heap payload would corrupt — its slot is not a String handle),
        // and `count_ir_calls` counts a String-fallback `UnwrapOr` node so this synthetic call keeps
        // `mir_calls <= ir_calls` (the same accounting as the `__str_concat` operator-desugar).
        if matches!(fallback.ty, Ty::String) && !is_result {
            let fb_args = match self.lower_call_args(std::slice::from_ref(fallback)) {
                Ok(a) => a,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let repr = match repr_of(&fallback.ty) {
                Ok(r) => r,
                Err(_) => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            let mut call_args = vec![CallArg::Handle(handle)];
            call_args.extend(fb_args);
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: "option.unwrap_or_str".to_string(),
                args: call_args,
                result: Some(repr),
            });
            if track_result {
                self.live_heap_handles.push(dst);
            }
            return Some(dst);
        }
        // A SCALAR `??`: read the tag (len @4) and pick the slot-0 payload vs the fallback. The
        // payload is an i64 value COPY (`Load width 8`) — fine for a scalar Ok/Some; a heap payload
        // is handled by the String branch above (Option) or stays out of subset (Result[String,…]).
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![handle] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let result = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
        // `IfThen` runs the THEN arm when `tag != 0`. For an OPTION that is Some (take the slot-0
        // payload); for a RESULT that is Err (take the FALLBACK — Ok is `tag == 0`, the ELSE arm).
        // So the two arms are SWAPPED between the cases. The ops emitted between IfThen/Else land in
        // the THEN body, those between Else/EndIf in the ELSE body — so the payload Load and the
        // fallback computation must each sit in the arm that USES them.
        if is_result {
            // THEN = Err (tag != 0) → the fallback computed HERE; ELSE = Ok → the slot-0 payload.
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::Else { val: Some(fb) });
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::EndIf { val: Some(payload) });
        } else {
            // THEN = Some (tag != 0) → the slot-0 payload loaded HERE; ELSE = None → the fallback.
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.ops.push(Op::Else { val: Some(payload) });
            let fb = match self.lower_scalar_value(fallback) {
                Some(v) => v,
                None => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            };
            self.ops.push(Op::EndIf { val: Some(fb) });
        }
        Some(result)
    }

    /// Emit `base + offset` then a `prim` load of `kind` at that address, returning the
    /// loaded value (an i64 in the prim floor's uniform model). The address arithmetic
    /// mirrors what `prim.handle(x) + offset` lowers to (`Op::ConstInt` + `Op::IntBinOp`).
    pub(crate) fn load_at_offset(&mut self, base: ValueId, offset: i64, kind: crate::PrimKind) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![addr] });
        dst
    }

    /// Lower ONE scalar `if` arm (a block's statements + a scalar tail value) with a
    /// per-arm scope frame: the heap temps the arm allocates are dropped WITHIN the arm
    /// (so taken-arm-only execution stays balanced). Returns the tail's scalar value.
    pub(crate) fn lower_scalar_arm(&mut self, arm: &IrExpr) -> Option<ValueId> {
        let (stmts, tail): (&[IrStmt], Option<&IrExpr>) = match &arm.kind {
            IrExprKind::Block { stmts, expr } => (stmts, expr.as_deref()),
            _ => (&[], Some(arm)),
        };
        let lhh_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in stmts {
            if self.lower_stmt(stmt).is_err() {
                self.in_frame -= 1;
                return None;
            }
        }
        // A nested `if`/`match` tail (an else-if chain — what a desugared `match`
        // produces) EXECUTES recursively via the scalar-if machinery; otherwise the
        // tail is a scalar value or a scalar call.
        let val = tail.and_then(|t| match &t.kind {
            IrExprKind::If { cond, then, else_ } => {
                self.try_lower_scalar_if(cond, then, else_, &t.ty)
            }
            // A nested VARIANT (Option/Result) match (`err(_) => match float.parse(s) { … }` —
            // the is_numeric_or_bool / looks_numeric chain) EXECUTES via the same tag-read
            // value-match the tail uses; its own arms recurse through `lower_scalar_arm`, so an
            // N-deep nest lowers. A LITERAL-pattern match desugars to the if-chain as before.
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                self.try_lower_variant_value_match(subject, arms, &t.ty)
            }
            IrExprKind::Match { subject, arms } => self
                .desugar_match_to_if(subject, arms, &t.ty)
                .and_then(|if_expr| match &if_expr.kind {
                    IrExprKind::If { cond, then, else_ } => {
                        self.try_lower_scalar_if(cond, then, else_, &t.ty)
                    }
                    _ => None,
                }),
            _ => self.lower_scalar_value(t).or_else(|| self.try_lower_scalar_call(t, &t.ty)),
        });
        self.in_frame -= 1;
        self.drop_arm_locals(lhh_mark);
        val
    }

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
        // The iterable is evaluated ONCE before the loop. A heap iterable goes through
        // `lower_call_args` — an already-tracked `Var` is borrowed (no new ownership),
        // a fresh heap value is materialized into an owned temp dropped at the OUTER
        // scope (its caps captured by the lowering). A scalar iterable (a `Range`)
        // carries no ownership; capture any call in it for caps.
        let container: Option<ValueId> = if is_heap_ty(&iterable.ty) {
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
            let elem_heap = find_var_ty(body, v).map(|t| is_heap_ty(&t)).unwrap_or(false);
            if elem_heap {
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
    pub(crate) fn try_lower_scalar_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> bool {
        if !matches!(cond.ty, Ty::Int | Ty::Bool) || body_breaks_or_continues(body) {
            return false;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

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
            if self.lower_stmt(stmt).is_err() {
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
        None
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_heap_result_cond(cond)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
        let then_obj = self.lower_heap_result_arm(then, result_ty)?;
        self.ops.push(Op::Else { val: Some(then_obj) });
        let else_obj = self.lower_heap_result_arm(else_, result_ty)?;
        self.ops.push(Op::EndIf { val: Some(else_obj) });
        Some(dst)
    }
}
