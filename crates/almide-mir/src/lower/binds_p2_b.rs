impl LowerCtx {

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the concat/interp/str-list/option-ctor quick wins, verbatim. `Ok(true)` means the
    /// caller already bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_quick(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // `let s = a + b` — a string concat EXECUTES to a fresh owned String (via the
        // self-host __str_concat), held by the binding and dropped at scope end.
        if let Some(dst) = self.try_lower_concat_str(value) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            return Ok(true);
        }
        // `let ys = xs + [7]` — a SCALAR-element list concat EXECUTES to a fresh owned list
        // (via the self-host __list_concat), held by the binding and dropped at scope end.
        // The result is a REAL, POPULATED block (len(a)+len(b) copied slots), so a later
        // `ys[i]` may index it directly. (A heap-element list concat returns None and falls
        // through to the deferred Opaque.)
        if let Some(dst) = self.try_lower_concat_list(value) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            self.materialized_lists.insert(dst);
            return Ok(true);
        }
        // `let s = "x=${n} y=${t}"` — a STRING INTERPOLATION over the executable
        // subset (Lit / String Var/LitStr / Int Var/LitInt parts) EXECUTES to a
        // fresh owned String via the same __str_concat chain, byte-matching v0;
        // held by the binding and dropped at scope end. An interp with a compound
        // (`${list}`) or call (`${f()}`) operand falls through to the deferred
        // `Alloc{Opaque}` below, unchanged.
        if let IrExprKind::StringInterp { parts } = &value.kind {
            if let Some(dst) = self.try_lower_string_interp(parts) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            // STRICT value mode: an interp the executable subset declined (a
            // BLOCK-bodied operand — `${int.to_string({ let x = …; x * 3 })}` —
            // or another non-lowerable piece) must NOT defer to the Opaque
            // below: the binding reads back as an EMPTY string while native
            // prints the real text — a silent wrong value on the verified
            // default (the C-136 elide family, interp edition). REFUSE — the
            // fn walls and v0 emits the correct bytes.
            if crate::lower::strict_values() {
                return Err(LowerError::Unsupported(
                    "string interpolation outside the executable subset — \
                     deferring it would read back an empty string not in this \
                     brick"
                        .into(),
                ));
            }
        }
        // `let xs = ["a" + "b", "c"]` — a List[String] literal with fresh-owned elements
        // (the heap-container-element concat position; the −214 caps recovery).
        if let Some(dst) = self.try_lower_str_list_literal(value) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            return Ok(true);
        }
        // An Option ctor in the executable subset (`Some(scalar)` / `None`) is
        // MATERIALIZED + tracked so a later `match` over the bound var executes;
        // everything else is the deferred fresh `Alloc` (value-semantics).
        if let Some(dst) = self.try_lower_option_ctor(value, ty) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            return Ok(true);
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the OptionSome/ResultOk/ResultErr honest-wall safety net, verbatim (the outer `if
    /// let` guard now doubles as the "not applicable" fallthrough). `Ok(true)` means the
    /// caller already bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_variant_honest_wall(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // HONEST-WALL SAFETY NET: a `some(<list>)` / `ok(<list>)` whose LIST payload the ctor
        // materializer DECLINED (an exotic element the scalar/String/literal arms don't cover —
        // e.g. a computed List[record]/List[List]) must NOT fall to the deferred Opaque `Alloc`
        // below, which would read `none` / `ok([])` (the some(computed)/ok(computed) silent
        // miscompile the adversarial fuzz surfaced). Wall instead — a wall is always safe, a
        // wrong byte never is.
        if let IrExprKind::OptionSome { expr }
        | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } = &value.kind
        {
            use almide_lang::types::constructor::TypeConstructorId;
            if matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::List, _) | Ty::Applied(TypeConstructorId::Map, _))
            {
                return Err(LowerError::Unsupported(
                    "some/ok of a list or map payload outside the executable subset cannot be \
                     faithfully materialized in this brick (e.g. an empty `[:]` — would defer \
                     to an empty container)"
                        .into(),
                ));
            }
            // A CALL payload (`ok(result.unwrap_or(…))` — the C-149
            // nested-share chain): ANF-materialize the call into a synth temp via
            // the SAME `lower_bind` path a `let tmp = call` takes (tracked, typed
            // drop, read shapes seeded — the lower_heap_extraction Call-container
            // discipline), then rebuild the ctor over the temp VAR and retry the
            // ctor materializer — its Var arms admit the payload with the share
            // (Dup) discipline. A payload the Var arms still decline WALLS (the
            // deferred Opaque would read `ok(0)` while native printed the err —
            // the C-138 family; a wall is always safe, a wrong byte never).
            // SCALAR call payloads take the same route (C-158): requiring a heap
            // payload here let `ok(<un-lowerable Float combinator chain>)` skip
            // to the deferred Opaque — an EMPTY block the formatter read as
            // `ok(0)` while native printed the real value (differential-fuzz
            // seed 1784512190387680000 index 74, a silent wrong value).
            // TUPLE literal payloads too (`ok((0.3, 1000000))` — the (Float, Int)
            // mixed-scalar payload the ctor materializer declines): deferred, the
            // `result.flat_unwrap_or` twin read the EMPTY block's payload out of
            // bounds — a runtime abort native never had (RunFailureDivergence).
            // And IF-merged payloads (`ok(if c then some("a") else some("b"))`):
            // deferred, the downstream unwrap misread the empty block as None and
            // took the fallback while native returned the real payload — the ANF
            // bind routes them through the proven heap-result-if bind machinery
            // (C-106) instead.
            if matches!(
                expr.kind,
                IrExprKind::Call { .. } | IrExprKind::Tuple { .. } | IrExprKind::If { .. }
            ) {
                let payload_ty = expr.ty.clone();
                let payload = (**expr).clone();
                let tmp = self.fresh_synth_var();
                self.lower_bind(tmp, &payload_ty, &payload)?;
                // The ANF bind itself may have DEFERRED (an Opaque skeleton —
                // e.g. a capturing-closure element): retrying the ctor over it
                // would wrap the same empty block. Wall instead.
                if self
                    .value_of
                    .get(&tmp)
                    .is_some_and(|v| self.deferred_opaque_binds.contains(v))
                {
                    return Err(LowerError::Unsupported(
                        "some/ok/err payload whose materialization deferred cannot \
                         be faithfully wrapped in this brick (walled, not read as a \
                         zeroed ctor)"
                            .into(),
                    ));
                }
                let synth = IrExpr {
                    kind: IrExprKind::Var { id: tmp },
                    ty: payload_ty,
                    span: payload.span,
                    def_id: None,
                };
                let rebuilt_kind = match &value.kind {
                    IrExprKind::OptionSome { .. } => {
                        IrExprKind::OptionSome { expr: Box::new(synth) }
                    }
                    IrExprKind::ResultOk { .. } => {
                        IrExprKind::ResultOk { expr: Box::new(synth) }
                    }
                    _ => IrExprKind::ResultErr { expr: Box::new(synth) },
                };
                let rebuilt = IrExpr {
                    kind: rebuilt_kind,
                    ty: value.ty.clone(),
                    span: value.span,
                    def_id: value.def_id,
                };
                if let Some(dst) = self.try_lower_option_ctor(&rebuilt, ty) {
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(true);
                }
                return Err(LowerError::Unsupported(
                    "some/ok/err of an un-admitted heap call payload cannot be \
                     faithfully materialized in this brick (walled, not read as a \
                     zeroed ctor)"
                        .into(),
                ));
            }
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the scalar/heap tuple construction, verbatim. `Ok(true)` means the caller already
    /// bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_tuple(&mut self, var: VarId, value: &IrExpr) -> Result<bool, LowerError> {
        // A scalar-field tuple `(a, b)` of NON-LITERAL fields (vars / scalar exprs) — a
        // literal `(3, 7)` is already an `Init::IntList` below. Construct the 2-slot block
        // and store each field's computed value (the tuple-machinery construction sibling
        // of the precise destructure). A heap-field tuple falls through to the Opaque alloc.
        if let IrExprKind::Tuple { elements } = &value.kind {
            if let Some(dst) = self.try_lower_scalar_tuple_construct(elements) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            // A HEAP-element tuple (`(1, "a")`, `(p, 9)`) — materialize the mixed block
            // + track its heap-slot mask, so `t.0`/`${tuple}` execute and the block (with
            // its owned heap elements) is reclaimed by a masked recursive drop. Rolls back
            // on a non-lowerable element (then Opaque → the Display walls).
            let mark = self.ops.len();
            let lhh_mark = self.live_heap_handles.len();
            if let Some(dst) = self.try_lower_tuple_construct(elements) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the scalar/heap record construction, verbatim. `Ok(true)` means the caller already
    /// bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_record(&mut self, var: VarId, value: &IrExpr) -> Result<bool, LowerError> {
        // A SCALAR-only record `R { x: 3, y: 4 }` — build the tight-packed,
        // width-aware block + store each field at its layout slot (the VALUE
        // MODEL: `r.x`/`r.y` read back exactly what was stored). A HEAP-field
        // record (a String/List field) needs an ownership-aware recursive drop
        // this brick does not have, so it falls through to the deferred Opaque
        // (which the field-access path then WALLS rather than mis-reads).
        if let IrExprKind::Record { .. } = &value.kind {
            if let Some(dst) = self.try_lower_scalar_record_construct(value) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            // A VARIANT record-ctor literal (`Data { … }`) outside the builder's
            // subset must WALL, not defer: a deferred Opaque variant read through a
            // CALL arg (`tree_sum(t)`) bypasses the match-side deferred gate and the
            // callee reads a garbage tag — the same miscompile class the Call-ctor
            // bind gate above already errors on.
            if let IrExprKind::Record { name: Some(n), .. } = &value.kind {
                if self.variant_layouts.ctor_to_type.contains_key(n.as_str()) {
                    return Err(LowerError::Unsupported(format!(
                        "variant record-ctor `{}` bound to a let/var cannot be \
                         faithfully materialized in this brick (a field outside the \
                         ctor subset)",
                        n.as_str()
                    )));
                }
            }
            // A record with one or more HEAP fields (`R { name: "x", n: i }`) —
            // materialize the mixed scalar+heap block + track its heap-slot mask, so a
            // `r.n` scalar read AND a `r.name` heap read execute and the block (with its
            // owned heap fields) is reclaimed by a masked recursive drop. Rolls back on
            // a partially-lowered out-of-subset field (a heap-returning-call field).
            let mark = self.ops.len();
            let lhh_mark = self.live_heap_handles.len();
            if let Some(dst) = self.try_lower_record_construct(value) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the spread-record construction, verbatim. `Ok(true)` means the caller already
    /// bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_spread_record(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // A SPREAD record `R { ...base, f: override }` — build a fresh block of the
        // same layout, COPYING each non-overridden field from `base` (a scalar load,
        // a heap-handle Dup so both records own a distinct reference) and storing the
        // overrides. So `let b2 = Box { ...b, value: 8 }` reads `b2.value=8
        // b2.label=old` while `b.label` still reads `old`. Rolls back to the deferred
        // Opaque (whose field reads WALL) on a non-materialized base / out-of-subset
        // override — never wrong bytes.
        if let IrExprKind::SpreadRecord { .. } = &value.kind {
            let mark = self.ops.len();
            let lhh_mark = self.live_heap_handles.len();
            if let Some(dst) = self.try_lower_spread_record_construct(value) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(true);
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh_mark);
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the scalar-list construct + non-empty heap-element-list honest wall, verbatim.
    /// `Ok(true)` means the caller already bound `var` and should return immediately.
    fn try_lower_bind_heap_fresh_scalar_list(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // A scalar `List[Int/Float/Bool]` literal with COMPUTED elements (`[1.0, inf, 0.5]`,
        // `[a, a]`) — build the block + store each slot (an all-literal list is the IntList
        // path in `alloc_init` below; a computed element can't fold to a constant).
        if let Some(dst) = self.try_lower_scalar_list_construct(value) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            return Ok(true);
        }
        // A NON-EMPTY `List[heap]` LITERAL that NONE of the materialization paths above
        // could build — a list of heap-FIELD records/tuples (`[R{name:String,…}, …]`), a
        // list of lists, a list of heap call-results. The flat `Init::Opaque` fallback
        // below would emit an EMPTY len-0 block (`list_new(0, …)`); a later `list.map` /
        // `list.sort_by` / `xs[i]` over it then silently reads NOTHING = wrong/empty bytes.
        // (A heap-field-record element needs a TWO-LEVEL recursive drop — the list frees
        // each record, each record frees its String fields — which the single-level
        // `DropListStr` cannot express without a new ownership op; that is the
        // nested-ownership frontier, out of this brick.) WALL the function cleanly instead
        // of mis-valuing it — the render discards it (no invalid wasm, no empty output).
        // GATED to a NON-EMPTY heap-element `List` LITERAL (an empty `[]`, a scalar list,
        // and a `List[String]`/scalar-aggregate list are all handled above), so this only
        // rejects the genuinely-unmaterializable case.
        if let IrExprKind::List { elements } = &value.kind {
            use almide_lang::types::constructor::TypeConstructorId;
            let heap_elem_list = matches!(ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
            if heap_elem_list && !elements.is_empty() {
                // A List[Record] literal materializes via the record-list builder (drop →
                // $__drop_list_<R>); other nested-ownership element lists stay walled.
                if let Some(dst) = self.try_lower_record_list_literal(value) {
                    self.value_of.insert(var, dst);
                    return Ok(true);
                }
                return Err(LowerError::Unsupported(
                    "non-empty List[heap] literal with nested-ownership elements \
                     (a heap-field record/tuple, a list, a call result) cannot be \
                     faithfully materialized in this brick (walled, not emitted empty)"
                        .into(),
                ));
            }
        }
        Ok(false)
    }

    /// Extracted from `Self::lower_bind_heap_fresh` (second-round split, cog reduction):
    /// the terminal `Alloc{Opaque}`-or-real-list fallback, verbatim.
    fn lower_bind_heap_fresh_opaque(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let dst = self.fresh_value();
        let repr = repr_of(ty)?;
        let init = alloc_init(value);
        // A NON-EMPTY SCALAR-element List literal that did NOT fold to a real
        // `Init::IntList` (a computed element the builders above declined — e.g. a
        // nested inadmissible-HOF chain, fuzz B-198/659): the flat Opaque reads as
        // an EMPTY list at any observation (`${r4}` printed `[]` while native
        // printed the values — the silent-wrong-value class). WALL instead — the
        // heap-element twin at the gate above already does; this is its scalar twin.
        if let IrExprKind::List { elements } = &value.kind {
            use almide_lang::types::constructor::TypeConstructorId;
            let scalar_elem_list = matches!(ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
            if scalar_elem_list && !elements.is_empty() && matches!(init, Init::Opaque) {
                return Err(LowerError::Unsupported(
                    "non-empty scalar-element list literal with an element outside \
                     the executable subset cannot be faithfully materialized in this \
                     brick (walled, not emitted empty)"
                        .into(),
                ));
            }
        }
        // An all-literal `Init::IntList` is a REAL, POPULATED block (every slot a constant) —
        // admit a direct `xs[i]` bounds-checked load over it. An `Init::Opaque` (a deferred /
        // unsupported value) is NOT tracked: indexing it would trap on cap 0.
        let real_list = matches!(init, Init::IntList(_));
        self.value_of.insert(var, dst);
        self.ops.push(Op::Alloc { dst, repr, init });
        self.live_heap_handles.push(dst);
        if real_list {
            self.materialized_lists.insert(dst);
        }
        self.record_elided_calls(value);
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_call_named(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &value.kind else { unreachable!() };
        // A custom-variant CONSTRUCTOR `let t = Num(9)` (ADT brick 2) is NOT a call —
        // build the tagged value-model block (tag@slot0 + scalar fields@slot1..), bound
        // + dropped at scope end (cert `i` + `d`, like the scalar-record bind). Must
        // precede the CallFn emission, which would emit a dangling `(call $Num)`. A
        // heap/recursive ctor field is ADT brick 5 → WALL (never a wrong-bytes block).
        if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) {
            if let Some(dst) = self.try_lower_variant_ctor(value) {
                self.value_of.insert(var, dst);
                self.live_heap_handles.push(dst);
                return Ok(());
            }
            return Err(LowerError::Unsupported(format!(
                "variant constructor `{}` bound to a let/var cannot be faithfully \
                 materialized in this brick (a heap/recursive field — ADT brick 5)",
                name.as_str()
            )));
        }
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        // A function-VALUED result (`let f = mk()`) is a CLOSURE BLOCK — the uniform
        // heap representation (`repr_of(Ty::Fn)` = Ptr), owned + dropped at scope end
        // like any heap result; `closure_values` (below) makes a later `f(args)`
        // dispatch through it.
        let repr = repr_of(ty)?;
        self.value_of.insert(var, dst);
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: name.as_str().to_string(),
            args: lowered,
            result: Some(repr),
        });
        self.live_heap_handles.push(dst);
        self.seed_call_named_heap_drop_route(dst, ty);
        self.seed_call_named_heap_read_shape(dst, ty);
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap_call_named` (second-round split, cog
    /// reduction): the mutually-exclusive drop-route selection for a Named-call's fresh
    /// heap result, verbatim (the original `if/else if` chain).
    fn seed_call_named_heap_drop_route(&mut self, dst: ValueId, ty: &Ty) {
        if !self.seed_call_named_heap_drop_route_a(dst, ty) {
            self.seed_call_named_heap_drop_route_b(dst, ty);
        }
    }

    /// Extracted from `Self::seed_call_named_heap_drop_route` (third-round split, cog
    /// reduction): the first half of the mutually-exclusive `if/else if` chain, verbatim.
    /// Returns whether a branch matched (the caller then skips the second half).
    fn seed_call_named_heap_drop_route_a(&mut self, dst: ValueId, ty: &Ty) -> bool {
        // Guard-clause flattening (codopsy7 max-depth sweep): the original body was a single
        // `if/else if` chain over mutually-exclusive type-shape checks — each `else if` adds a
        // full nesting level to the naive depth counter even though the arms never interact.
        // Rewritten as independent `if COND { ...; return true; }` guards, checked in the SAME
        // order as before, so the first-matching-wins semantics are byte-identical; only the
        // LAST guard's `return true` is replaced by falling through to the final `true`. Pure
        // control-flow-equivalent transform, no logic change.
        if crate::lower::is_res_intlist_strlist_ty(ty) {
            // `result.collect` — Result[List[Int], List[String]]: the TAG-AWARE
            // generated `$__drop_res_ilsl` (Err → recursive string free, Ok → flat;
            // either flat class would leak or double-free one side).
            self.variant_drop_handles.insert(dst, "res_ilsl".to_string());
            self.materialized_results_str.insert(dst);
            return true;
        }
        if crate::lower::is_list_list_str_ty(ty) {
            self.list_list_str_lists.insert(dst);
            return true;
        }
        if crate::lower::is_list_str_str_ty(ty) {
            // `List[(String,String)]` (map.entries) — DropListStrStr frees each tuple's two
            // Strings; the flat heap_elem_lists DropListStr would leak them (a render loop OOMs).
            self.str_str_elem_lists.insert(dst);
            return true;
        }
        if crate::lower::is_list_int_str_ty(ty) {
            // `List[(Int,String)]` (list.enumerate) — recursive `$__drop_list_int_str` (rc_dec
            // each tuple's String); the flat heap_elem_lists DropListStr would leak them.
            self.variant_drop_handles.insert(dst, "list_int_str".to_string());
            return true;
        }
        if crate::lower::is_map_ivh_ty(ty) {
            // `Map[Int, String]` — `$__drop_map_ivh` rc_decs each OWNED value slot.
            self.variant_drop_handles.insert(dst, "map_ivh".to_string());
            return true;
        }
        if crate::lower::is_map_fn_ty(ty) {
            // `Map[String, <Fn>]` — `$__drop_map_mclo` frees each value via
            // `__drop_closure` (the hval flat rc_dec would leak captured env).
            self.variant_drop_handles.insert(dst, "map_mclo".to_string());
            return true;
        }
        if crate::lower::is_map_hval_ty(ty) {
            // `Map[String, List[scalar]]` — `$__drop_map_hval` rc_decs all 2n slots.
            self.variant_drop_handles.insert(dst, "map_hval".to_string());
            return true;
        }
        if let Some(hname) = self.map_named_value_drop(ty) {
            // `Map[String, <record/variant>]` — the desugared map literal's
            // from_list result (type-driven sweep; see `map_named_value_drop`).
            self.variant_drop_handles.insert(dst, hname);
            return true;
        }
        if crate::lower::is_map_msv_ty(ty) {
            // `Map[String, Map[String, String]]` — `$__drop_map_msv` sweeps each
            // last-ref inner map's String slots (a flat rc_dec would leak them).
            self.variant_drop_handles.insert(dst, "map_msv".to_string());
            return true;
        }
        false
    }

    /// Extracted from `Self::seed_call_named_heap_drop_route` (third-round split, cog
    /// reduction): the second half of the mutually-exclusive `if/else if` chain, verbatim
    /// (only reached when the first half's chain did not match).
    fn seed_call_named_heap_drop_route_b(&mut self, dst: ValueId, ty: &Ty) {
        // Guard-clause flattening (codopsy7 max-depth sweep, same rationale as `_a` above):
        // independent `if COND { ...; return; }` guards in the SAME order as the original
        // `if/else if` chain — first-match-wins semantics preserved exactly.
        if crate::lower::is_map_mlo_ty(ty) {
            // `Map[String, List[Option[Int]]]` — `$__drop_map_mlo` sweeps each
            // last-ref value list's Option slots (a flat rc_dec would leak them).
            self.variant_drop_handles.insert(dst, "map_mlo".to_string());
            return;
        }
        if let Some(rname) = (match ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }) {
            // A `List[<recursive-drop record>]` result (`list.unique` over a
            // String-field record via the `__krec_*` twins): route to the generated
            // `$__drop_list_<R>` (emitted for EVERY recursive-drop record) — the
            // flat per-slot dec freed each element block but LEAKED its String
            // fields (the krec-unique residue).
            self.variant_drop_handles.insert(dst, format!("list_{rname}"));
            return;
        }
        if crate::lower::is_lenlist_list_ty(ty) {
            // `List[Result[_, String]]`/`List[Option[String]]` — the len-loop drop; the
            // flat DropListStr would leak each element's owned payload slots.
            self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
            return;
        }
        if crate::lower::is_opt_list_str_ty(ty) {
            // `Option[List[String]]` (the heap-acc fold value) — physically a 0/1-element
            // List[List[String]]; the nested DropListListStr sweep is its exact free (the
            // flat DropListStr would leak the stack Strings).
            self.list_list_str_lists.insert(dst);
            return;
        }
        if matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
        {
            // `Map[String, <scalar>]` (split layout, @4 = n): the DropListStr sweep
            // rc_decs exactly the n deep-copied key Strings (scalar value slots
            // untouched) — the bare flat rc_dec LEAKED every key copy per bind (a
            // latent leak the map.fold heap-acc loop made observable at a 4MB cap).
            self.heap_elem_lists.insert(dst);
            return;
        }
        if is_heap_elem_list_ty(ty) {
            self.heap_elem_lists.insert(dst);
            return;
        }
        if is_scalar_elem_list_ty(ty) {
            // A user fn returning `List[scalar]` yields a REAL, POPULATED list
            // block (the v1 calling convention — the same argument as the
            // variant/record seeds below; a callee that cannot build one WALLS,
            // and the render rejects the program). Admit a direct `xs[i]`
            // bounds-checked load over the bound result — the C-132 move-mode
            // write-back binds the returned buffer exactly here (`__mp_buf =
            // add_item(data, 1)` then `data = __mp_buf; data[0]`).
            self.materialized_lists.insert(dst);
        }
    }

    /// Extracted from `Self::lower_bind_heap_call_named` (second-round split, cog
    /// reduction): the independent (non-mutually-exclusive) read-shape tracking checks
    /// for a Named-call's fresh heap result, verbatim.
    fn seed_call_named_heap_read_shape(&mut self, dst: ValueId, ty: &Ty) {
        // A user fn returning `List[heap]` (`build_nested() -> List[List[Int]]`) is
        // likewise a REAL, POPULATED nested-ownership block, so admit the element
        // borrow `nested[i]` (LoadHandle at `$elem_addr`) over the bound var — the
        // exact mirror of the Module-call bind's `string.split` registration. Without
        // it, `nested[10]` fell to the container-grain Dup of the WHOLE list and a
        // consumer read the outer block (`list.len(mid)` = the OUTER len — the
        // rc_alloc_stress silent miscompile). Narrowed to `List[heap]` (NOT the
        // broader Option/Result/Map `is_heap_elem_list_ty` also matches) — only a
        // real list is `[i]`-indexable here.
        if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]))
        {
            self.materialized_lists.insert(dst);
        }
        // A `Value` result from a user fn (`let v = parse_number(c, raw)`) drops via the
        // runtime-tag-dispatched `DropValue` — the SAME marking the Module-call bind path does
        // (was missing here, so a let-bound Named-call Value leaked: a parse loop OOMs).
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(dst);
        }
        // A user fn RETURNING a function value (`let f = mk()` / `let f = adder(3)`)
        // yields a CLOSURE BLOCK — a fresh owned heap value (already in the scope-end
        // set like any heap result): track it so a later `f(args)` dispatches through
        // `Op::CallIndirect` via `emit_closure_call`.
        if matches!(ty, Ty::Fn { .. }) {
            self.closure_values.insert(dst);
        }
        // A user function returning Option/Result yields a REAL same-layout variant block
        // (the v1 calling convention — `seed_variant_param`'s contract). SEED its READ-shape
        // so a later `match x { … }` / `x ?? d` over the LET-BOUND var EXECUTES (reads the
        // tag) exactly as the direct-call-arg position already does (`lower_call_args`'s
        // Named arm). Adds ONLY layout knowledge — `dst` is already an owned heap value
        // dropped at scope end, so no ownership/cert change. This is what made
        // `let parsed = parse_oct(d); match parsed { … }` (num_signed_base, after the
        // let-bound-heap-`if` tail-duplication) lower instead of wall.
        if is_variant_ty(ty) {
            self.seed_variant_param(dst, ty);
        } else if let Some((_, tys)) = self.aggregate_field_tys(ty) {
            // A user function returning a RECORD/TUPLE yields a REAL same-layout block (the
            // callee built it via try_lower_record_construct). Seed its READ-shape
            // (materialized_aggregates) so a field read `p.y` loads the real slot instead of
            // falling back to the container-grain Dup (which returns the whole record — the
            // `mk(5).y` empty-string miscompile), AND its heap-slot MASK (record_masks) so the
            // OWNED scope-end drop frees exactly the heap fields (no leak, no double-free).
            let heap_slots: Vec<usize> =
                (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
            self.materialized_aggregates.insert(dst);
            self.record_masks.insert(dst, heap_slots);
            // A record with a Map/List[heap]/record/Value field drops RECURSIVELY ($__drop_<R>),
            // not the flat masked DropListStr (which would leak the nested heap) — route it. An
            // ANONYMOUS record return whose flat one-level mask would leak a nested heap field
            // (`{ data: Bytes, state: Cfb8State }` — aes cfb8) routes to its synthesized
            // `__drop_anonrec_<hash>` (so `state` is freed through `__drop_Cfb8State`).
            if let Some(name) = self.record_or_anon_drop_type_name(ty) {
                self.variant_drop_handles.insert(dst, name);
            }
        }
    }
}
