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
        let (xs, init_idx, lambda_params, lambda_body) = self.defunc_call_shape(func, args)?;
        let (params, body) = (&lambda_params, &lambda_body);
        // `list.find` — an EARLY-EXIT scan returning `Option[elem]`, with its OWN gating
        // (the map/filter source/result gates below don't apply to it, so it is dispatched
        // FIRST — placing it after the result gate silently killed it once).
        if func == "find" {
            return self.defunc_find_route(xs, params, body, result_ty);
        }
        if let Some(init) = init_idx.map(|ix| &args[ix]) {
            if let Some(dst) = self.defunc_acc_fold_routes(xs, params, body, init, result_ty) {
                return Some(dst);
            }
        }
        let (fuse_holder, fuse_index, fuse_second, fused_src) =
            detect_defunc_fusion(func, xs, params, body);
        let fused = fuse_holder.as_ref().map(|(p, b)| {
            (fused_src.expect("a fused source accompanies every fusion holder"), p.as_slice(), b)
        });
        let (xs, params, body) = fused.unwrap_or((xs, params.as_slice(), body));
        let gate = defunc_result_gate(func, result_ty)?;
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

        // SCALAR-TUPLE accumulator fold (the argmax idiom) — its own specialized
        // loop. (`init_idx` is Some exactly for a 3-arg fold, so no func check.)
        if let Some(init_e) = init_idx.map(|ix| &args[ix]) {
            if let Some(dst) = self.defunc_scalar_tuple_routes(
                xs, DefuncLambda { params, body }, init_e, fuse_index, result_ty,
            ) {
                return Some(dst);
            }
        }
        let result = self.run_defunc_route(
            func,
            xs,
            DefuncRoute {
                lambda: DefuncLambda { params, body },
                init: init_idx.map(|i| &args[i]),
                fuse: DefuncFusion { index: fuse_index, second: fuse_second.as_ref() },
                gate,
            },
        );
        if result.is_none() {
            crate::trace::trace("ALMIDE_DBG_ELEM", || {
                format!("[defunc] {func} route declined (rolled back)")
            });
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

    /// The combinator's argument shape: `(source, init index, lambda params, lambda body)`.
    /// The closure arg index per combinator: map/filter/flat_map/filter_map = arg 1,
    /// fold = arg 2 (after init).
    ///
    /// The CLOSURE arg is an INLINE lambda (`(x) => …`) OR a `Var` statically bound to a let lambda
    /// (`let g = (x) => …; xs |> list.map(g)` — the wasm-bindgen generate_dts/esm `sigs` shape, where
    /// a flat_map body defines `param_ty` and maps with it). A let-bound lambda is resolved through the
    /// EXISTING `lambda_bindings` registry (the same one the C1 direct-call inline uses) and inlined
    /// identically — its captures resolve through `value_of` exactly like an inline lambda. A first-
    /// class/opaque/FnRef closure is C2 (not inlinable here) → defer to the self-host path / WALL.
    #[allow(clippy::type_complexity)]
    fn defunc_call_shape<'a>(
        &self,
        func: &str,
        args: &'a [IrExpr],
    ) -> Option<(&'a IrExpr, Option<usize>, Vec<(VarId, Ty)>, IrExpr)> {
        let (xs, lambda_idx, init_idx) = match func {
            "map" | "filter" | "flat_map" | "filter_map" | "find" if args.len() == 2 => {
                (&args[0], 1usize, None)
            }
            "fold" if args.len() == 3 => (&args[0], 2usize, Some(1usize)),
            _ => return None,
        };
        let (params, body) = match &args[lambda_idx].kind {
            IrExprKind::Lambda { params, body, .. } => (params.clone(), (**body).clone()),
            IrExprKind::Var { id } => self.lambda_bindings.get(id).cloned()?,
            _ => return None,
        };
        Some((xs, init_idx, params, body))
    }

    /// Dispatch the admitted HOF to its loop lowerer. The caller holds the
    /// rollback marks — a `None` here means the route declined and the caller
    /// rolls the whole HOF back to the wall.
    fn run_defunc_route(
        &mut self,
        func: &str,
        xs: &IrExpr,
        route: DefuncRoute<'_>,
    ) -> Option<ValueId> {
        let DefuncRoute { lambda, init, fuse, gate } = route;
        let DefuncLambda { params, body } = lambda;
        if gate.str_acc {
            // flat_map / filter_map: a dedicated `List[String]` append-accumulator loop (concat each
            // element's sublist onto the loop-carried slot). The sublist body returns `List[String]`
            // (flat_map) or `Option[String]` (filter_map) — both are a `DynListStr` the concat appends,
            // and the per-leaf walker handles `some`/`none`/`[]`/list-concat uniformly by body shape.
            self.lower_defunc_str_acc_hof(xs, params, body)
        } else if gate.filter_map_heap {
            // filter_map → `List[record]`/`List[Value]`/`List[(String,Value)]`: a write-cursor result
            // list keeping the Ok/Some-arm-built OWNED element, skipping Err/None (the dojo shape).
            match gate.result_elem.as_ref() {
                Some(elem) => self.lower_defunc_filter_map_hof(xs, params, body, elem),
                None => None,
            }
        } else {
            self.lower_defunc_list_hof_inner(
                func,
                xs,
                DefuncLambda { params, body },
                DefuncAcc { init, result_elem: gate.result_elem },
                fuse,
            )
        }
    }

    /// `list.find` — an EARLY-EXIT scan returning `Option[elem]`. Fully rolled
    /// back on decline (the caller keeps the self-host / WALL path).
    fn defunc_find_route(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let f_ops = self.ops.len();
        let f_lhh = self.live_heap_handles.len();
        let f_lifted = self.lifted.len();
        let f_vo = self.value_of.clone();
        if let Some(dst) = self.try_lower_defunc_find(xs, params, body, result_ty) {
            self.last_call_had_unlifted_closure = false;
            return Some(dst);
        }
        self.rollback_scalar_loop(f_ops, f_lhh, f_lifted, f_vo);
        None
    }

    /// A TUPLE-accumulator `fold((<empty-list>, <int-init>), (state, e) => { let (acc, n) = state;
    /// (acc + [<elem>], n + <step>) })` returning `(List[T], Int)` — the wasm-bindgen
    /// `wasm_record_offsets` shape. The accumulator is a 2-tuple `(List[T], Int)`; the body
    /// destructures `state` then returns a tuple whose component0 is a `acc + [<elem>]` list APPEND
    /// and component1 a scalar `n + <step>`. The scalar result gate rejects this (a
    /// heap-and-not-String accumulator), so it is handled HERE with a dedicated loop that carries TWO
    /// slots (a List append-accumulator + an Int scalar local) and builds the result tuple ONCE
    /// after the loop. Each helper does its OWN strict gating + complete rollback (any deviation →
    /// None → rolls back → walls, never a wrong-bytes tuple). The RECORD-accumulator sibling
    /// (`{ out: List[String], in_ul: Bool }` — the playground `wrap_lists` (B)-mechanism shape)
    /// is tried second, with the same discipline.
    fn defunc_acc_fold_routes(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let tup_mark = self.ops.len();
        let tup_lhh = self.live_heap_handles.len();
        let tup_lifted = self.lifted.len();
        let tup_vo = self.value_of.clone();
        if let Some(dst) = self.try_lower_defunc_tuple_acc_fold(xs, params, body, init, result_ty) {
            // The closure was FAITHFULLY inlined — clear the unlifted-closure flag (see the tail
            // of `try_lower_defunc_list_hof`) so the bind path treats the tuple block as a
            // genuinely-materialized aggregate, NOT an unfaithful HOF to WALL.
            self.last_call_had_unlifted_closure = false;
            return Some(dst);
        }
        self.rollback_scalar_loop(tup_mark, tup_lhh, tup_lifted, tup_vo);
        let rec_mark = self.ops.len();
        let rec_lhh = self.live_heap_handles.len();
        let rec_lifted = self.lifted.len();
        let rec_vo = self.value_of.clone();
        if let Some(dst) = self.try_lower_defunc_record_acc_fold(xs, params, body, init, result_ty) {
            self.last_call_had_unlifted_closure = false;
            return Some(dst);
        }
        self.rollback_scalar_loop(rec_mark, rec_lhh, rec_lifted, rec_vo);
        None
    }

    /// SCALAR-TUPLE accumulator folds: the `(scalar, scalar)` argmax idiom, then
    /// the `(scalar, Option[scalar])` find_chunk scanner — each with its own
    /// specialized loop and its own gating/rollback.
    fn defunc_scalar_tuple_routes(
        &mut self,
        xs: &IrExpr,
        lambda: DefuncLambda<'_>,
        init_e: &IrExpr,
        fuse_index: Option<VarId>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let DefuncLambda { params, body } = lambda;
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
        self.try_lower_defunc_opt_tuple_fold(
            xs, DefuncLambda { params, body }, init_e, fuse_index, result_ty,
        )
    }
}

/// The admitted RESULT shapes of one defunctionalised HOF, all decided from
/// `(func, result_ty)` alone — `None` = not admitted (the whole HOF defers).
/// (A map/filter HEAP-element result — each slot an OWNED handle the
/// per-element body produces, the list tracked for the recursive scope-end
/// drop — is admission-only: it shows up here as `result_elem: Some`.)
struct DefuncGate {
    /// `flat_map`/`filter_map` building a `List[String]` result by CONCATENATING each element's
    /// sublist (`flat_map` → `List[String]`; `filter_map` → the 0-or-1 element `Option[String]`,
    /// physically a `DynListStr`) onto a loop-carried accumulator via the proven
    /// `__list_concat_rc` drop-old + SetLocal slot (the same `i(id)m` append-accumulator the
    /// heap `fold` arm uses) — plus `flat_map` → `List[Matrix]` (`heads |> list.flat_map((h) =>
    /// list.repeat(h, n_rep))` — the nn repeat_kv GQA shape), whose acc/leaf drop grain is
    /// derived from the list TYPE inside (`is_list_list_str_ty` → the nested DropListListStr sweep).
    str_acc: bool,
    /// A `filter_map` building a HEAP-but-non-String element list (`List[record]`/`List[Value]`/
    /// `List[(String,Value)]` — the dojo `backfill_dir` shape): a write-cursor result list (like
    /// `filter`) keeping the Ok/Some-arm-built OWNED element and skipping the Err/None arm —
    /// `lower_defunc_filter_map_hof`. (String-element filter_map stays the `str_acc` path.)
    filter_map_heap: bool,
    /// The result element type for a heap-element map (the per-element body's owned result is
    /// moved into a slot; the result list is recursively dropped). None ⇒ the scalar path.
    result_elem: Option<Ty>,
}

/// One admitted route + its inputs, bundled so the dispatcher's signature
/// stays small: the (possibly fused) lambda, the fold init, the fusion state
/// and the result-shape gate travel together.
struct DefuncRoute<'a> {
    lambda: DefuncLambda<'a>,
    init: Option<&'a IrExpr>,
    fuse: DefuncFusion<'a>,
    gate: DefuncGate,
}

/// The result type's single List element, when it is `List[T]`.
fn list_elem_ty(result_ty: &Ty) -> Option<&Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    match result_ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(&a[0]),
        _ => None,
    }
}

fn defunc_heap_elem_result(func: &str, result_ty: &Ty) -> bool {
    matches!(func, "map" | "filter") && list_elem_ty(result_ty).is_some_and(is_heap_ty)
}

fn defunc_str_acc_result(func: &str, result_ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(func, "flat_map" | "filter_map")
        && matches!(list_elem_ty(result_ty), Some(Ty::String))
        || (func == "flat_map"
            && matches!(list_elem_ty(result_ty),
                Some(Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _))))
}

fn defunc_filter_map_heap_result(func: &str, result_ty: &Ty) -> bool {
    func == "filter_map"
        && list_elem_ty(result_ty)
            .is_some_and(|e| is_heap_ty(e) && !matches!(e, Ty::String))
}

/// Decide the result-shape admission for one combinator. `fold` is always
/// admitted here — a SCALAR accumulator (Int/Bool/Float), OR any HEAP
/// accumulator the seed/body machinery can handle (String, a list, a Matrix —
/// `fold(layers, x, (h, l) => block(h, l))`): the inlined `acc = <body>` is
/// the loop-carried slot's drop-old + SetLocal (the proven i(id)m
/// append-accumulator pattern). The strict per-shape gating lives in the SEED
/// (LitStr/Var/list-literal only) and BODY (concat/fresh-owned-call only)
/// lowerers — an unsupported shape returns None there and the whole HOF rolls
/// back to the wall.
fn defunc_result_gate(func: &str, result_ty: &Ty) -> Option<DefuncGate> {
    let heap_elem = defunc_heap_elem_result(func, result_ty);
    let str_acc = defunc_str_acc_result(func, result_ty);
    let filter_map_heap = defunc_filter_map_heap_result(func, result_ty);
    let scalar_list_result = list_elem_ty(result_ty).is_some_and(|e| !is_heap_ty(e));
    let result_ok = match func {
        "map" | "filter" => heap_elem || scalar_list_result,
        "fold" => true,
        "flat_map" => str_acc,
        "filter_map" => str_acc || filter_map_heap,
        _ => false,
    };
    if !result_ok {
        return None;
    }
    let result_elem: Option<Ty> = if heap_elem || filter_map_heap {
        list_elem_ty(result_ty).cloned()
    } else {
        None
    };
    Some(DefuncGate { str_acc, filter_map_heap, result_elem })
}

/// Loop-fusion detection, dispatched by combinator:
///
/// enumerate+map (`list.map(list.enumerate(real), (entry) => { let (i,key)=entry; <tail> })`
/// → a map-with-index over `real`, binding i=loop-index + key=element, AVOIDING the (Int,String)
/// intermediate list entirely — no enumerate self-host, no new tuple-list drop); zip+map (the
/// loop iterates `a` as the primary source, borrows `b` alongside, binds p1 = b[i] each
/// iteration, and bounds the loop by min(len_a, len_b) — v0 zip semantics; the (A,B) tuple
/// list is never built); enumerate+FOLD (`args |> list.enumerate |> list.fold(init, (acc,
/// entry) => { let (i, key) = entry; … })`: iterate `real` directly, binding i=loop-index +
/// key=element + KEEPING the acc param — the `find_flag` shape).
///
/// Returns `(holder, fuse_index, fuse_second, fused_source)`: `holder` OWNS the fused
/// params+body (the caller rebinds its slices to it), `fused_source` is the unwrapped real
/// source, and both are `Some`/`None` together.
#[allow(clippy::type_complexity)]
fn detect_defunc_fusion<'a>(
    func: &str,
    xs: &'a IrExpr,
    params: &[(VarId, Ty)],
    body: &IrExpr,
) -> (
    Option<(Vec<(VarId, Ty)>, IrExpr)>,
    Option<VarId>,
    Option<(IrExpr, VarId, Ty)>,
    Option<&'a IrExpr>,
) {
    if func == "map" {
        if let Some((real, i_var, key_var, key_ty, tail)) = detect_enum_map_fusion(xs, params, body)
        {
            return (Some((vec![(key_var, key_ty)], tail)), Some(i_var), None, Some(real));
        }
        if let Some((a, b, p0, t0, p1, t1, new_body)) = detect_zip_map_fusion(xs, params, body) {
            return (
                Some((vec![(p0, t0)], new_body)),
                None,
                Some((b.clone(), p1, t1)),
                Some(a),
            );
        }
    } else if func == "fold" {
        if let Some((real, i_var, acc_param, key_var, key_ty, tail)) =
            detect_enum_fold_fusion(xs, params, body)
        {
            return (
                Some((vec![acc_param, (key_var, key_ty)], tail)),
                Some(i_var),
                None,
                Some(real),
            );
        }
    }
    (None, None, None, None)
}

include!("defunc_hof_inner.rs");
