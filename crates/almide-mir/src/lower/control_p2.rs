impl LowerCtx {
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
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
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
        if self.lower_branch_arm(None, err_body).is_err() {
            self.ops.truncate(ops_mark);
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
                use almide_lang::types::constructor::TypeConstructorId;
                if matches!(&bind_ty, Ty::Applied(TypeConstructorId::Option, _)) {
                    self.materialized_options.insert(payload);
                    if crate::lower::is_lenlist_list_ty(&bind_ty) {
                        self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
                    } else if crate::lower::is_heap_elem_list_ty(&bind_ty) {
                        self.heap_elem_lists.insert(payload);
                    }
                } else if crate::lower::is_result_ty(&bind_ty) {
                    self.materialized_results.insert(payload);
                    if crate::lower::is_lenlist_list_ty(&bind_ty) {
                        self.variant_drop_handles.insert(payload, "list_lenlist".to_string());
                    } else if crate::lower::is_heap_elem_list_ty(&bind_ty) {
                        self.heap_elem_lists.insert(payload);
                    }
                }
            } else {
                let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
                self.value_of.insert(bind_var, payload);
            }
        }
        if self.lower_branch_arm(None, ok_body).is_err() {
            self.ops.truncate(ops_mark);
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
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // EFFECT-RESULT SUBJECT (#76): a ctor-`match` over a CAN-ERR EFFECT call returning a Result
        // — an `@intrinsic`/impure stdlib `Module` effect (`process.kill`/`process.spawn`) or a bare
        // effect `RuntimeCall`. `lower_call_args` REFUSES such a heap-result effect call in argument
        // position (it would defer to an empty Opaque), so the match over it walled. Materialize it
        // HERE as a real OWNED Result handle so the tag-read below executes. HOLE-1: gated on
        // `effect_unwrap_admitted` (Result whose Ok payload has a real recursive drop — scalar /
        // String / Value / List[Value] / tuple-Ok); a RECORD-Ok effect result is REFUSED here and
        // falls through to the ordinary path (which walls), NEVER routed through a leaky flat cert.
        let used_effect_subj = self.is_effect_result_subject(subject);
        // Materialize/borrow + track the subject exactly as the statement Match entry does:
        // an owned ctor temp (`Some(5)`) is dropped at scope end; a tracked Var (`let o =
        // Some(5)`) is borrowed; a self-host Option/Result-returning call is tracked here.
        let subj = if used_effect_subj {
            match self.try_materialize_effect_result_subject(subject) {
                Some(v) => v,
                None => return rollback(self),
            }
        } else {
            match self
            .lower_call_args(std::slice::from_ref(subject))
            .ok()
            .and_then(|a| a.into_iter().next())
        {
            Some(CallArg::Handle(v)) => v,
            _ => return rollback(self),
        }
        };
        // A USER-FN `Named` call returning Option/Result is tracked the SAME as a self-host call: every
        // Option/Result value uses the one DynListStr len-as-tag repr (brick #51), so `match find_colon(t)
        // { none => …, some(cp) => … }` over `fn find_colon(..) -> Option[Int]` reads the tag @4 + binds the
        // scalar payload @12 identically. `subj` is the OWNED call result (live, dropped-before for a scalar
        // payload), and the tracking is per-subject. A HEAP-Ok user Result still self-gates (heap_or_scalar_
        // bind requires a str-result), so only scalar payloads lower — never a silently-wrong heap move-out.
        let is_named_call =
            matches!(&subject.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. });
        if is_self_host_option_call(subject)
            || (is_named_call
                && crate::lower::is_variant_ty(&subject.ty)
                && !crate::lower::is_result_ty(&subject.ty))
        {
            self.materialized_options.insert(subj);
            // An `Option[heap]` (`list.first(path): Option[String]` — toml set_nested's
            // `match list.first(path)`) OWNS its payload: track it as a nested-ownership list so the
            // Some-payload bind reads the borrowed element handle AND the scope-end drop is the
            // recursive DropListStr (mirrors control.rs:100 for the statement-match path).
            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                self.heap_elem_lists.insert(subj);
            }
        }
        if is_self_host_result_call(subject)
            || (is_named_call
                && crate::lower::is_result_ty(&subject.ty)
                && !Self::is_heap_ok_result(&subject.ty))
            // An EFFECT-result subject (process.kill / RuntimeCall) with a SCALAR-Ok / heap-Err
            // Result is tracked the SAME as a scalar self-host/Named result: len-as-tag @4, Err arm
            // binds slot-0 String, subject drops via DropListStr (the case-A heap_elem_lists below).
            || (used_effect_subj && !Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results.insert(subj);
            // Camp-4 sub-case 1 — a SCALAR-Ok / HEAP-Err `Result[Int, String]` (char_to_val; the
            // unwrap-`!`-desugar's `err($x) => err($x)` re-wrap). The len-as-tag read stays @4
            // (materialized_results, NOT _str), but ALSO track heap_elem_lists so (a) the Err arm's
            // String payload bind is ADMITTED (heap_or_scalar_bind) and (b) the subject drops via
            // `DropListStr` — EXACTLY right for this layout: Ok=len0 frees nothing (the int is scalar),
            // Err=len1 frees slot-0's String. Gated to scalar-Ok + heap-Err so a heap-Ok Result (a
            // different layout) is untouched. The Err arm move-out auto-`Dup`s in lower_heap_result_arm,
            // so drop-subject-after frees slot-0 once (no double-free — gate-checked).
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) = &subject.ty {
                if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    self.heap_elem_lists.insert(subj);
                }
            }
        }
        // A self-host HEAP-Ok Result (`value.as_string`/`value.as_array`/`result.zip` — cap-as-tag
        // DynListStr) is tracked as a str-result (the match reads tag @16 + binds the @12 payload
        // handle). The DROP differs by Ok-arm type: a `List[Value]` Ok (`value.as_array`) frees
        // RECURSIVELY (`value_result_lists` → `DropResultListValue`), else a String Ok frees flat
        // (`heap_elem_lists` → `DropListStr`). Type-driven so it is sound at every tracking site.
        // Camp-4 sub-case 2 — a USER heap-Ok `Result[heap, String]` (decode_chunks's
        // `Result[List[Int], String]`). Its CONSTRUCTION goes through `materialize_result_str`
        // (cap-as-tag @16, slot-0 @12 = the heap payload), so the MATCH must read cap-tag @16 too —
        // route it through the SAME str-result tracking (materialized_results_str + the by-type drop
        // below: List[Value]→value_result_lists, Value→value_result_results, else flat→heap_elem_lists
        // = DropListStr, exact for a List[Int]/String Ok). WITHOUT this the match read the len-as-tag
        // @4 over a cap-tag block — a SILENT MISCOMPILE (Ok payload + tag both misread).
        // *** HOLE-1 (the gate-INVISIBLE leak, now CLOSED by a real recursive drop) *** A record-Ok
        // subject (`Result[<recursive-drop record>, String]` — resolve_run_caps' `Result[Manifest,
        // String]`, load_manifest's nested record-Ok) is now ADMITTED: it carries the SAME cap-as-tag
        // DynListStr repr as every other str-result (slot-0 @12 = the Ok record / Err String, tag @16),
        // so the match reads tag @16 + binds the @12 payload identically. The ONE thing that made it a
        // leak — the flat `else => heap_elem_lists` (DropListStr) freeing only the @12 record HANDLE and
        // LEAKING the record's nested heap fields — is replaced below by routing the subject's scope-end
        // drop through `variant_drop_handles="resrec:<R>"` → `Op::DropWrapperRec { is_result: true }`:
        // at the wrapper's last ref it recurses into the @12 record via the generated `$__drop_<R>` (Ok
        // tag) / `rc_dec`s the @12 Err String, then frees the wrapper — freeing every nested heap field
        // exactly once (the 6625a5d3 / f75eecae machinery, the SAME `resrec:` the record-Ok CONSTRUCTION
        // side already uses via `try_lower_result_record_ctor`). Gated on `result_ok_record_drop_fn`
        // (the record HAS a generated `$__drop_<R>`); a record without one keeps the sound flat path.
        if is_self_host_result_str_call(subject)
            || (is_named_call && Self::is_heap_ok_result(&subject.ty))
            // An EFFECT-result subject with a HEAP-Ok Result (String/Value/List[Value]/tuple-Ok —
            // `effect_unwrap_admitted` already excluded RECORD-Ok; the by-type dispatch below is exact).
            || (used_effect_subj && Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results_str.insert(subj);
            if let Some(drop_fn) = self.result_ok_record_drop_fn(&subject.ty) {
                // RECORD-Ok `Result[<record>, String]`: route the subject's scope-end drop through the
                // recursive `Op::DropWrapperRec` (resrec:) — NOT the flat `heap_elem_lists` DropListStr
                // that leaks the record's nested heap (HOLE-1). `drop_op_for` checks `variant_drop_handles`
                // FIRST, so this wins over the `else` below; the Ok/Err arm binds the @12 handle as a
                // BORROW and the subject drops once AFTER the arms (`str_heap_bind`).
                self.variant_drop_handles.insert(subj, format!("resrec:{drop_fn}"));
            } else if crate::lower::is_result_listval_ty(&subject.ty) {
                self.value_result_lists.insert(subj);
            } else if crate::lower::is_value_result_ty(&subject.ty) {
                // `Result[Value, String]` (value.get): the Ok payload is a single dynamic Value —
                // its drop is the RECURSIVE `Op::DropResultValue` (Ok → `$__drop_value`), distinct
                // from a String-Ok's flat DropListStr.
                self.value_result_results.insert(subj);
            } else if crate::lower::is_str_int_result_ty(&subject.ty) {
                // `Result[(String, Int), String]` (toml parse_key_part): the Ok payload is a
                // (String, Int) tuple — its drop is the RECURSIVE `Op::DropResultStrInt` (frees the
                // tuple's String + tuple block), distinct from a flat DropListStr which would leak
                // the tuple's String (it would rc_dec the @12 tuple HANDLE only).
                self.str_int_result_results.insert(subj);
            } else if crate::lower::is_value_int_result_ty(&subject.ty) {
                // `Result[(Value, Int), String]` (toml parse_val): the Ok tuple's Value slot is freed
                // recursively via `Op::DropResultValueInt` (`$__drop_value_tuple`).
                self.value_int_result_results.insert(subj);
            } else if crate::lower::is_list_str_int_result_ty(&subject.ty) {
                // `Result[(List[String], Int), String]` (toml parse_key): the Ok tuple's List slot is
                // freed recursively via `Op::DropResultListStrInt`.
                self.list_str_int_result_results.insert(subj);
            } else if crate::lower::is_list_value_int_result_ty(&subject.ty) {
                // `Result[(List[Value], Int), String]` (toml collect_array_items): recursive
                // `Op::DropResultListValueInt` (`$__drop_list_value_tuple`).
                self.list_value_int_result_results.insert(subj);
            } else {
                self.heap_elem_lists.insert(subj);
            }
        }
        // Dispatch on the tracking set. An Option reads len-as-tag (Some=len≠0); a scalar
        // Result reads len-as-tag INVERSE (Err=len≠0, Ok=len0). The if-skeleton is uniform
        // (then = tag≠0, else = tag==0): Option → then=Some/else=None; Result → then=Err/else=Ok.
        // A BORROWED Option[heap] / Result[heap] FIELD subject (`match u.email { some(e) => …,
        // none => … }` where `email: Option[String]` is a field of the param `u`). `lower_call_args`
        // borrowed the field's variant handle — a nested-ownership block `u` still OWNS (freed by
        // `u`'s drop; NOT in `live_heap_handles`, so no owned-subject-drop conflict). Track it so the
        // tag-read + heap-payload BORROW bind below execute (the some-arm `e` is a second borrow of
        // the Option's @12 slot; the result String is moved out fresh). Same len-as-tag layout as a
        // self-host Option/Result value — only the SOURCE (a field borrow, not a call) differs.
        if matches!(&subject.kind, IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }) {
            use almide_lang::types::constructor::TypeConstructorId;
            match &subject.ty {
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => {
                    self.materialized_options.insert(subj);
                    if is_heap_ty(&a[0]) {
                        self.heap_elem_lists.insert(subj);
                    }
                }
                Ty::Applied(TypeConstructorId::Result, a)
                    if a.len() == 2 && !Self::is_heap_ok_result(&subject.ty) =>
                {
                    self.materialized_results.insert(subj);
                    if is_heap_ty(&a[1]) {
                        self.heap_elem_lists.insert(subj);
                    }
                }
                _ => {}
            }
        }
        let is_option = self.materialized_options.contains(&subj);
        // A scalar Result reads len-as-tag (@4); a HEAP-Ok `Result[String,String]` (value.as_string,
        // the cap-as-tag DynListStr) reads the tag at the slot-0 HIGH 32 bits (@16). Both arrange
        // Err=then(tag≠0)/Ok=else(tag0); only the tag OFFSET differs. A str-result match here is
        // ADMITTED for WILDCARD/scalar binds (`match value.as_string(v) { ok(_) => …, err(_) => … }`
        // — is_scalar_type); a heap-payload bind over a str-result (`ok(s: String)`) is the Camp-4
        // borrowed-slot case → gated out below (heap_or_scalar_bind already requires heap_elem_lists,
        // and the heap-RESULT branch defers it).
        let is_result_str = self.materialized_results_str.contains(&subj);
        let is_result = self.materialized_results.contains(&subj) || is_result_str;
        if !is_option && !is_result {
            return rollback(self);
        }
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
                            || s.value_result_lists.contains(&subj)
                            || s.value_result_results.contains(&subj)
                            // A record-Ok `Result[<record>, String]` subject (`resrec:<R>` drop
                            // handle): the `ok(m: record)` payload (AND the `err(e: String)` slot)
                            // binds the @12 handle as a BORROW; the subject's recursive
                            // `DropWrapperRec` frees the live block (record or Err String) once
                            // after the arms. A bare-Var move-out arm auto-`Dup`s, so no double-free.
                            || s.variant_drop_handles
                                .get(&subj)
                                .is_some_and(|h| h.starts_with("resrec:"))) =>
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
                IrPattern::None | IrPattern::Wildcard if is_option => Ok((false, None)),
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

    /// If `ty` is a CUSTOM variant (a user ADT) with a registered [`VariantLayout`], return its
    /// type name. Handles the three surface forms a variant type takes
    /// (`Named` / inline `Variant` / `Applied(UserDefined)`). `None` for Option/Result (those
    /// use the dedicated len-as-tag path) and every non-variant type.
    /// Is `subject` a Result-returning call `lower_call_args` REFUSES in subject position (it would
    /// defer to an empty Opaque, walling the ctor-match over it), but whose Ok payload the ctor-match
    /// CAN bind/drop soundly? The admitted Ok set is `effect_unwrap_admitted` (scalar / String /
    /// Value / List[Value] / tuple-Ok) PLUS a RECORD-Ok with a generated `$__drop_<R>`
    /// (`result_ok_record_drop_fn` — the HOLE-1 record-result now has a real recursive drop). Three
    /// call shapes materialize via [`Self::try_materialize_effect_result_subject`]:
    ///   1. an IMPURE stdlib/cross-module `Module` effect (`process.kill`, `manifest.load_manifest`) —
    ///      the original #76 case (CallFn the dotted effect name);
    ///   2. a bare effect `RuntimeCall`;
    ///   3. a PURE heap-Result `Module` call (`json.parse` / `toml.parse`) — the `let x = parse(c)!`
    ///      monadic-desugar subject. It is NOT a tracked self-host option/result call (those keep
    ///      their existing borrow/track path through `lower_call_args`), so it walled; route it
    ///      through `lower_pure_module_value_call` (the SAME emitted CallFn name as every other call
    ///      site — `list_heap_call_name`, never a raw dotted name) so the match reads a real block.
    pub(crate) fn is_effect_result_subject(&self, subject: &IrExpr) -> bool {
        if !effect_unwrap_admitted(&subject.ty)
            && self.result_ok_record_drop_fn(&subject.ty).is_none()
        {
            return false;
        }
        match &subject.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
                !crate::purity::is_pure(module.as_str(), func.as_str())
                    // A PURE heap-Result module call NOT already handled as a self-host
                    // option/result subject (those route through `lower_call_args` + the existing
                    // borrow/track sets, unchanged).
                    || (!is_self_host_option_call(subject)
                        && !is_self_host_result_call(subject)
                        && !is_self_host_result_str_call(subject))
            }
            IrExprKind::RuntimeCall { .. } => true,
            _ => false,
        }
    }

    /// Materialize a Result-call subject ([`Self::is_effect_result_subject`]) into a real OWNED
    /// Result handle pushed to `live_heap_handles` so the SUBJECT-DROP-BEFORE-ARMS / drop-after
    /// machinery frees it exactly once. A PURE `Module` call routes through
    /// `lower_pure_module_value_call` (its CallFn name = `list_heap_call_name`, byte-identical to
    /// every other site, and it records the call's caps properly — a pure `json.parse` is
    /// Stdout-free). An IMPURE `Module` effect / a `RuntimeCall` emit a direct `Op::CallFn` with the
    /// dotted/symbol name (the #76 path). Self-rolls-back on any partial failure. The OWNERSHIP cert
    /// stays exact (the heap result backs one `i`).
    pub(crate) fn try_materialize_effect_result_subject(&mut self, subject: &IrExpr) -> Option<ValueId> {
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.live_heap_handles.truncate(lhh_mark);
            None
        };
        // PURE module call (json.parse / toml.parse): route through the standard pure-module value
        // call so the emitted CallFn name + arg lowering match every other site.
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
            &subject.kind
        {
            if crate::purity::is_pure(module.as_str(), func.as_str()) {
                let Ok(dst) = self.lower_pure_module_value_call(
                    module.as_str(),
                    func.as_str(),
                    args,
                    &subject.ty,
                ) else {
                    return rollback(self);
                };
                if !self.live_heap_handles.contains(&dst) {
                    self.live_heap_handles.push(dst);
                }
                return Some(dst);
            }
        }
        // IMPURE effect Module / RuntimeCall: CallFn the dotted/symbol name directly.
        let (name, args): (String, &[IrExpr]) = match &subject.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                (format!("{}.{}", module.as_str(), func.as_str()), args)
            }
            IrExprKind::RuntimeCall { symbol, args } => (symbol.as_str().to_string(), args),
            _ => return None,
        };
        let Ok(lowered) = self.lower_call_args(args) else {
            return rollback(self);
        };
        let Ok(repr) = repr_of(&subject.ty) else {
            return rollback(self);
        };
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn { dst: Some(dst), name, args: lowered, result: Some(repr) });
        self.live_heap_handles.push(dst);
        Some(dst)
    }

    pub(crate) fn custom_variant_type_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let name = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Variant { name, .. } => name.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        self.variant_layouts.by_type.contains_key(&name).then_some(name)
    }

    /// EXECUTE a `match v { Ctor(binds…) => arm, … }` over a CUSTOM variant (ADT brick 3) —
    /// read the tag from slot 0 and dispatch to the matching arm; only the taken arm runs. The
    /// N-constructor generalization of [`Self::try_lower_variant_value_match`] (the 2-variant
    /// Option/Result case), over the v1 tag@slot0 + i64-slot value model (NOT the len-as-tag
    /// Option/Result repr). Returns the scalar result `dst`, or `None` (rolled back) outside
    /// the subset — the caller then walls (a Const-0 would silently pick a wrong arm — the ②
    /// cardinal rule).
    ///
    /// SUBSET: SCALAR result (ADT brick 3) OR a HEAP result over a BORROWED subject (ADT brick 4,
    /// e.g. recursive `to_string` — each arm reads the borrowed subject's scalar slots and moves
    /// out a fresh heap value). SCALAR ctor-field binds only (a heap/nested ctor field = ADT
    /// brick 5). No guards. An OWNED-temp subject with a heap result would need
    /// subject-drop-before-arms (the cert rejects the owned-borrow / arm-move-out overlap) — it
    /// WALLS here rather than emit cert-failing MIR (ADT brick 4b).
    ///
    /// SOUNDNESS: the subject is materialized/borrowed by `lower_call_args` (an owned ctor temp
    /// drops at scope end via `live_heap_handles`; a tracked Var/param borrows). The tag/field
    /// reads are scalar prims, the `IfThen`/`Else`/`EndIf` markers no-op in `verify_ownership`,
    /// and each arm is a per-arm-balanced frame (`lower_scalar_arm` / `lower_heap_result_arm`)
    /// with NO heap ownership event beyond the arm's own move-out — exactly the per-arm
    /// linearization the cert proves, wrapped so one arm runs. The LAST arm is the unconditional
    /// `else` (the frontend guarantees the match is exhaustive).
    /// TAIL-VALUE Result match over a len-as-tag subject with HEAP-result arms — the
    /// Camp-4 opener for the `compute` class:
    ///   `match safe_div(a, b) { ok(v) => ok(int.to_string(v)), err(e) => <heap arm> }`
    /// The SUBJECT is materialized as an OWNED tracked temp (a call result / Dup'd var —
    /// dropped at scope end, AFTER the arms: each arm binds its payload as a BORROW (a
    /// scalar copy for Ok @12; the slot-0 HANDLE for a heap Err — `param_values`, not a
    /// second owner) and constructs its own FRESH result via `lower_heap_result_arm`
    /// (which Dups any borrowed payload it re-wraps), so nothing outlives the subject.
    /// The IfThen/Else/EndIf merge carries the arm value out — the released-merge cert
    /// shape `lower_heap_result_if_inner` already proves, incl. the release-parity sweep.
    pub(crate) fn try_lower_result_match_value(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(result_ty) || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        // len-as-tag subjects only: a SCALAR Ok payload (Err payload scalar or heap).
        let (ok_pay_ty, err_pay_ty) = match &subject.ty {
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && !is_heap_ty(&a[0]) => {
                (a[0].clone(), a[1].clone())
            }
            _ => return None,
        };
        let mut ok_arm: Option<(&IrExpr, Option<VarId>)> = None;
        let mut err_arm: Option<(&IrExpr, Option<VarId>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Ok { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, .. } => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return None,
                    };
                    if ok_arm.is_some() {
                        return None;
                    }
                    ok_arm = Some((&arm.body, bind));
                }
                IrPattern::Err { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, .. } => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return None,
                    };
                    if err_arm.is_some() {
                        return None;
                    }
                    err_arm = Some((&arm.body, bind));
                }
                IrPattern::Wildcard => {
                    if err_arm.is_some() {
                        return None;
                    }
                    err_arm = Some((&arm.body, None));
                }
                _ => return None,
            }
        }
        let ((ok_body, ok_bind), (err_body, err_bind)) = match (ok_arm, err_arm) {
            (Some(o), Some(e)) => (o, e),
            _ => return None,
        };
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        // Materialize the subject to an OWNED tracked temp (a call result is fresh-owned; a
        // Var is borrowed — Dup it so the scope-end drop discipline is uniform). The temp is
        // in live_heap_handles → freed by the epilogue AFTER the merge move-out.
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
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        // THEN (tag != 0 = Err): payload = slot 0 (borrowed).
        if let Some(var) = err_bind {
            let payload = if is_heap_ty(&err_pay_ty) {
                let p = self.load_at_offset(h, 12, PrimKind::LoadHandle);
                self.param_values.insert(p);
                p
            } else {
                self.load_at_offset(h, 12, PrimKind::Load { width: 8 })
            };
            self.value_of.insert(var, payload);
        }
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let err_obj = match self.lower_heap_result_arm(err_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_err: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(err_obj) });
        // ELSE (tag == 0 = Ok): scalar payload copy.
        if let Some(var) = ok_bind {
            let _ = &ok_pay_ty;
            let payload = self.load_at_offset(h, 12, PrimKind::Load { width: 8 });
            self.value_of.insert(var, payload);
        }
        let live_after_err: Vec<ValueId> = self.live_heap_handles.clone();
        let ok_obj = match self.lower_heap_result_arm(ok_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_ok: Vec<ValueId> = live_after_err
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        // Release parity across the arms (the lower_heap_result_if_inner discipline).
        for x in &consumed_by_err {
            if !consumed_by_ok.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_ok {
            if !consumed_by_err.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(ok_obj) });
        Some(dst)
    }

    /// TAIL-VALUE match over a LIST subject with exactly one `[]` arm and one catch-all
    /// (`_` or a bind-all `ys`) — the len-tag twin of [`Self::try_lower_result_match_value`]:
    ///   `match list.filter(xs, f) { [] => None, ys => list.get(ys, 0) }`
    /// The subject is an OWNED tracked temp (a call result is fresh-owned; a Var is Dup'd).
    /// tag = len@4: THEN (len != 0) = the non-empty arm — a bind-all var ALIASES the subject
    /// temp itself (arm calls borrow it; if the arm MOVES it out, the release-parity sweep
    /// compensates with a drop on the empty side). ELSE (len == 0) = the `[]` arm. Same
    /// IfThen/Else/EndIf merge + release-parity discipline as the Result opener.
    pub(crate) fn try_lower_list_match_value(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if !is_heap_ty(result_ty) || arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        if !matches!(&subject.ty, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1) {
            return None;
        }
        let mut empty_arm: Option<&IrExpr> = None;
        let mut rest_arm: Option<(&IrExpr, Option<VarId>)> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::List { elements } if elements.is_empty() => {
                    if empty_arm.is_some() {
                        return None;
                    }
                    empty_arm = Some(&arm.body);
                }
                IrPattern::Bind { var, .. } => {
                    if rest_arm.is_some() {
                        return None;
                    }
                    rest_arm = Some((&arm.body, Some(*var)));
                }
                IrPattern::Wildcard => {
                    if rest_arm.is_some() {
                        return None;
                    }
                    rest_arm = Some((&arm.body, None));
                }
                _ => return None,
            }
        }
        let (empty_body, (rest_body, rest_bind)) = match (empty_arm, rest_arm) {
            (Some(e), Some(r)) => (e, r),
            _ => return None,
        };
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
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
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        // THEN (len != 0): the non-empty arm; the bind-all aliases the subject temp.
        if let Some(var) = rest_bind {
            self.value_of.insert(var, subj);
        }
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let rest_obj = match self.lower_heap_result_arm(rest_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_rest: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(rest_obj) });
        // ELSE (len == 0): the `[]` arm.
        let live_after_rest: Vec<ValueId> = self.live_heap_handles.clone();
        let empty_obj = match self.lower_heap_result_arm(empty_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_empty: Vec<ValueId> = live_after_rest
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        // Release parity across the arms (the lower_heap_result_if_inner discipline).
        for x in &consumed_by_rest {
            if !consumed_by_empty.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_empty {
            if !consumed_by_rest.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(empty_obj) });
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
        // The subject must be a registered custom variant; clone its layout out of the borrow.
        let type_name = self.custom_variant_type_name(&subject.ty)?;
        let layout = self.variant_layouts.by_type.get(&type_name)?.clone();
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
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // A DEFERRED-Opaque subject is an EMPTY block: reading its tag would take a wrong
        // arm silently (the record-ctor mt2 miscompile) — decline (the tail walls honestly).
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // A HEAP result over an OWNED subject temp would overlap the owned-subject borrow with the
        // arm's heap move-out (the cert rejects it). Subject-drop-before-arms is ADT brick 4b —
        // for now WALL it (a borrowed param/var subject, the recursive-to_string case, proceeds).
        if is_heap_ty(result_ty) && self.live_heap_handles.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        // Read the tag from slot 0, then emit the per-arm if-chain.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 });
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
                                binds.push((1 + i, *var, false))
                            }
                            // ANY heap field (`String`, a nested VARIANT, a `List[…]` —
                            // `ArrV(xs) => for x in xs`, the gguf ValArray consumer — Bytes,
                            // Matrix) binds as a BORROW of the slot handle: the subject owns
                            // it, a move-out auto-Dups, a borrow-pass just reads. The bind is
                            // type-agnostic (a slot-handle load); what the ARM does with it is
                            // gated by the arm-body lowering as usual.
                            IrPattern::Bind { var, ty } if is_heap_ty(ty) => {
                                binds.push((1 + i, *var, true))
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
                                binds.push((1 + idx, *var, false))
                            }
                            Some(IrPattern::Bind { var, ty }) if is_heap_ty(ty) => {
                                binds.push((1 + idx, *var, true))
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
            for (slot, var, is_heap) in binds {
                let off = layout::slot_offset(*slot) as i64;
                if *is_heap {
                    // BORROW the slot handle: the subject owns the String; a move-out auto-Dups
                    // in `lower_heap_result_arm`, a consuming re-use Dups in `lower_owned_heap_field`.
                    let p = self.load_at_offset(h, off, crate::PrimKind::LoadHandle);
                    self.param_values.insert(p);
                    self.value_of.insert(*var, p);
                } else {
                    let payload =
                        self.load_at_offset(h, off, crate::PrimKind::Load { width: 8 });
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
        let tag = self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 });
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
        self.lower_branch_arm(None, body)?;
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
