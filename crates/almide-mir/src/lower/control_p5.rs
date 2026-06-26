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
            "map" | "filter" | "flat_map" | "filter_map" if args.len() == 2 => (&args[0], 1usize, None),
            "fold" if args.len() == 3 => (&args[0], 2usize, Some(1usize)),
            _ => return None,
        };
        // The CLOSURE arg MUST be an INLINE lambda (`(x) => …`). A first-class Var/FnRef
        // closure is C2 (not inlinable here) — defer to the self-host path / WALL.
        let (params, body) = match &args[lambda_idx].kind {
            IrExprKind::Lambda { params, body, .. } => (params, body.as_ref()),
            _ => return None,
        };
        // enumerate+map FUSION: `list.map(list.enumerate(real), (entry) => { let (i,key)=entry; <tail> })`
        // → a map-with-index over `real`, binding i=loop-index + key=element, AVOIDING the (Int,String)
        // intermediate list entirely (no enumerate self-host, no new tuple-list drop). Rebind the
        // source/params/body to the fused form + remember the index var (bound to i_v in the inner).
        let fuse_holder: Option<(Vec<(VarId, Ty)>, IrExpr)>;
        let mut fuse_index: Option<VarId> = None;
        let (xs, params, body) = if func == "map" {
            match detect_enum_map_fusion(xs, params, body) {
                Some((real, i_var, key_var, key_ty, tail)) => {
                    fuse_index = Some(i_var);
                    fuse_holder = Some((vec![(key_var, key_ty)], tail));
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
        // The source element read is a uniform `load64` of slot i — for a SCALAR list it is the
        // value, for a HEAP list (`List[String]`/`List[Value]`) it is the element HANDLE (a borrow
        // the inlined body reads). `map` admits a heap source; `filter`/`fold` stay scalar-source
        // (their element move-out / accumulator paths are not heap-extended here).
        let src_scalar = matches!(&xs.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]));
        // `map` admits a heap source; `fold` now does too (a heap accumulator over a List[String],
        // e.g. `lines |> list.fold("", (acc, s) => acc + s)` — the element is read as a borrowed
        // handle like map's). `flat_map`/`filter_map` admit a heap source too (the toml/dojo cases
        // map over a `List[String]` of keys/codes — the element is a borrowed String handle). `filter`
        // stays scalar-source.
        if !src_scalar && !matches!(func, "map" | "fold" | "flat_map" | "filter_map") {
            return None;
        }
        // map: a HEAP-element result list (`List[String]`/`List[Value]`) is now built too — each
        // slot holds an OWNED handle the per-element body produces (via lower_heap_result_arm), and
        // the result list is tracked for the recursive scope-end drop. filter keeps scalar results;
        // fold a scalar accumulator. (A heap accumulator / heap-filter still defers.)
        let result_heap_elem = func == "map"
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
        // `flat_map`/`filter_map` over a `List[String]` source build a `List[String]` result by
        // CONCATENATING each element's sublist (`flat_map` → `List[String]`; `filter_map` → the 0-or-1
        // element `Option[String]`, physically a `DynListStr`) onto a loop-carried accumulator via the
        // proven `__list_concat_rc` drop-old + SetLocal slot (the same `i(id)m` append-accumulator the
        // heap `fold` arm uses). Gated to a `List[String]` result; any other element type defers.
        let result_str_acc = matches!(func, "flat_map" | "filter_map")
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String));
        let result_ok = match func {
            "map" => result_heap_elem
                || matches!(result_ty,
                    Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0])),
            "filter" => matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0])),
            // A SCALAR accumulator (Int/Bool/Float), OR a heap STRING accumulator (`fold("", (acc,x)
            // => acc + …)`): the inlined `acc = <body>` is the loop-carried slot's drop-old + SetLocal
            // (the proven i(id)m append-accumulator pattern), reclaiming each transient String.
            "fold" => !is_heap_ty(result_ty) || matches!(result_ty, Ty::String),
            "flat_map" | "filter_map" => result_str_acc,
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
        let result_elem: Option<Ty> = if result_heap_elem {
            match result_ty {
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => Some(a[0].clone()),
                _ => None,
            }
        } else {
            None
        };
        let result = if result_str_acc {
            // flat_map / filter_map: a dedicated `List[String]` append-accumulator loop (concat each
            // element's sublist onto the loop-carried slot). The sublist body returns `List[String]`
            // (flat_map) or `Option[String]` (filter_map) — both are a `DynListStr` the concat appends,
            // and the per-leaf walker handles `some`/`none`/`[]`/list-concat uniformly by body shape.
            self.lower_defunc_str_acc_hof(xs, params, body)
        } else {
            self.lower_defunc_list_hof_inner(
                func,
                xs,
                params,
                body,
                init_idx.map(|i| &args[i]),
                result_elem,
                fuse_index,
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

    fn lower_defunc_list_hof_inner(
        &mut self,
        func: &str,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: Option<&IrExpr>,
        result_elem: Option<Ty>,
        fuse_index: Option<VarId>,
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
        if let Some(elem) = &result_elem {
            if !matches!(elem, Ty::String) && !result_is_str_value_tuple && !result_is_value {
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
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

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
                    if let IrExprKind::LitStr { value: s } = &init_expr.kind {
                        let acc = self.fresh_value();
                        self.ops.push(Op::Alloc {
                            dst: acc,
                            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                            init: crate::Init::Str(s.clone()),
                        });
                        (Some(acc), None, None, None)
                    } else {
                        return None;
                    }
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
        let body_v = if let Some(elem_ty) = &result_elem {
            self.lower_heap_result_arm(body, elem_ty)
        } else if fold_acc_ty.is_some() {
            // A heap (String) fold accumulator: the body `acc + s` is a ConcatStr producing a FRESH
            // owned String returned as a BARE ValueId (NOT Consumed/registered — exactly the append-
            // accumulator producer). The reassignment below drops-old + SetLocal moves this in, so it
            // is single-owned by the slot (lower_heap_result_arm would double-register it → a scope-end
            // double-free). It reads the loop-carried `acc` BEFORE the drop (borrow-then-rebind). A
            // non-ConcatStr body returns None → the HOF rolls back and the caller WALLs.
            self.scalar_loop_depth += 1;
            let v = self.try_lower_concat_str(body);
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
        // temps + Consumed body_v (moved out), so this is a no-op (live is back to body_mark).
        self.drop_arm_locals(body_mark);

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
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![raddr, elem] });
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
        self.heap_elem_lists.insert(acc);

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8. The source is a `List[String]`
        // (heap), so the element is the slot's HANDLE — a BORROWED String the inlined body reads.
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda PARAM (the element) to the BORROWED slot handle. CAPTURES resolve through
        // `value_of` (already in scope). Register `elem` as a borrow (`param_values`) — the source list
        // owns it, so a body that tries to MOVE it out (a bare `some(elem)`) auto-acquires its own ref
        // rather than a bare move-out the checker would reject.
        self.value_of.insert(params[0].0, elem);
        self.param_values.insert(elem);

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
                // recursive drop (the uniform registration below adds the scope-end free).
                self.heap_elem_lists.insert(dst);
                dst
            }
            // A `Module` call (`(... ?? []) |> list.flat_map(...)` — the NESTED flat_map leaf): route
            // through the pure-module-call path, which re-enters the defunc HOF lowering and returns a
            // fresh OWNED `List[String]`. Mark it for the recursive drop.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let dst = self
                    .lower_pure_module_value_call(module.as_str(), func.as_str(), args, &leaf.ty)
                    .ok()?;
                self.heap_elem_lists.insert(dst);
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
        self.heap_elem_lists.insert(new);
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
