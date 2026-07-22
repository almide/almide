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
}
