impl LowerCtx {
    /// C1 DEFUNCTIONALIZATION — inline a `list.map`/`filter`/`fold` with an INLINE-LAMBDA
    /// closure argument as a SPECIALIZED loop at the call site: NO runtime closure, NO
    /// `Op::CallIndirect`, NO lifted `__lambda_*` function. The lambda body is lowered
    /// INLINE per element with its PARAM bound to the element (`let x = elem`) and its
    /// CAPTURES resolved through the EXISTING `value_of` map (an inline / let-bound lambda's
    /// free vars are already in scope at the call site — no env block, no substitution). So
    /// a CAPTURING lambda (`let k = 10; list.map(xs, (x) => x * k)`) WORKS: `k` is just a
    /// `Var` the inlined `x * k` reads through `value_of`, exactly as if hand-written as a
    /// `for x in xs` loop.
    ///
    /// SOUNDNESS by REUSE — the same machinery the for-in/for-list loops already prove
    /// sound (task #67): a real `LoopStart`/`LoopBreakUnless`/`LoopEnd` over a stable i64
    /// index local; the result list is a `DynList`/`DynStr`-grade fresh OWNED block built
    /// exactly like a scalar list LITERAL (`try_lower_scalar_list_slots`); the per-element
    /// body lowers via `lower_scalar_value` (pure, no ownership event), so NO heap temp
    /// crosses the back-edge. The inlined body's calls are REAL IR call nodes that
    /// `count_ir_calls` already counts in-place (the lambda body sits in the IR call-arg the
    /// gate's visitor walks), and the caps fold sees them directly — there is NO
    /// `CallIndirect` conservatism and NO elided marker, so a function stays caps-verified
    /// iff its inlined bodies are pure. A body the scalar subset cannot lower (a `println`
    /// side effect, a heap result) → `None` (rolled back), and the caller keeps the existing
    /// self-host-combinator / WALL path. NARROW to a SCALAR-element source list and a SCALAR
    /// lambda result/element (the dual-oracle subset): a heap element/result needs the
    /// nested-ownership build this slice does not emit, so it WALLS (defers) cleanly.
    ///
    /// Returns the result value (`map`/`filter`: a fresh OWNED scalar `List`; `fold`: a
    /// scalar accumulator carrying no ownership), or `None` (fully rolled back) when out of
    /// subset. The caller (`lower_pure_module_value_call`) treats the `Some` result exactly
    /// like a self-host combinator's: a fresh owned heap list is bound + dropped, a scalar
    /// fold result is bound.
    pub(crate) fn try_lower_defunc_list_hof(
        &mut self,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        // The closure arg index per combinator: map/filter/flat_map/filter_map = arg 1,
        // fold = arg 2 (after init).
        let (xs, lambda_idx, init_idx) = match func {
            "map" | "filter" | "flat_map" | "filter_map" | "find" if args.len() == 2 => {
                (&args[0], 1usize, None)
            }
            "fold" if args.len() == 3 => (&args[0], 2usize, Some(1usize)),
            _ => return None,
        };
        // The CLOSURE arg is an INLINE lambda (`(x) => …`) OR a `Var` statically bound to a let lambda
        // (`let g = (x) => …; xs |> list.map(g)` — the wasm-bindgen generate_dts/esm `sigs` shape, where
        // a flat_map body defines `param_ty` and maps with it). A let-bound lambda is resolved through the
        // EXISTING `lambda_bindings` registry (the same one the C1 direct-call inline uses) and inlined
        // identically — its captures resolve through `value_of` exactly like an inline lambda. A first-
        // class/opaque/FnRef closure is C2 (not inlinable here) → defer to the self-host path / WALL.
        let resolved_lambda: Option<(Vec<(VarId, Ty)>, IrExpr)> = match &args[lambda_idx].kind {
            IrExprKind::Lambda { params, body, .. } => Some((params.clone(), (**body).clone())),
            IrExprKind::Var { id } => self.lambda_bindings.get(id).cloned(),
            _ => None,
        };
        let (params, body) = match &resolved_lambda {
            Some((p, b)) => (p, b),
            None => return None,
        };
        // `list.find` — an EARLY-EXIT scan returning `Option[elem]`, with its OWN gating
        // (the map/filter source/result gates below don't apply to it, so it is dispatched
        // FIRST — placing it after `result_ok` silently killed it once).
        if func == "find" {
            let f_ops = self.ops.len();
            let f_lhh = self.live_heap_handles.len();
            let f_lifted = self.lifted.len();
            let f_vo = self.value_of.clone();
            if let Some(dst) = self.try_lower_defunc_find(xs, params, body, result_ty) {
                self.last_call_had_unlifted_closure = false;
                return Some(dst);
            }
            self.rollback_scalar_loop(f_ops, f_lhh, f_lifted, f_vo);
            return None;
        }
        // A TUPLE-accumulator `fold((<empty-list>, <int-init>), (state, e) => { let (acc, n) = state;
        // (acc + [<elem>], n + <step>) })` returning `(List[T], Int)` — the wasm-bindgen
        // `wasm_record_offsets` shape. The accumulator is a 2-tuple `(List[T], Int)`; the body
        // destructures `state` then returns a tuple whose component0 is a `acc + [<elem>]` list APPEND
        // and component1 a scalar `n + <step>`. The scalar `result_ok` gate below rejects this (a
        // heap-and-not-String accumulator), so handle it HERE with a dedicated loop that carries TWO
        // slots (a List append-accumulator + an Int scalar local) and builds the result tuple ONCE
        // after the loop. The helper does its OWN strict gating + complete rollback (any deviation →
        // None → rolls back → walls, never a wrong-bytes tuple).
        if func == "fold" && args.len() == 3 {
            let tup_mark = self.ops.len();
            let tup_lhh = self.live_heap_handles.len();
            let tup_lifted = self.lifted.len();
            let tup_vo = self.value_of.clone();
            if let Some(dst) = self.try_lower_defunc_tuple_acc_fold(
                xs,
                params,
                body,
                &args[init_idx.unwrap()],
                result_ty,
            ) {
                // The closure was FAITHFULLY inlined — clear the unlifted-closure flag (see the tail
                // of this function) so the bind path treats the tuple block as a genuinely-materialized
                // aggregate, NOT an unfaithful HOF to WALL.
                self.last_call_had_unlifted_closure = false;
                return Some(dst);
            }
            self.rollback_scalar_loop(tup_mark, tup_lhh, tup_lifted, tup_vo);
        }
        // enumerate+map FUSION: `list.map(list.enumerate(real), (entry) => { let (i,key)=entry; <tail> })`
        // → a map-with-index over `real`, binding i=loop-index + key=element, AVOIDING the (Int,String)
        // intermediate list entirely (no enumerate self-host, no new tuple-list drop). Rebind the
        // source/params/body to the fused form + remember the index var (bound to i_v in the inner).
        let fuse_holder: Option<(Vec<(VarId, Ty)>, IrExpr)>;
        let mut fuse_index: Option<VarId> = None;
        // zip+map FUSION second source: `(b_expr, p1_var, t1)` — the loop iterates `a`
        // as the primary source, borrows `b` alongside, binds p1 = b[i] each iteration,
        // and bounds the loop by min(len_a, len_b) (v0 zip semantics). The (A,B) tuple
        // list is never built.
        let mut fuse_second: Option<(IrExpr, VarId, Ty)> = None;
        let (xs, params, body) = if func == "map" {
            match detect_enum_map_fusion(xs, params, body) {
                Some((real, i_var, key_var, key_ty, tail)) => {
                    fuse_index = Some(i_var);
                    fuse_holder = Some((vec![(key_var, key_ty)], tail));
                    let (p, b) = fuse_holder.as_ref().unwrap();
                    (real, p.as_slice(), b)
                }
                None => match detect_zip_map_fusion(xs, params, body) {
                    Some((a, b, p0, t0, p1, t1, new_body)) => {
                        fuse_second = Some((b.clone(), p1, t1));
                        fuse_holder = Some((vec![(p0, t0)], new_body));
                        let (p, bd) = fuse_holder.as_ref().unwrap();
                        (a, p.as_slice(), bd)
                    }
                    None => {
                        fuse_holder = None;
                        (xs, params.as_slice(), body)
                    }
                },
            }
        } else if func == "fold" {
            // enumerate+FOLD fusion (`args |> list.enumerate |> list.fold(init, (acc, entry) => { let
            // (i, key) = entry; … })`): iterate `real` directly, binding i=loop-index + key=element +
            // KEEPING the acc param, so the `(Int,String)` intermediate is never built. The `find_flag`
            // shape.
            match detect_enum_fold_fusion(xs, params, body) {
                Some((real, i_var, acc_param, key_var, key_ty, tail)) => {
                    fuse_index = Some(i_var);
                    fuse_holder = Some((vec![acc_param, (key_var, key_ty)], tail));
                    let (p, b) = fuse_holder.as_ref().unwrap();
                    (real, p.as_slice(), b)
                }
                None => {
                    fuse_holder = None;
                    (xs, params.as_slice(), body)
                }
            }
        } else {
            fuse_holder = None;
            (xs, params.as_slice(), body)
        };
        let _ = &fuse_holder;
        // (Every combinator the entry `match func` admits — map/filter/fold/flat_map/
        // filter_map; `find` exited above — reads a heap source element as a borrowed
        // handle, so there is no name-keyed source gate here: the per-shape gating lives
        // in each combinator's own seed/body/result lowerers below.)
        // map: a HEAP-element result list (`List[String]`/`List[Value]`) is now built too — each
        // slot holds an OWNED handle the per-element body produces (via lower_heap_result_arm), and
        // the result list is tracked for the recursive scope-end drop. filter keeps scalar results;
        // fold a scalar accumulator. (A heap accumulator / heap-filter still defers.)
        let result_heap_elem = matches!(func, "map" | "filter")
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
        // `flat_map`/`filter_map` over a `List[String]` source build a `List[String]` result by
        // CONCATENATING each element's sublist (`flat_map` → `List[String]`; `filter_map` → the 0-or-1
        // element `Option[String]`, physically a `DynListStr`) onto a loop-carried accumulator via the
        // proven `__list_concat_rc` drop-old + SetLocal slot (the same `i(id)m` append-accumulator the
        // heap `fold` arm uses). Gated to a `List[String]` result; any other element type defers.
        let result_str_acc = matches!(func, "flat_map" | "filter_map")
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String))
            // A `flat_map` producing a `List[Matrix]` (`heads |> list.flat_map((h) =>
            // list.repeat(h, n_rep))` — the nn repeat_kv GQA shape): the SAME
            // append-accumulator loop; the acc/leaf drop grain is derived from the list
            // TYPE inside (`is_list_list_str_ty` → the nested DropListListStr sweep).
            || (func == "flat_map"
                && matches!(result_ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1
                        && matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))));
        // A `filter_map` building a HEAP-but-non-String element list (`List[record]`/`List[Value]`/
        // `List[(String,Value)]` — the dojo `backfill_dir` `task_files |> filter_map((f) => match
        // fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)), err(_) => none })`). A
        // write-cursor result list (like `filter`) keeping the Ok/Some-arm-built OWNED element and
        // skipping the Err/None arm — `lower_defunc_filter_map_hof`. (String-element filter_map stays
        // the `result_str_acc` accumulator path above.)
        let result_filter_map_heap = func == "filter_map"
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && is_heap_ty(&a[0]) && !matches!(a[0], Ty::String));
        let result_ok = match func {
            "map" => result_heap_elem
                || matches!(result_ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0])),
            "filter" => result_heap_elem
                || matches!(result_ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0])),
            // A SCALAR accumulator (Int/Bool/Float), OR any HEAP accumulator the seed/body
            // machinery can handle (String, a list, a Matrix — `fold(layers, x, (h, l) =>
            // block(h, l))`): the inlined `acc = <body>` is the loop-carried slot's
            // drop-old + SetLocal (the proven i(id)m append-accumulator pattern). The
            // strict per-shape gating lives in the SEED (LitStr/Var/list-literal only)
            // and BODY (concat/fresh-owned-call only) lowerers — an unsupported shape
            // returns None there and the whole HOF rolls back to the wall.
            "fold" => true,
            "flat_map" => result_str_acc,
            "filter_map" => result_str_acc || result_filter_map_heap,
            _ => false,
        };
        if !result_ok {
            return None;
        }
        // map/filter have exactly ONE param (the element); fold has TWO (acc, element).
        let expected_params = if func == "fold" { 2 } else { 1 };
        if params.len() != expected_params {
            return None;
        }
        // A HEAP-element map (source and/or result) inlines for BOTH a capturing and a non-capturing
        // closure: the inline is the preferred defunctionalized path (#67), and the lift path
        // (`list.map_str`) SILENTLY MIS-COMPILES a NESTED non-capturing heap map (csv `stringify`
        // returned `,`) — the inline executes it faithfully; a capturing closure has no liftable form
        // at all. (The SCALAR C1 inline already fires for both; this matches it for heap.) A body the
        // subset cannot lower still rolls back below → the caller's lift/WALL fallback is unchanged.

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let lifted_mark = self.lifted.len();
        let value_of_snapshot = self.value_of.clone();

        // The result element type for a heap-element map (the per-element body's owned result is
        // moved into a slot; the result list is recursively dropped). None ⇒ the scalar path.
        let result_elem: Option<Ty> = if result_heap_elem || result_filter_map_heap {
            match result_ty {
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(a[0].clone()),
                _ => None,
            }
        } else {
            None
        };
        // SCALAR-TUPLE accumulator fold (the argmax idiom) — its own specialized loop.
        if func == "fold" {
            if let Some(init_e) = init_idx.map(|ix| &args[ix]) {
                if matches!(result_ty, Ty::Tuple(ts) if ts.len() == 2
                    && !is_heap_ty(&ts[0]) && !is_heap_ty(&ts[1]))
                {
                    if let Some(dst) = self.try_lower_defunc_scalar_tuple_fold(
                        xs, params, body, init_e, fuse_index, result_ty,
                    ) {
                        return Some(dst);
                    }
                }
                // (scalar, Option[scalar]) accumulator — the find_chunk scanner.
                if let Some(dst) = self.try_lower_defunc_opt_tuple_fold(
                    xs, params, body, init_e, fuse_index, result_ty,
                ) {
                    return Some(dst);
                }
            }
        }
        let result = if result_str_acc {
            // flat_map / filter_map: a dedicated `List[String]` append-accumulator loop (concat each
            // element's sublist onto the loop-carried slot). The sublist body returns `List[String]`
            // (flat_map) or `Option[String]` (filter_map) — both are a `DynListStr` the concat appends,
            // and the per-leaf walker handles `some`/`none`/`[]`/list-concat uniformly by body shape.
            self.lower_defunc_str_acc_hof(xs, params, body)
        } else if result_filter_map_heap {
            // filter_map → `List[record]`/`List[Value]`/`List[(String,Value)]`: a write-cursor result
            // list keeping the Ok/Some-arm-built OWNED element, skipping Err/None (the dojo shape).
            match result_elem.as_ref() {
                Some(elem) => self.lower_defunc_filter_map_hof(xs, params, body, elem),
                None => None,
            }
        } else {
            self.lower_defunc_list_hof_inner(
                func,
                xs,
                params,
                body,
                init_idx.map(|i| &args[i]),
                result_elem,
                fuse_index,
                fuse_second.as_ref(),
            )
        };
        if result.is_none() {
            self.rollback_scalar_loop(ops_mark, lhh_mark, lifted_mark, value_of_snapshot);
        } else {
            // The closure was FAITHFULLY inlined (the body executes per element through real
            // ops) — there is NO unlifted/missing closure slot. Clear the flag so the bind
            // path treats the result as a genuinely-materialized list (`materialized_lists`),
            // NOT as an unfaithful HOF to WALL. (My result IS a real, populated block.)
            self.last_call_had_unlifted_closure = false;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_defunc_list_hof_inner(
        &mut self,
        func: &str,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: Option<&IrExpr>,
        result_elem: Option<Ty>,
        fuse_index: Option<VarId>,
        fuse_second: Option<&(IrExpr, VarId, Ty)>,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // A HEAP (String) fold accumulator: the inlined `acc = <body>` is a loop-carried slot
        // drop-old + SetLocal (vs a scalar SetLocal). `acc_ty` is the init's type.
        let fold_acc_ty: Option<Ty> =
            if func == "fold" { init.map(|e| e.ty.clone()).filter(is_heap_ty) } else { None };
        // The result list's recursive free depends on the element type: a String → `DropListStr`
        // (heap_elem_lists); a `(String, Value)` tuple → `DropListStrValue` (str_value_elem_lists,
        // the parse_records pair); a dynamic Value → `DropListValue` (value_elem_lists, parse_records'
        // outer `data |> list.map(row => value.object(…))`). Any other heap element defers cleanly.
        let result_is_str_value_tuple = matches!(&result_elem,
            Some(Ty::Tuple(tys)) if tys.len() == 2
                && matches!(tys[0], Ty::String) && crate::lower::is_value_ty(&tys[1]));
        let result_is_value = matches!(&result_elem, Some(t) if crate::lower::is_value_ty(t));
        // A `List[<record>]` result element with a generated recursive `$__drop_<R>` (`map`/`filter`
        // building/keeping records — porta load_porta_config's `env_keys |> list.map((k) => {key:k,
        // val:json.get_string(env_obj,k)??""})`, which CAPTURES env_obj). Admitted here so the CAPTURING
        // record-element closure inlines (captures resolve via value_of, control_p5 head) instead of
        // falling to lift_lambda (which rejects every capturing lambda) → an honest wall. The result list
        // is registered for the RECURSIVE `$__drop_list_<R>` below (NOT the flat DropListStr that leaks the
        // record's nested String fields — HOLE-1). A record WITHOUT a generated `$__drop_<R>` (e.g. an
        // anonymous structural record) keeps walling — no leaky flat drop.
        let result_record_drop: Option<String> =
            result_elem.as_ref().and_then(|t| self.record_drop_type_name(t));
        // A `List[scalar]` result element (`list.map(rows, (row) => list.slice(row, s, e))`
        // — the nn Matrix row ops): the inner list is a FLAT block whose rc_dec is its
        // full free, so the result list's per-slot DropListStr reclaims everything —
        // ownership-identical to a String element.
        let result_is_scalar_list = matches!(result_elem.as_ref(),
            Some(Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b))
                if b.len() == 1 && !is_heap_ty(&b[0]));
        // A `Matrix` result element (`heads |> list.map((h) => matrix.rms_norm_rows(h, g, e))`
        // — the nn per-head shape) or its structural `List[List[scalar]]` spelling: each
        // element is a TWO-LEVEL block (row handles inside), so the result list's scope-end
        // drop must be the nested `DropListListStr` (`list_list_str_lists`) — the flat
        // DropListStr would leak every element's rows.
        let result_is_matrix = matches!(result_elem.as_ref(),
            Some(Ty::Matrix)
            | Some(Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Matrix, _)))
            || matches!(result_elem.as_ref(),
                Some(Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b))
                    if b.len() == 1 && matches!(&b[0],
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, c)
                            if c.len() == 1 && !is_heap_ty(&c[0])));
        if let Some(elem) = &result_elem {
            if !matches!(elem, Ty::String)
                && !result_is_str_value_tuple
                && !result_is_value
                && !result_is_scalar_list
                && !result_is_matrix
                && result_record_drop.is_none()
            {
                return None;
            }
        }
        // Borrow the source list (evaluated once). A Var is borrowed; a fresh literal is
        // materialized into an owned temp dropped at the OUTER scope (it stays in
        // live_heap_handles). A non-handle iterable (a Range / scalar) is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let mut len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        // zip+map FUSION: borrow the SECOND source and bound the loop by
        // min(len_a, len_b) — v0's zip stops at the shorter list.
        let second = if let Some((b_expr, p1, t1)) = fuse_second {
            let b_v = match self
                .lower_call_args(std::slice::from_ref(b_expr))
                .ok()?
                .into_iter()
                .next()?
            {
                CallArg::Handle(v) => v,
                _ => return None,
            };
            let bh = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(bh), args: vec![b_v] });
            let len_b = self.load_at_offset(bh, 4, PrimKind::Load { width: 4 });
            let lt = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: lt, op: IntOp::Lt, a: len_v, b: len_b });
            let min_v = self.fresh_value();
            self.ops.push(Op::IfThen { cond: lt, dst: Some(min_v) });
            self.ops.push(Op::Else { val: Some(len_v) });
            self.ops.push(Op::EndIf { val: Some(len_b) });
            len_v = min_v;
            Some((bh, *p1, t1.clone()))
        } else {
            None
        };

        // The FOLD accumulator: a stable mutable scalar local seeded from `init`. map/filter
        // build a result list block of `len` slots instead.
        let (acc_local, result_list, result_h, cursor) = match func {
            "fold" => {
                let init_expr = init?;
                if is_heap_ty(&init_expr.ty) {
                    // A HEAP (String) accumulator: seed the loop-carried slot with a BARE fresh owned
                    // String (an i32 Alloc dst) — NOT registered for drop (the slot owns it; the loop's
                    // drop-old or the scope-end drop frees it exactly once). NO ConstInt seed (which
                    // would type the local i64 and mismatch the i32 handle stores). Reassigned in place
                    // via SetLocal each iteration — the proven i(id)m append-accumulator slot. Gated to
                    // a String LITERAL init (`fold("", …)` / `fold("prefix", …)`); a non-literal heap
                    // init rolls back (the HOF WALLs).
                    let seeded = match &init_expr.kind {
                        IrExprKind::LitStr { value: s } => {
                            let acc = self.fresh_value();
                            self.ops.push(Op::Alloc {
                                dst: acc,
                                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                                init: crate::Init::Str(s.clone()),
                            });
                            Some(acc)
                        }
                        // `fold(layers, x, …)` — a VAR init (usually a borrowed param):
                        // ACQUIRE an owned copy (`Dup`) so the slot owns its reference
                        // independently (the loop's drop-old frees exactly this chain).
                        IrExprKind::Var { id } => {
                            let src = self.value_for(*id).ok()?;
                            let acc = self.fresh_value();
                            self.ops.push(Op::Dup { dst: acc, src });
                            Some(acc)
                        }
                        // `fold(xs, [], …)` — an admitted list literal init.
                        IrExprKind::List { .. } => self
                            .try_lower_str_list_literal(init_expr)
                            .or_else(|| self.try_lower_scalar_list_construct(init_expr)),
                        _ => None,
                    };
                    let acc = seeded?;
                    // Classify the slot's DROP GRAIN from the accumulator TYPE, so the
                    // per-iteration drop-old (and the final move-out) frees the right
                    // shape — a `List[List[Float]]` (Matrix) accumulator would leak its
                    // rows under a flat Drop.
                    if crate::lower::is_list_list_str_ty(&init_expr.ty) {
                        self.list_list_str_lists.insert(acc);
                    } else if let Some(rname) = self.record_or_anon_drop_type_name(&init_expr.ty) {
                        self.variant_drop_handles.insert(acc, rname);
                    } else if crate::lower::is_lenlist_list_ty(&init_expr.ty) {
                        self.variant_drop_handles.insert(acc, "list_lenlist".to_string());
                    } else if is_heap_elem_list_ty(&init_expr.ty) {
                        self.heap_elem_lists.insert(acc);
                    }
                    (Some(acc), None, None, None)
                } else {
                    let init_v = self.lower_scalar_value(init_expr)?;
                    // A STABLE mutable local: ConstInt-seed then SetLocal to the init value (so the
                    // local is distinct and reassignable across iterations, the proven loop-state model).
                    let acc = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: acc, value: 0 });
                    self.ops.push(Op::SetLocal { local: acc, src: init_v });
                    (Some(acc), None, None, None)
                }
            }
            "map" | "filter" => {
                // A fresh OWNED `DynList` of `len` slots (map: len = len(xs); filter: len(xs) is
                // the MAX, the real length is patched to the write-cursor after the loop). Built
                // exactly like a scalar list literal — a flat block, scope-end `Drop`.
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::DynList { len: len_v },
                });
                let rh = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(rh), args: vec![dst] });
                // A heap-element map result: track the block for the recursive scope-end drop (frees
                // each element), not a flat Drop — a String element → DropListStr (heap_elem_lists);
                // a (String, Value) tuple → DropListStrValue (str_value_elem_lists). The per-element
                // body stores an OWNED handle into each slot (moved in, this list now owns it).
                if result_is_str_value_tuple {
                    self.str_value_elem_lists.insert(dst);
                } else if result_is_value {
                    self.value_elem_lists.insert(dst);
                } else if result_is_matrix {
                    // A List[Matrix] result — the nested two-level DropListListStr sweep.
                    self.list_list_str_lists.insert(dst);
                } else if let Some(rname) = &result_record_drop {
                    // A `List[<record>]` result: register the RECURSIVE `$__drop_list_<R>` (frees each
                    // element's nested heap fields via `$__drop_<R>`), NOT the flat `heap_elem_lists`
                    // DropListStr which would rc_dec only the element HANDLE and LEAK the record's String
                    // fields (HOLE-1). Identical registration the record-list LITERAL uses (binds_p3:517).
                    self.variant_drop_handles.insert(dst, format!("list_{rname}"));
                } else if result_elem.is_some() {
                    self.heap_elem_lists.insert(dst);
                }
                // filter needs a write-cursor (the count of kept elements) — a stable local.
                let cur = if func == "filter" {
                    let c = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: c, value: 0 });
                    Some(c)
                } else {
                    None
                };
                (None, Some(dst), Some(rh), cur)
            }
            _ => return None,
        };

        // The loop index (stable mutable i64 local) and the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8, then load64.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        // A HEAP source element is the slot's HANDLE (`LoadHandle` = i32 Ptr — the inlined body reads
        // it as a BORROWED heap value, e.g. `value.get(row, …)`); a SCALAR element is the i64 value.
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM(s). map/filter: the single element param = elem. fold: acc
        // (the stable local) + element param = elem. The CAPTURES need no binding — their
        // VarIds already resolve through `value_of`.
        let elem_param = if func == "fold" { params[1].0 } else { params[0].0 };
        self.value_of.insert(elem_param, elem);
        // zip+map FUSION: bind p1 = b[i] (same slot arithmetic on the second source).
        if let Some((bh, p1, t1)) = &second {
            let b_base = self.load_addr(*bh, 12);
            let b_addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: b_addr, op: IntOp::Add, a: b_base, b: i8_v });
            let b_elem = self.fresh_value();
            let b_read = if is_heap_ty(t1) { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
            self.ops.push(Op::Prim { kind: b_read, dst: Some(b_elem), args: vec![b_addr] });
            self.value_of.insert(*p1, b_elem);
            if is_heap_ty(t1)
                && (matches!(t1, Ty::Tuple(_)) || self.aggregate_field_tys(t1).is_some())
            {
                self.param_values.insert(b_elem);
                self.materialized_aggregates.insert(b_elem);
            }
        }
        // A heap-AGGREGATE element (a `(String,String)`/`(String,Value)` tuple, a record) bound as the
        // lambda param: register the borrowed handle as a materialized aggregate so the body's
        // `let (k,v)=pair` destructure BORROWS its slots (try_lower_tuple_destructure requires this;
        // without it the destructure declines → container-grain alias → every field reads garbage).
        if src_heap {
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a) = &xs.ty {
                if a.len() == 1
                    && (matches!(&a[0], Ty::Tuple(_)) || self.aggregate_field_tys(&a[0]).is_some())
                {
                    self.param_values.insert(elem);
                    self.materialized_aggregates.insert(elem);
                }
            }
        }
        if func == "fold" {
            self.value_of.insert(params[0].0, acc_local.unwrap());
        }
        // enumerate+map FUSION: bind the destructured INDEX var to the loop index `i_v` (a scalar),
        // so the fused body's `list.get_or(row, i, …)` reads the right index. (key was bound above as
        // the element param.)
        if let Some(i_var) = fuse_index {
            self.value_of.insert(i_var, i_v);
        }

        // Lower the lambda BODY inline as a per-iteration frame. SCALAR result → lower_scalar_value
        // (pure, no ownership event). HEAP result (`map` → List[String]) → lower_heap_result_arm,
        // which lowers a general heap-returning body (a call / concat / `??` / nested `list.map …
        // list.join` — the stringify_records cell projection) to a FRESH owned handle, Consumes it
        // (moved out of the iteration scope), and drops the body's own temps internally. A body the
        // subset cannot lower → None → the whole HOF rolls back and the caller WALLS (caps honest).
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        // A HEAP (String) fold accumulator whose body CONDITIONALLY replaces the accumulator
        // (`if cond then <new> else acc` — the `find_flag` shape): the unconditional drop-old +
        // SetLocal append-accumulator below cannot lower it (the `else acc` arm would drop-then-store
        // the FREED acc → use-after-free). Update the slot IN PLACE — only the THEN arm drops-old +
        // rebinds, the empty ELSE leaves acc untouched — so the loop slot owns exactly one ref at the
        // body's start and end in BOTH arms (the conditional-acquire invariant, OwnershipFilter.v's
        // CondLoop). The handler emits the whole `IfThen/Else/EndIf` + slot update itself, so the
        // generic `match func` update below is skipped.
        let cond_acc_handled = func == "fold"
            && fold_acc_ty.is_some()
            && acc_local.is_some()
            && {
                self.scalar_loop_depth += 1;
                // Returns true ONLY when fully handled; on a shape match it could not lower it
                // truncates its own ops + returns false, so we fall through to the concat/scalar
                // paths (which, for a non-conditional body, lower it; for a failed conditional body,
                // also fail → the whole HOF rolls back at the call site). On a non-conditional body
                // it returns false with no ops emitted.
                let ok = self.try_lower_cond_heap_acc_fold(body, params[0].0, acc_local.unwrap());
                self.scalar_loop_depth -= 1;
                ok
            };
        // `filter`'s body is the PREDICATE (a Bool) regardless of the result element type — the kept
        // ELEMENT (not the body) is stored. Only map/flat_map-style HOFs lower the body AS the heap
        // result element. So route filter to the scalar (Bool) path even when result_elem is Some.
        let body_v = if cond_acc_handled {
            // The slot was already updated in place; no merged body value flows out.
            Some(acc_local.unwrap())
        } else if let Some(elem_ty) = result_elem.as_ref().filter(|_| func != "filter") {
            self.lower_heap_result_arm(body, elem_ty)
        } else if fold_acc_ty.is_some() {
            // A heap (String) fold accumulator: the body `acc + s` is a ConcatStr producing a FRESH
            // owned String returned as a BARE ValueId (NOT Consumed/registered — exactly the append-
            // accumulator producer). The reassignment below drops-old + SetLocal moves this in, so it
            // is single-owned by the slot (lower_heap_result_arm would double-register it → a scope-end
            // double-free). It reads the loop-carried `acc` BEFORE the drop (borrow-then-rebind). A
            // non-ConcatStr body returns None → the HOF rolls back and the caller WALLs.
            self.scalar_loop_depth += 1;
            let v = self
                .try_lower_concat_str(body)
                // `acc + [x]` — a list append accumulator.
                .or_else(|| self.try_lower_concat_list(body))
                // `encoder_block_r(h, layer, n)` — a CALL producing the new accumulator
                // as a FRESH owned value (the calling convention): bare CallFn dst, moved
                // into the slot by the drop-old + SetLocal below.
                .or_else(|| self.try_lower_fold_acc_call(body));
            self.scalar_loop_depth -= 1;
            v
        } else {
            self.scalar_loop_depth += 1;
            let v = self.lower_scalar_value(body);
            self.scalar_loop_depth -= 1;
            v
        };
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let body_v = match body_v {
            Some(v) => v,
            None => return None,
        };
        // SCALAR: drop the body's heap temps. HEAP: lower_heap_result_arm already balanced its own
        // temps + Consumed body_v (moved out), so this is a no-op (live is back to body_mark). The
        // conditional-acc handler already dropped its per-arm temps WITHIN the then-arm, so live is
        // back to body_mark here too (no-op).
        self.drop_arm_locals(body_mark);

        // The conditional-acc fold already emitted its IfThen/Else/EndIf + in-place slot update — the
        // generic per-func slot update below must NOT run (it would re-drop + re-store the slot).
        if !cond_acc_handled {
        match func {
            "map" => {
                // result[i] = body_v.
                let rh = result_h.unwrap();
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: i8_v });
                if result_elem.is_some() {
                    // body_v is an OWNED heap handle (i32) already Consumed by lower_heap_result_arm
                    // (moved out of the iteration scope). Extend it to i64 (`PrimKind::Handle`,
                    // exactly the str-list-literal store) then store64 into the slot — the result list
                    // now owns it (its recursive DropListStr frees it at scope end).
                    let eh = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![body_v] });
                    self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, eh] });
                } else {
                    self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, body_v] });
                }
            }
            "filter" => {
                // if body_v (Bool) then { result[cursor] = elem; cursor += 1 }.
                let rh = result_h.unwrap();
                let cur = cursor.unwrap();
                self.ops.push(Op::IfThen { cond: body_v, dst: None });
                // then-arm: store elem at result[cursor*8], bump cursor.
                let c8 = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: c8, op: IntOp::Mul, a: cur, b: eight });
                let rbase = self.load_addr(rh, 12);
                let raddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: raddr, op: IntOp::Add, a: rbase, b: c8 });
                // A HEAP filter keeps the source ELEMENT (a BORROWED handle, `param_values`): CLONE it
                // (Dup, cert `a` = a new owned ref) and MOVE it into the output list (Consume, cert `m`).
                // The `a..m` is LOCALLY balanced — both in THIS then-arm, the else-arm does nothing — so
                // the existing flat certificate accepts it WITHOUT a loop-carried conditional slot (the
                // output list is alloc'd once, not a SetLocal-rebound slot; per kept element a fresh
                // Dup'd object is acquired and immediately moved into the list, whose recursive
                // DropListStr/DropListValue frees it). A SCALAR filter stores the i64 value directly (no
                // ownership). OwnershipFilter.v's CondLoop proves the more general loop-carried form; this
                // locally-balanced shape needs only the base checker.
                let stored = if result_elem.is_some() {
                    let cloned = self.fresh_value();
                    self.ops.push(Op::Dup { dst: cloned, src: elem });
                    self.ops.push(Op::Consume { v: cloned });
                    let eh = self.fresh_value();
                    self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(eh), args: vec![cloned] });
                    eh
                } else {
                    elem
                };
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, stored] });
                let cnext = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: cnext, op: IntOp::Add, a: cur, b: one_v });
                self.ops.push(Op::SetLocal { local: cur, src: cnext });
                self.ops.push(Op::Else { val: None });
                self.ops.push(Op::EndIf { val: None });
            }
            "fold" => {
                // acc = body_v. A HEAP acc DROPS the old slot value first (the loop-carried `i(id)m`
                // append-accumulator pattern: each transient String reclaimed), then moves the new one
                // in. A scalar acc just rebinds (no handle to free).
                if fold_acc_ty.is_some() {
                    let drop_op = self.drop_op_for(acc_local.unwrap());
                    self.ops.push(drop_op);
                }
                self.ops.push(Op::SetLocal { local: acc_local.unwrap(), src: body_v });
            }
            _ => return None,
        }
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        match func {
            // A HEAP acc's final value is an OWNED String returned to the caller, which registers it
            // for the outer scope-end drop (the same as the map/filter result list — C1 does NOT push
            // it itself, or it would be double-dropped).
            "fold" => Some(acc_local.unwrap()),
            "map" => Some(result_list.unwrap()),
            "filter" => {
                // Patch the result list's `len` field (offset 4) to the write-cursor: the
                // visible length is the count of kept elements (cap stays len(xs), unused
                // tail slots are harmless — a `${list}` Display reads `len`, an `xs[i]`
                // bounds-checks against `len`). `store32` at result_h + 4.
                let rh = result_h.unwrap();
                let cur = cursor.unwrap();
                let four = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: four, value: 4 });
                let lenaddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lenaddr, cur] });
                Some(result_list.unwrap())
            }
            _ => None,
        }
    }

    /// A HEAP (String) `list.fold` body that CONDITIONALLY replaces the accumulator —
    /// `if cond then <new-string> else acc` (the `find_flag` shape). Updates the loop-carried
    /// String slot IN PLACE: the THEN arm produces a FRESH owned String, DROPS the old slot value,
    /// and `SetLocal`'s the slot to the new one; the (empty) ELSE arm leaves the slot untouched. So
    /// the slot owns EXACTLY ONE reference at the body's start and end in BOTH arms — the conditional-
    /// acquire invariant (drop-then-rebind in a guarded arm, OwnershipFilter.v's CondLoop). The
    /// unconditional append-accumulator update CANNOT lower this: its `else acc` would drop-then-store
    /// the FREED slot (use-after-free). Each per-arm operand temp (the `??` operand's materialized
    /// Option block) is dropped WITHIN the then-arm (before `Else`), so neither arm leaks.
    ///
    /// Returns `true` iff fully lowered. On a NON-conditional body (a plain `acc + s` concat) it
    /// returns `false` with NO ops emitted (the caller's concat/scalar path takes over). On a
    /// conditional body it cannot fully lower it TRUNCATES the ops it pushed + returns `false` (the
    /// caller falls through, fails, and the whole HOF rolls back — never a wrong-bytes slot).
    fn try_lower_cond_heap_acc_fold(
        &mut self,
        body: &IrExpr,
        acc_param: VarId,
        acc_local: ValueId,
    ) -> bool {
        use almide_ir::IrExprKind;
        // Unwrap a single-expression Block (the post-fusion `tail` is `{ <if> }`).
        let inner = match &body.kind {
            IrExprKind::Block { stmts, expr } if stmts.is_empty() => match expr {
                Some(e) => e.as_ref(),
                None => return false,
            },
            _ => body,
        };
        let (cond, then, else_) = match &inner.kind {
            IrExprKind::If { cond, then, else_ } => (cond.as_ref(), then.as_ref(), else_.as_ref()),
            _ => return false,
        };
        // The ELSE arm must be EXACTLY the accumulator param (`else acc`): that is what makes the
        // empty-else in-place update sound (the slot is genuinely unchanged when `cond` is false).
        match &else_.kind {
            IrExprKind::Var { id } if *id == acc_param => {}
            _ => return false,
        }
        // The shape matched — roll back our own ops on any sub-failure (so the caller's fall-through
        // path starts clean).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        self.ops.push(Op::IfThen { cond: cond_v, dst: None });
        // THEN arm: a FRESH owned String (BARE — NOT Consumed/registered; the slot will own it).
        let arm_mark = self.live_heap_handles.len();
        let new_v = match self.lower_cond_acc_then_value(then) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        // Drop the OLD slot value, then move the new one in (the slot's single ref is preserved).
        let drop_op = self.drop_op_for(acc_local);
        self.ops.push(drop_op);
        self.ops.push(Op::SetLocal { local: acc_local, src: new_v });
        // Free the then-arm's operand temps (e.g. the `??` operand Option block) WITHIN the arm.
        self.drop_arm_locals(arm_mark);
        self.ops.push(Op::Else { val: None });
        // ELSE arm: empty — the slot is left as-is.
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// Lower the THEN arm of a conditional heap-acc fold to a BARE owned String (rc 1, NOT Consumed,
    /// NOT registered for scope-end drop — the accumulator slot becomes its sole owner via the
    /// caller's drop-old + `SetLocal`). Any operand temp it materializes IS registered (freed by the
    /// caller's per-arm `drop_arm_locals`). `None` ⇒ out of subset (the caller rolls back + walls).
    fn lower_cond_acc_then_value(&mut self, then: &IrExpr) -> Option<ValueId> {
        use almide_ir::{BinOp, IrExprKind};
        match &then.kind {
            // `e!` — strip the effect-propagation unwrap (identity on the Ok payload here).
            IrExprKind::Unwrap { expr } => self.lower_cond_acc_then_value(expr),
            // `<option/result> ?? <fallback>` (`list.get(args, i+1) ?? ""`) → a fresh owned String.
            IrExprKind::UnwrapOr { expr, fallback } => {
                self.try_lower_option_unwrap_or(expr, fallback, false)
            }
            // A string LITERAL → a fresh owned String block.
            IrExprKind::LitStr { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(value.clone()),
                });
                Some(dst)
            }
            // `a + b` string concat → a fresh owned String.
            IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => self.try_lower_concat_str(then),
            // `acc + [x]` list concat → a fresh owned list (the straight-line / loop append-
            // accumulator THEN arm). `try_lower_concat_list` reads the loop-carried `acc` slot as a
            // BORROW and emits a fresh owned list via `__list_concat`/`__list_concat_rc`; the caller's
            // drop-old + `SetLocal` then moves it into the slot (single-owned). It classifies the
            // result's recursive drop kind (heap_elem/value/…); the caller copies that onto the slot
            // so the per-iteration drop-old frees the right grain.
            IrExprKind::BinOp { op: BinOp::ConcatList, .. } => self.try_lower_concat_list(then),
            // A string INTERPOLATION → a fresh owned String.
            IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
            // A bare heap Var → ACQUIRE our own reference (`Dup`) so the slot owns it independently.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            // `pair.0` — a HEAP field of the borrowed tuple/record element (the
            // lookup_token fold: `if pair.1 == target then pair.0 else acc`): load
            // the slot's handle (a borrow of the container's field) then ACQUIRE
            // (`Dup`) so the accumulator slot owns its own reference.
            IrExprKind::TupleIndex { object, index } => {
                use crate::{IntOp, PrimKind};
                let obj = match &object.kind {
                    IrExprKind::Var { id } => self.value_for(*id).ok()?,
                    _ => return None,
                };
                if !self.materialized_aggregates.contains(&obj) {
                    return None;
                }
                let off = self.aggregate_index_offset_any(&object.ty, *index)?;
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![obj] });
                let offc = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: offc, value: off as i64 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: offc });
                let raw = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(raw), args: vec![addr] });
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src: raw });
                Some(dst)
            }
            _ => None,
        }
    }

    /// Copy the RECURSIVE-DROP classification of one heap object onto another (the loop-carried slot
    /// inherits its feeder's drop grain). A `List[String]`/flat-element list drops via `DropListStr`,
    /// a `List[Value]` via `DropListValue`, a `(String,Value)`/`(String,String)`/`(Int,String)` tuple
    /// list / record / variant element list via its dedicated recursive drop — so the slot's
    /// per-iteration drop-old frees exactly the same grain its feeder allocated (no leak, no
    /// double-free). A plain String/scalar feeder is in no set ⇒ a no-op (the slot stays a flat `Drop`).
    pub(crate) fn copy_heap_drop_class(&mut self, from: ValueId, to: ValueId) {
        if self.heap_elem_lists.contains(&from) {
            self.heap_elem_lists.insert(to);
        }
        if self.value_elem_lists.contains(&from) {
            self.value_elem_lists.insert(to);
        }
        if self.str_value_elem_lists.contains(&from) {
            self.str_value_elem_lists.insert(to);
        }
        if self.str_str_elem_lists.contains(&from) {
            self.str_str_elem_lists.insert(to);
        }
        if self.list_list_str_lists.contains(&from) {
            self.list_list_str_lists.insert(to);
        }
        if self.value_handles.contains(&from) {
            self.value_handles.insert(to);
        }
        if let Some(k) = self.variant_drop_handles.get(&from).cloned() {
            self.variant_drop_handles.insert(to, k);
        }
    }

    /// STRAIGHT-LINE identity-else shadow rebind — `let acc = if cond then acc + [x] else acc` bound
    /// to a let/var OUTSIDE any loop (porta `serialize_opts`: 7 stacked optional-arg appends on one
    /// `args` slot). The ELSE arm is EXACTLY the accumulator var, so this is the PROVEN loop-carried
    /// `i(id)m` append-accumulator slot, just UNROLLED to a fixed straight-line sequence: the THEN arm
    /// produces a FRESH owned value, DROPS the old slot, and `SetLocal`s the slot to the new one; the
    /// (empty) ELSE leaves the slot untouched. So the slot owns EXACTLY ONE reference at each rebind's
    /// start and end in BOTH arms (the conditional-acquire invariant, OwnershipChecker.v
    /// `check_line_unroll_sound`, which quantifies over ALL iteration counts — including this
    /// unrolling). `ownership_certificate` folds each rebind to a `(id)` CLoop body, so the whole body
    /// reads `i(id)…(id)m` — each `(id)` the same rc-preserving unit the loop slot proves, accepted by
    /// the kernel-checked `check_cert_lc`.
    ///
    /// `acc_local` MUST be the OWNED, scope-tracked heap handle behind `acc` (the seed's `[]`/`""`),
    /// already in `live_heap_handles` — the caller aliases the new shadow to it and does NOT re-push,
    /// so the single scope-end drop (or tail move-out) still covers it. Returns `true` iff fully
    /// lowered; on a sub-failure it TRUNCATES its own ops and returns `false` (the caller walls).
    pub(crate) fn try_lower_line_cond_acc(
        &mut self,
        value: &IrExpr,
        acc_id: VarId,
        acc_local: ValueId,
    ) -> bool {
        use almide_ir::IrExprKind;
        let (cond, then, else_) = match &value.kind {
            IrExprKind::If { cond, then, else_ } => (cond.as_ref(), then.as_ref(), else_.as_ref()),
            _ => return false,
        };
        // The ELSE arm must be EXACTLY the accumulator var (`else acc`) — that is what makes the
        // empty-else in-place update sound (the slot is genuinely unchanged when `cond` is false).
        match &else_.kind {
            IrExprKind::Var { id } if *id == acc_id => {}
            _ => return false,
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        self.ops.push(Op::IfThen { cond: cond_v, dst: None });
        // THEN arm: a FRESH owned value (BARE — NOT Consumed/registered; the slot will own it).
        let arm_mark = self.live_heap_handles.len();
        let new_v = match self.lower_cond_acc_then_value(then) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        // The slot inherits the fresh value's recursive-drop grain, so the drop-old below (and at
        // every later rebind, and at scope end) frees exactly the right element grain.
        self.copy_heap_drop_class(new_v, acc_local);
        // Drop the OLD slot value, then move the new one in (the slot's single ref is preserved).
        let drop_op = self.drop_op_for(acc_local);
        self.ops.push(drop_op);
        self.ops.push(Op::SetLocal { local: acc_local, src: new_v });
        // Free the then-arm's operand temps (e.g. a materialized list-literal operand) WITHIN the arm.
        self.drop_arm_locals(arm_mark);
        self.ops.push(Op::Else { val: None });
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// C1 DEFUNCTIONALIZATION for a TUPLE-accumulator `list.fold` whose accumulator is a 2-tuple
    /// `(List[T], Int)` — the wasm-bindgen `wasm_record_offsets` shape:
    ///
    /// ```text
    /// widths |> list.fold(([], 0), (state, w) => {
    ///   let (acc, off) = state
    ///   (acc + [off], off + w)
    /// })
    /// ```
    ///
    /// The accumulator carries a growing `List[T]` (the offsets) and an `Int` cursor. The scalar
    /// `result_ok` gate (`!is_heap_ty || String`) rejects a `(List[T], Int)` accumulator (heap +
    /// not-String), so this dedicated path carries TWO loop-state slots and builds the result tuple
    /// ONCE after the loop:
    ///
    ///   1. The LIST component — a heap slot SEEDED with a fresh EMPTY `List` (an i32 `Alloc`, NOT
    ///      registered for drop: the slot owns it). Each iteration the body's component0 `acc + [elem]`
    ///      is lowered to a FRESH owned list via `try_lower_concat_list` (reading the slot through
    ///      `value_of[acc]`), then the slot is DROPPED-OLD + `SetLocal`'d to the new list — the PROVEN
    ///      `i(id)m` append-accumulator slot (identical to the String-fold arm at the top of
    ///      `lower_defunc_list_hof_inner` and to `append_owned_sub_to_acc`).
    ///   2. The INT component — a STABLE scalar local, `SetLocal`'d each iteration with the body's
    ///      component1 `n + step`.
    ///
    /// The body's `let (acc, off) = state` destructure is NEVER materialized as a tuple block — `acc`
    /// binds DIRECTLY to the List slot and `off` to the Int slot (read the two slots in place). The
    /// body's final `(c0, c1)` tuple is NEVER materialized per iteration — it is destructured into the
    /// two slot updates. After the loop the result tuple `(List_slot, Int_slot)` is built ONCE via
    /// `try_lower_tuple_construct` (a masked block: slot0 a moved-in OWNED list handle, slot1 the Int),
    /// so the caller's Module-call bind path tracks it as a `materialized_aggregate` with the right
    /// heap-slot mask and its scope-end drop frees exactly the list + the block. The immediate
    /// `match pair { (offs, _) => offs }` extraction then borrows + acquires slot 0 (see
    /// `try_lower_tuple_extract_match`).
    ///
    /// SOUNDNESS — both slots are independently balanced per iteration:
    ///  - The List slot is the proven loop-carried `i(id)m`: seeded `i` (fresh empty), each iteration a
    ///    fresh concat (`i`), drop-old (`d`), `SetLocal` (folds the new `i` into the slot — `m`). After
    ///    the loop its single live value is MOVED into the tuple block's slot 0 (cert `m`); the block's
    ///    masked drop (or the move-out at the `match` extraction) frees it exactly once. No leak, no
    ///    double-free for any iteration count.
    ///  - The Int slot is a scalar — no ownership event (a plain `SetLocal`).
    ///
    /// STRICT GATE (any deviation → `None` → the caller rolls back → WALLs, never a wrong-bytes tuple):
    /// a 2-tuple `(List[T], Int)` accumulator, an empty-list-literal + scalar-int LITERAL init, a body
    /// that is exactly `{ let (a, b) = state; (a + [<elem>], <scalar-int>) }` (the destructure reads the
    /// state param; component0 a `ConcatList` reading the bound list var; component1 a scalar Int).
    fn try_lower_defunc_tuple_acc_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::IrStmtKind;
        use crate::PrimKind;

        // GATE — fold has exactly TWO params (state, element).
        if params.len() != 2 {
            return None;
        }
        let state_param = params[0].0;
        let elem_param = params[1].0;

        // GATE — the accumulator/result type is a 2-tuple `(List[T], Int)`.
        let tup_tys = match result_ty {
            Ty::Tuple(tys) if tys.len() == 2 => tys,
            _ => return None,
        };
        let list_elem_ty = match &tup_tys[0] {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        if !matches!(tup_tys[1], Ty::Int) {
            return None;
        }
        // A SCALAR list element (`(List[Int], Int)` — `wasm_record_offsets`) OR a `String` element
        // (`(List[String], Int)`). Both are admitted: the scalar slot is a FLAT `__list_concat` + flat
        // `Drop`; the String slot is `__list_concat_rc` + the recursive `DropListStr` (marked
        // `heap_elem_lists`), and the SOURCE element of a heap (`List[String]`) source is read as the
        // slot's i32 HANDLE (`LoadHandle`, below) — reading it as an i64 scalar was the
        // `expected i32, found i64` invalid-wasm bug. A heap-FIELD aggregate element (a tuple/record
        // list) still defers (its masked recursive drop is out of this slice).
        let list_elem_scalar = !is_heap_ty(&list_elem_ty);
        let list_elem_str = matches!(list_elem_ty, Ty::String);
        if !list_elem_scalar && !list_elem_str {
            return None;
        }

        // GATE — the INIT is `(<empty-list-literal>, <int-literal>)`.
        let init_elems = match &init.kind {
            IrExprKind::Tuple { elements } if elements.len() == 2 => elements,
            _ => return None,
        };
        match &init_elems[0].kind {
            IrExprKind::List { elements } if elements.is_empty() => {}
            _ => return None,
        }
        let int_init_v = match &init_elems[1].kind {
            IrExprKind::LitInt { value } => *value,
            _ => return None,
        };

        // GATE — the BODY is `{ let (acc, n) = state; <interior pure lets…>; (c0, c1) }`: the FIRST
        // statement destructures the state param's two components, then ZERO OR MORE pure value-binding
        // statements (the element destructure `let (i,f)=fe`, a scalar `let off_s = int.to_string(off)`,
        // a heap `let store = match get_kind(f) {…}` — the wasm-bindgen `field_stores` shape), then a
        // 2-tuple tail. The interior statements are lowered as per-iteration EFFECTS (their heap temps
        // freed within the iteration frame); a statement the body subset cannot lower → None → walls.
        let (stmts, tail) = match &body.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } if !stmts.is_empty() => (stmts, tail.as_ref()),
            _ => return None,
        };
        let (acc_var, n_var) = match &stmts[0].kind {
            IrStmtKind::BindDestructure { pattern, value } => {
                // The destructured value must be the STATE param (the destructure reads the two slots,
                // not a fresh tuple).
                match &value.kind {
                    IrExprKind::Var { id } if *id == state_param => {}
                    _ => return None,
                }
                match pattern {
                    IrPattern::Tuple { elements } if elements.len() == 2 => {
                        let a = match &elements[0] {
                            IrPattern::Bind { var, ty }
                                if matches!(ty,
                                    Ty::Applied(TypeConstructorId::List, l) if l.len() == 1) =>
                            {
                                *var
                            }
                            _ => return None,
                        };
                        let b = match &elements[1] {
                            IrPattern::Bind { var, ty } if matches!(ty, Ty::Int) => *var,
                            _ => return None,
                        };
                        (a, b)
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };
        // The tail must be a 2-tuple `(c0, c1)` whose component0 is a `acc + […]` ConcatList reading
        // the destructured list var, and component1 a scalar Int. (The component0 shape is re-checked
        // by `try_lower_concat_list` below; here we just require the tuple shape + a ConcatList whose
        // left reads `acc_var`, so a non-append body — e.g. `(other_list, …)` — defers.)
        let tail_elems = match &tail.kind {
            IrExprKind::Tuple { elements } if elements.len() == 2 => elements,
            _ => return None,
        };
        match &tail_elems[0].kind {
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, left, .. } => {
                match &left.kind {
                    IrExprKind::Var { id } if *id == acc_var => {}
                    _ => return None,
                }
            }
            _ => return None,
        }
        if !matches!(tail_elems[0].ty,
            Ty::Applied(TypeConstructorId::List, ref a) if a.len() == 1)
            || !matches!(tail_elems[1].ty, Ty::Int)
        {
            return None;
        }

        // ----- Lowering -----
        // Borrow the source list (evaluated once). A Var is borrowed; a fresh literal is materialized
        // into an owned temp dropped at the OUTER scope. A non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // Seed the LIST slot: a fresh EMPTY `List` (an i32 Alloc dst). The loop-carried slot OWNS it —
        // reassigned via drop-old + SetLocal each iteration (the proven i(id)m slot). NOT registered
        // for drop here: the slot owns it; the loop's drop-old or the post-loop move-into-tuple frees
        // it exactly once. A `List[String]` slot is marked `heap_elem_lists` so EVERY drop of it
        // (drop-old) is the recursive `DropListStr` (frees its owned element Strings).
        let zero_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero_len, value: 0 });
        let list_slot = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: list_slot,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: zero_len },
        });
        if list_elem_str {
            self.heap_elem_lists.insert(list_slot);
        }

        // Seed the INT slot: a STABLE mutable scalar local (ConstInt-seed then SetLocal to the init —
        // the proven loop-state model: distinct + reassignable across iterations).
        let int_slot = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: int_slot, value: 0 });
        let int_init_const = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: int_init_const, value: int_init_v });
        self.ops.push(Op::SetLocal { local: int_slot, src: int_init_const });

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8. The READ WIDTH depends on the
        // SOURCE element type — a SCALAR source (`List[Int]`) is the i64 VALUE (`Load { width: 8 }`); a
        // HEAP source (`List[String]`) is the slot's String HANDLE (`LoadHandle` = i32 Ptr), read as a
        // BORROWED heap value the body copies (`acc + [w]`, `string.len(w)`). Hardcoding the i64 load
        // for a heap source declared the element local i64 while `string.len`/`__list_concat_rc` expect
        // an i32 handle — the `expected i32, found i64` invalid wasm. Mirrors `lower_defunc_list_hof_
        // inner`'s `src_heap` element read.
        let src_heap = matches!(&xs.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda params + the destructured state vars to the SLOTS (NO tuple state block):
        //   element param → elem (the loaded value/handle)
        //   acc (destructured list var) → the List slot
        //   n   (destructured int var)  → the Int slot
        // Captures need no binding — their VarIds already resolve through `value_of`.
        self.value_of.insert(elem_param, elem);
        // A HEAP source element is a BORROW (the source list owns it) — record it so the body's
        // `acc + [w]` rc-incs/copies it rather than moving it out (no double-free with the source's
        // own drop). A scalar element is a value copy, no ownership.
        if src_heap {
            self.param_values.insert(elem);
        }
        self.value_of.insert(acc_var, list_slot);
        self.value_of.insert(n_var, int_slot);
        // `acc` reads the List slot as a tracked nested-ownership list so a `acc + [x]` concat
        // borrows it (the slot owns it; the concat rc-incs/byte-copies into a fresh list).
        self.materialized_lists.insert(list_slot);

        // Lower the INTERIOR statements (`stmts[1..]` — `let (i,f)=fe`, `let off_s = int.to_string(off)`,
        // `let store = match get_kind(f) {…}`) as per-iteration EFFECTS, then BOTH tuple components,
        // reading the OLD slot values BEFORE updating either slot (the body semantics: `(acc + [store],
        // off + w)` reads `acc`/`off` as the iteration's incoming state). A heap temp an interior stmt
        // binds (the `store` String) is freed within the per-iteration frame by `drop_arm_locals`; the
        // concat already copied/rc-inc'd it into the new list, so it is single-owned + freed.
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let mut stmts_ok = true;
        for s in &stmts[1..] {
            // A `let store = if/match {…}` interior stmt binds a HEAP-result branch. `lower_stmt` →
            // `lower_bind` WALLS that (the function-scope `im·im·d` it cannot prove). HERE it is sound:
            // materialize the merged owned `dst` (per-arm `"im"`), bind `store`, and push it to
            // `live_heap_handles` so the PER-ITERATION `drop_arm_locals(body_mark)` frees it once —
            // the proven `i…d` per-iteration frame (NOT a function-scope drop). The concat `acc + [store]`
            // copies/rc-incs it into the new list before that drop. Other stmts use the normal `lower_stmt`.
            if let IrStmtKind::Bind { var, ty: bty, value, .. } = &s.kind {
                if is_heap_ty(bty)
                    && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
                {
                    let merged = match &value.kind {
                        IrExprKind::If { cond, then, else_ } => {
                            self.try_lower_heap_result_if(cond, then, else_, bty)
                        }
                        IrExprKind::Match { subject, arms } => self
                            .desugar_match_to_if(subject, arms, bty)
                            .and_then(|if_e| match &if_e.kind {
                                IrExprKind::If { cond, then, else_ } => {
                                    self.try_lower_heap_result_if(cond, then, else_, bty)
                                }
                                _ => None,
                            }),
                        _ => None,
                    };
                    match merged {
                        Some(dst) => {
                            self.value_of.insert(*var, dst);
                            if !self.live_heap_handles.contains(&dst) {
                                self.live_heap_handles.push(dst);
                            }
                            continue;
                        }
                        None => {
                            stmts_ok = false;
                            break;
                        }
                    }
                }
            }
            if self.lower_stmt(s).is_err() {
                stmts_ok = false;
                break;
            }
        }
        // component0: `acc + [elem]` → a FRESH owned list (reads the List slot). try_lower_concat_list
        // returns a bare ValueId (registered in the right recursive-drop set, NOT in live_heap_handles).
        let new_list = if stmts_ok { self.try_lower_concat_list(&tail_elems[0]) } else { None };
        // component1: `n + step` → a scalar value (reads the Int slot).
        let new_int = new_list.and_then(|_| self.lower_scalar_value(&tail_elems[1]));
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let (new_list, new_int) = match (new_list, new_int) {
            (Some(l), Some(n)) => (l, n),
            _ => return None,
        };
        // Free any stray per-iteration heap temp (the concat's arg temps) within the iteration frame.
        self.drop_arm_locals(body_mark);

        // Update the LIST slot: DROP-OLD the previous value, then SetLocal. The new list inherits the
        // slot's recursive-drop set (String → heap_elem_lists) so the slot's drop stays the right kind.
        if list_elem_str {
            self.heap_elem_lists.insert(new_list);
        }
        let drop_old = self.drop_op_for(list_slot);
        self.ops.push(drop_old);
        self.ops.push(Op::SetLocal { local: list_slot, src: new_list });

        // Update the INT slot (a scalar SetLocal — no ownership event).
        self.ops.push(Op::SetLocal { local: int_slot, src: new_int });

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Build the RESULT tuple `(List_slot, Int_slot)` ONCE after the loop. The List slot's single
        // live value is MOVED into slot 0 (cert `m`); the Int slot's value is stored into slot 1. The
        // block is a masked aggregate (slot 0 heap), tracked by the caller's bind path for the
        // scope-end masked drop. The slot's recursive-drop set (heap_elem_lists for String) carries to
        // the block via `try_lower_tuple_construct`'s element handling? No — the tuple's masked drop is
        // a flat `DropListStr` rc_dec of slot 0; for a `List[String]` slot 0 the recursive element free
        // happens at the FINAL owner (the moved-out list at the `match` extraction / the caller), not
        // here, since the move-out keeps the inner list's rc ≥ 1 through the tuple's flat drop.
        //
        // `try_lower_tuple_construct` requires the slot value to be a real owned heap field — re-tag the
        // List slot as a live heap handle so its consume-into-the-tuple is the MOVE the construct emits.
        if !self.live_heap_handles.contains(&list_slot) {
            self.live_heap_handles.push(list_slot);
        }
        let tup = self.build_tuple_acc_result(list_slot, int_slot)?;
        Some(tup)
    }

    /// Build the `(List, Int)` result block of a tuple-accumulator fold: a 2-slot masked aggregate
    /// with slot 0 the MOVED-IN owned `list` handle (cert `m` — the loop slot's single live value) and
    /// slot 1 the scalar `int`. Tracked `record_masks = [0]` + `materialized_aggregates` so a later
    /// `t.0`/`(x,_)=>x` borrow reads the real slot and the scope-end drop frees slot 0 + the block.
    fn build_tuple_acc_result(&mut self, list: ValueId, int: ValueId) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: 2 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        // slot 0 = the list handle (MOVED in — extend to i64 then store64, then Consume the source).
        let off0 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off0, value: 12 });
        let addr0 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr0, op: IntOp::Add, a: h, b: off0 });
        let lh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(lh), args: vec![list] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr0, lh] });
        self.ops.push(Op::Consume { v: list });
        self.live_heap_handles.retain(|x| *x != list);
        // slot 1 = the int value (store64).
        let off1 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off1, value: 20 });
        let addr1 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr1, op: IntOp::Add, a: h, b: off1 });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr1, int] });
        // A masked aggregate: slot 0 is the heap list, slot 1 scalar. The caller's Module-bind path
        // re-derives this mask from the result type, but set it here too so the value is self-describing
        // (a nested-ownership `List[String]` slot 0 is handled by the move-out at the `match` extraction;
        // the flat masked `DropListStr` rc_dec of a still-shared list block is leak-safe — see the doc on
        // `try_lower_defunc_tuple_acc_fold`).
        self.record_masks.insert(dst, vec![0]);
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }

    /// C1 DEFUNCTIONALIZATION for `list.flat_map` / `list.filter_map` over a `List[String]` source
    /// producing a `List[String]` — the toml `emit_sections` / dojo `hints_block` shapes. Each
    /// element's closure body produces a SUBLIST (`flat_map` → `List[String]`; `filter_map` → the
    /// 0-or-1-element `Option[String]`, physically the SAME `DynListStr`-of-Strings), which is
    /// CONCATENATED onto a loop-carried `List[String]` ACCUMULATOR via the proven `__list_concat_rc`
    /// drop-old + SetLocal slot.
    ///
    /// SOUNDNESS by REUSE — the accumulator is the exact `i(id)m` loop-carried slot the heap `fold`
    /// arm proves (`OwnershipChecker.v check_line_unroll_sound`): seeded with a fresh empty `List`
    /// (`i`), each iteration `acc = __list_concat_rc(acc, sub)` is a drop-old (`d`) + SetLocal (folds
    /// the concat's `i` into the slot) — a refcount-preserving body, leak/double-free-free for any
    /// iteration count. The per-element SUBLIST is lowered as a SYNTHETIC `let` (an OWNED tracked temp
    /// — NOT a moved-out `Consume`), so my explicit `drop_arm_locals` frees it (`id`) within the
    /// iteration; `__list_concat_rc` BORROWS both args (rc-incs each element into the new list), so
    /// after the drops the elements are single-owned by `acc`. A body the let-bind subset cannot lower
    /// returns `None` → the whole HOF rolls back and the caller WALLs (caps honest).
    fn lower_defunc_str_acc_hof(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // The closure body returns `Option[String]` (filter_map) or `List[String]` (flat_map) — both a
        // `DynListStr` of Strings. Bail on an obvious non-heap body (the walker re-checks per leaf).
        if !is_heap_ty(&body.ty) {
            return None;
        }

        // Borrow the source list (evaluated once); a non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // Seed the ACCUMULATOR: a fresh EMPTY `List[String]` (an i32 Alloc dst). This is the
        // loop-carried slot — reassigned in place via SetLocal each iteration (the proven i(id)m slot).
        // NOT registered for drop here: the slot owns it; the loop's drop-old or the caller's scope-end
        // drop frees it exactly once. Marked `heap_elem_lists` so EVERY drop of it (drop-old + the
        // caller's scope-end) is the recursive `DropListStr` (frees its owned element Strings).
        let zero_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero_len, value: 0 });
        let acc = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: acc,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: zero_len },
        });
        // The acc's drop grain follows the ELEMENT class: a matrix-shaped sublist type
        // (`List[Matrix]` — repeat_kv) needs the nested DropListListStr sweep; a String
        // element the flat per-slot DropListStr.
        if crate::lower::is_list_list_str_ty(&body.ty) {
            self.list_list_str_lists.insert(acc);
        } else {
            self.heap_elem_lists.insert(acc);
        }

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8. The READ WIDTH depends on the
        // SOURCE element type, NOT the heap String OUTPUT this HOF accumulates: a HEAP source
        // (`List[String]`/`List[Value]`) element is the slot's HANDLE (`LoadHandle` = i32 Ptr, a
        // BORROWED heap value the body reads); a SCALAR source (`List[Int]` — e.g. `filter_map((x:Int)
        // => Some(int.to_string(x)))`) element is the i64 VALUE (`Load { width: 8 }`). Hardcoding
        // `LoadHandle` for a scalar source loaded each i64 Int element as an i32, corrupting every i64
        // op consuming it (`i64.rem_s`/`i64.gt_s`/`i64.eq`/`int.to_string`) → invalid wasm (the
        // filter_map heap-result-element-load-width holes). Mirrors `lower_defunc_list_hof_inner`'s
        // `src_heap` element read.
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM (the element) to the loaded value/handle. CAPTURES resolve through
        // `value_of`. Only a HEAP element is a BORROW (`param_values`) — the source list owns it, so a
        // body that MOVES it out (`some(elem)`) auto-acquires its own ref. A SCALAR element is a plain
        // i64 value (no ownership, no `param_values`) — registering it as a borrow would mis-route its
        // (non-existent) drop.
        self.value_of.insert(params[0].0, elem);
        if src_heap {
            self.param_values.insert(elem);
        }

        // Lower the closure BODY by PUSHING its control flow (if/match/block) DOWN and APPENDING each
        // terminal sublist to the loop-carried `acc` slot — NEVER binding a merged-if heap value (which
        // the flat certificate cannot drop soundly — `im·im·d`; see `lower_bind`). Each leaf is appended
        // via the proven `acc = acc + <leaf>` slot reassignment (drop-old + SetLocal = `i(id)`). A leaf
        // the subset cannot lower → `None` → the whole HOF rolls back and the caller WALLs.
        self.in_frame += 1;
        self.in_defunc_body += 1;
        let ok = self.append_body_to_str_acc(body, acc).is_some();
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if !ok {
            return None;
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // The accumulator's final value is an OWNED `List[String]` returned to the caller, which
        // registers it for the outer scope-end drop (C1 does NOT push it itself — that would
        // double-drop). It is already in `heap_elem_lists` so that drop is the recursive `DropListStr`.
        Some(acc)
    }

    /// C1 DEFUNCTIONALIZATION for a `list.filter_map` building a HEAP-but-non-String element list
    /// (`List[record]`/`List[Value]`/`List[(String,Value)]`) — the dojo `backfill_dir` shape
    /// `task_files |> list.filter_map((f) => match fs.read_text(dir+"/"+f) { ok(c) =>
    /// some(parse_task_md(f, c)), err(_) => none })`. A write-cursor result list (like `filter`)
    /// combined with a keep/skip VARIANT match (like the str-acc path, but the kept element is an
    /// OWNED record/Value MOVED into the cursor slot instead of a String appended to an accumulator).
    ///
    /// The per-element body MUST be a 2-arm variant `match subj { … }` over a self-host Option/Result
    /// CALL subject (`append_variant_match_to_result_list`): the keep arm yields `some(<elem>)` (build
    /// the element, store at the cursor, bump), the skip arm yields `none` (no-op). A non-match body,
    /// a non-self-host subject, or an out-of-subset element returns `None` → the caller rolls back +
    /// WALLs.
    ///
    /// SOUNDNESS by REUSE: the result list is alloc'd once at `len(xs)` (the MAX) with its real length
    /// patched to the write-cursor after the loop (exactly `filter`, control_p5 `filter` arm); each
    /// KEPT element is a FRESH OWNED record/Value (`lower_heap_result_arm`, cert `i`) MOVED into the
    /// slot (`Consume` = `m`), the list's recursive `DropList*` freeing all `cursor` elements at scope
    /// end — the proven capturing-filter conditional-acquire (5a0a9efb). The per-iteration subject is a
    /// balanced `i…d` episode (dropped after the arms), its payload borrowed through the keep arm.
    fn lower_defunc_filter_map_hof(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        result_elem: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        // The body is a 2-arm variant match (the keep/skip decision), OR a BLOCK whose leading lets feed
        // the tail match (`(k) => { let val = json.get_string(obj, k); match val { … } }` — porta
        // load_porta_config's CAPTURING `secrets` filter_map). The leading lets are lowered per-iteration
        // AFTER the element param is bound (captures resolve via value_of), BEFORE the match arms. Defer
        // anything else.
        let (lead_stmts, subject, arms): (&[almide_ir::IrStmt], &IrExpr, &[IrMatchArm]) = match &body.kind
        {
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                (&[], subject.as_ref(), arms.as_slice())
            }
            IrExprKind::Block { stmts, expr: Some(tail) } => match &tail.kind {
                IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                    (stmts.as_slice(), subject.as_ref(), arms.as_slice())
                }
                _ => return None,
            },
            _ => return None,
        };

        // Borrow the source list (evaluated once); a non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // A fresh OWNED `DynList` of `len(xs)` slots (the MAX; the real length is patched to the
        // write-cursor after the loop). The recursive scope-end drop set follows the element type,
        // exactly like the `map` heap-element result: a `(String, Value)` tuple → DropListStrValue, a
        // dynamic Value → DropListValue, else (a String or a record handle) → the recursive DropListStr
        // (rc_dec each owned element handle — the convention the C2-lifted record `map` already uses).
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: len_v },
        });
        let rh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(rh), args: vec![dst] });
        let result_is_str_value_tuple = matches!(result_elem,
            Ty::Tuple(tys) if tys.len() == 2
                && matches!(tys[0], Ty::String) && crate::lower::is_value_ty(&tys[1]));
        if result_is_str_value_tuple {
            self.str_value_elem_lists.insert(dst);
        } else if crate::lower::is_value_ty(result_elem) {
            self.value_elem_lists.insert(dst);
        } else {
            self.heap_elem_lists.insert(dst);
        }
        // The write-cursor (count of kept elements) — a stable mutable local.
        let cursor = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: cursor, value: 0 });

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list (a handle for a heap source, an i64 value for a scalar
        // source) — mirrors `lower_defunc_list_hof_inner`'s `src_heap` element read.
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM (the element). Captures resolve through `value_of`. A HEAP element is a
        // BORROW (`param_values`); a heap AGGREGATE is also a materialized aggregate (so a `let
        // (k,v)=pair` destructure borrows its slots).
        self.value_of.insert(params[0].0, elem);
        if src_heap {
            self.param_values.insert(elem);
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a) = &xs.ty {
                if a.len() == 1
                    && (matches!(&a[0], Ty::Tuple(_)) || self.aggregate_field_tys(&a[0]).is_some())
                {
                    self.materialized_aggregates.insert(elem);
                }
            }
        }

        // Lower the per-element keep/skip variant match into the write-cursor result list. A Block body's
        // leading lets (`let val = json.get_string(obj, k)`) are lowered FIRST in this per-iteration frame
        // (their captures resolve via value_of, the element param is bound above) so the tail match's
        // subject (`val`) is in scope; their own heap temps are freed within the iteration frame.
        self.in_frame += 1;
        self.in_defunc_body += 1;
        let mut lead_ok = true;
        for stmt in lead_stmts {
            if self.lower_stmt(stmt).is_err() {
                lead_ok = false;
                break;
            }
        }
        let ok = lead_ok
            && self
                .append_variant_match_to_result_list(subject, arms, rh, cursor, result_elem, eight)
                .is_some();
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if !ok {
            return None;
        }

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Patch the result list's `len` field (offset 4) to the write-cursor (the count of kept
        // elements); the unused tail slots are harmless (a `${list}`/`xs[i]` reads `len`).
        let four = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: four, value: 4 });
        let lenaddr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 4 },
            dst: None,
            args: vec![lenaddr, cursor],
        });
        Some(dst)
    }

    /// Walk a `flat_map`/`filter_map` closure BODY, pushing its control flow (if / match / block)
    /// DOWN and appending each TERMINAL sublist to the loop-carried `acc` slot. The body returns
    /// `List[String]` (flat_map) or `Option[String]` (filter_map); each terminal is one of:
    ///   - `some(e)` / a singleton/multi `List` literal → append those elements
    ///   - `none` / `[]` → no-op
    ///   - a `List[String]` concat / call / `??` → append the whole (owned, droppable) sublist
    /// Returns `Some(())` on success, `None` (the caller rolls back + WALLs) for any out-of-subset
    /// leaf. CRUCIAL: a merged-if/match heap value is NEVER bound — the if/match is a UNIT control
    /// structure with appends in the arms, so the unsound `im·im·d` flat-cert shape never arises.
    fn append_body_to_str_acc(&mut self, body: &IrExpr, acc: ValueId) -> Option<()> {
        match &body.kind {
            // `e!` (effect-fn error propagation) is the identity on its inner sublist.
            IrExprKind::Unwrap { expr } => self.append_body_to_str_acc(expr, acc),
            // A block `{ stmts; tail }`: lower the statements as effects in a per-arm frame (their heap
            // locals ride to scope end / are dropped by `drop_arm_locals`), then recurse the tail.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let arm_mark = self.live_heap_handles.len();
                for s in stmts {
                    self.lower_stmt(s).ok()?;
                }
                let r = self.append_body_to_str_acc(tail, acc);
                self.drop_arm_locals(arm_mark);
                r
            }
            // `if cond then T else E`: a UNIT control structure — lower cond to a scalar bool, then
            // recurse each arm as an append (only the taken arm runs). NO merged heap value. The cond's
            // own transient heap temps (`x == ""` → a `""` literal) are freed in a cond-LOCAL frame
            // BEFORE the `IfThen` (`lower_heap_result_cond`) — never deferred into an arm, where the
            // sibling branch would leave them uninitialized (the nested-if `rc_dec`-of-garbage trap).
            IrExprKind::If { cond, then, else_ } => {
                let cond_v = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: cond_v, dst: None });
                self.unit_arm_depth += 1;
                let then_ok = self.append_body_to_str_acc(then, acc);
                self.ops.push(Op::Else { val: None });
                let else_ok = then_ok.and_then(|_| self.append_body_to_str_acc(else_, acc));
                self.unit_arm_depth -= 1;
                self.ops.push(Op::EndIf { val: None });
                else_ok
            }
            // A `match` desugaring to a Bool/literal `if` chain (NOT a variant some/none match — that
            // is the variant-payload-bind frontier, deferred here). Recurse the desugared if.
            IrExprKind::Match { subject, arms } if !is_variant_ty(&subject.ty) => {
                let if_expr = self.desugar_match_to_if(subject, arms, &body.ty)?;
                self.append_body_to_str_acc(&if_expr, acc)
            }
            // A VARIANT `match subj { some(pl) => …, none => … }` over a per-element `Option[Value]`
            // (`match json.get(case, "payload") { some(pl) => if get_kind(pl)=="tuple" then <map>
            // else [], none => [] }` — the bindgen `gen_variant_type/struct/class` shape). A UNIT
            // control structure (NO merged value): tag-read @4, then APPEND each arm into `acc` by
            // recursing the walker — never bind a merged heap-if value. The Some payload `pl` is the
            // subject's BORROWED slot-0 handle (`param_values`), live through the some-arm; the
            // per-iteration subject is dropped AFTER both arms. See `append_variant_match_to_str_acc`.
            IrExprKind::Match { subject, arms } if is_variant_ty(&subject.ty) => {
                self.append_variant_match_to_str_acc(subject, arms, acc)
            }
            // `none` / `[]` (empty) — append nothing.
            IrExprKind::OptionNone => Some(()),
            IrExprKind::List { elements } if elements.is_empty() => Some(()),
            // `some(e)` — append the singleton `[e]`: lower the String payload to a FRESH OWNED value
            // (a LitStr / concat / `${interp}` / a Dup'd Var / a call result), store it into a
            // 1-element `List[String]` (an owned droppable sublist), then append + free it.
            IrExprKind::OptionSome { expr } => {
                let arm_mark = self.live_heap_handles.len();
                let piece = self.lower_owned_str_payload(expr)?;
                let sub = self.materialize_str_singleton(piece);
                self.append_owned_sub_to_acc(sub, acc, arm_mark)
            }
            // A `List[String]` literal / call / `??` leaf — an OWNED, DROPPABLE sublist (a single `i`,
            // NOT a merged-if). Lower it owned, then append + free it.
            IrExprKind::List { .. } | IrExprKind::Call { .. } | IrExprKind::UnwrapOr { .. } => {
                let arm_mark = self.live_heap_handles.len();
                let sub = self.lower_owned_str_sublist(body)?;
                self.append_owned_sub_to_acc(sub, acc, arm_mark)
            }
            // A `left + right` ConcatList leaf. FIRST try the whole concat as ONE owned sublist (the
            // common `acc_list + [x]` shape — unchanged). If that DECLINES (an operand is itself a
            // UNIT control structure that has no owned-value form — the julia `match {…} + [""]`
            // shape: the left is a variant match the str-acc walker appends, not materializes),
            // APPEND each operand IN ORDER instead: `acc += left; acc += right`. Order-preserving
            // (left's elements then right's, exactly `left + right`) and each operand is an
            // independently-balanced append (the per-leaf `i(id)` discipline), so the concat is the
            // same loop-carried `i(id)m` whether materialized whole or appended piecewise. Both
            // operands must be appendable; otherwise the whole HOF rolls back + WALLs.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, left, right } => {
                let ops_mark = self.ops.len();
                let arm_mark = self.live_heap_handles.len();
                if let Some(sub) = self.lower_owned_str_sublist(body) {
                    return self.append_owned_sub_to_acc(sub, acc, arm_mark);
                }
                // Roll back EVERY op + handle the declined whole-concat attempt pushed (a partial
                // `try_lower_concat_list` may have emitted ops before failing), then append the two
                // operands separately (left then right — concat order).
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(arm_mark);
                self.append_body_to_str_acc(left, acc)?;
                self.append_body_to_str_acc(right, acc)
            }
            // A bare `Var` leaf (`{ let lines = …; lines }` — the flat_map body that binds its
            // per-element sublist to a `let` then returns it, the bindgen `gen_unpack_named` /
            // `gen_variant_type` shape). The Var is an OWNED, tracked `List[String]` local (its `let`
            // pushed it to `live_heap_handles` + `heap_elem_lists`, freed by the ENCLOSING block's
            // `drop_arm_locals`). APPEND it to `acc` by BORROW: `__list_concat_rc(acc, lines)` rc-incs
            // its elements into the new acc, then drop-old acc + SetLocal — exactly the owned-sublist
            // append, but WITHOUT taking ownership of `lines` (no `Consume`, no per-leaf
            // `drop_arm_locals` over it). `lines` is freed EXACTLY ONCE by its own `let` scope (the
            // block teardown), and the concat co-acquired its elements into `acc` — no double-free, no
            // leak. A BORROWED Var (a param/loop-element in `param_values`, owned elsewhere) is also
            // safe: the concat only borrows it, and we never free it here. An untracked/global Var (no
            // `value_for`) declines → the HOF rolls back + the caller WALLs (honest).
            IrExprKind::Var { id } => {
                let sub = self.value_for(*id).ok()?;
                let new = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(new),
                    name: "__list_concat_rc".to_string(),
                    args: vec![CallArg::Handle(acc), CallArg::Handle(sub)],
                    result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
                });
                self.heap_elem_lists.insert(new);
                let drop_acc = self.drop_op_for(acc);
                self.ops.push(drop_acc);
                self.ops.push(Op::SetLocal { local: acc, src: new });
                Some(())
            }
            _ => None,
        }
    }

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
        self.materialized_options.insert(subj);
        if crate::lower::is_heap_elem_list_ty(&subject.ty) {
            self.heap_elem_lists.insert(subj);
        }
        // Parse the arms into (some_body, some_bind, none_body). A SCALAR payload binds a value COPY;
        // a HEAP payload (`pl: Value`) binds the slot-0 @12 handle as a BORROW, gated on the subject
        // being a tracked nested-ownership list (`heap_elem_lists`). A nested ctor / heap bind over a
        // non-nested-ownership subject declines.
        let mut some: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut none: Option<&IrExpr> = None;
        for arm in arms {
            match &arm.pattern {
                IrPattern::Some { inner } => {
                    let bind = match inner.as_ref() {
                        IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Some((*var, false)),
                        IrPattern::Bind { var, ty }
                            if is_heap_ty(ty) && self.heap_elem_lists.contains(&subj) =>
                        {
                            Some((*var, true))
                        }
                        IrPattern::Wildcard => None,
                        _ => {
                            self.ops.truncate(ops_mark);
                            self.live_heap_handles.truncate(lhh_mark);
                            return None;
                        }
                    };
                    if some.is_some() {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                    some = Some((&arm.body, bind));
                }
                IrPattern::None | IrPattern::Wildcard => {
                    if none.is_some() {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    }
                    none = Some(&arm.body);
                }
                _ => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        let ((some_body, some_bind), none_body) = match (some, none) {
            (Some(s), Some(n)) => (s, n),
            _ => {
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
        if let Some((bind_var, is_heap)) = some_bind {
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
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
        Some(())
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
        rh: ValueId,
        cursor: ValueId,
        result_elem: &Ty,
        eight: ValueId,
    ) -> Option<()> {
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
        // Track the subject EXACTLY as `try_lower_variant_value_match` (control_p2): a self-host or
        // user `Named` Option/Result, with the type-driven drop set so the per-iteration subject drop
        // frees its owned payload correctly. The arm tag arrangement is the uniform skeleton then=tag≠0
        // / else=tag==0 (Option → then=Some/else=None; Result → then=Err/else=Ok).
        let is_named_call =
            matches!(&subject.kind, IrExprKind::Call { target: CallTarget::Named { .. }, .. });
        if is_self_host_option_call(subject)
            || (is_named_call
                && is_variant_ty(&subject.ty)
                && !crate::lower::is_result_ty(&subject.ty))
        {
            self.materialized_options.insert(subj);
            if crate::lower::is_heap_elem_list_ty(&subject.ty) {
                self.heap_elem_lists.insert(subj);
            }
        }
        if is_self_host_result_call(subject)
            || (is_named_call
                && crate::lower::is_result_ty(&subject.ty)
                && !Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results.insert(subj);
            // Scalar-Ok / heap-Err `Result[Int, String]` (the byte-match fixture's `mkResult`): the
            // len-as-tag read stays @4, but track heap_elem_lists so the Err arm's String payload drops
            // via DropListStr (Ok=len0 frees nothing, Err=len1 frees slot-0's String).
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a) =
                &subject.ty
            {
                if a.len() == 2 && !is_heap_ty(&a[0]) && is_heap_ty(&a[1]) {
                    self.heap_elem_lists.insert(subj);
                }
            }
        }
        if is_self_host_result_str_call(subject)
            || (is_named_call && Self::is_heap_ok_result(&subject.ty))
        {
            self.materialized_results_str.insert(subj);
            if crate::lower::is_result_listval_ty(&subject.ty) {
                self.value_result_lists.insert(subj);
            } else if crate::lower::is_value_result_ty(&subject.ty) {
                self.value_result_results.insert(subj);
            } else if crate::lower::is_str_int_result_ty(&subject.ty) {
                self.str_int_result_results.insert(subj);
            } else if crate::lower::is_value_int_result_ty(&subject.ty) {
                self.value_int_result_results.insert(subj);
            } else if crate::lower::is_list_str_int_result_ty(&subject.ty) {
                self.list_str_int_result_results.insert(subj);
            } else if crate::lower::is_list_value_int_result_ty(&subject.ty) {
                self.list_value_int_result_results.insert(subj);
            } else {
                self.heap_elem_lists.insert(subj);
            }
        }
        let is_option = self.materialized_options.contains(&subj);
        let is_result_str = self.materialized_results_str.contains(&subj);
        let is_result = self.materialized_results.contains(&subj) || is_result_str;
        if !is_option && !is_result {
            return rollback(self);
        }
        let tag_off = if is_result_str { 16 } else { 4 };
        // Parse the arms into (then_body, then_bind) [tag != 0] and (else_body, else_bind) [tag == 0],
        // the uniform skeleton: Option → then=Some / else=None; Result → then=Err / else=Ok. A heap
        // payload binds the @12 handle as a BORROW (gated on the subject being a nested-ownership list);
        // a scalar payload a value copy; a wildcard nothing.
        let heap_or_scalar_bind = |s: &Self, inner: &IrPattern| -> Result<Option<(VarId, bool)>, ()> {
            match inner {
                IrPattern::Bind { var, ty } if !is_heap_ty(ty) => Ok(Some((*var, false))),
                IrPattern::Bind { var, ty }
                    if is_heap_ty(ty)
                        && (s.heap_elem_lists.contains(&subj)
                            || s.value_result_lists.contains(&subj)
                            || s.value_result_results.contains(&subj)) =>
                {
                    Ok(Some((*var, true)))
                }
                IrPattern::Wildcard => Ok(None),
                _ => Err(()),
            }
        };
        let mut then_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        let mut else_slot: Option<(&IrExpr, Option<(VarId, bool)>)> = None;
        for arm in arms {
            let parsed: Result<(bool, Option<(VarId, bool)>), ()> = match &arm.pattern {
                IrPattern::Some { inner } if is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::None | IrPattern::Wildcard if is_option => Ok((false, None)),
                IrPattern::Err { inner } if !is_option => {
                    heap_or_scalar_bind(self, inner).map(|b| (true, b))
                }
                IrPattern::Ok { inner } if !is_option => {
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
        // tag = load32(handle(subj) + tag_off); bind payload(s) BEFORE the IfThen (in scope for the
        // arm that reads them); then a UNIT IfThen (dst None) with per-arm keep/skip.
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        let bind_payload = |s: &mut Self, bind: Option<(VarId, bool)>| {
            if let Some((bind_var, is_heap)) = bind {
                let payload = if is_heap {
                    s.load_at_offset(h, 12, PrimKind::LoadHandle)
                } else {
                    s.load_at_offset(h, 12, PrimKind::Load { width: 8 })
                };
                s.value_of.insert(bind_var, payload);
                if is_heap {
                    s.param_values.insert(payload);
                }
            }
        };
        bind_payload(self, then_bind);
        bind_payload(self, else_bind);
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
        if let Some(pos) = self.live_heap_handles.iter().rposition(|&v| v == subj) {
            self.live_heap_handles.remove(pos);
            let op = self.drop_op_for(subj);
            self.ops.push(op);
        }
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

    /// C1 DEFUNCTIONALIZATION for `list.find` — the EARLY-EXIT scan `xs |> list.find((t) =>
    /// <pred>)` (the gguf/ggml `find_tensor` / `get_metadata_*` shape, whose predicate CAPTURES
    /// the search key, so the lift path can never serve it). A specialized loop over the source
    /// with the FIRST match written into a pre-allocated 0-or-1 OPTION block (the len-as-tag
    /// layout: 1 slot, `len@4` starts 0 = none, overwritten to 1 = some on the hit), then an
    /// immediate loop break.
    ///
    /// SOUNDNESS by REUSE: the option block is ONE fresh owned Alloc (cert `i`) the caller
    /// binds/moves like any self-host Option result; the hit arm's payload acquire is the
    /// per-arm `Dup` (+1, cert `a`) + `Consume` (move-in, `m`) the heap-result-if arms prove —
    /// executed at most once (the break). The predicate lowers via `lower_heap_result_cond`
    /// (scalar / heap-eq / pure-call conds; its transient temps are freed within the cond
    /// frame). Out of subset → `None` (the caller rolls back + WALLs — never invalid wasm).
    /// A RICH record payload routes the option's drop through `optrec:<R>` (the
    /// `materialize_opt_aggregate_some` convention); a matrix-shaped payload has no two-level
    /// option drop yet → defers.
    fn try_lower_defunc_find(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::PrimKind;
        use almide_lang::types::constructor::TypeConstructorId;
        if params.len() != 1 {
            return None;
        }
        let elem_ty = match &xs.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        if !matches!(result_ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1) {
            return None;
        }
        // A matrix-shaped payload (`Option[Matrix]` / `Option[List[List[Float]]]`) would need a
        // two-level option drop this brick does not route — defer (never a row leak).
        if matches!(&elem_ty, Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))
            || crate::lower::is_list_list_str_ty(&Ty::Applied(
                TypeConstructorId::List,
                vec![elem_ty.clone()],
            ))
        {
            return None;
        }
        let elem_heap = is_heap_ty(&elem_ty);
        // Borrow the source list (evaluated once).
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // The result OPTION block: 1 slot, len@4 overwritten to 0 (none) until a hit.
        let one_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_len, value: 1 });
        let opt = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: opt,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: one_len },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![opt] });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let len_addr = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![len_addr, zero] });

        // Loop index + step.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // elem = xs[i] — a borrowed handle (heap) or the i64 value (scalar).
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if elem_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });
        self.value_of.insert(params[0].0, elem);
        if elem_heap {
            self.param_values.insert(elem);
            // An aggregate element's field read (`t.name`) borrows its real slot.
            if matches!(&elem_ty, Ty::Tuple(_)) || self.aggregate_field_tys(&elem_ty).is_some() {
                self.materialized_aggregates.insert(elem);
            }
            self.seed_variant_param(elem, &elem_ty);
        }

        // The predicate (Bool) — scalar / heap-eq / pure-call conds; temps freed in the frame.
        let pred = self.lower_heap_result_cond(body)?;

        // On the hit: write the payload into slot 0, flip len to 1 (some), then break.
        self.ops.push(Op::IfThen { cond: pred, dst: None });
        let slot_addr = self.load_addr(oh, 12);
        if elem_heap {
            let dup = self.fresh_value();
            self.ops.push(Op::Dup { dst: dup, src: elem });
            let ph = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![dup] });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![slot_addr, ph] });
            self.ops.push(Op::Consume { v: dup });
        } else {
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![slot_addr, elem] });
        }
        let one_tag = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_tag, value: 1 });
        let len_addr2 = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![len_addr2, one_tag] });
        self.ops.push(Op::Else { val: None });
        self.ops.push(Op::EndIf { val: None });

        // Break when found (LoopBreakUnless breaks on 0 → pass 1 - pred), else advance.
        let not_pred = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: not_pred, op: IntOp::Sub, a: one_v, b: pred });
        self.ops.push(Op::LoopBreakUnless { cond: not_pred });
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Drop routing: a RICH record payload frees through the option-wrapper recursion
        // (`optrec:<R>` → `$__drop_<R>` at the wrapper's last ref); a flat heap payload
        // (String / List[scalar] / flat record) keeps the caller's per-slot classification
        // (`heap_elem_lists` — DropListStr over the 0-or-1 slot). The option READS as a
        // materialized 0-or-1 list either way.
        if let Some(rn) = self.record_or_anon_drop_type_name(&elem_ty) {
            self.variant_drop_handles.insert(opt, format!("optrec:{rn}"));
        } else if elem_heap {
            self.heap_elem_lists.insert(opt);
        }
        self.materialized_lists.insert(opt);
        Some(opt)
    }

    /// Lower a `List[String]`-valued LEAF (a list literal, a `+` concat, a named call, or a `??`) to
    /// a FRESH OWNED, TRACKED sublist (in `live_heap_handles` with the recursive `DropListStr` drop) —
    /// a single allocation (`i`), so it is soundly droppable (unlike a merged-if). `None` out of subset.
    fn lower_owned_str_sublist(&mut self, leaf: &IrExpr) -> Option<ValueId> {
        let v = match &leaf.kind {
            // A list LITERAL / `+` CONCAT: the builders mark the right recursive-drop set
            // (`heap_elem_lists` for `List[String]`) but do NOT push to `live_heap_handles` — the
            // uniform registration below does, so `append_owned_sub_to_acc`'s `drop_arm_locals` frees
            // the per-leaf sublist (`id`, not a LEAK).
            IrExprKind::List { .. } => self.try_lower_str_list_literal(leaf)?,
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                self.try_lower_concat_list(leaf)?
            }
            IrExprKind::UnwrapOr { expr, fallback } => {
                // `(value.as_array(val) ?? [])` etc. — the option-unwrap helper returns a fresh owned
                // list (track_result=true registers it in live_heap_handles + the recursive drop set).
                self.try_lower_option_unwrap_or(expr, fallback, true)?
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&leaf.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                // A `List[String]` call result is a fresh owned nested-ownership list — mark its
                // recursive drop (the uniform registration below adds the scope-end free). A
                // matrix-shaped sublist (`List[Matrix]`) needs the nested DropListListStr grain.
                if crate::lower::is_list_list_str_ty(&leaf.ty) {
                    self.list_list_str_lists.insert(dst);
                } else {
                    self.heap_elem_lists.insert(dst);
                }
                dst
            }
            // A `Module` call (`(... ?? []) |> list.flat_map(...)` — the NESTED flat_map leaf): route
            // through the pure-module-call path, which re-enters the defunc HOF lowering and returns a
            // fresh OWNED `List[String]`. Mark it for the recursive drop.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let dst = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &leaf.ty)
                    .ok()?;
                if crate::lower::is_list_list_str_ty(&leaf.ty) {
                    self.list_list_str_lists.insert(dst);
                } else {
                    self.heap_elem_lists.insert(dst);
                }
                dst
            }
            _ => return None,
        };
        // UNIFORM REGISTRATION: every leaf sublist is a FRESH OWNED block — track it in
        // `live_heap_handles` (idempotent) so the per-leaf `drop_arm_locals` after the concat frees it
        // exactly once. WITHOUT this a list-literal/concat leaf (the builders skip the push) would be
        // an `i` with no `d` — a LEAK the proven checker REJECTs.
        if !self.live_heap_handles.contains(&v) {
            self.live_heap_handles.push(v);
        }
        Some(v)
    }

    /// Lower a `some(...)` String PAYLOAD to a FRESH OWNED String handle (rc 1, droppable): a string
    /// literal, a `+` concat, a `${...}` interpolation, a Dup'd tracked/borrowed Var, or a String-
    /// returning named/module call. `None` for any other shape (the HOF then rolls back + WALLs).
    fn lower_owned_str_payload(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            IrExprKind::LitStr { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(value.clone()),
                });
                Some(dst)
            }
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => self.try_lower_concat_str(expr),
            IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
            // A Var (the borrowed element or a let-local) — Dup a fresh owned reference the singleton
            // will own; the original keeps its own reference (dropped at its own scope).
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&expr.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, &expr.ty).ok()
            }
            // `option ?? "fallback"` — a String-returning unwrap (track_result=false: the result is
            // moved into the singleton below, not registered separately).
            IrExprKind::UnwrapOr { expr: inner, fallback } => {
                self.try_lower_option_unwrap_or(inner, fallback, false)
            }
            _ => None,
        }
    }

    /// Materialize a 1-element `List[String]` owning `piece` (an owned String handle MOVED into slot 0)
    /// — a droppable sublist tracked in `live_heap_handles` (recursive `DropListStr`). The SAME block
    /// `materialize_opt_str_some` builds, but pushed for the per-leaf `drop_arm_locals` free.
    fn materialize_str_singleton(&mut self, piece: ValueId) -> ValueId {
        use crate::PrimKind;
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let obj = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: obj,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynListStr { len: one },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![obj] });
        let twelve = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: twelve, value: 12 });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: oh, b: twelve });
        let ph = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph), args: vec![piece] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, ph] });
        self.ops.push(Op::Consume { v: piece });
        self.live_heap_handles.retain(|h| *h != piece);
        self.heap_elem_lists.insert(obj);
        self.live_heap_handles.push(obj);
        obj
    }

    /// Append an OWNED, DROPPABLE sublist `sub` (created since `arm_mark`) to the loop-carried `acc`
    /// slot: `new = __list_concat_rc(acc, sub)` (borrows both, rc-incs each element into the new list),
    /// then DROP-OLD `acc` + SetLocal the slot to `new` (the proven `i(id)` loop-carried append), then
    /// free `sub` (and any other per-leaf temps) via `drop_arm_locals`. The element refcounts the old
    /// `acc` and `sub` held are released by their drops; the concat already co-acquired them into `new`.
    fn append_owned_sub_to_acc(&mut self, sub: ValueId, acc: ValueId, arm_mark: usize) -> Option<()> {
        let new = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(new),
            name: "__list_concat_rc".to_string(),
            args: vec![CallArg::Handle(acc), CallArg::Handle(sub)],
            result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        // `new` inherits the acc's drop grain (a matrix-elem acc sweeps two levels).
        if self.list_list_str_lists.contains(&acc) {
            self.list_list_str_lists.insert(new);
        } else {
            self.heap_elem_lists.insert(new);
        }
        // DROP-OLD the previous accumulator value, then rebind the slot IN PLACE to `new`. `SetLocal`
        // folds `new`'s `i` into the slot stream (the loop-carried `i(id)m`).
        let drop_acc = self.drop_op_for(acc);
        self.ops.push(drop_acc);
        self.ops.push(Op::SetLocal { local: acc, src: new });
        // Free the per-leaf sublist + any helper temps (the concat borrowed them; their elements are
        // now co-owned by `new`). `drop_arm_locals` emits the recursive `DropListStr` — the balanced
        // `id` for these per-iteration owned temps.
        self.drop_arm_locals(arm_mark);
        Some(())
    }

    /// `base + offset` as a fresh value (the address-arithmetic half of `load_at_offset`,
    /// without the load — used when the loaded address feeds further arithmetic).
    fn load_addr(&mut self, base: ValueId, offset: i64) -> ValueId {
        let off = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off, value: offset });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: off });
        addr
    }

    pub(crate) fn lower_while(&mut self, cond: &IrExpr, body: &[IrStmt]) -> Result<(), LowerError> {
        // First try to EXECUTE it as a real scalar-state loop; on any out-of-subset
        // feature this rolls back cleanly and we reach the model-one-iteration form below.
        if self.try_lower_scalar_while(cond, body) {
            return Ok(());
        }
        // The fallback below runs the body straight-line ONCE (the model-one-iteration
        // form). A `break`/`continue` (no early-exit branch) and a HEAP ACCUMULATOR
        // reassignment (deferred → the accumulation is dropped) BOTH make that one
        // iteration produce the wrong answer — WALL them rather than silently miscompile.
        // (Walling BEFORE lowering the body avoids emitting partial ops; the executable
        // `try_lower_scalar_while` already declined both shapes and rolled back.)
        self.wall_break_over_heap_frame(body, "while", self.live_heap_handles.len())?;
        if body_reassigns_heap(body) {
            return Err(LowerError::Unsupported(
                "while body with a heap-accumulator reassignment cannot be faithfully lowered \
                 (the model-one-iteration fallback defers the reassignment, dropping the \
                 accumulation) not in this brick"
                    .into(),
            ));
        }
        self.record_elided_calls(cond);
        let mark = self.live_heap_handles.len();
        self.in_frame += 1;
        for stmt in body {
            self.lower_stmt(stmt)?;
        }
        self.in_frame -= 1;
        self.drop_arm_locals(mark);
        Ok(())
    }

    /// Post-lowering loop-body admission for `break`/`continue` reaching the
    /// MODEL-ONE-ITERATION fallback (the executable `try_lower_scalar_*` paths already
    /// decline a break/continue body and roll back, so this is only hit when the loop
    /// linearizes to one modeled iteration). That fallback runs the body straight-line
    /// ONCE with NO loop and NO early-exit branch, so it CANNOT honor an early exit: the
    /// break/continue is silently dropped and the loop produces the wrong answer (e.g.
    /// `while i<100 { if i==7 then break; i=i+1 }; print(i)` → v0 `7`, the one-iteration
    /// form `1`). WALL it — a break/continue is faithfully executed only by the real-loop
    /// markers (`try_lower_scalar_while`/`_for_*`), which do not yet cover early exits.
    /// (This SUBSUMES the prior heap-frame leak wall: a heap-frame early exit would also
    /// skip a per-iteration Drop, but the selection bug walls every break/continue first.)
    pub(crate) fn wall_break_over_heap_frame(
        &self,
        body: &[IrStmt],
        what: &str,
        _mark: usize,
    ) -> Result<(), LowerError> {
        if body_breaks_or_continues(body) {
            return Err(LowerError::Unsupported(format!(
                "{what} body with break/continue cannot be faithfully lowered (the model-one-iteration fallback runs the body once with no early-exit branch, losing the break/continue) not in this brick"
            )));
        }
        Ok(())
    }
}

impl LowerCtx {
    /// Lower a fold-accumulator BODY that is a direct CALL (`(h, k) => step(h, k)` —
    /// the transformer layer fold) to a BARE fresh owned heap value: a Named user fn
    /// via `CallFn`, a pure Module fn via the routed self-host name. The result is
    /// NOT registered for scope-end drop — the caller's drop-old + `SetLocal` makes
    /// the loop slot its single owner. Unwraps a trivial `{ <call> }` block.
    fn try_lower_fold_acc_call(&mut self, body: &IrExpr) -> Option<ValueId> {
        use almide_ir::IrExprKind;
        let inner = match &body.kind {
            IrExprKind::Block { stmts, expr: Some(e) } if stmts.is_empty() => e.as_ref(),
            _ => body,
        };
        match &inner.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&inner.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if crate::purity::is_pure(module.as_str(), func.as_str()) =>
            {
                let mark = self.live_heap_handles.len();
                let v = self.lower_pure_module_value_call(module, func, args, &inner.ty).ok()?;
                // lower_pure_module_value_call tracks its result for scope-end drop —
                // the slot must be the SINGLE owner, so untrack it (bare).
                if let Some(pos) = self.live_heap_handles.iter().rposition(|&h| h == v) {
                    self.live_heap_handles.remove(pos);
                }
                let _ = mark;
                Some(v)
            }
            _ => None,
        }
    }
}

impl LowerCtx {
    /// C1 defunc for a SCALAR-TUPLE accumulator fold — the argmax idiom:
    /// `enumerate(xs) |> fold((0, -1.0e308), (acc, entry) => { let (bi,bv)=acc;
    /// let (i,v)=entry; if v > bv then (i,v) else (bi,bv) })`. The accumulator's
    /// two SCALAR components live in two mutable locals (no tuple block per
    /// iteration); the body's tail must be a component-wise tuple (optionally
    /// under one `if`). After the loop the pair is materialized ONCE as a real
    /// 2-slot block (registered as a materialized aggregate, so a downstream
    /// `.0`/`.1` projection reads the real slot). Fully rolled back on any
    /// out-of-subset shape (the caller walls).
    #[allow(clippy::too_many_arguments)]
    fn try_lower_defunc_scalar_tuple_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        fuse_index: Option<VarId>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_ir::{IrPattern, IrStmtKind};
        // Accumulator type: a 2-tuple of scalars; the result is the same tuple.
        let (t1, t2) = match result_ty {
            Ty::Tuple(ts) if ts.len() == 2 && !is_heap_ty(&ts[0]) && !is_heap_ty(&ts[1]) => {
                (ts[0].clone(), ts[1].clone())
            }
            _ => return None,
        };
        let _ = (&t1, &t2);
        // init = (e1, e2), both scalar-lowerable.
        let IrExprKind::Tuple { elements: init_elems } = &init.kind else { return None };
        if init_elems.len() != 2 {
            return None;
        }
        // body = Block{ [let (a1, a2) = acc, ...maybe nothing else], tail }
        let acc_var = params[0].0;
        let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else { return None };
        if stmts.is_empty() {
            return None;
        }
        // stmts[0] must be the acc destructure; any FURTHER stmts (`let a = …; let rank = …`
        // — the best_pair_index preamble) lower per-iteration via the ordinary stmt
        // machinery inside the loop (their heap temps freed within the iteration).
        let extra_stmts = &stmts[1..];
        let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value } =
            &stmts[0].kind
        else {
            return None;
        };
        if pats.len() != 2 || !matches!(&value.kind, IrExprKind::Var { id } if *id == acc_var) {
            return None;
        }
        let a1 = match &pats[0] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        let a2 = match &pats[1] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        // tail: an if-TREE whose every leaf is a 2-tuple (`if a then (..) else if b
        // then (..) else (..)` — the find_chunk chain). PROJECT the tree per
        // component: the same conditions, each leaf replaced by its idx-th element
        // (conditions are pure scalar expressions — the scalar path admits nothing
        // effectful — so evaluating them once per component is value-identical).
        fn project(e: &IrExpr, idx: usize, comp_ty: &Ty) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::Tuple { elements } if elements.len() == 2 => {
                    Some(elements[idx].clone())
                }
                IrExprKind::If { cond, then, else_ } => {
                    let t = project(then, idx, comp_ty)?;
                    let el = project(else_, idx, comp_ty)?;
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: cond.clone(),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: comp_ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                _ => None,
            }
        }
        let proj1 = project(tail, 0, &t1)?;
        let proj2 = project(tail, 1, &t2)?;

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let vo_snapshot = self.value_of.clone();
        let mut fail = || -> Option<ValueId> {
            None
        };
        let _ = &mut fail;
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.value_of = vo_snapshot;
                return None;
            }};
        }

        // Seed the two component locals.
        let s1 = match self.lower_scalar_value(&init_elems[0]) {
            Some(v) => v,
            None => bail!(),
        };
        let s2 = match self.lower_scalar_value(&init_elems[1]) {
            Some(v) => v,
            None => bail!(),
        };
        self.value_of.insert(a1, s1);
        self.value_of.insert(a2, s2);

        // Borrow the source, read the length.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok().and_then(|mut a| a.pop())
        {
            Some(CallArg::Handle(v)) => v,
            _ => bail!(),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // elem = xs[i] (a scalar slot or a borrowed heap handle).
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: i8_v });
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let elem = self.fresh_value();
        let rk = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: rk, dst: Some(elem), args: vec![addr] });
        self.value_of.insert(params[1].0, elem);
        if let Some(iv) = fuse_index {
            self.value_of.insert(iv, i_v);
        }

        // Per-iteration preamble stmts, then per-component evaluation of the projected
        // trees (both read the PRE-update locals), then a simultaneous SetLocal pair.
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let updates: Option<(ValueId, ValueId)> = (|| {
            for st in extra_stmts {
                if self.lower_stmt(st).is_err() {
                    return None;
                }
            }
            let v1 = self.lower_scalar_value(&proj1)?;
            let v2 = self.lower_scalar_value(&proj2)?;
            Some((v1, v2))
        })();
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let (n1, n2) = match updates {
            Some(p) => p,
            None => {
                self.value_of = vo_snapshot;
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // Free the iteration's owned temps (the `?? ""` copies) before the back-edge.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::SetLocal { local: s1, src: n1 });
        self.ops.push(Op::SetLocal { local: s2, src: n2 });
        self.ops.push(Op::IntBinOp { dst: i_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::LoopEnd);

        // Materialize the resulting tuple ONCE (2 uniform slots) and register its
        // read shape so a downstream `.0`/`.1` loads the real slot.
        let two = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: two, value: 2 });
        let tup = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: tup,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: two },
        });
        let th = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
        let sl0 = self.load_addr(th, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![sl0, s1] });
        let sl1 = self.load_addr(th, 20);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![sl1, s2] });
        // NOT pushed to live_heap_handles — the CALLER tracks the returned value
        // exactly like a self-host combinator result (a second push double-drops).
        self.materialized_aggregates.insert(tup);
        self.last_call_had_unlifted_closure = false;
        Some(tup)
    }
}

impl LowerCtx {
    /// EXECUTE `let e = match <Option[(s1, s2)]> { some(p) => p, none => (f1, f2) }` —
    /// the let-BOUND scalar-tuple Option match (the fft `list.get(xs,k) ?? (0.0,0.0)`
    /// pick after the tuple-unwrap_or desugar). The let-bound heap-result match is
    /// normally unlowerable (per-arm move-out vs scope-end drop breaks the flat cert),
    /// but a SCALAR-TUPLE payload needs no per-arm alloc at all: merge each COMPONENT
    /// through the scalar IfThen skeleton (Some → the payload tuple's slot, None → the
    /// fallback component), then build ONE 2-slot block the binding owns — a single
    /// `i…d` object, cert-clean by construction. Returns the owned tuple ValueId, or
    /// `None` (fully rolled back) outside the exact shape.
    pub(crate) fn try_lower_scalar_tuple_option_match_bind(
        &mut self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::{IrMatchArm, IrPattern};
        // Option[<2-scalar tuple>] subject, exactly two guard-free arms.
        let tuple_ty = match &subject.ty {
            Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => match &a[0] {
                Ty::Tuple(ts)
                    if ts.len() == 2 && !is_heap_ty(&ts[0]) && !is_heap_ty(&ts[1]) =>
                {
                    a[0].clone()
                }
                _ => return None,
            },
            _ => return None,
        };
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        let find = |want_some: bool| -> Option<&IrMatchArm> {
            arms.iter().find(|a| match &a.pattern {
                IrPattern::Some { .. } => want_some,
                IrPattern::None | IrPattern::Wildcard => !want_some,
                _ => false,
            })
        };
        let some_arm = find(true)?;
        let none_arm = find(false)?;
        // some(p) => Var(p) (the payload passthrough) — the only admitted Some body.
        let p_var = match &some_arm.pattern {
            IrPattern::Some { inner } => match &**inner {
                IrPattern::Bind { var, .. } => *var,
                _ => return None,
            },
            _ => return None,
        };
        if !matches!(&some_arm.body.kind, IrExprKind::Var { id } if *id == p_var) {
            return None;
        }
        // none => (f1, f2) with scalar-lowerable components.
        let IrExprKind::Tuple { elements: fb } = &none_arm.body.kind else { return None };
        if fb.len() != 2 {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }};
        }
        // Materialize/borrow the Option subject (a self-host option call is tracked +
        // dropped at scope end by the caller's machinery; a Var is borrowed).
        let subj = match self.lower_call_args(std::slice::from_ref(subject)) {
            Ok(mut a) => match a.pop() {
                Some(CallArg::Handle(v)) => v,
                _ => bail!(),
            },
            Err(_) => bail!(),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 4 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let t = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(t), args: vec![addr] });
            t
        };
        // Component k: IfThen(tag) → payload.slot[k] (LoadHandle @12 then load64 @12/@20),
        // Else → fallback component (pure scalar).
        let mut comps: [ValueId; 2] = [ValueId(0), ValueId(0)];
        for (k, comp) in comps.iter_mut().enumerate() {
            let m = self.fresh_value();
            self.ops.push(Op::IfThen { cond: tag, dst: Some(m) });
            let ph = {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                let p = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(p), args: vec![addr] });
                p
            };
            // ph is an i32 handle local — widen through Prim::Handle before i64 address math.
            let ph64 = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph64), args: vec![ph] });
            let slot = {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 + (k as i64) * 8 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: ph64, b: off });
                let v = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
                v
            };
            self.ops.push(Op::Else { val: Some(slot) });
            let fbv = match self.lower_scalar_value(&fb[k]) {
                Some(v) => v,
                None => bail!(),
            };
            self.ops.push(Op::EndIf { val: Some(fbv) });
            *comp = m;
        }
        // ONE owned 2-slot block for the binding (the single cert object).
        let two = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: two, value: 2 });
        let tup = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: tup,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: two },
        });
        let th = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
        for (k, comp) in comps.iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (k as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: th, b: off });
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, *comp],
            });
        }
        let _ = tuple_ty;
        Some(tup)
    }
}

impl LowerCtx {
    /// C1 defunc for a `(scalar, Option[scalar])` accumulator fold — the wav
    /// find_chunk_at scanner: `fold(range, (pos, none), (state, _) => { let (p, found)
    /// = state; match found { some(_) => state, none => <if-tree over (p', none|some)> } })`.
    /// The Option component runs as TWO scalar locals (tag: 0=none/1=some, payload);
    /// every tail leaf is projected per SUB-component (the match-over-found becomes an
    /// `if tag != 0`). After the loop the Option materializes ONCE — a cap-1 block whose
    /// len field is OVERWRITTEN with the tag (len-as-tag, no branch) and which this
    /// SCOPE owns; the result tuple holds it as a BORROWED slot (view semantics — the
    /// downstream `.1` projection Dup-acquires). Fully rolled back outside the shape.
    ///
    /// The body lowers ONCE per iteration as a UNIT control tree (Block stmts and `if`
    /// conds emitted a single time); each tuple LEAF computes all three component values
    /// and SetLocals them together. (The earlier per-sub-component PROJECTION re-lowered
    /// shared preambles up to 3× — value-identical because scalar-only, but it emitted
    /// 3× the CallFn ops and permanently tripped the corpus `mir <= ir` caps gate.)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_lower_defunc_opt_tuple_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        fuse_index: Option<VarId>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::{BinOp, IrPattern, IrStmtKind};
        // (scalar, Option[scalar]) accumulator only.
        match result_ty {
            Ty::Tuple(ts)
                if ts.len() == 2
                    && !is_heap_ty(&ts[0])
                    && matches!(&ts[1],
                        Ty::Applied(TypeConstructorId::Option, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) => {}
            _ => return None,
        }
        // Seed: (e0, none).
        let IrExprKind::Tuple { elements: init_elems } = &init.kind else { return None };
        if init_elems.len() != 2 || !matches!(init_elems[1].kind, IrExprKind::OptionNone) {
            return None;
        }
        let acc_var = params[0].0;
        let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else { return None };
        if stmts.len() != 1 {
            return None;
        }
        let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value } =
            &stmts[0].kind
        else {
            return None;
        };
        if pats.len() != 2 || !matches!(&value.kind, IrExprKind::Var { id } if *id == acc_var) {
            return None;
        }
        let p_var = match &pats[0] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        let found_var = match &pats[1] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        // Synthetic vars standing for the tag/payload locals inside projected trees.
        let base = crate::lower::max_var_id(body).max(crate::lower::max_var_id(init)) + 1;
        let ft = VarId(base);
        let fv = VarId(base + 1);

        #[derive(Clone, Copy, PartialEq)]
        enum Comp {
            C0,
            C1Tag,
            C1Val,
        }
        fn subst_var(e: &IrExpr, from: VarId, to: VarId) -> IrExpr {
            let mut out = e.clone();
            fn walk(e: IrExpr, from: VarId, to: VarId) -> IrExpr {
                let mut e = e.map_children(&mut |c| walk(c, from, to));
                if let IrExprKind::Var { id } = &mut e.kind {
                    if *id == from {
                        *id = to;
                    }
                }
                e
            }
            out = walk(out, from, to);
            out
        }
        fn int_expr(kind: IrExprKind, like: &IrExpr) -> IrExpr {
            IrExpr { kind, ty: Ty::Int, span: like.span.clone(), def_id: like.def_id }
        }
        fn tag_of(e: &IrExpr, found_var: VarId, ft: VarId) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::OptionNone => Some(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                IrExprKind::OptionSome { .. } => {
                    Some(int_expr(IrExprKind::LitInt { value: 1 }, e))
                }
                IrExprKind::Var { id } if *id == found_var => {
                    Some(int_expr(IrExprKind::Var { id: ft }, e))
                }
                _ => None,
            }
        }
        fn val_of(e: &IrExpr, found_var: VarId, fv: VarId) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::OptionNone => Some(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                IrExprKind::OptionSome { expr } => Some((**expr).clone()),
                IrExprKind::Var { id } if *id == found_var => {
                    Some(int_expr(IrExprKind::Var { id: fv }, e))
                }
                _ => None,
            }
        }
        fn project(
            e: &IrExpr,
            comp: Comp,
            acc_var: VarId,
            p_var: VarId,
            found_var: VarId,
            ft: VarId,
            fv: VarId,
        ) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::Tuple { elements } if elements.len() == 2 => match comp {
                    Comp::C0 => Some(elements[0].clone()),
                    Comp::C1Tag => tag_of(&elements[1], found_var, ft),
                    Comp::C1Val => val_of(&elements[1], found_var, fv),
                },
                IrExprKind::Var { id } if *id == acc_var => Some(match comp {
                    Comp::C0 => int_expr(IrExprKind::Var { id: p_var }, e),
                    Comp::C1Tag => int_expr(IrExprKind::Var { id: ft }, e),
                    Comp::C1Val => int_expr(IrExprKind::Var { id: fv }, e),
                }),
                IrExprKind::If { cond, then, else_ } => {
                    let t = project(then, comp, acc_var, p_var, found_var, ft, fv)?;
                    let el = project(else_, comp, acc_var, p_var, found_var, ft, fv)?;
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: cond.clone(),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let t = project(tail, comp, acc_var, p_var, found_var, ft, fv)?;
                    Some(IrExpr {
                        kind: IrExprKind::Block {
                            stmts: stmts.clone(),
                            expr: Some(Box::new(t)),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                // `match found { some(b) => X, none => Y }` → `if ft != 0 then X[b:=fv] else Y`.
                IrExprKind::Match { subject, arms }
                    if matches!(&subject.kind, IrExprKind::Var { id } if *id == found_var)
                        && arms.len() == 2
                        && arms.iter().all(|a| a.guard.is_none()) =>
                {
                    let some_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Some { .. }))?;
                    let none_arm = arms
                        .iter()
                        .find(|a| matches!(a.pattern, IrPattern::None | IrPattern::Wildcard))?;
                    let some_body = match &some_arm.pattern {
                        IrPattern::Some { inner } => match &**inner {
                            IrPattern::Bind { var, .. } => subst_var(&some_arm.body, *var, fv),
                            IrPattern::Wildcard => some_arm.body.clone(),
                            _ => return None,
                        },
                        _ => return None,
                    };
                    let t = project(&some_body, comp, acc_var, p_var, found_var, ft, fv)?;
                    let el = project(&none_arm.body, comp, acc_var, p_var, found_var, ft, fv)?;
                    let cond = int_expr(
                        IrExprKind::BinOp {
                            op: BinOp::Neq,
                            left: Box::new(int_expr(IrExprKind::Var { id: ft }, e)),
                            right: Box::new(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                        },
                        e,
                    );
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                _ => None,
            }
        }
        // The tail must be a projectable component tree (the gate) — checked up front so
        // the single-pass emitter below never leaves partial control flow on a decline.
        if project(tail, Comp::C0, acc_var, p_var, found_var, ft, fv).is_none()
            || project(tail, Comp::C1Tag, acc_var, p_var, found_var, ft, fv).is_none()
            || project(tail, Comp::C1Val, acc_var, p_var, found_var, ft, fv).is_none()
        {
            return None;
        }

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let vo_snapshot = self.value_of.clone();
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.value_of = vo_snapshot;
                return None;
            }};
        }
        // Locals: s0 = seed.0; tag = 0; val = 0.
        let s0 = match self.lower_scalar_value(&init_elems[0]) {
            Some(v) => v,
            None => bail!(),
        };
        let tloc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tloc, value: 0 });
        let vloc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: vloc, value: 0 });
        self.value_of.insert(p_var, s0);
        self.value_of.insert(ft, tloc);
        self.value_of.insert(fv, vloc);

        // Source loop (same skeleton as the scalar-tuple fold).
        let list_v = match self
            .lower_call_args(std::slice::from_ref(xs))
            .ok()
            .and_then(|mut a| a.pop())
        {
            Some(CallArg::Handle(v)) => v,
            _ => bail!(),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base_addr = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base_addr, b: i8_v });
        let elem = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![addr] });
        self.value_of.insert(params[1].0, elem);
        if let Some(iv) = fuse_index {
            self.value_of.insert(iv, i_v);
        }
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let emitted =
            self.emit_opt_tuple_fold_body(tail, acc_var, found_var, ft, fv, s0, tloc, vloc);
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if emitted.is_none() {
            bail!();
        }
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::IntBinOp { dst: i_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::LoopEnd);

        // Materialize the Option ONCE: a cap-1 len-as-tag block — store the payload,
        // then OVERWRITE len(@4) with the tag (0 → none, 1 → some; cap stays 1).
        let one2 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one2, value: 1 });
        let opt = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: opt,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: one2 },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![opt] });
        let pslot = self.load_addr(oh, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![pslot, vloc] });
        let lslot = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lslot, tloc] });
        // The SCOPE owns the Option block; the tuple below only borrows it.
        self.live_heap_handles.push(opt);

        // The (scalar, Option) result tuple — slot1 is the BORROWED Option handle.
        let two = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: two, value: 2 });
        let tup = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: tup,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: two },
        });
        let th = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
        let s0slot = self.load_addr(th, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s0slot, s0] });
        let s1slot = self.load_addr(th, 20);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s1slot, oh] });
        self.materialized_aggregates.insert(tup);
        self.last_call_had_unlifted_closure = false;
        Some(tup)
    }

    /// SINGLE-PASS body emitter for the (scalar, Option[scalar]) fold: walk the tail as
    /// a UNIT control tree — Block statements and `if` conditions lower exactly ONCE —
    /// and at each tuple LEAF compute all three component values (scalar, tag, payload)
    /// before SetLocal-ing the three loop-carried locals together. A `state` leaf (the
    /// unchanged accumulator) emits nothing. The shape was pre-validated by `project`
    /// (the gate), so a `None` here only rolls back through the caller's marks.
    #[allow(clippy::too_many_arguments)]
    fn emit_opt_tuple_fold_body(
        &mut self,
        e: &IrExpr,
        acc_var: VarId,
        found_var: VarId,
        ft: VarId,
        fv: VarId,
        s0: ValueId,
        tloc: ValueId,
        vloc: ValueId,
    ) -> Option<()> {
        use almide_ir::IrPattern;
        match &e.kind {
            // The unchanged-accumulator leaf (`some(_) => state`): all three locals keep
            // their values — no ops.
            IrExprKind::Var { id } if *id == acc_var => Some(()),
            // A tuple LEAF `(e0, none | some(x) | found)` — compute all three component
            // values FIRST (they read the OLD locals), then SetLocal together.
            IrExprKind::Tuple { elements } if elements.len() == 2 => {
                let n0 = self.lower_scalar_value(&elements[0])?;
                let (nt, nv) = match &elements[1].kind {
                    IrExprKind::OptionNone => {
                        let z0 = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: z0, value: 0 });
                        let z1 = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: z1, value: 0 });
                        (z0, z1)
                    }
                    IrExprKind::OptionSome { expr } => {
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        let v = self.lower_scalar_value(expr)?;
                        (one, v)
                    }
                    IrExprKind::Var { id } if *id == found_var => (tloc, vloc),
                    _ => return None,
                };
                self.ops.push(Op::SetLocal { local: s0, src: n0 });
                self.ops.push(Op::SetLocal { local: tloc, src: nt });
                self.ops.push(Op::SetLocal { local: vloc, src: nv });
                Some(())
            }
            // A shared preamble Block: statements lower ONCE; per-iteration heap locals
            // (a `let id = bytes_to_string(…)` String) are freed within the frame.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let mark = self.live_heap_handles.len();
                self.in_frame += 1;
                let mut ok = true;
                for st in stmts {
                    if self.lower_stmt(st).is_err() {
                        ok = false;
                        break;
                    }
                }
                let r = if ok {
                    self.emit_opt_tuple_fold_body(tail, acc_var, found_var, ft, fv, s0, tloc, vloc)
                } else {
                    None
                };
                self.drop_arm_locals(mark);
                self.in_frame -= 1;
                r
            }
            // `if cond then A else B` — the cond lowers ONCE (its transient temps freed
            // in the cond frame); each arm recurses as a unit arm (no merged value).
            IrExprKind::If { cond, then, else_ } => {
                let c = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: c, dst: None });
                let t = self.emit_opt_tuple_fold_body(then, acc_var, found_var, ft, fv, s0, tloc, vloc);
                self.ops.push(Op::Else { val: None });
                let el = t.and_then(|_| {
                    self.emit_opt_tuple_fold_body(else_, acc_var, found_var, ft, fv, s0, tloc, vloc)
                });
                self.ops.push(Op::EndIf { val: None });
                el
            }
            // `match found { some(b) => X, none => Y }` — the tag local IS the cond
            // (0 = none / 1 = some); the some-arm binder rebinds to the payload var.
            IrExprKind::Match { subject, arms }
                if matches!(&subject.kind, IrExprKind::Var { id } if *id == found_var)
                    && arms.len() == 2
                    && arms.iter().all(|a| a.guard.is_none()) =>
            {
                let some_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Some { .. }))?;
                let none_arm = arms
                    .iter()
                    .find(|a| matches!(a.pattern, IrPattern::None | IrPattern::Wildcard))?;
                let some_body = match &some_arm.pattern {
                    IrPattern::Some { inner } => match &**inner {
                        IrPattern::Bind { var, .. } => {
                            crate::lower::subst_var_ir(&some_arm.body, *var, fv)
                        }
                        IrPattern::Wildcard => some_arm.body.clone(),
                        _ => return None,
                    },
                    _ => return None,
                };
                self.ops.push(Op::IfThen { cond: tloc, dst: None });
                let t = self.emit_opt_tuple_fold_body(
                    &some_body, acc_var, found_var, ft, fv, s0, tloc, vloc,
                );
                self.ops.push(Op::Else { val: None });
                let el = t.and_then(|_| {
                    self.emit_opt_tuple_fold_body(
                        &none_arm.body, acc_var, found_var, ft, fv, s0, tloc, vloc,
                    )
                });
                self.ops.push(Op::EndIf { val: None });
                el
            }
            _ => None,
        }
    }
}
