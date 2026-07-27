/// What lowering one call argument produced.
///
/// Most arms yield a value the caller pushes. A few push their own arguments
/// directly — a string interpolation becomes several — and those arms used to
/// `return Ok(())` out of the whole function. Naming that outcome is what lets
/// the table be split without an arm silently pushing twice or not at all.
enum ArgOutcome {
    /// The argument value; the caller pushes it.
    Value(CallArg),
    /// The arm already pushed everything it needed.
    Pushed,
}

/// The ELEMENT-family verdict of [`LowerCtx::try_lower_concat_list`] — one field per
/// admitted `ConcatList` element shape, each the like-named predicate binding of the
/// original single-body function. Built by [`LowerCtx::classify_concat_list_elem`] (which
/// carries each predicate's comment); consumed by the operand-route pick, the
/// `__list_concat` vs `__list_concat_rc` choice, and the result-drop registration.
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep).
struct ConcatListElemShape {
    scalar_elem: bool,
    heap_elem: bool,
    str_value_elem: bool,
    list_str_elem: bool,
    flat_list_elem: bool,
    record_elem: Option<String>,
    str_str_elem: bool,
    int_str_elem: bool,
    str_int_elem: bool,
    scalar_aggregate_elem: bool,
    flat_variant_elem: bool,
    rich_variant_elem: Option<String>,
    closure_elem: bool,
    list_scalar_aggregate_elem: bool,
}

impl ConcatListElemShape {
    /// The admissibility bail of `try_lower_concat_list` — the condition moved VERBATIM
    /// (same flags, same polarity, same order): `true` when NO admitted element family
    /// matched, so the concat returns `None` (deferred — never wrong bytes). Extracted
    /// from `try_lower_concat_list` (codopsy round-2 complexity sweep).
    fn matches_no_admitted_family(&self) -> bool {
        !self.scalar_elem && !self.heap_elem && !self.str_value_elem && !self.list_str_elem
            && !self.flat_list_elem && !self.str_str_elem && !self.int_str_elem
            && !self.str_int_elem && !self.scalar_aggregate_elem && !self.flat_variant_elem
            && !self.closure_elem && !self.list_scalar_aggregate_elem
            && self.rich_variant_elem.is_none() && self.record_elem.is_none()
    }
}

/// A `(String, Value)` TUPLE element (the yaml `pairs` shape) — `__list_concat_rc` rc-owns each
/// tuple, freed recursively by `Op::DropListStrValue` (rc_dec the String slot + `$__drop_value` the
/// Value slot, per tuple). The two-heap-field aggregate `DropListStr` cannot express.
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, tuple family 1 of 4).
fn is_str_value_tuple_elem(elem_ty: &Ty) -> bool {
    matches!(elem_ty,
        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String)
            && crate::lower::is_value_ty(&tys[1]))
}

/// A `List[String]` ELEMENT (so the concat's `value` is a `List[List[String]]` — the csv
/// `rows + [cur]` shape). `__list_concat_rc` rc-incs each inner-list handle (the new outer list
/// co-owns each row); the outer's recursive `Op::DropListListStr` frees each row's cells + each
/// row block.
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, nested-list family 1 of 2).
fn is_list_str_elem(elem_ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(elem_ty,
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && matches!(a[0], Ty::String))
}

/// A `List[scalar]` ELEMENT (so the concat's `value` is `List[List[Int]]` — the memory_stress
/// `outer + [[i*100, …]]` nested-accumulator shape). The inner block is FLAT
/// (header + i64 slots, no inner handles), so per-element `rc_dec` IS its full
/// free — the SAME physics as a String element: `__list_concat_rc` co-owns each
/// inner-list handle and the scope-end `DropListStr` frees each + the outer.
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, nested-list family 2 of 2).
fn is_flat_scalar_list_elem(elem_ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(elem_ty,
        Ty::Applied(TypeConstructorId::List, a)
            if a.len() == 1 && !is_heap_ty(&a[0]))
}

/// A `(String, String)` TUPLE element (the `map.entries` shape) — `__list_concat_rc` rc-owns each
/// tuple, freed recursively by `Op::DropListStrStr` (per tuple: rc_dec BOTH String slots). The
/// (String,String) counterpart of `str_value_elem`.
/// Widened to (String, <flat block>) — String OR List[scalar] second slot (the
/// hval map literal's `("xs", [1, 2, 3])` pairs): DropListStrStr's two per-slot
/// rc_decs are each a FULL free for any flat block (`is_list_str_str_ty`'s
/// documented physics).
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, tuple family 2 of 4).
fn is_str_flat_block_tuple_elem(elem_ty: &Ty) -> bool {
    let flat_snd = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, b)
                if b.len() == 1 && !is_heap_ty(&b[0]))
    };
    matches!(elem_ty,
        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && flat_snd(&tys[1]))
}

/// An `(Int, String)` TUPLE element (the `list.enumerate` shape) — `__list_concat_rc` rc-owns each
/// tuple, freed recursively by `$__drop_list_int_str` (per tuple: rc_dec the String slot @20 only,
/// the Int @12 is scalar). Routed via variant_drop_handles (a DropVariant, like the record case).
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, tuple family 3 of 4).
fn is_int_str_tuple_elem(elem_ty: &Ty) -> bool {
    matches!(elem_ty,
        Ty::Tuple(tys) if tys.len() == 2 && !is_heap_ty(&tys[0]) && matches!(tys[1], Ty::String))
}

/// A `(String, Int)` TUPLE element (the gguf `entries + [(key, pos)]` metadata
/// accumulator) — the MIRROR of `int_str_elem`: rc-own each tuple, recursive drop via
/// `DropListStrInt` (rc_dec the String slot @12 only; the Int @20 is scalar).
/// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, tuple family 4 of 4).
fn is_str_int_tuple_elem(elem_ty: &Ty) -> bool {
    matches!(elem_ty,
        Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::String) && matches!(tys[1], Ty::Int))
}

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
    ///
    /// Decomposed for the codopsy round-2 complexity sweep: the element-family
    /// classification, the operand lowering, and the result-drop registration are
    /// verbatim moves into [`Self::classify_concat_list_elem`],
    /// [`Self::lower_concat_list_operands`], and
    /// [`Self::register_concat_list_result_drop`]; this router keeps the whole decision
    /// order (classify → admissibility bail → lower-or-rollback → emit → register).
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
        let shape = self.classify_concat_list_elem(&elem_ty);
        if shape.matches_no_admitted_family() {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let arg_exprs = [(**left).clone(), (**right).clone()];
        let args = match self.lower_concat_list_operands(&arg_exprs, &elem_ty, &shape) {
            Some(a) => a,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let dst = self.fresh_value();
        let name = if shape.scalar_elem { "__list_concat" } else { "__list_concat_rc" };
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name: name.to_string(),
            args,
            result: Some(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }),
        });
        self.register_concat_list_result_drop(shape, &elem_ty, dst);
        Some(dst)
    }

    /// Classify a `ConcatList`'s ELEMENT type into the admitted element families — every
    /// predicate binding of the original body, each carrying the comment that records
    /// which source shape it admits and why its drop is exact. Extracted from
    /// `try_lower_concat_list` (codopsy round-2 complexity sweep, phase 1 of 3); the
    /// tuple and nested-list families moved one step further into the per-shape
    /// predicate fns above the impl, comments with them. All predicates are pure `&self`
    /// type tests, so the grouping changes no evaluation.
    fn classify_concat_list_elem(&self, elem_ty: &Ty) -> ConcatListElemShape {
        // SCALAR-element (i64 slots: Int/Float/Bool) → byte-copy `__list_concat`. HEAP-element String or
        // Value (OWNED handle slots) → the rc-incrementing `__list_concat_rc` (the new list co-owns each
        // element; the source's recursive drop frees its own refs). A heap-FIELD aggregate element
        // (tuple/record with inner heap) still DEFERS — it needs the masked recursive drop (tuple-heap).
        let scalar_elem = !is_heap_ty(elem_ty);
        let heap_elem =
            is_heap_ty(elem_ty) && (matches!(elem_ty, Ty::String) || crate::lower::is_value_ty(elem_ty));
        // A RECORD element (`parent.children + [child]` — the svg `add_child` shape): `__list_concat_rc`
        // rc-incs each record handle (the new list co-owns each), freed recursively by the generated
        // `$__drop_list_<R>` (each element → `$__drop_<R>`). Gated to a recursive-drop record so that fn
        // exists. An ANONYMOUS structural record element (`items + [{ x: …, content: nm, … }]` — the
        // ceangal zip_view_rects append; the checker leaves the element structural) routes to its
        // synthesized `$__drop_list_anonrec_<hash>` wrapper via the same registry.
        let record_elem = self.record_or_anon_drop_type_name(elem_ty);
        // A FLAT-variant ELEMENT (`acc + [r.val]` where `acc: List[ValType]`, the wasm-binary
        // recursive-accumulator shape) — each element is a single OWNED tag-block (no inner handle),
        // so `__list_concat_rc` rc-incs each element handle (the new list co-owns each block) and the
        // scope-end `DropListStr` `rc_dec`s each element + the list block (a flat variant block's
        // `rc_dec` IS its full free). Byte-identical to the proven `List[String]` cert — mirrors the
        // `elem_flat_variant` arm of the List-LITERAL builder (binds.rs). A variant carrying a
        // `String`/nested/`List` field is NOT flat (`is_flat_variant_ty` = false) and stays walled.
        let flat_variant_elem = self.variant_layouts.is_flat_variant_ty(elem_ty);
        // A RICH (recursive-drop) variant ELEMENT (`acc + [instr_r.val]` where `instr_r.val: Instr` —
        // the wasm bytecode instruction accumulator). `__list_concat_rc` rc-incs each element handle
        // (the new list co-owns each block); the scope-end / teardown `$__drop_list_<V>` frees each
        // element RECURSIVELY via `$__drop_<V>` (a flat `rc_dec` would leak each Instr's nested
        // `List[Instr]`). Routed via `variant_drop_handles="list_<V>"`, like the record case.
        let rich_variant_elem = self.variant_layouts.is_rich_variant_ty(elem_ty, &|rn| {
            crate::lower::canonical_record_key(&self.record_layouts, rn).is_some()
        });
        // A CLOSURE element (`fs + [() => …]` — the `list.push`-of-a-closure desugar over a
        // `List[() -> Unit]` var): `__list_concat_rc` rc-incs each closure-block handle (the
        // new list co-owns each element); the scope-end `$__drop_list_closure` frees each
        // element recursively via `__drop_closure` — the SAME route the List[Fn] LITERAL
        // builder registers (`ListElemDrop::Closure`), so build and concat agree on the drop.
        let closure_elem = matches!(elem_ty, Ty::Fn { .. });
        ConcatListElemShape {
            scalar_elem,
            heap_elem,
            str_value_elem: is_str_value_tuple_elem(elem_ty),
            list_str_elem: is_list_str_elem(elem_ty),
            flat_list_elem: is_flat_scalar_list_elem(elem_ty),
            record_elem,
            str_str_elem: is_str_flat_block_tuple_elem(elem_ty),
            int_str_elem: is_int_str_tuple_elem(elem_ty),
            str_int_elem: is_str_int_tuple_elem(elem_ty),
            scalar_aggregate_elem: self.is_scalar_aggregate_elem(elem_ty),
            flat_variant_elem,
            rich_variant_elem,
            closure_elem,
            list_scalar_aggregate_elem: self.is_list_scalar_aggregate_elem(elem_ty),
        }
    }

    /// An ALL-SCALAR aggregate element (`first + [(re, im)]` — the fft Complex
    /// accumulator): each element block holds only inline scalars, so the flat
    /// per-slot-rc_dec `DropListStr` IS its full free (same physics as the binds-side
    /// `elem_scalar_aggregate` list literal).
    /// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep,
    /// aggregate family 1 of 2).
    fn is_scalar_aggregate_elem(&self, elem_ty: &Ty) -> bool {
        self.aggregate_field_tys(elem_ty)
            .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
            .is_some()
    }

    /// A `List[<SCALAR-ONLY record>]` ELEMENT (so the concat's `value` is
    /// `List[List[P]]` — snaidhm's ttf `contours = contours + [pts]`,
    /// #888). The inner list is a block of FLAT element blocks, so the
    /// nested `DropListListStr` sweep is exact: it frees each inner
    /// element then each inner block then the outer — the same physics as
    /// the `List[List[String]]` case (`is_list_str_elem`), since a scalar-only
    /// record's `rc_dec` IS its full free (that is what
    /// `scalar_aggregate_elem` already relies on one level down).
    /// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep,
    /// aggregate family 2 of 2).
    fn is_list_scalar_aggregate_elem(&self, elem_ty: &Ty) -> bool {
        matches!(elem_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a) if a.len() == 1)
            && match elem_ty {
                Ty::Applied(_, a) => self
                    .aggregate_field_tys(&a[0])
                    .and_then(|(_, tys)| crate::lower::layout::scalar_slots(&tys))
                    .is_some(),
                _ => false,
            }
    }

    /// Lower the two `ConcatList` operands to call args, picking the FORCED-literal route
    /// for a record / scalar-aggregate element and the generic route otherwise — the
    /// original `args` computation moved verbatim (same route conditions, same fallbacks,
    /// same failure result). `None` means the caller rolls back its op/handle marks and
    /// defers, exactly as before. Extracted from `try_lower_concat_list` (codopsy round-2
    /// complexity sweep, phase 2 of 3).
    fn lower_concat_list_operands(
        &mut self,
        arg_exprs: &[IrExpr; 2],
        elem_ty: &Ty,
        shape: &ConcatListElemShape,
    ) -> Option<Vec<CallArg>> {
        // A RECORD-element concat (`acc + [{...}]`, the wasm section-parser append): a `[{record
        // literal}]` operand infers a STRUCTURAL element type, so the generic `lower_call_args` →
        // `try_lower_record_list_literal` declines it. Lower a list-LITERAL operand via the forced
        // helper with the concat's NAMED element type (`elem_ty`) so it materializes + registers its
        // drop with the SAME declared layout the concat result uses (`list_<Named>`). Other operands
        // (the `acc` Var / a nested concat) lower generically.
        // A FLAT-record element takes the same forced-literal route as a
        // recursive-drop record: its `[xs[i]]` / `[{…}]` operand needs the
        // per-element arms of the literal builder, which the generic
        // call-argument path does not have (#888/#904).
        if shape.record_elem.is_some() || shape.scalar_aggregate_elem {
            let mut out: Vec<CallArg> = Vec::with_capacity(arg_exprs.len());
            let mut ok = true;
            for a in arg_exprs {
                if !self.lower_forced_literal_concat_operand(a, elem_ty, shape, &mut out) {
                    ok = false;
                    break;
                }
            }
            if ok { Some(out) } else { None }
        } else {
            self.lower_call_args(arg_exprs).ok()
        }
    }

    /// ONE operand of the forced-literal `ConcatList` route — the per-operand body of
    /// [`Self::lower_concat_list_operands`]'s loop, moved verbatim: a list LITERAL goes
    /// through the forced record-literal helper (with the record-vs-flat fallback rule
    /// below), anything else lowers generically. `false` is exactly the former
    /// `{ ok = false; break; }` — the caller stops and returns `None` — following the
    /// same early-exit renaming this file's `lower_call_arg_into` decomposition proved
    /// (an early `break` becomes a named outcome). Extracted from
    /// `try_lower_concat_list` (codopsy round-2 complexity sweep, phase 2 of 3,
    /// per-operand leaf).
    fn lower_forced_literal_concat_operand(
        &mut self,
        a: &IrExpr,
        elem_ty: &Ty,
        shape: &ConcatListElemShape,
        out: &mut Vec<CallArg>,
    ) -> bool {
        if matches!(a.kind, IrExprKind::List { .. }) {
            match self.try_lower_record_list_literal_as(a, Some(elem_ty)) {
                Some(d) => {
                    out.push(CallArg::Handle(d));
                    true
                }
                // A RECURSIVE-drop record must NOT fall back: the generic path
                // materializes a structural literal in SOURCE field order, which the
                // declared-order `$__drop_<R>` would then free wrongly (the soundness
                // crux this forced route exists for). A FLAT record has no such drop —
                // per-element `rc_dec` is its full free whatever the order — so it
                // keeps the generic path as a fallback, which is exactly the route it
                // took before the literal builder learned its element arms. That makes
                // this widening strictly ADDITIVE: shapes that lowered before still
                // take the same path, and only the ones that walled gain a route
                // (#888/#904).
                None if shape.record_elem.is_some() => false,
                None => match self.lower_call_args(std::slice::from_ref(a)) {
                    Ok(mut la) => {
                        out.append(&mut la);
                        true
                    }
                    Err(_) => false,
                },
            }
        } else {
            match self.lower_call_args(std::slice::from_ref(a)) {
                Ok(mut la) => {
                    out.append(&mut la);
                    true
                }
                Err(_) => false,
            }
        }
    }

    /// Mark the heap-element result for the correct RECURSIVE drop (DropListValue per `$__drop_value`
    /// for Value, DropListStr per-slot rc_dec for String) so scope-end / loop teardown frees each
    /// owned element — the leak-safety the cert-invisible per-element rc_inc relies on the drop for.
    /// Guard-clause flattening of the former else-if chain — each branch now returns
    /// directly instead of falling through to the shared tail expression, so
    /// every arm's OUTCOME (which set/map `dst` lands in) is unchanged. No behavior
    /// change — see docs/roadmap/active/code-health-codopsy.md.
    /// Extracted from `try_lower_concat_list` (codopsy round-2 complexity sweep, phase 3
    /// of 3): every guard moved verbatim in the original order; every arm of the original
    /// ended in `Some(dst)`, so the single caller returns `Some(dst)` unconditionally
    /// after this registration.
    fn register_concat_list_result_drop(
        &mut self,
        shape: ConcatListElemShape,
        elem_ty: &Ty,
        dst: ValueId,
    ) {
        if self.register_concat_list_block_or_tuple_drop(&shape, elem_ty, dst) {
            return;
        }
        self.register_concat_list_nested_or_variant_drop(shape, dst);
    }

    /// The FIRST run of the drop-registration guard chain — the single-block element
    /// families (String/Value, flat variant, all-scalar aggregate) and the two-slot
    /// TUPLE families, every guard moved verbatim in the original order. `true` when a
    /// guard fired (the original arm `return`ed here), so the second run never sees a
    /// shape the first already registered — the tried-in-original-order split of a
    /// guard router. Extracted from `try_lower_concat_list` (codopsy round-2 complexity
    /// sweep, phase 3 of 3, guard run 1 of 2).
    fn register_concat_list_block_or_tuple_drop(
        &mut self,
        shape: &ConcatListElemShape,
        elem_ty: &Ty,
        dst: ValueId,
    ) -> bool {
        if shape.heap_elem {
            if crate::lower::is_value_ty(elem_ty) {
                self.value_elem_lists.insert(dst);
            } else {
                self.heap_elem_lists.insert(dst);
            }
            return true;
        }
        if shape.flat_variant_elem {
            // A flat-variant element block owns no inner handle, so the per-element-`rc_dec`
            // `DropListStr` (each element + the list block) IS its full free — the SAME cert as a
            // `List[String]`. (Checked AFTER `heap_elem`, before the tuple/record arms.)
            self.heap_elem_lists.insert(dst);
            return true;
        }
        if shape.str_value_elem {
            self.str_value_elem_lists.insert(dst);
            return true;
        }
        if shape.str_str_elem {
            self.str_str_elem_lists.insert(dst);
            return true;
        }
        if shape.int_str_elem {
            self.variant_drop_handles.insert(dst, "list_int_str".to_string());
            return true;
        }
        if shape.str_int_elem {
            self.variant_drop_handles.insert(dst, "list_str_int".to_string());
            return true;
        }
        if shape.scalar_aggregate_elem {
            self.heap_elem_lists.insert(dst);
            return true;
        }
        false
    }

    /// The SECOND run of the drop-registration guard chain — the nested-list families
    /// and the generated recursive-drop routes (closure, rich variant, record), every
    /// guard moved verbatim in the original order after the first run's guards.
    /// A no-guard-fires fall-through registers nothing, exactly like the original's
    /// final `Some(dst)` tail (the scalar-element case). Extracted from
    /// `try_lower_concat_list` (codopsy round-2 complexity sweep, phase 3 of 3,
    /// guard run 2 of 2).
    fn register_concat_list_nested_or_variant_drop(
        &mut self,
        shape: ConcatListElemShape,
        dst: ValueId,
    ) {
        if shape.list_str_elem || shape.list_scalar_aggregate_elem {
            // Two-level: the nested sweep frees each inner element, each inner
            // block, then the outer — exact for a scalar-only record element
            // too, whose own rc_dec is its full free (#888).
            self.list_list_str_lists.insert(dst);
            return;
        }
        if shape.flat_list_elem {
            // Flat inner blocks: per-slot rc_dec is each element's FULL free.
            self.heap_elem_lists.insert(dst);
            return;
        }
        if shape.closure_elem {
            // Per-element recursive free via `$__drop_list_closure` → `__drop_closure`.
            self.variant_drop_handles.insert(dst, "list_closure".to_string());
            return;
        }
        if let Some(vname) = shape.rich_variant_elem {
            // RECURSIVE per-element drop via `$__drop_list_<V>` (the generated variant list drop).
            self.variant_drop_handles.insert(dst, format!("list_{vname}"));
            return;
        }
        if let Some(rname) = shape.record_elem {
            self.variant_drop_handles.insert(dst, format!("list_{rname}"));
        }
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
        let mut outcome = None;
        if outcome.is_none() {
            outcome = self.lower_call_arg_leaf(a, out)?;
        }
        if outcome.is_none() {
            outcome = self.lower_call_arg_aggregate(a, out)?;
        }
        if outcome.is_none() {
            outcome = self.lower_call_arg_operator(a, out)?;
        }
        if outcome.is_none() {
            outcome = self.lower_call_arg_call(a, out)?;
        }
        match outcome {
            Some(ArgOutcome::Value(arg)) => {
                out.push(arg);
                Ok(())
            }
            Some(ArgOutcome::Pushed) => Ok(()),
            None => Err(LowerError::Unsupported(format!(
                "call argument {} not in this brick",
                kind_name(&a.kind)
            ))),
        }
    }
}

include!("calls_arg.rs");
