impl LowerCtx {

    /// The SUBJECT phase of [`Self::try_lower_variant_value_match`]: materialize/
    /// borrow + track the match subject (effect-result, self-host call, user call,
    /// member, tracked var …) and classify its Option/Result repr. Returns `None`
    /// AFTER rolling back to the given marks (the caller's rollback discipline).
    /// Verbatim text move (#781).
    fn variant_match_subject(
        &mut self,
        subject: &IrExpr,
        ops_mark: usize,
        lhh_mark: usize,
    ) -> Option<(ValueId, bool, bool, bool)> {
        // Roll back PERSISTENT side effects too: a probed subject containing a lambda
        // LIFTS it (self.lifted), and an abandoned probe would leave a dead lifted fn
        // whose CallFn ops double-count the caps gate's mir tally (mir > ir breach).
        let lifted_mark = self.lifted.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.lifted.truncate(lifted_mark);
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
        if (is_self_host_result_call(subject) && !Self::is_heap_ok_result(&subject.ty))
            || (is_named_call
                && crate::lower::is_result_ty(&subject.ty)
                && !Self::is_heap_ok_result(&subject.ty))
            // An EFFECT-result subject (process.kill / RuntimeCall) with a SCALAR-Ok / heap-Err
            // Result is tracked the SAME as a scalar self-host/Named result: len-as-tag @4, Err arm
            // binds slot-0 String, subject drops via DropListStr (the case-A heap_elem_lists below).
            // The self-host list is guarded by TYPE too: `result.collect`/`result.map` are listed
            // there but return a HEAP-Ok Result for heap payloads — cap-as-tag, NOT len-as-tag —
            // which the str-result branch below now tracks (reading @4 misdispatched EVERY heap-Ok
            // collect to the Err arm — the parse_all List[Int]-as-List[String] garbage join).
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
            // Any OTHER self-host Module call with a HEAP-Ok Result (`result.collect` /
            // `result.map` over a heap payload — listed len-as-tag but cap-as-tag for these
            // instantiations): TYPE decides the repr, not the list. Every heap-Ok Result is
            // BUILT cap-as-tag (the ok()/err() ctors' materialize_result_str layout), so the
            // read side must agree universally.
            || (is_self_host_result_call(subject) && Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results_str.insert(subj);
            self.track_heap_ok_result_subject_drop(subj, &subject.ty);
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
        Some((subj, is_option, is_result_str, is_result))
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
        if !effect_unwrap_admitted(&subject.ty, &self.variant_layouts)
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
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let rollback = |s: &mut Self| {
            s.ops.truncate(ops_mark);
            s.lifted.truncate(lifted_mark);
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

    /// Is the Map KEY type (of the first-arg/result Map) a NULLARY-ONLY variant
    /// (every case fieldless — `Direction`)? Gates the `_vtag` tag-normalized map
    /// family in `list_heap_call_name` (a free fn without layout access).
    pub(crate) fn map_key_is_nullary_variant(&self, arg_tys: &[Ty], result_ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let probe = |t: &Ty| {
            matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2
                    && self
                        .custom_variant_type_name(&a[0])
                        .and_then(|n| self.variant_layouts.by_type.get(&n).cloned())
                        .is_some_and(|l| {
                            !l.cases.is_empty() && l.cases.iter().all(|c| c.fields.is_empty())
                        }))
        };
        arg_tys.first().is_some_and(probe) || probe(result_ty)
    }

    /// Is the Map KEY type an ALL-Int/Bool-field record (`Color { r, g, b }`)?
    /// Gates the `_srec` string-normalized map family (a Float field's
    /// bits-to-string would split -0.0/NaN from native's f64 eq — excluded).
    pub(crate) fn map_key_is_scalar_record(&self, arg_tys: &[Ty], result_ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let probe = |t: &Ty| {
            matches!(t, Ty::Applied(TypeConstructorId::Map, a)
                if a.len() == 2
                    && self.custom_variant_type_name(&a[0]).is_none()
                    && self
                        .aggregate_field_tys(&a[0])
                        .is_some_and(|(_, tys)| {
                            !tys.is_empty()
                                && tys.iter().all(|t| matches!(t, Ty::Int | Ty::Bool))
                        }))
        };
        arg_tys.first().is_some_and(probe) || probe(result_ty)
    }

    /// The C-015 STRING-FIELD-record key/element intercept: `Map[P, <scalar|String>]`
    /// / `Set[P]` / `list.unique(List[P])` where P is a declared record of String +
    /// Int/Bool fields (≥1 String — the all-scalar case is the `_srec` family's) —
    /// routed to the GENERATED `__krec_*` twins (drop_sources.rs): the key normalizes
    /// INJECTIVELY into a String (length-prefixed content), and the backing container
    /// is the proven `_str`/`_skv` family. Lookup/build fns only — an
    /// iteration-returning fn would surface the normalized strings and keeps its wall.
    pub(crate) fn krec_call_name(
        &self,
        module: &str,
        func: &str,
        arg_tys: &[Ty],
        result_ty: &Ty,
    ) -> Option<String> {
        // Pattern-1 module-name router (codopsy8 complexity sweep): the 3 arms below are
        // independent, self-contained (`&self`, read-only) classifications, called on the
        // SAME `module` dispatch as the original `match module { .. }` — a pure text-move
        // split, no logic change. The shared `strrec` closure is now the `krec_strrec`
        // method (callable from every arm's own function, not just a closure capture).
        match module {
            "map" => self.krec_call_name_map(func, arg_tys, result_ty),
            "set" => self.krec_call_name_set(func, arg_tys, result_ty),
            "list" if func == "unique" => self.krec_call_name_list_unique(arg_tys),
            _ => None,
        }
    }

    /// Shared by every arm of [`Self::krec_call_name`]: is `t` a declared record of
    /// String + Int/Bool fields (≥1 String — the all-scalar case is the `_srec` family's)?
    /// If so, its normalized `__krec_*` key is the type name. Extracted from
    /// `krec_call_name`'s original `strrec` closure (codopsy8 complexity sweep). Verbatim.
    fn krec_strrec(&self, t: &Ty) -> Option<String> {
        let Ty::Named(n, _) = t else { return None };
        if self.custom_variant_type_name(t).is_some() {
            return None;
        }
        let (_, tys) = self.aggregate_field_tys(t)?;
        (!tys.is_empty()
            && tys.iter().all(|f| matches!(f, Ty::Int | Ty::Bool | Ty::String))
            && tys.iter().any(|f| matches!(f, Ty::String)))
        .then(|| n.as_str().to_string())
    }

    /// Extracted from `krec_call_name` (codopsy8 complexity sweep, the `"map"` arm):
    /// `Map[P, <scalar|String>]` where P is a String-field record — routes to the
    /// GENERATED `__krec_map_*` twins, keyed by the record name AND the value class
    /// (`iv` scalar / `sv` String, since the backing container differs). `len` reads the
    /// BACKING flavor's header directly rather than a `__krec_*` name. Verbatim.
    fn krec_call_name_map(&self, func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let probe = |t: &Ty| -> Option<(String, Ty)> {
            let Ty::Applied(TypeConstructorId::Map, a) = t else { return None };
            if a.len() != 2 {
                return None;
            }
            Some((self.krec_strrec(&a[0])?, a[1].clone()))
        };
        let (rname, vty) = arg_tys.first().and_then(&probe).or_else(|| probe(result_ty))?;
        let vclass = match vty {
            Ty::Int | Ty::Bool => "iv",
            Ty::String => "sv",
            _ => return None,
        };
        match func {
            "from_list" | "set" | "get" | "contains" => {
                Some(format!("__krec_map_{func}_{rname}_{vclass}"))
            }
            // len reads the BACKING flavor's header directly.
            "len" => Some(if vclass == "iv" { "map.len_skv".to_string() } else { "map.len_str".to_string() }),
            _ => None,
        }
    }

    /// Extracted from `krec_call_name` (codopsy8 complexity sweep, the `"set"` arm):
    /// `Set[P]` where P is a String-field record — routes to the GENERATED
    /// `__krec_set_*` twins, keyed by the record name. Verbatim.
    fn krec_call_name_set(&self, func: &str, arg_tys: &[Ty], result_ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let probe = |t: &Ty| -> Option<String> {
            let Ty::Applied(TypeConstructorId::Set, a) = t else { return None };
            if a.len() != 1 {
                return None;
            }
            self.krec_strrec(&a[0])
        };
        let rname = arg_tys.first().and_then(&probe).or_else(|| probe(result_ty))?;
        match func {
            "from_list" | "contains" | "insert" => Some(format!("__krec_set_{func}_{rname}")),
            _ => None,
        }
    }

    /// Extracted from `krec_call_name` (codopsy8 complexity sweep, the `"list" if func ==
    /// "unique"` arm): `list.unique(List[P])` where P is a String-field record — routes to
    /// the GENERATED `__krec_list_unique_*` twin. An UNANNOTATED record literal keeps its
    /// STRUCTURAL type (the r5 lesson) — its block lays fields in SOURCE order, so the norm
    /// is generated against the structural shape, keyed by the anon hash, rather than
    /// through `krec_strrec` (which requires a declared `Named` record). Verbatim.
    fn krec_call_name_list_unique(&self, arg_tys: &[Ty]) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Some(Ty::Applied(TypeConstructorId::List, a)) = arg_tys.first() else { return None };
        if a.len() != 1 {
            return None;
        }
        if let Ty::Record { fields } = &a[0] {
            if !fields.is_empty()
                && fields.iter().all(|(_, t)| matches!(t, Ty::Int | Ty::Bool | Ty::String))
                && fields.iter().any(|(_, t)| matches!(t, Ty::String))
            {
                return Some(format!(
                    "__krec_list_unique_{}",
                    crate::lower::anon_record_drop_name(fields)
                ));
            }
            return None;
        }
        let rname = self.krec_strrec(&a[0])?;
        Some(format!("__krec_list_unique_{rname}"))
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
    /// TAIL-VALUE match over an `Option[<heap>]` subject with HEAP-result arms — the
    /// Option twin of [`Self::try_lower_result_match_value`] (the shape its scalar-payload
    /// self-gate called "the true Camp-4 frontier"):
    ///   `match acc { none => none, some(stack) => <heap arm> }`  (is_balanced's fold step)
    /// Same merge discipline as the Result opener: subject via `lower_call_args` (a borrowed
    /// param stays caller-owned; an owned temp joins the scope epilogue), tag = len@4 with
    /// OPTION polarity (THEN tag≠0 = Some, ELSE = None), the Some payload bound as a
    /// slot-0 HANDLE BORROW (`param_values`, not a second owner — `lower_heap_result_arm`
    /// Dups any borrowed payload it re-wraps), arms via `lower_heap_result_arm` with the
    /// release-parity sweep.
    pub(crate) fn try_lower_option_match_value(
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
        // HEAP payloads only — a scalar payload already executes via
        // `try_lower_variant_value_match` (tried before this in every caller).
        match &subject.ty {
            Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && is_heap_ty(&a[0]) => {}
            _ => return None,
        }
        let mut some_arm: Option<(&IrExpr, Option<VarId>)> = None;
        let mut none_arm: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, .. } => Some(*var),
                        IrPattern::Wildcard => None,
                        _ => return None,
                    };
                    if some_arm.is_some() {
                        return None;
                    }
                    some_arm = Some((&arm.body, bind));
                }
                IrPattern::None => {
                    if none_arm.is_some() {
                        return None;
                    }
                    none_arm = Some(&arm.body);
                }
                IrPattern::Wildcard => {
                    if none_arm.is_some() {
                        return None;
                    }
                    none_arm = Some(&arm.body);
                }
                _ => return None,
            }
        }
        let ((some_body, some_bind), none_body) = match (some_arm, none_arm) {
            (Some(s), Some(n)) => (s, n),
            _ => return None,
        };
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
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
        if self.deferred_opaque_binds.contains(&subj) {
            self.ops.truncate(ops_mark);
            self.lifted.truncate(lifted_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: tag, dst: Some(dst) });
        // THEN (tag != 0 = Some): payload = the slot-0 HANDLE, a BORROW of the subject's slot.
        if let Some(var) = some_bind {
            let payload = self.load_at_offset(h, 12, PrimKind::LoadHandle);
            self.param_values.insert(payload);
            self.value_of.insert(var, payload);
        }
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let some_obj = match self.lower_heap_result_arm(some_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_some: Vec<ValueId> =
            outer.iter().copied().filter(|x| !self.live_heap_handles.contains(x)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(some_obj) });
        // ELSE (tag == 0 = None).
        let live_after_some: Vec<ValueId> = self.live_heap_handles.clone();
        let none_obj = match self.lower_heap_result_arm(none_body, result_ty) {
            Some(v) => v,
            _ => {
                self.ops.truncate(ops_mark);
                self.lifted.truncate(lifted_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let consumed_by_none: Vec<ValueId> = live_after_some
            .iter()
            .copied()
            .filter(|x| !self.live_heap_handles.contains(x))
            .collect();
        // Release parity across the arms (the lower_heap_result_if_inner discipline).
        for x in &consumed_by_some {
            if !consumed_by_none.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.push(op);
            }
        }
        for x in &consumed_by_none {
            if !consumed_by_some.contains(x) {
                let op = self.drop_op_for(*x);
                self.ops.insert(else_marker_at, op);
            }
        }
        self.ops.push(Op::EndIf { val: Some(none_obj) });
        Some(dst)
    }
}
