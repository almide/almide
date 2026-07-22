impl LowerCtx {
    /// The nested-Option/Result-payload seeding for [`Self::try_lower_result_match`]'s Ok-arm
    /// bind (`ok(m) => …` where `m: Option[_]`/`Result[_, _]` — a NESTED variant payload):
    /// tracks `payload`'s READ-shape so an inner `match` EXECUTES instead of falling to the
    /// both-arms linearization. NOT [`Self::seed_nested_option_bind_payload`] — that sibling
    /// (used by `try_lower_unit_if`'s Some-bind) ALSO seeds a RECORD/TUPLE aggregate payload,
    /// which this call site's original inline code never did; reusing it here would add new
    /// behavior for a record-payload Ok-bind, not just flatten nesting. Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn seed_nested_option_result_bind_payload(&mut self, payload: ValueId, bind_ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        if matches!(bind_ty, Ty::Applied(TypeConstructorId::Option, _)) {
            self.materialized_options.insert(payload);
            if crate::lower::is_lenlist_list_ty(bind_ty) {
                self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
            } else if crate::lower::is_heap_elem_list_ty(bind_ty) {
                self.heap_elem_lists.insert(payload);
            }
            return;
        }
        if crate::lower::is_result_ty(bind_ty) {
            self.materialized_results.insert(payload);
            if crate::lower::is_lenlist_list_ty(bind_ty) {
                self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
            } else if crate::lower::is_heap_elem_list_ty(bind_ty) {
                self.heap_elem_lists.insert(payload);
            }
        }
    }

    /// EXECUTE a `match r { Ok(v) => …, Err(e) => … }` over a MATERIALIZED Result — only the taken
    /// arm runs. The Result analogue of [`Self::try_lower_variant_match`], reusing the same
    /// per-arm-balanced cert: the markers no-op in `verify_ownership`, each arm is a per-arm frame,
    /// the tag/payload reads are scalar prims. The len-as-tag is INVERSE of Option: `len == 0` = Ok
    /// (the value is a scalar slot-0 COPY, load64), `len != 0` = Err (the message is a borrowed
    /// `LoadHandle` of slot 0 — the Result owns it, freed by the scope-end DropListStr, so the bound
    /// var is not a second owner). SOUNDNESS — gated on `subject ∈ materialized_results`: only a
    /// known DynListStr-Result is read len-as-tag; any other (deferred `Opaque`, len 0) would
    /// MISREAD as Ok, so it is not in the set and keeps the sound LINEARIZED match.
    pub(crate) fn try_lower_result_match(
        &mut self,
        subject_value: Option<ValueId>,
        arms: &[IrMatchArm],
    ) -> bool {
        use crate::PrimKind;
        // A HEAP-Ok `Result[String, String]` (cap-as-tag, Ok binds a heap String) vs the scalar
        // `Result[Int, String]` (len-as-tag, Ok binds a scalar int).
        let (subj, str_result) = match subject_value {
            Some(v) if self.materialized_results_str.contains(&v) => (v, true),
            Some(v) if self.materialized_results.contains(&v) => (v, false),
            _ => return false,
        };
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return false;
        }
        // Exactly [Ok(scalar-bind?), Err(heap-bind?)], no nested ctors, Unit bodies. An Ok binds a
        // SCALAR Int (value copy); an Err binds a heap String (borrowed slot-0 handle), gated to a
        // nested-ownership subject (so the Result keeps ownership through its DropListStr).
        let mut ok: Option<(&IrExpr, Option<(VarId, Ty)>)> = None;
        let mut err: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = match inner.as_ref() {
                        // Scalar Ok (Result[Int,String]) binds a scalar int; a heap-Ok
                        // (Result[String,String]) binds a heap String — gated to `str_result`.
                        IrPattern::Bind { var, ty } if is_heap_ty(ty) == str_result => Some((*var, ty.clone())),
                        IrPattern::Wildcard => None,
                        _ => return false,
                    };
                    if ok.is_some() {
                        return false;
                    }
                    ok = Some((&arm.body, bind));
                }
                IrPattern::Err { inner } => {
                    let bind = match inner.as_ref() {
                        // `heap_elem_lists` covers the flat-drop str-results (`value.as_string`);
                        // `value_result_lists`/`value_result_results` are the RECURSIVE-drop
                        // twins (`Result[List[Value],String]` / `Result[Value,String]` —
                        // `seed_variant_param` routes these there instead, since their Ok
                        // payload needs `DropResultListValue`/`DropResultValue`, not the flat
                        // `DropListStr` a String-Ok gets). Mirrors `try_lower_variant_value_
                        // match`'s `heap_or_scalar_bind` (~line 463-479), the value-position
                        // twin of this statement-position match — WITHOUT this the Err-bind
                        // here is strictly narrower than its twin, so a `Result[Value,String]`
                        // subject (json_path_edges' `p_set`) falls through to the untracked-
                        // subject both-arms-linearization wall even though the twin would
                        // admit it.
                        // A RICH-VARIANT Err payload needing a recursive drop (`Result[Int,
                        // MathError]`, `err(Overflow(msg))` — bidirectional_type_test's structured
                        // error): `try_lower_result_err_variant_ctor` tracks such a subject via
                        // `variant_drop_handles = "res_<V>"` (a GENERATED `$__drop_res_<V>`,
                        // drop_sources.rs), NOT `heap_elem_lists` (explicitly removed there once
                        // `needs_rec` is true) — so this Bind guard, unlike its value-position twin
                        // (`try_lower_variant_value_match`'s `heap_or_scalar_bind`, which already
                        // admits `resrec:`/`optrec:`), had no matching case at all.
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty)
                                && (self.heap_elem_lists.contains(&subj)
                                    || self.value_result_lists.contains(&subj)
                                    || self.value_result_results.contains(&subj)
                                    || self
                                        .variant_drop_handles
                                        .get(&subj)
                                        .is_some_and(|h| h.starts_with("res_"))) =>
                        {
                            Some((*var, true))
                        }
                        IrPattern::Wildcard => None,
                        _ => return false,
                    };
                    if err.is_some() {
                        return false;
                    }
                    err = Some((&arm.body, bind));
                }
                // A TOP-LEVEL `_` catch-all as the non-Ok arm (`match r { ok($q) => …, _ => … }`
                // — the regrouped codec-roundtrip shape): tag != 0 ⇒ not-Ok ⇒ the wildcard body,
                // binding nothing. Positionally identical to `err(_)` once Ok holds the other arm.
                IrPattern::Wildcard => {
                    if err.is_some() {
                        return false;
                    }
                    err = Some((&arm.body, Option::None));
                }
                _ => return false,
            }
        }
        let ((ok_body, ok_bind), (err_body, err_bind)) = match (ok, err) {
            (Some(o), Some(e)) => (o, e),
            _ => return false,
        };
        if !matches!(ok_body.ty, Ty::Unit) || !matches!(err_body.ty, Ty::Unit) {
            return false;
        }
        // tag = load32(handle(subj) + 4); if tag != 0 then Err-arm else Ok-arm (len 0 = Ok).
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        // The tag is the HIGH 32 bits of slot 0 (@16) for a heap-Ok Result (the low 32 bits @12 hold
        // the owned String handle), `len` (@4) for a scalar one.
        let tag_off = if str_result { 16 } else { 4 };
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        // THEN (tag != 0 = Err): the message is the BORROWED slot-0 handle.
        if let Some((bind_var, _)) = err_bind {
            let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            self.value_of.insert(bind_var, payload);
            self.param_values.insert(payload);
        }
        // Exactly ONE arm runs (the unit-if discipline): outer-var reassignments
        // inside an arm mutate the stable local IN PLACE — see `unit_arm_depth`.
        self.unit_arm_depth += 1;
        if self.lower_branch_arm(None, err_body).is_err() {
            self.unit_arm_depth -= 1;
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::Else { val: None });
        // ELSE (tag == 0 = Ok): a scalar Result yields the slot-0 int COPY; a heap-Ok Result yields
        // the BORROWED slot-0 String handle (the Result keeps ownership through its DropListStr).
        if let Some((bind_var, bind_ty)) = ok_bind {
            if str_result {
                let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.value_of.insert(bind_var, payload);
                self.param_values.insert(payload);
                // NESTED VARIANT PAYLOAD: `ok(m)` where m is itself an
                // Option/Result (`Result[Option[record], String]` — porta
                // read_message's monadic-desugar Ok arm holding `match m
                // { some(req)/none }`). Track the BORROWED payload like the
                // Some-bind path above does, so the INNER match BRANCHES on
                // its tag instead of hitting the (walled) linearization.
                self.seed_nested_option_result_bind_payload(payload, &bind_ty);
            } else {
                let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
                self.value_of.insert(bind_var, payload);
            }
        }
        let ok_ok = self.lower_branch_arm(None, ok_body).is_ok();
        self.unit_arm_depth -= 1;
        if !ok_ok {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return false;
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// VALUE-position variant match: a `match opt { Some(x) => <scalar>, None => <scalar> }`
    /// (or `Ok/Err`) used as an OPERAND / let / call-argument EXECUTES to a SCALAR `dst` —
    /// read the tag, run ONLY the taken arm, bind the scalar payload. The value analogue of
    /// [`Self::try_lower_variant_match`] / [`Self::try_lower_result_match`] (which require
    /// UNIT arms): without it a ctor-pattern value match desugared to nothing (a `Some`/`Ok`
    /// pattern is not `subj == lit`) and the result local stayed UNSET = a silent 0.
    /// Returns `None` (rolled back) outside the subset — the caller then WALLs (a Const-0
    /// would silently pick a wrong arm).
    ///
    /// SOUNDNESS — the subject is materialized/borrowed by `lower_call_args` (an owned ctor
    /// temp drops at scope end via `live_heap_handles`; a tracked Var borrows), gated on
    /// `∈ materialized_options/results` so the len-as-tag read is only over a value KNOWN to
    /// carry the layout (`Some`=len1 / `None`=len0; scalar `Ok`=len0 / `Err`=len≠0). The
    /// tag/payload reads are scalar prims, the `IfThen`/`Else`/`EndIf` markers no-op in
    /// `verify_ownership`, and each arm is a scalar value with NO heap ownership event —
    /// exactly the per-arm-balanced linearization the cert already proves, wrapped so one
    /// arm runs. The enclosing `lower_scalar_value` self-rollback restores `ops` +
    /// `live_heap_handles` on a miss, so the subject materialize is rollback-safe. SCALAR
    /// payload + SCALAR result only (a heap-result variant match merges heap arms — later).
    pub(crate) fn try_lower_variant_value_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // SCALAR result, OR a HEAP result over a SCALAR-PAYLOAD variant via the
        // SUBJECT-DROP-BEFORE-ARMS desugar (below): copy the scalar tag/payload, DROP the
        // owned subject BEFORE the arms, then run the proven heap-result-`if` (scalar cond) —
        // so the arm's per-arm heap move-out no longer overlaps the owned-subject borrow the
        // checker rejected. A HEAP-PAYLOAD variant (`Option[String]` — the arm borrows the
        // subject's slot, no scalar copy possible) stays the true Camp-4 frontier and is
        // gated out below.
        if !is_heap_ty(&subject.ty)
            || arms.len() != 2
            || arms.iter().any(|a| a.guard.is_some())
        {
            return None;
        }
        // DESUGAR a tuple Some/Ok payload — `some((idx, line)) => B` → `some($p) => { let (idx,line) = $p; B }`.
        // The single var `$p` is bound below via the HEAP-payload path (into `param_values`), so the
        // `let (idx,line) = $p` tuple destructure then lowers (`try_lower_tuple_destructure` borrows each
        // slot). A raw tuple VAR/param destructure alone walls (no `param_values` entry), so the rewrite to
        // the @12-handle bind is required, not a plain var destructure. `$p` ids start above subject+arms.
        let has_tuple_payload = arms.iter().any(|a| {
            matches!(&a.pattern, IrPattern::Some { inner } | IrPattern::Ok { inner }
                if matches!(&**inner, IrPattern::Tuple { .. }))
        });
        let desugared: Vec<IrMatchArm>;
        let arms: &[IrMatchArm] = if has_tuple_payload {
            let mut next = arms
                .iter()
                .map(|a| crate::lower::max_var_id(&a.body))
                .max()
                .unwrap_or(0)
                .max(crate::lower::max_var_id(subject))
                + 1;
            let mut out: Vec<IrMatchArm> = Vec::with_capacity(arms.len());
            for a in arms {
                let inner_tuple = match &a.pattern {
                    IrPattern::Some { inner } | IrPattern::Ok { inner } => match &**inner {
                        IrPattern::Tuple { elements } => Some(elements.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(elements) = inner_tuple else {
                    out.push(a.clone());
                    continue;
                };
                let p = VarId(next);
                next += 1;
                let tuple_ty = Ty::Tuple(
                    elements
                        .iter()
                        .map(|e| match e {
                            IrPattern::Bind { ty, .. } => ty.clone(),
                            _ => Ty::Unknown,
                        })
                        .collect(),
                );
                let p_inner = Box::new(IrPattern::Bind { var: p, ty: tuple_ty.clone() });
                let new_pat = match &a.pattern {
                    IrPattern::Some { .. } => IrPattern::Some { inner: p_inner },
                    _ => IrPattern::Ok { inner: p_inner },
                };
                let destr = IrStmt {
                    kind: IrStmtKind::BindDestructure {
                        pattern: IrPattern::Tuple { elements },
                        value: IrExpr {
                            kind: IrExprKind::Var { id: p },
                            ty: tuple_ty,
                            span: None,
                            def_id: None,
                        },
                    },
                    span: None,
                };
                let body = IrExpr {
                    kind: IrExprKind::Block {
                        stmts: vec![destr],
                        expr: Some(Box::new(a.body.clone())),
                    },
                    ty: a.body.ty.clone(),
                    span: a.body.span.clone(),
                    def_id: a.body.def_id,
                };
                out.push(IrMatchArm { pattern: new_pat, guard: a.guard.clone(), body });
            }
            desugared = out;
            &desugared
        } else {
            arms
        };
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.lifted.truncate(lifted_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // Decomposed (#781, cog 129 → phases): the SUBJECT resolution + tracking
        // classification (~185 lines) is a verbatim text move into
        // `variant_match_subject` — its `None` performs the same mark rollback.
        let (subj, is_option, is_result_str, is_result) =
            self.variant_match_subject(subject, ops_mark, lhh_mark)?;
        // Parse the two arms into (then_body, then_bind, else_body, else_bind) where a bind is
        // an optional SCALAR payload var (`Some(x)` / `Ok(x)` / a scalar `Err(c)`). A heap bind
        // (`Err(msg: String)`) is allowed only when the arm body never needs it as an owner —
        // here it is bound as a BORROW of the Result's owned slot-0 handle, gated on the subject
        // being a nested-ownership list (it frees the payload at scope end). A wildcard binds nothing.
        let heap_or_scalar_bind = |s: &Self, inner: &IrPattern| -> Result<Option<(VarId, bool, Ty)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false, ty.clone()))),
                // A heap payload bind is admitted over a nested-ownership subject — a str-result
                // (`heap_elem_lists`, the `value.as_string` String payload) OR a value-array result
                // (`value_result_lists`, the `value.as_array` `List[Value]` payload, e.g. `ok(items)
                // => emit_seq(items)`). Both bind the @12 handle as a BORROW (drop-subject-after).
                IrPattern::Bind { var, ty }
                    if is_heap_ty(ty)
                        && (s.heap_elem_lists.contains(&subj)
                            // `Option[List[String]]` (the heap-acc fold value) — routed to the
                            // nested DropListListStr set; the payload bind discipline is identical
                            // (a borrowed @12 handle, the subject's own recursive drop frees it).
                            || s.list_list_str_lists.contains(&subj)
                            || s.value_result_lists.contains(&subj)
                            || s.value_result_results.contains(&subj)
                            // A record-Ok `Result[<record>, String]` subject (`resrec:<R>` drop
                            // handle): the `ok(m: record)` payload (AND the `err(e: String)` slot)
                            // binds the @12 handle as a BORROW; the subject's recursive
                            // `DropWrapperRec` frees the live block (record or Err String) once
                            // after the arms. A bare-Var move-out arm auto-`Dup`s, so no double-free.
                            // An option-of-variant subject (`optrec:<T>`, `some(Number(7))`):
                            // the Some-arm payload binds the @12 variant handle as a BORROW;
                            // the subject's recursive drop frees the payload once after the
                            // arms — the same resrec discipline.
                            || s.variant_drop_handles
                                .get(&subj)
                                .is_some_and(|h| {
                                    h.starts_with("resrec:")
                                        || h.starts_with("optrec:")
                                        // A rich-variant Err payload needing recursive drop
                                        // (`res_<V>`, `try_lower_result_err_variant_ctor`) — the
                                        // Err bind is a BORROW of the @12 variant handle, freed
                                        // by the subject's own `$__drop_res_<V>` at scope end
                                        // (mirrors the statement-position twin, `try_lower_
                                        // result_match`).
                                        || h.starts_with("res_")
                                })) =>
                {
                    Ok(Some((*var, true, ty.clone())))
                }
                IrPattern::Wildcard => Ok(None),
                _ => Err(()),
            }
        };
        let mut then_slot: Option<(&IrExpr, Option<(VarId, bool, Ty)>)> = None;
        let mut else_slot: Option<(&IrExpr, Option<(VarId, bool, Ty)>)> = None;
        for arm in arms {
            let parsed: Result<(bool, Option<(VarId, bool, Ty)>), ()> = match &arm.pattern {
                // Option Some (then) / None (else). Use heap_or_scalar_bind so a HEAP Some-payload
                // (`some(key)` where key: String/Value/Tuple — toml set_nested's `match list.first(path)`)
                // binds the @12 handle as a BORROW, gated on the Option[heap] subject being tracked
                // nested-ownership (heap_elem_lists, set at tracking time). A scalar payload still binds
                // a copy. Without this only a Tuple Some-payload lowered (scalar_bind's narrow branch).
                IrPattern::Some { inner } if is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::None if is_option => Ok((false, None)),
                // A Wildcard takes the Option else-side ONLY when the subject is not ALSO
                // result-tracked: a Result Err CTOR bind reuses the Some(string) machinery
                // (materialize_opt_str_some inserts materialized_options), so both flags are
                // true — Result semantics must win (the flexible-side arm below).
                IrPattern::Wildcard if is_option && !is_result => Ok((false, None)),
                // Result Err (then) / Ok (else). BOTH use `heap_or_scalar_bind`: a scalar Result
                // binds a scalar payload, a str-result (`value.as_string`) binds its slot-0 String
                // as a BORROW (gated on `heap_elem_lists` — only a nested-ownership subject, so a
                // scalar Result still rejects a heap bind). The Ok side carries the str-result's
                // String payload (`ok(s) => emit_scalar(s)`), the very thing `emit` needs.
                IrPattern::Err { inner } if is_result => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::Ok { inner } if is_result => {
                    heap_or_scalar_bind(self, inner).map(|b| (false, b))
                }
                // A WILDCARD arm over a RESULT subject (`if let v = x { A } else { B }` —
                // the frontend's if-let desugar emits `Ok(v) => A, _ => B`): it takes
                // whichever side the ctor arm did NOT (then=Err when Ok is filled, else=Ok
                // when Err is filled). A wildcard BEFORE any ctor arm is ambiguous → reject.
                IrPattern::Wildcard
                    if is_result && (then_slot.is_some() != else_slot.is_some()) =>
                {
                    Ok((then_slot.is_none(), None))
                }
                _ => Err(()),
            };
            match parsed {
                Ok((true, bind)) if then_slot.is_none() => then_slot = Some((&arm.body, bind)),
                Ok((false, bind)) if else_slot.is_none() => else_slot = Some((&arm.body, bind)),
                _ => return rollback(self),
            }
        }
        let ((then_body, then_bind), (else_body, else_bind)) = match (then_slot, else_slot) {
            (Some(t), Some(e)) => (t, e),
            _ => return rollback(self),
        };
        let heap_res = is_heap_ty(result_ty);
        let has_heap_bind =
            matches!(then_bind, Some((_, true, _))) || matches!(else_bind, Some((_, true, _)));
        // A HEAP result with a HEAP-PAYLOAD bind is admitted ONLY over a str-result
        // (`value.as_string` — slot-0 @12 owns the ONE String, the Ok/Err tag at @16). The
        // payload binds as a BORROW (`LoadHandle` @12, in `param_values`), the OWNED subject is
        // dropped AFTER the arms (not before) so the borrow is live through them, and a bare-Var
        // arm (`ok(s) => s`) auto-acquires (`Op::Dup`) — so the drop-after frees the subject's
        // slot-0 String exactly once whether an arm borrows it (a call arg) or returns it. The
        // `emit` shape (`match value.as_string(v) { ok(s) => emit_scalar(s), err(_) => … }`) is
        // exactly this. A NON-str heap payload (a heap-Result-of-list, an Array element) has no
        // single-slot borrow rep yet — the true Camp-4 frontier — so it still defers.
        let str_heap_bind = heap_res && has_heap_bind && is_result_str;
        // The Option-tuple payload (`some((idx,line))`): a heap bind over an OPTION subject is always
        // the desugared tuple-handle borrow (scalar_bind only returns heap for a `Ty::Tuple`). It is
        // handled exactly like `str_heap_bind` — borrow @12, subject drops AFTER the arms — but reads
        // the Option len-as-tag @4 (not the str-result cap-tag @16).
        let opt_tuple_bind = heap_res && has_heap_bind && is_option;
        // Camp-4 sub-case 1: a SCALAR-Ok / HEAP-Err `Result[Int, String]` (the unwrap-`!`-desugar's
        // `err($x) => err($x)`). It reads the len-as-tag @4 (a scalar result, NOT the str-result @16)
        // but binds the Err arm's slot-0 String @12 as a BORROW — admitted because we marked it
        // `heap_elem_lists` at tracking time (so `DropListStr` frees slot-0 when Err=len1). The Err
        // arm's move-out auto-`Dup`s in lower_heap_result_arm, drop-subject-AFTER frees it once.
        let result_heap_err_bind =
            heap_res && has_heap_bind && is_result && !is_result_str && self.heap_elem_lists.contains(&subj);
        if heap_res && has_heap_bind && !is_result_str && !opt_tuple_bind && !result_heap_err_bind {
            return rollback(self);
        }
        // Emit: h = handle(subj); tag = load32(h + off); dst = if tag != 0 then <then> else <else>.
        // A scalar Option/Result reads len-as-tag (@4); a heap-Ok `Result[String,String]`
        // (value.as_string) reads the cap-as-tag at the slot-0 HIGH 32 bits (@16).
        let tag_off = if is_result_str { 16 } else { 4 };
        let dst = self.fresh_value();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        // Bind the scalar payload(s) as subj-independent COPIES (load64 @12) BEFORE the arms —
        // for the heap-result case this is what severs the arm's heap move-out from the subject.
        let bind_payload = |s: &mut Self, bind: Option<(VarId, bool, Ty)>| {
            if let Some((bind_var, is_heap, bind_ty)) = bind {
                let payload = if is_heap {
                    s.load_at_offset(h, 12, PrimKind::LoadHandle)
                } else {
                    s.load_at_offset(h, 12, PrimKind::Load { width: 8 })
                };
                s.value_of.insert(bind_var, payload);
                if is_heap {
                    s.param_values.insert(payload);
                    // NESTED VARIANT PAYLOAD: `ok(m)` / `some(m)` where m is itself an Option/Result
                    // (`Result[Option[record], String]` — the `read_message()!` monadic-desugar Ok arm
                    // holding porta's `match m { some(req)/none }`; `Option[Result[String,String]]` — the
                    // nested-compound interp). SEED the BORROWED payload's READ-shape via the canonical
                    // seeder so the INNER match BRANCHES on its tag. Using `seed_variant_param` (not a
                    // hand-rolled Option/Result split) is what distinguishes a cap-as-tag both-heap
                    // Result (`Result[String,String]`, tag@16) from a len-as-tag scalar-Ok Result (tag@4)
                    // — the old split mis-seeded the former as len-as-tag, so an inner `match r` read
                    // tag@4 (the `Option[Result[String,String]]` interp `some(ok)` → `some(err)` bug).
                    s.seed_variant_param(payload, &bind_ty);
                }
            }
        };
        bind_payload(self, then_bind);
        bind_payload(self, else_bind);
        // SUBJECT-DROP-BEFORE-ARMS (the design that the checker accepts): for a HEAP result,
        // drop the OWNED subject NOW — before the arms — so its lifetime (`i..d`, balanced and
        // independent) does not overlap the per-arm heap move-out + branch-join (which is then
        // exactly the proven heap-result-`if` over a scalar cond). A BORROWED subject (param /
        // tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched; the
        // scalar payload copy above already makes the arms subj-independent. Scalar-result
        // matches keep the subject live (unchanged — they were already proven). A str-result
        // HEAP-bind (`str_heap_bind`) is the exception: its payload BORROWS slot-0, so the subject
        // must stay live THROUGH the arms — its drop is deferred to AFTER the branch-join below.
        if heap_res && !str_heap_bind && !opt_tuple_bind && !result_heap_err_bind {
            if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
                self.live_heap_handles.remove(pos);
                let op = self.drop_op_for(subj);
                self.ops.push(op);
            }
        }
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        let lower_arm = |s: &mut Self, body: &IrExpr| -> Option<ValueId> {
            if heap_res {
                s.lower_heap_result_arm(body, result_ty)
            } else {
                s.lower_scalar_arm(body)
            }
        };
        // BRANCH OWNERSHIP ISOLATION: the two arms are ALTERNATE (only one runs), so each must lower
        // from the SAME ownership state. A borrowed param consumed in the THEN arm (`pairs + [(k,v)]`)
        // must still be available to the ELSE arm (`value.object(pairs)`) and vice versa — without this
        // the THEN arm's `Consume`/move leaks into the ELSE arm's lowering-time view and the ELSE arm
        // walls. Snapshot the owned/borrowed sets before THEN, restore them before ELSE (the emitted ops
        // are per-branch; only the lowering-time tracking is reset). The shared payload binds (cp, $p)
        // were inserted before IfThen, so they survive in both.
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        // THEN (tag != 0): the Some payload / the Err message.
        let then_val = match lower_arm(self, then_body) {
            Some(v) => v,
            None => return rollback(self),
        };
        // RELEASE PARITY (mirrors lower_heap_result_if_inner): which OUTER
        // handles did the then arm MOVE out (e.g. `err(msg)` over an outer
        // let-bound String)? The snapshot restore below makes them live again
        // for the else arm's lowering — but the move happens only on the THEN
        // path, so without a compensating sibling-arm release the else path
        // leaks it AND the post-join scope-end drop double-frees the then path.
        let consumed_by_then: Vec<ValueId> = lhh_snapshot
            .iter()
            .copied()
            .filter(|h| !self.live_heap_handles.contains(h))
            .collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_val) });
        self.param_values = pv_snapshot;
        self.live_heap_handles = lhh_snapshot.clone();
        self.materialized_aggregates = ma_snapshot;
        // ELSE (tag == 0): the None branch / the scalar Ok payload.
        let else_val = match lower_arm(self, else_body) {
            Some(v) => v,
            None => return rollback(self),
        };
        let consumed_by_else: Vec<ValueId> = lhh_snapshot
            .iter()
            .copied()
            .filter(|h| !self.live_heap_handles.contains(h))
            .collect();
        for h in &consumed_by_then {
            if !consumed_by_else.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.push(op); // the ELSE arm releases what THEN moved out …
                self.live_heap_handles.retain(|x| x != h); // … and scope-end must not re-release
            }
        }
        for h in &consumed_by_else {
            if !consumed_by_then.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.insert(else_marker_at, op); // the THEN arm releases what ELSE moved out
            }
        }
        self.ops.push(Op::EndIf { val: Some(else_val) });
        // SUBJECT-DROP-AFTER-ARMS (the str-result heap-bind path): the payload borrowed slot-0, so
        // the subject stayed live through both arms — drop the OWNED subject ONCE here, after the
        // branch-join. The merged result `dst` is a fresh arm value (a concat, a Dup'd copy, a new
        // call result), independent of the freed subject, so freeing the subject's slot-0 String is
        // sound (a bare-Var arm already Dup'd it; a call arm only borrowed it). A BORROWED subject
        // (param / tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched.
        if str_heap_bind || opt_tuple_bind || result_heap_err_bind {
            if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
                self.live_heap_handles.remove(pos);
                let op = self.drop_op_for(subj);
                self.ops.push(op);
            }
        }
        Some(dst)
    }

    /// The HEAP-Ok Result SUBJECT drop-route classification for
    /// [`Self::variant_match_subject`] — routes `subj`'s scope-end drop by the Ok payload's
    /// exact shape (a recursive `resrec:`/`DropResult*` class) instead of the flat
    /// `heap_elem_lists`/`DropListStr` fallback. Verbatim extraction (guard-clause
    /// flattening) of the former inline if-else-if chain, no behavior change — see
    /// docs/roadmap/active/code-health-codopsy.md.
    fn track_heap_ok_result_subject_drop(&mut self, subj: ValueId, ty: &Ty) {
        if let Some(drop_fn) = self.result_ok_record_drop_fn(ty) {
            // RECORD-Ok `Result[<record>, String]`: route the subject's scope-end drop through the
            // recursive `Op::DropWrapperRec` (resrec:) — NOT the flat `heap_elem_lists` DropListStr
            // that leaks the record's nested heap (HOLE-1). `drop_op_for` checks `variant_drop_handles`
            // FIRST, so this wins over the `else` below; the Ok/Err arm binds the @12 handle as a
            // BORROW and the subject drops once AFTER the arms (`str_heap_bind`).
            self.variant_drop_handles.insert(subj, format!("resrec:{drop_fn}"));
            return;
        }
        if crate::lower::is_result_listval_ty(ty) {
            self.value_result_lists.insert(subj);
            return;
        }
        if crate::lower::is_value_result_ty(ty) {
            // `Result[Value, String]` (value.get): the Ok payload is a single dynamic Value —
            // its drop is the RECURSIVE `Op::DropResultValue` (Ok → `$__drop_value`), distinct
            // from a String-Ok's flat DropListStr.
            self.value_result_results.insert(subj);
            return;
        }
        if crate::lower::is_str_int_result_ty(ty) {
            // `Result[(String, Int), String]` (toml parse_key_part): the Ok payload is a
            // (String, Int) tuple — its drop is the RECURSIVE `Op::DropResultStrInt` (frees the
            // tuple's String + tuple block), distinct from a flat DropListStr which would leak
            // the tuple's String (it would rc_dec the @12 tuple HANDLE only).
            self.str_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_value_int_result_ty(ty) {
            // `Result[(Value, Int), String]` (toml parse_val): the Ok tuple's Value slot is freed
            // recursively via `Op::DropResultValueInt` (`$__drop_value_tuple`).
            self.value_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_list_str_int_result_ty(ty) {
            // `Result[(List[String], Int), String]` (toml parse_key): the Ok tuple's List slot is
            // freed recursively via `Op::DropResultListStrInt`.
            self.list_str_int_result_results.insert(subj);
            return;
        }
        if crate::lower::is_list_value_int_result_ty(ty) {
            // `Result[(List[Value], Int), String]` (toml collect_array_items): recursive
            // `Op::DropResultListValueInt` (`$__drop_list_value_tuple`).
            self.list_value_int_result_results.insert(subj);
            return;
        }
        self.heap_elem_lists.insert(subj);
    }
}
