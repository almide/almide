/// The accumulator of a defunctionalised list HOF.
///
/// `init` is the fold seed (absent for map/filter); `result_elem` is the
/// element type of the produced list (absent for a fold, which produces a
/// scalar). Exactly one of the two shapes applies per HOF, so carrying them
/// together keeps that pairing in one place.
pub(crate) struct DefuncAcc<'a> {
    pub init: Option<&'a IrExpr>,
    pub result_elem: Option<Ty>,
}

/// The closure a defunctionalised list HOF applies.
///
/// Its parameters and body always travel together — a body lowered against a
/// different parameter list binds the wrong variables — so they travel as one
/// value.
#[derive(Copy, Clone)]
pub(crate) struct DefuncLambda<'a> {
    pub params: &'a [(VarId, Ty)],
    pub body: &'a IrExpr,
}

/// The loop-fusion state for a defunctionalised list HOF.
///
/// `index` is the induction variable a fused `enumerate` binds; `second` is the
/// second source of a fused zip. Both are absent for an unfused HOF.
#[derive(Copy, Clone)]
pub(crate) struct DefuncFusion<'a> {
    pub index: Option<VarId>,
    pub second: Option<&'a (IrExpr, VarId, Ty)>,
}

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
                &args[init_idx.expect("init_idx is Some when func == \"fold\" && args.len() == 3, checked at this match's guard")],
                result_ty,
            ) {
                // The closure was FAITHFULLY inlined — clear the unlifted-closure flag (see the tail
                // of this function) so the bind path treats the tuple block as a genuinely-materialized
                // aggregate, NOT an unfaithful HOF to WALL.
                self.last_call_had_unlifted_closure = false;
                return Some(dst);
            }
            self.rollback_scalar_loop(tup_mark, tup_lhh, tup_lifted, tup_vo);
            // The RECORD-accumulator sibling (`{ out: List[String], in_ul: Bool }` — the
            // playground `wrap_lists` (B)-mechanism shape). Same strict-gate + full-rollback
            // discipline; see `try_lower_defunc_record_acc_fold`.
            let rec_mark = self.ops.len();
            let rec_lhh = self.live_heap_handles.len();
            let rec_lifted = self.lifted.len();
            let rec_vo = self.value_of.clone();
            if let Some(dst) = self.try_lower_defunc_record_acc_fold(
                xs,
                params,
                body,
                &args[init_idx.expect("init_idx is Some when func == \"fold\" && args.len() == 3, checked at this match's guard")],
                result_ty,
            ) {
                self.last_call_had_unlifted_closure = false;
                return Some(dst);
            }
            self.rollback_scalar_loop(rec_mark, rec_lhh, rec_lifted, rec_vo);
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
                    let (p, b) = fuse_holder.as_ref().expect("fuse_holder was just set to Some on the previous line");
                    (real, p.as_slice(), b)
                }
                None => match detect_zip_map_fusion(xs, params, body) {
                    Some((a, b, p0, t0, p1, t1, new_body)) => {
                        fuse_second = Some((b.clone(), p1, t1));
                        fuse_holder = Some((vec![(p0, t0)], new_body));
                        let (p, bd) = fuse_holder.as_ref().expect("fuse_holder was just set to Some on the previous line");
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
                    let (p, b) = fuse_holder.as_ref().expect("fuse_holder was just set to Some on the previous line");
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
                        xs, DefuncLambda { params, body }, init_e, fuse_index, result_ty,
                    ) {
                        return Some(dst);
                    }
                }
                // (scalar, Option[scalar]) accumulator — the find_chunk scanner.
                if let Some(dst) = self.try_lower_defunc_opt_tuple_fold(
                    xs, DefuncLambda { params, body }, init_e, fuse_index, result_ty,
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
                DefuncLambda { params, body },
                DefuncAcc { init: init_idx.map(|i| &args[i]), result_elem },
                DefuncFusion { index: fuse_index, second: fuse_second.as_ref() },
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
}

include!("defunc_hof_inner.rs");
