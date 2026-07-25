// ── The defunctionalised list-HOF loop body ──────────────────────
//
// `lower_defunc_list_hof_inner`, split out of `defunc_hof.rs` (which decides
// WHETHER a HOF defunctionalises) so neither half is oversized. `include!`d from
// there, so imports and the `Defunc*` view types come from that file.

impl LowerCtx {
    #[allow(clippy::too_many_arguments)]
    fn lower_defunc_list_hof_inner(
        &mut self,
        func: &str,
        xs: &IrExpr,
        lambda: DefuncLambda<'_>,
        acc: DefuncAcc<'_>,
        fuse: DefuncFusion<'_>,
    ) -> Option<ValueId> {
        let DefuncAcc { init, result_elem } = acc;
        let DefuncLambda { params, body } = lambda;
        let DefuncFusion { index: fuse_index, second: fuse_second } = fuse;
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
            self.value_of.insert(params[0].0, acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"));
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
                let ok = self.try_lower_cond_heap_acc_fold(body, params[0].0, acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"));
                self.scalar_loop_depth -= 1;
                ok
            };
        // `filter`'s body is the PREDICATE (a Bool) regardless of the result element type — the kept
        // ELEMENT (not the body) is stored. Only map/flat_map-style HOFs lower the body AS the heap
        // result element. So route filter to the scalar (Bool) path even when result_elem is Some.
        let body_v = if cond_acc_handled {
            // The slot was already updated in place; no merged body value flows out.
            Some(acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"))
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
                let rh = result_h.expect("result_h is Some here: the \"map\" arm is only reached when func == \"map\", which seeds result_h above");
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
                let rh = result_h.expect("result_h is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds result_h above");
                let cur = cursor.expect("cursor is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds cursor above");
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
                    let drop_op = self.drop_op_for(acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"));
                    self.ops.push(drop_op);
                }
                self.ops.push(Op::SetLocal { local: acc_local.expect("acc_local is Some here: func == \"fold\" is the only path that reaches this branch"), src: body_v });
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
            "fold" => Some(acc_local.expect("acc_local is Some here: this arm only runs when func == \"fold\"")),
            "map" => Some(result_list.expect("result_list is Some here: this arm only runs when func == \"map\"")),
            "filter" => {
                // Patch the result list's `len` field (offset 4) to the write-cursor: the
                // visible length is the count of kept elements (cap stays len(xs), unused
                // tail slots are harmless — a `${list}` Display reads `len`, an `xs[i]`
                // bounds-checks against `len`). `store32` at result_h + 4.
                let rh = result_h.expect("result_h is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds result_h above");
                let cur = cursor.expect("cursor is Some here: the \"filter\" arm is only reached when func == \"filter\", which seeds cursor above");
                let four = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: four, value: 4 });
                let lenaddr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: lenaddr, op: IntOp::Add, a: rh, b: four });
                self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lenaddr, cur] });
                Some(result_list.expect("result_list is Some here: this arm only runs when func == \"filter\", which shares result_list with map"))
            }
            _ => None,
        }
    }
}
