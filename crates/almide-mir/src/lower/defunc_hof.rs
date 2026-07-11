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
}
