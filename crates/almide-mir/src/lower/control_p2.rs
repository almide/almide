/// Which borrow-through discipline (if any) keeps a variant-match subject
/// alive through the arms — decided once by
/// [`LowerCtx::variant_match_route`], read at the two subject-drop sites.
struct VariantMatchRoute {
    /// The match produces a heap value (the arms lower via `lower_heap_result_arm`).
    heap_res: bool,
    /// str-result heap bind: the payload borrows slot-0, subject drops AFTER the arms.
    str_heap_bind: bool,
    /// Option-tuple payload borrow — same drop-after discipline, len-as-tag @4.
    opt_tuple_bind: bool,
    /// Scalar-Ok / heap-Err `Result[Int, String]` borrow — same drop-after discipline.
    result_heap_err_bind: bool,
}

impl VariantMatchRoute {
    /// The subject's drop is deferred to after the branch-join (a payload
    /// borrows its slot-0 through the arms).
    fn subject_drops_after(&self) -> bool {
        self.str_heap_bind || self.opt_tuple_bind || self.result_heap_err_bind
    }
}

/// Is this parsed payload bind a HEAP (borrowed @12 handle) bind?
fn is_heap_payload_bind(bind: &Option<(VarId, bool, Ty)>) -> bool {
    matches!(bind, Some((_, true, _)))
}

/// The value-position variant match's entry admission: a HEAP subject, exactly
/// two arms, no guards.
fn variant_match_entry_ok(subject: &IrExpr, arms: &[IrMatchArm]) -> bool {
    is_heap_ty(&subject.ty) && two_unguarded_arms(arms)
}

/// Exactly two arms, none guarded — the two-sided match admission every
/// executed variant match requires.
fn two_unguarded_arms(arms: &[IrMatchArm]) -> bool {
    arms.len() == 2 && arms.iter().all(|a| a.guard.is_none())
}

/// Both arm bodies are `Unit` — the statement-position (unit-if) requirement.
fn both_unit(a: &IrExpr, b: &IrExpr) -> bool {
    matches!(a.ty, Ty::Unit) && matches!(b.ty, Ty::Unit)
}

/// Fill a one-shot slot; `None` (decline) if it was already filled — the
/// duplicate-arm rejection.
fn fill_once<T>(slot: &mut Option<T>, v: T) -> Option<()> {
    if slot.is_some() {
        return None;
    }
    *slot = Some(v);
    Some(())
}

/// The Ok-arm bind of a statement-position Result match: scalar Ok
/// (Result[Int,String]) binds a scalar int; a heap-Ok (Result[String,String])
/// binds a heap String — gated to `str_result`.
fn result_ok_arm_bind(inner: &IrPattern, str_result: bool) -> Result<Option<(VarId, Ty)>, ()> {
    match inner {
        IrPattern::Bind { var, ty } if is_heap_ty(ty) == str_result => Ok(Some((*var, ty.clone()))),
        IrPattern::Wildcard => Ok(None),
        _ => Err(()),
    }
}

/// Classify ONE variant-match arm as `(is_then_side, bind)` via the shared
/// payload-bind rule, or `Err(())` for an unsupported shape.
///
/// Option Some (then) / None (else). The bind rule lets a HEAP Some-payload
/// (`some(key)` where key: String/Value/Tuple — toml set_nested's `match list.first(path)`)
/// bind the @12 handle as a BORROW, gated on the Option[heap] subject being tracked
/// nested-ownership (heap_elem_lists, set at tracking time); a scalar payload binds
/// a copy. A Wildcard takes the Option else-side ONLY when the subject is not ALSO
/// result-tracked: a Result Err CTOR bind reuses the Some(string) machinery
/// (materialize_opt_str_some inserts materialized_options), so both flags are
/// true — Result semantics must win (the flexible-side arm). Result Err (then) /
/// Ok (else) BOTH use the bind rule: a scalar Result binds a scalar payload, a
/// str-result (`value.as_string`) binds its slot-0 String as a BORROW (gated on
/// `heap_elem_lists` — only a nested-ownership subject, so a scalar Result still
/// rejects a heap bind); the Ok side carries the str-result's String payload
/// (`ok(s) => emit_scalar(s)`), the very thing `emit` needs. A WILDCARD arm over
/// a RESULT subject (`if let v = x { A } else { B }` — the frontend's if-let
/// desugar emits `Ok(v) => A, _ => B`) takes whichever side the ctor arm did NOT
/// (then=Err when Ok is filled, else=Ok when Err is filled); a wildcard BEFORE
/// any ctor arm is ambiguous → reject.
fn classify_variant_arm(
    arm: &IrMatchArm,
    flags: (bool, bool),
    filled: (bool, bool),
    bind: &dyn Fn(&IrPattern) -> Result<Option<(VarId, bool, Ty)>, ()>,
) -> Result<(bool, Option<(VarId, bool, Ty)>), ()> {
    let (is_option, is_result) = flags;
    let (then_filled, else_filled) = filled;
    match &arm.pattern {
        IrPattern::Some { inner } if is_option => bind(inner).map(|b| (true, b)),
        IrPattern::None if is_option => Ok((false, None)),
        IrPattern::Wildcard if is_option && !is_result => Ok((false, None)),
        IrPattern::Err { inner } if is_result => bind(inner).map(|b| (true, b)),
        IrPattern::Ok { inner } if is_result => bind(inner).map(|b| (false, b)),
        IrPattern::Wildcard if is_result && (then_filled != else_filled) => {
            Ok((!then_filled, None))
        }
        _ => Err(()),
    }
}

/// DESUGAR a tuple Some/Ok payload — `some((idx, line)) => B` → `some($p) => { let (idx,line) = $p; B }`.
/// The single var `$p` is bound via the HEAP-payload path (into `param_values`), so the
/// `let (idx,line) = $p` tuple destructure then lowers (`try_lower_tuple_destructure` borrows each
/// slot). A raw tuple VAR/param destructure alone walls (no `param_values` entry), so the rewrite to
/// the @12-handle bind is required, not a plain var destructure. `$p` ids start above subject+arms.
/// `None` = no tuple payload anywhere (the caller keeps the original arms).
fn desugar_tuple_payload_arms(subject: &IrExpr, arms: &[IrMatchArm]) -> Option<Vec<IrMatchArm>> {
    let has_tuple_payload = arms.iter().any(|a| {
        matches!(&a.pattern, IrPattern::Some { inner } | IrPattern::Ok { inner }
            if matches!(&**inner, IrPattern::Tuple { .. }))
    });
    if !has_tuple_payload {
        return None;
    }
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
    Some(out)
}

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
        if !two_unguarded_arms(arms) {
            return false;
        }
        let ((ok_body, ok_bind), (err_body, err_bind)) =
            match self.parse_result_match_arms(arms, subj, str_result) {
                Some(slots) => slots,
                None => return false,
            };
        if !both_unit(ok_body, err_body) {
            return false;
        }
        // tag = load32(handle(subj) + 4); if tag != 0 then Err-arm else Ok-arm (len 0 = Ok).
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.lifted.truncate(lifted_mark);
            s.live_heap_handles.truncate(lhh_mark);
            false
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        // The tag is the HIGH 32 bits of slot 0 (@16) for a heap-Ok Result (the low 32 bits @12 hold
        // the owned String handle), `len` (@4) for a scalar one.
        let tag_off = if str_result { 16 } else { 4 };
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        self.ops.push(Op::IfThen { cond: tag, dst: None });
        self.bind_result_err_payload(h, err_bind);
        // Exactly ONE arm runs (the unit-if discipline): outer-var reassignments
        // inside an arm mutate the stable local IN PLACE — see `unit_arm_depth`.
        self.unit_arm_depth += 1;
        if self.lower_branch_arm(None, err_body).is_err() {
            self.unit_arm_depth -= 1;
            return rollback(self);
        }
        self.ops.push(Op::Else { val: None });
        self.bind_result_ok_payload(h, ok_bind, str_result);
        let ok_ok = self.lower_branch_arm(None, ok_body).is_ok();
        self.unit_arm_depth -= 1;
        if !ok_ok {
            return rollback(self);
        }
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// Parse the statement-position Result match's arms: exactly
    /// [Ok(scalar-bind?), Err(heap-bind?)], no nested ctors. An Ok binds a
    /// SCALAR Int (value copy); an Err binds a heap String (borrowed slot-0 handle), gated to a
    /// nested-ownership subject (so the Result keeps ownership through its DropListStr). A
    /// TOP-LEVEL `_` catch-all as the non-Ok arm (`match r { ok($q) => …, _ => … }` — the
    /// regrouped codec-roundtrip shape): tag != 0 ⇒ not-Ok ⇒ the wildcard body, binding
    /// nothing — positionally identical to `err(_)` once Ok holds the other arm.
    #[allow(clippy::type_complexity)]
    fn parse_result_match_arms<'a>(
        &self,
        arms: &'a [IrMatchArm],
        subj: ValueId,
        str_result: bool,
    ) -> Option<(
        (&'a IrExpr, Option<(VarId, Ty)>),
        (&'a IrExpr, Option<(VarId, bool)>),
    )> {
        let mut ok: Option<(&IrExpr, Option<(VarId, Ty)>)> = None;
        let mut err: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = result_ok_arm_bind(inner, str_result).ok()?;
                    fill_once(&mut ok, (&arm.body, bind))?;
                }
                IrPattern::Err { inner } => {
                    let bind = self.result_err_bind(subj, inner).ok()?;
                    fill_once(&mut err, (&arm.body, bind))?;
                }
                // A WILDCARD takes whichever side the ctor arm did NOT: after an Ok
                // arm it is the not-Ok (err) side — the original behavior; after an
                // ERR arm (`match r { err(e) => …, _ => … }`, the desugared
                // nested-pattern custom-E shape) it is the not-Err (ok) side,
                // binding nothing. A wildcard BEFORE any ctor arm is ambiguous —
                // reject (same rule as `classify_variant_arm`).
                IrPattern::Wildcard if err.is_some() && ok.is_none() => {
                    fill_once(&mut ok, (&arm.body, Option::None))?
                }
                IrPattern::Wildcard if ok.is_some() => {
                    fill_once(&mut err, (&arm.body, Option::None))?
                }
                _ => return None,
            }
        }
        match (ok, err) {
            (Some(o), Some(e)) => Some((o, e)),
            _ => None,
        }
    }

    /// The Err-arm payload bind of a statement-position Result match.
    ///
    /// `heap_elem_lists` covers the flat-drop str-results (`value.as_string`);
    /// `value_result_lists`/`value_result_results` are the RECURSIVE-drop
    /// twins (`Result[List[Value],String]` / `Result[Value,String]` —
    /// `seed_variant_param` routes these there instead, since their Ok
    /// payload needs `DropResultListValue`/`DropResultValue`, not the flat
    /// `DropListStr` a String-Ok gets). Mirrors `try_lower_variant_value_
    /// match`'s `heap_or_scalar_bind`, the value-position
    /// twin of this statement-position match — WITHOUT this the Err-bind
    /// here is strictly narrower than its twin, so a `Result[Value,String]`
    /// subject (json_path_edges' `p_set`) falls through to the untracked-
    /// subject both-arms-linearization wall even though the twin would
    /// admit it.
    /// A RICH-VARIANT Err payload needing a recursive drop (`Result[Int,
    /// MathError]`, `err(Overflow(msg))` — bidirectional_type_test's structured
    /// error): `try_lower_result_err_variant_ctor` tracks such a subject via
    /// `variant_drop_handles = "res_<V>"` (a GENERATED `$__drop_res_<V>`,
    /// drop_sources.rs), NOT `heap_elem_lists` (explicitly removed there once
    /// `needs_rec` is true) — so this Bind guard, unlike its value-position twin
    /// (which already admits `resrec:`/`optrec:`), had no matching case at all
    /// before that route was added.
    fn result_err_bind(
        &self,
        subj: ValueId,
        inner: &IrPattern,
    ) -> Result<Option<(VarId, bool)>, ()> {
        match inner {
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
                Ok(Some((*var, true)))
            }
            IrPattern::Wildcard => Ok(None),
            _ => Err(()),
        }
    }

    /// THEN (tag != 0 = Err): the message is the BORROWED slot-0 handle.
    fn bind_result_err_payload(&mut self, h: ValueId, err_bind: Option<(VarId, bool)>) {
        use crate::PrimKind;
        if let Some((bind_var, _)) = err_bind {
            let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            self.value_of.insert(bind_var, payload);
            self.param_values.insert(payload);
        }
    }

    /// The Ok-arm payload bind (the ELSE side, tag == 0): a scalar Result
    /// yields the slot-0 int COPY; a heap-Ok Result yields the BORROWED slot-0
    /// String handle (the Result keeps ownership through its DropListStr).
    fn bind_result_ok_payload(&mut self, h: ValueId, ok_bind: Option<(VarId, Ty)>, str_result: bool) {
        use crate::PrimKind;
        let Some((bind_var, bind_ty)) = ok_bind else { return };
        if str_result {
            let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            self.value_of.insert(bind_var, payload);
            self.param_values.insert(payload);
            // NESTED VARIANT PAYLOAD: `ok(m)` where m is itself an
            // Option/Result (`Result[Option[record], String]` — porta
            // read_message's monadic-desugar Ok arm holding `match m
            // { some(req)/none }`). Track the BORROWED payload like the
            // Some-bind path does, so the INNER match BRANCHES on
            // its tag instead of hitting the (walled) linearization.
            self.seed_nested_option_result_bind_payload(payload, &bind_ty);
        } else {
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.value_of.insert(bind_var, payload);
        }
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
        if !variant_match_entry_ok(subject, arms) {
            return None;
        }
        let desugared = desugar_tuple_payload_arms(subject, arms);
        let arms: &[IrMatchArm] = desugared.as_deref().unwrap_or(arms);
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
        let parsed_arms = self.parse_variant_match_arms(arms, subj, is_option, is_result);
        let ((then_body, then_bind), (else_body, else_bind)) = match parsed_arms {
            Some(slots) => slots,
            None => return rollback(self),
        };
        let has_heap_bind = is_heap_payload_bind(&then_bind) || is_heap_payload_bind(&else_bind);
        let route = match self.variant_match_route(
            subj, result_ty, (is_option, is_result, is_result_str), has_heap_bind,
        ) {
            Some(r) => r,
            None => return rollback(self),
        };
        // Emit: h = handle(subj); tag = load32(h + off); dst = if tag != 0 then <then> else <else>.
        // A scalar Option/Result reads len-as-tag (@4); a heap-Ok `Result[String,String]`
        // (value.as_string) reads the cap-as-tag at the slot-0 HIGH 32 bits (@16).
        let tag_off = if is_result_str { 16 } else { 4 };
        let dst = self.fresh_value();
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        self.bind_variant_payload(h, then_bind);
        self.bind_variant_payload(h, else_bind);
        // SUBJECT-DROP-BEFORE-ARMS (the design that the checker accepts): for a HEAP result,
        // drop the OWNED subject NOW — before the arms — so its lifetime (`i..d`, balanced and
        // independent) does not overlap the per-arm heap move-out + branch-join (which is then
        // exactly the proven heap-result-`if` over a scalar cond). A BORROWED subject (param /
        // tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched; the
        // scalar payload copy above already makes the arms subj-independent. Scalar-result
        // matches keep the subject live (unchanged — they were already proven). A borrow-through
        // route (`str_heap_bind`/`opt_tuple_bind`/`result_heap_err_bind`) is the exception: its
        // payload BORROWS slot-0, so the subject must stay live THROUGH the arms — its drop is
        // deferred to AFTER the branch-join below.
        if route.heap_res && !route.subject_drops_after() {
            self.drop_owned_subject(subj);
        }
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        if self
            .lower_variant_match_branches((then_body, else_body), route.heap_res, result_ty)
            .is_none()
        {
            return rollback(self);
        }
        // SUBJECT-DROP-AFTER-ARMS (the borrow-through routes): the payload borrowed slot-0, so
        // the subject stayed live through both arms — drop the OWNED subject ONCE here, after the
        // branch-join. The merged result `dst` is a fresh arm value (a concat, a Dup'd copy, a new
        // call result), independent of the freed subject, so freeing the subject's slot-0 String is
        // sound (a bare-Var arm already Dup'd it; a call arm only borrowed it). A BORROWED subject
        // (param / tracked var, not in `live_heap_handles`) is owned elsewhere → left untouched.
        if route.subject_drops_after() {
            self.drop_owned_subject(subj);
        }
        Some(dst)
    }

    /// Drop the OWNED subject once, if this scope owns it — a BORROWED subject
    /// (param / tracked var, not in `live_heap_handles`) is owned elsewhere and
    /// left untouched.
    fn drop_owned_subject(&mut self, subj: ValueId) {
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
    }

    /// Bind one arm's payload as a subj-independent value BEFORE the arms —
    /// for the heap-result case this is what severs the arm's heap move-out
    /// from the subject. A scalar payload is a COPY (load64 @12); a heap
    /// payload borrows the @12 handle (into `param_values`).
    fn bind_variant_payload(&mut self, h: ValueId, bind: Option<(VarId, bool, Ty)>) {
        use crate::PrimKind;
        let Some((bind_var, is_heap, bind_ty)) = bind else { return };
        let payload = if is_heap {
            self.load_at_offset(h, 12, PrimKind::LoadHandle)
        } else {
            self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
        };
        self.value_of.insert(bind_var, payload);
        if is_heap {
            self.param_values.insert(payload);
            // NESTED VARIANT PAYLOAD: `ok(m)` / `some(m)` where m is itself an Option/Result
            // (`Result[Option[record], String]` — the `read_message()!` monadic-desugar Ok arm
            // holding porta's `match m { some(req)/none }`; `Option[Result[String,String]]` — the
            // nested-compound interp). SEED the BORROWED payload's READ-shape via the canonical
            // seeder so the INNER match BRANCHES on its tag. Using `seed_variant_param` (not a
            // hand-rolled Option/Result split) is what distinguishes a cap-as-tag both-heap
            // Result (`Result[String,String]`, tag@16) from a len-as-tag scalar-Ok Result (tag@4)
            // — the old split mis-seeded the former as len-as-tag, so an inner `match r` read
            // tag@4 (the `Option[Result[String,String]]` interp `some(ok)` → `some(err)` bug).
            self.seed_variant_param(payload, &bind_ty);
        }
    }

    /// Lower BOTH match arms inside the already-open `IfThen` with BRANCH
    /// OWNERSHIP ISOLATION and RELEASE PARITY, closing the join with `EndIf`.
    ///
    /// ISOLATION: the two arms are ALTERNATE (only one runs), so each must lower
    /// from the SAME ownership state. A borrowed param consumed in the THEN arm (`pairs + [(k,v)]`)
    /// must still be available to the ELSE arm (`value.object(pairs)`) and vice versa — without this
    /// the THEN arm's `Consume`/move leaks into the ELSE arm's lowering-time view and the ELSE arm
    /// walls. Snapshot the owned/borrowed sets before THEN, restore them before ELSE (the emitted ops
    /// are per-branch; only the lowering-time tracking is reset). The shared payload binds (cp, $p)
    /// were inserted before IfThen, so they survive in both.
    ///
    /// RELEASE PARITY (mirrors lower_heap_result_if_inner): which OUTER
    /// handles did the then arm MOVE out (e.g. `err(msg)` over an outer
    /// let-bound String)? The snapshot restore makes them live again
    /// for the else arm's lowering — but the move happens only on the THEN
    /// path, so without a compensating sibling-arm release the else path
    /// leaks it AND the post-join scope-end drop double-frees the then path.
    /// `None` = an arm declined; the caller rolls the whole match back.
    fn lower_variant_match_branches(
        &mut self,
        bodies: (&IrExpr, &IrExpr),
        heap_res: bool,
        result_ty: &Ty,
    ) -> Option<()> {
        let (then_body, else_body) = bodies;
        let lower_arm = |s: &mut Self, body: &IrExpr| -> Option<ValueId> {
            if heap_res {
                s.lower_heap_result_arm(body, result_ty)
            } else {
                s.lower_scalar_arm(body)
            }
        };
        let pv_snapshot = self.param_values.clone();
        let lhh_snapshot = self.live_heap_handles.clone();
        let ma_snapshot = self.materialized_aggregates.clone();
        // THEN (tag != 0): the Some payload / the Err message.
        let then_val = lower_arm(self, then_body)?;
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
        let else_val = lower_arm(self, else_body)?;
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
        Some(())
    }

    /// Parse the two arms into `((then_body, then_bind), (else_body, else_bind))` where a bind is
    /// an optional SCALAR payload var (`Some(x)` / `Ok(x)` / a scalar `Err(c)`). A heap bind
    /// (`Err(msg: String)`) is allowed only when the arm body never needs it as an owner —
    /// there it is bound as a BORROW of the Result's owned slot-0 handle, gated on the subject
    /// being a nested-ownership list (it frees the payload at scope end). A wildcard binds nothing.
    /// `None` = an unsupported arm shape / a duplicate side (the caller rolls back).
    #[allow(clippy::type_complexity)]
    fn parse_variant_match_arms<'a>(
        &self,
        arms: &'a [IrMatchArm],
        subj: ValueId,
        is_option: bool,
        is_result: bool,
    ) -> Option<(
        (&'a IrExpr, Option<(VarId, bool, Ty)>),
        (&'a IrExpr, Option<(VarId, bool, Ty)>),
    )> {
        let heap_or_scalar_bind = |inner: &IrPattern| -> Result<Option<(VarId, bool, Ty)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false, ty.clone()))),
                // A heap payload bind is admitted over a nested-ownership subject — a str-result
                // (`heap_elem_lists`, the `value.as_string` String payload) OR a value-array result
                // (`value_result_lists`, the `value.as_array` `List[Value]` payload, e.g. `ok(items)
                // => emit_seq(items)`). Both bind the @12 handle as a BORROW (drop-subject-after).
                IrPattern::Bind { var, ty }
                    if is_heap_ty(ty)
                        && (self.heap_elem_lists.contains(&subj)
                            // `Option[List[String]]` (the heap-acc fold value) — routed to the
                            // nested DropListListStr set; the payload bind discipline is identical
                            // (a borrowed @12 handle, the subject's own recursive drop frees it).
                            || self.list_list_str_lists.contains(&subj)
                            || self.value_result_lists.contains(&subj)
                            || self.value_result_results.contains(&subj)
                            // A record-Ok `Result[<record>, String]` subject (`resrec:<R>` drop
                            // handle): the `ok(m: record)` payload (AND the `err(e: String)` slot)
                            // binds the @12 handle as a BORROW; the subject's recursive
                            // `DropWrapperRec` frees the live block (record or Err String) once
                            // after the arms. A bare-Var move-out arm auto-`Dup`s, so no double-free.
                            // An option-of-variant subject (`optrec:<T>`, `some(Number(7))`):
                            // the Some-arm payload binds the @12 variant handle as a BORROW;
                            // the subject's recursive drop frees the payload once after the
                            // arms — the same resrec discipline.
                            || self.variant_drop_handles
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
            let filled = (then_slot.is_some(), else_slot.is_some());
            let parsed = classify_variant_arm(arm, (is_option, is_result), filled, &heap_or_scalar_bind);
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

    /// Classify the heap-bind ROUTE of an admitted variant match — which
    /// borrow-through discipline (if any) keeps the subject alive through the
    /// arms. `None` = a heap result with a heap bind in none of the admitted
    /// routes (the true Camp-4 frontier — the caller rolls back and walls).
    fn variant_match_route(
        &self,
        subj: ValueId,
        result_ty: &Ty,
        flags: (bool, bool, bool),
        has_heap_bind: bool,
    ) -> Option<VariantMatchRoute> {
        let (is_option, is_result, is_result_str) = flags;
        let heap_res = is_heap_ty(result_ty);
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
        let result_heap_err_bind = heap_res
            && has_heap_bind
            && is_result
            && !is_result_str
            && self.heap_elem_lists.contains(&subj);
        if heap_res && has_heap_bind && !is_result_str && !opt_tuple_bind && !result_heap_err_bind {
            return None;
        }
        Some(VariantMatchRoute { heap_res, str_heap_bind, opt_tuple_bind, result_heap_err_bind })
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
