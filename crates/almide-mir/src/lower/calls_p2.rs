impl LowerCtx {
    /// Lower call arguments to [`CallArg`]s. A heap var is BORROWED (`Handle`), a
    /// scalar var is a `Scalar`, an int literal is an `Imm`. A nested CALL argument
    /// (`f(g(x))` / `f(string.trim(s))`) is MATERIALIZED: the inner call's result
    /// is computed into a fresh OWNED temp, then BORROWED into the outer call and
    /// dropped at scope end — cert `i` (call-result) + `d` (drop), both backed by
    /// real ops; the temp's capabilities are folded transitively by the corpus gate
    /// (an effectful callee taints the caller honestly). The inner call must itself
    /// be admissible: a `Named` user call, or a first-order pure stdlib `Module`
    /// call. Anything else is an explicit `Unsupported` (totality).
    /// Lower a `BinOp::ConcatStr` (string `a + b`) to a `CallFn` to the self-host `__str_concat`
    /// (auto-linked) — a FRESH owned String of byte-len(a)+byte-len(b). The operands lower as
    /// borrowed-or-materialized call args (like any heap call); the result is a fresh owned heap
    /// value the CALLER owns (a bind drops it `d`, a tail returns it `m`, an arg materializes +
    /// drops it). OWNERSHIP is the SAME proven shape as any heap-result Named/Module call
    /// (CallFn-heap-result = cert `i`). Nested `a + b + c` recurses (each ConcatStr → one call).
    /// Returns `None` (rolled back) if an operand doesn't lower. The mir↔ir gate counts each
    /// `ConcatStr` node as 1 IR call (classify_corpus.rs) so this synthetic CallFn keeps
    /// `mir_calls <= ir_calls`.
    pub(crate) fn try_lower_concat_str(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        let IrExprKind::BinOp { op: BinOp::ConcatStr, left, right } = &value.kind else {
            return None;
        };
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let arg_exprs = [(**left).clone(), (**right).clone()];
        let args = match self.lower_call_args(&arg_exprs) {
            Ok(a) => a,
            Err(_) => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "__str_concat".to_string(),
            args,
            result: Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        Some(dst)
    }

    /// Lower a `BinOp::ConcatList` (list `a + b`) over a SCALAR-element list (`List[Int/Float/Bool]`)
    /// to a `CallFn` to the self-host `__list_concat` (auto-linked) — a FRESH owned list of
    /// len(a)+len(b) i64 slots, both element ranges byte-copied. The operands lower as borrowed-or-
    /// materialized call args (like any heap call); the result is a fresh owned list the CALLER owns
    /// (a bind drops it `d`, a tail returns it `m`, an arg materializes + drops it). OWNERSHIP is the
    /// SAME proven shape as any heap-result Named/Module call (CallFn-heap-result = cert `i`), exactly
    /// like `try_lower_concat_str`. GATED to a SCALAR element type: a heap-element list (`List[String]`)
    /// has owned String handles in its slots that a copy would ALIAS (double-free on drop), so it
    /// returns `None` (deferred — never wrong bytes). Nested `a + b + c` recurses (each ConcatList →
    /// one call). The mir↔ir gate counts each `ConcatList` node as 1 IR call (classify_corpus.rs) so
    /// this synthetic CallFn keeps `mir_calls <= ir_calls`.
    pub(crate) fn try_lower_concat_list(&mut self, value: &IrExpr) -> Option<ValueId> {
        use almide_ir::BinOp;
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::BinOp { op: BinOp::ConcatList, left, right } = &value.kind else {
            return None;
        };
        let elem_ty = match &value.ty {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        // SCALAR-element (i64 slots: Int/Float/Bool) → byte-copy `__list_concat`. HEAP-element String or
        // Value (OWNED handle slots) → the rc-incrementing `__list_concat_rc` (the new list co-owns each
        // element; the source's recursive drop frees its own refs). A heap-FIELD aggregate element
        // (tuple/record with inner heap) still DEFERS — it needs the masked recursive drop (tuple-heap).
        let scalar_elem = !is_heap_ty(&elem_ty);
        let heap_elem =
            is_heap_ty(&elem_ty) && (matches!(elem_ty, Ty::String) || crate::lower::is_value_ty(&elem_ty));
        // A `(String, Value)` TUPLE element (the yaml `pairs` shape) — `__list_concat_rc` rc-owns each
        // tuple, freed recursively by `Op::DropListStrValue` (rc_dec the String slot + `$__drop_value` the
        // Value slot, per tuple). The two-heap-field aggregate `DropListStr` cannot express.
        let str_value_elem = matches!(&elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
                && crate::lower::is_value_ty(&tys[1]));
        // A `List[String]` ELEMENT (so `value` is a `List[List[String]]` — the csv `rows + [cur]`
        // shape). `__list_concat_rc` rc-incs each inner-list handle (the new outer list co-owns each
        // row); the outer's recursive `Op::DropListListStr` frees each row's cells + each row block.
        let list_str_elem = matches!(&elem_ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::String));
        // A `List[scalar]` ELEMENT (so `value` is `List[List[Int]]` — the memory_stress
        // `outer + [[i*100, …]]` nested-accumulator shape). The inner block is FLAT
        // (header + i64 slots, no inner handles), so per-element `rc_dec` IS its full
        // free — the SAME physics as a String element: `__list_concat_rc` co-owns each
        // inner-list handle and the scope-end `DropListStr` frees each + the outer.
        let flat_list_elem = matches!(&elem_ty,
            Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && !is_heap_ty(&a[0]));
        // A RECORD element (`parent.children + [child]` — the svg `add_child` shape): `__list_concat_rc`
        // rc-incs each record handle (the new list co-owns each), freed recursively by the generated
        // `$__drop_list_<R>` (each element → `$__drop_<R>`). Gated to a recursive-drop record so that fn
        // exists. An ANONYMOUS structural record element (`items + [{ x: …, content: nm, … }]` — the
        // ceangal zip_view_rects append; the checker leaves the element structural) routes to its
        // synthesized `$__drop_list_anonrec_<hash>` wrapper via the same registry.
        let record_elem = self.record_or_anon_drop_type_name(&elem_ty);
        // A `(String, String)` TUPLE element (the `map.entries` shape) — `__list_concat_rc` rc-owns each
        // tuple, freed recursively by `Op::DropListStrStr` (per tuple: rc_dec BOTH String slots). The
        // (String,String) counterpart of `str_value_elem`.
        // Widened to (String, <flat block>) — String OR List[scalar] second slot (the
        // hval map literal's `("xs", [1, 2, 3])` pairs): DropListStrStr's two per-slot
        // rc_decs are each a FULL free for any flat block (`is_list_str_str_ty`'s
        // documented physics).
        let flat_snd = |t: &Ty| {
            matches!(t, Ty::String)
                || matches!(t, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                    if b.len() == 1 && !is_heap_ty(&b[0]))
        };
        let str_str_elem = matches!(&elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && flat_snd(&tys[1]));
        // An `(Int, String)` TUPLE element (the `list.enumerate` shape) — `__list_concat_rc` rc-owns each
        // tuple, freed recursively by `$__drop_list_int_str` (per tuple: rc_dec the String slot @20 only,
        // the Int @12 is scalar). Routed via variant_drop_handles (a DropVariant, like the record case).
        let int_str_elem = matches!(&elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && matches!(tys[1], Ty::String));
        // A `(String, Int)` TUPLE element (the gguf `entries + [(key, pos)]` metadata
        // accumulator) — the MIRROR of `int_str_elem`: rc-own each tuple, recursive drop via
        // `DropListStrInt` (rc_dec the String slot @12 only; the Int @20 is scalar).
        let str_int_elem = matches!(&elem_ty,
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Int));
        // An ALL-SCALAR aggregate element (`first + [(re, im)]` — the fft Complex
        // accumulator): each element block holds only inline scalars, so the flat
        // per-slot-rc_dec `DropListStr` IS its full free (same physics as the binds-side
        // `elem_scalar_aggregate` list literal).
        let scalar_aggregate_elem = self
            .aggregate_field_tys(&elem_ty)
            .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
            .is_some();
        // A FLAT-variant ELEMENT (`acc + [r.val]` where `acc: List[ValType]`, the wasm-binary
        // recursive-accumulator shape) — each element is a single OWNED tag-block (no inner handle),
        // so `__list_concat_rc` rc-incs each element handle (the new list co-owns each block) and the
        // scope-end `DropListStr` `rc_dec`s each element + the list block (a flat variant block's
        // `rc_dec` IS its full free). Byte-identical to the proven `List[String]` cert — mirrors the
        // `elem_flat_variant` arm of the List-LITERAL builder (binds.rs). A variant carrying a
        // `String`/nested/`List` field is NOT flat (`is_flat_variant_ty` = false) and stays walled.
        let flat_variant_elem = self.variant_layouts.is_flat_variant_ty(&elem_ty);
        // A RICH (recursive-drop) variant ELEMENT (`acc + [instr_r.val]` where `instr_r.val: Instr` —
        // the wasm bytecode instruction accumulator). `__list_concat_rc` rc-incs each element handle
        // (the new list co-owns each block); the scope-end / teardown `$__drop_list_<V>` frees each
        // element RECURSIVELY via `$__drop_<V>` (a flat `rc_dec` would leak each Instr's nested
        // `List[Instr]`). Routed via `variant_drop_handles="list_<V>"`, like the record case.
        let rich_variant_elem = self.variant_layouts.is_rich_variant_ty(&elem_ty, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        });
        // A CLOSURE element (`fs + [() => …]` — the `list.push`-of-a-closure desugar over a
        // `List[() -> Unit]` var): `__list_concat_rc` rc-incs each closure-block handle (the
        // new list co-owns each element); the scope-end `$__drop_list_closure` frees each
        // element recursively via `__drop_closure` — the SAME route the List[Fn] LITERAL
        // builder registers (`ListElemDrop::Closure`), so build and concat agree on the drop.
        let closure_elem = matches!(&elem_ty, Ty::Fn { .. });
        if !scalar_elem && !heap_elem && !str_value_elem && !list_str_elem && !flat_list_elem
            && !str_str_elem && !int_str_elem && !str_int_elem && !scalar_aggregate_elem
            && !flat_variant_elem && !closure_elem
            && rich_variant_elem.is_none() && record_elem.is_none()
        {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let arg_exprs = [(**left).clone(), (**right).clone()];
        // A RECORD-element concat (`acc + [{...}]`, the wasm section-parser append): a `[{record
        // literal}]` operand infers a STRUCTURAL element type, so the generic `lower_call_args` →
        // `try_lower_record_list_literal` declines it. Lower a list-LITERAL operand via the forced
        // helper with the concat's NAMED element type (`elem_ty`) so it materializes + registers its
        // drop with the SAME declared layout the concat result uses (`list_<Named>`). Other operands
        // (the `acc` Var / a nested concat) lower generically.
        let args = if record_elem.is_some() {
            let mut out: Vec<CallArg> = Vec::with_capacity(arg_exprs.len());
            let mut ok = true;
            for a in &arg_exprs {
                if matches!(a.kind, IrExprKind::List { .. }) {
                    match self.try_lower_record_list_literal_as(a, Some(&elem_ty)) {
                        Some(d) => out.push(CallArg::Handle(d)),
                        None => { ok = false; break; }
                    }
                } else {
                    match self.lower_call_args(std::slice::from_ref(a)) {
                        Ok(mut la) => out.append(&mut la),
                        Err(_) => { ok = false; break; }
                    }
                }
            }
            if ok { Some(out) } else { None }
        } else {
            self.lower_call_args(&arg_exprs).ok()
        };
        let args = match args {
            Some(a) => a,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let dst = self.fresh_value();
        let name = if scalar_elem { "__list_concat" } else { "__list_concat_rc" };
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: name.to_string(),
            args,
            result: Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        // Mark the heap-element result for the correct RECURSIVE drop (DropListValue per `$__drop_value`
        // for Value, DropListStr per-slot rc_dec for String) so scope-end / loop teardown frees each
        // owned element — the leak-safety the cert-invisible per-element rc_inc relies on the drop for.
        if heap_elem {
            if crate::lower::is_value_ty(&elem_ty) {
                self.value_elem_lists.insert(dst);
            } else {
                self.heap_elem_lists.insert(dst);
            }
        } else if flat_variant_elem {
            // A flat-variant element block owns no inner handle, so the per-element-`rc_dec`
            // `DropListStr` (each element + the list block) IS its full free — the SAME cert as a
            // `List[String]`. (Checked AFTER `heap_elem`, before the tuple/record arms.)
            self.heap_elem_lists.insert(dst);
        } else if str_value_elem {
            self.str_value_elem_lists.insert(dst);
        } else if str_str_elem {
            self.str_str_elem_lists.insert(dst);
        } else if int_str_elem {
            self.variant_drop_handles.insert(dst, "list_int_str".to_string());
        } else if str_int_elem {
            self.variant_drop_handles.insert(dst, "list_str_int".to_string());
        } else if scalar_aggregate_elem {
            self.heap_elem_lists.insert(dst);
        } else if list_str_elem {
            self.list_list_str_lists.insert(dst);
        } else if flat_list_elem {
            // Flat inner blocks: per-slot rc_dec is each element's FULL free.
            self.heap_elem_lists.insert(dst);
        } else if closure_elem {
            // Per-element recursive free via `$__drop_list_closure` → `__drop_closure`.
            self.variant_drop_handles.insert(dst, "list_closure".to_string());
        } else if let Some(vname) = rich_variant_elem {
            // RECURSIVE per-element drop via `$__drop_list_<V>` (the generated variant list drop).
            self.variant_drop_handles.insert(dst, format!("list_{vname}"));
        } else if let Some(rname) = record_elem {
            self.variant_drop_handles.insert(dst, format!("list_{rname}"));
        }
        Some(dst)
    }

    /// Lower a STRING INTERPOLATION `"…${e}…"` to a FRESH owned String, byte-matching
    /// v0 (`emit_string_interp`), via the proven `__str_concat` self-host runtime.
    ///
    /// MODEL: the UNIFORM [`crate::lower::desugar_string_interp`] folds the K parts into
    /// a LEFT-nested `BinOp::ConcatStr` tree seeded by `""`, each part wrapped in its
    /// type's `to_string` (a Lit/String part is a no-call leaf; an Int → `int.to_string`,
    /// a Bool → `bool.to_string`, a Float/compound → `<module>.to_string`). This routine
    /// then lowers that tree through the EXISTING [`Self::try_lower_concat_str`] — the
    /// same path the `+` operator uses. Concatenating with a leading `""` is byte-
    /// identical to v0 (`"" ++ bytes == bytes`), so the rendered String matches v0 in
    /// EVERY position (bind / call-arg / tail / concat-operand / match-arm), and the
    /// caller owns the fresh result exactly like any `try_lower_concat_str` value.
    ///
    /// THE GATE-EXACTNESS INVARIANT (why this never regresses caps): the desugar admits a
    /// part ONLY when its leaf lowers to exactly one `CallFn` (a pure `module.to_string`)
    /// or a no-call passthrough, so `try_lower_concat_str` CANNOT roll back here. The
    /// corpus gate's `count_ir_calls` counts the call NODES of the SAME desugared tree,
    /// so `mir_calls == ir_calls` for the interp's contribution BY CONSTRUCTION — no
    /// `mir > ir` (forbidden), no spurious `ir > mir` taint. A part with no admitted
    /// `to_string` module (a Tuple/Record/variant) makes the desugar return `None`; the
    /// interp then stays the deferred `Alloc{Opaque}` (credited 0 by the gate), fully
    /// memory-safe. A Float/compound part DESUGARS but its `to_string` is UNLINKED, so
    /// the enclosing function emits an unlinked call and the RENDER WALL rejects it — it
    /// is out of profile and cannot be a `count != lower` mismatch.
    pub(crate) fn try_lower_string_interp(&mut self, parts: &[IrStringPart]) -> Option<ValueId> {
        // The desugar decides, per record/tuple part, EXPAND (a STATICALLY-expandable Var — a
        // materialized-aggregate binding with displayable fields → the recursive Display tree,
        // byte-matching v0) vs WRAP (any other aggregate → ONE unlinked `compound.to_string`, so
        // the function walls at render). The SAME static predicate (`aggregate_part_expandable`)
        // drives the corpus gate's `interp_synthetic_call_names`, so the synthetic call COUNT the
        // gate credits equals the one this lowering emits BY CONSTRUCTION.
        //
        // SAFETY GATE: "expandable" is a STATIC over-approximation (a `Var` need not denote a
        // materialized block — e.g. `let p = f()` is an Opaque call result). Reading its fields
        // would print garbage. So when the desugar WOULD expand a part but the var is NOT in
        // `materialized_aggregates` at lowering time, route the WHOLE interp to the compound WALL
        // — padded to the gate's synthetic-call count so `mir == ir` still holds (the extra calls
        // are pure elided markers; the one unlinked `compound.to_string` walls the function).
        if self.first_unmaterialized_expand_part(parts) {
            return Some(self.lower_interp_compound_wall(parts));
        }
        let tree = crate::lower::desugar_string_interp(parts, &self.record_layouts)?;
        self.try_lower_concat_str(&tree)
    }

    /// Is there a record/tuple part the desugar would EXPAND (statically `aggregate_part_expandable`)
    /// but whose Var is NOT actually a materialized aggregate at lowering time — so its field reads
    /// would be garbage? `false` when every would-expand part is genuinely materialized (the fold is
    /// safe). When `true`, the caller routes the whole interp to the count-padded compound wall.
    fn first_unmaterialized_expand_part(&self, parts: &[IrStringPart]) -> bool {
        parts.iter().any(|p| {
            let IrStringPart::Expr { expr } = p else { return false };
            if !crate::lower::aggregate_part_expandable(expr, &self.record_layouts) {
                return false;
            }
            let materialized = match &expr.kind {
                IrExprKind::Var { id } => self
                    .value_of
                    .get(id)
                    .is_some_and(|v| self.materialized_aggregates.contains(v)),
                _ => false,
            };
            !materialized
        })
    }

    /// Lower an interpolation whose statically-expandable record/tuple part is NOT materialized at
    /// runtime: route to ONE unlinked `compound.to_string` (the result — walls the function at
    /// render, so its bytes never run) PLUS `pad` pure elided markers so the MIR call count EQUALS
    /// the gate's `interp_synthetic_call_names` count for this interp (`mir == ir`, no false caps
    /// taint, no forbidden `mir > ir`). The markers (`__str_concat` / dotted `to_string`) reach no
    /// Stdout. The returned `dst` is tracked by the CALLER (like `try_lower_concat_str`).
    fn lower_interp_compound_wall(&mut self, parts: &[IrStringPart]) -> ValueId {
        // The gate counts this interp's synthetic calls assuming the expand happens. We emit ONE
        // real `compound.to_string` + (gate_count - 1) pure markers so the totals match exactly.
        let gate_count = crate::lower::interp_synthetic_call_names(parts, &self.record_layouts).len();
        let mut emitted = 0usize;
        let dst = self.fresh_value();
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: "compound.to_string".to_string(),
            args: Vec::new(),
            result: Some(crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        emitted += 1;
        while emitted < gate_count {
            self.ops.push(Op::CallFn {
                dst: None,
                name: "__str_concat".to_string(),
                args: Vec::new(),
                result: None,
            });
            emitted += 1;
        }
        dst
    }

    pub(crate) fn lower_call_args(&mut self, args: &[IrExpr]) -> Result<Vec<CallArg>, LowerError> {
        // Decomposed (#781, cog 184): the per-argument dispatch is a verbatim text
        // move into `lower_call_arg_into` (its early-`continue` arms become early
        // `return Ok(())`) — behavior proven by the cert byte-identity ladder.
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            self.lower_call_arg_into(a, &mut out)?;
        }
        Ok(out)
    }

    /// One call argument of [`Self::lower_call_args`] — the fresh-vs-borrowed
    /// materialization dispatch over every argument producer; multi-push shapes
    /// append to `out` directly and return early. Verbatim text move.
    fn lower_call_arg_into(
        &mut self,
        a: &IrExpr,
        out: &mut Vec<CallArg>,
    ) -> Result<(), LowerError> {
        let arg = match &a.kind {
            // A FUNCTION-typed var (`f` passed on to `__map_fill(…, f, …)`) is a SCALAR
            // table slot, NOT a borrowed heap handle — pass it by value so the callee can
            // CallIndirect through it. (Its `Ty::Fn` is_heap, so it must precede the heap
            // Var arm.) This threads a closure through nested self-host helpers.
            IrExprKind::Var { id } if matches!(a.ty, Ty::Fn { .. }) => {
                CallArg::Scalar(self.value_or_global(*id)?)
            }
            IrExprKind::Var { id } if is_heap_ty(&a.ty) => {
                let v = self.value_or_global(*id)?;
                // F2 pass-2 consumer gate (#790): a DEFERRED Opaque bind passed as a call
                // argument hands the callee an EMPTY block it reads executably (the same
                // class as the eq-operand and interp-bind holes). Strict mode REFUSES;
                // the permissive classifier keeps the borrow for call accounting.
                if crate::lower::strict_values() && self.deferred_opaque_binds.contains(&v) {
                    return Err(LowerError::Unsupported(
                        "deferred (Opaque) value passed as a call argument — the callee \
                         would read an empty block not in this brick"
                            .into(),
                    ));
                }
                CallArg::Handle(v)
            }
            IrExprKind::Var { id } => CallArg::Scalar(self.value_or_global(*id)?),
            IrExprKind::LitInt { value } => CallArg::Imm(*value),
            // A lambda ARGUMENT (`list.map(xs, (x) => x + n)`): LIFT it to a fresh
            // `__lambda_*` function and pass its CLOSURE BLOCK (a borrowed heap
            // handle — fnidx in slot 0, captures in slots 1…) — the callee invokes
            // it via `Op::CallIndirect` through its function-typed param. This is
            // the call-site half of higher-order self-host (`list.map`/`filter`/
            // `fold`), capturing lambdas included (scalar captures).
            IrExprKind::Lambda { params, body, .. } => {
                match self.lift_lambda(params, body) {
                    Some(blk) => CallArg::Handle(blk),
                    // A lambda OUTSIDE the liftable subset would materialize a
                    // deferred `Init::Opaque` (an EMPTY closure env) and pass it to
                    // the callee, which would invoke garbage = a SILENT MISCOMPILE.
                    // Reject.
                    None => {
                        return Err(LowerError::Unsupported(
                            "lambda outside the liftable subset in a call-argument \
                             position (would pass an empty deferred closure env)"
                                .into(),
                        ))
                    }
                }
            }
            // A STRING INTERPOLATION argument (`println("x=${n}")`, `f("hi ${s}")`)
            // over the executable subset — lowered to a fresh owned String via the
            // __str_concat chain, borrowed into the call and dropped at scope end
            // (cert `i` + `d`, identical to a materialized heap-literal arg). A
            // compound/call-operand interp returns None and falls through to the
            // deferred `Alloc{Opaque}` below (its inner calls recorded as elided),
            // unchanged. (This is the highest-traffic interp position — every
            // `println("…${x}…")` real program uses it.)
            IrExprKind::StringInterp { parts } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_string_interp(parts) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    // A non-lowerable interp as a call ARGUMENT would materialize a
                    // deferred `Init::Opaque` (an EMPTY String) and BORROW it into the
                    // call — the callee reads zero bytes = a SILENT MISCOMPILE. Reject
                    // explicitly so the enclosing function walls cleanly instead of
                    // emitting wrong output.
                    None => {
                        return Err(LowerError::Unsupported(
                            "non-lowerable string interpolation in a call-argument position \
                             (would borrow an empty deferred String)"
                                .into(),
                        ))
                    }
                }
            }
            // An Option/Result CONSTRUCTOR argument (`f(Some(8))`, `g(Ok(y))`,
            // `h(Err("e"))`, `k(None)`) materializes a REAL tagged block via
            // `try_lower_option_ctor` — the SAME `OptSome`/`OptNone`/DynListStr-Result
            // blocks a `let o = Some(8)` builds (len-as-tag, scalar payload moved in /
            // owned heap Err) — borrowed into the call and dropped at scope end via
            // `materialized_call_arg`: cert `i` (alloc) + `d` (drop), identical to the
            // verified fresh-heap bind. Outside that subset (a heap payload it declines,
            // e.g. a borrowed-param `Some(p)`) it WALLs — never the `Init::Opaque` empty
            // value the grouped arm below would build (which a callee reads as zero
            // bytes = a silent miscompile).
            IrExprKind::OptionSome { .. }
            | IrExprKind::OptionNone
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_option_ctor(a, &a.ty) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(format!(
                            "{} argument cannot be faithfully materialized in this brick \
                             (a heap payload outside the executable subset)",
                            kind_name(&a.kind)
                        )))
                    }
                }
            }
            // A RECORD literal argument (`f(P { x: 3, y: 4 })`) materializes the real
            // layout block via `try_lower_record_construct` (the SAME block a `let p =
            // P{..}` builds — scalar fields stored, heap fields moved in), borrowed into
            // the call and dropped at scope end via `materialized_call_arg`: cert `i`
            // (alloc) + `d` (drop), identical to the verified fresh-heap bind. Outside the
            // subset (a heap-returning-call field) it WALLs — never an `Init::Opaque` empty.
            IrExprKind::Record { .. } => {
                let repr = repr_of(&a.ty)?;
                // heap-field records via `try_lower_record_construct`; all-scalar-field
                // records (`Point { x, y }`) via `try_lower_scalar_record_construct`.
                match self
                    .try_lower_record_construct(a)
                    .or_else(|| self.try_lower_scalar_record_construct(a))
                {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "record argument cannot be faithfully materialized in this \
                             brick (a field outside the executable subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A SPREAD-record argument (`upd({ ...opts, entry: next }, …)` — the dominant
            // porta recursive-parser shape `parse_options(args, idx+2, {...opts, field: v})`):
            // materialize the fresh same-layout block via `try_lower_spread_record_construct`
            // (each non-overridden field copied from the materialized base — a scalar Load / a
            // borrowed-handle Dup — and the overrides stored), then BORROW it into the call +
            // drop at scope end via `materialized_call_arg` (which seeds its heap-slot
            // `record_masks` + recursive `$__drop_<R>`): cert `i` (alloc + per-field moves) + `d`
            // (recursive drop), identical to the verified `let r = {...base, …}; f(r)` bind. The
            // SAME producer + drop wiring the Record arm uses — only the base-copy differs.
            // Outside the subset (a non-materialized base, an override field outside the
            // executable subset) it returns None → WALL (never an `Init::Opaque` empty record).
            IrExprKind::SpreadRecord { .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_spread_record_construct(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "spread-record argument cannot be faithfully materialized in this \
                             brick (a non-materialized base or a field outside the subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A fresh HEAP literal argument (`f("x")`, `f([1, 2, 3])`):
            // materialized into an owned temp via `Alloc`, borrowed into the
            // call, dropped at scope end — cert `i` (alloc) + `d` (drop), both
            // backed, identical to the verified fresh-heap bind.
            // A TUPLE argument (`f((slice, pos))`): the same masked nested-ownership block as a
            // record arg — heap elements via `try_lower_tuple_construct` (each `lower_owned_heap_field`
            // moved in), all-scalar via `try_lower_scalar_tuple_construct`, borrowed into the call +
            // dropped at scope end. `materialized_call_arg` already seeds the Tuple's `record_masks`
            // + recursive `$__drop_<R>` (calls_p4.rs), so the leaf fields then the block are freed —
            // no leak, no double-free. An unlowerable element returns `None` and WALLs (never Opaque).
            IrExprKind::Tuple { elements } => {
                let repr = repr_of(&a.ty)?;
                match self
                    .try_lower_tuple_construct(elements)
                    .or_else(|| self.try_lower_scalar_tuple_construct(elements))
                {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(
                            "Tuple argument cannot be faithfully materialized in this brick \
                             (an element outside the executable subset)"
                                .into(),
                        ))
                    }
                }
            }
            // A heap-result `if` operand (`"a=" + (if c then "x" else "y")` — the StringInterp
            // `${if …}` desugar; `f(if c then a else b)`): materialize it via try_lower_heap_result_if
            // into an OWNED+TRACKED value (cert `i`, scope-end `d`) — EXACTLY the let-bound heap-if
            // path (`let s = if …; f(s)` already lowers) — then BORROW it into the call
            // (`CallArg::Handle`). Without this a heap-result-`if` operand fell to the deferred Opaque
            // arm below (rejected → the function walled). Closes the StringInterp-with-`${if}` wall
            // (porta `format_tool_log`) + any call/concat with a heap-result-`if` arg.
            IrExprKind::If { cond, then, else_ } if is_heap_ty(&a.ty) => {
                match self.try_lower_heap_result_if(cond, then, else_, &a.ty) {
                    // Route through `materialized_call_arg` (the sibling `Match` arm's own
                    // B52 fix) rather than a bare `CallArg::Handle`: it tracks `dst` in
                    // `live_heap_handles` AND, for a Tuple/Record `a.ty`, seeds the
                    // precise-destructure masks `lower_destructure` (binds_p2.rs) needs —
                    // without it, `let (a,b) = if c then (..) else (..)` materialized the
                    // tuple fine but its scalar-component destructure fell to the generic
                    // container-grain fallback (STRICT mode's `Const-0` wall). Verified
                    // SAFE for the pre-existing plain-value case too (`println(if c then
                    // "x" else "y")` in a 10,000× loop, 4MB cap, no leak/double-free before
                    // OR after this change — `live_heap_handles` is the SAME scope-end-drop
                    // list either path already respects, so adding the tracking here closes
                    // a genuine latent gap rather than introducing a double-free).
                    Some(dst) => {
                        let repr = repr_of(&a.ty)?;
                        self.materialized_call_arg(dst, repr, &a.ty)
                    }
                    None => {
                        return Err(LowerError::Unsupported(
                            "heap-result `if` in a call-argument position outside the executable \
                             subset"
                                .into(),
                        ))
                    }
                }
            }
            // A heap-result `match` operand (`let (label, len) = match x { s if .. => (..), s
            // => (..) }` — a tuple-pattern-let desugar that routes the match's VALUE through a
            // call argument): desugar the match to an equivalent `if`/`else if` chain via the
            // EXISTING, PROVEN `desugar_match_to_if` (the same transform tail/bind positions
            // already use), then lower it through the EXISTING, PROVEN heap-result-`if`
            // call-arg path directly above — no new lowering machinery, just wiring Match into
            // the If arm's already-working call-arg handling. Without this, EVERY heap-result
            // match operand fell straight to the generic wall below.
            IrExprKind::Match { subject, arms } if is_heap_ty(&a.ty) => {
                // `desugar_match_to_if` wraps its result in a `Block` (hoisted `let`
                // bindings PRECEDING the `If`) whenever the subject isn't one of the
                // freely-substitutable KINDS `build_match_chain`'s `subject_pure` admits
                // (`Var`/`LitInt`/`LitBool`/`LitFloat` — a `LitStr` subject, e.g. a
                // single-use `let x = "hello"; match x {...}` after an EARLIER inlining
                // pass propagates `x`'s literal value into the subject position, is NOT
                // in that list, so it takes the conservative `bind_subject` path instead
                // of inline substitution). This site only ever pattern-matched a BARE
                // `If`, declining outright on the Block-wrapped form — closing the ENTIRE
                // "match value in a call-argument position" class for any subject shape
                // needing the hoist, not just the LitStr case (`match arms returning
                // tuples`'s `let (label, len) = match x {s if .. => (..), s => (..)}`).
                // Lower the hoisted `let`s first (their own scope-end drops apply
                // normally), THEN unwrap to the inner `If` and proceed exactly as before.
                let lifted = self.desugar_match_to_if(subject, arms, &a.ty).and_then(|e| {
                    let (stmts, if_expr) = match e.kind {
                        IrExprKind::If { .. } => (Vec::new(), e),
                        IrExprKind::Block { stmts, expr: Some(tail) } => (stmts, *tail),
                        _ => return None,
                    };
                    let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return None };
                    for s in &stmts {
                        self.lower_stmt(s).ok()?;
                    }
                    self.try_lower_heap_result_if(cond, then, else_, &a.ty)
                });
                match lifted {
                    // Route through `materialized_call_arg` (not a bare `CallArg::Handle`):
                    // it tracks `dst` in `live_heap_handles` AND, for a Tuple/Record `a.ty`,
                    // seeds `record_masks`/`variant_drop_handles` from `aggregate_field_tys`
                    // — the SAME seeding `lower_destructure`'s OWN precise-tuple-extraction
                    // path (binds_p2.rs) needs to find already done (it only seeds when
                    // `live_heap_handles.contains(&subj)`, which a bare `CallArg::Handle`
                    // never satisfies). WITHOUT this, `let (label, len) = match x {...}`
                    // materialized the tuple fine but its DESTRUCTURE fell to the generic
                    // container-grain `bind_pattern` fallback, which WALLS a scalar
                    // component in STRICT mode (a Const-0 would silently corrupt `len`).
                    Some(dst) => {
                        let repr = repr_of(&a.ty)?;
                        self.materialized_call_arg(dst, repr, &a.ty)
                    }
                    None => {
                        return Err(LowerError::Unsupported(
                            "heap-result `match` in a call-argument position outside the \
                             executable subset"
                                .into(),
                        ))
                    }
                }
            }
            IrExprKind::LitStr { .. }
            | IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
            // A CLOSURE value argument (`register((x) => …)`): a fresh heap env,
            // materialized + borrowed into the call. The callee borrows it per the
            // borrow-by-default convention; its body's calls are elided ⇒ the gate
            // taints the function caps-unverified (invocation caps unknown).
            // (A NON-CAPTURING `Lambda` arg is intercepted BELOW and lifted to a scalar
            // FuncRef slot passed by value — `list.map(xs, (x) => x + 1)`; only a
            // capturing one reaches this deferred Opaque arm.)
            | IrExprKind::ClosureCreate { .. } => {
                let repr = repr_of(&a.ty)?;
                // A NON-EMPTY `List[String]` (or scalar-aggregate-element) LITERAL arg
                // (`f(["a", "b"])`) materializes the REAL nested-ownership DynListStr via the
                // same builder the RETURN position uses (each element moved/Dup'd in), borrowed
                // into the call + dropped at scope end by DropListStr (cert `i` + recursive `d`).
                // Without this it fell to `alloc_init` → `Init::Opaque` empty list = rejected as
                // a silent miscompile below. (An empty/`List[Value]`/computed list still defers
                // to `alloc_init`, unchanged — the foundation for heap-element-list call args.)
                // A NON-EMPTY heap-element `List[String]`/aggregate literal → the nested-ownership
                // builder; a SCALAR-element `List[Int/Float/Bool]` with NON-literal elements
                // (`[pos]`, `[a, b]`) → the flat `DynList` + `store64` builder (a scalar list owns
                // no heap, so the scope-end drop is a flat `Drop`). Both yield a REAL populated
                // block, vs the `alloc_init` `Init::Opaque` that an all-literal-only path leaves
                // (rejected below). Closes `f([pos])` / the `acc + [pos]` append-accumulator element.
                // An EMPTY-map arg (`fold(xs, [:], …)` seed, `takes([:])` with ascription):
                // the SAME layout-agnostic 0-length block an empty-map BIND builds (a v1
                // Map is a paired-slot List; len 0 ⇒ the drop frees only the block) —
                // REAL (never Opaque), borrowed into the call + dropped at scope end.
                if matches!(&a.kind, IrExprKind::EmptyMap)
                    || matches!(&a.kind, IrExprKind::MapLiteral { entries } if entries.is_empty())
                {
                    if let Some(dst) = self.try_lower_scalar_list_slots(&[]) {
                        out.push(self.materialized_call_arg(dst, repr, &a.ty));
                        return Ok(());
                    }
                }
                if matches!(&a.kind, IrExprKind::List { .. }) {
                    // A List[Record] literal arg (`group([rect(…), …])`): the record-list builder
                    // already pushes it to live_heap_handles + routes its drop to $__drop_list_<R>,
                    // so pass the handle directly (a second materialized_call_arg would double-track).
                    if let Some(dst) = self.try_lower_record_list_literal(a) {
                        out.push(CallArg::Handle(dst));
                        return Ok(());
                    }
                    if let Some(dst) = self
                        .try_lower_str_list_literal(a)
                        .or_else(|| self.try_lower_scalar_list_construct(a))
                    {
                        out.push(self.materialized_call_arg(dst, repr, &a.ty));
                        return Ok(());
                    }
                    // `f([a, b])` where a/b are TRACKED heap Vars with FLAT content
                    // (`list.flatten([first, second])` — the fft two-accumulator merge;
                    // `matrix.from_rows([r0, r1])`): materialize a fresh list CO-OWNING
                    // each element (Dup +1), dropped flat at scope end (per-slot rc_dec
                    // + block — a flat-content element's rc_dec IS its full free).
                    if let Some(dst) = self.try_lower_heap_var_list_literal(a) {
                        // A BORROW-VIEW list (slots are borrowed handles; the block-only
                        // plain Drop is already tracked inside the builder) — pass the
                        // handle directly, NO materialized_call_arg re-track.
                        out.push(CallArg::Handle(dst));
                        return Ok(());
                    }
                }
                let init = alloc_init(a);
                // `alloc_init` faithfully materializes a string literal and a scalar-
                // literal list/tuple; every other constructor (Map/Record/Result/Option/
                // closure, a computed-element list) yields `Init::Opaque` — an EMPTY heap
                // value. Borrowing an empty value into the call lets the callee read zero
                // bytes = a SILENT MISCOMPILE, so reject the unfaithful case explicitly.
                if matches!(init, Init::Opaque) {
                    return Err(LowerError::Unsupported(format!(
                        "{} argument cannot be faithfully materialized in this brick \
                         (would borrow an empty deferred heap value)",
                        kind_name(&a.kind)
                    )));
                }
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc { dst, repr, init });
                self.record_elided_calls(a);
                self.materialized_call_arg(dst, repr, &a.ty)
            }
            // A Bool literal argument (`f(true)`): the real value 1/0 (the `if` cond
            // a callee branches on). `LitInt` is already an `Imm` above.
            IrExprKind::LitBool { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: if *value { 1 } else { 0 } });
                CallArg::Scalar(dst)
            }
            // A Float literal arg (`f(2.5)`): the i64-uniform value carries the f64 BITS
            // (the float-floor render reinterprets), so `2.5` materializes as ConstInt.
            IrExprKind::LitFloat { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::ConstInt { dst, value: crate::lower::float_lit_bits(*value, &a.ty) });
                CallArg::Scalar(dst)
            }
            // `f(a + b)` — a string concat in a CALL-ARG position (also a NESTED `a + b + c`,
            // where `a + b` is the left operand arg). Lower it to the __str_concat call; its
            // fresh owned String is borrowed into the outer call and dropped at scope end.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_concat_str(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    // A non-lowerable string concat as a call ARGUMENT would borrow an
                    // empty deferred String into the callee = a SILENT MISCOMPILE. Reject.
                    None => {
                        return Err(LowerError::Unsupported(
                            "non-lowerable string concat in a call-argument position \
                             (would borrow an empty deferred String)"
                                .into(),
                        ))
                    }
                }
            }
            // `f(xs + [7])` — a SCALAR-element list concat in a CALL-ARG position. Lower it to
            // the __list_concat call; its fresh owned list is borrowed into the outer call and
            // dropped at scope end. A heap-element list concat (or a non-lowerable operand)
            // returns None and falls to the deferred Opaque.
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_concat_list(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    // A non-lowerable list concat (heap-element / non-lowerable operand) as a
                    // call ARGUMENT would borrow an empty deferred list = a SILENT MISCOMPILE.
                    None => {
                        return Err(LowerError::Unsupported(
                            "non-lowerable list concat in a call-argument position \
                             (would borrow an empty deferred list)"
                                .into(),
                        ))
                    }
                }
            }
            // `f(opt ?? default)` — a `??` over a self-host materialized Option in a CALL-ARG
            // position (`int.to_string(list.get(xs, i) ?? 0)` / `println(list.get(ss, i) ?? "d")`
            // — extremely common). The let-bind path executes this via
            // `try_lower_option_unwrap_or`; the arg position must too, else the Option call
            // deferred to a bare elided-call marker (wrong arity → invalid wasm). A SCALAR result
            // passes by value; a HEAP-String result (`option.unwrap_or_str` — a fresh owned
            // String, tracked for scope-end drop by the helper) passes as a borrowed Handle. A
            // non-String-heap / non-Option operand returns None and defers below.
            IrExprKind::UnwrapOr { expr, fallback } => {
                let mark = self.ops.len();
                let lhh_mark = self.live_heap_handles.len();
                match self.try_lower_option_unwrap_or(expr, fallback, true) {
                    Some(v) if is_heap_ty(&a.ty) => CallArg::Handle(v),
                    Some(v) => CallArg::Scalar(v),
                    None => {
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        if is_heap_ty(&a.ty) {
                            // A non-lowerable `??` with a HEAP result as a call ARGUMENT
                            // would borrow an empty deferred heap value = a SILENT
                            // MISCOMPILE. Reject. (A SCALAR `??` falls to the deferred
                            // `Const` 0 below — the separate silent-zero class, left as-is.)
                            return Err(LowerError::Unsupported(
                                "non-lowerable `??` with a heap result in a call-argument \
                                 position (would borrow an empty deferred heap value)"
                                    .into(),
                            ));
                        }
                        // A `??` over an OPTION operand whose Some-payload could NOT be read
                        // (`r.opt ?? -1.0` over an `Option[scalar]` FIELD access — no tracked
                        // handle) must NOT silently take the fallback: a `Const 0` reads the
                        // WRONG arm when the Option is `Some` (a silent miscompile, exposed once
                        // derived-Codec `Option` decode links — codec_float_int). WALL it. A
                        // Result `??` (`int.parse(s) ?? -1`) keeps the Const-0 class below.
                        if matches!(&expr.ty,
                            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _))
                        {
                            return Err(LowerError::Unsupported(
                                "non-lowerable `??` over an Option operand in a call-argument \
                                 position cannot be faithfully computed (a Const-0 would take \
                                 the fallback when the Option is Some) not in this brick"
                                    .into(),
                            ));
                        }
                        let dst = self.fresh_value();
                        self.record_elided_calls(a);
                        if crate::lower::strict_values() {
                return Err(crate::lower::strict_const_wall(&format!("call argument ({})", kind_name(&a.kind))));
            }
            self.ops.push(Op::Const { dst });
                        CallArg::Scalar(dst)
                    }
                }
            }
            // A scalar-result `match` over a HEAP subject must EXECUTE: a VARIANT
            // (Option/Result) via the tag-read value-match, a scalar-pattern subject via
            // the desugared if-chain. If it falls outside the executable subset (e.g. a
            // `match s { "a" => 1, _ => 9 }` over a String — string equality is not yet
            // lowered) a Const-0 fallback would SILENTLY pick a wrong arm, so WALL it. The
            // executing forms (`match o`/`match list.get(..)`/`match n { 1 => .. }`)
            // return a real `CallArg::Scalar` here.
            IrExprKind::Match { subject, .. }
                if !is_heap_ty(&a.ty) && is_heap_ty(&subject.ty) =>
            {
                let mark = self.ops.len();
                match self.lower_scalar_value(a) {
                    Some(v) => CallArg::Scalar(v),
                    None => {
                        self.ops.truncate(mark);
                        return Err(LowerError::Unsupported(
                            "scalar-result match over a heap subject in a call-argument \
                             position outside the executable subset cannot be faithfully \
                             computed (a Const-0 would silently pick a wrong arm) not in \
                             this brick"
                                .into(),
                        ));
                    }
                }
            }
            // A fresh BinOp/UnOp result as an argument (`f(a + b)`, `f(-n)`), or an
            // ERROR OPERATOR result (`f(x!)`, `f(x ?? d)`, `f(x?.field)`): a fresh
            // computed value — a heap result is materialized via `Alloc` (borrowed
            // and dropped), a scalar result is a `Const`. Operands carry their own
            // ownership; the operator's value (and any early-return) is deferred.
            // An `f(x!)` (Unwrap — effect-fn error propagation) as a call ARGUMENT was a
            // deferred `Const`/`Opaque` = a SILENT MISCOMPILE (`f(int.parse(s)!)` passed 0).
            // The faithful lowering needs early-return-on-Err (a later brick); until then
            // WALL it — NEVER pass a silently-wrong value (the ② cardinal rule).
            IrExprKind::Unwrap { .. } => {
                return Err(LowerError::Unsupported(
                    "unwrap `!` in a call-argument position cannot be faithfully computed \
                     (needs early-return propagation; a Const/Opaque would be a silently \
                     wrong value) not in this brick"
                        .into(),
                ));
            }
            // A RANGE argument with SCALAR bounds (`f(0..n)` — the gguf
            // parse_metadata_entries `for _ in 0..count` append-accumulator desugar):
            // materialize the REAL list via the self-hosted `list.range` (a fresh owned
            // List[Int], borrowed into the call, dropped at scope end). An inclusive
            // range widens the end by one (`a..=b` = `range(a, b+1)`), exactly v0's
            // iteration space. Non-scalar bounds still wall below.
            IrExprKind::Range { start, end, inclusive } if is_heap_ty(&a.ty) => {
                let range_mark = self.ops.len();
                let (s_v, e_v0) = match (
                    self.lower_scalar_value(start),
                    self.lower_scalar_value(end),
                ) {
                    (Some(sv), Some(ev)) => (sv, ev),
                    _ => {
                        self.ops.truncate(range_mark);
                        return Err(LowerError::Unsupported(
                            "heap-result Range in a call-argument position cannot be                                  faithfully computed in this brick (non-scalar bound)"
                                .into(),
                        ));
                    }
                };
                let mut e_v = e_v0;
                if *inclusive {
                    let one = self.fresh_value();
                    self.ops.push(Op::ConstInt { dst: one, value: 1 });
                    let e2 = self.fresh_value();
                    self.ops.push(Op::IntBinOp {
                        dst: e2,
                        op: crate::IntOp::Add,
                        a: e_v,
                        b: one,
                    });
                    e_v = e2;
                }
                let repr = repr_of(&a.ty)?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: "list.range".to_string(),
                    args: vec![CallArg::Scalar(s_v), CallArg::Scalar(e_v)],
                    result: Some(repr),
                });
                out.push(self.materialized_call_arg(dst, repr, &a.ty));
                return Ok(());
            }
            IrExprKind::BinOp { .. }
            | IrExprKind::UnOp { .. }
            | IrExprKind::Try { .. }
            // (UnwrapOr is handled in full — scalar + heap — by the dedicated arm above.)
            | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. }
            // A RANGE (`f(0..n)`), a RUNTIME CALL, or an `if`/`match` ARGUMENT is a
            // fresh value of the same shape — a deferred `Alloc{Opaque}`/`Const`,
            // its calls (incl. the branch arms' calls) captured by
            // `record_elided_calls`; the arms' values/effects are deferred.
            | IrExprKind::Range { .. }
            | IrExprKind::RuntimeCall { .. }
            | IrExprKind::If { .. }
            | IrExprKind::Match { .. } => {
                if is_heap_ty(&a.ty) {
                    // A heap-result operator / branch as a call ARGUMENT (`f(a ++ b)`
                    // unlowered, `f(if c then "a" else "b")`, `f(0..n)`) would borrow an
                    // empty deferred heap value into the callee = a SILENT MISCOMPILE.
                    return Err(LowerError::Unsupported(format!(
                        "heap-result {} in a call-argument position cannot be faithfully \
                         computed in this brick (would borrow an empty deferred heap value)",
                        kind_name(&a.kind)
                    )));
                } else {
                    // A scalar Int arithmetic / comparison / prim arg computes its
                    // REAL value (`f(n / 10)` → IntBinOp); outside that subset it
                    // rolls back to the deferred Const + elided caps marker.
                    let mark = self.ops.len();
                    match self.lower_scalar_value(a) {
                        Some(v) => CallArg::Scalar(v),
                        None => {
                            self.ops.truncate(mark);
                            let dst = self.fresh_value();
                            self.record_elided_calls(a);
                            if crate::lower::strict_values() {
                return Err(crate::lower::strict_const_wall(&format!("call argument ({})", kind_name(&a.kind))));
            }
            self.ops.push(Op::Const { dst });
                            CallArg::Scalar(dst)
                        }
                    }
                }
            }
            // A field/element/tuple EXTRACTION argument. A SCALAR result is an
            // unambiguous COPY → `Const`. A HEAP result is an ALIAS/share of
            // the container → `Op::Dup` of the container value (the container-
            // grain field access), borrowed into the call and dropped at scope
            // end. (A nested-container extraction stays walled inside
            // `lower_heap_extraction`.)
            IrExprKind::Member { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::TupleIndex { .. } => {
                if is_heap_ty(&a.ty) {
                    let repr = repr_of(&a.ty)?;
                    // A non-var container (`f().x`) cannot be aliased (no single `src` to
                    // `Dup`); the deferred Opaque empty value borrowed into the callee is a
                    // SILENT MISCOMPILE, so a failed extraction rejects here.
                    let dst = self.lower_heap_extraction(a)?;
                    // A precise heap-field BORROW (`b.label`) is in `param_values` — the
                    // container owns it, so it is passed by Handle WITHOUT joining the
                    // scope-end drop set (no second owner, no double-free). A container-
                    // grain Dup / deferred Opaque is a fresh owned temp → tracked normally.
                    if self.param_values.contains(&dst) {
                        CallArg::Handle(dst)
                    } else {
                        self.materialized_call_arg(dst, repr, &a.ty)
                    }
                } else {
                    // A SCALAR extraction (`r.x`, `t.0`, `xs[i]`) — load the REAL field /
                    // element value from the block's layout slot when the container is a
                    // materialized scalar aggregate / a tracked list (the VALUE MODEL).
                    // `lower_scalar_value` dispatches Member/TupleIndex to the field load and
                    // IndexAccess to the bounds-checked `$elem_addr` load. Outside that subset
                    // (a non-var / heap-field-aggregate container, or a computed container
                    // `g().field`) it rolls back to a deferred `Const` copy with the
                    // container's calls elided (the caps fold then sees them), as before.
                    let mark = self.ops.len();
                    match self.lower_scalar_value(a) {
                        Some(v) => CallArg::Scalar(v),
                        None => {
                            self.ops.truncate(mark);
                            // ANF-LIFT `f().x` (a scalar field on a call result — the
                            // paren-defaults `mk_defaults().port` shape): bind the call
                            // to a SYNTHETIC temp exactly like `let tmp = f(); tmp.x`
                            // (the tail.rs heap-extraction discipline — the bind tracks
                            // + seeds the record's read shape), then the field-slot load
                            // resolves over the tracked temp.
                            let lifted = if let IrExprKind::Member { object, field } = &a.kind
                            {
                                if matches!(object.kind, IrExprKind::Call { .. })
                                    && is_heap_ty(&object.ty)
                                {
                                    let tmp = self.fresh_synth_var();
                                    let field = *field;
                                    let obj_ty = object.ty.clone();
                                    self.lower_bind(tmp, &obj_ty, object).ok().and_then(
                                        |_| {
                                            let synth = IrExpr {
                                                kind: IrExprKind::Member {
                                                    object: Box::new(IrExpr {
                                                        kind: IrExprKind::Var { id: tmp },
                                                        ty: obj_ty,
                                                        span: object.span.clone(),
                                                        def_id: None,
                                                    }),
                                                    field,
                                                },
                                                ty: a.ty.clone(),
                                                span: a.span.clone(),
                                                def_id: None,
                                            };
                                            self.lower_scalar_value(&synth)
                                        },
                                    )
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if let Some(v) = lifted {
                                out.push(CallArg::Scalar(v));
                                return Ok(());
                            }
                            self.ops.truncate(mark);
                            // A scalar field access on a COMPUTED CALL result (`mk(5).x`)
                            // — the call result is not a tracked aggregate, so a Const-0
                            // reads a WRONG value (and the record-returning callee now
                            // renders, making it observable). WALL it. A tracked-Var
                            // container (`r.x`) lowered above and never reaches here; other
                            // computed containers keep the deferred Const (unchanged).
                            if let IrExprKind::Member { object, .. } = &a.kind {
                                if matches!(object.kind, IrExprKind::Call { .. }) {
                                    return Err(LowerError::Unsupported(
                                        "scalar field access on a computed call result \
                                         cannot be faithfully computed in this brick (a \
                                         Const-0 would read a wrong value) not in this brick"
                                            .into(),
                                    ));
                                }
                            }
                            let dst = self.fresh_value();
                            if crate::lower::strict_values() {
                return Err(crate::lower::strict_const_wall(&format!("call argument ({})", kind_name(&a.kind))));
            }
            self.ops.push(Op::Const { dst });
                            self.record_elided_calls(a);
                            CallArg::Scalar(dst)
                        }
                    }
                }
            }
            // A custom-variant CONSTRUCTOR argument (`val(Num(7))`, `f(Eof)`) — NOT a
            // function call: materialize the tagged value-model block (tag@slot0 + scalar
            // fields@slot1..) via `try_lower_variant_ctor`, borrowed into the call and
            // dropped at scope end (cert `i` + `d`, like the record-literal arg above).
            // Must PRECEDE the generic Named-call arm, which would emit a dangling
            // `CallFn "Num"` (an unlinked call = invalid wasm). Outside the subset (a
            // heap/recursive ctor field — ADT brick 5) it WALLs, never a wrong-bytes block.
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
                if self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
            {
                let repr = repr_of(&a.ty)?;
                match self.try_lower_variant_ctor(a) {
                    Some(dst) => self.materialized_call_arg(dst, repr, &a.ty),
                    None => {
                        return Err(LowerError::Unsupported(format!(
                            "variant constructor `{}` argument cannot be faithfully \
                             materialized in this brick (a heap/recursive field — ADT brick 5)",
                            name.as_str()
                        )))
                    }
                }
            }
            // A Named user-call result, materialized into an owned temp.
            IrExprKind::Call { target: CallTarget::Named { name }, args: inner, .. } => {
                let inner_args = self.lower_call_args(inner)?;
                let dst = self.fresh_value();
                let repr = repr_of(&a.ty)?;
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: inner_args,
                    result: Some(repr),
                });
                let arg = self.materialized_call_arg(dst, repr, &a.ty);
                // A user function returning Option/Result yields a REAL same-layout variant
                // block (an in-profile `-> Option[T]` callee returns `OptSome`/`OptNone`,
                // a `-> Result[..]` the DynListStr — the v1 calling convention, the SAME
                // evidence as a variant PARAM). Seed the READ-shape so a `match`/`??` over
                // this owned call result EXECUTES (reads the tag) instead of WALLing/deferring.
                // Ownership is unchanged — `materialized_call_arg` already registered the
                // scope-end drop; `seed_variant_param` adds only layout knowledge.
                if is_variant_ty(&a.ty) {
                    self.seed_variant_param(dst, &a.ty);
                }
                arg
            }
            // A first-order pure stdlib Module-call result, materialized (the
            // purity + higher-order gates live in `lower_pure_module_value_call`).
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args: inner, .. } => {
                let dst = self.lower_pure_module_value_call(
                    module.as_str(),
                    func.as_str(),
                    inner,
                    &a.ty,
                )?;
                let repr = repr_of(&a.ty)?;
                // A prim BORROW result (`prim.load_str` = LoadHandle, marked in `param_values`)
                // is owned by its source block — pass it by Handle WITHOUT joining the scope-end
                // drop set, exactly like a precise heap-field borrow (`b.label`). Dropping it
                // would double-free with the owner's drop (a Str Value's tag-4 payload free).
                if self.param_values.contains(&dst) {
                    CallArg::Handle(dst)
                } else {
                    self.materialized_call_arg(dst, repr, &a.ty)
                }
            }
            // A `Method`/`Computed`-target call argument (`f(obj.m())`,
            // `f((g)())`): an UNRESOLVABLE callee (dispatch / closure value not
            // known here) — model it as a DEFERRED fresh value (a heap `Alloc`
            // borrowed+dropped, a scalar `Const`). Its receiver's/args' calls are
            // captured by `record_elided_calls`, but the method/computed call
            // itself is NOT (skipped), so the source has MORE call nodes than the
            // MIR ⇒ the `ir_calls > mir_calls` gate TAINTS the function caps-
            // unverified (honest — the callee's capabilities are unknown). The
            // result value is deferred, like every Opaque.
            IrExprKind::Call { target, args: inner, .. } => {
                if is_heap_ty(&a.ty) {
                    // C1 HEAP DIRECT-CALL INLINE: a heap-result `Computed` call `f(x)` whose
                    // callee is a statically-known let-bound INLINE lambda is DEFUNCTIONALIZED
                    // to its inlined body — a FRESH OWNED heap value (tracked for scope-end
                    // drop), BORROWED into this outer call. This EXECUTES `"${param_ty(p)}"`
                    // (the bindgen `generate_dts` inner-map cell) instead of walling. Rollback-
                    // safe (`try_inline_direct_lambda_call_heap` restores ops + handles on a
                    // miss), so a non-let-lambda `Method`/`Computed` callee falls through to the
                    // reject below — the sound silent-miscompile guard is preserved.
                    if let CallTarget::Computed { callee } = target {
                        let mark = self.ops.len();
                        let lhh = self.live_heap_handles.len();
                        if let Some(v) =
                            self.try_inline_direct_lambda_call_heap(callee, inner, &a.ty)
                        {
                            // `v` is already in `live_heap_handles` (the inline tracks it), so
                            // pass it by Handle WITHOUT `materialized_call_arg` (which would
                            // double-track → a double-free). A String result drops via the flat
                            // `Op::Drop` (rc_dec), already correct for the default scope-end drop.
                            out.push(CallArg::Handle(v));
                            return Ok(());
                        }
                        self.ops.truncate(mark);
                        self.live_heap_handles.truncate(lhh);
                        // A heap-result call THROUGH a KNOWN CLOSURE value
                        // (`println(hi("world"))` where `hi` bound a closure block):
                        // EXECUTE it via the closure dispatch — a fresh OWNED value
                        // (cert `i`, scope-end drop), borrowed into the outer call.
                        // Mirrors the bind-position closure call (binds_p2).
                        if let Some(blk) = self.closure_block_of_mut(callee) {
                            if let (Ok(crepr), Ok(lowered)) =
                                (repr_of(&a.ty), self.lower_call_args(inner))
                            {
                                let dst = self.fresh_value();
                                self.emit_closure_call(blk, Some(dst), lowered, Some(crepr));
                                self.live_heap_handles.push(dst);
                                out.push(CallArg::Handle(dst));
                                return Ok(());
                            }
                            self.ops.truncate(mark);
                            self.live_heap_handles.truncate(lhh);
                        }
                    }
                    // An unresolvable `Method`/`Computed` call with a HEAP result as a
                    // call ARGUMENT (`f(obj.m())`, `f((g)())`) would borrow an empty
                    // deferred heap value into the callee = a SILENT MISCOMPILE. Reject.
                    // (A SCALAR result still defers to `Const` 0 below — silent-zero class.)
                    return Err(LowerError::Unsupported(
                        "unresolvable method/computed call with a heap result in a \
                         call-argument position (would borrow an empty deferred heap value)"
                            .into(),
                    ));
                }
                // C1 DIRECT-CALL INLINE: a SCALAR-result `Computed` call `f(x)` whose callee
                // is a statically-known let-bound INLINE lambda is DEFUNCTIONALIZED to its
                // inlined body (`try_lower_scalar_call`'s Computed arm). This EXECUTES
                // `int.to_string(f(1))` (= 3 for `let f = (x) => string.len(s) + x`) instead
                // of the deferred `Const 0` silent-zero below. `try_lower_scalar_call` is
                // rollback-safe (restores ops + handles on a miss), so a non-inlinable
                // Method/Computed callee falls through to the deferred `Const` exactly as
                // before — the caps fold still tags it via `record_elided_calls`.
                let mark = self.ops.len();
                if let Some(v) = self.try_lower_scalar_call(a, &a.ty) {
                    CallArg::Scalar(v)
                } else {
                    self.ops.truncate(mark);
                    let dst = self.fresh_value();
                    self.record_elided_calls(a);
                    if crate::lower::strict_values() {
                return Err(crate::lower::strict_const_wall(&format!("call argument ({})", kind_name(&a.kind))));
            }
            self.ops.push(Op::Const { dst });
                    CallArg::Scalar(dst)
                }
            }
            other => {
                return Err(LowerError::Unsupported(format!(
                    "call argument {} not in this brick",
                    kind_name(other)
                )))
            }
        };
        out.push(arg);
        Ok(())
    }
}

impl LowerCtx {
    /// Materialize a list LITERAL argument (`list.flatten([first, second])`,
    /// `[c_add(e, t)]`) as a BORROW-VIEW: a fresh 2-slot block whose slots hold
    /// BORROWED handles (a tracked Var stays owned by its binding; a fresh call
    /// result is pushed to `live_heap_handles` and freed at scope end). The view
    /// block itself is tracked with NO element set, so its scope-end drop is the
    /// plain block-only `Op::Drop` — the slots' owners are untouched (no double
    /// free, no leak; the callee rc_incs whatever it keeps). Flat-content element
    /// types only (deeper nesting keeps walling).
    pub(crate) fn try_lower_heap_var_list_literal(&mut self, a: &IrExpr) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::List { elements } = &a.kind else { return None };
        if elements.is_empty() {
            return None;
        }
        let elem_ty = match &a.ty {
            Ty::Applied(TypeConstructorId::List, ts) if ts.len() == 1 => &ts[0],
            _ => return None,
        };
        let flat_content = matches!(elem_ty, Ty::String | Ty::Bytes)
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::Bytes, _))
            || matches!(elem_ty,
                Ty::Applied(TypeConstructorId::List, inner)
                    if inner.len() == 1
                        && (!is_heap_ty(&inner[0])
                            || self.aggregate_field_tys(&inner[0])
                                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                                .is_some()))
            // An all-scalar aggregate element itself (`[c_add(e, t)]` — a (Float, Float)
            // Complex): its block holds only inline scalars, rc_dec IS its full free.
            || self
                .aggregate_field_tys(elem_ty)
                .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                .is_some();
        if !flat_content {
            return None;
        }
        let ops_mark = self.ops.len();
        // Each element is a tracked heap Var (borrowed — Dup'd into the slot below) or a
        // heap-returning NAMED call (`[c_add(e, t)]` — fft's concat element): the call's
        // fresh OWNED result is moved into the slot directly (no Dup).
        let lhh_mark = self.live_heap_handles.len();
        let mut handles: Vec<ValueId> = Vec::with_capacity(elements.len());
        for e in elements {
            match &e.kind {
                IrExprKind::Var { id } => {
                    let Ok(src) = self.value_for(*id) else {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    };
                    handles.push(src);
                }
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if !self.variant_layouts.ctor_to_type.contains_key(name.as_str()) =>
                {
                    let Ok(lowered) = self.lower_call_args(args) else {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    };
                    let Ok(erepr) = repr_of(&e.ty) else {
                        self.ops.truncate(ops_mark);
                        self.live_heap_handles.truncate(lhh_mark);
                        return None;
                    };
                    let dst = self.fresh_value();
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: name.as_str().to_string(),
                        args: lowered,
                        result: Some(erepr),
                    });
                    // The fresh result is OWNED by this scope (freed at scope end,
                    // AFTER the callee borrowed it through the view).
                    self.live_heap_handles.push(dst);
                    handles.push(dst);
                }
                _ => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            }
        }
        let n = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: n, value: elements.len() as i64 });
        let list = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: list,
            repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynListStr { len: n },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![list] });
        for (i, src) in handles.into_iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (i as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: h, b: off });
            let oh = self.fresh_value();
            self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(oh), args: vec![src] });
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, oh],
            });
        }
        // The view block itself: tracked with NO element set → plain block-only Drop.
        self.live_heap_handles.push(list);
        Some(list)
    }
}
