impl LowerCtx {
    /// The type-driven scope-end drop handle for a `Map[String, <Named>]` value (the
    /// desugared map literal's `from_list_hobj` result, split layout): a VARIANT value
    /// routes to the generated `$__drop_map_<V>` (key rc_dec + flat/recursive value free,
    /// generated for EVERY variant); a SCALAR-ONLY record to `$__drop_map_rec_<R>`
    /// (both slots flat rc_dec). A heap-field RECORD value returns `None` — no generated
    /// sweep exists, so the bind keeps the honest deferral/wall (never a leaky flat link).
    pub(crate) fn map_named_value_drop(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::Map, a) = ty else { return None };
        if a.len() != 2 || !matches!(a[0], Ty::String) {
            return None;
        }
        let Ty::Named(n, _) = &a[1] else { return None };
        let ns = n.as_str();
        if self.variant_layouts.by_type.contains_key(ns) {
            return Some(format!("map_{}", crate::lower::drop_fn_ident(ns)));
        }
        if crate::lower::canonical_record_key(&self.record_layouts, ns).is_some()
            && self
                .aggregate_field_tys(&a[1])
                .is_some_and(|(_, tys)| tys.iter().all(|t| !is_heap_ty(t)))
        {
            return Some(format!("map_rec_{}", crate::lower::drop_fn_ident(ns)));
        }
        None
    }

    /// `List[(<scalar>, <RICH variant V>)]` — the `list.enumerate_h` result
    /// (#1496). Slot0 @12 is scalar, slot1 @20 is a variant owning further
    /// heap, so the exact free is the generated `$__drop_list_int_<V>` —
    /// `DropListStr`'s flat sweep would free the tuple blocks and leak every
    /// element's tree. Mirrors `map_named_value_drop` above: admission ⊆
    /// generation, because `is_rich_variant_ty` asks the same question the
    /// drop generator's filter does; anything it cannot name returns `None`
    /// and the caller keeps the honest deferral/wall.
    pub(crate) fn list_int_variant_drop(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let Ty::Applied(TypeConstructorId::List, a) = ty else { return None };
        let [Ty::Tuple(tys)] = &a[..] else { return None };
        let [k, v] = &tys[..] else { return None };
        if is_heap_ty(k) {
            return None;
        }
        let vn = self.variant_layouts.is_rich_variant_ty(v, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        })?;
        Some(format!("list_int_{}", crate::lower::drop_fn_ident(&vn)))
    }

    pub(crate) fn lower_bind(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // `let r = e!` (Unwrap — effect-fn error propagation) bound to a let/var was a deferred
        // `Const`/`Alloc{Opaque}` = a SILENT MISCOMPILE (`int.parse(s)!` bound 0, `g()!` empty).
        // The early-return brick HAS now arrived (`Op::Return`, R2): under the probe the ONE
        // rule lowers it linearly; outside the admitted shapes (and always with the probe off,
        // where the position desugars still run first) WALL it — NEVER bind a silently-wrong
        // value (the ② cardinal rule). Both scalar + heap paths.
        if matches!(&value.kind, IrExprKind::Unwrap { .. }) {
            if self.try_lower_bind_unwrap_return(var, ty, value)? {
                return Ok(());
            }
            return Err(LowerError::Unsupported(
                "unwrap `!` bound to a let/var cannot be faithfully computed (needs early-return \
                 propagation; a Const/Opaque would be a silently wrong value) not in this brick"
                    .into(),
            ));
        }
        // A `Try` bind (the auto-`?` node the codec DERIVE decoders synthesize
        // per `?`-bound field — user auto-? was removed by ADR-0008/E041) has
        // the SAME propagation semantics as `!`. Under the probe the position
        // desugars that used to restructure it are off, so route it through
        // the one rule; a decline falls through to the existing chain (the
        // nested-arm reach wall stays the honest fallback). Probe-off is
        // untouched — the desugar ladder still owns the node.
        if crate::lower::bang_return_probe() && matches!(&value.kind, IrExprKind::Try { .. }) {
            if self.try_lower_bind_unwrap_return(var, ty, value)? {
                return Ok(());
            }
        }
        // A BLOCK-valued bind (`let a = { let n = 5; n * n }` — an inlined pipe-lambda, or any block
        // in value position): lower the block's statements as effects in the current scope, then bind
        // `var` to the block's TAIL by recursing. Without this the Block falls through to the scalar
        // path's deferred `Const` and mis-lowers to 0. A block-local `let` extends to the outer scope
        // — a conservative, memory-safe lifetime extension (the same discipline as a deferred reassign).
        if let IrExprKind::Block { stmts, expr: Some(tail) } = &value.kind {
            for stmt in stmts {
                self.lower_stmt(stmt)?;
            }
            return self.lower_bind(var, ty, tail);
        }
        // A SHARED-CELL var (captured by a lambda AND mutated — cells.rs): bind it
        // into a 1-slot heap cell instead of a plain local, so the closure and the
        // enclosing scope share storage. Only the admitted inner classes take a cell;
        // an unadmitted class binds normally and `lift_lambda`'s mutated-capture
        // gate refuses the lift — an honest wall, never the value-copy miscompile.
        if self.cell_vars.contains(&var) {
            if let Some(class) = self.cell_class_of_ctx(ty) {
                return self.lower_cell_bind(var, ty, value, class);
            }
        }
        // Decomposed (#781, cog 272): the SCALAR path and the HEAP path are
        // verbatim text moves into `lower_bind_scalar` / `lower_bind_heap` —
        // behavior proven by the classify wall-list byte-identity ladder.
        if !is_heap_ty(ty) {
            return self.lower_bind_scalar(var, ty, value);
        }
        self.lower_bind_heap(var, ty, value)
    }

    /// THE ONE `!` RULE (return-op-eradication R2, law 6): `let x = f()!` in a
    /// declared-Result fn lowers to a linear err-check + frame exit + payload
    /// bind — no continuation nesting, no tail duplication, no loop flag:
    ///
    ///   v = lower(f())                      // the callee's Result carrier
    ///   tag = load32(handle(v) + 4)         // scalar family: len-as-tag
    ///   IfThen(tag) {                       // err ⇒ this path EXITS the frame
    ///     drops(live_heap_handles ∖ {v})    // derived from the live walk
    ///     Return(v)                         // the carrier IS the fn's err value
    ///   } EndIf
    ///   x = load64(handle(v) + 12)          // straight-line ok-payload bind
    ///
    /// Increment 1 admits: probe ON, declared-Result fn, SCALAR family on BOTH
    /// sides (the err layout is family-uniform, so the callee's carrier is a
    /// valid fn-ret value with NO rebox), scalar/Unit payload, matching err
    /// types. Everything else declines to the wall (the R2 decline matrix).
    /// The callee lowers through the ordinary `lower_bind` machinery (full
    /// tracking/seeding) under a [`Self::speculate`] snapshot, so a decline
    /// leaves no half-emitted ops.
    pub(crate) fn try_lower_bind_unwrap_return(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        use crate::PrimKind;
        if !crate::lower::bang_return_probe() {
            return Ok(false);
        }
        let dbg = std::env::var_os("ALMIDE_DBG_BANG").is_some();
        macro_rules! decline {
            ($gate:expr) => {{
                if dbg {
                    eprintln!("BANG-DECLINE {} :: {}", $gate, self.fn_name);
                }
                return Ok(false);
            }};
        }
        let (IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }) = &value.kind else {
            return Ok(false);
        };
        // Decomposed (codopsy A, cog 78 → the frame below): the admission
        // analysis — every decline gate up to the payload class — is a pure
        // read of the ctx, moved verbatim to `bang_return_admission`
        // (binds_p2_bang.rs). It NAMES the gate it would have taken and THIS
        // frame prints it, so the ALMIDE_DBG_BANG trace is byte-identical.
        let adm = match self.bang_return_admission(ty, expr, dbg) {
            Ok(adm) => adm,
            Err(gate) => decline!(gate),
        };
        let BangAdmission { void_fn, callee_is_option, callee_fam, rebox, .. } = adm;
        // The err components must agree — the pass-through would type-pun a
        // mismatched err payload (the collect_map! class; v0 map_err-coerces).
        if !callee_is_option && self.unwrap_tail_err_mismatch(expr) {
            decline!("err-mismatch");
        }
        // Lower the callee through the FULL existing bind machinery onto a
        // synthetic var, under a speculation snapshot: a decline (or a carrier
        // the rule's ownership story cannot hold — it needs an OWNED live
        // handle: the err path moves it out, the ok path leaves it to the
        // scope-end drop) rolls back with no half-emitted ops.
        let attempt = self.speculate(|ctx| {
            let tmp = VarId(crate::lower::desugar_var_seed());
            ctx.lower_bind(tmp, &expr.ty, expr).ok()?;
            let v = *ctx.value_of.get(&tmp)?;
            if !ctx.live_heap_handles.contains(&v) {
                return None;
            }
            Some(v)
        });
        let Some(v) = attempt else { decline!("callee-lowering") };
        if dbg {
            eprintln!(
                "BANG-FIRE {} :: callee_ty={:?} fam={:?} opt={} rebox={} void={}",
                self.fn_name, expr.ty, callee_fam, callee_is_option, rebox, void_fn
            );
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![v] });
        // The err tag: Scalar family reads len-as-tag @4; HeapOk reads the
        // dedicated tag slot @16 (len is pinned to 1 there).
        // The void abort's message pieces are allocated BEFORE the branch (the
        // overflow-abort precedent: pre-branch alloc, in-arm die, continue
        // path frees at scope end — no ownership event inside an arm). An
        // OPTION none has no payload, so its line is fully static; a Result
        // err's line is `"Error: " + <msg @12> + "\n"` (build_main_die_line's
        // exact spelling), concatenated IN the arm with unreachable balancing
        // drops after the die (the arm machinery's own shape).
        let void_msg_pieces =
            if void_fn { Some(self.bang_void_msg_pieces(callee_is_option)) } else { None };
        let tag_off = if callee_is_option {
            4
        } else {
            match callee_fam {
                crate::lower::ResultFamily::Scalar => 4,
                crate::lower::ResultFamily::HeapOk => 16,
            }
        };
        let tag = self.load_at_offset(h, tag_off, PrimKind::Load { width: 4 });
        // Result: nonzero tag = err (the exit). Option: len@4 == 0 = none —
        // invert to an eq-0 scalar so the SAME IfThen(exit) frame serves both.
        let exit_cond = if callee_is_option {
            let zero = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: zero, value: 0 });
            let is_none = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: is_none, op: crate::IntOp::Eq, a: tag, b: zero });
            is_none
        } else {
            tag
        };
        self.ops.push(Op::IfThen { cond: exit_cond, dst: None });
        self.emit_bang_exit_arm(h, v, &adm, void_msg_pieces);
        self.ops.push(Op::EndIf { val: None });
        self.bind_bang_ok_payload(var, ty, h, &adm);
        Ok(true)
    }

    /// The SCALAR half of [`Self::lower_bind`] (`!is_heap_ty(ty)`): Copy values,
    /// no ownership accounting — executable scalar calls / literals / arithmetic /
    /// if- and match-values / `??` / var copies, else the deferred `Const`
    /// (strict mode walls it). Verbatim text move.
    fn lower_bind_scalar(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> Result<(), LowerError> {
        // Scalar binding: a Copy value, no ownership accounting. A RESOLVABLE
        // scalar call (`let n = add(2, 3)`, `let m = string.len(s)`) is lowered to
        // a real executable `CallFn` (args materialized, the scalar result bound)
        // so it RUNS. Any other scalar value — arithmetic, a literal, an
        // unresolvable Method/Computed call — keeps the deferred `Const` + elided-
        // caps marker: its CONTENT is carried by a later brick, its calls still
        // folded for capabilities (`var n = obj.m()` elided ⇒ honest caps taint).
        if let Some(dst) = self.try_lower_scalar_call(value, ty) {
            self.value_of.insert(var, dst);
            return Ok(());
        }
        // A UNIT-typed call bind (`let $h = println(x)` — the ANF temp the
        // ctor-net mints for `ok(println(x))`, the guard-restructure shape of
        // `guard c else err(…)` in an `effect fn -> Unit`, #1734): the bind IS
        // the statement. Run the call's effects for real, then bind the 0
        // placeholder — a Unit value is never read, exactly the Unit-Ok
        // payload discipline.
        if *ty == Ty::Unit
            && matches!(value.kind, IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. })
        {
            let ops_mark = self.ops.len();
            match self.lower_stmt_expr(value) {
                Ok(()) => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst, value: 0 });
                    self.value_of.insert(var, dst);
                    return Ok(());
                }
                Err(_) => {
                    self.ops.truncate(ops_mark);
                }
            }
        }
        if self.try_lower_bind_scalar_literal(var, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_operator_or_prim(var, value) {
            return Ok(());
        }
        // A scalar `if`/`match` VALUE (`let step = if c then 0 else 1`) EXECUTES — only
        // the taken arm runs — via the if-marker machinery; a non-literal `match` or a
        // non-scalar subject falls through to the deferred `Const`.
        if self.try_lower_bind_scalar_if_value(var, ty, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_match(var, ty, value)? {
            return Ok(());
        }
        if self.try_lower_bind_scalar_unwrap_or(var, value)? {
            return Ok(());
        }
        if self.try_lower_bind_scalar_var_alias(var, value) {
            return Ok(());
        }
        if self.try_lower_bind_scalar_projection(var, value) {
            return Ok(());
        }
        self.lower_bind_scalar_deferred_const(var, value)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an Int/Float/Bool LITERAL binds to its real materialized scalar value.
    /// `true` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_literal(&mut self, var: VarId, value: &IrExpr) -> bool {
        // An INT literal carries its real value (`ConstInt` → `(i64.const v)`),
        // the scalar-value foundation; other scalars stay the deferred `Const`. A FLOAT
        // literal carries its f64 BITS the same way (the float-floor render reinterprets).
        // A BOOL literal materializes to ConstInt 0/1 (else `var b=true` stays a deferred
        // Const 0, and `if b` / `match b { true=>.. }` always takes the false arm).
        if let IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } = &value.kind
        {
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a BinOp/UnOp/prim-floor RuntimeCall computes inside the executable scalar
    /// subset (rolling `ops` back to the entry mark when it does not). `true` means `var`
    /// is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_operator_or_prim(&mut self, var: VarId, value: &IrExpr) -> bool {
        // A scalar Int Add/Sub/Mul computes its real value (IntBinOp), and a
        // scalar prim-floor call (`let n = prim.load32(a)`) becomes an Op::Prim —
        // both via lower_scalar_value; outside the subset it rolls back to `Const`.
        // A UnOp (`let hc = not list.is_empty(xs)`, `let m = -n`) goes the SAME way —
        // without it, `not <call>` fell to the deferred `Const` below (the operand call
        // unemitted, the var silently 0 → the `not list.is_empty` render_el miscompile).
        if let IrExprKind::BinOp { .. }
        | IrExprKind::UnOp { .. }
        | IrExprKind::RuntimeCall { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an `if`-VALUE runs through the if-marker machinery. `true` means `var` is
    /// bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_if_value(&mut self, var: VarId, ty: &Ty, value: &IrExpr) -> bool {
        if let IrExprKind::If { cond, then, else_ } = &value.kind {
            if let Some(dst) = self.try_lower_scalar_if(cond, then, else_, ty) {
                self.value_of.insert(var, dst);
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// the ordered `match`-VALUE strategy chain — tuple extract, custom variant, the
    /// Option/Result variant match (which WALLS outside its subset), the single-arm
    /// multi-bind tuple destructure, then the desugared-`if` arm. `Ok(true)` means `var`
    /// is bound and the caller returns `Ok(())` immediately; `Ok(false)` falls through to
    /// the next scalar strategy. Same check ORDER, same rollbacks, same wall.
    fn try_lower_bind_scalar_match(
        &mut self,
        var: VarId,
        ty: &Ty,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        let IrExprKind::Match { subject, arms } = &value.kind else {
            return Ok(false);
        };
        if self.try_lower_bind_scalar_tuple_extract(var, subject, arms) {
            return Ok(true);
        }
        // A CUSTOM variant (user ADT) subject — tag@slot0 dispatch (ADT brick 3).
        // `let v = match t { Num(n) => n, … }`. Without this the ctor-pattern match
        // fell through to a deferred Const 0 (a silent miscompile).
        if let Some(dst) = self.try_lower_custom_variant_match(subject, arms, ty) {
            self.value_of.insert(var, dst);
            return Ok(true);
        }
        if self.try_lower_bind_scalar_variant_match(var, ty, subject, arms)? {
            return Ok(true);
        }
        if self.try_lower_bind_scalar_tuple_destructure_arm(var, ty, subject, arms) {
            return Ok(true);
        }
        if self.try_lower_bind_scalar_desugared_if_arm(var, ty, subject, arms) {
            return Ok(true);
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a single-arm tuple-destructure `match` extracting ONE scalar component
    /// loads that component's slot (rolling `ops` back on a miss). `true` means `var` is
    /// bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_tuple_extract(
        &mut self,
        var: VarId,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        // A single-arm tuple-destructure `let n = match pair { (_, n) => n }` extracting a
        // SCALAR component — semantically `let n = pair.<i>` (the non-tail tuple-accumulator
        // `fold` cursor extraction). Load the real scalar slot value (a Copy — no ownership).
        if let Some((idx, elem_ty)) = self.tuple_extract_match_index(subject, arms) {
            if !is_heap_ty(&elem_ty) {
                let synth = Self::synth_tuple_index(subject, idx, elem_ty);
                let mark = self.ops.len();
                if let Some(dst) = self.lower_scalar_value(&synth) {
                    self.value_of.insert(var, dst);
                    return true;
                }
                self.ops.truncate(mark);
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether an Option/Result-subject `match` executes as a tag-read value-match — and
    /// the WALL that a variant subject outside the executable subset takes instead of a
    /// silently-wrong Const-0. `Ok(true)` means `var` is bound and the caller returns
    /// `Ok(())` immediately; `Ok(false)` only for a NON-variant subject.
    fn try_lower_bind_scalar_variant_match(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Result<bool, LowerError> {
        // A VARIANT (Option/Result) subject — execute the tag-read value-match
        // (only the taken arm runs, the scalar payload bound). A ctor pattern is not
        // `subj == lit`, so it can't reach `desugar_match_to_if`; without this the
        // result stayed an unset deferred Const (a silent 0).
        if is_variant_ty(&subject.ty) {
            if let Some(dst) = self.try_lower_variant_value_match(subject, arms, ty) {
                self.value_of.insert(var, dst);
                return Ok(true);
            }
            // Outside the executable subset a Const-0 would silently pick a wrong
            // arm — WALL (the discipline: an unfaithful variant match rejects, never
            // emits a deferred 0).
            return Err(LowerError::shaped(
                subject.span,
                WallShape::VariantValueMatch,
                "variant (Option/Result) match bound to a let/var outside the \
                 executable subset cannot be faithfully computed (a Const-0 would \
                 silently pick a wrong arm) not in this brick",
            ));
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a single-arm tuple `match` binding MULTIPLE components lowers each from its
    /// layout slot and then the arm body (rolling `ops`/`live_heap_handles` back on a
    /// miss). `true` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_tuple_destructure_arm(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        // A single-arm tuple-destructure `let r = match t { (a, b) => <body> }` binding
        // MULTIPLE components (not the single-extract case above): bind each component from its
        // tuple SLOT (the layout-aware loader), then lower the arm body as the bound value.
        // WITHOUT this the multi-bind tuple match fell to the deferred `Const 0` below (a, b
        // read 0). SCALAR result only (a heap arm value needs the merged-result path); rolls
        // back to the Const on a miss.
        if matches!(subject.ty, Ty::Tuple(_))
            && arms.len() == 1
            && arms[0].guard.is_none()
            && matches!(&arms[0].pattern, almide_ir::IrPattern::Tuple { .. })
            && !is_heap_ty(ty)
        {
            // Guard-clause flattening (codopsy7 max-depth sweep): the original nested
            // `if let`/`if` chain is rewritten as `let-else` guards inside a labeled block —
            // any failure `break`s straight to the SAME rollback (`ops`/`live_heap_handles`
            // truncate) the original chain's implicit fall-through reached, then falls
            // through to the next match strategy below. Same check order, same rollback,
            // same success path (`return Ok(())`); pure control-flow rewrite.
            let almide_ir::IrPattern::Tuple { elements } = &arms[0].pattern else {
                unreachable!("matches! above already proved this arm's pattern is Tuple")
            };
            let mark = self.ops.len();
            let lhh = self.live_heap_handles.len();
            'single_arm_tuple: {
                // Materialize the tuple subject as a borrowed handle (its slots are real).
                let Ok(Some(CallArg::Handle(subj))) = self
                    .lower_call_args(std::slice::from_ref(subject))
                    .map(|v| v.into_iter().next())
                else {
                    break 'single_arm_tuple;
                };
                if !self.try_lower_tuple_destructure(elements, subj, Some(&subject.ty)) {
                    break 'single_arm_tuple;
                }
                let Some(dst) = self.lower_scalar_value(&arms[0].body) else {
                    break 'single_arm_tuple;
                };
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether the `match` desugars to a literal-arm `if` (or binder/guard `Block`) that
    /// `lower_scalar_arm` runs, rolling `ops`/`live_heap_handles` back on a miss. `true`
    /// means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_desugared_if_arm(
        &mut self,
        var: VarId,
        ty: &Ty,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> bool {
        if let Some(if_expr) = self.desugar_match_to_if(subject, arms, ty) {
            // `If` (literal arms) OR `Block` (`{ let x = subj; if … }` for a
            // binder/guarded arm) — `lower_scalar_arm` runs both; roll back on a miss.
            let mark = self.ops.len();
            let lhh = self.live_heap_handles.len();
            if let Some(dst) = self.lower_scalar_arm(&if_expr) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
            self.live_heap_handles.truncate(lhh);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a scalar `??` executes as a tag read + payload/fallback — and the WALL a
    /// VARIANT operand outside that subset takes instead of a silently-wrong Const-0.
    /// `Ok(true)` means `var` is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_unwrap_or(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<bool, LowerError> {
        // `let idx = string.index_of(s, x) ?? -1` — a `??` over a materialized Option
        // EXECUTES to a scalar (tag read + payload/fallback), unwrapping the self-host
        // Option[Int] fns; outside the subset a `??` over a VARIANT operand can't read
        // the tag (e.g. a USER-function Option/Result result not yet tracked as
        // materialized) — a Const-0 would be a wrong value, so WALL (never silently 0).
        if let IrExprKind::UnwrapOr { expr, fallback } = &value.kind {
            if let Some(dst) = self.try_lower_option_unwrap_or(expr, fallback, true) {
                self.value_of.insert(var, dst);
                return Ok(true);
            }
            if is_variant_ty(&expr.ty) {
                return Err(LowerError::Unsupported(
                    "?? over an Option/Result operand outside the executable subset (e.g. a \
                     user-function result not tracked as materialized) cannot be faithfully \
                     computed (a Const-0 would be a wrong value) not in this brick"
                        .into(),
                ));
            }
        }
        Ok(false)
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a bare-`Var` scalar RHS aliases its source value — and whether a MUTABLE
    /// binding instead gets its OWN local via the `+ 0` identity copy. `true` means `var`
    /// is bound and the caller returns `Ok(())` immediately.
    fn try_lower_bind_scalar_var_alias(&mut self, var: VarId, value: &IrExpr) -> bool {
        // `let v = w` aliasing a SCALAR var — v denotes the SAME value (a scalar is freely
        // duplicable: no copy, no ownership). Without this, a bare-Var scalar RHS fell to the
        // deferred `Const` below and silently became 0 (the param-alias zeroing trap).
        //
        // A SCALAR `let/var v = w` ALWAYS gets its own value, seeded with a type-agnostic
        // i64 copy (`v = w + 0` — integer-add of 0 is identity on the i64-uniform bits of
        // Int/Float/Bool). Two mirror-image corruptions forced this, one per direction:
        //   - a MUTABLE `var v = w` that aliased w's local: a later `v = …` would
        //     `SetLocal` w's slot and SILENTLY CORRUPT w (the sha1 `var a = h0; … a = temp`
        //     trap that clobbered h0);
        //   - an immutable `let v = w` where W ITSELF is loop-carried (#1322): the alias
        //     denotes w's stable local, so v reads w's POST-assignment value — the affine
        //     gcd swap `let t = y; y = x % y; x = t` degenerated to x == y (t=0 on every
        //     leg the v1 spine renders; v0/codegen-v3 was correct). "let is never
        //     reassigned" was true but aimed at the wrong side of the alias.
        // The copy anchors the READ at bind position, which is the value semantics both
        // directions need. A HEAP `let v = w` keeps the alias: heap reassignment inside
        // loops/arms is walled or deferred, never SetLocal'd in place.
        if let IrExprKind::Var { id } = &value.kind {
            if let Ok(src) = self.value_for(*id) {
                if !is_heap_ty(&value.ty) {
                    let zero = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: zero, value: 0 });
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp {
                        dst,
                        op: crate::IntOp::Add,
                        a: src,
                        b: zero,
                    });
                    self.value_of.insert(var, dst);
                } else {
                    self.value_of.insert(var, src);
                }
                return true;
            }
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// whether a SCALAR field/element/global projection loads its real slot value
    /// (rolling `ops` back on a miss). `true` means `var` is bound and the caller returns
    /// `Ok(())` immediately.
    fn try_lower_bind_scalar_projection(&mut self, var: VarId, value: &IrExpr) -> bool {
        // `let d = r.x` / `let d = t.0` / `let d = xs[i]` — a SCALAR field / element
        // projection LOADS the real value from the materialized aggregate's layout slot
        // (the VALUE MODEL); `xs[i]` is a bounds-checked `$elem_addr` load. Outside the
        // materialized subset it rolls back to the deferred `Const`.
        // A `Var` RHS reaches here only when the alias arm above MISSED (`value_for`
        // resolves locals, not globals): `let id = region_count` — a GLOBAL read. The
        // scalar-value path routes it through `value_or_global` (a mutable global's
        // slot Load / an immutable one's const materialization), a fresh dst either
        // way — no alias to protect, so the mutable-binding `+0` copy is not needed.
        if let IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. }
        | IrExprKind::Var { .. } = &value.kind
        {
            let mark = self.ops.len();
            if let Some(dst) = self.lower_scalar_value(value) {
                self.value_of.insert(var, dst);
                return true;
            }
            self.ops.truncate(mark);
        }
        false
    }

    /// Extracted verbatim from [`Self::lower_bind_scalar`] (codopsy round-3 sweep, #852):
    /// the TERMINAL deferred `Const` — the value every scalar strategy above declined.
    /// Strict value mode walls it instead; the permissive caps-counting path defers and
    /// still folds the elided calls for capabilities.
    fn lower_bind_scalar_deferred_const(
        &mut self,
        var: VarId,
        value: &IrExpr,
    ) -> Result<(), LowerError> {
        let dst = self.fresh_value();
        self.value_of.insert(var, dst);
        if crate::lower::strict_values() {
            if std::env::var("ALMIDE_BOUNDED_DEBUG").is_ok() {
                eprintln!("[bounded-debug] deferred bind var={var:?} value kind = {}",
                    match &value.kind {
                        IrExprKind::RuntimeCall { symbol, .. } => format!("RuntimeCall {}", symbol.as_str()),
                        IrExprKind::Call { .. } => "Call".to_string(),
                        other => { let d = format!("{other:?}"); d.chars().take(120).collect() },
                    });
            }
            return Err(crate::lower::strict_const_wall("binding"));
        }
        self.ops.push(Op::Const { dst });
        self.record_elided_calls(value);
        Ok(())
    }

}
