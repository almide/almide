impl LowerCtx {

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_call_module(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &value.kind else { unreachable!() };
        let dst =
            self.lower_pure_module_value_call(module.as_str(), func.as_str(), args, ty)?;
        self.value_of.insert(var, dst);
        let faithful = self.check_call_module_faithful(module.as_str(), func.as_str(), args)?;
        self.seed_call_module_heap_read_shape(dst, ty, module.as_str(), func.as_str(), faithful);
        self.seed_call_module_heap_drop_route(dst, ty);
        // A `Value` result (value.str/int/… or a Value-returning combinator) drops via the
        // runtime-tag-dispatched DropValue (a heap-payload Value owns one handle).
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(dst);
        }
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap_call_module` (second-round split, cog
    /// reduction): the HOF-faithfulness guard, verbatim. Returns whether the call is
    /// FAITHFULLY executable (every closure arg lifted, no un-representable fn-typed data
    /// arg); an unfaithful higher-order call still WALLS via `Err`, exactly as before.
    fn check_call_module_faithful(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<bool, LowerError> {
        // A SCALAR-element `List[Int/Float/Bool]` result from a self-host list call is a REAL,
        // POPULATED block — admit a direct `xs[i]` — ONLY when the call is FAITHFULLY executable:
        //  (1) every closure arg LIFTED (an unlifted `list.map(fns, (f) => f(10))` runs the
        //      combinator with a missing slot → empty/garbage), AND
        //  (2) no DATA argument carries an UN-REPRESENTABLE function type (this comment
        //      historically said "list.map(fns, …) over fns: List[(Int)->Int] — a list of
        //      closures the v1 model cannot represent" — no longer true: B36 shipped
        //      `List[<Fn>]` literal construction + a generated per-element `$__drop_
        //      list_closure`, so a `List[Fn]` DATA arg is now a REAL, populated,
        //      correctly-freed block — excluded below). A Fn buried in some OTHER shape
        //      (a record/tuple field, a nested nested-List[List[Fn]]) is still unrepresented
        //      and stays walled. The combinator's OWN closure arg (a `Lambda`/`FnRef`,
        //      function-typed by construction) is EXCLUDED too — it is handled by (1), and
        //      `(p) => p.x` over `points: List[Point]` is the faithful case that must stay
        //      tracked.
        // Otherwise the result is unmaterialized and a `xs[i]` over it would TRAP on cap 0, so
        // it is left deferring to `Const 0` (mis-valued, never a new runtime crash).
        let data_arg_has_fn = args.iter().any(|a| {
            // A let-bound lambda passed BY NAME (`let g = (x) => …; xs |> list.map(g)`) is a
            // CLOSURE arg — try_lower_defunc_list_hof resolves it via lambda_bindings and inlines
            // it faithfully (calls.rs). Without recognizing the `Var` here it is misread as a
            // fn-typed DATA arg (a `list.map(fns, …)` over a list-of-closures the v1 model can't
            // represent) and the guard below WALLS it — even though the inline succeeded. This is
            // the bind-vs-tail discrepancy: the tail/value position has no such data-arg guard, so
            // `let g = …; xs |> map(g)` lowered as a TAIL but walled as a `let r = xs |> map(g)`.
            let is_closure_arg = matches!(
                &a.kind,
                IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } | IrExprKind::ClosureCreate { .. }
            ) || matches!(&a.kind, IrExprKind::Var { id } if self.lambda_bindings.contains_key(id));
            // `List[<Fn>]` (B36) and `Map[String, <Fn>]` (the mclo family — the
            // hval handle-level twins + `$__drop_map_mclo`) are representable
            // closure CONTAINERS — excluded from the wall.
            let is_representable_closure_list = matches!(&a.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, e)
                    if e.len() == 1 && matches!(e[0], Ty::Fn { .. }))
                || crate::lower::is_map_fn_ty(&a.ty);
            !is_closure_arg && !is_representable_closure_list && crate::lower::ty_contains_fn(&a.ty)
        });
        let faithful = !self.last_call_had_unlifted_closure && !data_arg_has_fn;
        // WALL the UNFAITHFUL higher-order combinator instead of silently
        // mis-valuing it. A HOF call (`list.map`/`filter`/`fold`…) over a
        // CAPTURING/param-invoking lambda (no liftable slot) or a fn-typed DATA
        // arg (`list.map(fns, (f) => f(10))` over `fns: List[(Int)->Int]` — a
        // list of closures the v1 model cannot represent) runs the self-host
        // combinator with a missing/garbage closure and produces a zero-filled
        // result. Leaving the result deferred (a `Const 0` `xs[i]`) emits WRONG
        // BYTES — a silent miscompile. Walling the whole function here is the
        // honest outcome (render discards it cleanly; no invalid wasm, no wrong
        // output). The FAITHFUL case (every closure lifted, no fn-typed data —
        // `list.map(xs, (x) => x + 1)`, `(p) => p.x` over `List[Point]`) is
        // UNTOUCHED, so the in-scope HOF byte-matches stay materialized.
        if crate::lower::is_higher_order(args) && !faithful {
            if std::env::var("ALMIDE_DBG_ANF").is_ok() {
                eprintln!(
                    "[hof-guard] {}.{} unlifted={} data_arg_has_fn={}",
                    module,
                    func,
                    self.last_call_had_unlifted_closure,
                    data_arg_has_fn
                );
            }
            return Err(LowerError::Unsupported(format!(
                "{module}.{func} with an unliftable/closure-list higher-order argument \
                 cannot execute faithfully in this brick (walled, not mis-valued)"
            )));
        }
        Ok(faithful)
    }

    /// Extracted from `Self::lower_bind_heap_call_module` (second-round split, cog
    /// reduction): the `faithful`-gated + self-host fn-name read-shape tracking,
    /// verbatim.
    fn seed_call_module_heap_read_shape(
        &mut self,
        dst: ValueId,
        ty: &Ty,
        module: &str,
        func: &str,
        faithful: bool,
    ) {
        if is_scalar_elem_list_ty(ty) && faithful {
            self.materialized_lists.insert(dst);
        }
        // A faithful `List[heap]` result (`string.split`/`chars`/`lines` → `List[String]`,
        // or a heap-element list combinator) is ALSO a REAL, POPULATED nested-ownership block
        // whose slots hold owned element HANDLES — so a value-position `xs[i]` over the bound
        // var can LoadHandle element i at `$elem_addr` (the heap-element borrow path in
        // `try_lower_heap_field_borrow`, gated on `materialized_lists`). Without registering
        // it, `parts[i]` fell to the container-grain `Dup` of the WHOLE list → a String
        // consumer read the list HEADER bytes (the `string.split`-subscript miscompiles).
        // Narrowed to `List[heap]` (NOT the broader Option/Result/Map that
        // `is_heap_elem_list_ty` also matches) — only a real list is `[i]`-indexable here.
        if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]))
            && faithful
        {
            self.materialized_lists.insert(dst);
        }
        // A self-host returning a RECORD/TUPLE (`list.partition` → (List, List)):
        // seed the READ-shape — without it a `.0`/`.1` projection falls to the
        // container-grain Dup and a consumer reads the TUPLE header
        // (list.len(result.0) returned the tuple's len 2, not the slot list's 5 —
        // the pipe_chain partition miscompile, 2026-07-17). READ-shape ONLY: the
        // drop stays the pre-existing flat one (re-routing it through record_masks
        // here imbalanced the ownership cert — the callee's fills are opaque to
        // the caller's witness; the Named arm's mask rides a different accounting).
        if faithful && self.aggregate_field_tys(ty).is_some() {
            self.materialized_aggregates.insert(dst);
        }
        // A BORROW result (`prim.load_str` of a list slot — the list still owns it) is NOT
        // added to the scope-end drop set; everything else is a fresh owned value.
        if !self.param_values.contains(&dst) {
            self.live_heap_handles.push(dst);
        }
        // A self-host Option fn (`list.get`) returns a real materialized Option —
        // track the bound result so a later `match` over the var EXECUTES.
        if is_self_host_option_module_fn(module, func) {
            self.materialized_options.insert(dst);
        }
        // A FUNCTION-valued module-call result (`let f = map.get_or(m, k, d)` —
        // the closure-valued map read): the result IS a closure block — track it
        // so a later `f()` dispatches via CallIndirect, and its scope-end drop
        // routes to the recursive `$__drop_closure` (`closure_values` drives
        // `drop_op_for`; a captured env slot would leak under the flat rc_dec).
        if matches!(ty, Ty::Fn { .. }) {
            self.closure_values.insert(dst);
        }
        // A self-host Result fn (`int.parse`) returns a real materialized Result — track it
        // so a later `match r { Ok(v) => …, Err(e) => … }` over the var EXECUTES.
        if is_self_host_result_module_fn(module, func) {
            self.materialized_results.insert(dst);
        }
        // A self-host HEAP-Ok Result fn (`value.as_string`/`value.as_array`) — track it in the
        // cap-as-tag set so a `match` reads tag @16 + binds the @12 payload. The DROP differs
        // by Ok-arm: a `List[Value]` Ok (`value.as_array`) frees recursively
        // (`value_result_lists` → `DropResultListValue`), else a String Ok flat (`DropListStr`).
        if crate::lower::is_self_host_result_str_module_fn(module, func) {
            self.materialized_results_str.insert(dst);
            if crate::lower::is_result_listval_ty(ty) {
                self.value_result_lists.insert(dst);
            } else if crate::lower::is_value_result_ty(ty) {
                // `Result[Value, String]` (value.get) — a single dynamic Value Ok, freed
                // recursively by `Op::DropResultValue` (Ok → `$__drop_value`).
                self.value_result_results.insert(dst);
            } else {
                self.heap_elem_lists.insert(dst);
            }
        }
    }

    /// Extracted from `Self::lower_bind_heap_call_module` (second-round split, cog
    /// reduction): the mutually-exclusive drop-route selection for a Module-call's fresh
    /// heap result, verbatim (the original `if/else if` chain — NOT the same helper as
    /// `Self::seed_call_named_heap_drop_route`: this one omits the `is_scalar_elem_list_ty`
    /// tail arm, already handled above via the `faithful` gate).
    fn seed_call_module_heap_drop_route(&mut self, dst: ValueId, ty: &Ty) {
        if !self.seed_call_module_heap_drop_route_a(dst, ty) {
            self.seed_call_module_heap_drop_route_b(dst, ty);
        }
    }

    /// Extracted from `Self::seed_call_module_heap_drop_route` (third-round split, cog
    /// reduction): the first half of the mutually-exclusive `if/else if` chain, verbatim.
    /// Returns whether a branch matched (the caller then skips the second half).
    fn seed_call_module_heap_drop_route_a(&mut self, dst: ValueId, ty: &Ty) -> bool {
        // Guard-clause flattening (codopsy7 max-depth sweep): independent `if COND { ...;
        // return true; }` guards, checked in the SAME order as the original `if/else if`
        // chain, so first-match-wins semantics are preserved exactly (pure control-flow
        // rewrite, no logic change). A `List[String]` result (string.split / a List[String]
        // combinator) is a nested-ownership list — its scope-end drop must recursively free
        // elements.
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
            // `List[(Int,String)]` (list.enumerate) — recursive `$__drop_list_int_str`; the flat
            // heap_elem_lists DropListStr would leak each tuple's String (a 10⁴ loop OOMs).
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
        false
    }

    /// Extracted from `Self::seed_call_module_heap_drop_route` (third-round split, cog
    /// reduction): the second half of the mutually-exclusive `if/else if` chain, verbatim
    /// (only reached when the first half's chain did not match).
    fn seed_call_module_heap_drop_route_b(&mut self, dst: ValueId, ty: &Ty) {
        // Guard-clause flattening (codopsy7 max-depth sweep, same rationale as `_a` above).
        if crate::lower::is_map_msv_ty(ty) {
            // `Map[String, Map[String, String]]` — `$__drop_map_msv` sweeps each
            // last-ref inner map's String slots (a flat rc_dec would leak them).
            self.variant_drop_handles.insert(dst, "map_msv".to_string());
            return;
        }
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
        }
    }

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_call_computed(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &value.kind else { unreachable!() };
        // A tracked closure VAR — or a RECORD-SLOT closure (`h.run("hello")` —
        // B8's Computed(Member); `closure_block_of_mut` loads the slot borrow).
        let blk = match self.closure_block_of_mut(callee) {
            Some(b) => b,
            None => {
                return Err(LowerError::Unsupported(
                    "heap-result record-slot closure call over an unresolvable \
                     container not in this brick"
                        .into(),
                ))
            }
        };
        let repr = repr_of(ty)?;
        let lowered = self.lower_call_args(args)?;
        let dst = self.fresh_value();
        self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
        self.value_of.insert(var, dst);
        self.live_heap_handles.push(dst);
        // A closure-RETURNING closure call (`let triple = make_multiplier(3)` where
        // `make_multiplier` is a lifted lambda whose tail lifts a capturing lambda):
        // the result IS a closure block the callee moved out — track it so a later
        // `triple(4)` dispatches through it (`Op::CallIndirect`) AND its scope-end
        // drop routes to the recursive `$__drop_closure` (a heap capture would leak
        // under the default flat rc_dec).
        if matches!(ty, Ty::Fn { .. }) {
            self.closure_values.insert(dst);
        }
        // The funcref returns its Result/Option in the SAME materialized layout an `ok()`/
        // `err()` ctor builds (a lifted lambda's body goes through `materialize_result_*`), so
        // SEED its read-shape — a later `match o { ok/err }` over the bound var then reads its
        // real tag instead of walling (the higher-order-Result-callback path `fan.map` needs).
        self.seed_variant_param(dst, ty);
        // An `Option[List[String]]` closure result (the heap-acc fold's per-iteration
        // acc): the flat `heap_elem_lists` seed above would free ONE level only,
        // leaking the inner list's Strings every iteration (a fold loop OOMs) — route
        // its scope-end drop to the nested `DropListListStr` sweep instead.
        if crate::lower::is_opt_list_str_ty(ty) {
            self.heap_elem_lists.remove(&dst);
            self.list_list_str_lists.insert(dst);
        }
        // A MAP closure result (the map.fold heap-acc's per-iteration acc — the
        // `(a, k, v) => ["fresh": v]` fresh-map closure): the bare
        // `live_heap_handles` default is a FLAT rc_dec, which frees the map block
        // but LEAKS its key Strings every iteration (the 100k fold loop OOMs at a
        // 4MB cap). Route the scope-end drop to the DropListStr sweep — exact for
        // BOTH map layouts: `Map[String, String]` (interleaved, @4 = 2n, every
        // slot a String handle) and `Map[String, <scalar>]` (split, @4 = n, the
        // sweep rc_decs exactly the n key slots; the scalar value slots beyond
        // are untouched).
        if matches!(ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String)
                && (!is_heap_ty(&a[1]) || matches!(a[1], Ty::String)))
        {
            self.heap_elem_lists.insert(dst);
        }
        Ok(())
    }

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_match(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::Match { subject, arms } = &value.kind else { unreachable!() };
        // `let e = match <Option[(s1,s2)]> { some(p) => p, none => (f1,f2) }` — the
        // tuple-unwrap_or desugar output: EXECUTE via component merges + ONE owned
        // block (no per-arm alloc — cert-clean single object).
        if let Some(dst) = self.try_lower_scalar_tuple_option_match_bind(subject, arms) {
            self.value_of.insert(var, dst);
            self.live_heap_handles.push(dst);
            self.materialized_aggregates.insert(dst);
            return Ok(());
        }
        // A single-arm tuple-destructure `let offs = match pair { (o, _) => o }` extracting a
        // HEAP component — semantically `let offs = pair.<i>` (the non-tail tuple-accumulator
        // `fold` extraction). BORROW the slot handle (the tuple keeps ownership) then ACQUIRE
        // an OWNED reference (`Op::Dup`, cert `a`) the binding holds + drops at scope end — so
        // both the tuple's masked drop and this binding's drop are balanced (no double-free, no
        // leak). Mirrors the `Member`/`TupleIndex` heap-extraction bind arm.
        if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
            if is_heap_ty(&elem_ty) {
                let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                if let Some(borrow) = self.try_lower_heap_field_borrow(&synth) {
                    let dst = self.fresh_value();
                    self.ops.push(Op::Dup { dst, src: borrow });
                    self.value_of.insert(var, dst);
                    self.live_heap_handles.push(dst);
                    return Ok(());
                }
            }
        }
        // A TUPLE subject of scalar elements/expressions with a HEAP result
        // (`let s = match (string.len(x), n % 5) { (2, 0) => "…", _ => "…" }`):
        // the ordered refinement chain (heap merge), bound + scope-tracked
        // like any owned heap value.
        if let Some(dst) = self.try_lower_tuple_refinement_match(subject, arms, ty) {
            self.value_of.insert(var, dst);
            if !self.live_heap_handles.contains(&dst) {
                self.live_heap_handles.push(dst);
            }
            return Ok(());
        }
        Err(LowerError::Unsupported(
            "heap-result `match` bound to a let/var cannot be faithfully \
             computed in this brick (would bind an empty deferred heap value); \
             the merged result has no sound scope-end drop in the flat certificate"
                .into(),
        ))
    }

    /// Extracted from `Self::lower_bind_heap` (pattern-2 uniform-arm split, cog reduction):
    /// the arm body verbatim, re-narrowed via `let-else`. Pure text move.
    fn lower_bind_heap_if(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        let IrExprKind::If { else_, .. } = &value.kind else { unreachable!() };
        // A VARIANT (Option/Result)-typed `if` RHS (`let $r = if c then ok(T)
        // else err(e)` — the tail err-raise normalization's two-step bind, and
        // the hand-written equivalent): EXECUTE via the proven heap-result-if
        // machinery (each arm materializes + Consumes its value; the merge is
        // the ONE owned rc=1 block), bind + scope-track it, and SEED its
        // variant READ-shape so a following `$r!` / `match $r` takes the
        // executing tag-read path — the same classification a call-result
        // bind gets (seed_variant_param is read-shape only: no ownership
        // change, the scope-end drop stays this binding's).
        if is_variant_ty(ty) {
            if let IrExprKind::If { cond, then, else_ } = &value.kind {
                if let Some(obj) = self.try_lower_heap_result_if(cond, then, else_, ty) {
                    self.value_of.insert(var, obj);
                    if !self.live_heap_handles.contains(&obj) {
                        self.live_heap_handles.push(obj);
                    }
                    self.seed_variant_param(obj, ty);
                    return Ok(());
                }
            }
        }
        // STRAIGHT-LINE identity-else shadow rebind `let acc = if cond then acc + [x] else acc`
        // (porta `serialize_opts`' 7 stacked optional-arg appends on one `args` slot). The ELSE
        // arm is EXACTLY the accumulator var — the PROVEN loop-carried `i(id)m` append slot,
        // UNROLLED straight-line. Drop-old + `SetLocal` the slot in place (the THEN arm only);
        // the new shadow ALIASES the same slot (NOT re-pushed to live_heap_handles — one
        // scope-end drop / tail move-out covers it). Each rebind folds to a `(id)` CLoop body
        // in the certificate (check_line_unroll_sound, the same unit the loop slot proves).
        if let IrExprKind::Var { id: acc_id } = &else_.kind {
            if let Some(&acc_local) = self.value_of.get(acc_id) {
                // The slot must be an OWNED, scope-tracked heap handle (the seed's `[]`/`""`) —
                // NOT a borrowed param field (`param_values`), whose drop-old would release a
                // reference we do not own. A borrow falls through to the wall.
                if self.live_heap_handles.contains(&acc_local)
                    && !self.param_values.contains(&acc_local)
                {
                    let mark = self.ops.len();
                    if self.try_lower_line_cond_acc(value, *acc_id, acc_local) {
                        self.value_of.insert(var, acc_local);
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                }
            }
        }
        Err(LowerError::Unsupported(
            "heap-result `if` bound to a let/var cannot be faithfully \
             computed in this brick (would bind an empty deferred heap value); \
             the merged result has no sound scope-end drop in the flat certificate"
                .into(),
        ))
    }

    /// Extracted from `Self::lower_destructure` (codopsy7 max-depth sweep): seed the
    /// masked-aggregate drop tracking for a CALL-RESULT tuple's owned heap slots, verbatim
    /// (pure text move, no logic change — only pulled out of its enclosing `if` to reset the
    /// naive nesting-depth counter, which the deeply-nested `if let`/`if` chain here was
    /// tripping even though each level is a distinct, load-bearing condition). See the
    /// call site in `lower_destructure` for why this only runs for an owned, still-live,
    /// not-yet-masked tuple result.
    fn seed_call_result_tuple_mask(&mut self, subj: ValueId, elements: &[IrPattern], value: &IrExpr) {
        // The tuple's element types: from value.ty when it is a Tuple, ELSE (brick 5) — an
        // effect-fn `let (v,p) = f()!` whose `!` Unwrap render_program strips to a Call, so
        // value.ty is the effect Result, NOT a Ty::Tuple — from the PATTERN's bound types.
        // Without the pattern fallback the seed misses and the destructure container-grains
        // (reads slot 0 as the whole handle + slot 1 as Const 0 — the `8212 / 0` garbage).
        let elem_tys: Option<Vec<Ty>> = if let Ty::Tuple(tys) = &value.ty {
            Some(tys.clone())
        } else if matches!(&value.kind, IrExprKind::Unwrap { .. } | IrExprKind::Call { .. }) {
            Some(
                elements
                    .iter()
                    .map(|p| match p {
                        IrPattern::Bind { ty, .. } => ty.clone(),
                        _ => Ty::Unit,
                    })
                    .collect(),
            )
        } else {
            None
        };
        let Some(tys) = elem_tys else { return };
        // A (Value, scalar) tuple's Value slot needs the RECURSIVE __drop_value_tuple
        // (a flat record_masks rc_dec leaks the Value's nested payload → 10⁴ OOM) — the
        // same routing brick 3's construct uses.
        let value_tuple =
            tys.len() == 2 && crate::lower::is_value_ty(&tys[0]) && !is_heap_ty(&tys[1]);
        let heap_slots: Vec<usize> = (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
        if value_tuple {
            self.variant_drop_handles.insert(subj, "value_tuple".to_string());
            self.materialized_aggregates.insert(subj);
        } else if !heap_slots.is_empty() {
            self.record_masks.insert(subj, heap_slots);
            self.materialized_aggregates.insert(subj);
        }
    }

    /// `let (a, b) = …` — a TUPLE destructuring bind. Two sound shapes:
    ///
    /// 1. From a tuple LITERAL `(x, y)` of the same arity — lowered COMPONENT-WISE
    ///    as ordinary binds (`lower_bind` reused: a `Var` is an alias `Dup`, a
    ///    literal an `Alloc`/`Const`, a call a real `CallFn` whose caps are
    ///    captured, NOT elided). The tuple is never materialized.
    /// 2. From a tracked heap VAR `t` — each HEAP component aliases the WHOLE
    ///    container `t` (an `Op::Dup`, the container-grain field access of the
    ///    field-access op), each SCALAR component is a `Const` copy. Aliasing the
    ///    container keeps it alive for each component's lifetime (a conservative
    ///    lifetime widening, never a UAF); component-PRECISE identity (`a == t.0`)
    ///    is deferred to the layout brick.
    ///
    /// A `Wildcard` component is ignored. Anything else — a non-tuple/nested/
    /// constructor/record pattern, or a value that is neither a matching tuple
    /// literal nor a tracked heap var — stays an explicit `Unsupported` (totality).
    pub(crate) fn lower_destructure(&mut self, pattern: &IrPattern, value: &IrExpr) -> Result<(), LowerError> {
        // Shape 1: component-wise from a same-arity tuple LITERAL — each component is
        // bound to the ACTUAL element (a fresh value / alias, not a container alias),
        // the most precise lowering. The element's call caps are captured, not elided.
        if let (IrPattern::Tuple { elements: pats }, IrExprKind::Tuple { elements: vals }) =
            (pattern, &value.kind)
        {
            if pats.len() == vals.len() {
                for (p, v) in pats.iter().zip(vals) {
                    match p {
                        IrPattern::Bind { var, ty } => self.lower_bind(*var, ty, v)?,
                        IrPattern::Wildcard => {}
                        // A NESTED tuple sub-pattern `(b, c)` binds against the
                        // corresponding element value `v` — recurse (the same two sound
                        // shapes: a same-arity tuple literal binds component-wise, a
                        // tracked heap var aliases the container).
                        IrPattern::Tuple { .. } => self.lower_destructure(p, v)?,
                        _ => {
                            return Err(LowerError::Unsupported(
                                "destructure sub-pattern (only a bound var, `_`, or nested tuple) not in this brick"
                                    .into(),
                            ))
                        }
                    }
                }
                return Ok(());
            }
        }
        // Shape 2 (general): materialize/borrow the value as a SUBJECT (a tracked heap
        // var is borrowed, a fresh heap value is materialized + dropped at scope end),
        // then bind the pattern CONTAINER-GRAIN (each heap binding aliases the whole
        // subject — `bind_pattern`). Handles tuple-from-var, constructor, record, and
        // option/result destructuring; the bound vars drop at scope end.
        let subject: Option<ValueId> = if is_heap_ty(&value.ty) {
            match self.lower_call_args(std::slice::from_ref(value))?.into_iter().next() {
                Some(CallArg::Handle(v)) => Some(v),
                _ => None,
            }
        } else {
            self.record_elided_calls(value);
            None
        };
        // PRECISE tuple field extraction (the layout brick): a tuple value is a block
        // [rc][len][cap][f0@12, f1@20, ...]; a destructure (`let (a, b) = t`) loads each field at
        // its OWN slot instead of the container-grain alias. A SCALAR field is a value COPY; a HEAP
        // field (`let (inner, z) = n` over `((Int,Int), Int)`) is the BORROWED slot handle (the
        // tuple keeps ownership through its masked scope-end drop). Without this, `bind_pattern`
        // aliased the WHOLE container for a heap field and emitted `Const 0` for a scalar field
        // alongside it = the `8192:2000:0` miscompile.
        if let IrPattern::Tuple { elements } = pattern {
            if let Some(subj) = subject {
                // A CALL-RESULT tuple (`let (v, n) = dispatch(..)`) is a real OWNED block the
                // callee built (the `lower_tail` Tuple materialize) but `materialized_call_arg`
                // tracked it only flatly (a plain Drop would LEAK its heap slot, and it is not a
                // `materialized_aggregate` so the precise destructure below bails to the `Const 0`
                // container-alias garbage). SEED it as a masked aggregate: record the heap-slot
                // mask (so the scope-end drop is the recursive `DropListStr` that frees the owned
                // String/Value slot) + mark it `materialized_aggregates` (so per-slot borrow reads
                // execute). Only for an owned, still-live result (in `live_heap_handles`) — a
                // borrowed param/var already carries its own tracking.
                if !self.materialized_aggregates.contains(&subj)
                    && self.live_heap_handles.contains(&subj)
                {
                    self.seed_call_result_tuple_mask(subj, elements, value);
                }
                if self.try_lower_tuple_destructure(elements, subj, Some(&value.ty)) {
                    return Ok(());
                }
            }
        }
        // PRECISE record field extraction (`let { x, y } = p`) — the record sibling of the tuple
        // path above. Load each field from its OWN layout slot instead of the container-grain alias
        // (`bind_pattern` bound every field to the record pointer → `i64.add` on two ptrs / NUL
        // Strings). A CALL-RESULT record (`let { … } = mk()`) is seeded as a masked aggregate first
        // (so heap fields borrow + the scope-end drop frees them), exactly like the tuple seed.
        if let IrPattern::RecordPattern { fields, .. } = pattern {
            if let Some(subj) = subject {
                if !self.materialized_aggregates.contains(&subj)
                    && self.live_heap_handles.contains(&subj)
                {
                    if let Some((_, tys)) = self.aggregate_field_tys(&value.ty) {
                        let heap_slots: Vec<usize> =
                            (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                        if !heap_slots.is_empty() {
                            self.record_masks.insert(subj, heap_slots);
                        }
                        self.materialized_aggregates.insert(subj);
                    }
                }
                if self.try_lower_record_destructure(fields, &value.ty, subj) {
                    return Ok(());
                }
            }
        }
        self.bind_pattern(pattern, subject)
    }
}
