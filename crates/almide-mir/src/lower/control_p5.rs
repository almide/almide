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
        // The closure arg index per combinator: map/filter = arg 1, fold = arg 2 (after init).
        let (xs, lambda_idx, init_idx) = match func {
            "map" | "filter" if args.len() == 2 => (&args[0], 1usize, None),
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
        // handle like map's). `filter` stays scalar-source.
        if !src_scalar && !matches!(func, "map" | "fold") {
            return None;
        }
        // map: a HEAP-element result list (`List[String]`/`List[Value]`) is now built too — each
        // slot holds an OWNED handle the per-element body produces (via lower_heap_result_arm), and
        // the result list is tracked for the recursive scope-end drop. filter keeps scalar results;
        // fold a scalar accumulator. (A heap accumulator / heap-filter still defers.)
        let result_heap_elem = func == "map"
            && matches!(result_ty,
                Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
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
        let result = self.lower_defunc_list_hof_inner(
            func,
            xs,
            params,
            body,
            init_idx.map(|i| &args[i]),
            result_elem,
            fuse_index,
        );
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
