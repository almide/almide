// The heap `ok(...)` payload lowering of binds_p4_b.rs (max-lines split —
// pure text move; included from binds_b.rs after binds_p4_b.rs).

impl LowerCtx {
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

    /// `ok(Label { text: "a", hint: none })` / `ok(Circle(2.0))` — a VARIANT CTOR Ok
    /// payload (#1134 Shape 1, the codec Shape roundtrip): the Result twin of
    /// [`Self::try_opt_variant_ctor_payload`]. The fresh owned tag-block is MOVED into
    /// the Ok slot; a type whose payload owns heap fields routes the wrapper drop
    /// through the recursive `resrec:<type>` arm ([`Self::materialize_result_aggregate`]
    /// — a flat `DropListStr` would leak the ctor's String/heap fields), the rest keep
    /// the flat wrapper — the same split the Option side takes.
    pub(crate) fn try_lower_result_ok_variant_ctor(
        &mut self,
        value: &IrExpr,
        ty: &Ty,
    ) -> Option<ValueId> {
        let IrExprKind::ResultOk { expr } = &value.kind else {
            return None;
        };
        if !Self::is_heap_ok_result(ty) {
            return None;
        }
        let ctor_name = match &expr.kind {
            IrExprKind::Record { name: Some(n), .. } => n.as_str().to_string(),
            IrExprKind::Call { target: CallTarget::Named { name }, .. } => name.as_str().to_string(),
            _ => return None,
        };
        let type_name = self.variant_layouts.lookup_ctor_for(&ctor_name, &expr.ty)?.0.to_string();
        let repr = repr_of(ty).ok()?;
        let needs_rec = self
            .variant_layouts
            .needs_recursive_drop(&type_name, &|rn| {
                crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
            });
        let piece = self.try_lower_variant_ctor(expr)?;
        let dst = if needs_rec {
            self.materialize_result_aggregate(piece, repr, false, type_name)
        } else {
            self.materialize_result_str(piece, repr, false, false)
        };
        self.seed_variant_param(dst, ty);
        Some(dst)
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
            IrExprKind::Var { id } if self.is_unmaterialized_global_var(*id) => {
                self.piece_from_global_var(*id)?
            }
            IrExprKind::LitStr { value } => self.piece_from_lit_str(&expr.ty, value)?,
            // A variant-CTOR name here would emit a dangling `(call $Circle)` —
            // decline instead, so the ctor chain's own arm (the
            // `try_lower_*_variant_ctor` twins, which also pick the RIGHT
            // wrapper drop route for rich payloads) handles it (#1064 row:
            // `ok(Circle(2.5))` as a call argument shipped the unlinked name).
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args,
                ..
            } if !self.variant_layouts.is_ctor_for(name.as_str(), &expr.ty) => {
                self.piece_from_named_call(&expr.ty, name, args)?
            }
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
            _ => return self.result_ok_heap_computed_piece(expr),
        };
        Some(piece)
    }

    /// The COMPUTED tail of [`Self::result_ok_heap_piece`]'s router, the Ok-Result
    /// sibling of [`Self::opt_heap_general_computed_piece`]. Reached exactly when
    /// none of the leading arms selected, so the relative order is unchanged.
    fn result_ok_heap_computed_piece(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        match &expr.kind {
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
            } if matches!(&expr.ty, Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && (!is_heap_ty(&a[0]) || matches!(a[0], Ty::String))) =>
            {
                self.piece_from_computed_scalar_list(expr)
            }
            // A `Map[String, Int]` (map_skv) Ok payload (`ok(["a": 1])` → `ok(map.from_list(…))`)
            // — mirror the OptionSome map arm: the flat drop frees the map_skv block.
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(&expr.ty, Ty::Applied(TypeConstructorId::Map, a)
                    if a.len() == 2 && is_heap_ty(&a[0]) && !is_heap_ty(&a[1])) =>
            {
                self.piece_from_computed_map(expr)
            }
            // `ok(float.to_fixed(x, 4))` — a PURE Module call yielding a fresh owned STRING
            // Ok payload (fuzz C-class 323/768: the un-admitted stdlib call fell to the
            // deferred Opaque and the ZEROED block printed `ok("")` — a silent wrong value).
            IrExprKind::Call {
                target: CallTarget::Module { .. },
                ..
            } if matches!(expr.ty, Ty::String) => self.piece_from_module_string_call(expr),
            // `ok((if c then a else b))` — a heap-result IF/MATCH String Ok payload
            // (the fuzz F-858 family's Result sibling): the heap-result-if machinery
            // yields the one owned result, moved into the Ok slot.
            IrExprKind::If { .. } | IrExprKind::Match { .. } if matches!(expr.ty, Ty::String) => {
                self.piece_from_heap_result_if_match(expr)
            }
            _ => None,
        }
    }

    /// The str-element list via the str builder; a SCALAR-element list (`ok([4, 5])`,
    /// List[Int]) via the scalar-slots builder (incl the empty `ok([])`), which the
    /// str builder declines. Both yield a fresh owned block moved into the Ok slot.
    fn piece_from_ok_list_literal(
        &mut self,
        expr: &IrExpr,
        elements: &[IrExpr],
    ) -> Option<ValueId> {
        match self.try_lower_str_list_literal(expr) {
            Some(obj) => Some(obj),
            None => self.try_lower_scalar_list_slots(elements),
        }
    }
}
