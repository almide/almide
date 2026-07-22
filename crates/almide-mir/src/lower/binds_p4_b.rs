impl LowerCtx {

    fn try_lower_opt_tuple_and_variant_payloads(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::OptionSome { expr }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self.lower_owned_heap_field(expr)?;
                Some(self.materialize_opt_int_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.variant_layouts.ctor_to_type.contains_key(name.as_str())) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &expr.kind
                else {
                    return None;
                };
                let type_name = self.variant_layouts.ctor_to_type.get(name.as_str())?.clone();
                let needs_rec = self.variant_layouts.needs_recursive_drop(&type_name, &|rn| {
                    crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
                });
                let piece = self.try_lower_variant_ctor(expr)?;
                Some(if needs_rec {
                    self.materialize_opt_aggregate_some(piece, repr, type_name)
                } else {
                    self.materialize_opt_str_some(piece, repr)
                })
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Record { .. })
                    && self.aggregate_field_tys(&expr.ty).is_some() =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self
                    .try_lower_record_construct(expr)
                    .or_else(|| self.try_lower_scalar_record_construct(expr))?;
                Some(match self.record_or_anon_drop_type_name(&expr.ty) {
                    Some(rname) => self.materialize_opt_aggregate_some(piece, repr, rname),
                    None => self.materialize_opt_str_some(piece, repr),
                })
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Tuple { elements } = &expr.kind else { return None };
                let elements = elements.clone();
                let piece = self.try_lower_scalar_tuple_construct(&elements)?;
                Some(self.materialize_opt_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if tys.len() == 2
                            && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::String)) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Tuple { elements } = &expr.kind else { return None };
                let elements = elements.clone();
                let piece = self.try_lower_tuple_construct(&elements)?;
                let obj = self.materialize_opt_str_some(piece, repr);
                self.variant_drop_handles.insert(obj, "opt_str_str".to_string());
                Some(obj)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && !is_heap_ty(&tys[1])) =>
            {
                let repr = repr_of(ty).ok()?;
                let IrExprKind::Tuple { elements } = &expr.kind else { return None };
                let elements = elements.clone();
                let piece = self.try_lower_tuple_construct(&elements)?;
                let obj = self.materialize_opt_str_some(piece, repr);
                self.variant_drop_handles.insert(obj, "opt_str_int".to_string());
                Some(obj)
            }
            IrExprKind::OptionSome { expr }
                if crate::lower::is_value_ty(&expr.ty)
                    || matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String)) =>
            {
                // `Some(Value)` (list.get_value) OR `Some(List[String])` (list.get_liststr over a
                // List[List[String]]): share a NESTED-heap element by handle — Dup the borrowed element
                // into a co-owned ref (`lower_owned_heap_field`), materialize the 0-or-1 Option. The flat
                // rc_dec drop is correct (co-owned; the source list keeps its ref and frees the shared
                // block at the last ref via its own drop).
                let repr = repr_of(ty).ok()?;
                let piece = self.lower_owned_heap_field(expr)?;
                Some(self.materialize_opt_str_some(piece, repr))
            }
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                let repr = repr_of(ty).ok()?;
                let drop_fn = self.record_or_anon_drop_type_name(&expr.ty)?;
                let piece = self.try_lower_record_construct(expr)?;
                Some(self.materialize_opt_aggregate_some(piece, repr, drop_fn))
            }
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && matches!(&expr.ty, Ty::Named(..))
                    && self
                        .aggregate_field_tys(&expr.ty)
                        .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self
                    .try_lower_record_construct(expr)
                    .or_else(|| self.try_lower_scalar_record_construct(expr))?;
                Some(self.materialize_opt_str_some(piece, repr))
            }
            _ => None,
        }
    }

    fn try_lower_opt_heap_general(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        // Dup, do NOT move: the ctor gets its OWN co-owned reference and
                        // the var keeps its handle + its scope-end drop. Moving consumed
                        // the var — a SECOND `ok(r0)` then found nothing and deferred to
                        // the zeroed Opaque, printing `ok("")` (fuzz seed-20260718 index
                        // 248); native value-semantics copies each time. The same
                        // borrow-then-Dup discipline as the param arm below.
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    // `Some(Some(..))` / `Some(None)` / `Some(Ok(..))` / `Some(Err(..))` — a NESTED
                    // Option/Result ctor payload. Build the inner Option/Result block recursively
                    // (a fresh OWNED handle), then MOVE it into the outer Some's slot — exactly like
                    // an owned `Var`/Named-call payload. Without this case the nested ctor fell to
                    // `_ => None` and the whole `some(some(42))` degraded to an EMPTY Opaque list
                    // (the nested-Option construction miscompile).
                    IrExprKind::OptionSome { .. }
                    | IrExprKind::OptionNone
                    | IrExprKind::ResultOk { .. }
                    | IrExprKind::ResultErr { .. } => self.try_lower_option_ctor(expr, &expr.ty)?,
                    // `some(p.name)` — a HEAP FIELD projection payload (the optional-chain
                    // `p?.f` desugar's Some arm over a record payload): BORROW the field's
                    // slot handle from the materialized container, `Dup` into a fresh
                    // CO-OWNED ref, and move THAT in — the container keeps its own
                    // reference (freed once by its owner), the wrapper owns the Dup (the
                    // borrowed-param discipline above). Gated to a String field so the
                    // flat materialize_opt_str_some drop is exact.
                    IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let borrow = self.try_lower_heap_field_borrow(expr)?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src: borrow });
                        dup
                    }
                    // A COMPUTED String Some payload (`some("v=" + s)` / `some("v=${x}")`) — the
                    // fresh-owned `__str_concat` chain, operand temps dropped here (the ok/err
                    // ConcatStr/StringInterp arms' Option sibling).
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // A SCALAR-element LIST-literal Some payload (`some([1, 2, 3])`, `some([])`) — build
                    // the fresh owned block (0-length for the empty case, which `try_lower_scalar_list_
                    // construct` declines), moved into the Some slot; `materialize_opt_str_some`'s
                    // heap_elem_lists drop frees it flat (a scalar-element list has no nested ownership).
                    IrExprKind::List { elements }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
                    {
                        self.try_lower_scalar_list_slots(elements)?
                    }
                    // A COMPUTED scalar-element list payload (`some(list.map(xs, f))`, `some([1,2] |> …)`,
                    // `some(a + b)`) — lower the fresh owned list via `lower_owned_heap_field` (which
                    // tracks it in live_heap_handles) then MOVE it into the Some slot (retain-remove so
                    // materialize_opt_str_some is the SOLE owner). Gated to a SCALAR-element list so the
                    // flat heap_elem_lists drop is exact. Without this the computed payload fell to
                    // `_ => None` → a deferred Opaque Option reading `none` (the some(computed) miscompile).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                    | IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `some(string.slice(s, …))` — a PURE Module call yielding a fresh owned
                    // STRING payload (the parse_tag tail-if family): lower_owned_heap_field
                    // routes it via lower_pure_module_value_call; MOVE it into the Some slot
                    // (retain-remove — materialize_opt_str_some is the sole owner, its flat
                    // heap_elem_lists drop frees the one String exactly).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `some((if c then a else b))` — a heap-result IF/MATCH String payload
                    // (fuzz F-858: the un-admitted if fell to the deferred Opaque and the
                    // zeroed option READ `none` — a silent flip). EXECUTE it via the proven
                    // heap-result-if machinery (lower_owned_heap_field's If/Match arms), MOVE
                    // the one owned result into the Some slot. Gated to a String payload so
                    // the flat drop is exact.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // A `Map[String, Int]` (map_skv) Some payload (`some(["a": 1])` → `some(map.from_list
                    // (…))`) — lower the map (a Module call) and MOVE it into the Some slot. The map's own
                    // block is freed by the flat heap_elem_lists drop, exactly as a bare `let m = […]`
                    // (a map_skv block frees like a DynListStr). Gated to the map_skv (String key, scalar
                    // value) layout.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                            if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                // materialize_opt_str_some tracks materialized_options + heap_elem_lists.
                Some(self.materialize_opt_str_some(piece, repr))
            }
            _ => None,
        }
    }

    fn try_lower_opt_fallback_and_none(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::OptionSome { expr } => {
                // SCALAR payload only — `lower_scalar_value` returns `None` for a heap
                // payload, which IS the gate (a heap `Some` aliases its element, a later
                // refinement; it falls through to the deferred `Opaque` path, untracked).
                let payload = self.lower_scalar_value(expr)?;
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptSome { payload } });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            IrExprKind::OptionNone => {
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                // `None` is the 0-element Option, sized like `OptSome` (`Init::OptNone`) so the
                // free-list reuses a block between Some/None results; tracked as materialized.
                self.ops.push(Op::Alloc { dst, repr, init: Init::OptNone });
                self.materialized_options.insert(dst);
                // A HEAP-payload Option (`let x: Option[Msg] = none`) ALSO registers the
                // nested-ownership class so a downstream match ADMITS its Some-arm payload
                // bind (heap_or_scalar_bind gates on it); DropListStr over len 0 frees only
                // the block, so the class change is drop-equivalent for a None value.
                if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = ty {
                    if a.len() == 1 && is_heap_ty(&a[0]) {
                        self.heap_elem_lists.insert(dst);
                    }
                }
                Some(dst)
            }
            _ => None,
        }
    }

    fn try_lower_result_ok_heap(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        // Dup, do NOT move: the ctor gets its OWN co-owned reference and
                        // the var keeps its handle + its scope-end drop. Moving consumed
                        // the var — a SECOND `ok(r0)` then found nothing and deferred to
                        // the zeroed Opaque, printing `ok("")` (fuzz seed-20260718 index
                        // 248); native value-semantics copies each time. The same
                        // borrow-then-Dup discipline as the param arm below.
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    // `ok([])` / `ok(["a", …])` — a LIST-literal Ok payload (the
                    // tail-duplicated `let xs = if c then load(p)! else []` else-arm,
                    // porta resolve_env). The string-list literal builder yields a
                    // fresh owned block movable into the Result exactly like a call
                    // piece; an out-of-subset element list returns None (wall kept).
                    IrExprKind::List { elements } => {
                        let e = (**expr).clone();
                        // A str-element list via the str builder; a SCALAR-element list (`ok([4, 5])`,
                        // List[Int]) via the scalar-slots builder (incl the empty `ok([])`), which the
                        // str builder declines. Both yield a fresh owned block moved into the Ok slot.
                        match self.try_lower_str_list_literal(&e) {
                            Some(obj) => obj,
                            None => self.try_lower_scalar_list_slots(elements)?,
                        }
                    }
                    // `ok("n" + int.to_string(x))` — a COMPUTED String Ok payload (a `ConcatStr`
                    // chain, the `fan.map`/effect-fn `ok(label)` shape). `try_lower_concat_str` yields a
                    // fresh owned `__str_concat` result (movable into the Result exactly like a call
                    // piece); its borrowed operand temps drop here so only the concat result survives to
                    // be consumed by `materialize_result_str`.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `ok(some(5))` / `ok(none)` / `ok(ok(7))` / `ok(err("x"))` — a NESTED Option/Result
                    // ctor Ok payload. Build the inner Option/Result block recursively (a fresh OWNED
                    // handle), moved into the outer Ok's slot — exactly like the OptionSome nested arm.
                    // Without this the nested ctor fell to `_ => None` and the inner degraded to an EMPTY
                    // Opaque (the Result-outer nested-interp `ok(none)` miscompile).
                    IrExprKind::OptionSome { .. }
                    | IrExprKind::OptionNone
                    | IrExprKind::ResultOk { .. }
                    | IrExprKind::ResultErr { .. } => self.try_lower_option_ctor(expr, &expr.ty)?,
                    // A COMPUTED list Ok payload (`ok(list.map(xs, f))`, `ok(a + b)`) — lower the fresh
                    // owned list, moved into the Ok slot (retain-remove so materialize_result_str is the
                    // sole owner). Gated to a SCALAR- or STRING-element list — the two element kinds whose
                    // drop `materialize_result_str` routes exactly (flat for scalar, per-element String
                    // free for List[String], the same as the `ok(["a", …])` literal path). Mirrors the
                    // OptionSome computed arm; without it `ok(computed)` fell to a deferred Opaque `ok([])`.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                    | IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && (!is_heap_ty(&a[0]) || matches!(a[0], Ty::String))) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // A `Map[String, Int]` (map_skv) Ok payload (`ok(["a": 1])` → `ok(map.from_list(…))`)
                    // — mirror the OptionSome map arm: the flat drop frees the map_skv block.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                            if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `ok(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned STRING
                    // Ok payload (fuzz C-class 323/768: the un-admitted stdlib call fell to the
                    // deferred Opaque and the ZEROED block printed `ok("")` — a silent wrong value).
                    // `lower_owned_heap_field` routes it via `lower_pure_module_value_call` (purity/
                    // HOF gates apply there); MOVE it into the Ok slot (retain-remove — the
                    // materialized Result is the sole owner, its flat DropListStr slot-0 free is
                    // exact, the same discipline as the Named-call String piece above).
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `ok((if c then a else b))` — a heap-result IF/MATCH String Ok payload
                    // (the fuzz F-858 family's Result sibling): the heap-result-if machinery
                    // yields the one owned result, moved into the Ok slot.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_result_str(piece, repr, false, false);
                // TRACK the bound Result like every other materialized producer —
                // without this a later `match $t { ok/err }` over the LET-BOUND var
                // was UNTRACKED and rolled back (the monadic-desugar else-arm
                // `let $t = ok([]); match $t` — porta resolve_env walled on it).
                self.seed_variant_param(dst, ty);
                Some(dst)
            }
            _ => None,
        }
    }

    fn try_lower_result_small_arms(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultOk { expr } if !is_heap_ty(&expr.ty) => {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_ok(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            IrExprKind::ResultErr { expr }
                if !is_heap_ty(&expr.ty)
                    && matches!(ty,
                        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                            if a.len() == 2 && !is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
            {
                let payload = self.lower_scalar_value(expr)?;
                let repr = repr_of(ty).ok()?;
                let dst = self.materialize_result_err_scalar(payload, repr);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            IrExprKind::ResultErr { .. } if self.is_scalar_ok_variant_err_result(ty) => {
                self.try_lower_result_err_variant_ctor(value, ty)
            }
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty)
                    && !matches!(&expr.kind, IrExprKind::Var { .. })
                    && self.is_heap_ok_variant_err_result(ty) =>
            {
                self.try_lower_result_err_variant_ctor_heap_ok(value, ty)
            }
            _ => None,
        }
    }

    fn try_lower_result_err_heap_ok_result(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultErr { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        // Dup, do NOT move: the ctor gets its OWN co-owned reference and
                        // the var keeps its handle + its scope-end drop. Moving consumed
                        // the var — a SECOND `ok(r0)` then found nothing and deferred to
                        // the zeroed Opaque, printing `ok("")` (fuzz seed-20260718 index
                        // 248); native value-semantics copies each time. The same
                        // borrow-then-Dup discipline as the param arm below.
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    // `err("bad " + reason)` — a COMPUTED String Err payload (`ConcatStr`). Same
                    // fresh-owned `__str_concat` piece as an `ok(concat)`; operand temps drop here.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `err(["a", "b"])` — a `List[String]` LITERAL payload (the result.collect
                    // Err side, `Result[List[Int], List[String]]`): the inner list builds
                    // fresh-owned; the Result block's flat DropListStr would free slot-0 as a
                    // STRING (leaking the inner list's elements), so RECLASSIFY the drop below
                    // to the recursive list-of-list-str free.
                    IrExprKind::List { .. }
                        if matches!(&expr.ty,
                            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, i)
                                if i.len() == 1 && matches!(i[0], Ty::String)) =>
                    {
                        let obj = self.try_lower_str_list_literal(expr)?;
                        let dst = self.materialize_result_str(obj, repr, true, false);
                        self.heap_elem_lists.remove(&dst);
                        self.list_list_str_lists.insert(dst);
                        return Some(dst);
                    }
                    // `err(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned
                    // STRING Err payload (fuzz C-class: fell to the deferred Opaque whose zeroed
                    // block even flipped the TAG — printed `ok("")` for an err). Same piece as the
                    // ok-side Module-call arm; the cap-as-tag Err slot owns the one String.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `err((if c then a else b))` — a heap-result IF/MATCH String Err payload
                    // (the F-858 family): the one owned result moves into the Err slot.
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                Some(self.materialize_result_str(piece, repr, true, false))
            }
            _ => None,
        }
    }

    fn try_lower_result_err_heap_fallback(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultErr { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                // A FRESH owned message only — a LitStr alloc, a Named-call result, or an OWNED
                // `Var` (one in `live_heap_handles` — a freshly-built/closure-returned String, NOT
                // a BORROWED param). Consuming a borrow into the Err would move out a value the
                // caller still owns (a double-free the checker rejects), so a borrowed `Var` falls
                // through to the sound deferred `Opaque`.
                let piece = match &expr.kind {
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.live_heap_handles.contains(&v))
                            .unwrap_or(false) =>
                    {
                        // Dup, do NOT move: the ctor gets its OWN co-owned reference and
                        // the var keeps its handle + its scope-end drop. Moving consumed
                        // the var — a SECOND `ok(r0)` then found nothing and deferred to
                        // the zeroed Opaque, printing `ok("")` (fuzz seed-20260718 index
                        // 248); native value-semantics copies each time. The same
                        // borrow-then-Dup discipline as the param arm below.
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    // A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
                    // fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
                    // (cert `a`) and move THAT in — the caller keeps its own reference (freed by
                    // its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
                    // the spread-record copy already proves.
                    IrExprKind::Var { id }
                        if self
                            .value_for(*id)
                            .map(|v| self.param_values.contains(&v))
                            .unwrap_or(false) =>
                    {
                        let src = self.value_for(*id).ok()?;
                        let dup = self.fresh_value();
                        self.ops.push(Op::Dup { dst: dup, src });
                        dup
                    }
                    IrExprKind::LitStr { value } => {
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::Alloc { dst: p, repr: pr, init: Init::Str(value.clone()) });
                        p
                    }
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        let lowered = self.lower_call_args(args).ok()?;
                        let pr = repr_of(&expr.ty).ok()?;
                        let p = self.fresh_value();
                        self.ops.push(Op::CallFn {
                            dst: Some(p),
                            name: name.as_str().to_string(),
                            args: lowered,
                            result: Some(pr),
                        });
                        p
                    }
                    // A COMPUTED String Err payload (`ConcatStr`) — fresh-owned concat piece.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_concat_str(expr)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // An INTERPOLATED String payload (`err("bad ${id}")`, `ok("v=${x}")`) — the
                    // same fresh-owned `__str_concat` chain as the ConcatStr arm (the interp IS a
                    // concat fold); operand temps drop here so only the result survives the move.
                    IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                        let mark = self.live_heap_handles.len();
                        let obj = self.try_lower_string_interp(parts)?;
                        self.drop_arm_locals(mark);
                        obj
                    }
                    // `err(float.to_fixed(x, 4))` for a SCALAR-Ok Result — a PURE Module call
                    // yielding a fresh owned STRING Err payload (fuzz C-class, len-as-tag twin of
                    // the heap-Ok Module-call arms): the deferred Opaque zeroed the block. Same
                    // fresh-owned move-in as the Named-call piece above.
                    IrExprKind::Call { target: CallTarget::Module { .. }, .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    // `err((if c then a else b))` for a SCALAR-Ok Result — the heap-result
                    // IF/MATCH String Err payload (the F-858 family, len-as-tag twin).
                    IrExprKind::If { .. } | IrExprKind::Match { .. }
                        if matches!(expr.ty, Ty::String) =>
                    {
                        let p = self.lower_owned_heap_field(expr)?;
                        self.live_heap_handles.retain(|h| *h != p);
                        p
                    }
                    _ => return None,
                };
                let dst = self.materialize_opt_str_some(piece, repr);
                // materialize_opt_str_some registers the OPTION read-shape; this value is
                // a RESULT (len-as-tag, Err = len 1) — a reader that keeps both entries
                // resolves it as an Option (`is_result = results ∧ ¬options`) and takes
                // the Err payload as a Some payload (`err("x") ?? 0` returned the String
                // HANDLE — result_option_matrix's "if with ??"). Result-only tracking.
                self.materialized_options.remove(&dst);
                self.materialized_results.insert(dst);
                Some(dst)
            }
            _ => None,
        }
    }

}
