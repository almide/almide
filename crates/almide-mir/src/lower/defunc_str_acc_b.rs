/// The list an append-accumulator loop is writing into.
///
/// `handle` and `cursor` are both `ValueId` and were adjacent parameters, so a
/// transposition writes the element through the list handle — a memory write to
/// the wrong address that still type-checks.
#[derive(Copy, Clone)]
pub(crate) struct ResultList<'a> {
    pub handle: ValueId,
    pub cursor: ValueId,
    pub elem: &'a Ty,
    /// The element stride in bytes, materialised once by the caller.
    pub elem_size: ValueId,
}

impl LowerCtx {

    /// A flat_map closure body that is a VARIANT `match subj { some(pl) => …, none => … }` over a
    /// per-element `Option[Value]` subject (`match json.get(case, "payload") { … }` — the bindgen
    /// `gen_variant_type/struct/class` shape). Lowered as a UNIT control structure that APPENDS each
    /// arm into the loop-carried `acc` slot (NO merged heap-if value — the same discipline the `if`
    /// case proves). Returns `Some(())` on success, `None` (the caller rolls back + WALLs) outside
    /// the subset.
    ///
    /// SUBSET: a 2-arm `[some(scalar|heap bind?), none]` (no guards), over an Option whose materialize
    /// makes it a TRACKED nested-ownership block (a self-host Option call — `json.get`/`list.first`/…).
    /// A self-host Result subject (Ok/Err) self-gates out (only Option here); a custom variant / a
    /// non-self-host Option declines.
    ///
    /// SOUNDNESS — per-iteration subject + borrowed payload, exactly the `try_lower_variant_value_match`
    /// `str_heap_bind`/`opt_tuple_bind` path but for a UNIT append target:
    ///  - The subject `json.get(case, "payload")` is materialized into a FRESH OWNED `Option[Value]`
    ///    block (cert `i`) INSIDE this iteration's frame, tracked `materialized_options` +
    ///    `heap_elem_lists` so its drop is the recursive `DropListStr` (frees the owned Value payload).
    ///  - A `some(pl)` HEAP payload binds `pl` to the subject's slot-0 handle (`LoadHandle` @12, in
    ///    `param_values`) — a BORROW: the subject still owns it, so `pl` is NOT a second owner and the
    ///    some-arm's reads/appends never free it. A consuming append auto-acquires (the leaf builders
    ///    Dup/copy into the owned sublist), so no double-free.
    ///  - The subject must stay live THROUGH the some-arm (the borrow is read there), so it is dropped
    ///    AFTER both arms — within THIS iteration's frame (cert `d`). So the subject is a balanced
    ///    `i…d` episode per iteration (like the heap `if` cond's transient temps), and `acc` stays the
    ///    loop-carried `i(id)m` slot. No leak (the subject + its payload are freed each iteration), no
    ///    double-free (the payload is borrowed, freed once by the subject's `DropListStr`), no
    ///    sibling-arm trap (the tag picks exactly one arm; the appends are per-arm-balanced).
    ///  - BRANCH OWNERSHIP ISOLATION: the two arms are ALTERNATE — snapshot/restore the
    ///    owned/borrowed sets around the some-arm so a borrow it consumes does not leak into the
    ///    none-arm's lowering view (mirrors `try_lower_variant_value_match`). The `acc` SetLocal is a
    ///    real op (survives the snapshot restore — only lowering-time tracking is reset).
    fn append_variant_match_to_str_acc(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        acc: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        // ONLY an INLINE self-host Option CALL subject (`match json.get(case, "payload") { … }`). A
        // let-bound Var subject (`let pv = json.get(…); match pv { … }`) is NOT admitted: borrowing
        // that Option block in the unit-append context produced an EMPTY some-arm (WRONG bytes — a value
        // miscompile the leak oracle does not catch), so it WALLs honestly until that read is
        // understood. A Result / custom variant / a non-self-host Option also declines.
        if arms.len() != 2
            || arms.iter().any(|a| a.guard.is_some())
            || !is_self_host_option_call(subject)
            || !is_heap_ty(&subject.ty)
        {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize the per-element subject into a FRESH OWNED Option block (cert `i`), dropped AFTER
        // the arms (cert `d`) within THIS iteration — the per-iteration `i…d` balance. Track it like the
        // statement/value match entry (so the heap-payload bind gate opens AND the post-arm drop is the
        // recursive DropListStr that frees the owned Value payload).
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        self.value_shapes.insert(subj, crate::lower::VariantShape::Option);
        if crate::lower::is_heap_elem_list_ty(&subject.ty) {
            self.value_drops.entry(subj).or_default().flat_elems = true;
        }
        let ((some_body, some_bind), none_body) = match self.parse_option_append_arms(arms, subj) {
            Some(parsed) => parsed,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // tag = load32(handle(subj) + 4); if tag != 0 then Some-arm else None-arm (UNIT — dst None).
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        // Bind the Some payload BEFORE the IfThen so it is in scope for the some-arm. A SCALAR is a
        // value COPY (load64); a HEAP element is `LoadHandle` (@12, an i32 Ptr) recorded in
        // `param_values` (a BORROW — the subject owns it, freed by its post-arm DropListStr).
        self.bind_variant_payload_from_slot0(h, some_bind);
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        // BRANCH OWNERSHIP ISOLATION: the arms are alternate — snapshot the owned/borrowed sets
        // before the some-arm, restore before the none-arm (the emitted ops are per-branch; only the
        // lowering-time tracking is reset). The shared payload binds survive (inserted before IfThen).
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        self.unit_arm_depth += 1;
        let some_ok = self.append_body_to_str_acc(some_body, acc);
        self.ops.push(Op::Else { val: None });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot;
        self.materialized_aggregates = ma_snapshot;
        let none_ok = some_ok.and_then(|_| self.append_body_to_str_acc(none_body, acc));
        self.unit_arm_depth -= 1;
        self.ops.push(Op::EndIf { val: None });
        if none_ok.is_none() {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // SUBJECT-DROP-AFTER-ARMS: the Some payload borrowed slot-0, so the fresh per-iteration Option
        // stayed live through both arms — drop it ONCE here (the recursive DropListStr frees it + its
        // owned Value payload), closing the per-iteration `i…d` balance. The subject is ALWAYS a fresh
        // owned inline-call result (the Var-subject borrow form is gated out above), so this drop is
        // unconditional.
        self.drop_owned_subject_after_arms(subj);
        Some(())
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_str_acc`] (codopsy round-3 sweep,
    /// #852): parses the two arms into `((some_body, some_bind), none_body)`, deciding each Some
    /// payload's bind form (scalar COPY / heap BORROW / nothing). Takes `&self` — it reads the
    /// tracking sets but emits NO ops, which is exactly why the caller's rollback (unchanged, now at
    /// the call site) is equivalent to the original inline rollback: nothing can have been emitted
    /// between a decline here and the truncate there.
    #[allow(clippy::type_complexity)]
    fn parse_option_append_arms<'a>(
        &self,
        arms: &'a [IrMatchArm],
        subj: ValueId,
    ) -> Option<((&'a IrExpr, Option<(VarId, bool)>), &'a IrExpr)> {
        // Parse the arms into (some_body, some_bind, none_body). A SCALAR payload binds a value COPY;
        // a HEAP payload (`pl: Value`) binds the slot-0 @12 handle as a BORROW, gated on the subject
        // being a tracked nested-ownership list (`heap_elem_lists`). A nested ctor / heap bind over a
        // non-nested-ownership subject declines.
        let mut some: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = self.heap_elem_list_or_scalar_bind(subj, inner.as_ref()).ok()?;
                    if some.is_some() {
                        return None;
                    }
                    some = Some((&arm.body, bind));
                }
                IrPattern::None | IrPattern::Wildcard => {
                    if none.is_some() {
                        return None;
                    }
                    none = Some(&arm.body);
                }
                _ => return None,
            }
        }
        match (some, none) {
            (Some(s), Some(n)) => Some((s, n)),
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_str_acc`] (codopsy round-3 sweep,
    /// #852) via [`Self::parse_option_append_arms`]: decides ONE `some(…)` payload's bind form — a
    /// scalar `Bind` is a value COPY, a heap `Bind` the @12 handle as a BORROW, a `Wildcard` binds
    /// nothing, any other pattern declines. NOT [`Self::heap_or_scalar_variant_bind`] (the
    /// result-list twin) — that one ALSO admits a heap bind over a `value_result_lists` /
    /// `value_result_results` subject, which the str-acc path never did; the gate here is
    /// `heap_elem_lists` ALONE, so the two must stay separate.
    fn heap_elem_list_or_scalar_bind(
        &self,
        subj: ValueId,
        inner: &IrPattern,
    ) -> Result<Option<(VarId, bool)>, ()> {
        match inner {
            IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
            IrPattern::Bind { var, ty }
                if is_heap_ty(ty) && self.value_drops.get(&subj).is_some_and(|d| d.flat_elems) =>
            {
                Ok(Some((*var, true)))
            }
            IrPattern::Wildcard => Ok(None),
            _ => Err(()),
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_str_acc`] AND
    /// [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep, #852) — the block was
    /// TEXTUALLY IDENTICAL in both (there a `bind_payload` closure): binds one arm's payload from
    /// the subject's slot-0 (@12), a SCALAR as a value COPY (load64), a HEAP element as a
    /// `LoadHandle` BORROW recorded in `param_values`. NOT [`Self::bind_variant_payload`]
    /// (control_p2.rs) — that sibling also takes the payload's `Ty` and `seed_variant_param`s a heap
    /// bind, which NEITHER call site here ever did; reusing it would add new behavior for a nested
    /// variant payload, not just move code.
    fn bind_variant_payload_from_slot0(&mut self, h: ValueId, bind: Option<(VarId, bool)>) {
        use crate::PrimKind;
        if let Some((bind_var, is_heap)) = bind {
            let payload = if is_heap {
                self.load_at_offset(h, 12, PrimKind::LoadHandle)
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(bind_var, payload);
            if is_heap {
                self.param_values.insert(payload);
            }
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_str_acc`] AND
    /// [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep, #852) — the block was
    /// TEXTUALLY IDENTICAL in both: takes the per-iteration subject out of `live_heap_handles` and
    /// emits its drop, closing the per-iteration `i…d` balance. Same body as `drop_owned_subject`
    /// (control_p2.rs), which is private to that module and so not callable from here; the reason
    /// each call site's own comment stays at the call site is that it justifies the drop POSITION
    /// (after both arms), which is a property of the caller, not of this move.
    fn drop_owned_subject_after_arms(&mut self, subj: ValueId) {
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
    }

    /// The HEAP-Ok Result SUBJECT drop-route classification for
    /// [`Self::append_variant_match_to_result_list`] — routes `subj`'s scope-end drop by the
    /// Ok payload's exact shape. NOT [`Self::track_heap_ok_result_subject_drop`] (control_p2.rs)
    /// — that sibling ALSO checks `result_ok_record_drop_fn` first (a RECORD-Ok `resrec:`
    /// route), which this call site's original inline chain never did; reusing it here would
    /// add new behavior for a record-Ok subject, not just flatten nesting. Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn track_heap_ok_result_subj_drop_no_record(&mut self, subj: ValueId, ty: &Ty) {
        if crate::lower::is_result_listval_ty(ty) {
            self.value_drops.entry(subj).or_default().value_result_list = true;
            return;
        }
        if crate::lower::is_value_result_ty(ty) {
            self.value_drops.entry(subj).or_default().value_result = true;
            return;
        }
        if crate::lower::is_str_int_result_ty(ty) {
            self.value_drops.entry(subj).or_default().str_int_result = true;
            return;
        }
        if crate::lower::is_value_int_result_ty(ty) {
            self.value_drops.entry(subj).or_default().value_int_result = true;
            return;
        }
        if crate::lower::is_list_str_int_result_ty(ty) {
            self.value_drops.entry(subj).or_default().list_str_int_result = true;
            return;
        }
        if crate::lower::is_list_value_int_result_ty(ty) {
            self.value_drops.entry(subj).or_default().list_value_int_result = true;
            return;
        }
        self.value_drops.entry(subj).or_default().flat_elems = true;
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852): records the freshly materialized subject in the tracking set(s) that decide its
    /// per-iteration drop route — a self-host or user `Named` Option, a scalar-Ok (len-as-tag)
    /// Result, or a heap-Ok (cap-as-tag) str-Result.
    fn track_variant_match_subject_drop_sets(&mut self, subject: &IrExpr, subj: ValueId) {
        // Track the subject EXACTLY as `try_lower_variant_value_match` (control_p2): a self-host or
        // user `Named` Option/Result, with the type-driven drop set so the per-iteration subject drop
        // frees its owned payload correctly. The arm tag arrangement is the uniform skeleton then=tag≠0
        // / else=tag==0 (Option → then=Some/else=None; Result → then=Err/else=Ok).
        let is_named_call =
            matches!(&subject.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. });
        self.track_subject_as_option(subject, subj, is_named_call);
        self.track_subject_as_scalar_ok_result(subject, subj, is_named_call);
        self.track_subject_as_heap_ok_result(subject, subj, is_named_call);
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852) via [`Self::track_variant_match_subject_drop_sets`]: the FIRST of the three sequential
    /// subject-shape tests — a self-host or user `Named` OPTION subject (len-as-tag @4), plus the
    /// nested-ownership `heap_elem_lists` route for a heap payload.
    fn track_subject_as_option(&mut self, subject: &IrExpr, subj: ValueId, is_named_call: bool) {
        if is_self_host_option_call(subject)
            || (is_named_call
                && is_variant_ty(&subject.ty)
                && !crate::lower::is_result_ty(&subject.ty))
        {
            self.value_shapes.insert(subj, crate::lower::VariantShape::Option);
            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                self.value_drops.entry(subj).or_default().flat_elems = true;
            }
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852) via [`Self::track_variant_match_subject_drop_sets`]: the SECOND of the three sequential
    /// subject-shape tests — a self-host or user `Named` scalar-Ok RESULT subject (len-as-tag @4).
    fn track_subject_as_scalar_ok_result(
        &mut self,
        subject: &IrExpr,
        subj: ValueId,
        is_named_call: bool,
    ) {
        if is_self_host_result_call(subject)
            || (is_named_call
                && crate::lower::is_result_ty(&subject.ty)
                && !Self::is_heap_ok_result(&subject.ty))
        {
            self.value_shapes.insert(subj, crate::lower::VariantShape::ResultScalar);
            // Scalar-Ok / heap-Err `Result[Int, String]` (the byte-match fixture's `mkResult`): the
            // len-as-tag read stays @4, but track heap_elem_lists so the Err arm's String payload drops
            // via DropListStr (Ok=len0 frees nothing, Err=len1 frees slot-0's String).
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) =
                &subject.ty
            {
                if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    self.value_drops.entry(subj).or_default().flat_elems = true;
                }
            }
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852) via [`Self::track_variant_match_subject_drop_sets`]: the THIRD of the three sequential
    /// subject-shape tests — a self-host or user `Named` heap-Ok str-RESULT subject (cap-as-tag @16),
    /// whose exact drop route is then picked by
    /// [`Self::track_heap_ok_result_subj_drop_no_record`].
    fn track_subject_as_heap_ok_result(
        &mut self,
        subject: &IrExpr,
        subj: ValueId,
        is_named_call: bool,
    ) {
        if is_self_host_result_str_call(subject)
            || (is_named_call && Self::is_heap_ok_result(&subject.ty))
        {
            self.value_shapes.insert(subj, crate::lower::VariantShape::ResultHeapOk);
            self.track_heap_ok_result_subj_drop_no_record(subj, &subject.ty);
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`]'s `heap_or_scalar_bind`
    /// closure (codopsy round-3 sweep, #852): decides ONE variant payload's bind form — a scalar
    /// `Bind` is a value COPY, a heap `Bind` the @12 handle as a BORROW (admitted only over a subject
    /// tracked as nested-ownership), a `Wildcard` binds nothing, any other pattern declines.
    fn heap_or_scalar_variant_bind(
        &self,
        subj: ValueId,
        inner: &IrPattern,
    ) -> Result<Option<(VarId, bool)>, ()> {
        match inner {
            IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
            IrPattern::Bind { var, ty }
                if is_heap_ty(ty)
                    && (self.value_drops.get(&subj).is_some_and(|d| d.flat_elems)
                        || self.value_drops.get(&subj).is_some_and(|d| d.value_result_list)
                        || self.value_drops.get(&subj).is_some_and(|d| d.value_result)) =>
            {
                Ok(Some((*var, true)))
            }
            IrPattern::Wildcard => Ok(None),
            _ => Err(()),
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852): parses the two arms into the uniform then/else skeleton. Takes `&self` — it emits NO
    /// ops, which is exactly why the caller's `rollback` (unchanged, now at the call site) is
    /// equivalent to the original inline `return rollback(self)`: nothing can have been emitted
    /// between a decline here and the truncate there.
    #[allow(clippy::type_complexity)]
    fn parse_keep_skip_arms<'a>(
        &self,
        arms: &'a [IrMatchArm],
        subj: ValueId,
        is_option: bool,
    ) -> Option<((&'a IrExpr, Option<(VarId, bool)>), (&'a IrExpr, Option<(VarId, bool)>))> {
        // Parse the arms into (then_body, then_bind) [tag != 0] and (else_body, else_bind) [tag == 0],
        // the uniform skeleton: Option → then=Some / else=None; Result → then=Err / else=Ok. A heap
        // payload binds the @12 handle as a BORROW (gated on the subject being a nested-ownership list);
        // a scalar payload a value copy; a wildcard nothing.
        let mut then_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut else_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            let parsed: Result<(bool, Option<(VarId, bool)>), ()> =
                self.keep_skip_arm_side_and_bind(subj, is_option, &arm.pattern);
            match parsed {
                Ok((true, bind)) if then_slot.is_none() => then_slot = Some((&arm.body, bind)),
                Ok((false, bind)) if else_slot.is_none() => else_slot = Some((&arm.body, bind)),
                _ => return None,
            }
        }
        match (then_slot, else_slot) {
            (Some(t), Some(e)) => Some((t, e)),
            _ => None,
        }
    }

    /// Extracted verbatim from [`Self::append_variant_match_to_result_list`] (codopsy round-3 sweep,
    /// #852) via [`Self::parse_keep_skip_arms`]: decides which SIDE of the uniform tag skeleton ONE
    /// arm belongs to (`true` = then / tag != 0, `false` = else / tag == 0) together with its payload
    /// bind — Option `some`/`none` vs Result `err`/`ok`, the INVERSE tag pairing. Declines (`Err`) on
    /// any pattern outside those four.
    fn keep_skip_arm_side_and_bind(
        &self,
        subj: ValueId,
        is_option: bool,
        pattern: &IrPattern,
    ) -> Result<(bool, Option<(VarId, bool)>), ()> {
        match pattern {
            IrPattern::Some { inner } if is_option => {
                self.heap_or_scalar_variant_bind(subj, inner).map(|b| (true, b))
            }
            IrPattern::None | IrPattern::Wildcard if is_option => Ok((false, None)),
            IrPattern::Err { inner } if !is_option => {
                self.heap_or_scalar_variant_bind(subj, inner).map(|b| (true, b))
            }
            IrPattern::Ok { inner } if !is_option => {
                self.heap_or_scalar_variant_bind(subj, inner).map(|b| (false, b))
            }
            _ => Err(()),
        }
    }

    /// A `filter_map` closure body that is a 2-arm VARIANT `match subj { … }` deciding keep/skip,
    /// lowered into a WRITE-CURSOR result list (`lower_defunc_filter_map_hof`). The subject is a
    /// self-host Option CALL (`some(pl)`/`none`) OR a self-host Result(-str) CALL (`ok(pl)`/`err(_)`)
    /// — the dojo `match fs.read_text(dir+"/"+f) { ok(content) => some(parse_task_md(f, content)),
    /// err(_) => none }`. Mirrors `append_variant_match_to_str_acc` (UNIT control, per-arm action,
    /// branch isolation, drop-subject-after) BUT (a) ADMITS Result `ok`/`err` arms with the INVERSE
    /// tag (Result Ok = tag==0 vs Option Some = tag!=0) exactly as `try_lower_variant_value_match`
    /// already does (control_p2), and (b) the keep arm stores an OWNED record/Value at the cursor
    /// instead of appending a String. Returns `Some(())` on success, `None` (the caller rolls back +
    /// WALLs) outside the subset.
    ///
    /// SOUNDNESS — per-iteration subject + borrowed payload, exactly the Option path:
    ///  - The subject (`fs.read_text(…)`) is materialized into a FRESH OWNED block (cert `i`) INSIDE
    ///    this iteration's frame, tracked so its post-arm drop frees the owned payload recursively. A
    ///    str-Result (cap-as-tag @16) is `materialized_results_str` + `heap_elem_lists` (DropListStr
    ///    frees slot-0's String); a scalar Result (len-as-tag @4) is `materialized_results`; an Option
    ///    (len-as-tag @4) is `materialized_options` (+ `heap_elem_lists` for a heap payload).
    ///  - A `some(pl)`/`ok(pl)` HEAP payload binds `pl` to the subject's slot-0 @12 handle as a BORROW
    ///    (`param_values`) — the subject still owns it, freed once by its post-arm drop. The keep arm
    ///    builds a FRESH OWNED element (`lower_heap_result_arm`), so no double-free.
    ///  - The subject must stay live THROUGH the keep arm (the borrow is read there) → dropped AFTER
    ///    both arms (cert `d`), closing the per-iteration `i…d` balance.
    ///  - BRANCH OWNERSHIP ISOLATION around the then-arm (snapshot/restore param_values +
    ///    live_heap_handles + materialized_aggregates), so a consume in one alternate arm does not
    ///    leak into the other's lowering view.
    fn append_variant_match_to_result_list(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        out: ResultList<'_>,
    ) -> Option<()> {
        let ResultList { handle: rh, cursor, elem: result_elem, elem_size: eight } = out;
        use crate::PrimKind;
        // Gate: a 2-arm guard-free match over a heap (variant) subject. The subject must materialize to
        // a self-host Option/Result(-str) CALL or a USER `Named` call returning Option/Result (NOT a
        // let-bound Var — only a Call subject passes the tracking below, mirroring
        // `try_lower_variant_value_match`). A custom variant / non-variant subject rolls back at the
        // `is_option/is_result` check.
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) || !is_heap_ty(&subject.ty) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // Materialize the per-element subject into a FRESH OWNED block (cert `i`), dropped AFTER the
        // arms (cert `d`) within THIS iteration.
        let subj = match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => return rollback(self),
        };
        self.track_variant_match_subject_drop_sets(subject, subj);
        let is_option = self.value_shapes.get(&subj) == Some(&crate::lower::VariantShape::Option);
        let is_result_str = self.value_shapes.get(&subj) == Some(&crate::lower::VariantShape::ResultHeapOk);
        let is_result = self.value_shapes.get(&subj) == Some(&crate::lower::VariantShape::ResultScalar) || is_result_str;
        if !is_option && !is_result {
            return rollback(self);
        }
        let tag_off = if is_result_str { 16 } else { 4 };
        let ((then_body, then_bind), (else_body, else_bind)) =
            match self.parse_keep_skip_arms(arms, subj, is_option) {
                Some(parsed) => parsed,
                None => return rollback(self),
            };
        // tag = load32(handle(subj) + tag_off); bind payload(s) BEFORE the IfThen (in scope for the
        // arm that reads them); then a UNIT IfThen (dst None) with per-arm keep/skip.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        self.bind_variant_payload_from_slot0(h, then_bind);
        self.bind_variant_payload_from_slot0(h, else_bind);
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        self.unit_arm_depth += 1;
        let then_ok = self.emit_filter_map_arm(then_body, rh, cursor, result_elem, eight);
        self.ops.push(Op::Else { val: None });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot;
        self.materialized_aggregates = ma_snapshot;
        let else_ok =
            then_ok.and_then(|_| self.emit_filter_map_arm(else_body, rh, cursor, result_elem, eight));
        self.unit_arm_depth -= 1;
        self.ops.push(Op::EndIf { val: None });
        if else_ok.is_none() {
            return rollback(self);
        }
        // SUBJECT-DROP-AFTER-ARMS: the keep arm borrowed slot-0, so the fresh per-iteration subject
        // stayed live through both arms — drop it ONCE here, closing the per-iteration `i…d` balance.
        self.drop_owned_subject_after_arms(subj);
        Some(())
    }

    /// One arm of a `filter_map` keep/skip variant match (`append_variant_match_to_result_list`):
    ///   - `none` / `[]` (empty) → SKIP (no store).
    ///   - `some(<elem>)` → KEEP: build `<elem>` as a FRESH OWNED record/Value (`lower_heap_result_arm`,
    ///     which Consumes it = moved out of the iteration scope), store its handle at `result[cursor*8]`,
    ///     then `cursor += 1`. The element is already owned (rc 1) → just store, NO `Dup` (unlike
    ///     `filter`, which keeps a BORROWED source element).
    /// A `e!` wrapper is stripped (effect-fn error propagation is identity on its inner value here).
    /// Any other body shape returns `None` → the caller rolls back + WALLs.
    fn emit_filter_map_arm(
        &mut self,
        body: &IrExpr,
        rh: ValueId,
        cursor: ValueId,
        result_elem: &Ty,
        eight: ValueId,
    ) -> Option<()> {
        use crate::PrimKind;
        let body = match &body.kind {
            IrExprKind::Unwrap { expr } => expr.as_ref(),
            _ => body,
        };
        match &body.kind {
            IrExprKind::OptionNone => Some(()),
            IrExprKind::List { elements } if elements.is_empty() => Some(()),
            // A BLOCK arm body (`none => { let obj = …; let b = …; if b then some(e) else none }` —
            // porta load_porta_config's secrets `none`-arm): lower the leading lets as per-arm effects
            // (their captures resolve via value_of; their heap temps freed at the arm frame end), then
            // recurse on the tail. Mirrors `append_body_to_str_acc`'s Block case for the str-acc path.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let arm_mark = self.live_heap_handles.len();
                for s in stmts {
                    self.lower_stmt(s).ok()?;
                }
                let r = self.emit_filter_map_arm(tail, rh, cursor, result_elem, eight);
                self.drop_arm_locals(arm_mark);
                r
            }
            // A CONDITIONAL arm body (`if from_env then some(e2) else none` — the secrets none-arm's
            // keep/skip decision): a UNIT control structure, only the taken arm runs. Lower the cond to a
            // scalar bool, then recurse each arm as an append/skip into the SAME result-list cursor (the
            // cursor's `SetLocal` increment is in-place under `unit_arm_depth`). No merged heap value —
            // the record is built+stored INSIDE the taken arm. Mirrors `append_body_to_str_acc`'s If case.
            IrExprKind::If { cond, then, else_ } => {
                let cond_v = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: cond_v, dst: None });
                self.unit_arm_depth += 1;
                let then_ok = self.emit_filter_map_arm(then, rh, cursor, result_elem, eight);
                self.ops.push(Op::Else { val: None });
                let else_ok =
                    then_ok.and_then(|_| self.emit_filter_map_arm(else_, rh, cursor, result_elem, eight));
                self.unit_arm_depth -= 1;
                self.ops.push(Op::EndIf { val: None });
                else_ok
            }
            IrExprKind::OptionSome { expr } => {
                let arm_mark = self.live_heap_handles.len();
                let elem_v = self.lower_heap_result_arm(expr, result_elem)?;
                // store the OWNED element handle at result[cursor*8].
                let c8 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: c8, op: IntOp::Mul, a: cursor, b: eight });
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: c8 });
                let eh = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![elem_v] });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![raddr, eh],
                });
                // cursor += 1.
                let one = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: one, value: 1 });
                let cnext = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: cnext, op: IntOp::Add, a: cursor, b: one });
                self.ops.push(Op::SetLocal { local: cursor, src: cnext });
                self.drop_arm_locals(arm_mark);
                Some(())
            }
            _ => None,
        }
    }
}
