impl LowerCtx {
    fn lower_scalar_value_inner(&mut self, expr: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        match &expr.kind {
            IrExprKind::Var { id } => self.value_or_global(*id).ok(),
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
            // A FLOAT literal: the i64-uniform value holds the f64 BITS, so `3.5` materializes
            // as `ConstInt(3.5_f64.to_bits())`. The render's float prims reinterpret it back.
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: value.to_bits() as i64 });
                Some(dst)
            }
            IrExprKind::BinOp { op, left, right } => {
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
                // FLOAT arithmetic + comparison operators → the prim float floor (Op::Prim). The
                // operands are Float (the i64-uniform f64 bits); the prim reinterprets around the
                // wasm f64 op. Pure scalar — no ownership (cert untouched). This makes float-heavy
                // self-host (libm / dtoa) write `a * b` instead of `prim.fmul(a, b)`.
                let fkind = match op {
                    BinOp::AddFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Add)),
                    BinOp::SubFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Sub)),
                    BinOp::MulFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Mul)),
                    BinOp::DivFloat => Some(crate::PrimKind::FloatBin(crate::FBinOp::Div)),
                    BinOp::Lt if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Lt)),
                    BinOp::Lte if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Le)),
                    BinOp::Gt if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Gt)),
                    BinOp::Gte if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ge)),
                    BinOp::Eq if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Eq)),
                    BinOp::Neq if matches!(left.ty, Ty::Float) => Some(crate::PrimKind::FloatCmp(crate::FCmpOp::Ne)),
                    _ => None,
                };
                if let Some(kind) = fkind {
                    let a = self.lower_scalar_value(left)?;
                    let b = self.lower_scalar_value(right)?;
                    let dst = self.fresh_value();
                    self.ops.push(Op::Prim { kind, dst: Some(dst), args: vec![a, b] });
                    return Some(dst);
                }
                // STRING equality (`c == ":"` / `a != b` over String) → the self-host
                // `string.eq` byte-compare call (→ scalar Bool). Both operands are BORROWED
                // heap String handles (the call reads + copies; no ownership event). `!=` is
                // `1 - eq`. This is the dominant real-parser condition; without it the cond
                // silently lowered to 0 (false) — the yaml/char-scan miscompile.
                if matches!(op, BinOp::Eq | BinOp::Neq) && matches!(left.ty, Ty::String) {
                    let args = [(**left).clone(), (**right).clone()];
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
                    let args = [(**left).clone(), (**right).clone()];
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
                // `list_a == list_b` over a List[Int|String|Value]: a deep element-wise compare call
                // (→ scalar Bool). Same both-arms-linearization fix as Value/String ==. element type
                // picks the variant; other element types stay unhandled (the if then walls, loud).
                if matches!(op, BinOp::Eq | BinOp::Neq) {
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, es) =
                        &left.ty
                    {
                        let variant = if es.len() != 1 {
                            None
                        } else if matches!(es[0], Ty::Int) {
                            Some("eq_int")
                        } else if matches!(es[0], Ty::String) {
                            Some("eq_str")
                        } else if crate::lower::is_value_ty(&es[0]) {
                            Some("eq_value")
                        } else {
                            None
                        };
                        if let Some(v) = variant {
                            let args = [(**left).clone(), (**right).clone()];
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
                let iop = match op {
                    BinOp::AddInt => crate::IntOp::Add,
                    BinOp::SubInt => crate::IntOp::Sub,
                    BinOp::MulInt => crate::IntOp::Mul,
                    BinOp::DivInt => crate::IntOp::Div,
                    BinOp::ModInt => crate::IntOp::Mod,
                    // Ordering comparisons (the `if` condition) — INT operands only (a
                    // Float compare uses the prim float floor above; a String compare needs
                    // a different op). Gate on the operand type.
                    BinOp::Lt if matches!(left.ty, Ty::Int) => crate::IntOp::Lt,
                    BinOp::Lte if matches!(left.ty, Ty::Int) => crate::IntOp::Le,
                    BinOp::Gt if matches!(left.ty, Ty::Int) => crate::IntOp::Gt,
                    BinOp::Gte if matches!(left.ty, Ty::Int) => crate::IntOp::Ge,
                    // Equality — INT or BOOL operands. A `Bool` is an i64 0/1 (a Var loads
                    // its 0/1, a `LitBool` materializes `ConstInt 0/1` above), so the SAME
                    // `IntOp::Eq`/`Ne` render is bit-exact for `b == false` / `b1 != b2` as
                    // for `n == 0`. (Ordering on Bool is undefined in v0, so it is NOT
                    // admitted; a Float/String/compound `==` still needs a distinct op.)
                    BinOp::Eq if matches!(left.ty, Ty::Int | Ty::Bool) => crate::IntOp::Eq,
                    BinOp::Neq if matches!(left.ty, Ty::Int | Ty::Bool) => crate::IntOp::Ne,
                    // Logical `and`/`or` on Bool operands → EAGER `i64.and`/`i64.or` of the
                    // two lowered Bools (each an i64 0/1: a `LitBool` materializes ConstInt
                    // 0/1, a Var loads its 0/1, a nested compare yields 0/1). This is
                    // BIT-EXACT with v0, which itself evaluates BOTH operands unconditionally
                    // (`emit(left); emit(right); i32.and/i32.or` — NO short-circuit) — so
                    // eager `and`/`or` is the faithful transcription, not an approximation.
                    // 0/1 ∧ 0/1 (resp. ∨) stays in {0,1}, so the result is a valid Bool the
                    // `if` condition / `to_string` reads uniformly. The SOUNDNESS subtlety
                    // (v0 is eager so there is no observable to short-circuit) is moot for a
                    // pure operand; a SIDE-EFFECTING operand (a printing call) would still be
                    // executed once by v0's eager emit, but to keep the cert/effect reasoning
                    // simple we only admit operands that `lower_scalar_value` accepts as a
                    // pure scalar predicate below — a non-lowerable operand returns None
                    // (WALL), never both-arms / never 0.
                    BinOp::And if matches!(left.ty, Ty::Bool) => crate::IntOp::And,
                    BinOp::Or if matches!(left.ty, Ty::Bool) => crate::IntOp::Or,
                    // Pow, Float, concat, non-Int/Bool compares: defer.
                    _ => return None,
                };
                // `and`/`or` admit only PURE operands. v0's eager emit evaluates BOTH
                // unconditionally, so an effect-free operand is bit-exact; but a
                // heap-materializing operand (`is_empty(x) and contains(y, "@")`) would
                // register an owned temp whose consume escapes the enclosing per-arm
                // frame (a dangling `m`). Gate it out → WALL to the sound prior lowering.
                // The arithmetic/comparison ops keep the plain `lower_scalar_value`: by
                // type their operands are Int/Float/Bool scalars that never materialize a
                // heap temp, so the pure-gate would be a no-op there.
                let is_logic = matches!(iop, crate::IntOp::And | crate::IntOp::Or);
                let (a, b) = if is_logic {
                    (self.lower_scalar_operand(left)?, self.lower_scalar_operand(right)?)
                } else {
                    (self.lower_scalar_value(left)?, self.lower_scalar_value(right)?)
                };
                let dst = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst, op: iop, a, b });
                Some(dst)
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
                        self.ops.push(Op::Prim {
                            kind: crate::PrimKind::FloatUn(crate::FUnOp::Neg),
                            dst: Some(dst),
                            args: vec![x],
                        });
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
            _ => None,
        }
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
        if func == "alloc_list_str" || func == "alloc_set_str" || func == "alloc_map_str" || func == "alloc_map_skv" {
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
            "rc_dec" | "rc_inc"
                if matches!(
                    self.fn_name.as_str(),
                    "__drop_value"
                        | "__drop_list_value"
                        | "__svdrop_list"
                        | "__ssdrop_list"
                        | "__isdrop_list" // List[(Int,String)] recursive free (list.enumerate)
                        | "__drop_list_str_value"
                        | "__drop_result_lv"
                        | "__varr_copy"
                        | "__vfill"
                        | "__lc_copy_rc"
                        | "__copy_slots_rc" // list.set_str: rc-copy each String element (co-own)
                        | "__set_slot_str"  // list.set_str: rc_dec the replaced element + rc_inc the new
                        | "list_set_str"    // (also reaches rc via the helpers above; admit by name)
                        | "__ldls_share" // list.take/drop_liststr sublist (rc_inc each shared inner list)
                        | "value_get"     // Object linear-scan get (rc_inc the found value)
                        | "__vobj_fill"   // Object shallow-copy (rc_inc each key/value) — value.object
                        | "__vdrop_obj"   // Object recursive free (rc_dec key, __drop_value value)
                        | "__lsv_copy"    // list.set_value: rc-copy each Value element (co-own)
                        | "__lsv_set"     // list.set_value: __drop_value the replaced + rc_inc the new
                        | "list_set_value"
                        | "__lsv_insert_fill" // list.insert_value: rc_inc the inserted Value
                        | "__ls_insert_fill"  // list.insert_str: rc_inc the inserted String
                        | "__sort_copy_rc"    // list.sort_str: rc-copy each String element (co-own)
                        | "__vmerge_fill_a" // value.merge: rc_inc each kept/overridden key+value (co-own)
                        | "__vmerge_app_b"  // value.merge: rc_inc each appended b key+value (co-own)
                ) || self.fn_name.starts_with("__drop_") =>
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
            let v = self.lower_scalar_value(a).ok_or_else(|| {
                LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
            })?;
            lowered.push(v);
        }
        let dst = if matches!(kind, PrimKind::Store { .. } | PrimKind::RcDec | PrimKind::RcInc) {
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
