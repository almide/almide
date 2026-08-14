/// A full snapshot of the ValueId-keyed TRACKING state a speculative lowering
/// attempt can pollute — the read-shape sets, the drop-route maps, the lift
/// list, the op stream. `LowerCtx::speculate` restores ALL of it when the
/// attempt declines, so "try a rewrite, fall back on decline" can never leave
/// a stale entry that alters a later path's decisions (the option_result_ctors
/// regression: a declined heap-`??`-as-match attempt left the SUBJECT's stable
/// ValueId in `materialized_options`, and the downstream #1270 bind rewrite
/// then took a different, walling branch). Ops/lhh/lifted are length-truncated;
/// the set/map state has no such watermark, so it is cloned wholesale — heavy,
/// but compile-time-only and bounded by per-function set sizes. This is the
/// #1100 rollback discipline generalized from the op stream to the WHOLE
/// decision state; #1414's per-value repr eventually deletes the need.
struct SpecSnapshot {
    ops_len: usize,
    lhh_len: usize,
    lifted_len: usize,
    value_shapes: std::collections::HashMap<ValueId, crate::lower::VariantShape>,
    value_drops: std::collections::HashMap<ValueId, crate::lower::DropFacts>,
    materialized_lists: std::collections::HashSet<ValueId>,
    value_handles: std::collections::HashSet<ValueId>,
    value_elem_lists: std::collections::HashSet<ValueId>,
    str_value_elem_lists: std::collections::HashSet<ValueId>,
    materialized_aggregates: std::collections::HashSet<ValueId>,
    closure_values: std::collections::HashSet<ValueId>,
    record_masks: std::collections::HashMap<ValueId, Vec<usize>>,
}

impl LowerCtx {
    /// Seed a value's VARIANT read shape AND drop route from its TYPE — law 4
    /// of the rot-eradication constitution (the type IS the per-value
    /// knowledge), the #1414 seed-once entry point: the ONE function every
    /// position (bind, statement-subject, `??` merge, ANF temp) consults, so
    /// the read-shape/drop refinements can never drift apart per position
    /// again (the bind path's missing `list_str_result_results` arm leaked
    /// element Strings for exactly that reason). The refinement set is the
    /// UNION of what the positions historically seeded, all type-keyed:
    ///
    /// - Option: len-as-tag read; a heap payload also opens the heap-bind gate
    ///   and routes the flat elementwise drop (`heap_elem_lists`).
    /// - Result (Scalar family): len-as-tag read; a HEAP Err payload also
    ///   opens the Err-bind gate + flat drop (the Camp-4 sub-case).
    /// - Result (HeapOk family): cap-as-tag read (@16) plus the payload-class
    ///   drop route — `List[Value]` recursive (`value_result_lists`), `Value`
    ///   recursive (`value_result_results`), `List[String]` recursive
    ///   (`list_str_result_results`), the fold-msi tag-aware routes, else the
    ///   flat `heap_elem_lists`.
    ///
    /// The #1414 endgame turns this into the shape attached at `fresh_value`
    /// time; until then this is the single insertion point.
    pub(crate) fn seed_variant_value_shape(&mut self, v: ValueId, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        match ty {
            Ty::Applied(TC::Option, a) if a.len() == 1 => {
                self.value_shapes.insert(v, crate::lower::VariantShape::Option);
                if is_heap_ty(&a[0]) {
                    self.value_drops.entry(v).or_default().flat_elems = true;
                }
            }
            Ty::Applied(TC::Result, a) => match crate::lower::result_family(ty) {
                crate::lower::ResultFamily::Scalar => {
                    self.value_shapes.insert(v, crate::lower::VariantShape::ResultScalar);
                    // Camp-4: scalar-Ok / HEAP-Err — the Err arm's payload bind
                    // gate + the flat drop (Ok=len0 frees nothing, Err=len1
                    // frees the slot-0 String).
                    if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                        self.value_drops.entry(v).or_default().flat_elems = true;
                    }
                }
                crate::lower::ResultFamily::HeapOk => {
                    self.value_shapes.insert(v, crate::lower::VariantShape::ResultHeapOk);
                    if crate::lower::is_result_listval_ty(ty) {
                        self.value_drops.entry(v).or_default().value_result_list = true;
                    } else if crate::lower::is_value_result_ty(ty) {
                        self.value_drops.entry(v).or_default().value_result = true;
                    } else if crate::lower::is_list_str_result_ty(ty) {
                        self.value_drops.entry(v).or_default().list_str_result = true;
                        self.value_drops.entry(v).or_default().flat_elems = true;
                    } else if crate::lower::is_res_map_si_ty(ty)
                        || crate::lower::is_res_list_map_si_ty(ty)
                    {
                        let route = if crate::lower::is_res_map_si_ty(ty) {
                            "res_msi"
                        } else {
                            "res_lmsi"
                        };
                        self.value_drops.entry(v).or_default().named_route = Some(route.to_string());
                        self.value_drops.entry(v).or_default().flat_elems = true;
                    } else {
                        self.value_drops.entry(v).or_default().flat_elems = true;
                    }
                }
            },
            _ => {}
        }
    }

    /// Run `f` speculatively: on `Some` keep everything it did; on `None`
    /// restore the complete tracking snapshot taken at entry. See
    /// [`SpecSnapshot`].
    fn speculate<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let snap = SpecSnapshot {
            ops_len: self.ops.len(),
            lhh_len: self.live_heap_handles.len(),
            lifted_len: self.lifted.len(),
            value_shapes: self.value_shapes.clone(),
            value_drops: self.value_drops.clone(),
            materialized_lists: self.materialized_lists.clone(),
            value_handles: self.value_handles.clone(),
            value_elem_lists: self.value_elem_lists.clone(),
            str_value_elem_lists: self.str_value_elem_lists.clone(),
            materialized_aggregates: self.materialized_aggregates.clone(),
            closure_values: self.closure_values.clone(),
            record_masks: self.record_masks.clone(),
        };
        let out = f(self);
        if out.is_none() {
            self.ops.truncate(snap.ops_len);
            self.live_heap_handles.truncate(snap.lhh_len);
            self.lifted.truncate(snap.lifted_len);
            self.value_shapes = snap.value_shapes;
            self.value_drops = snap.value_drops;
            self.materialized_lists = snap.materialized_lists;
            self.value_handles = snap.value_handles;
            self.value_elem_lists = snap.value_elem_lists;
            self.str_value_elem_lists = snap.str_value_elem_lists;
            self.materialized_aggregates = snap.materialized_aggregates;
            self.closure_values = snap.closure_values;
            self.record_masks = snap.record_masks;
        }
        out
    }
}

impl LowerCtx {
    pub(crate) fn try_lower_option_unwrap_or(
        &mut self,
        expr: &IrExpr,
        fallback: &IrExpr,
        track_result: bool,
    ) -> Option<ValueId> {
        // An `Option[record]` operand (`list.get(tools, i) ?? { name: "", … }`) has NO faithful
        // `??` lowering yet: the Value-shaped `option.value_unwrap_or` corrupts a record field
        // block (both arms printed garbage / empty fields vs v0), and no other path here handles
        // it. DECLINE outright so the whole `??` walls cleanly (never a wrong byte) — a correct
        // record-payload unwrap-or is a follow-up. Gated to a record/anon-record Option payload.
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &expr.ty
        {
            if a.len() == 1 && self.record_or_anon_drop_type_name(&a[0]).is_some() {
                return None;
            }
        }
        // #1418 SPECULATIVE ORDER INVERSION (scalar slice): try the MATCH form
        // FIRST — `e ?? d` as `match e { ok(p) => p, err(_) => d }` (Roc's
        // canonical desugar, character-identical) through the generic scalar
        // value-match machinery — and only fall back to the route chain below
        // when it declines (ops rolled back; the routes stay the safety net).
        // Every shape the match path takes migrates off the routes with no
        // behavior flag; the migration completes when the fallback count hits
        // zero and the routes are deleted. SCALAR payloads only in this slice:
        // the scalar routes emit no synthetic helper CallFn either, so the
        // caps-counter accounting (#1079) is unchanged whichever path fires.
        // A PROPAGATING fallback (`?? f(..)!`) stays on the route: the scalar
        // value-match operand path accepts an Unwrap arm but emits an invalid
        // early-return for it (i32 Result block where the merge expects i64 —
        // found by this very inversion, 2026-08-15); the route's
        // `lower_scalar_value` Unwrap arm is the proven lowering. Lift this
        // guard when the value-match path learns the propagation arm.
        let fallback_propagates =
            matches!(&fallback.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. });
        if !is_heap_ty(&fallback.ty) && !fallback_propagates {
            let ops_mark = self.ops.len();
            let lhh_mark = self.live_heap_handles.len();
            let synth = IrExpr {
                kind: IrExprKind::Unit,
                ty: fallback.ty.clone(),
                span: expr.span.clone(),
                def_id: None,
            };
            let rewritten = if expr.ty.is_result() {
                Some(Self::unwrap_or_as_result_match(&synth, expr, fallback))
            } else if expr.ty.is_option() {
                Some(Self::unwrap_or_as_option_match(&synth, expr, fallback))
            } else {
                None
            };
            if let Some(m) = rewritten {
                if let Some(v) = self.lower_scalar_match_operand(&m) {
                    crate::trace::trace("ALMIDE_DBG_QQ", || {
                        format!("[qq] match-first took {:?} ?? (scalar)", expr.ty)
                    });
                    return Some(v);
                }
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                // Subject-ANF retry (the scalar twin of the heap section's):
                // a CALL subject the scalar match cannot classify — a
                // FALLIBLE-CLOSURE carrier (`g("21") ?? -1`, ADR-0009), a
                // helper outside the name sets — binds fine through the
                // bind-position machinery (which owns the Computed-call
                // carrier arm), after which the match dispatches on a
                // tracked, type-seeded VAR. `speculate` rolls back a decline.
                let m2 = m.clone();
                let out = self.speculate(|ctx| {
                    let IrExprKind::Match { subject, .. } = &m2.kind else { unreachable!() };
                    if !matches!(subject.kind, IrExprKind::Call { .. }) {
                        return None;
                    }
                    let t = almide_ir::VarId(crate::lower::desugar_var_seed());
                    ctx.lower_bind(t, &subject.ty, subject).ok()?;
                    if let Ok(tval) = ctx.value_for(t) {
                        ctx.seed_variant_value_shape(tval, &subject.ty);
                    }
                    let tv = IrExpr {
                        kind: IrExprKind::Var { id: t },
                        ty: subject.ty.clone(),
                        span: subject.span.clone(),
                        def_id: None,
                    };
                    let IrExprKind::Match { arms, .. } = &m2.kind else { unreachable!() };
                    let m3 = IrExpr {
                        kind: IrExprKind::Match { subject: Box::new(tv), arms: arms.clone() },
                        ty: m2.ty.clone(),
                        span: m2.span.clone(),
                        def_id: None,
                    };
                    ctx.lower_scalar_match_operand(&m3)
                });
                crate::trace::trace("ALMIDE_DBG_QQ", || match out {
                    Some(_) => format!("[qq] match took {:?} ?? (scalar, anf)", expr.ty),
                    None => format!("[qq] declined {:?} ?? (scalar)", expr.ty),
                });
                if out.is_some() {
                    return out;
                }
            }
        }
        // THE DELETION (#1418, rot-eradication R1): the route chain is GONE.
        // Every heap-payload `??` rides the generic value-match machinery below
        // — the same lowering the corpus proved complete under the
        // ALMIDE_QQ_NO_ROUTES readiness probe (419/419 equal, 0 unexpected)
        // before this excision. A shape neither path takes DECLINES to the
        // honest wall; nothing blind-reads anything (the old scalar terminal's
        // len@4 misread class died with the chain).
        if !is_heap_ty(&fallback.ty) || fallback_propagates {
            return None;
        }
        // The CLOSURE payload class (`map.get(m, "add") ?? ((x) => x)` — the
        // closure-valued map coalesce): the route BODY moved here (rot-
        // eradication R1's completion architecture — payload classes are cases
        // inside the one path, not routes beside it). A generic variant match
        // cannot bind an `Option[<Fn>]` payload; this case borrows slot 0,
        // Dups the closure block, and lifts the fallback lambda into the else
        // arm — byte-identical to the retired `lower_closure_unwrap_or`.
        {
            use almide_lang::types::constructor::TypeConstructorId as TC;
            let is_opt_fn = matches!(&expr.ty,
                Ty::Applied(TC::Option, a) if a.len() == 1 && matches!(a[0], Ty::Fn { .. }));
            if is_opt_fn {
                if let IrExprKind::Lambda { params, body, .. } = &fallback.kind {
                    let (params, body) = (params.clone(), body.clone());
                    let out = self.speculate(|ctx| {
                        use crate::PrimKind;
                        let handle = match &expr.kind {
                            IrExprKind::Var { id } => ctx.value_for(*id).ok()?,
                            IrExprKind::Call { .. } => {
                                let t = almide_ir::VarId(crate::lower::desugar_var_seed());
                                ctx.lower_bind(t, &expr.ty, expr).ok()?;
                                ctx.value_for(t).ok()?
                            }
                            _ => return None,
                        };
                        let h = ctx.fresh_value();
                        ctx.ops.push(Op::Prim {
                            kind: PrimKind::Handle,
                            dst: Some(h),
                            args: vec![handle],
                        });
                        let tag = ctx.load_at_offset(h, 4, PrimKind::Load { width: 4 });
                        let result = ctx.fresh_value();
                        ctx.ops.push(Op::IfThen { cond: tag, dst: Some(result) });
                        let borrowed = ctx.load_at_offset(h, 12, PrimKind::LoadHandle);
                        let owned = ctx.fresh_value();
                        ctx.ops.push(Op::Dup { dst: owned, src: borrowed });
                        ctx.ops.push(Op::Else { val: Some(owned) });
                        let fb = ctx.lift_lambda(&params, &body)?;
                        // The fallback block is ELSE-ARM-LOCAL and MOVES into
                        // the merge — remove it from the scope-end drop set
                        // (`lift_lambda` pushed it): an unconditional drop
                        // would free a never-allocated local on the Some path.
                        ctx.live_heap_handles.retain(|x| *x != fb);
                        ctx.ops.push(Op::EndIf { val: Some(fb) });
                        if track_result {
                            ctx.live_heap_handles.push(result);
                        }
                        ctx.closure_values.insert(result);
                        Some(result)
                    });
                    if out.is_some() {
                        return out;
                    }
                }
                return None;
            }
        }
        let synth = IrExpr {
            kind: IrExprKind::Unit,
            ty: fallback.ty.clone(),
            span: expr.span.clone(),
            def_id: None,
        };
        let m = if expr.ty.is_result() {
            Self::unwrap_or_as_result_match(&synth, expr, fallback)
        } else if expr.ty.is_option() {
            Self::unwrap_or_as_option_match(&synth, expr, fallback)
        } else {
            return None;
        };
        let out = self.speculate(|ctx| {
            let IrExprKind::Match { subject, arms } = &m.kind else { unreachable!() };
            if let Some(v) = ctx.try_lower_variant_value_match(subject, arms, &m.ty) {
                return Some(v);
            }
            // Subject-ANF retry: a CALL subject the value-match path cannot
            // classify (a payload-returning helper outside the name sets)
            // often lowers fine as a BIND — the bind module-call path routes
            // the callee — after which the match dispatches on a tracked VAR,
            // the proven subject shape. The enclosing `speculate` rolls
            // everything back on decline.
            if !matches!(subject.kind, IrExprKind::Call { .. }) {
                return None;
            }
            let t = almide_ir::VarId(crate::lower::desugar_var_seed());
            ctx.lower_bind(t, &subject.ty, subject).ok()?;
            // The bind's own seeding is NAME-gated (the rot this arc razes) —
            // seed the temp BY TYPE (law 4: the type IS the knowledge).
            if let Ok(tval) = ctx.value_for(t) {
                ctx.seed_variant_value_shape(tval, &subject.ty);
            }
            let tv = IrExpr {
                kind: IrExprKind::Var { id: t },
                ty: subject.ty.clone(),
                span: subject.span.clone(),
                def_id: None,
            };
            ctx.try_lower_variant_value_match(&tv, arms, &m.ty)
        });
        if let Some(v) = out {
            // Seed the MERGE's read shape by TYPE (the #1270 bind-position
            // discipline) and OWN it per the caller's position.
            self.seed_variant_value_shape(v, &m.ty);
            if track_result && is_heap_ty(&m.ty) {
                self.live_heap_handles.push(v);
            }
        }
        crate::trace::trace("ALMIDE_DBG_QQ", || match out {
            Some(_) => format!("[qq] match took {:?} ?? (heap)", expr.ty),
            None => format!("[qq] declined {:?} ?? (heap)", expr.ty),
        });
        out
    }

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
        // Every caller emits this arm under REAL `IfThen`/`Else`/`EndIf` markers (the
        // scalar-if machinery, the variant/refinement chains, the desugared match
        // chains) — exactly ONE arm runs at runtime, so a mutable-global/shared-cell
        // write inside it is a real conditional effect, and a scalar reassignment of
        // an outer var must hit its stable local (see `LowerCtx::unit_arm_depth`).
        // Without this the modeled-frame fence (`in_frame > 0 && unit_arm_depth == 0`)
        // misfired on the ordinary early-return guard `if len(d) < 12 then 0 else
        // { bytes.set_i32_le(g, …) … }`: the arm's stmt walled, the scalar-if rolled
        // back, and the whole fn fell to the linearization wall blaming the CONDITION
        // (#907, nendo's load_vrm_data). The fence still walls the true both-arms
        // linearization — `lower_branch_arm` raises only `in_frame`.
        self.unit_arm_depth += 1;
        for stmt in stmts {
            if self.lower_stmt(stmt).is_err() {
                self.unit_arm_depth -= 1;
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
            // A nested CUSTOM-variant (user ADT) match (`A(b) => match b { … }` — the
            // mutual-recursive-types depth walk): dispatch on its tag exactly like the
            // tail does. Checked BEFORE the Option/Result variant path (a custom ADT is
            // not an Option/Result). Without this the nested match fell to the deferred
            // Const-0 (a silent miscompile).
            IrExprKind::Match { subject, arms }
                if self.custom_variant_type_name(&subject.ty).is_some() =>
            {
                self.try_lower_custom_variant_match(subject, arms, &t.ty)
            }
            // A TUPLE-of-bound-vars subject (the frontend's factored form of a
            // multi-arm nested-ctor match — C-070): per-element refinement chain.
            IrExprKind::Match { subject, arms }
                if matches!(subject.kind, IrExprKind::Tuple { .. }) =>
            {
                self.try_lower_tuple_refinement_match(subject, arms, &t.ty)
            }
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
            // A SCALAR-typed `Try`/`Unwrap` tail over a never-err effect call (`else { … loop(n-1, acc) }`
            // — the recursive value-accumulator: the frontend wraps the propagating effect call in
            // `Try` for the auto-`?`, and the call-type unwrap pass already made its `.ty` the raw scalar
            // `T`). The `?`/`!` over a never-err call is a no-op (it always returns Ok), so strip it and
            // lower the inner call as the scalar value. WITHOUT this the `Try`/`Unwrap` fell to the
            // `_ => lower_scalar_value | try_lower_scalar_call` arm (neither handles a `Try`/`Unwrap`
            // node) → the arm FAILED → the scalar-`if` rolled back → the whole body collapsed to a
            // deferred Const (the recursive accumulator returned a garbage 0). The scalar-arm twin of
            // the statement/tail effect-`Try` fix.
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } if !is_heap_ty(&t.ty) => {
                self.lower_scalar_value(expr).or_else(|| self.try_lower_scalar_call(expr, &expr.ty))
            }
            _ => self.lower_scalar_value(t).or_else(|| self.try_lower_scalar_call(t, &t.ty)),
        });
        self.unit_arm_depth -= 1;
        self.in_frame -= 1;
        self.drop_arm_locals(lhh_mark);
        val
    }
}

