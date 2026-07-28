impl LowerCtx {

    /// The heap-result `Match` arm (BOTH the variant-subject-guarded fast path and its
    /// generic fallback — co-located, see the router doc comment), `Block`, the `Call`
    /// arms (Named/Module, plus BOTH `Computed`-callee arms for the same co-location
    /// reason), the borrowed-field `Member`/`TupleIndex` projections, and the trailing
    /// generic scalar-Ok catch-all. Verbatim subset of the original single match.
    /// Split again (codopsy r2, #852): every arm BODY moved wholesale into the named
    /// `..._arm` decider it calls, and each compound guard into its named predicate —
    /// this match is now a pure ROUTER. Every pattern and every guard is verbatim and
    /// in the ORIGINAL order (match arms commit on the first pattern+guard hit, so
    /// keeping every arm that can OVERLAP in this ONE match preserves the exact
    /// fallthrough semantics — in particular the co-located Match / Computed-call
    /// pairs). The trailing `Member`/`TupleIndex`/scalar-Ok group moved as a pure
    /// SUFFIX delegate ([`Self::lower_borrowed_projection_or_scalar_ok_arm`], the
    /// `_` arm): a suffix split cannot change which arm commits.
    fn lower_heap_result_arm_ctrl(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::Match { subject, arms }
                if self.is_variant_shaped_match_subject(&subject.ty) =>
            {
                self.lower_variant_subject_match_arm(subject, arms, result_ty)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.lower_nested_heap_result_if_arm(cond, then, else_, result_ty)
            }
            IrExprKind::List { elements }
                if Self::is_heap_elem_list_literal_result(result_ty) =>
            {
                self.lower_heap_elem_list_literal_arm(arm, elements)
            }
            IrExprKind::Block { stmts, expr } => {
                self.lower_block_stmts_then_tail_arm(stmts, expr, result_ty)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                self.lower_named_call_or_variant_ctor_arm(arm, name, args, result_ty)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                self.lower_pure_module_call_arm(module, func, args, result_ty)
            }
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if is_heap_ty(&arm.ty) && self.closure_value_of(callee).is_some() =>
            {
                self.lower_known_funcref_call_arm(callee, args, result_ty)
            }
            IrExprKind::Match { subject, arms } => {
                self.lower_desugarable_match_arm(subject, arms, result_ty)
            }
            IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. }
                if is_heap_ty(&arm.ty) =>
            {
                self.lower_inline_lambda_call_arm(callee, args, result_ty)
            }
            _ => self.lower_borrowed_projection_or_scalar_ok_arm(arm, result_ty),
        }
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the router's
    /// trailing arm group — the borrowed-container `Member`/`TupleIndex` projections and the
    /// generic scalar-Ok catch-all, verbatim and in the original order. A pure SUFFIX split
    /// of the router match: every arm kind not claimed by an earlier router arm lands here,
    /// exactly as it landed on these trailing arms in the single match — a suffix delegate
    /// cannot change which arm commits.
    fn lower_borrowed_projection_or_scalar_ok_arm(
        &mut self,
        arm: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        match &arm.kind {
            IrExprKind::Member { object, field }
                if self.is_projectable_borrowed_container(object) =>
            {
                self.lower_borrowed_field_projection_arm(object, field)
            }
            IrExprKind::TupleIndex { object, index }
                if self.is_projectable_borrowed_container(object) =>
            {
                self.lower_borrowed_tuple_projection_arm(object, *index)
            }
            _ if Self::is_scalar_ok_payload_of_result(&arm.ty, result_ty) => {
                self.lower_scalar_result_ok_wrap_arm(arm, result_ty)
            }
            _ => None,
        }
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// fast-path `Match` arm's guard — the subject is an Option/Result variant or a
    /// CUSTOM variant type. Verbatim.
    fn is_variant_shaped_match_subject(&self, ty: &Ty) -> bool {
        crate::lower::is_variant_ty(ty) || self.custom_variant_type_name(ty).is_some()
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// variant-subject heap-result `Match` arm body, verbatim.
    ///
    /// A heap-result MATCH arm — the monadic `!`-desugar inside a tail-duplicated
    /// `if` (`let xs = if c then load(p)! else []; ok(xs + t)` becomes
    /// `if c then { match load(p) { err(e)=>err(e), ok(xs)=>ok(xs+t) } } else …`,
    /// porta resolve_env/serve/validate). Delegate to the SAME variant value-match
    /// machinery the fn-tail position already uses (rollback-safe: a shape outside
    /// its subset returns None and the caller keeps the wall — never invalid wasm).
    fn lower_variant_subject_match_arm(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        // Option/Result subjects via the value match; a CUSTOM-variant subject (the
        // regrouped `err($q)` INNER match over a borrowed variant payload — the
        // `compute` class) via the tag@slot0 dispatcher, which accepts a heap result
        // over a BORROWED subject (the recursive-to_string precedent).
        let obj = match self.try_lower_variant_value_match(subject, arms, result_ty) {
            Some(v) => v,
            // An `Option[<heap>]` inner subject (the fold-step nested match over
            // `list.last(stack)`) — the merge-based Option twin, then the custom
            // tag@slot0 dispatcher.
            _ => match self.try_lower_option_match_value(subject, arms, result_ty) {
                Some(v) => v,
                _ => self.try_lower_custom_variant_match(subject, arms, result_ty)?,
            },
        };
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// nested heap-result `If` arm body, verbatim.
    ///
    /// A NESTED heap-result `if` arm: the branch desugar stacks one
    /// hoisted `if`-bind per level, so a two-`if` element chain
    /// arrives as `if c { … tail: if c' {…} else {…} } else …` (the
    /// ceangal todo_item). Recurse through the same if-lowering —
    /// level-by-level release parity is exactly the invariant
    /// `lower_heap_result_if_inner` maintains — and move the merged
    /// result out (`Consume`, the same `im` balance the Call arm
    /// carries).
    fn lower_nested_heap_result_if_arm(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let dst = self.try_lower_heap_result_if(cond, then, else_, result_ty)?;
        self.ops.push(crate::Op::Consume { v: dst });
        Some(dst)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// `List`-literal arm's guard — the result type is a `List` of exactly one HEAP
    /// element type. Verbatim.
    fn is_heap_elem_list_literal_result(result_ty: &Ty) -> bool {
        matches!(result_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && crate::lower::is_heap_ty(&a[0]))
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// heap-element `List`-literal arm body, verbatim.
    ///
    /// A `List` LITERAL arm over HEAP elements (`if n <= 0 then []
    /// else [leaf(n)]` — ceangal's kids_of, the #875 class in arm
    /// position): the BIND-position record-list builder already
    /// materializes the non-empty form as ONE owned block with its
    /// registered recursive drop — route the arm through it and move
    /// the block out (`Consume`, the Call-arm "im" balance). The EMPTY
    /// arm is the zero-length block of the same layout; every list
    /// drop at len 0 is exactly the block free, so the moved-out
    /// object is uniform across arms.
    fn lower_heap_elem_list_literal_arm(
        &mut self,
        arm: &IrExpr,
        elements: &[IrExpr],
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = if elements.is_empty() {
            let len = self.fresh_value();
            self.ops.push(crate::Op::ConstInt { dst: len, value: 0 });
            let dst = self.fresh_value();
            self.ops.push(crate::Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynList { len },
            });
            dst
        } else {
            self.try_lower_record_list_literal(arm)?
        };
        self.ops.push(crate::Op::Consume { v: obj });
        self.live_heap_handles.retain(|x| *x != obj);
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// `Block` arm body, verbatim. (Its doc paragraph below had sat above the Match
    /// arm since an earlier arm reorder — carried back to the code it documents.)
    ///
    /// A BLOCK arm (`else { let c = string.get(s, pos) ?? ""; <heap-tail> }` — the
    /// dominant real-parser shape): lower its statements as effects in a per-arm frame,
    /// then its tail as the arm's moved-out heap value (recursing into this same arm
    /// lowering, which `Consume`s the tail). The block's own heap let-locals (tracked in
    /// `live_heap_handles` since `arm_mark`) are freed WITHIN the arm via
    /// `drop_arm_locals`; the moved-out value is `Consume`d (never in that set), so it is
    /// not double-freed. Same per-arm balance the scalar block arm proves.
    fn lower_block_stmts_then_tail_arm(
        &mut self,
        stmts: &[IrStmt],
        expr: &Option<Box<IrExpr>>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let tail = expr.as_deref()?;
        let arm_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        let mut ok = true;
        for stmt in stmts {
            if let Err(e) = self.lower_stmt(stmt) {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!("[heap-if-arm] Block stmt declined: {e:?}")
                });
                ok = false;
                break;
            }
        }
        let obj = if ok {
            self.lower_heap_result_arm(tail, result_ty)
        } else {
            None
        };
        self.drop_arm_locals(arm_mark);
        self.in_frame -= 1;
        obj
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// Named-`Call` arm body — the variant-ctor-vs-real-call decider — verbatim.
    ///
    /// A direct user-call arm (`if c then f(x) else "d"`): the callee returns a
    /// FRESH owned heap value (CallFn-with-heap-result = cert `i`), moved out by the
    /// arm's `Consume` (cert `m`) — the same `"im"` balance as a literal arm. Any
    /// heap arg the call MATERIALIZES (a heap-literal/fresh-value arg) is dropped
    /// WITHIN the arm (`drop_arm_locals`), NOT at function scope: a per-arm temp
    /// freed at function scope would `Drop` an uninitialized local when the OTHER arm
    /// ran (garbage rc_dec → trap). Per-arm, the temp is freed only if this arm
    /// executes — the same per-iteration-balance discipline the loops use.
    fn lower_named_call_or_variant_ctor_arm(
        &mut self,
        arm: &IrExpr,
        name: &almide_lang::intern::Sym,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        // A DIRECT SELF-RECURSIVE call arm (`name == fn_name`) is the unbounded-
        // stack tail-recursion shape (`fn spin = if … then acc else spin(…)`).
        // v1 has NO TCO, so EXECUTING it deeply overflows the wasm call stack
        // (a fail-stop trap). Executing the heap-result if here would convert a
        // shallow-correct / deep-trapping recursion — a NET LOSS over the sound
        // Opaque fallback for the canonical 2M-deep TCO acceptance fixture. WALL
        // it (→ `None`): the function keeps its memory-safe linearized form until
        // real TCO lands. (A non-self call recurses no deeper than the caller, so
        // it stays admitted.)
        // EXCEPTION: inside a defunctionalized `list.map` body (`children |> list.map((c) =>
        // render_el(c, …))`) the self-call is BOUNDED — it recurses to the tree's DEPTH, not
        // the unbounded linear depth of a tail loop — so executing it is correct (matches v0's
        // own recursion) and is admitted. The wall applies only to a function-TAIL self-call.
        // EXPERIMENT (toml): allow a function-tail self-call to lower as a REAL recursive
        // CallFn (matches v0's own native recursion exactly — same call-stack depth, same
        // bytes). The previous unconditional wall kept a sound Opaque/linearized fallback to
        // avoid a 2M-deep tail-loop wasm stack overflow; but a TCO-able tail loop is already
        // rewritten by try_tco_rewrite BEFORE here (never reaches this arm), so what remains
        // is a general-arg recursion (toml parse_doc/set_nested/append_aot) whose depth is
        // bounded by the input exactly as v0's is. Gated by the full test (the 2M-deep TCO
        // acceptance fixture is TCO'd, not executed here — if it regresses, this is reverted).
        let _ = name;
        // A VARIANT-CTOR arm (`else Para(line)` / `then Blank` — the parse_line
        // if-chain): the "call" is a CONSTRUCTOR, not a function — emitting a
        // `CallFn $Para` dangles (caught at render as unlinked, walling the whole
        // file). Build the tagged block (`try_lower_variant_ctor`, the binds_p2
        // guard's exact twin) and MOVE it out — the same per-arm `"im"` balance;
        // field temps the ctor materializes are moved into the block, and any
        // stray arm temp is freed by `drop_arm_locals`.
        if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) {
            let arm_mark = self.live_heap_handles.len();
            let obj = self.try_lower_variant_ctor(arm)?;
            self.live_heap_handles.retain(|x| *x != obj);
            self.ops.push(Op::Consume { v: obj });
            self.drop_arm_locals(arm_mark);
            return Some(obj);
        }
        let repr = repr_of(result_ty).ok()?;
        let arm_mark = self.live_heap_handles.len();
        let lowered = self.lower_call_args(args).ok()?;
        let obj = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(obj),
            name: name.as_str().to_string(),
            args: lowered,
            result: Some(repr),
        });
        self.ops.push(Op::Consume { v: obj });
        // Free materialized arg temps inside the arm (obj is moved out, never in
        // `live_heap_handles`, so it is not among them).
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// pure stdlib `Module`-call arm body, verbatim.
    ///
    /// A PURE stdlib `Module`-call arm (`match n { 0 => "a", _ => int.to_string(n) }` —
    /// the single most common real-program shape). Same per-arm `"im"` balance as the
    /// Named-call arm: the pure call returns a FRESH owned heap value (`i`), the arm's
    /// `Consume` moves it out (`m`); any heap arg it materializes is freed within the arm
    /// (`drop_arm_locals`). The purity gate lives in `lower_pure_module_value_call` (an
    /// impure/HO/unsupported call errors → `None` → the caller keeps the sound Opaque
    /// fallback). Was the gap that dropped a real-program `match → stdlib-call` to Opaque.
    fn lower_pure_module_call_arm(
        &mut self,
        module: &almide_lang::intern::Sym,
        func: &almide_lang::intern::Sym,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self
            .lower_pure_module_value_call(module.as_str(), func.as_str(), args, result_ty)
            .ok()?;
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// known-funcref `Computed`-call arm body, verbatim.
    ///
    /// A heap-result call through a KNOWN funcref arm (`Leaf(v) => leaf(v)`,
    /// `Node(l, r) => merge(…)` — tree_fold's arms call fn-typed PARAMS): execute
    /// via the closure-table call, the tail-position machinery ported per-arm
    /// (tail.rs's Computed case). Same per-arm `"im"` balance as the Named-call
    /// arm: the indirect call returns a FRESH owned heap value (`i`), the arm's
    /// `Consume` moves it out (`m`); arg temps free within the arm. An UNKNOWN
    /// callee falls through to the C1 direct-lambda inline case below.
    fn lower_known_funcref_call_arm(
        &mut self,
        callee: &IrExpr,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let blk = self.closure_value_of(callee)?;
        let lowered = self.lower_call_args(args).ok()?;
        let obj = self.fresh_value();
        let repr = repr_of(result_ty).ok()?;
        self.emit_closure_call(blk, Some(obj), lowered, Some(repr));
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// generic (fallback) `Match` arm body, verbatim.
    ///
    /// A NESTED `match` arm (`match int.parse(c) { ok(n) => value.int(n), err(_) =>
    /// match float.parse(c) { … } }` — try_decimal; `if … then match int.from_hex(..) {
    /// ok(n) => value.int(n), err(_) => value.str(raw) } else …` — parse_number's then-arm).
    /// Recurse through the SAME machinery the tail-position `match` uses: a variant subject
    /// runs the proven `try_lower_variant_value_match` (subject-drop-before-arms over a
    /// scalar payload, then a heap-result-`if` skeleton), an Int-literal subject desugars
    /// to a nested heap-result `if`. The recursive call ALREADY `Consume`s each leaf arm
    /// (the move-out balance) and returns the merged if-result `dst` — so this arm adds NO
    /// extra `Consume` (exactly like the nested-`If` arm above), avoiding a double-move-out.
    /// Cert-clean: it composes two already-proven, internally-balanced lowerings; on any
    /// out-of-subset shape the inner attempt rolls itself back and returns `None`, so the
    /// OUTER `try_lower_heap_result_if` restores the op stream and walls the function.
    fn lower_desugarable_match_arm(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        // PER-ARM FRAME: the match SUBJECT (`int.from_hex(string.drop(c, 2))`) materializes
        // heap-arg temps (the `string.drop` result) into `live_heap_handles`. Unlike every
        // other arm kind here, the match lowering does not move them out — they must be freed
        // WITHIN this arm (inside the wasm then/else branch), else they leak to the FUNCTION
        // scope-end where an UNCONDITIONAL `rc_dec` of an uninitialized local (when the OTHER
        // arm ran) is a `rc_dec(0)` trap — the yaml `parse_number` 0x-branch crash. The
        // recursive lowering Consumes the moved-out result (never in the set), so drop_arm_locals
        // frees exactly the subject-eval temps.
        let arm_mark = self.live_heap_handles.len();
        if is_variant_ty(&subject.ty) {
            if let Some(dst) =
                self.try_lower_variant_value_match(subject, arms, result_ty)
            {
                self.drop_arm_locals(arm_mark);
                return Some(dst);
            }
        }
        // A nested LIST `[] / catch-all` match arm (the tuple-of-lists classify
        // shape after desugar_tuple_empty_list_match): the same merge-based
        // machinery the tail uses — its EndIf merge moves the value out (no
        // extra Consume, like the recursive Match case below); the Dup'd
        // subject temp frees within the arm (`drop_arm_locals`).
        if let Some(dst) = self.try_lower_list_match_value(subject, arms, result_ty) {
            self.drop_arm_locals(arm_mark);
            return Some(dst);
        }
        if let Some(if_expr) = self.desugar_match_to_if(subject, arms, result_ty) {
            if let IrExprKind::If { cond, then, else_ } = &if_expr.kind {
                if let Some(dst) =
                    self.lower_heap_result_if_inner(cond, then, else_, result_ty)
                {
                    self.drop_arm_locals(arm_mark);
                    return Some(dst);
                }
            }
        }
        None
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// direct-lambda-inline `Computed`-call arm body, verbatim.
    ///
    /// A heap-result `Computed`-callee call arm (`xs |> list.map((p) => param_ty(p))` — the
    /// bindgen inner-map cell calls a let-bound INLINE lambda returning String). C1 HEAP
    /// DIRECT-CALL INLINE: defunctionalize it to its inlined body — a FRESH OWNED heap value,
    /// moved out by this arm's `Consume` (cert `m`), the same per-arm `"im"` balance as the
    /// Named-call arm. The inline tracks its result in `live_heap_handles`; detach it (it is
    /// moved out, not a scope-end local) before `Consume`, then `drop_arm_locals` frees any
    /// arg/body temp the inline left. A non-let-lambda callee rolls back (`None`) → the caller
    /// keeps its sound Opaque/wall (no invalid wasm).
    fn lower_inline_lambda_call_arm(
        &mut self,
        callee: &IrExpr,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let arm_mark = self.live_heap_handles.len();
        let obj = self.try_inline_direct_lambda_call_heap(callee, args, result_ty)?;
        // The inlined result is moved out of this arm (Consume), so detach it from the live
        // set; `drop_arm_locals` then frees only the inline's transient temps.
        self.live_heap_handles.retain(|h| *h != obj);
        self.ops.push(Op::Consume { v: obj });
        self.drop_arm_locals(arm_mark);
        Some(obj)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// shared guard of the `Member`/`TupleIndex` projection arms — decides whether the
    /// container admits the borrow-then-`Dup` field projection. Verbatim.
    ///
    /// SCOPED to a BORROWED-PARAM container (`is_borrowed_param_container` — `opts` is a record
    /// param the CALLER owns): this is the RETURN-materializer brick for projecting a borrowed
    /// param's heap field. A LOCAL container (`else result.out` over a `list.fold` result, the
    /// playground `wrap_lists`) is the LOOP-CARRIED-accumulator frontier (the `(B)` mechanism) —
    /// admitting it makes the enclosing fold body lower, whose defunctionalized elided-call
    /// count then outruns the source count-gate (a caps WALL BREACH). Defer the local-container
    /// case (`None`) so it keeps its existing wall — the loop-slot work owns it. The param case
    /// is exactly the documented borrow-then-`Dup` `dup_borrowed_slot` is built for.
    /// (B)-mechanism widening: a MATERIALIZED LOCAL container (`else result.out` over a
    /// record-literal/self-host-fold bind — the playground `wrap_lists`) is now ALSO
    /// admitted via `is_materialized_local_container` — the old defunc elided-call-count
    /// objection applied to the DEFUNC fold path; a self-host fold is a REAL CallFn, so
    /// the caps count is unaffected (re-verified by the corpus caps gate on this change).
    fn is_projectable_borrowed_container(&self, object: &IrExpr) -> bool {
        self.is_borrowed_param_container(object) || self.is_materialized_local_container(object)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// `Member` projection arm body, verbatim.
    ///
    /// A bare heap-FIELD-projection arm (`preopen_dirs: if … then opts.preopen_dirs else […]`
    /// — the porta build_config spread-override If's then-arm is `opts.preopen_dirs`, a
    /// `Member`). The arm must MOVE OUT an owned reference, but the field is still owned by its
    /// container (`opts`, a borrowed param the caller owns). BORROW the slot handle
    /// (`LoadHandle` of `container_handle + offset`) and ACQUIRE a fresh owned reference
    /// (`dup_borrowed_slot` = `Op::Dup`, cert `a`-grade), then MOVE it out (`Op::Consume` =
    /// cert `m`) — the SAME per-arm `"am"` balance as the bare-Var arm, with the ORIGINAL
    /// slot untouched (no double-free: the Dup'd ref is independent; the container drops its
    /// own ref once at its scope end). A `TupleIndex` projection is identical.
    /// `dup_borrowed_slot` tracks the owned ref in `live_heap_handles`; the `retain` detaches
    /// it (it is moved out, NOT a scope-end local) before the per-arm teardown. Defers (`None`)
    /// for an unresolvable container / non-heap slot — the caller keeps its sound wall.
    fn lower_borrowed_field_projection_arm(
        &mut self,
        object: &IrExpr,
        field: &almide_lang::intern::Sym,
    ) -> Option<ValueId> {
        let offset = self.aggregate_field_offset_any(&object.ty, field.as_str())?;
        let arm_mark = self.live_heap_handles.len();
        let h = self.resolve_aggregate_container_handle(object)?;
        let owned = self.dup_borrowed_slot(h, offset);
        self.ops.push(Op::Consume { v: owned });
        self.live_heap_handles.retain(|x| *x != owned);
        self.drop_arm_locals(arm_mark);
        Some(owned)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// `TupleIndex` projection arm body — "A `TupleIndex` projection is identical"
    /// (the `Member` arm's doc above). Verbatim.
    fn lower_borrowed_tuple_projection_arm(
        &mut self,
        object: &IrExpr,
        index: usize,
    ) -> Option<ValueId> {
        let offset = self.aggregate_index_offset_any(&object.ty, index)?;
        let arm_mark = self.live_heap_handles.len();
        let h = self.resolve_aggregate_container_handle(object)?;
        let owned = self.dup_borrowed_slot(h, offset);
        self.ops.push(Op::Consume { v: owned });
        self.live_heap_handles.retain(|x| *x != owned);
        self.drop_arm_locals(arm_mark);
        Some(owned)
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// trailing catch-all's guard — the arm is a SCALAR whose type is exactly the
    /// `Result`'s Ok payload (String Err). Verbatim.
    fn is_scalar_ok_payload_of_result(arm_ty: &Ty, result_ty: &Ty) -> bool {
        !is_heap_ty(arm_ty)
            && matches!(result_ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                    if a.len() == 2 && a[0] == *arm_ty && matches!(a[1], Ty::String))
    }

    /// Extracted from [`Self::lower_heap_result_arm_ctrl`] (codopsy r2, #852): the
    /// trailing generic scalar-Ok catch-all body, verbatim.
    ///
    /// The GENERAL scalar arm of a Result-typed dispatch (`if n < 0 then fail(..)
    /// else 0` in an AUTO-WRAP/declared-Result fn — effect_tco's `checked` base
    /// case): any scalar-subset expression whose type is the Result's Ok payload
    /// wraps via the SAME `materialize_result_ok` the Var arm above uses. Bounded
    /// by `lower_scalar_value`'s own subset (a miss keeps the wall).
    fn lower_scalar_result_ok_wrap_arm(&mut self, arm: &IrExpr, result_ty: &Ty) -> Option<ValueId> {
        let payload = self.lower_scalar_value(arm)?;
        let repr = repr_of(result_ty).ok()?;
        let obj = self.materialize_result_ok(payload, repr);
        self.ops.push(Op::Consume { v: obj });
        Some(obj)
    }
}
