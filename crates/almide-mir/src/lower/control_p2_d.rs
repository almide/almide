impl LowerCtx {

    fn tuple_refinement_chain(
        &mut self,
        elems: &[(ValueId, Ty)],
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let heap = is_heap_ty(result_ty);
        let lower_arm = |s: &mut Self, body: &IrExpr| -> Option<ValueId> {
            if heap {
                s.lower_heap_result_arm(body, result_ty)
            } else {
                s.lower_scalar_arm(body)
            }
        };
        let arm = arms.first()?;
        if matches!(arm.pattern, IrPattern::Wildcard) {
            return lower_arm(self, &arm.body);
        }
        let IrPattern::Tuple { elements: pats } = &arm.pattern else { return None };
        let mut conj: Option<ValueId> = None;
        for (p, (v, ty)) in pats.iter().zip(elems.iter()) {
            let ci = match p {
                IrPattern::Wildcard => continue,
                IrPattern::Literal { expr } => {
                    let lit = self.fresh_value();
                    let value = match &expr.kind {
                        IrExprKind::LitInt { value } => *value,
                        IrExprKind::LitBool { value } => *value as i64,
                        _ => return None,
                    };
                    self.ops.push(Op::ConstInt { dst: lit, value });
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Eq, a: *v, b: lit });
                    c
                }
                IrPattern::Constructor { .. } => {
                    let vname = self.custom_variant_type_name(ty)?;
                    self.nested_refinement_cond(*v, p, &vname)?
                }
                _ => return None,
            };
            conj = Some(match conj {
                None => ci,
                Some(prev) => {
                    let c = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Mul, a: prev, b: ci });
                    c
                }
            });
        }
        // An all-wildcard tuple arm is irrefutable — its body IS the rest.
        let Some(cond) = conj else {
            return lower_arm(self, &arm.body);
        };
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        // RELEASE PARITY for the HEAP merge (mirrors emit_variant_arm_chain): an
        // outer handle one side moves out must be released by the other, else the
        // branch-grouped cert rejects the path-dependent accounting. The scalar
        // path emits no ownership events, so the parity sets stay empty there.
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let then_v = lower_arm(self, &arm.body)?;
        let consumed_by_then: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_v) });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let rest_v = self.tuple_refinement_chain(elems, &arms[1..], result_ty)?;
        let consumed_by_else: Vec<ValueId> = live_after_then
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        for x in &consumed_by_then {
            if !consumed_by_else.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_else {
            if !consumed_by_then.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(rest_v) });
        Some(dst)
    }

    pub(crate) fn try_lower_custom_variant_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        if arms.is_empty() || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        // C-070: nested refutable ctor/literal patterns — the ordered refinement
        // chain (declines untouched shapes; the tag-switch below owns the rest).
        if let Some(dst) = self.try_lower_nested_refinement_match(subject, arms, result_ty) {
            return Some(dst);
        }
        // The subject must be a registered custom variant; clone its layout out of the borrow.
        let type_name = self.custom_variant_type_name(&subject.ty)?;
        let layout = self.variant_layouts.by_type.get(&type_name)?.clone();
        // DEPTH-2 patterns over a SINGLE-CTOR outer (`match o { Wrap(A(n)) => …, Wrap(B(m))
        // => … }` — the `pick` shape): the one outer ctor ALWAYS matches, so the match IS the
        // inner dispatch over the payload. STRIP the outer layer (arms' patterns become the
        // inner patterns; the inner layout comes from the payload field's variant type) and
        // remember to UNWRAP one level: the dispatch handle becomes the payload's slot-1
        // handle (a BORROW — the subject keeps owning it; loaded only after the subject
        // materializes, so no wrong-ctor garbage read is possible: there IS no other ctor).
        let mut layout = layout;
        let mut stripped: Vec<IrMatchArm>;
        let mut arms: &[IrMatchArm] = arms;
        let mut unwrap_single = false;
        // Guard-clause flattening of the former 6-deep nested-if (no `else` anywhere: any
        // unmet condition falls through to the code after this block, unchanged, exactly as
        // the original fell out of the if-pyramid — `break` exits the labeled block and
        // resumes there). `case`'s borrow of the OLD `layout` still ends before `layout =
        // inner_layout` reassigns it (its last use, inside `all_nested`, is unchanged and
        // stays textually before the reassignment). No behavior change — see
        // docs/roadmap/active/code-health-codopsy.md.
        'single_ctor_strip: {
            if layout.cases.len() != 1 {
                break 'single_ctor_strip;
            }
            let case = &layout.cases[0];
            if case.fields.len() != 1 {
                break 'single_ctor_strip;
            }
            let all_nested = arms.iter().all(|a| matches!(&a.pattern,
                IrPattern::Constructor { name, args }
                    if *name == case.ctor && args.len() == 1
                        && matches!(args[0], IrPattern::Constructor { .. })));
            if !all_nested {
                break 'single_ctor_strip;
            }
            let inner_ty = case.fields[0].1.clone();
            let Some(inner_name) = self.custom_variant_type_name(&inner_ty) else {
                break 'single_ctor_strip;
            };
            let Some(inner_layout) = self.variant_layouts.by_type.get(&inner_name).cloned()
            else {
                break 'single_ctor_strip;
            };
            stripped = Vec::with_capacity(arms.len());
            for a in arms {
                let IrPattern::Constructor { args, .. } = &a.pattern else {
                    unreachable!("gated above")
                };
                stripped.push(IrMatchArm {
                    pattern: args[0].clone(),
                    guard: a.guard.clone(),
                    body: a.body.clone(),
                });
            }
            layout = inner_layout;
            arms = &stripped;
            unwrap_single = true;
        }
        let plans = self.parse_variant_arms(&layout, arms)?;
        // A SINGLE-arm HEAP-result match (a 1-ctor newtype `unbox`, `match b { B(x) => x }`) that
        // returned the arm value DIRECTLY to `func.ret` would double-move (the arm's move-out
        // `Consume` + the ret's move — the `amm`/`aamdm` net-−1 the proven checker REJECTS). A
        // 1-CTOR variant's tag ALWAYS matches (there is no other constructor), so route the arm
        // through an IfThen `dst` (one ret move, exactly like a multi-arm match) whose ELSE is an
        // unreachable empty-heap block — never executed, so no leak. A single-arm WILDCARD (`_ =>
        // body`) has no ctor tag to test → stays declined (a later brick). See
        // [`Self::emit_single_ctor_heap_arm`].
        let sole_ctor_heap = is_heap_ty(result_ty)
            && plans.len() == 1
            && matches!(plans[0].0, VariantArmKind::Ctor { .. });
        if is_heap_ty(result_ty) && plans.len() == 1 && !sole_ctor_heap {
            return None;
        }
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize/borrow the subject → a Handle (the variant block pointer).
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // A DEFERRED-Opaque subject is an EMPTY block: reading its tag would take a wrong
        // arm silently (the record-ctor mt2 miscompile) — decline (the tail walls honestly).
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // A HEAP result over an OWNED subject temp would overlap the owned-subject borrow with the
        // arm's heap move-out (the cert rejects it). Subject-drop-before-arms is ADT brick 4b —
        // for now WALL it (a borrowed param/var subject, the recursive-to_string case, proceeds).
        if is_heap_ty(result_ty) && self.live_heap_handles.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // Read the tag from slot 0, then emit the per-arm if-chain.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        // The depth-2 single-outer unwrap: the dispatch handle becomes the payload's
        // slot-1 handle (BORROWED — the subject owns it; freed by the subject's own
        // recursive drop, so param_values keeps it un-dropped here).
        let h = if unwrap_single {
            let payload = self.load_at_offset(
                h,
                layout::slot_offset(1) as i64,
                PrimKind::LoadHandle,
            );
            self.param_values.insert(payload);
            let ph = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![payload] });
            ph
        } else {
            h
        };
        // Rung-5 variants slab: the tag is slot 0 of the subject block — the
        // TARGET-NEUTRAL `ListGetScalar` reads it on both legs. The depth-2
        // unwrap (`h` re-pointed at the payload block) keeps the raw load: its
        // container is the borrowed payload, not `subj`.
        let tag = if unwrap_single {
            self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 })
        } else {
            let idx = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: idx, value: 0 });
            let t = self.fresh_value();
            self.ops.push(Op::ListGetScalar { dst: t, list: subj, idx });
            t
        };
        let emitted = if sole_ctor_heap {
            let (kind, body) = &plans[0];
            self.emit_single_ctor_heap_arm(h, tag, kind, body, result_ty, subj)
        } else {
            self.emit_variant_arm_chain(h, tag, &plans, result_ty, subj)
        };
        match emitted {
            Some(dst) => Some(dst),
            None => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
        }
    }

    /// Route a SOLE-constructor HEAP-result arm through an IfThen `dst` (one ret move) with an
    /// unreachable empty-heap ELSE. A 1-ctor variant's tag always equals `arm_tag`, so the `else` is
    /// dead — it exists only so the arm value flows through the branch-merge `dst` the ownership
    /// certificate needs (a direct return would double-move — see the caller). The empty-heap block is
    /// never allocated at runtime, so it cannot leak.
    fn emit_single_ctor_heap_arm(
        &mut self,
        h: ValueId,
        tag: ValueId,
        kind: &VariantArmKind,
        body: &IrExpr,
        result_ty: &Ty,
        subj: ValueId,
    ) -> Option<ValueId> {
        let arm_tag = match kind {
            VariantArmKind::Ctor { tag, .. } => *tag,
            VariantArmKind::Wildcard | VariantArmKind::BindAll { .. } => return None,
        };
        let dst = self.fresh_value();
        let tc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tc, value: arm_tag });
        let cond = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond, op: IntOp::Eq, a: tag, b: tc });
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        let then_v = self.lower_variant_arm_value(kind, body, h, result_ty, true, subj)?;
        self.ops.push(Op::Else { val: Some(then_v) });
        // The dead `else`: a fresh owned empty-string block (a heap i32 handle, repr-compatible with
        // any heap result). `Consume` moves it into `dst` exactly as a real arm value would.
        let repr = repr_of(result_ty).ok()?;
        let else_v = self.fresh_value();
        self.ops.push(Op::Alloc { dst: else_v, repr, init: crate::Init::Str(String::new()) });
        self.ops.push(Op::Consume { v: else_v });
        self.ops.push(Op::EndIf { val: Some(else_v) });
        Some(dst)
    }

    /// Parse a custom-variant `match`'s arms into per-arm plans — shared by the value-result
    /// ([`Self::try_lower_custom_variant_match`]) and Unit-statement
    /// ([`Self::lower_custom_variant_unit_match`]) paths. `None` (the caller walls / declines)
    /// if any arm is outside the scalar-field subset: a guard, a heap-field bind, a nested ctor
    /// pattern, or a binder catch-all `x => …` (all later bricks). The bodies stay borrowed
    /// from `arms` (a param, not `self`) — no borrow conflict with the lowering that follows.
    fn parse_variant_arms<'a>(
        &self,
        layout: &VariantLayout,
        arms: &'a [IrMatchArm],
    ) -> Option<Vec<(VariantArmKind, &'a IrExpr)>> {
        let mut plans: Vec<(VariantArmKind, &IrExpr)> = Vec::with_capacity(arms.len());
        for arm in arms {
            if arm.guard.is_some() {
                return None;
            }
            let kind = match &arm.pattern {
                IrPattern::Constructor { name, args } => {
                    let case = layout.case_by_ctor(name)?;
                    if args.len() != case.fields.len() {
                        return None;
                    }
                    let mut binds = Vec::new();
                    for (i, fp) in args.iter().enumerate() {
                        match fp {
                            IrPattern::Wildcard => {}
                            // slot 1+i (slot 0 is the tag). A SCALAR field binds by value copy.
                            IrPattern::Bind { var, ty } if !is_heap_ty(ty) => {
                                binds.push((1 + i, *var, false, ty.clone()))
                            }
                            // ANY heap field (`String`, a nested VARIANT, a `List[…]` —
                            // `ArrV(xs) => for x in xs`, the gguf ValArray consumer — Bytes,
                            // Matrix) binds as a BORROW of the slot handle: the subject owns
                            // it, a move-out auto-Dups, a borrow-pass just reads. The bind is
                            // type-agnostic (a slot-handle load); what the ARM does with it is
                            // gated by the arm-body lowering as usual.
                            IrPattern::Bind { var, ty } if is_heap_ty(ty) => {
                                binds.push((1 + i, *var, true, ty.clone()))
                            }
                            // a nested ctor pattern — a later brick.
                            _ => return None,
                        }
                    }
                    VariantArmKind::Ctor { tag: case.tag as i64, binds }
                }
                // A RECORD-ctor pattern (`Node { left, right, value }`, `Data { seq, .. }`,
                // `Click { .. }`): resolve each named field to its declared slot (1 + index)
                // and bind exactly like the positional ctor arm — scalar by value copy, heap
                // as a borrow of the slot handle. `..`/unmentioned fields bind nothing; a
                // NESTED field pattern stays a later brick.
                IrPattern::RecordPattern { name, fields, rest: _ } => {
                    let case = layout.case_by_ctor(name)?;
                    let mut binds = Vec::new();
                    for f in fields {
                        let idx = case
                            .fields
                            .iter()
                            .position(|(n, _)| n.as_str() == f.name)?;
                        match &f.pattern {
                            None | Some(IrPattern::Wildcard) => {}
                            Some(IrPattern::Bind { var, ty }) if !is_heap_ty(ty) => {
                                binds.push((1 + idx, *var, false, ty.clone()))
                            }
                            Some(IrPattern::Bind { var, ty }) if is_heap_ty(ty) => {
                                binds.push((1 + idx, *var, true, ty.clone()))
                            }
                            _ => return None,
                        }
                    }
                    VariantArmKind::Ctor { tag: case.tag as i64, binds }
                }
                IrPattern::Wildcard => VariantArmKind::Wildcard,
                // A BINDER catch-all (`e => …`): binds the whole subject (borrow), any tag.
                IrPattern::Bind { var, ty } if is_heap_ty(ty) => {
                    VariantArmKind::BindAll { var: *var }
                }
                _ => return None,
            };
            plans.push((kind, &arm.body));
        }
        Some(plans)
    }

    /// Bind a custom-variant arm's ctor fields from the block's slots (a `Wildcard` arm binds
    /// nothing). A SCALAR field is an i64 value COPY (`Load`); a leaf-heap (`String`) field is a
    /// `Dup`'d OWNED copy of the slot handle (`LoadHandle` then `Op::Dup`, rc+1) pushed to
    /// `live_heap_handles` so the ARM FRAME drops it at arm end (`emit_variant_arm_chain` marks
    /// before this call). The OWNED copy — not a borrow — is what the proven checker needs: a
    /// consuming re-use moves an owned ref, a read-only use drops it, a move-out hands it off,
    /// all rc-balanced; a BORROW would `Consume`/`m` at rc 0 on a re-use (the rejected double-free).
    fn bind_variant_arm(&mut self, kind: &VariantArmKind, h: ValueId, subj: ValueId) {
        if let VariantArmKind::BindAll { var } = kind {
            // The whole-subject borrow: the subject's owner (an outer temp / param) keeps
            // the reference; a consuming re-use in the arm (`err(e)`) Dups it.
            self.value_of.insert(*var, subj);
            self.param_values.insert(subj);
            return;
        }
        if let VariantArmKind::Ctor { binds, .. } = kind {
            for (slot, var, is_heap, fty) in binds {
                let off = layout::slot_offset(*slot) as i64;
                if *is_heap {
                    // BORROW the slot handle: the subject owns the String; a move-out auto-Dups
                    // in `lower_heap_result_arm`, a consuming re-use Dups in `lower_owned_heap_field`.
                    let p = self.load_at_offset(h, off, crate::PrimKind::LoadHandle);
                    self.param_values.insert(p);
                    // An Option/Result PAYLOAD field (`Box(Option[Int])` — the depth-2
                    // single-outer + builtin-inner class): seed the borrowed handle's
                    // READ-shape so the inner `match $f { Some(n)/None }` executes (reads the
                    // len/cap tag) instead of walling on a scalar destructure. Gated to
                    // Option/Result exactly (the canonical seeder's variant arms) — a
                    // String/List/nested-variant bind keeps today's borrow-only discipline.
                    if matches!(fty,
                        Ty::Applied(
                            almide_lang::types::constructor::TypeConstructorId::Option
                                | almide_lang::types::constructor::TypeConstructorId::Result,
                            _
                        ))
                    {
                        self.seed_variant_param(p, fty);
                    }
                    // A CLOSURE payload (`Run(f) => f()` — the variant-stored closure
                    // class): the borrowed handle IS a closure block — admit it to the
                    // dispatch set so the arm's `f(…)` lowers to `CallIndirect`.
                    if matches!(fty, Ty::Fn { .. }) {
                        self.closure_values.insert(p);
                    }
                    self.value_of.insert(*var, p);
                } else {
                    // Rung-5 variants slab: a SCALAR payload is a plain slot of the
                    // subject block — target-neutral ListGetScalar (native `subj[slot]`).
                    let _ = off;
                    let idx = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: idx, value: *slot as i64 });
                    let payload = self.fresh_value();
                    self.ops.push(Op::ListGetScalar { dst: payload, list: subj, idx });
                    self.value_of.insert(*var, payload);
                }
            }
        }
    }

    /// Lower a UNIT-result custom-variant `match` in STATEMENT position (ADT brick 3, the unit
    /// sibling of [`Self::try_lower_custom_variant_match`]) — read the tag@slot0 and run only the
    /// taken arm's EFFECTS. The subject is ALREADY materialized/borrowed by the caller (the
    /// statement-`Match` entry), passed as `subject_value`.
    ///
    /// A custom variant must NEVER fall to the both-arms LINEARIZATION (that runs every arm's
    /// effects = a silent miscompile — e.g. all three `println`s instead of one), so this returns
    /// `Err` (WALL) on an out-of-subset arm rather than declining to the linearizer. Each arm
    /// runs in a per-arm frame (`lower_branch_arm`), wrapped in `IfThen`/`Else`/`EndIf` markers
    /// (no-ops in `verify_ownership`); the last arm / any wildcard is the unconditional else.
    pub(crate) fn lower_custom_variant_unit_match(
        &mut self,
        subject_ty: &Ty,
        subject_value: Option<ValueId>,
        arms: &[IrMatchArm],
    ) -> Result<(), LowerError> {
        use crate::PrimKind;
        let wall = |what: &str| {
            Err(LowerError::Unsupported(format!(
                "custom-variant statement match {what} cannot be faithfully lowered (a both-arms \
                 linearization would run every arm's effects) not in this brick"
            )))
        };
        let Some(subj) = subject_value else {
            return wall("over a non-materialized subject");
        };
        // A DEFERRED-Opaque subject is an EMPTY block: reading its tag would execute a
        // wrong arm silently (the record-ctor mt2 miscompile) — wall it honestly.
        if self.deferred_opaque_binds.contains(&subj) {
            return wall("over a deferred (unmaterialized) subject");
        }
        let type_name = match self.custom_variant_type_name(subject_ty) {
            Some(n) => n,
            None => return wall("over an unresolved variant type"),
        };
        let layout = match self.variant_layouts.by_type.get(&type_name) {
            Some(l) => l.clone(),
            None => return wall("over an unregistered variant"),
        };
        let plans = match self.parse_variant_arms(&layout, arms) {
            Some(p) if !p.is_empty() => p,
            _ => return wall("with an arm outside the scalar-field subset"),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        // Rung-5 variants slab: slot-0 tag via the target-neutral ListGetScalar.
        let idx = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: idx, value: 0 });
        let tag = self.fresh_value();
        self.ops.push(Op::ListGetScalar { dst: tag, list: subj, idx });
        self.emit_variant_unit_chain(h, tag, &plans, subj)
    }

    /// Emit the right-nested `if tag == t0 { arm0 } else if … else { last }` chain for a
    /// UNIT-result custom-variant statement match. Each arm is a per-arm effect frame
    /// (`lower_branch_arm` with no result), the markers carry `val: None`. The last plan / any
    /// wildcard is the unconditional else. `Err` (the whole match walls) if an arm body is out
    /// of subset — the unit sibling of [`Self::emit_variant_arm_chain`].
    fn emit_variant_unit_chain(
        &mut self,
        h: ValueId,
        tag: ValueId,
        plans: &[(VariantArmKind, &IrExpr)],
        subj: ValueId,
    ) -> Result<(), LowerError> {
        let Some(((kind, body), rest)) = plans.split_first() else {
            return Ok(());
        };
        if rest.is_empty() || matches!(kind, VariantArmKind::Wildcard | VariantArmKind::BindAll { .. }) {
            return self.lower_variant_unit_arm(kind, body, h, subj);
        }
        let arm_tag = match kind {
            VariantArmKind::Ctor { tag, .. } => *tag,
            VariantArmKind::Wildcard | VariantArmKind::BindAll { .. } => {
                unreachable!("handled above")
            }
        };
        let tc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tc, value: arm_tag });
        let cond = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond, op: IntOp::Eq, a: tag, b: tc });
        self.ops.push(Op::IfThen { cond, dst: None });
        self.lower_variant_unit_arm(kind, body, h, subj)?;
        self.ops.push(Op::Else { val: None });
        self.emit_variant_unit_chain(h, tag, rest, subj)?;
        self.ops.push(Op::EndIf { val: None });
        Ok(())
    }

    /// Lower one UNIT-statement custom-variant arm (its effects), with a PER-ARM FRAME that
    /// drops the arm's `Dup`'d heap-field binds at arm end (the unit sibling of
    /// [`Self::lower_variant_arm_value`]). The mark precedes `bind_variant_arm` so a heap field
    /// bound + read by the effect (`println(s)`) is released here. Scalar arms add nothing.
    fn lower_variant_unit_arm(
        &mut self,
        kind: &VariantArmKind,
        body: &IrExpr,
        h: ValueId,
        subj: ValueId,
    ) -> Result<(), LowerError> {
        let mark = self.live_heap_handles.len();
        self.bind_variant_arm(kind, h, subj);
        // This arm executes under REAL `IfThen`/`Else`/`EndIf` markers (only the taken
        // arm runs — `emit_variant_unit_chain`), so it is an EXECUTABLE unit arm:
        // raise `unit_arm_depth` so a shared-cell/mutable-global write inside it is
        // admitted as a real conditional effect (the modeled-frame guard keys on this;
        // `lower_branch_arm` alone raises only `in_frame`, which also covers the
        // both-arms linearization where such a write MUST wall).
        self.unit_arm_depth += 1;
        let r = self.lower_branch_arm(None, body);
        self.unit_arm_depth -= 1;
        r?;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Emit the right-nested `if tag == t0 { arm0 } else if … else { last }` chain for a
    /// custom-variant value match, returning the ValueId holding the chain's result. The LAST
    /// plan is the unconditional `else` (no tag test — exhaustiveness guarantees it matches); a
    /// `Wildcard` anywhere is likewise an unconditional `else` (the rest is unreachable). Each
    /// arm body lowers in its own per-arm frame — `lower_scalar_arm` for a scalar result
    /// (ADT brick 3), `lower_heap_result_arm` for a heap result (ADT brick 4, the arm moves out
    /// a fresh heap value). `None` (caller rolls back) if an arm body is outside the subset.
    fn emit_variant_arm_chain(
        &mut self,
        h: ValueId,
        tag: ValueId,
        plans: &[(VariantArmKind, &IrExpr)],
        result_ty: &Ty,
        subj: ValueId,
    ) -> Option<ValueId> {
        let heap = is_heap_ty(result_ty);
        let ((kind, body), rest) = plans.split_first()?;
        // The last arm, or any Wildcard, is the unconditional else (no tag test).
        if rest.is_empty() || matches!(kind, VariantArmKind::Wildcard | VariantArmKind::BindAll { .. }) {
            return self.lower_variant_arm_value(kind, body, h, result_ty, heap, subj);
        }
        let arm_tag = match kind {
            VariantArmKind::Ctor { tag, .. } => *tag,
            VariantArmKind::Wildcard | VariantArmKind::BindAll { .. } => {
                unreachable!("handled above")
            }
        };
        let dst = self.fresh_value();
        let tc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tc, value: arm_tag });
        let cond = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond, op: IntOp::Eq, a: tag, b: tc });
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        // RELEASE PARITY (mirrors lower_heap_result_if_inner): an OUTER handle
        // this arm moves out must be released by the rest of the chain, and vice
        // versa — otherwise the accounting is path-dependent (the branch-grouped
        // cert `{m|}` rejects it; this keeps the lowering ahead of the checker).
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let then_v = self.lower_variant_arm_value(kind, body, h, result_ty, heap, subj)?;
        let consumed_by_then: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_v) });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let else_v = self.emit_variant_arm_chain(h, tag, rest, result_ty, subj)?;
        let consumed_by_else: Vec<ValueId> = live_after_then
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        for x in &consumed_by_then {
            if !consumed_by_else.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op); // the rest of the chain releases what this arm moved out
            }
        }
        for x in &consumed_by_else {
            if !consumed_by_then.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op); // this arm releases what the chain moved out
            }
        }
        self.ops.push(Op::EndIf { val: Some(else_v) });
        Some(dst)
    }

    /// Lower one custom-variant arm to its value, with a PER-ARM FRAME that drops the arm's
    /// `Dup`'d heap-field binds at arm end. The mark is taken BEFORE `bind_variant_arm` (whose
    /// owned heap binds land in `live_heap_handles`), so `drop_arm_locals` releases exactly the
    /// fields not moved out: a borrow-passed field (`tos(l)`) drops here; a moved-out field
    /// (`Text(s) => s`) was `Dup`+`Consume`'d again by `lower_heap_result_arm`, so its original
    /// bind still drops here (rc-balanced — the transient extra ref is freed). A scalar arm adds
    /// nothing to the frame, so this is a no-op for the brick-2/3 paths.
    fn lower_variant_arm_value(
        &mut self,
        kind: &VariantArmKind,
        body: &IrExpr,
        h: ValueId,
        result_ty: &Ty,
        heap: bool,
        subj: ValueId,
    ) -> Option<ValueId> {
        let mark = self.live_heap_handles.len();
        self.bind_variant_arm(kind, h, subj);
        let v = if heap {
            self.lower_heap_result_arm(body, result_ty)
        } else {
            self.lower_scalar_arm(body)
        }?;
        self.drop_arm_locals(mark);
        Some(v)
    }
}
