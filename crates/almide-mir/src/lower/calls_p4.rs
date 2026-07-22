impl LowerCtx {
    /// `Option[scalar] ==` core over two already-materialized Option block HANDLES (byte
    /// addresses). Branchless masked compare over the 0-or-1-element layout (tag = len @+4:
    /// None=0, Some=1; scalar payload at +12): eq = (tagL==tagR) AND (both-None OR payloadEq).
    /// The +12 load is UNCONDITIONAL but MASKED (a None side's +12 is an in-bounds garbage read
    /// the AND discards), so no control flow and no trap. Shared by the Var-operand path
    /// (`lower_eq_typed`) and the cond-eq materialized-operand path.
    pub(crate) fn option_scalar_eq_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        elem: &Ty,
    ) -> ValueId {
        let tag_l = self.load_at_offset(hl, 4, crate::PrimKind::Load { width: 4 });
        let tag_r = self.load_at_offset(hr, 4, crate::PrimKind::Load { width: 4 });
        let pay_l = self.load_at_offset(hl, 12, crate::PrimKind::Load { width: 8 });
        let pay_r = self.load_at_offset(hr, 12, crate::PrimKind::Load { width: 8 });
        let tags_eq = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: tags_eq, op: crate::IntOp::Eq, a: tag_l, b: tag_r });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let is_none = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: is_none, op: crate::IntOp::Eq, a: tag_l, b: zero });
        let pay_eq = self.fresh_value();
        if matches!(elem, Ty::Float) {
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::FloatCmp(crate::FCmpOp::Eq),
                dst: Some(pay_eq),
                args: vec![pay_l, pay_r],
            });
        } else {
            self.ops.push(Op::IntBinOp { dst: pay_eq, op: crate::IntOp::Eq, a: pay_l, b: pay_r });
        }
        let none_or_pay = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: none_or_pay, op: crate::IntOp::Or, a: is_none, b: pay_eq });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::And, a: tags_eq, b: none_or_pay });
        dst
    }

    /// `Option[heap] ==` core over two already-materialized DynListStr Option block HANDLES.
    /// CONDITIONAL compare — the payload eq (the recursive typed eq over the loaded payload
    /// handles: string.eq/value.eq/list.eq_* calls, or a nested tuple/variant/Option compose)
    /// runs ONLY when BOTH are Some, so a None side's payload handle is never dereferenced:
    /// dst = if (tagL AND tagR) then payloadEq(data[0]) else (tagL == tagR). Shared by the
    /// value-position BinOp and cond-eq paths (both via `typed_slot_eq`).
    pub(crate) fn option_heap_eq_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        elem_ty: &Ty,
        depth: u32,
    ) -> Option<ValueId> {
        let tag_l = self.load_at_offset(hl, 4, crate::PrimKind::Load { width: 4 });
        let tag_r = self.load_at_offset(hr, 4, crate::PrimKind::Load { width: 4 });
        let both_some = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both_some, op: crate::IntOp::And, a: tag_l, b: tag_r });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: both_some, dst: Some(dst) });
        // THEN (both Some): the heap payload eq — only here is data[0] dereferenced.
        let pay_l = self.load_payload_addr(hl, 12);
        let pay_r = self.load_payload_addr(hr, 12);
        let pay_eq = self.typed_slot_eq(pay_l, pay_r, elem_ty, depth + 1)?;
        self.ops.push(Op::Else { val: Some(pay_eq) });
        // ELSE: both-None → equal, one-Some-one-None → not equal (== of the tags).
        let tags_eq = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: tags_eq, op: crate::IntOp::Eq, a: tag_l, b: tag_r });
        self.ops.push(Op::EndIf { val: Some(tags_eq) });
        Some(dst)
    }

    /// `Result[scalar, String] ==` core over two already-materialized Result block HANDLES.
    /// Ok is scalar (len@4=0, value@12), Err is a String (len@4=1, handle@12). The scalar okEq is
    /// computed unconditionally (masked); the heap errEq (`string.eq`) runs ONLY in the both-Err
    /// branch, so an Ok side's @12 (a scalar, not a handle) is never dereferenced:
    /// dst = if bothErr then errEq else (bothOk AND okEq). Shared by both eq-operand paths.
    pub(crate) fn result_scalar_eq_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        ok_ty: &Ty,
    ) -> Option<ValueId> {
        let tag_l = self.load_at_offset(hl, 4, crate::PrimKind::Load { width: 4 });
        let tag_r = self.load_at_offset(hr, 4, crate::PrimKind::Load { width: 4 });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let ok_l = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: ok_l, op: crate::IntOp::Eq, a: tag_l, b: zero });
        let ok_r = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: ok_r, op: crate::IntOp::Eq, a: tag_r, b: zero });
        let both_ok = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both_ok, op: crate::IntOp::And, a: ok_l, b: ok_r });
        let pay_ok_l = self.load_at_offset(hl, 12, crate::PrimKind::Load { width: 8 });
        let pay_ok_r = self.load_at_offset(hr, 12, crate::PrimKind::Load { width: 8 });
        let ok_eq = self.fresh_value();
        if matches!(ok_ty, Ty::Float) {
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::FloatCmp(crate::FCmpOp::Eq),
                dst: Some(ok_eq),
                args: vec![pay_ok_l, pay_ok_r],
            });
        } else {
            self.ops.push(Op::IntBinOp { dst: ok_eq, op: crate::IntOp::Eq, a: pay_ok_l, b: pay_ok_r });
        }
        let inner = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: inner, op: crate::IntOp::And, a: both_ok, b: ok_eq });
        let both_err = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both_err, op: crate::IntOp::And, a: tag_l, b: tag_r });
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: both_err, dst: Some(dst) });
        // THEN (both Err): the heap String eq — only here is @12 read as a handle.
        let pay_err_l = self.load_payload_addr(hl, 12);
        let pay_err_r = self.load_payload_addr(hr, 12);
        let err_eq = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(err_eq),
            name: "string.eq".to_string(),
            args: vec![CallArg::Handle(pay_err_l), CallArg::Handle(pay_err_r)],
            result: Some(repr_of(&Ty::Bool).ok()?),
        });
        self.ops.push(Op::Else { val: Some(err_eq) });
        self.ops.push(Op::EndIf { val: Some(inner) });
        Some(dst)
    }

    fn lower_scalar_value_inner(&mut self, expr: &IrExpr) -> Option<ValueId> {
        match &expr.kind {
            IrExprKind::Var { id } => self.value_or_global(*id).ok(),
            // A SCALAR BLOCK body (`{ let freq = …; let h = …; h * enorm }` — the mel
            // inner map): lower the binds then the scalar tail. Any stmt outside the
            // subset rolls this back (partial ops truncated) and returns None — the
            // caller keeps its own fallback/wall.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let mark = self.ops.len();
                let lhh = self.live_heap_handles.len();
                for st in stmts {
                    if self.lower_stmt(st).is_err() {
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh);
                        return None;
                    }
                }
                match self.lower_scalar_value(tail) {
                    Some(v) => Some(v),
                    None => {
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh);
                        None
                    }
                }
            }
            // A SCALAR record field / tuple element (`r.x`, `t.0`) — load from the
            // block's layout slot. Defers (→ None) for a non-materialized container.
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } => {
                self.lower_scalar_field_access(expr)
            }
            // A scalar list element `xs[i]` (`xs: List[Int/Float/Bool]`) — a bounds-checked
            // element load. Defers (→ None) for a heap element (an i32-handle slot) or a
            // non-resolvable container.
            IrExprKind::IndexAccess { object, index } => {
                self.lower_scalar_index_access(object, index, &expr.ty)
            }
            IrExprKind::LitInt { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: *value });
                Some(dst)
            }
            // A Bool is a scalar int (true = 1, false = 0) — the `if` condition.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: if *value { 1 } else { 0 } });
                Some(dst)
            }
            // A FLOAT literal: the i64-uniform value holds the f64 BITS (LOW-32 f32 bits for a
            // Float32-typed literal), so `3.5` materializes as `ConstInt(float_lit_bits(3.5))`.
            // The render's float prims reinterpret it back.
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: crate::lower::float_lit_bits(*value, &expr.ty) });
                Some(dst)
            }
            // Decomposed (#781): the 340-line operator dispatch is a verbatim
            // text move into `lower_scalar_binop`.
            IrExprKind::BinOp { op, left, right } => {
                self.lower_scalar_binop(expr, op, left, right)
            }
            // A scalar-result PRIMITIVE-FLOOR call (`prim.handle`/`prim.load32`/
            // `prim.fd_write`) — `@intrinsic` lowers it to a `RuntimeCall`; we map the
            // `almide_rt_prim_*` symbol to an [`Op::Prim`] (NOT the deferred Const a
            // generic RuntimeCall gets). The self-host floor reaching executable code.
            IrExprKind::RuntimeCall { symbol, args } => {
                let func = symbol.as_str().strip_prefix("almide_rt_prim_")?;
                self.lower_prim_call(func, args).ok().flatten()
            }
            // The same prim floor reached as a MODULE call (`prim.handle(buf)`) in a value
            // position — e.g. an address operand `prim.handle(buf) + LIST_HEADER`. prim
            // calls are pure scalar/handle ops (no ownership), so this is the narrow,
            // sound subset (NOT the general scalar-call-in-operand admission).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "prim" =>
            {
                self.lower_prim_call(func.as_str(), args).ok().flatten()
            }
            // A sized-int WIDENING conversion (`int8.to_int64(x)` …) — the IDENTITY on
            // the canonical-i64 slot (see `identity_int_widening_call`): forward the
            // operand's value, no call, no ownership. The caps counter skips the node
            // by the same predicate, so `mir == ir` holds by construction.
            IrExprKind::Call { .. }
                if crate::lower::identity_int_widening_call(expr).is_some() =>
            {
                let operand = crate::lower::identity_int_widening_call(expr)?;
                self.lower_scalar_value(operand)
            }
            // `float.from_int(x)` — the single-instruction sitofp floor (#806 step 2):
            // one `PrimKind::F64FromInt`, no call, no ownership. The caps counter
            // skips the node by the same predicate, so `mir == ir` by construction.
            IrExprKind::Call { .. }
                if crate::lower::float_from_int_prim_call(expr).is_some() =>
            {
                let operand = crate::lower::float_from_int_prim_call(expr)?;
                let v = self.lower_scalar_value(operand)?;
                let dst = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: crate::PrimKind::F64FromInt,
                    dst: Some(dst),
                    args: vec![v],
                });
                Some(dst)
            }
            // A scalar `if`/`match` as an OPERAND (`a + (if c then 1 else 2)`,
            // `n + match k { 0 => x, _ => y }`): EXECUTE it to a scalar via the same
            // `try_lower_scalar_if` the let-bind path uses — only the taken arm runs. The
            // helper is self-contained: it marks BOTH `ops` and `live_heap_handles`, drops
            // every per-arm heap temp WITHIN its arm (so on success `live_heap_handles` is
            // exactly at entry — no net ownership event), and fully rolls back on a miss. So
            // it honors `lower_scalar_value`'s contract and a caller's `ops`-only truncate
            // stays correct. A heap-RESULT if/match is NOT this path (string `+` is ConcatStr,
            // and a let-bound heap if is the separate escalated-cert path) — it defers.
            IrExprKind::If { cond, then, else_ } if !is_heap_ty(&expr.ty) => {
                self.try_lower_scalar_if(cond, then, else_, &expr.ty)
            }
            IrExprKind::Match { subject, arms } if !is_heap_ty(&expr.ty) => {
                // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
                if let Some(dst) = self.try_lower_custom_variant_match(subject, arms, &expr.ty) {
                    return Some(dst);
                }
                // A TUPLE-of-bound-vars subject (the frontend's factored form of a
                // multi-arm nested-ctor match — C-070): per-element refinement chain.
                if let Some(dst) = self.try_lower_tuple_refinement_match(subject, arms, &expr.ty) {
                    return Some(dst);
                }
                // A VARIANT (Option/Result) subject — execute via the tag-read value-match
                // (ctor patterns are not `subj == lit`, so `desugar_match_to_if` can't reach
                // them; the result would stay an unset 0 = a silent miscompile).
                if is_heap_ty(&subject.ty) {
                    return self.try_lower_variant_value_match(subject, arms, &expr.ty);
                }
                // The desugared chain may be an `If` (literal arms) OR a `Block` (`{ let x =
                // subj; if … }` for a binder/guarded-binder arm) — `lower_scalar_arm` handles
                // both (its tail-`if`/`match` recursion runs the scalar-if machinery).
                let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
                self.lower_scalar_arm(&if_expr)
            }
            // A scalar user/stdlib CALL as an OPERAND (`5 + string.len(s)`, `5 +
            // string.len("abc")` after the optimizer inlines a `let s = "abc"`, `g(a) +
            // h(b)`, `string.len(s) > 0`, a nested `f(g(x))`): EXECUTE it via the same
            // `try_lower_scalar_call` the direct-bind path uses. Its argument lowering
            // (`lower_call_args`) materializes/borrows heap args exactly as a bound `let k =
            // call` already does — a heap `Var` is BORROWED (`CallArg::Handle`, no ownership
            // event), a FRESH heap literal is `Alloc`'d into an owned temp released at scope
            // end. The latter pushes to `live_heap_handles`, but the SELF-ROLLBACK wrapper
            // (see `lower_scalar_value`) restores both `ops` and `live_heap_handles` if this
            // (or a sibling operand) later fails, so the materialize is rollback-safe. A
            // Method/Computed/impure-Module callee returns `None` from `try_lower_scalar_call`
            // (rolled back) and DEFERS — honest, the caps fold tags the elided callee. A heap
            // RESULT operand is NOT this path (string `+` is ConcatStr; a let-bound heap if is
            // the separate escalated-cert path) — it is gated out by `!is_heap_ty`.
            IrExprKind::Call { .. } if !is_heap_ty(&expr.ty) => {
                self.try_lower_scalar_call(expr, &expr.ty)
            }
            // A scalar UNARY op (`-a`, `not x`). The operand lowers via the SAME scalar
            // value path (a Var load, a literal, a nested compare/arith) — if it is not
            // scalar-lowerable we return None (WALL/defer), never a silent 0. Previously
            // there was NO UnOp arm here, so EVERY `-a` / `not x` in a value position fell
            // through to `_ => None` → the caller's Const-0 materialization, reading 0; and
            // in an `if` CONDITION the un-lowered cond made `try_lower_scalar_if` /
            // `try_lower_unit_if` run BOTH arms. This arm closes both failures.
            IrExprKind::UnOp { op, operand } => {
                use almide_ir::UnOp;
                // The operand is EAGER (always evaluated), so a `not (c == "'")` whose
                // `string.eq` materializes the `"'"` literal is lowered with that temp
                // `Alloc`'d + `Drop`'d LOCALLY (`lower_scalar_operand`'s frame) — internally
                // `i…d`-balanced, no temp escaping to the enclosing per-arm heap frame. A
                // non-scalar-lowerable operand still WALLS (→ None).
                let x = self.lower_scalar_operand(operand)?;
                let dst = self.fresh_value();
                match op {
                    // Integer negation: `0 - x` (no dedicated wasm i64 negate op; the
                    // IntBinOp Sub renders `i64.sub` of a ConstInt 0 and x, matching v0's
                    // `0i64 - x`). i64::MIN negation overflows identically to v0's wrapping.
                    UnOp::NegInt => {
                        let zero = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
                        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: zero, b: x });
                        Some(dst)
                    }
                    // Float negation: the existing `f64.neg` prim (the i64-uniform value
                    // holds the f64 bits; the prim reinterprets around `f64.neg` — sign-bit
                    // flip, so `-0.0` and NaN behave exactly as v0's `f64::neg`).
                    UnOp::NegFloat => {
                        let kind = if matches!(operand.ty, Ty::Float32) {
                            crate::PrimKind::F32Un(crate::FUnOp::Neg)
                        } else {
                            crate::PrimKind::FloatUn(crate::FUnOp::Neg)
                        };
                        self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![x] });
                        Some(dst)
                    }
                    // Boolean `not`: a Bool is an i64 0/1, so `1 - b` flips it (b∈{0,1} →
                    // 1-b∈{1,0}). Renders `i64.sub` of ConstInt 1 and b; the result stays in
                    // {0,1}, a valid Bool the `if` condition reads uniformly.
                    UnOp::Not => {
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        self.ops.push(Op::IntBinOp { dst, op: crate::IntOp::Sub, a: one, b: x });
                        Some(dst)
                    }
                }
            }
            // A SCALAR `??` in a value/operand position (`(int.parse(s) ?? 0) - 48`,
            // `(codepoint(ch) ?? 0)` fed to arithmetic) — execute the unwrap (tag read +
            // payload-or-fallback) via the same machinery the tail/let positions use. Without this
            // arm a `??` operand fell to `_ => None` → the caller's `Const 0`, so the WHOLE BinOp
            // silently read 0 (`(x ?? 0) - 48` → 0, not x-48). Scalar result only — a heap-String
            // `??` is not a scalar value operand. `try_lower_option_unwrap_or` is rollback-safe and
            // emits its own balanced Option materialize/drop, exactly like the scalar-Call arm above.
            IrExprKind::UnwrapOr { expr, fallback } if !is_heap_ty(&fallback.ty) => {
                self.try_lower_option_unwrap_or(expr, fallback, false)
            }
            // A scalar `e!` (Unwrap) in a VALUE/OPERAND position (`acc + int.parse(s)!`) over a
            // `Result[scalar, String]` call. The auto-`?` left the operand as `Unwrap{Call(Result)}`
            // (the let-bind position got it type-stripped to a bare scalar Call; the BinOp operand
            // did not). Lower the inner call to its OWNED Result block, then EXTRACT the Ok scalar
            // payload @12 (the same len-as-tag layout `let x = parse(s)!` reads). The `!` traps on
            // Err in the v1 model; the Ok-payload read is correct for the Ok path. WITHOUT this arm
            // the operand fell to `_ => None` → the whole BinOp arg rolled back to `Const 0` (the
            // recursive `acc + parse!` accumulator silently summed 0). Scalar Ok only — a heap-Ok
            // `Result[String,String]!` value operand stays walled (the recursive-ownership frontier).
            IrExprKind::Unwrap { expr: inner }
                if !is_heap_ty(&expr.ty) && is_result_ty(&inner.ty) =>
            {
                let ops_mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                // Lower the inner call as a heap value (the materialized Result block). Reuse the
                // scalar-call machinery with the inner's REAL Result type so it builds/borrows the
                // block (registered `materialized_results`), exactly like the bind.
                if let Some(block) = self.try_lower_scalar_call(inner, &inner.ty) {
                    // Ok payload @12 (len-as-tag: Ok = len 0, the scalar in slot 0 @12).
                    let h = self.fresh_value();
                    self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![block] });
                    let off = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: off, value: 12 });
                    let addr = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
                    let payload = self.fresh_value();
                    self.ops.push(Op::Prim { kind: crate::PrimKind::Load { width: 8 }, dst: Some(payload), args: vec![addr] });
                    return Some(payload);
                }
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                None
            }
            _ => None,
        }
    }

    /// The scalar `BinOp` dispatch of [`Self::lower_scalar_value_inner`] —
    /// arithmetic / comparison / logic / concat over executable scalar operands.
    /// Verbatim text move (#781, the cog-198 driver arm).
    ///
    /// SIZED-NUMERIC admission (the three predicates below): every sized integer
    /// width lives in the SAME canonical-i64 runtime value as `Int` (sign-extended
    /// signed / zero-extended unsigned — the `Ty` docs pin `Int64` repr == `Int`,
    /// and `extern_wasm_abi` maps all widths to one I64), so `i64.eq`/`i64.ne` is
    /// bit-exact for ALL of them. ORDERING is the signed compare — correct for the
    /// signed widths and for unsigned ≤ 32 bits (zero-extended ⇒ non-negative), but
    /// WRONG for a `UInt64` ≥ 2^63 (reads negative), so `UInt64` stays walled there
    /// (`IntOp` has no unsigned compare). Floats are uniformly f64-bits-in-i64
    /// (`LitFloat`, the prim float floor, and the extern ABI all agree), so
    /// `Float32`/`Float64` ride the same `FloatCmp` as `Float`.
    pub(crate) fn int_eq_operand_ty(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Int
                | Ty::Bool
                | Ty::Int8
                | Ty::Int16
                | Ty::Int32
                | Ty::Int64
                | Ty::UInt8
                | Ty::UInt16
                | Ty::UInt32
                | Ty::UInt64
        )
    }

    pub(crate) fn int_ord_operand_ty(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Int
                | Ty::Bool
                | Ty::Int8
                | Ty::Int16
                | Ty::Int32
                | Ty::Int64
                | Ty::UInt8
                | Ty::UInt16
                | Ty::UInt32
        )
    }

    pub(crate) fn float_operand_ty(ty: &Ty) -> bool {
        matches!(ty, Ty::Float | Ty::Float32 | Ty::Float64)
    }

    fn lower_scalar_binop(
        &mut self,
        expr: &IrExpr,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        if let Some(dst) = self.lower_scalar_binop_pow_float(expr, op, left, right) {
            return Some(dst);
        }
        if let Some(dst) = self.lower_scalar_binop_eq_family(op, left, right) {
            return Some(dst);
        }
        if let Some(dst) = self.lower_scalar_binop_cmp_and_heap_eq(op, left, right) {
            return Some(dst);
        }
        self.lower_scalar_binop_shortcircuit_or_int(op, left, right)
    }

    /// Extracted from `Self::lower_scalar_binop` (seventh-round split, cog reduction):
    /// the `**` pow-desugar + FLOAT prim sub-chain, verbatim.
    fn lower_scalar_binop_pow_float(
        &mut self,
        expr: &IrExpr,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        if let Some(dst) = self.lower_scalar_binop_pow(expr, op, left, right) {
            return Some(dst);
        }
        self.lower_scalar_binop_float_prim(op, left, right)
    }

    /// Extracted from `Self::lower_scalar_binop_pow_float` (eighth-round split, cog
    /// reduction): the `**` pow-desugar sub-chain, verbatim.
    fn lower_scalar_binop_pow(
        &mut self,
        expr: &IrExpr,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        use almide_ir::BinOp;
        // The `**` OPERATOR has no single hardware instruction — it desugars to a CALL into
        // the self-hosted pow stdlib, exactly as if the user wrote `math.fpow(a, b)` /
        // `math.pow(a, b)`. `PowFloat` → `math.fpow` (the bit-exact libm transcription),
        // `PowInt` → `math.pow` (exponentiation-by-squaring). Both callees live in a
        // PURE_MODULES module, so the synthesized `Op::CallFn` carries an EMPTY capability
        // witness (sound), and the corpus `count_ir_calls` credits the operator node 1:1 so
        // `mir_calls <= ir_calls` holds BY CONSTRUCTION (no elision-masking over-count).
        let pow_callee = match op {
            BinOp::PowFloat => Some("math.fpow"),
            BinOp::PowInt => Some("math.pow"),
            _ => None,
        };
        if let Some(callee) = pow_callee {
            let a = self.lower_scalar_value(left)?;
            let b = self.lower_scalar_value(right)?;
            let repr = repr_of(&expr.ty).ok()?;
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: callee.to_string(),
                args: vec![CallArg::Scalar(a), CallArg::Scalar(b)],
                result: Some(repr),
            });
            return Some(dst);
        }
        None
    }

    /// Extracted from `Self::lower_scalar_binop_pow_float` (eighth-round split, cog
    /// reduction): the FLOAT prim-floor sub-chain, verbatim.
    /// Extracted from `Self::lower_scalar_binop_float_prim` (ninth-round split, cog
    /// reduction): the pure `BinOp` + f32/f64-shape → `PrimKind` lookup, verbatim (a
    /// static value computation, no `&mut self` needed).
    fn scalar_binop_float_prim_kind(
        op: &almide_ir::BinOp,
        is_f32: bool,
        ty: &Ty,
    ) -> Option<crate::PrimKind> {
        Self::scalar_binop_float_prim_arith_kind(op, is_f32)
            .or_else(|| Self::scalar_binop_float_prim_cmp_kind(op, is_f32, ty))
    }

    /// Extracted from `Self::scalar_binop_float_prim_kind` (tenth-round split, cog
    /// reduction): the f32/f64 arithmetic sub-match, verbatim (disjoint patterns from the
    /// comparison half — a pure lookup, safe to split via `.or_else`).
    fn scalar_binop_float_prim_arith_kind(op: &almide_ir::BinOp, is_f32: bool) -> Option<crate::PrimKind> {
        use almide_ir::BinOp;
        match op {
            BinOp::AddFloat if is_f32 => Some(crate::PrimKind::F32Bin(crate::FBinOp::Add)),
            BinOp::SubFloat if is_f32 => Some(crate::PrimKind::F32Bin(crate::FBinOp::Sub)),
            BinOp::MulFloat if is_f32 => Some(crate::PrimKind::F32Bin(crate::FBinOp::Mul)),
            BinOp::DivFloat if is_f32 => Some(crate::PrimKind::F32Bin(crate::FBinOp::Div)),
            BinOp::AddFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Add)),
            BinOp::SubFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Sub)),
            BinOp::MulFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Mul)),
            BinOp::DivFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Div)),
            _ => None,
        }
    }

    /// Extracted from `Self::scalar_binop_float_prim_kind` (tenth-round split, cog
    /// reduction): the f32/f64 comparison sub-match, verbatim (disjoint patterns from the
    /// arithmetic half — a pure lookup, safe to split via `.or_else`).
    fn scalar_binop_float_prim_cmp_kind(op: &almide_ir::BinOp, is_f32: bool, ty: &Ty) -> Option<crate::PrimKind> {
        // The two guards CAN both be true (`float_operand_ty` includes `Float32`), so the
        // F32 group MUST be tried first — exactly the original arm order (`is_f32` arms
        // precede the `float_operand_ty` arms in the single source match).
        Self::scalar_binop_float_prim_f32_cmp_kind(op, is_f32)
            .or_else(|| Self::scalar_binop_float_prim_f64_cmp_kind(op, ty))
    }

    /// Extracted from `Self::scalar_binop_float_prim_cmp_kind` (eleventh-round split, cog
    /// reduction): the F32Cmp arms, verbatim.
    fn scalar_binop_float_prim_f32_cmp_kind(op: &almide_ir::BinOp, is_f32: bool) -> Option<crate::PrimKind> {
        use almide_ir::BinOp;
        match op {
            BinOp::Lt if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Lt)),
            BinOp::Lte if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Le)),
            BinOp::Gt if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Gt)),
            BinOp::Gte if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Ge)),
            BinOp::Eq if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Eq)),
            BinOp::Neq if is_f32 => Some(crate::PrimKind::F32Cmp(crate::FCmpOp::Ne)),
            _ => None,
        }
    }

    /// Extracted from `Self::scalar_binop_float_prim_cmp_kind` (eleventh-round split, cog
    /// reduction): the FloatCmp arms, verbatim. Only reached (per the caller's `.or_else`)
    /// when the F32 group did not match.
    fn scalar_binop_float_prim_f64_cmp_kind(op: &almide_ir::BinOp, ty: &Ty) -> Option<crate::PrimKind> {
        use almide_ir::BinOp;
        match op {
            BinOp::Lt if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Lt)),
            BinOp::Lte if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Le)),
            BinOp::Gt if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Gt)),
            BinOp::Gte if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ge)),
            BinOp::Eq if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Eq)),
            BinOp::Neq if Self::float_operand_ty(ty) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ne)),
            _ => None,
        }
    }

    fn lower_scalar_binop_float_prim(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        // FLOAT arithmetic + comparison operators → the prim float floor (Op::Prim). The
        // operands are Float (the i64-uniform f64 bits); the prim reinterprets around the
        // wasm f64 op. Pure scalar — no ownership (cert untouched). This makes float-heavy
        // self-host (libm / dtoa) write `a * b` instead of `prim.fmul(a, b)`.
        // A Float32 operand carries the LOW-32 f32 pattern (the F32Demote/IntToF32
        // convention), so it routes to the F32 prim family: the f64 ops on those bits
        // computed a denormal-garbage sum, and per-op f32 rounding is the native/v0
        // semantics anyway.
        let is_f32 = matches!(left.ty, Ty::Float32);
        let fkind = Self::scalar_binop_float_prim_kind(op, is_f32, &left.ty);
        if let Some(kind) = fkind {
            let a = self.lower_scalar_value(left)?;
            let b = self.lower_scalar_value(right)?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![a, b] });
            return Some(dst);
        }
        None
    }

    /// Extracted from `Self::lower_scalar_binop` (seventh-round split, cog reduction):
    /// the String/Value/List/Map deep-equality sub-chain, verbatim.
    fn lower_scalar_binop_eq_family(
        &mut self,
        op: &almide_ir::BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Option<ValueId> {
        if let Some(dst) = self.lower_scalar_binop_eq_string_value(op, left, right) {
            return Some(dst);
        }
        self.lower_scalar_binop_eq_list_map(op, left, right)
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
        if es.len() != 1 {
            None
        } else if matches!(es[0], Ty::Int) {
            Some("eq_int")
        } else if matches!(es[0], Ty::String) {
            Some("eq_str")
        } else if crate::lower::is_value_ty(&es[0]) {
            Some("eq_value")
        } else if matches!(es[0], Ty::Float) {
            Some("eq_float")
        } else if matches!(es[0], Ty::Bool) {
            Some("eq_bool")
        } else {
            None
        }
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
        use crate::PrimKind;
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
        // `prim.read_text_file(path)` — the WASI file-read floor (fs.read_text). ONE
        // BORROWED `String` arg (the path; the caller still owns it). Its dst is a FRESH
        // OWNED `Result[String, String]` built by the render in the EXACT
        // `materialize_result_str` cap-as-tag layout (1-slot DynListStr, payload @12, tag
        // @16). Tracked like a heap-Ok Result: `materialized_results_str` so a downstream
        // `match`/`!` reads tag @16, AND `heap_elem_lists` so the heap-payload bind gates open
        // AND the scope-end drop is the flat `DropListStr` (frees the one owned String @12 +
        // the block — a flat `Drop` would leak the String). Carries Capability::FsRead
        // (counted in cap_witness). The render emits the WASI path_open/fd_read sequence.
        if func == "read_text_file" || func == "read_bytes_file" {
            // read_bytes_file is the raw-bytes twin: the SAME WASI floor + Result block
            // (the render's $read_text_file reads raw bytes; only the almd-level Ok TYPE
            // differs), so one PrimKind serves both.
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.read_text_file path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ReadTextFile, dst: Some(dst), args: vec![path] });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.read_dir(path)` — the WASI directory-listing floor (fs.list_dir). ONE BORROWED
        // `String` arg (the path). Its dst is a FRESH OWNED `Result[List[String], String]` built
        // by the render ($read_dir) in the cap-as-tag layout (1-slot wrapper, payload @12 = a
        // List[String], tag @16). Tracked like a heap-Ok Result: `materialized_results_str` so a
        // downstream `match`/`!` reads tag @16, AND `heap_elem_lists` so the heap-payload bind
        // gates open, AND `list_str_result_results` so the scope-end drop is the RECURSIVE
        // `DropResultListStr` (frees the payload List's element Strings + block; a flat
        // `DropListStr` would leak them) — checked BEFORE heap_elem_lists in `drop_op_for`.
        // Carries Capability::FsRead (counted in cap_witness). The render emits the WASI
        // path_open(O_DIRECTORY)/fd_readdir sequence (skip `.`/`..`, sort, build the list).
        if func == "read_dir" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.read_dir path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ReadDir, dst: Some(dst), args: vec![path] });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            self.list_str_result_results.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.write_text_file(path, content)` — the WASI file-WRITE floor (fs.write). TWO
        // BORROWED `String` args (the path + the content; the caller still owns both). Its dst is a
        // FRESH OWNED `Result[Unit, String]` built by the render ($write_text_file): Ok(()) with
        // `len@4 = 0` (no payload String — the `materialize_result_ok` convention) so the scope-end
        // flat `DropListStr` frees nothing at @12, or Err(msg) with `len@4 = 1` + `@12 = msg` (the
        // flat drop frees the one owned message). Tracked like a heap Result: `materialized_results_str`
        // so a downstream `match`/`!` reads the @16 tag, AND `heap_elem_lists` so the heap-payload
        // bind gates open AND the scope-end drop is the flat `DropListStr` (sound for BOTH arms given
        // the `len@4 = 0` Ok convention — NO `list_str_result_results`: there is no nested payload).
        // Carries Capability::FsWrite (counted in cap_witness). The render emits the WASI
        // path_open(O_CREAT|O_TRUNC)/fd_write sequence.
        if func == "write_text_file" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.write_text_file path is not a lowerable scalar/handle".into())
            })?;
            let content = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported("prim.write_text_file content is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::WriteTextFile,
                dst: Some(dst),
                args: vec![path, content],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.make_dir(path)` — the WASI directory-CREATE floor (fs.mkdir_p). ONE BORROWED
        // `String` arg (the path; the caller still owns it). Its dst is a FRESH OWNED
        // `Result[Unit, String]` built by the render ($make_dir): Ok(()) with `len@4 = 0` (no
        // payload String — the `materialize_result_ok` convention, IDENTICAL to write_text_file's
        // Ok arm) so the scope-end flat `DropListStr` frees nothing at @12, or Err(msg) with
        // `len@4 = 1` + `@12 = msg` (the flat drop frees the one owned message). Tracked exactly
        // like write_text_file's heap Result: `materialized_results_str` so a downstream `match`/`!`
        // reads the @16 tag, AND `heap_elem_lists` so the heap-payload bind gates open AND the
        // scope-end drop is the flat `DropListStr`. Carries Capability::FsWrite (a mkdir IS a
        // filesystem write — counted in cap_witness). The render emits the WASI recursive
        // path_create_directory sequence.
        if func == "make_dir" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.make_dir path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::MakeDir,
                dst: Some(dst),
                args: vec![path],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.remove_all(path)` — the WASI recursive-remove floor (fs.remove_all). ONE BORROWED
        // `String` arg (the path; the caller still owns it). Its dst is a FRESH OWNED
        // `Result[Unit, String]` built by the render ($remove_all): Ok(()) with `len@4 = 0` (no
        // payload String — the `materialize_result_ok` convention, IDENTICAL to make_dir's Ok arm)
        // so the scope-end flat `DropListStr` frees nothing at @12, or Err(msg) with `len@4 = 1` +
        // `@12 = msg` (the flat drop frees the one owned message). Tracked exactly like make_dir's
        // heap Result: `materialized_results_str` so a downstream `match`/`!` reads the @16 tag, AND
        // `heap_elem_lists` so the heap-payload bind gates open AND the scope-end drop is the flat
        // `DropListStr`. Carries Capability::FsWrite (a recursive remove IS a filesystem write —
        // counted in cap_witness). The render emits the WASI recursive
        // path_remove_directory/path_unlink_file sequence.
        if func == "remove_all" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.remove_all path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::RemoveAll,
                dst: Some(dst),
                args: vec![path],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.path_exists(path)` — the WASI path-stat floor (fs.exists). ONE BORROWED `String`
        // arg (the path; the caller still owns it). Its dst is a SCALAR `Bool` (i64 0/1) — UNLIKE
        // every other fs prim, a stat allocates NO heap result, so the dst is tracked in NO
        // classification set (no `materialized_results_str` / `heap_elem_lists`): it is a plain
        // scalar with no scope-end drop and no ownership-cert `i`. Carries Capability::FsRead (a
        // stat IS a filesystem read — counted in cap_witness). The render emits the WASI
        // path_filestat_get query (errno 0 = exists).
        // `prim.path_filestat(bufaddr, path)` — the WASI FULL-stat floor (fs.stat). TWO args: a
        // raw scratch ADDRESS (an i64 scalar — the self-host's own Bytes data region, so the
        // caller owns the buffer) and a BORROWED `String` path. dst = the SCALAR errno (0 = the
        // 64-byte WASI filestat is at bufaddr). Like path_exists this allocates NO heap result —
        // the dst joins no classification set. Carries Capability::FsRead (counted in cap_witness).
        if func == "path_filestat" {
            let bufaddr = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.path_filestat buffer address is not a lowerable scalar".into(),
                )
            })?;
            let path = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.path_filestat path is not a lowerable scalar/handle".into(),
                )
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::PathFilestat,
                dst: Some(dst),
                args: vec![bufaddr, path],
            });
            return Ok(Some(dst));
        }
        // `prim.ptr_to_int` / `prim.int_to_ptr` — REINTERPRET casts (identity at the
        // value level: the RawPtr IS the i64 address). No op emitted — the operand's
        // ValueId passes through, so the cert sees nothing (a pure hat-swap).
        if func == "ptr_to_int" || func == "int_to_ptr" {
            let v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim ptr cast operand is not a lowerable scalar".into(),
                )
            })?;
            return Ok(Some(v));
        }
        if func == "path_exists" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.path_exists path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::PathExists,
                dst: Some(dst),
                args: vec![path],
            });
            return Ok(Some(dst));
        }
        // `prim.read_line()` — the WASI stdin-line floor (io.read_line). NO args. Its dst is a
        // FRESH OWNED canonical `String` (one line of stdin, newline excluded) built by the render
        // ($read_line). A plain String owns NO nested handles, so it is tracked in NO classification
        // set — its scope-end drop (if not moved out as a return) is the flat `Op::Drop` that frees
        // the block (a `DropListStr` would WRONGLY treat the byte payload as i64 element handles).
        // Carries Capability::Stdin (counted in cap_witness). The render emits the byte-by-byte
        // fd_read-from-fd-0 sequence.
        if func == "read_line" {
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::ReadLine,
                dst: Some(dst),
                args: vec![],
            });
            return Ok(Some(dst));
        }
        // `prim.read_n_bytes(n)` — the WASI stdin-N-bytes floor (io.read_n_bytes). The n arg is a
        // scalar Int (byte count); dst is a FRESH OWNED `Bytes` block (byte-buffer layout, built by the
        // preamble `$read_n_bytes`). Carries Capability::Stdin (counted via certificate.rs). Like
        // read_line, a plain Bytes owns no nested handles, so its scope-end drop is the flat `Op::Drop`.
        if func == "read_n_bytes" {
            let n = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.read_n_bytes needs a scalar Int byte count not in this brick".into(),
                )
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::ReadNBytes,
                dst: Some(dst),
                args: vec![n],
            });
            return Ok(Some(dst));
        }
        // Bitwise binary ops lower to a scalar `Op::IntBinOp` (i64 and/or/xor/shl/shr_s),
        // not an `Op::Prim` — the int.band/bor/bxor/bshl/bshr floor. No ownership.
        let bitop = match func {
            "band" => Some(crate::IntOp::And),
            "bor" => Some(crate::IntOp::Or),
            "bxor" => Some(crate::IntOp::Xor),
            "bshl" => Some(crate::IntOp::Shl),
            "bshr" => Some(crate::IntOp::Shr),
            "bshr_u" => Some(crate::IntOp::ShrU),
            _ => None,
        };
        if let Some(op) = bitop {
            let a = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} arg 0 is not a lowerable scalar"))
            })?;
            let b = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} arg 1 is not a lowerable scalar"))
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op, a, b });
            return Ok(Some(dst));
        }
        let kind = match func {
            "handle" => PrimKind::Handle,
            "die" => PrimKind::Die,
            "load8" => PrimKind::Load { width: 1 },
            "load32" => PrimKind::Load { width: 4 },
            "load64" => PrimKind::Load { width: 8 },
            // Load a 4-byte handle KEEPING Ptr repr — reads a String element out of a list slot
            // (a borrow of the slot's String, for passing to a closure / String fn).
            "load_str" => PrimKind::LoadHandle,
            // Generic typed `load_handle[A]` — the same i32-handle-keeping load as `load_str`, for
            // reading a `List[Value]`/`Value` payload out of a Value's slot (the Value model floor).
            "load_handle" => PrimKind::LoadHandle,
            "store32" => PrimKind::Store { width: 4 },
            "store8" => PrimKind::Store { width: 1 },
            "store64" => PrimKind::Store { width: 8 },
            // Raw refcount free/acquire — the Value drop/copy mechanism. GATED to the value-model
            // self-host fns (the trusted recursive-free / shallow-copy, like the inline DropListStr):
            // an UNTRACKED free exposed to arbitrary code would let any fn double-free outside the
            // ownership cert's sight, so only the value-model drop/copy routines may name it: the
            // recursive drop (`__drop_value`, rc_dec), the array shallow-copy (`__varr_copy`, rc_inc),
            // the as_array element-list fill (`__vfill`, rc_inc), and the heap-element list-concat copy
            // (`__lc_copy_rc`, rc_inc — the new list co-owns each appended element, balanced by the
            // source's recursive DropListStr/DropListValue). See docs/roadmap/active/v1-value-model.md.
            //
            // TRUST GROUNDING (柱C Brick 3): these names are a CO-OWN-PRODUCER / RECURSIVE-DROP whitelist
            // — a producer (`__varr_copy`/`__vobj_fill`/`__copy_value`/`__lc_copy_rc`/…) rc_inc's each
            // loaded element (+1) into a fresh container; its balancing rc_dec lives in the SEPARATE
            // recursive drop (`__drop_value`/`__vdrop_arr`/…) over the SAME elements. That cross-loop,
            // element-count-keyed balance is PROVEN leak/double-free-free on the Coq kernel by
            // proofs/CoownLoop.v (`coown_fill_drop_neutral` ⇒ `coown_copy_no_leak` + `…no_double_free`,
            // in the check.sh proof gate). So this gate is no longer bare trust: a name belongs here iff
            // it is a co-own producer or recursive-drop consumer following that proven pattern, and its
            // adherence is ratcheted by the spec/wasm_cross/*_leak_loop fixtures. Cert-PROVING each
            // producer per-function (retiring the whitelist) needs the typed nested-element model + the
            // cross-function fill↔drop pairing that consumes CoownLoop.v — the remaining Brick-3
            // engineering (docs/roadmap/active/value-rc-cert.md).
            // The co-own producer / recursive-drop whitelist lives in ONE shared anchor
            // (crate::coown_names) grounded in proofs/CoownLoop.v + CoownCompose.v — see that module.
            "rc_dec" | "rc_inc"
                if crate::coown_names::is_coown_rc_routine(self.fn_name.as_str())
                    || self.fn_name.starts_with("__drop_")
                    // `__krec_uniqfill_<R>` — the GENERATED list.unique fill over a
                    // String-field-record element (C-015): rc_inc each KEPT element
                    // into the result (the __uh_acquire pattern, per-type generated
                    // like __drop_*; drop_sources.rs is the single emitter).
                    || self.fn_name.starts_with("__krec_uniqfill_") =>
            {
                // `__drop_*` also covers the GENERATED per-type custom-variant recursive drops
                // (`__drop_Expr`, ADT brick 5b) — the same trusted prim-only free routine.
                if func == "rc_dec" { PrimKind::RcDec } else { PrimKind::RcInc }
            }
            "rc_dec" | "rc_inc" => {
                return Err(LowerError::Unsupported(format!(
                    "prim.{func} is restricted to the value-model drop/copy routines (untracked free)"
                )))
            }
            "fd_write" => PrimKind::FdWrite,
            "random_get" => PrimKind::RandomGet,
            "clock_time_get" => PrimKind::ClockTimeGet,
            // The FLOAT floor (the f64 bits live in the i64-uniform value; render reinterprets).
            "fabs" => PrimKind::FloatUn(crate::FUnOp::Abs),
            "fsqrt" => PrimKind::FloatUn(crate::FUnOp::Sqrt),
            "ffloor" => PrimKind::FloatUn(crate::FUnOp::Floor),
            "fceil" => PrimKind::FloatUn(crate::FUnOp::Ceil),
            "fneg" => PrimKind::FloatUn(crate::FUnOp::Neg),
            "fadd" => PrimKind::FloatBin(crate::FBinOp::Add),
            "fsub" => PrimKind::FloatBin(crate::FBinOp::Sub),
            "fmul" => PrimKind::FloatBin(crate::FBinOp::Mul),
            "fdiv" => PrimKind::FloatBin(crate::FBinOp::Div),
            "fmin" => PrimKind::FloatBin(crate::FBinOp::Min),
            "fmax" => PrimKind::FloatBin(crate::FBinOp::Max),
            "fcopysign" => PrimKind::FloatBin(crate::FBinOp::CopySign),
            "flt" => PrimKind::FloatCmp(crate::FCmpOp::Lt),
            "fle" => PrimKind::FloatCmp(crate::FCmpOp::Le),
            "fgt" => PrimKind::FloatCmp(crate::FCmpOp::Gt),
            "fge" => PrimKind::FloatCmp(crate::FCmpOp::Ge),
            "feq" => PrimKind::FloatCmp(crate::FCmpOp::Eq),
            "fne" => PrimKind::FloatCmp(crate::FCmpOp::Ne),
            "f2i" => PrimKind::FloatToInt,
            "i2f" => PrimKind::IntToFloat,
            "fbits" | "ffrombits" => PrimKind::FloatBits,
            // f32 narrowing/widening (f32 value = its 32-bit pattern in the low half of the i64).
            "f2f32" => PrimKind::F32Demote,
            // `f32_2f` (Float32→Float) and `bits_to_f32` (raw 32-bit pattern→Float) are the SAME
            // f64.promote_f32 over a low-32 f32 pattern.
            "f32_2f" | "bits_to_f32" => PrimKind::F32Promote,
            "i2f32" => PrimKind::IntToF32,
            "f32bits" => PrimKind::F32Bits,
            _ => return Err(LowerError::Unsupported(format!("unknown primitive prim.{func}"))),
        };
        let mut lowered = Vec::with_capacity(args.len());
        for a in args {
            // A STRING-LITERAL argument to `prim.handle` — the frontend's single-use
            // let-inliner pushes `let tbl = "…"; prim.handle(tbl)` into
            // `prim.handle("…")` (the generated case-mapping tables). Materialize
            // the literal block exactly as its let-bound form would (owned Alloc,
            // scope-end drop) and hand the prim its handle — the scalar-tail
            // deferred-Const fallback was silently returning 0 as the address.
            if matches!(kind, PrimKind::Handle) {
                if let IrExprKind::LitStr { value } = &a.kind {
                    let dst = self.fresh_value();
                    self.ops.push(Op::Alloc {
                        dst,
                        repr: repr_of(&a.ty)?,
                        init: crate::Init::Str(value.clone()),
                    });
                    self.live_heap_handles.push(dst);
                    lowered.push(dst);
                    continue;
                }
                // A COMPUTED String argument (`prim.die(prim.handle("assertion failed: "
                // + msg))` — the 2-arg assert's computed-message die): materialize the
                // concat/interp chain to an owned block (scope-tracked, dropped at the
                // arm/scope end like the literal above) and hand the prim its handle.
                // Without this the whole assert's unit-if rolled back and the wall
                // (misleadingly) named the CONDITION.
                if matches!(
                    &a.kind,
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. }
                        | IrExprKind::StringInterp { .. }
                ) {
                    let obj = match &a.kind {
                        IrExprKind::BinOp { .. } => self.try_lower_concat_str(a),
                        IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
                        _ => unreachable!(),
                    };
                    if let Some(obj) = obj {
                        self.live_heap_handles.push(obj);
                        lowered.push(obj);
                        continue;
                    }
                }
            }
            let v = self.lower_scalar_value(a).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
            })?;
            lowered.push(v);
        }
        let dst = if matches!(kind, PrimKind::Store { .. } | PrimKind::RcDec | PrimKind::RcInc | PrimKind::Die | PrimKind::ProcExit) {
            None
        } else {
            Some(self.fresh_value())
        };
        // `prim.load_str` (LoadHandle) yields a BORROW of a list slot's String — the list still owns
        // it. Mark the result BORROWED so a `let` binding does not add it to the scope-end drop set
        // (that would double-free with the owning list's DropListStr).
        if matches!(kind, PrimKind::LoadHandle) {
            if let Some(d) = dst {
                self.param_values.insert(d);
            }
        }
        self.ops.push(Op::Prim { kind, dst, args: lowered });
        Ok(dst)
    }

    /// Register a freshly-materialized call-result temp used as a call argument: a
    /// HEAP temp is BORROWED into the call (`Handle`) and added to the scope-end
    /// drop set (it is owned by THIS scope, not moved out, so it is released after
    /// the call returns); a scalar temp is passed by value. A NESTED-OWNERSHIP temp
    /// (a `List[String]` from `set.from_list(string.split(…))`, etc.) is ALSO recorded
    /// in `heap_elem_lists` so its scope-end drop is the recursive `DropListStr` that
    /// frees the owned element Strings — a flat `Drop` would free only the block and
    /// LEAK the elements (per-iteration in a loop → OOM). Cert is unchanged: one `i`
    /// (alloc) + one `d` (drop) for the temp; DropListStr vs Drop is the runtime
    /// realization of that same single `d`.
    pub(crate) fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr, ty: &Ty) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            // A `value.as_array(v) ?? []` arg temp (the materialized `??` operand) is a
            // Result[List[Value],String] that OWNS its inner list — its drop must free the list AND
            // its element Values RECURSIVELY (`DropResultListValue`); the flat `heap_elem_lists`
            // fallback would only rc_dec the inner-list handle, LEAKING the element Values (a loop
            // OOMs). Checked BEFORE is_heap_elem_list_ty, which also matches this Result type.
            // (A Result[Value,String]'s Ok Value is CO-OWNED — value.get Dup's the object's slot, which
            // keeps its ref — so the flat rc_dec drop is correct there; a recursive free would
            // double-free the still-referenced slot. So only the list case is reclassified here.)
            if crate::lower::is_result_listval_ty(ty) {
                self.value_result_lists.insert(dst);
            } else if crate::lower::is_list_list_str_ty(ty) {
                self.list_list_str_lists.insert(dst);
            } else if crate::lower::is_list_str_str_ty(ty) {
                // `List[(String,String)]` (map.entries) arg temp — DropListStrStr frees each tuple's
                // two Strings; the flat heap_elem_lists fallback would leak them.
                self.str_str_elem_lists.insert(dst);
            } else if crate::lower::is_lenlist_list_ty(ty) {
                self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
            } else if crate::lower::is_map_fn_ty(ty) {
                // `Map[String, <Fn>]` arg temp — `$__drop_map_mclo` frees each value via
                // `__drop_closure` (a flat sweep would leak every captured env slot).
                self.variant_drop_handles.insert(dst, "map_mclo".to_string());
            } else if let Some(hname) = self.map_named_value_drop(ty) {
                self.variant_drop_handles.insert(dst, hname);
            } else if crate::lower::is_map_msv_ty(ty) {
                // `Map[String, Map[String, String]]` arg temp (the inline nested-map literal
                // fed straight to `map.get_or` — map_fold_heap_acc's r7): `$__drop_map_msv`
                // sweeps each last-ref inner map; the flat fallback leaked the whole nested
                // map per iteration (loop OOM).
                self.variant_drop_handles.insert(dst, "map_msv".to_string());
            } else if crate::lower::is_map_mlo_ty(ty) {
                // `Map[String, List[Option[Int]]]` arg temp — `$__drop_map_mlo` (the
                // bind-site route, mirrored; the flat fallback would leak the value lists).
                self.variant_drop_handles.insert(dst, "map_mlo".to_string());
            } else if let Some(rname) = (match ty {
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                    if a.len() == 1 =>
                {
                    self.record_or_anon_drop_type_name(&a[0])
                }
                _ => None,
            }) {
                // A `List[<recursive-drop record>]` arg temp — `$__drop_list_<R>` (the
                // bind-site route, mirrored; the flat fallback leaked each element's
                // String fields — the krec-unique residue).
                self.variant_drop_handles.insert(dst, format!("list_{rname}"));
            } else if matches!(ty,
                Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                    if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
            {
                // `Map[String, <scalar>]` arg temp — the key-slot sweep (split layout, @4 = n),
                // mirroring the bind-site fix; the flat fallback leaked every key copy.
                self.heap_elem_lists.insert(dst);
            } else if crate::lower::is_heap_elem_list_ty(ty) {
                self.heap_elem_lists.insert(dst);
            }
            // A `Value` call-argument temp (`f(value.array([…]))`, `f(value.str(s))`) drops via the
            // runtime-tag-dispatched `Op::DropValue` (recursive — an Array frees its element Values, a
            // Str its String), NOT a flat `Op::Drop` (which would leak the nested payload). Without
            // this a tag-5 Array / tag-4 Str passed as an argument leaks at the call-site scope end.
            if crate::lower::is_value_ty(ty) {
                self.value_handles.insert(dst);
            }
            // A RECORD/TUPLE call-argument temp (`f(mk(x))` — a fresh record passed by handle) drops at
            // the call-site scope end. Without a mask it falls to a flat `Op::Drop` (rc_dec the record
            // block only), LEAKING every heap field (the `f(mk(x))`-in-a-loop OOM). Seed its heap-slot
            // `record_masks` (the masked drop frees the leaf fields) and, when a field is a
            // Map/List[heap]/record/Value, route to the recursive `$__drop_<R>` via variant_drop_handles.
            if let Some((_, tys)) = self.aggregate_field_tys(ty) {
                let heap_slots: Vec<usize> =
                    (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                self.record_masks.insert(dst, heap_slots);
                if let Some(name) = self.record_drop_type_name(ty) {
                    self.variant_drop_handles.insert(dst, name);
                }
            }
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }
}
