impl LowerCtx {

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
    fn lower_scalar_binop_eq_map_set(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // `map_a == map_b` over the two implemented map reprs — a deep
        // order-independent compare call (→ scalar Bool), same shape as list ==.
        if matches!(op, BinOp::Eq | BinOp::Neq) {
            let is_set_str = matches!(&left.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Set, a)
                    if a.len() == 1 && matches!(a[0], Ty::String));
            let is_map_skv = matches!(&left.ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                    if a.len() == 2 && matches!(a[0], Ty::String) && !crate::lower::is_heap_ty(&a[1]));
            let admitted = crate::lower::is_map_ivh_ty(&left.ty)
                || crate::lower::is_map_hval_ty(&left.ty)
                || is_map_skv
                || is_set_str;
            if admitted {
                let module = if is_set_str { "set" } else { "map" };
                // Pass the BARE "eq" — `list_heap_call_name` attaches the repr
                // suffix (`map.eq_ivh` / `map.eq_hval`) from the subject type,
                // exactly like every other map call site.
                let args = [left.clone(), right.clone()];
                let eq = self
                    .lower_pure_module_value_call(module, "eq", &args, &Ty::Bool)
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
        None
    }

    /// Extracted from `Self::lower_scalar_binop` (seventh-round split, cog reduction):
    /// the String-ordering-cmp + heap-typed `==`/`!=` sub-chain, verbatim.
    fn lower_scalar_binop_cmp_and_heap_eq(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // String ordering `< <= > >=` → `string.cmp(a,b)` (lexicographic, -1/0/1) compared with
        // 0. WITHOUT this the comparison fell through to the i64-handle path → arbitrary order
        // (silent), or the if linearized both arms. Both operands BORROWED (cmp only reads).
        if matches!(op, BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte)
            && matches!(left.ty, Ty::String)
        {
            let args = [left.clone(), right.clone()];
            let cmp = self
                .lower_pure_module_value_call("string", "cmp", &args, &Ty::Int)
                .ok()?;
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
            return Some(dst);
        }
        // Heap `==` / `!=` in a VALUE position (Option/Result/tuple/record/custom variant —
        // any layout the recursive typed-eq engine composes): the SAME materialized engine
        // the unit-if cond uses. Operands materialize (a tracked Var borrowed, a fresh
        // ctor/call an owned temp freed at frame teardown); the eq only reads. Was both-arms-
        // linearized (silent). Rolls back fully on a shape outside the engine — the caller
        // then defers/walls (loud, never wrong).
        if matches!(op, BinOp::Eq | BinOp::Neq) && is_heap_ty(&left.ty) {
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
        }
        None
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
        if matches!(op, BinOp::And | BinOp::Or) && matches!(left.ty, Ty::Bool) {
            let ops_mark = self.ops.len();
            let lhh_mark = self.live_heap_handles.len();
            // The RHS is evaluated INSIDE the taken IfThen/Else branch, so use
            // `lower_scalar_operand` — it wraps the operand in a per-branch frame that frees any
            // transient heap temp it allocates (a `contains(y, "@")` materializes its String
            // arg) WITHIN the branch, keeping it `i…d`-balanced. (The eager path used
            // `lower_scalar_operand` too; using bare `lower_scalar_value` walled those heap-temp
            // operands → a coverage regression.) The LHS (a pure Bool) is likewise framed.
            if let Some(lhs) = self.lower_scalar_operand(left) {
                let dst = self.fresh_value();
                self.ops.push(Op::IfThen { cond: lhs, dst: Some(dst) });
                // THEN branch: `and` evaluates RHS here; `or` yields the constant `true`.
                let then_val = if matches!(op, BinOp::And) {
                    self.lower_scalar_operand(right)
                } else {
                    let t = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: t, value: 1 });
                    Some(t)
                };
                if let Some(tv) = then_val {
                    self.ops.push(Op::Else { val: Some(tv) });
                    // ELSE branch: `and` yields the constant `false`; `or` evaluates RHS here.
                    let else_val = if matches!(op, BinOp::And) {
                        let f = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: f, value: 0 });
                        Some(f)
                    } else {
                        self.lower_scalar_operand(right)
                    };
                    if let Some(ev) = else_val {
                        self.ops.push(Op::EndIf { val: Some(ev) });
                        return Some(dst);
                    }
                }
            }
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        }
        None
    }

    /// Extracted from `Self::lower_scalar_binop_shortcircuit_or_int` (eighth-round split,
    /// cog reduction): the final eager `IntBinOp` fallback (with the narrow
    /// signed-division-overflow guard), verbatim (only reached when the operator is NOT
    /// `and`/`or` over Bool).
    /// Extracted from `Self::lower_scalar_binop_int_fallback` (ninth-round split, cog
    /// reduction): the pure `BinOp` + operand-shape → `IntOp` lookup, verbatim (a static
    /// value computation, no `&mut self` needed).
    fn scalar_binop_int_op(op: &almide_ir::BinOp, left_ty: &Ty) -> Option<crate::IntOp> {
        use almide_ir::BinOp;
        Some(match op {
            BinOp::AddInt => crate::IntOp::Add,
            BinOp::SubInt => crate::IntOp::Sub,
            BinOp::MulInt => crate::IntOp::Mul,
            BinOp::DivInt => crate::IntOp::Div,
            BinOp::ModInt => crate::IntOp::Mod,
            // Ordering comparisons (the `if` condition) — INT or BOOL operands (Bool is an i64
            // 0/1, and v0's bool Ord is false < true = 0 < 1, so the i64 compare is bit-exact).
            // A Float compare uses the prim float floor above; String ordering is the cmp-call
            // above. Gate on the operand type.
            BinOp::Lt if Self::int_ord_operand_ty(left_ty) => crate::IntOp::Lt,
            BinOp::Lte if Self::int_ord_operand_ty(left_ty) => crate::IntOp::Le,
            BinOp::Gt if Self::int_ord_operand_ty(left_ty) => crate::IntOp::Gt,
            BinOp::Gte if Self::int_ord_operand_ty(left_ty) => crate::IntOp::Ge,
            // Equality — INT or BOOL operands. A `Bool` is an i64 0/1 (a Var loads
            // its 0/1, a `LitBool` materializes `ConstInt 0/1` above), so the SAME
            // `IntOp::Eq`/`Ne` render is bit-exact for `b == false` / `b1 != b2` as
            // for `n == 0`. (Ordering on Bool is undefined in v0, so it is NOT
            // admitted; a Float/String/compound `==` still needs a distinct op.)
            BinOp::Eq if Self::int_eq_operand_ty(left_ty) => crate::IntOp::Eq,
            BinOp::Neq if Self::int_eq_operand_ty(left_ty) => crate::IntOp::Ne,
            // (Logical `and`/`or` are SHORT-CIRCUITED via control flow above — they never
            // reach this eager `IntBinOp` path. Native + interp evaluate the RHS lazily.)
            // Pow, Float, concat, non-Int/Bool compares: defer.
            _ => return None,
        })
    }

    fn lower_scalar_binop_int_fallback(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        let iop = Self::scalar_binop_int_op(op, &left.ty)?;
        let a = self.lower_scalar_value(left)?;
        let b = self.lower_scalar_value(right)?;
        self.emit_narrow_div_overflow_guard(iop, &left.ty, a, b);
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: iop, a, b });
        Some(dst)
    }

    /// Extracted from `Self::lower_scalar_binop_int_fallback` (ninth-round split, cog
    /// reduction): the narrow signed-division-overflow guard injection, verbatim.
    fn emit_narrow_div_overflow_guard(&mut self, iop: crate::IntOp, left_ty: &Ty, a: ValueId, b: ValueId) {
        // NARROW signed division overflow (`Int8` MIN ÷ -1 — int8_div_overflow):
        // the operands live in the i64 model, so the preamble's checked helper
        // only catches i64::MIN ÷ -1; the narrow MIN wraps silently (v0 aborts
        // "Error: integer overflow" + exit 1). Inject the width guard as MIR ops:
        // if (a == MIN_w) & (b == -1) → prim.die with the SAME message bytes.
        if !matches!(iop, crate::IntOp::Div | crate::IntOp::Mod) {
            return;
        }
        let min_w = match left_ty {
            Ty::Int8 => Some(-128i64),
            Ty::Int16 => Some(-32768i64),
            Ty::Int32 => Some(-2147483648i64),
            _ => None,
        };
        let Some(mw) = min_w else { return };
        let minc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: minc, value: mw });
        let negc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: negc, value: -1 });
        let c1 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: c1, op: crate::IntOp::Eq, a, b: minc });
        let c2 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: c2, op: crate::IntOp::Eq, a: b, b: negc });
        let both = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both, op: crate::IntOp::And, a: c1, b: c2 });
        let msg = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: msg,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::Str("Error: integer overflow\n".into()),
        });
        let mh = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(mh), args: vec![msg] });
        self.ops.push(Op::IfThen { cond: both, dst: None });
        self.ops.push(Op::Prim { kind: crate::PrimKind::Die, dst: None, args: vec![mh] });
        self.ops.push(Op::Else { val: None });
        self.ops.push(Op::EndIf { val: None });
        // the message block is dead on the non-abort path — release it
        self.ops.push(Op::Drop { v: msg });
    }

    /// Lower a `prim.*` PRIMITIVE-FLOOR call to an [`Op::Prim`] — the v1 self-host
    /// floor (raw memory + the fd_write host call), mapped by name, NOT a real
    /// `CallFn`/runtime symbol. Each arg lowers to a ValueId via
    /// [`Self::lower_scalar_value`] (a handle var / int literal / int-arith). Returns
    /// the result `dst` (load / fd_write / handle) or `None` (a store is Unit).
    pub(crate) fn lower_prim_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        // Each `matches!` mirrors the exact `func == "…"` name set the corresponding
        // group's own blocks test — a router by NAME only, no behavior of its own; the
        // group function is never called for a name outside its set.
        if matches!(
            func,
            "alloc_str" | "alloc_bytes" | "alloc_list" | "alloc_list_f64" | "alloc_set"
                | "alloc_map" | "alloc_value" | "alloc_list_str" | "alloc_set_str"
                | "alloc_map_str" | "alloc_map_skv" | "alloc_map_kv" | "store_str"
        ) {
            return self.lower_prim_call_alloc(func, args);
        }
        if matches!(
            func,
            "args_get_list" | "args_get_list_full" | "env_get" | "read_text_file"
                | "read_bytes_file" | "read_dir" | "write_text_file" | "make_dir"
                | "remove_all" | "path_filestat" | "path_exists"
        ) {
            return self.lower_prim_call_fs_env(func, args);
        }
        if matches!(func, "ptr_to_int" | "int_to_ptr" | "read_line" | "read_n_bytes") {
            return self.lower_prim_call_ptr_io(func, args);
        }
        self.lower_prim_call_generic(func, args)
    }

    /// Extracted from `Self::lower_prim_call` (eleventh-round split, cog reduction): the
    /// `alloc_*`/`store_str` name group, verbatim (only ever called for a name in the
    /// router's matching `matches!` set, so no "unrecognized name" fallthrough is needed).
    fn lower_prim_call_alloc(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        if matches!(func, "alloc_str" | "alloc_bytes" | "alloc_list" | "alloc_list_f64" | "alloc_set" | "alloc_map" | "alloc_value") {
            return self.lower_prim_call_alloc_scalar(func, args);
        }
        self.lower_prim_call_alloc_str(func, args)
    }

    /// Extracted from `Self::lower_prim_call_alloc` (twelfth-round split, cog reduction):
    /// the scalar-element alloc name group, verbatim (only ever called for a name in the
    /// caller's matching `matches!` set).
    fn lower_prim_call_alloc_scalar(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        // `prim.alloc_str(byte_len)` allocates a runtime-sized OWNED String — an `Op::Alloc`
        // (cert `i`, a fresh owned object), NOT a scalar prim. The caller fills its bytes
        // via `prim.store8`; the result is moved out / dropped like any heap value.
        // `prim.alloc_str(n)` / `prim.alloc_bytes(n)` BOTH allocate a runtime-sized OWNED byte
        // block (`Init::DynStr`: rc=1, len set, data filled by store8) — physically identical;
        // they differ only in the prim's DECLARED return type (String vs Bytes). A flat heap
        // value (no nested ownership), moved out / dropped like any String.
        if func == "alloc_str" || func == "alloc_bytes" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} length is not a lowerable scalar"))
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynStr { len: len_v },
            });
            return Ok(Some(dst));
        }
        // `prim.alloc_list(n)` allocates a runtime-sized OWNED `List[Int]` of n i64 slots —
        // an `Op::Alloc` (cert `i`), the list-building sibling of alloc_str. The caller
        // fills it via `prim.store64`; moved out / dropped like any heap value.
        if func == "alloc_list" || func == "alloc_list_f64" || func == "alloc_set" || func == "alloc_map" || func == "alloc_value" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.alloc_list length is not a lowerable scalar".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynList { len: len_v },
            });
            return Ok(Some(dst));
        }
        unreachable!("lower_prim_call_alloc_scalar called with a name outside its caller-matched set: {func}")
    }

    /// Extracted from `Self::lower_prim_call_alloc` (twelfth-round split, cog reduction):
    /// the heap-element (nested-ownership) alloc + `store_str` name group, verbatim (only
    /// ever called for a name outside `lower_prim_call_alloc_scalar`'s set).
    fn lower_prim_call_alloc_str(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        // `prim.alloc_list_str(n)` allocates a runtime-sized OWNED `List[String]` (n slots,
        // physically identical to alloc_list) — but the dst is tracked as a NESTED-OWNERSHIP
        // list, so its scope-end drop is a recursive `DropListStr` (frees the owned element
        // Strings) and `prim.store_str` Consumes each String moved into it (Machinery 2).
        if func == "alloc_list_str" || func == "alloc_set_str" || func == "alloc_map_str" || func == "alloc_map_skv" || func == "alloc_map_kv" {
            let len_v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.alloc_list_str length is not a lowerable scalar".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynListStr { len: len_v },
            });
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.store_str(list, byte_addr_of_slot, piece)` — store the String `piece`'s handle
        // into the list slot at `byte_addr_of_slot` AND CONSUME the piece (its reference is
        // MOVED into the list, which now owns it — cert `m`, removed from the scope drop set).
        // The slot holds the i64-widened handle; `DropListStr` later i32.wrap's it to free.
        if func == "store_str" {
            let addr = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.store_str slot address is not a lowerable scalar".into())
            })?;
            // The piece must be a tracked heap var (so we can Consume it). Its handle value:
            let piece = match &args[1].kind {
                IrExprKind::Var { id } => self.value_for(*id)?,
                _ => {
                    return Err(LowerError::Unsupported(
                        "prim.store_str piece must be a heap variable (to consume)".into(),
                    ))
                }
            };
            // The slot value is the piece's HANDLE (its address as an i64). Op::Prim Handle
            // gives that; store it 8-wide at the slot, then Consume the piece (move-out).
            let handle = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(handle), args: vec![piece] });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr, handle] });
            self.ops.push(Op::Consume { v: piece });
            self.live_heap_handles.retain(|h| *h != piece);
            return Ok(None);
        }
        unreachable!("lower_prim_call_alloc called with a name outside its router-matched set: {func}")
    }

    /// Extracted from `Self::lower_prim_call` (eleventh-round split, cog reduction): the
    /// WASI env/fs-floor name group, verbatim (only ever called for a name in the
    /// router's matching `matches!` set, so no "unrecognized name" fallthrough is needed).
    fn lower_prim_call_fs_env(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        if matches!(func, "args_get_list" | "args_get_list_full" | "env_get") {
            return self.lower_prim_call_env(func, args);
        }
        self.lower_prim_call_fs(func, args)
    }

    /// Extracted from `Self::lower_prim_call_fs_env` (twelfth-round split, cog
    /// reduction): the CLI-args/environ name group, verbatim (only ever called for a
    /// name in the caller's matching `matches!` set).
    fn lower_prim_call_env(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        // `prim.args_get_list()` — the WASI args→`List[String]` floor (env.args). NO
        // args; its dst is a FRESH OWNED `List[String]` of `argv[1..]` (a heap Ptr, like
        // an Alloc), so it is registered like `alloc_list_str`: a NESTED-OWNERSHIP list
        // whose scope-end drop is the recursive `DropListStr` (frees the owned element
        // Strings) — a flat `Drop` would leak them. Carries Capability::CliArgs (counted
        // in cap_witness). The render emits the WASI args_sizes_get/args_get sequence.
        if func == "args_get_list" {
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ArgsGetList, dst: Some(dst), args: vec![] });
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.args_get_list_full()` — the argv[0]-INCLUSIVE twin (process.args =
        // std::env::args()). Same fresh OWNED List[String] + DropListStr + CliArgs
        // discipline; renders through the SAME parameterized $args_get_list bridge.
        if func == "args_get_list_full" {
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ArgsGetListFull, dst: Some(dst), args: vec![] });
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.env_get(name)` — the WASI environ lookup floor (env.get). ONE BORROWED
        // `String` arg (the variable name; the caller still owns it). Its dst is a FRESH
        // OWNED `Option[String]` in the `materialize_opt_str_some` layout (0-slot none /
        // 1-slot some owning the value String @12), registered in `heap_elem_lists` so
        // the scope-end drop is the flat `DropListStr` (frees the payload String, if
        // any, then the block). Carries Capability::CliArgs — the Env profile's
        // canonical cap (counted in cap_witness exactly like ArgsGetList).
        if func == "env_get" && args.len() == 1 {
            let key = match self.lower_call_args(args)?.into_iter().next() {
                Some(CallArg::Handle(v)) => v,
                _ => {
                    return Err(LowerError::Unsupported(
                        "prim.env_get needs a borrowed String name".into(),
                    ))
                }
            };
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::EnvGet, dst: Some(dst), args: vec![key] });
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `env_get` with a wrong arg count (the ONLY name in this group with an extra
        // guard beyond the bare name test) falls all the way through the original
        // single-match to the terminal "unknown primitive" wall — replicated verbatim
        // here (NOT `unreachable!`: this guard genuinely can fail for a matched name).
        Err(LowerError::Unsupported(format!("unknown primitive prim.{func}")))
    }
}
