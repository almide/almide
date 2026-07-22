impl LowerCtx {
    /// Name router over `value.kind`'s guard family — each arm's condition is
    /// UNCHANGED (still evaluated by `match` exactly as before, in the same
    /// order, so arm SELECTION is byte-identical); only the arm BODY moved
    /// into a named helper (a verbatim cut, no logic change). Each guard
    /// gates a distinct, non-overlapping payload shape (a different `Ty`
    /// literal pattern), so this is the established "uniform match arm"
    /// split, not the value-position-match family that must stay whole.
    fn try_lower_opt_tuple_and_variant_payloads(
        &mut self,
        value: &IrExpr,
        ty: &Ty,
    ) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::OptionSome { expr }
                if matches!(&expr.ty,
                    Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)) =>
            {
                self.try_opt_int_str_tuple_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind,
                    IrExprKind::Call { target: CallTarget::Named { name }, .. }
                        if self.variant_layouts.ctor_to_type.contains_key(name.as_str())) =>
            {
                self.try_opt_variant_ctor_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Record { .. })
                    && self.aggregate_field_tys(&expr.ty).is_some() =>
            {
                self.try_opt_record_aggregate_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if !tys.is_empty() && tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                self.try_opt_scalar_tuple_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if tys.len() == 2
                            && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::String)) =>
            {
                self.try_opt_str_str_tuple_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(&expr.kind, IrExprKind::Tuple { .. })
                    && matches!(&expr.ty,
                        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && !is_heap_ty(&tys[1])) =>
            {
                self.try_opt_str_int_tuple_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if crate::lower::is_value_ty(&expr.ty)
                    || matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, e)
                        if e.len() == 1 && matches!(e[0], Ty::String)) =>
            {
                self.try_opt_value_or_liststr_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && self.record_or_anon_drop_type_name(&expr.ty).is_some() =>
            {
                self.try_opt_record_drop_payload(expr, ty)
            }
            IrExprKind::OptionSome { expr }
                if matches!(expr.kind, IrExprKind::Record { .. })
                    && matches!(&expr.ty, Ty::Named(..))
                    && self
                        .aggregate_field_tys(&expr.ty)
                        .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t))) =>
            {
                self.try_opt_record_scalar_fields_payload(expr, ty)
            }
            _ => None,
        }
    }

    fn try_opt_int_str_tuple_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let piece = self.lower_owned_heap_field(expr)?;
        Some(self.materialize_opt_int_str_some(piece, repr))
    }

    fn try_opt_variant_ctor_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let IrExprKind::Call {
            target: CallTarget::Named { name },
            ..
        } = &expr.kind
        else {
            return None;
        };
        let type_name = self
            .variant_layouts
            .ctor_to_type
            .get(name.as_str())?
            .clone();
        let needs_rec = self
            .variant_layouts
            .needs_recursive_drop(&type_name, &|rn| {
                crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
            });
        let piece = self.try_lower_variant_ctor(expr)?;
        Some(if needs_rec {
            self.materialize_opt_aggregate_some(piece, repr, type_name)
        } else {
            self.materialize_opt_str_some(piece, repr)
        })
    }

    fn try_opt_record_aggregate_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let piece = self
            .try_lower_record_construct(expr)
            .or_else(|| self.try_lower_scalar_record_construct(expr))?;
        Some(match self.record_or_anon_drop_type_name(&expr.ty) {
            Some(rname) => self.materialize_opt_aggregate_some(piece, repr, rname),
            None => self.materialize_opt_str_some(piece, repr),
        })
    }

    fn try_opt_scalar_tuple_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else {
            return None;
        };
        let elements = elements.clone();
        let piece = self.try_lower_scalar_tuple_construct(&elements)?;
        Some(self.materialize_opt_str_some(piece, repr))
    }

    fn try_opt_str_str_tuple_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else {
            return None;
        };
        let elements = elements.clone();
        let piece = self.try_lower_tuple_construct(&elements)?;
        let obj = self.materialize_opt_str_some(piece, repr);
        self.variant_drop_handles
            .insert(obj, "opt_str_str".to_string());
        Some(obj)
    }

    fn try_opt_str_int_tuple_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let IrExprKind::Tuple { elements } = &expr.kind else {
            return None;
        };
        let elements = elements.clone();
        let piece = self.try_lower_tuple_construct(&elements)?;
        let obj = self.materialize_opt_str_some(piece, repr);
        self.variant_drop_handles
            .insert(obj, "opt_str_int".to_string());
        Some(obj)
    }

    /// `Some(Value)` (list.get_value) OR `Some(List[String])` (list.get_liststr over a
    /// List[List[String]]): share a NESTED-heap element by handle — Dup the borrowed element
    /// into a co-owned ref (`lower_owned_heap_field`), materialize the 0-or-1 Option. The flat
    /// rc_dec drop is correct (co-owned; the source list keeps its ref and frees the shared
    /// block at the last ref via its own drop).
    fn try_opt_value_or_liststr_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let piece = self.lower_owned_heap_field(expr)?;
        Some(self.materialize_opt_str_some(piece, repr))
    }

    fn try_opt_record_drop_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let drop_fn = self.record_or_anon_drop_type_name(&expr.ty)?;
        let piece = self.try_lower_record_construct(expr)?;
        Some(self.materialize_opt_aggregate_some(piece, repr, drop_fn))
    }

    fn try_opt_record_scalar_fields_payload(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        let repr = repr_of(ty).ok()?;
        let piece = self
            .try_lower_record_construct(expr)
            .or_else(|| self.try_lower_scalar_record_construct(expr))?;
        Some(self.materialize_opt_str_some(piece, repr))
    }

    /// Outer name router — unchanged (the single guarded arm still decides
    /// whether this applies; only the "which construction strategy does this
    /// heap payload's `expr.kind` need" inner match moved to
    /// [`Self::opt_heap_general_piece`], a verbatim extraction: same arms,
    /// same order, same `_ => return None` (now returning from the helper,
    /// propagated back out here through `?` — identical short-circuit).
    fn try_lower_opt_heap_general(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::OptionSome { expr } if is_heap_ty(&expr.ty) => {
                let repr = repr_of(ty).ok()?;
                let piece = self.opt_heap_general_piece(expr)?;
                // materialize_opt_str_some tracks materialized_options + heap_elem_lists.
                Some(self.materialize_opt_str_some(piece, repr))
            }
            _ => None,
        }
    }

    /// The per-`expr.kind` heap-payload construction strategy for
    /// [`Self::try_lower_opt_heap_general`]. Each arm's body was already a
    /// self-contained construction strategy (no cross-arm state) — moved to
    /// a named helper (same "uniform match arm" split as
    /// [`Self::try_lower_opt_tuple_and_variant_payloads`] above, one level
    /// deeper). The router keeps every pattern/guard verbatim, in the same
    /// order, so arm SELECTION — including `_ => return None` — is
    /// byte-identical.
    fn opt_heap_general_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let piece = match &expr.kind {
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.live_heap_handles.contains(&v))
                    .unwrap_or(false) =>
            {
                self.piece_from_live_heap_var(*id)?
            }
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.param_values.contains(&v))
                    .unwrap_or(false) =>
            {
                self.piece_from_borrowed_param_var(*id)?
            }
            IrExprKind::LitStr { value } => self.piece_from_lit_str(&expr.ty, value)?,
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args,
                ..
            } => self.piece_from_named_call(&expr.ty, name, args)?,
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
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
                if matches!(expr.ty, Ty::String) =>
            {
                self.piece_from_heap_field_projection(expr)?
            }
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                ..
            } => self.piece_from_concat_str(expr)?,
            IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                self.piece_from_string_interp(parts)?
            }
            IrExprKind::List { elements }
                if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
            {
                self.try_lower_scalar_list_slots(elements)?
            }
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            }
            | IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatList,
                ..
            } if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) =>
            {
                self.piece_from_computed_scalar_list(expr)?
            }
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(expr.ty, Ty::String) => self.piece_from_module_string_call(expr)?,
            IrExprKind::If { .. } | IrExprKind::Match { .. } if matches!(expr.ty, Ty::String) => {
                self.piece_from_heap_result_if_match(expr)?
            }
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                            if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
            {
                self.piece_from_computed_map(expr)?
            }
            _ => return None,
        };
        Some(piece)
    }

    /// Dup, do NOT move: the ctor gets its OWN co-owned reference and the var
    /// keeps its handle + its scope-end drop. Moving consumed the var — a
    /// SECOND `ok(r0)` then found nothing and deferred to the zeroed Opaque,
    /// printing `ok("")` (fuzz seed-20260718 index 248); native value-
    /// semantics copies each time. The same borrow-then-Dup discipline as
    /// [`Self::piece_from_borrowed_param_var`] below.
    fn piece_from_live_heap_var(&mut self, id: almide_ir::VarId) -> Option<ValueId> {
        let src = self.value_for(id).ok()?;
        let dup = self.fresh_value();
        self.ops.push(Op::Dup { dst: dup, src });
        Some(dup)
    }

    /// A BORROWED param payload (`effect fn fail(msg: String) = err(msg)` — the
    /// fan-family tail ctors): Dup the param's handle into a fresh CO-OWNED ref
    /// (cert `a`) and move THAT in — the caller keeps its own reference (freed by
    /// its owner once), the wrapper owns the Dup. The borrow-then-Dup discipline
    /// the spread-record copy already proves.
    fn piece_from_borrowed_param_var(&mut self, id: almide_ir::VarId) -> Option<ValueId> {
        let src = self.value_for(id).ok()?;
        let dup = self.fresh_value();
        self.ops.push(Op::Dup { dst: dup, src });
        Some(dup)
    }

    fn piece_from_lit_str(&mut self, expr_ty: &Ty, value: &str) -> Option<ValueId> {
        let pr = repr_of(expr_ty).ok()?;
        let p = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: p,
            repr: pr,
            init: Init::Str(value.to_string()),
        });
        Some(p)
    }

    fn piece_from_named_call(
        &mut self,
        expr_ty: &Ty,
        name: &almide_lang::intern::Sym,
        args: &[IrExpr],
    ) -> Option<ValueId> {
        let lowered = self.lower_call_args(args).ok()?;
        let pr = repr_of(expr_ty).ok()?;
        let p = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(p),
            name: name.as_str().to_string(),
            args: lowered,
            result: Some(pr),
        });
        Some(p)
    }

    /// `some(p.name)` — a HEAP FIELD projection payload (the optional-chain
    /// `p?.f` desugar's Some arm over a record payload): BORROW the field's
    /// slot handle from the materialized container, `Dup` into a fresh
    /// CO-OWNED ref, and move THAT in — the container keeps its own
    /// reference (freed once by its owner), the wrapper owns the Dup (the
    /// borrowed-param discipline above). Gated to a String field so the
    /// flat materialize_opt_str_some drop is exact.
    fn piece_from_heap_field_projection(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let borrow = self.try_lower_heap_field_borrow(expr)?;
        let dup = self.fresh_value();
        self.ops.push(Op::Dup {
            dst: dup,
            src: borrow,
        });
        Some(dup)
    }

    /// A COMPUTED String Some payload (`some("v=" + s)` / `some("v=${x}")`) — the
    /// fresh-owned `__str_concat` chain, operand temps dropped here (the ok/err
    /// ConcatStr/StringInterp arms' Option sibling).
    fn piece_from_concat_str(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let mark = self.live_heap_handles.len();
        let obj = self.try_lower_concat_str(expr)?;
        self.drop_arm_locals(mark);
        Some(obj)
    }

    fn piece_from_string_interp(&mut self, parts: &[almide_ir::IrStringPart]) -> Option<ValueId> {
        let mark = self.live_heap_handles.len();
        let obj = self.try_lower_string_interp(parts)?;
        self.drop_arm_locals(mark);
        Some(obj)
    }

    /// A COMPUTED scalar-element list payload (`some(list.map(xs, f))`, `some([1,2] |> …)`,
    /// `some(a + b)`) — lower the fresh owned list via `lower_owned_heap_field` (which
    /// tracks it in live_heap_handles) then MOVE it into the Some slot (retain-remove so
    /// materialize_opt_str_some is the SOLE owner). Gated to a SCALAR-element list so the
    /// flat heap_elem_lists drop is exact. Without this the computed payload fell to
    /// `_ => None` → a deferred Opaque Option reading `none` (the some(computed) miscompile).
    fn piece_from_computed_scalar_list(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let p = self.lower_owned_heap_field(expr)?;
        self.live_heap_handles.retain(|h| *h != p);
        Some(p)
    }

    /// `some(string.slice(s, …))` — a PURE Module call yielding a fresh owned
    /// STRING payload (the parse_tag tail-if family): lower_owned_heap_field
    /// routes it via lower_pure_module_value_call; MOVE it into the Some slot
    /// (retain-remove — materialize_opt_str_some is the sole owner, its flat
    /// heap_elem_lists drop frees the one String exactly).
    fn piece_from_module_string_call(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let p = self.lower_owned_heap_field(expr)?;
        self.live_heap_handles.retain(|h| *h != p);
        Some(p)
    }

    /// `some((if c then a else b))` — a heap-result IF/MATCH String payload
    /// (fuzz F-858: the un-admitted if fell to the deferred Opaque and the
    /// zeroed option READ `none` — a silent flip). EXECUTE it via the proven
    /// heap-result-if machinery (lower_owned_heap_field's If/Match arms), MOVE
    /// the one owned result into the Some slot. Gated to a String payload so
    /// the flat drop is exact.
    fn piece_from_heap_result_if_match(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let p = self.lower_owned_heap_field(expr)?;
        self.live_heap_handles.retain(|h| *h != p);
        Some(p)
    }

    /// A `Map[String, Int]` (map_skv) Some payload (`some(["a": 1])` → `some(map.from_list
    /// (…))`) — lower the map (a Module call) and MOVE it into the Some slot. The map's own
    /// block is freed by the flat heap_elem_lists drop, exactly as a bare `let m = […]`
    /// (a map_skv block frees like a DynListStr). Gated to the map_skv (String key, scalar
    /// value) layout.
    fn piece_from_computed_map(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let p = self.lower_owned_heap_field(expr)?;
        self.live_heap_handles.retain(|h| *h != p);
        Some(p)
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
                self.ops.push(Op::Alloc {
                    dst,
                    repr,
                    init: Init::OptSome { payload },
                });
                self.materialized_options.insert(dst);
                Some(dst)
            }
            IrExprKind::OptionNone => {
                let dst = self.fresh_value();
                let repr = repr_of(ty).ok()?;
                // `None` is the 0-element Option, sized like `OptSome` (`Init::OptNone`) so the
                // free-list reuses a block between Some/None results; tracked as materialized.
                self.ops.push(Op::Alloc {
                    dst,
                    repr,
                    init: Init::OptNone,
                });
                self.materialized_options.insert(dst);
                // A HEAP-payload Option (`let x: Option[Msg] = none`) ALSO registers the
                // nested-ownership class so a downstream match ADMITS its Some-arm payload
                // bind (heap_or_scalar_bind gates on it); DropListStr over len 0 frees only
                // the block, so the class change is drop-equivalent for a None value.
                if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) =
                    ty
                {
                    if a.len() == 1 && is_heap_ty(&a[0]) {
                        self.heap_elem_lists.insert(dst);
                    }
                }
                Some(dst)
            }
            _ => None,
        }
    }

    /// Outer name router — unchanged; the inner "which construction strategy
    /// does this heap Ok payload's `expr.kind` need" match moved to
    /// [`Self::result_ok_heap_piece`], the same verbatim-extraction technique
    /// as [`Self::try_lower_opt_heap_general`] above.
    fn try_lower_result_ok_heap(&mut self, value: &IrExpr, ty: &Ty) -> Option<ValueId> {
        match &value.kind {
            IrExprKind::ResultOk { expr }
                if is_heap_ty(&expr.ty) && Self::is_heap_ok_result(ty) =>
            {
                let repr = repr_of(ty).ok()?;
                let piece = self.result_ok_heap_piece(expr)?;
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

    /// The per-`expr.kind` heap-Ok-payload construction strategy for
    /// [`Self::try_lower_result_ok_heap`] — verbatim extraction of that
    /// function's former inner `match &expr.kind { .. }`.
    /// Same per-`expr.kind` shape as [`Self::opt_heap_general_piece`] above
    /// (the Ok-Result sibling of the OptionSome case) — arm bodies that are
    /// BYTE-IDENTICAL to that function's helpers (verified by inspection:
    /// same code, same comments) call the SAME helper method rather than a
    /// duplicate; arms unique to the Result-Ok shape get their own helper.
    /// Router pattern/guard/order is otherwise untouched.
    fn result_ok_heap_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        let piece = match &expr.kind {
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.live_heap_handles.contains(&v))
                    .unwrap_or(false) =>
            {
                self.piece_from_live_heap_var(*id)?
            }
            IrExprKind::Var { id }
                if self
                    .value_for(*id)
                    .map(|v| self.param_values.contains(&v))
                    .unwrap_or(false) =>
            {
                self.piece_from_borrowed_param_var(*id)?
            }
            IrExprKind::LitStr { value } => self.piece_from_lit_str(&expr.ty, value)?,
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args,
                ..
            } => self.piece_from_named_call(&expr.ty, name, args)?,
            // `ok([])` / `ok(["a", …])` — a LIST-literal Ok payload (the
            // tail-duplicated `let xs = if c then load(p)! else []` else-arm,
            // porta resolve_env). The string-list literal builder yields a
            // fresh owned block movable into the Result exactly like a call
            // piece; an out-of-subset element list returns None (wall kept).
            IrExprKind::List { elements } => self.piece_from_ok_list_literal(expr, elements)?,
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                ..
            } => self.piece_from_concat_str(expr)?,
            IrExprKind::StringInterp { parts } if matches!(expr.ty, Ty::String) => {
                self.piece_from_string_interp(parts)?
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
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            }
            | IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatList,
                ..
            } if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 && (!is_heap_ty(&a[0]) || matches!(a[0], Ty::String))) =>
            {
                self.piece_from_computed_scalar_list(expr)?
            }
            // A `Map[String, Int]` (map_skv) Ok payload (`ok(["a": 1])` → `ok(map.from_list(…))`)
            // — mirror the OptionSome map arm: the flat drop frees the map_skv block.
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(&expr.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                    if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
            {
                self.piece_from_computed_map(expr)?
            }
            // `ok(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned STRING
            // Ok payload (fuzz C-class 323/768: the un-admitted stdlib call fell to the
            // deferred Opaque and the ZEROED block printed `ok("")` — a silent wrong value).
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(expr.ty, Ty::String) => self.piece_from_module_string_call(expr)?,
            // `ok((if c then a else b))` — a heap-result IF/MATCH String Ok payload
            // (the fuzz F-858 family's Result sibling): the heap-result-if machinery
            // yields the one owned result, moved into the Ok slot.
            IrExprKind::If { .. } | IrExprKind::Match { .. } if matches!(expr.ty, Ty::String) => {
                self.piece_from_heap_result_if_match(expr)?
            }
            _ => return None,
        };
        Some(piece)
    }

    /// The str-element list via the str builder; a SCALAR-element list (`ok([4, 5])`,
    /// List[Int]) via the scalar-slots builder (incl the empty `ok([])`), which the
    /// str builder declines. Both yield a fresh owned block moved into the Ok slot.
    fn piece_from_ok_list_literal(
        &mut self,
        expr: &IrExpr,
        elements: &[IrExpr],
    ) -> Option<ValueId> {
        let e = expr.clone();
        match self.try_lower_str_list_literal(&e) {
            Some(obj) => Some(obj),
            None => self.try_lower_scalar_list_slots(elements),
        }
    }
}

// try_lower_result_small_arms / try_lower_result_err_heap_ok_result /
// try_lower_result_err_heap_fallback and their helpers continue in
// binds_p4_b_b.rs (max-lines split — pure text move, this file's `impl
// LowerCtx` closes above and reopens there; see binds_b.rs's include! chain).
