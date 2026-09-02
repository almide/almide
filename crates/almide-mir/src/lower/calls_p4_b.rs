impl LowerCtx {

    /// `() == ()` / `!=` over Unit: the type has ONE inhabitant, so equality is a
    /// compile-time constant (Eq → 1, Neq → 0) — there is no operand read to emit.
    /// Restricted to CALL-FREE operands: folding `f() == ()` would elide f's
    /// effects (record_elided_calls feeds the caps CLASSIFIER, not the render);
    /// a call-bearing operand falls through to the existing walls, loud.
    pub(crate) fn lower_scalar_binop_eq_unit(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        if !matches!(op, BinOp::Eq | BinOp::Neq) {
            return None;
        }
        if !matches!(left.ty, Ty::Unit) || !matches!(right.ty, Ty::Unit) {
            return None;
        }
        if crate::lower::expr_contains_call(left) || crate::lower::expr_contains_call(right)
        {
            return None;
        }
        let dst = self.fresh_value();
        let value = if matches!(op, BinOp::Eq) { 1 } else { 0 };
        self.ops.push(Op::ConstInt { dst, value });
        Some(dst)
    }

    /// Extracted from `Self::lower_scalar_binop_eq_family` (eighth-round split, cog
    /// reduction): the String/Value deep-equality sub-chain, verbatim.
    fn lower_scalar_binop_eq_string_value(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // STRING equality (`c == ":"` / `a != b` over String) → the self-host
        // `string.eq` byte-compare call (→ scalar Bool). Both operands are BORROWED
        // heap String handles (the call reads + copies; no ownership event). `!=` is
        // `1 - eq`. This is the dominant real-parser condition; without it the cond
        // silently lowered to 0 (false) — the yaml/char-scan miscompile.
        if matches!(op, BinOp::Eq | BinOp::Neq) && matches!(left.ty, Ty::String) {
            let args = [left.clone(), right.clone()];
            let eq = self
                .lower_pure_module_value_call("string", "eq", &args, &Ty::Bool)
                .ok()?;
            if matches!(op, BinOp::Eq) {
                return Some(eq);
            }
            let one = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: one, value: 1 });
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
            return Some(dst);
        }
        // `value.eq` deep-structural call (→ scalar Bool) for a `Value == Value` / `!=`. Without
        // this the heap `==` did not lower to a scalar cond, so an `if value==value …` fell to the
        // both-arms linearization and ran BOTH arms (silent miscompile). Both operands BORROWED
        // (value_eq only reads). `!=` is `1 - eq`. The recursive value_eq byte-matches v0's Value
        // PartialEq.
        if matches!(op, BinOp::Eq | BinOp::Neq) && crate::lower::is_value_ty(&left.ty) {
            let args = [left.clone(), right.clone()];
            let eq = self
                .lower_pure_module_value_call("value", "eq", &args, &Ty::Bool)
                .ok()?;
            if matches!(op, BinOp::Eq) {
                return Some(eq);
            }
            let one = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: one, value: 1 });
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
            return Some(dst);
        }
        None
    }

    /// Extracted from `Self::lower_scalar_binop_eq_family` (eighth-round split, cog
    /// reduction): the List/Map/Set deep-equality sub-chain, verbatim.
    fn lower_scalar_binop_eq_list_map(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        if let Some(dst) = self.lower_scalar_binop_eq_list(op, left, right) {
            return Some(dst);
        }
        self.lower_scalar_binop_eq_map_set(op, left, right)
    }

    /// Extracted from `Self::lower_scalar_binop_eq_list_map` (ninth-round split, cog
    /// reduction): the List deep-equality sub-chain, verbatim.
    /// Extracted from `Self::lower_scalar_binop_eq_list` (tenth-round split, cog
    /// reduction): the element-type → `list.eq_*` callee-name lookup, verbatim (a static
    /// value computation, no `&mut self` needed).
    fn list_eq_call_variant(es: &[Ty]) -> Option<&'static str> {
        // Guard-clause flattening (codopsy7 max-depth sweep): independent early-return checks
        // in the SAME order as the original `if/else if` chain (pure control-flow rewrite).
        if es.len() != 1 {
            return None;
        }
        if matches!(es[0], Ty::Int) {
            return Some("eq_int");
        }
        if matches!(es[0], Ty::String) {
            return Some("eq_str");
        }
        if crate::lower::is_value_ty(&es[0]) {
            return Some("eq_value");
        }
        if matches!(es[0], Ty::Float) {
            return Some("eq_float");
        }
        if matches!(es[0], Ty::Bool) {
            return Some("eq_bool");
        }
        None
    }

    fn lower_scalar_binop_eq_list(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // `list_a == list_b` over a List[Int|String|Value]: a deep element-wise compare call
        // (→ scalar Bool). Same both-arms-linearization fix as Value/String ==. element type
        // picks the variant; other element types stay unhandled (the if then walls, loud).
        if matches!(op, BinOp::Eq | BinOp::Neq) {
            if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, es) =
                &left.ty
            {
                let variant = Self::list_eq_call_variant(es);
                if let Some(v) = variant {
                    let args = [left.clone(), right.clone()];
                    let eq = self
                        .lower_pure_module_value_call("list", v, &args, &Ty::Bool)
                        .ok()?;
                    if matches!(op, BinOp::Eq) {
                        return Some(eq);
                    }
                    let one = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: one, value: 1 });
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
                    return Some(dst);
                }
            }
        }
        None
    }

    /// Extracted from `Self::lower_scalar_binop_eq_list_map` (ninth-round split, cog
    /// reduction): the Map/Set deep-equality sub-chain, verbatim.
    // Is `ty` one of the map/set reprs this deep-equality path admits, and if so which
    // self-host MODULE ("set"/"map") the eq call routes to? Named (codopsy cc) — collapses
    // the compound `admitted` boolean (4 ORs) into one predicate call.
    fn map_set_eq_module_name(ty: &Ty) -> Option<&'static str> {
        let is_set_str = matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Set, a)
                if a.len() == 1 && matches!(a[0], Ty::String));
        let is_map_skv = matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && !crate::lower::is_heap_ty(&a[1]));
        let admitted = crate::lower::is_map_ivh_ty(ty)
            || crate::lower::is_map_hval_ty(ty)
            || is_map_skv
            || is_set_str;
        if !admitted {
            return None;
        }
        Some(if is_set_str { "set" } else { "map" })
    }

    fn lower_scalar_binop_eq_map_set(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // `map_a == map_b` over the two implemented map reprs — a deep
        // order-independent compare call (→ scalar Bool), same shape as list ==.
        if !matches!(op, BinOp::Eq | BinOp::Neq) {
            return None;
        }
        let module = Self::map_set_eq_module_name(&left.ty)?;
        // Pass the BARE "eq" — `list_heap_call_name` attaches the repr suffix
        // (`map.eq_ivh` / `map.eq_hval`) from the subject type, exactly like every
        // other map call site.
        let args = [left.clone(), right.clone()];
        let eq = self.lower_pure_module_value_call(module, "eq", &args, &Ty::Bool).ok()?;
        if matches!(op, BinOp::Eq) {
            return Some(eq);
        }
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
        Some(dst)
    }

    /// The String-ordering-cmp case of [`Self::lower_scalar_binop_cmp_and_heap_eq`] below,
    /// split out (codopsy cc) — guarded on `op ∈ {Lt,Lte,Gt,Gte}`, DISJOINT from the heap-eq
    /// case's `op ∈ {Eq,Neq}` guard, so chaining the two via `.or_else()` is safe (a guard
    /// match here that then internally fails can never spuriously satisfy the other guard —
    /// the op value is fixed, and it's provably outside the other case's op set).
    fn lower_scalar_binop_string_cmp(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // String ordering `< <= > >=` → `string.cmp(a,b)` (lexicographic, -1/0/1) compared with
        // 0. WITHOUT this the comparison fell through to the i64-handle path → arbitrary order
        // (silent), or the if linearized both arms. Both operands BORROWED (cmp only reads).
        if !(matches!(op, BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte) && matches!(left.ty, Ty::String)) {
            return None;
        }
        let args = [left.clone(), right.clone()];
        let cmp = self.lower_pure_module_value_call("string", "cmp", &args, &Ty::Int).ok()?;
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let iop = match op {
            BinOp::Lt => crate::IntOp::Lt,
            BinOp::Lte => crate::IntOp::Le,
            BinOp::Gt => crate::IntOp::Gt,
            _ => crate::IntOp::Ge,
        };
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: iop, a: cmp, b: zero });
        Some(dst)
    }

    /// The heap-typed `==`/`!=` case of [`Self::lower_scalar_binop_cmp_and_heap_eq`] below,
    /// split out (codopsy cc) — see that fn's doc for the `.or_else()`-safety argument.
    fn lower_scalar_binop_heap_eq(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // Heap `==` / `!=` in a VALUE position (Option/Result/tuple/record/custom variant —
        // any layout the recursive typed-eq engine composes): the SAME materialized engine
        // the unit-if cond uses. Operands materialize (a tracked Var borrowed, a fresh
        // ctor/call an owned temp freed at frame teardown); the eq only reads. Was both-arms-
        // linearized (silent). Rolls back fully on a shape outside the engine — the caller
        // then defers/walls (loud, never wrong).
        if !(matches!(op, BinOp::Eq | BinOp::Neq) && is_heap_ty(&left.ty)) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        if let Some(eq) = self.lower_heap_eq_typed_materialized(left, right, &left.ty) {
            if matches!(op, BinOp::Eq) {
                return Some(eq);
            }
            let one = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: one, value: 1 });
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: eq });
            return Some(dst);
        }
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        None
    }

    /// Extracted from `Self::lower_scalar_binop` (seventh-round split, cog reduction):
    /// the String-ordering-cmp + heap-typed `==`/`!=` sub-chain, now a thin router over the
    /// two disjoint-guard helpers above.
    fn lower_scalar_binop_cmp_and_heap_eq(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        self.lower_scalar_binop_string_cmp(op, left, right)
            .or_else(|| self.lower_scalar_binop_heap_eq(op, left, right))
    }

    /// Extracted from `Self::lower_scalar_binop` (seventh-round split, cog reduction):
    /// the short-circuit `and`/`or` control-flow lowering + the final eager `IntBinOp`
    /// fallback (with the narrow signed-division-overflow guard), verbatim.
    fn lower_scalar_binop_shortcircuit_or_int(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // The short-circuit `and`/`or` sub-chain ALWAYS returns (`Some` on success, `None`
        // as an explicit wall) whenever its own guard is true — it never falls through to
        // the int-op chain below (see the original `return None;` at its guard's tail).
        // Re-checking the same pure guard here (no side effects, so evaluating it twice is
        // safe) lets the router pick the right helper without new shared state.
        if matches!(op, BinOp::And | BinOp::Or) && matches!(left.ty, Ty::Bool) {
            return self.lower_scalar_binop_shortcircuit(op, left, right);
        }
        self.lower_scalar_binop_int_fallback(op, left, right)
    }

    /// Extracted from `Self::lower_scalar_binop_shortcircuit_or_int` (eighth-round split,
    /// cog reduction): the short-circuit `and`/`or` control-flow lowering, verbatim (only
    /// called when the caller has already confirmed the `and`/`or`-over-Bool guard).
    fn lower_scalar_binop_shortcircuit(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // SHORT-CIRCUIT `and`/`or` — native AND the interp oracle evaluate the RHS LAZILY
        // (only when the LHS does not already decide the result). The prior EAGER `IntOp::And`/
        // `Or` (materializing BOTH operands) made a RHS with a trap/side effect (`a != 0 and
        // (10 / a) > 0`, `len > 5 and xs[5] == 0`) execute unconditionally → a divide-by-zero /
        // OOB-`elem_addr` trap native never reaches. Lower to control flow so the RHS ops are
        // emitted INSIDE the taken branch only:
        //   `a and b` → `if a then b else false`   (RHS only when a is true)
        //   `a or  b` → `if a then true else b`    (RHS only when a is false)
        // Uses the same IfThen/Else/EndIf scalar markers as `try_lower_scalar_if`; the LHS is a
        // pure Bool scalar, so no per-arm heap frame is needed. A non-lowerable operand rolls
        // back (truncate) and falls through to the deferred path — never both-arms, never wrong.
        if !(matches!(op, BinOp::And | BinOp::Or) && matches!(left.ty, Ty::Bool)) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        if let Some(dst) = self.try_lower_scalar_binop_shortcircuit_body(op, left, right) {
            return Some(dst);
        }
        // A non-lowerable operand anywhere in the body above rolls back to exactly the state
        // before this attempt — whatever ops the body already pushed (IfThen alone, or
        // IfThen+Else) are undone here, regardless of WHICH operand failed.
        self.ops.truncate(ops_mark);
        self.live_heap_handles.truncate(lhh_mark);
        None
    }

    /// The op-emitting body of [`Self::lower_scalar_binop_shortcircuit`], flattened from 3
    /// levels of `if let Some(..) = .. else { <same tail> }` nesting to `?`-early-return
    /// (codopsy cog) — same op sequence, same failure points; the caller now owns the ONE
    /// rollback (previously duplicated implicitly by falling out of each nested `if`).
    fn try_lower_scalar_binop_shortcircuit_body(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // The RHS is evaluated INSIDE the taken IfThen/Else branch, so use
        // `lower_scalar_operand` — it wraps the operand in a per-branch frame that frees any
        // transient heap temp it allocates (a `contains(y, "@")` materializes its String arg)
        // WITHIN the branch, keeping it `i…d`-balanced. The LHS (a pure Bool) is likewise framed.
        let lhs = self.lower_scalar_operand(left)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: lhs, dst: Some(dst) });
        // THEN branch: `and` evaluates RHS here; `or` yields the constant `true`.
        let then_val = if matches!(op, BinOp::And) {
            self.lower_scalar_operand(right)?
        } else {
            let t = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: t, value: 1 });
            t
        };
        self.ops.push(Op::Else { val: Some(then_val) });
        // ELSE branch: `and` yields the constant `false`; `or` evaluates RHS here.
        let else_val = if matches!(op, BinOp::And) {
            let f = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: f, value: 0 });
            f
        } else {
            self.lower_scalar_operand(right)?
        };
        self.ops.push(Op::EndIf { val: Some(else_val) });
        Some(dst)
    }

}
