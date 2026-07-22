impl LowerCtx {

    /// Materialize both operands of a heap `==` into owned blocks (in the current cond frame) and
    /// emit the typed equality, returning the Bool ValueId. Each operand is materialized by
    /// `materialize_eq_operand` (a tracked Var is BORROWED, a fresh heap value is an owned temp
    /// added to `live_heap_handles` with its recursive drop set). The eq BORROWS the operand
    /// handles (it only reads), so the owned temps survive to the frame teardown. The per-type
    /// dispatch is the ONE recursive engine [`Self::typed_slot_eq`]; an unsupported shape returns
    /// None and the caller rolls back (walls) — never wrong bytes.
    pub(crate) fn lower_heap_eq_typed_materialized(
        &mut self,
        left: &IrExpr,
        right: &IrExpr,
        ty: &Ty,
    ) -> Option<ValueId> {
        let lb = self.materialize_eq_operand(left, ty)?;
        let rb = self.materialize_eq_operand(right, ty)?;
        let lh = self.handle_of(lb);
        let rh = self.handle_of(rb);
        self.typed_slot_eq(lh, rh, ty, 0)
    }

    /// Recursive typed equality over two 8-byte SLOT VALUES of type `ty` — the ONE eq engine
    /// every `==`/`!=` compose routes through (the unit-if cond, the value-position BinOp, and
    /// every nested payload/field recursion). A scalar slot holds the value itself (int-class
    /// i64 / float-class f64 bits); a heap slot holds the block's byte-address HANDLE, read
    /// through its layout:
    ///   String/Value/List[T] → the borrowed module eq call; Option/Result → the tag-compare
    ///   cores; tuple/record → per-slot recursion ANDed; custom variant → tag eq + a
    ///   tag-dispatched per-field recursion chain.
    /// Every path only READS (borrows) — no ownership events — so the recursion composes freely
    /// inside branch merges, and a `None` anywhere propagates to the caller's rollback (the
    /// unlowered shape walls, never wrong output). `depth` caps type-level recursion (a variant
    /// containing itself — `Cons(Int, MyList)` — would otherwise recurse forever at COMPILE
    /// time); a capped shape walls, honest (runtime-recursive eq needs a synthesized fn brick).
    /// The `String`/`Value`/`List[T]` borrowed-handle module-eq call name for
    /// [`Self::typed_slot_eq`]'s slot classification — a pure, `self`-free lookup keyed on
    /// `ty` alone (no ownership events, matching the doc above). Verbatim extraction
    /// (guard-clause flattening) of the former inline if-else-if chain, no behavior change —
    /// see docs/roadmap/active/code-health-codopsy.md.
    fn module_eq_call_name(ty: &Ty) -> Option<&'static str> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        if matches!(ty, Ty::String) {
            return Some("string.eq");
        }
        if crate::lower::is_value_ty(ty) {
            return Some("value.eq");
        }
        let Ty::Applied(TC::List, es) = ty else {
            return None;
        };
        if es.len() != 1 {
            return None;
        }
        Self::list_elem_eq_call_name(&es[0])
    }

    /// The `List[T]` ELEMENT classification half of [`Self::module_eq_call_name`] — split out
    /// (rather than left nested) so neither function's guard-clause chain re-exceeds the
    /// depth this extraction exists to fix. Verbatim extraction, no behavior change.
    fn list_elem_eq_call_name(elem_ty: &Ty) -> Option<&'static str> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        if matches!(elem_ty, Ty::Int) {
            return Some("list.eq_int");
        }
        if matches!(elem_ty, Ty::String) {
            return Some("list.eq_str");
        }
        if matches!(elem_ty, Ty::Float) {
            return Some("list.eq_float");
        }
        if matches!(elem_ty, Ty::Bool) {
            return Some("list.eq_bool");
        }
        if crate::lower::is_value_ty(elem_ty) {
            return Some("list.eq_value");
        }
        if let Ty::Applied(TC::List, inner) = elem_ty {
            // Nested lists — the element-wise recursion into the flat list eq
            // (value_core `list_eq_list_*`).
            if inner.len() == 1 && matches!(inner[0], Ty::Int) {
                return Some("list.eq_list_int");
            }
            if inner.len() == 1 && matches!(inner[0], Ty::Float) {
                return Some("list.eq_list_float");
            }
            if inner.len() == 1 && matches!(inner[0], Ty::String) {
                return Some("list.eq_list_str");
            }
            return None;
        }
        if let Ty::Applied(TC::Option, inner) = elem_ty {
            // List[Option[Int/Bool]] — element-wise len-as-tag + i64 payload
            // compare (value_core `list_eq_opt_int`). Scalar payloads only: a
            // Float payload's slot is f64 BITS (bit-eq ≠ `==` on -0.0/NaN).
            if inner.len() == 1 && matches!(inner[0], Ty::Int | Ty::Bool) {
                return Some("list.eq_opt_int");
            }
            return None;
        }
        if let Ty::Tuple(ts) = elem_ty {
            // List[Tuple[scalar…]] — a scalar tuple block is LAYOUT-IDENTICAL to a
            // same-arity List of its slot class (len@4 = arity, 8-byte slots @12),
            // so the nested-list eq of the matching class compares it exactly:
            // Int/Bool slots bit-compare (list.eq_list_int), all-Float slots
            // float-compare per slot (list.eq_list_float). A MIXED Int/Float
            // tuple has no matching flat class (a bit-compare on the Float slot
            // is wrong on -0.0/NaN) — decline, the eq site walls honestly.
            if !ts.is_empty() && ts.iter().all(|t| matches!(t, Ty::Int | Ty::Bool)) {
                return Some("list.eq_list_int");
            }
            if !ts.is_empty() && ts.iter().all(|t| matches!(t, Ty::Float)) {
                return Some("list.eq_list_float");
            }
            return None;
        }
        None
    }

    pub(crate) fn typed_slot_eq(
        &mut self,
        lv: ValueId,
        rv: ValueId,
        ty: &Ty,
        depth: u32,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        const MAX_EQ_DEPTH: u32 = 8;
        if depth > MAX_EQ_DEPTH {
            return None;
        }
        // Scalars — the slot IS the value.
        if Self::float_operand_ty(ty) {
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: crate::PrimKind::FloatCmp(crate::FCmpOp::Eq),
                dst: Some(dst),
                args: vec![lv, rv],
            });
            return Some(dst);
        }
        if Self::int_eq_operand_ty(ty) {
            let dst = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst, op: IntOp::Eq, a: lv, b: rv });
            return Some(dst);
        }
        // String / Value / List[T] — the borrowed-handle module eq call.
        let module_eq: Option<&str> = Self::module_eq_call_name(ty);
        if let Some(name) = module_eq {
            let dst = self.fresh_value();
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: name.to_string(),
                args: vec![CallArg::Handle(lv), CallArg::Handle(rv)],
                result: Some(repr_of(&Ty::Bool).ok()?),
            });
            return Some(dst);
        }
        // A `List[<custom variant>]` — the synthesized loop helper (the element
        // eq is the variant helper; a non-variant element stayed in the module
        // table above). Generated once per parent fn, called at the site.
        if let Ty::Applied(TC::List, es) = ty {
            if es.len() == 1 {
                if let Some(elem_name) = self.custom_variant_type_name(&es[0]) {
                    if let Some(elem_layout) =
                        self.variant_layouts.by_type.get(&elem_name).cloned()
                    {
                        let (elem_key, elem_inst) =
                            instantiate_variant_layout(&elem_name, &elem_layout, &es[0]);
                        if self.ensure_list_eq_helper(&elem_key, &elem_inst) {
                            let name = self.list_eq_helper_name(&elem_key);
                            return Some(self.emit_eq_helper_call(name, lv, rv));
                        }
                    }
                    return None;
                }
            }
        }
        // A custom VARIANT — a RECURSIVE one (or one already being generated)
        // routes through its synthesized helper (recursion-by-call; the static
        // inline could never terminate); everything else keeps the proven
        // inline tag-dispatch chain, byte-identical to before.
        if let Some(tyname) = self.custom_variant_type_name(ty) {
            let layout = self.variant_layouts.by_type.get(&tyname).cloned();
            if let Some(layout) = layout {
                // A GENERIC variant instantiates per-use (`Tree[Int]`): the
                // helper key carries the args and the layout's field types are
                // substituted, so `Leaf(T)` compares as `Leaf(Int)`.
                let (key, layout) = instantiate_variant_layout(&tyname, &layout, ty);
                if self.synth_eq_types.contains(&key) || self.variant_needs_eq_helper(&tyname) {
                    if self.ensure_variant_eq_helper(&key, &layout) {
                        let name = self.eq_helper_name(&key);
                        return Some(self.emit_eq_helper_call(name, lv, rv));
                    }
                    return None;
                }
                return self.variant_eq_from_handles(lv, rv, &layout, depth);
            }
        }
        // Option[T] — the scalar masked compare or the heap conditional compare.
        if let Ty::Applied(TC::Option, oa) = ty {
            if oa.len() == 1 {
                if !is_heap_ty(&oa[0]) {
                    return Some(self.option_scalar_eq_from_handles(lv, rv, &oa[0]));
                }
                return self.option_heap_eq_from_handles(lv, rv, &oa[0], depth);
            }
        }
        // Result[T, E] — the proven masked core for the (scalar Ok, String Err) layout;
        // the general both-Ok/both-Err conditional recursion for every other payload pair.
        if let Ty::Applied(TC::Result, ra) = ty {
            if ra.len() == 2 {
                if !is_heap_ty(&ra[0]) && matches!(ra[1], Ty::String) {
                    return self.result_scalar_eq_from_handles(lv, rv, &ra[0]);
                }
                return self.result_eq_general_from_handles(lv, rv, &ra[0], &ra[1], depth);
            }
        }
        // Tuple / record — per-slot recursion, ANDed in declaration order.
        if let Some((_names, ftys)) = self.aggregate_field_tys(ty) {
            return self.aggregate_eq_from_handles(lv, rv, &ftys, depth);
        }
        None
    }

    /// Tuple/record `==` over two materialized block HANDLES: load each declaration-order
    /// uniform slot (a heap field as its owned handle, a scalar as its value), recurse the
    /// typed eq per field, AND-fold. An empty aggregate compares equal.
    fn aggregate_eq_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        ftys: &[Ty],
        depth: u32,
    ) -> Option<ValueId> {
        let mut acc: Option<ValueId> = None;
        for (i, fty) in ftys.iter().enumerate() {
            let off = crate::lower::layout::slot_offset(i) as i64;
            let (lf, rf) = if is_heap_ty(fty) {
                (self.load_payload_addr(hl, off), self.load_payload_addr(hr, off))
            } else {
                (
                    self.load_at_offset(hl, off, crate::PrimKind::Load { width: 8 }),
                    self.load_at_offset(hr, off, crate::PrimKind::Load { width: 8 }),
                )
            };
            let e = self.typed_slot_eq(lf, rf, fty, depth + 1)?;
            acc = Some(match acc {
                None => e,
                Some(prev) => {
                    let dst = self.fresh_value();
                    self.ops.push(Op::IntBinOp { dst, op: IntOp::And, a: prev, b: e });
                    dst
                }
            });
        }
        Some(acc.unwrap_or_else(|| {
            let dst = self.fresh_value();
            self.ops.push(Op::ConstInt { dst, value: 1 });
            dst
        }))
    }

    /// Custom-variant `==` over two materialized block HANDLES (`[tag@slot0][field0@slot1]…`):
    /// tag-eq AND a tag-dispatched field-compare chain — each field-carrying ctor's fields
    /// recurse through the typed eq (ANDed), fieldless ctors compare true. All merge values are
    /// scalar Bools — the IfThen/Else/EndIf merges carry no ownership. A field whose typed eq
    /// cannot lower (e.g. a recursive `List[Self]` payload) propagates None → the caller walls.
    pub(crate) fn variant_eq_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        layout: &crate::lower::VariantLayout,
        depth: u32,
    ) -> Option<ValueId> {
        let toff = crate::lower::layout::slot_offset(0) as i64;
        let tl = self.load_at_offset(hl, toff, crate::PrimKind::Load { width: 8 });
        let tr = self.load_at_offset(hr, toff, crate::PrimKind::Load { width: 8 });
        let t_eq = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: t_eq, op: IntOp::Eq, a: tl, b: tr });
        // Field chain under EQUAL tags: nested merges over the field-carrying ctors; the
        // fieldless remainder compares true.
        let fielded: Vec<(i64, Vec<Ty>)> = layout
            .cases
            .iter()
            .filter(|c| !c.fields.is_empty())
            .map(|c| (c.tag as i64, c.fields.iter().map(|(_, t)| t.clone()).collect()))
            .collect();
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: t_eq, dst: Some(dst) });
        // then-branch: the chain value.
        let mut ends: Vec<ValueId> = Vec::new();
        for (tag, ftys) in &fielded {
            let tagv = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: tagv, value: *tag });
            let is_c = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: is_c, op: IntOp::Eq, a: tl, b: tagv });
            let d2 = self.fresh_value();
            self.ops.push(Op::IfThen { cond: is_c, dst: Some(d2) });
            // This ctor's fields at slots 1.. — recurse the typed eq per field, AND-fold.
            let mut cmp: Option<ValueId> = None;
            for (j, fty) in ftys.iter().enumerate() {
                let foff = crate::lower::layout::slot_offset(1 + j) as i64;
                let (l1, r1) = if is_heap_ty(fty) {
                    (self.load_payload_addr(hl, foff), self.load_payload_addr(hr, foff))
                } else {
                    (
                        self.load_at_offset(hl, foff, crate::PrimKind::Load { width: 8 }),
                        self.load_at_offset(hr, foff, crate::PrimKind::Load { width: 8 }),
                    )
                };
                let f_eq = self.typed_slot_eq(l1, r1, fty, depth + 1)?;
                cmp = Some(match cmp {
                    None => f_eq,
                    Some(prev) => {
                        let a2 = self.fresh_value();
                        self.ops.push(Op::IntBinOp { dst: a2, op: IntOp::And, a: prev, b: f_eq });
                        a2
                    }
                });
            }
            self.ops.push(Op::Else { val: Some(cmp?) });
            ends.push(d2);
        }
        // innermost else: no field-ctor matched (a fieldless ctor) — equal.
        let one = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one, value: 1 });
        let mut inner: ValueId = one;
        // close the nested merges inside-out.
        for d2 in ends.iter().rev() {
            self.ops.push(Op::EndIf { val: Some(inner) });
            inner = *d2;
        }
        // The chain's value is the OUTERMOST merge (`ends[0]`): each nested if's
        // dst is only assigned along the path that reaches it, so yielding an
        // inner dst reads an unassigned local whenever an OUTER arm was taken
        // (first-ctor eq returned false with ≥2 fielded ctors).
        let then_v = inner;
        let zero = self.fresh_value();
        self.ops.push(Op::Else { val: Some(then_v) });
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::EndIf { val: Some(zero) });
        Some(dst)
    }

    /// General `Result[T, E] ==` over two materialized block HANDLES (len@4 = 0 Ok / 1 Err,
    /// payload@12) for payload pairs beyond the proven (scalar, String) masked core. Two SIBLING
    /// gated compares — dst = (bothOk ? okEq : 0) | (bothErr ? errEq : 0) — so a payload slot is
    /// only ever read INSIDE the branch that knows both sides carry that variant (a heap handle
    /// is never dereferenced against the wrong layout), and a mixed Ok/Err pair compares false.
    fn result_eq_general_from_handles(
        &mut self,
        hl: ValueId,
        hr: ValueId,
        ok_ty: &Ty,
        err_ty: &Ty,
        depth: u32,
    ) -> Option<ValueId> {
        // TWO Result layouts share this eq: a SCALAR-Ok Result is len-as-tag (@4: 0 = Ok,
        // 1 = Err), but a HEAP-Ok Result is the cap-as-tag 1-slot block (len@4 is ALWAYS 1;
        // the Ok/Err tag lives in slot 0's HIGH 32 bits @16 — materialize_result_str).
        // Reading @4 on the heap-Ok layout classified EVERY value as Err, so
        // `ok(xs) == ok(ys)` string-compared the two payload HANDLES (false for equal lists).
        let tag_off: i64 = if is_heap_ty(ok_ty) { 16 } else { 4 };
        let tag_l = self.load_at_offset(hl, tag_off, crate::PrimKind::Load { width: 4 });
        let tag_r = self.load_at_offset(hr, tag_off, crate::PrimKind::Load { width: 4 });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        let ok_l = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: ok_l, op: IntOp::Eq, a: tag_l, b: zero });
        let ok_r = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: ok_r, op: IntOp::Eq, a: tag_r, b: zero });
        let both_ok = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both_ok, op: IntOp::And, a: ok_l, b: ok_r });
        let both_err = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: both_err, op: IntOp::And, a: tag_l, b: tag_r });
        // Gated Ok compare.
        let ok_gate = self.fresh_value();
        self.ops.push(Op::IfThen { cond: both_ok, dst: Some(ok_gate) });
        let (pl, pr) = if is_heap_ty(ok_ty) {
            (self.load_payload_addr(hl, 12), self.load_payload_addr(hr, 12))
        } else {
            (
                self.load_at_offset(hl, 12, crate::PrimKind::Load { width: 8 }),
                self.load_at_offset(hr, 12, crate::PrimKind::Load { width: 8 }),
            )
        };
        let ok_eq = self.typed_slot_eq(pl, pr, ok_ty, depth + 1)?;
        self.ops.push(Op::Else { val: Some(ok_eq) });
        let f1 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: f1, value: 0 });
        self.ops.push(Op::EndIf { val: Some(f1) });
        // Gated Err compare.
        let err_gate = self.fresh_value();
        self.ops.push(Op::IfThen { cond: both_err, dst: Some(err_gate) });
        let (el, er) = if is_heap_ty(err_ty) {
            (self.load_payload_addr(hl, 12), self.load_payload_addr(hr, 12))
        } else {
            (
                self.load_at_offset(hl, 12, crate::PrimKind::Load { width: 8 }),
                self.load_at_offset(hr, 12, crate::PrimKind::Load { width: 8 }),
            )
        };
        let err_eq = self.typed_slot_eq(el, er, err_ty, depth + 1)?;
        self.ops.push(Op::Else { val: Some(err_eq) });
        let f2 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: f2, value: 0 });
        self.ops.push(Op::EndIf { val: Some(f2) });
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op: IntOp::Or, a: ok_gate, b: err_gate });
        Some(dst)
    }

    /// The byte-address (`Prim::Handle`) of a materialized block — the operand handed to an eq core.
    pub(crate) fn handle_of(&mut self, block: ValueId) -> ValueId {
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![block] });
        h
    }

    /// LoadHandle a nested payload slot AND normalize it to the eq engine's i64
    /// byte-ADDRESS form (`Prim::Handle`): a raw `LoadHandle` result is an i32 Ptr
    /// local, but every structural recursion (`load_at_offset`'s IntBinOp address
    /// math) consumes handles as i64 — mixing the classes emitted
    /// `(i64.add (local.get $v:i32))`, invalid wasm (the Option[(Int, String)] eq).
    /// Module-eq callees (`string.eq`/`list.eq_*`) accept the i64 form through
    /// `render_arg_wasm`'s Handle wrap, so normalizing is uniformly safe.
    pub(crate) fn load_payload_addr(&mut self, h: ValueId, off: i64) -> ValueId {
        let raw = self.load_at_offset(h, off, crate::PrimKind::LoadHandle);
        self.handle_of(raw)
    }

    /// Materialize ONE operand of a heap `==` cond into a block whose handle the eq core reads.
    /// - A tracked heap `Var` is BORROWED: its existing block is returned, NOT added to the cond
    ///   frame (it is owned elsewhere and drops at its own scope — the eq only reads it).
    /// - Any other heap operand (a heap-returning CALL, an Option/Result CTOR, a String literal /
    ///   concat) is materialized into a FRESH OWNED block via `lower_owned_heap_field`, which
    ///   pushes it to `live_heap_handles`; we then register its RECURSIVE drop set
    ///   (`heap_elem_lists` / `value_handles` / …) so the cond-frame `drop_arm_locals` frees it
    ///   AND its owned payload (no leak). The owned temp is freed exactly once (the frame `d`),
    ///   never double-freed (the eq borrows it, never consumes). Returns None for a non-
    ///   materializable shape → the caller walls.
    fn materialize_eq_operand(&mut self, expr: &IrExpr, ty: &Ty) -> Option<ValueId> {
        if let IrExprKind::Var { id } = &expr.kind {
            // A tracked heap Var (a param or let-local) — borrow its block, no new ownership.
            let v = self.value_for(*id).ok()?;
            // The eq READS the block through the operand type's layout, so a DEFERRED
            // Opaque bind (any heap type — a variant/tuple/record slot read of an empty
            // block is a raw un-bounds-checked load, garbage compared silently) must
            // decline on the strict path: the eq walls and v0 emits the correct bytes
            // (the C-136 wrong-value family, consumer side).
            if crate::lower::strict_values() && self.deferred_opaque_binds.contains(&v) {
                return None;
            }
            // An Option/Result var must additionally be a genuinely MATERIALIZED block
            // (ctor-built, or a param seeded by `seed_variant_param`). A deferred value
            // that escaped the bind set — e.g. an optional-chain result outside the
            // executable subset — has len 0, which the len-as-tag read MISREADS as
            // `none`/`Ok` (`assert_eq(p?.name, some("Alice"))` compared false while
            // native compared true — a silent wrong verdict, the #790 optional-chain
            // row). Decline instead.
            use almide_lang::types::constructor::TypeConstructorId as TC;
            if matches!(ty, Ty::Applied(TC::Option | TC::Result, _))
                && !self.materialized_options.contains(&v)
                && !self.materialized_results.contains(&v)
                && !self.materialized_results_str.contains(&v)
            {
                return None;
            }
            return Some(v);
        }
        // An Option/Result CTOR (`some("")`, `none`, `ok(v)`, `err(m)`) — `try_lower_option_ctor`
        // (which handles BOTH Option and Result ctors) builds the DynListStr/OptSome block +
        // registers its recursive drop set (`materialize_opt_str_some` → `heap_elem_lists` +
        // `materialized_options`), but does NOT push to `live_heap_handles`. Push it so the cond-
        // frame `drop_arm_locals` frees it (and its owned payload) exactly once after the
        // (borrowing) eq.
        if matches!(
            &expr.kind,
            IrExprKind::OptionSome { .. }
                | IrExprKind::OptionNone
                | IrExprKind::ResultOk { .. }
                | IrExprKind::ResultErr { .. }
        ) {
            let obj = self.try_lower_option_ctor(expr, ty)?;
            if !self.live_heap_handles.contains(&obj) {
                self.live_heap_handles.push(obj);
            }
            return Some(obj);
        }
        // `rs[i] == ok(v)` — an ELEMENT of a materialized heap-element list (the
        // fan.settle results literal): BORROW the element handle (`$elem_addr` +
        // `LoadHandle` — the list owns it and the eq only reads), the Var-borrow
        // discipline element-precise. Nothing joins the cond frame — no drop, no
        // ownership event; an untracked/deferred container declines inside the
        // borrow and falls through to the materializer below.
        if let IrExprKind::IndexAccess { .. } = &expr.kind {
            if let Some(b) = self.try_lower_heap_field_borrow(expr) {
                return Some(b);
            }
        }
        // Otherwise a heap-returning CALL / literal / concat — materialize a fresh OWNED block
        // (`lower_owned_heap_field` pushes it to `live_heap_handles`). The call path leaves the
        // block FLAT, so register the recursive drop set from the operand TYPE — else an
        // `Option[String]`/`List[String]` temp leaks its inner Strings. Idempotent.
        let obj = self.lower_owned_heap_field(expr)?;
        if self.live_heap_handles.contains(&obj) {
            self.register_owned_heap_eq_drop(obj, ty);
        }
        Some(obj)
    }

    /// Register the recursive drop set for a freshly materialized heap eq-operand block, mirroring
    /// the call-binding tracking in `lower_bind` so the cond-frame teardown frees nested ownership.
    pub(crate) fn register_owned_heap_eq_drop(&mut self, obj: ValueId, ty: &Ty) {
        if crate::lower::is_list_list_str_ty(ty) {
            self.list_list_str_lists.insert(obj);
        } else if crate::lower::is_list_str_str_ty(ty) {
            self.str_str_elem_lists.insert(obj);
        } else if crate::lower::is_list_int_str_ty(ty) {
            self.variant_drop_handles.insert(obj, "list_int_str".to_string());
        } else if crate::lower::is_lenlist_list_ty(ty) {
            self.variant_drop_handles.insert(obj, "list_lenlist".to_string());
        } else if is_heap_elem_list_ty(ty) {
            // List[heap] / Option[heap] / Result[_, heap] — the DynListStr recursive free.
            self.heap_elem_lists.insert(obj);
        }
        if crate::lower::is_value_ty(ty) {
            self.value_handles.insert(obj);
        }
    }

    fn lower_heap_result_if_inner(
        &mut self,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let cond_v = self.lower_heap_result_cond(cond)?;
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: cond_v, dst: Some(dst) });
        // RELEASE PARITY across the arms. An arm may MOVE an OUTER scope-level
        // handle into its result — the effect tail's `err(msg)` moves the error
        // accumulator into the Err block — which removes it from
        // `live_heap_handles` GLOBALLY, though the move runs only on that arm's
        // PATH. The sibling arm must then release it ITSELF: without the
        // compensating Drop the non-moving path LEAKS the handle (one error
        // accumulator per call on the happy path). The flat certificate hid this
        // by counting the moving arm's `m` unconditionally; the branch-grouped
        // cert (`{m|}` — arms disagree) REJECTS it, which is how it was found.
        // With the Drop the arms agree (`{m|d}`) and the leak is gone. Nested
        // heap-result ifs recurse through here, so parity holds level by level.
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        let then_obj = self.lower_heap_result_arm(then, result_ty)?;
        let consumed_by_then: Vec<ValueId> =
            outer.iter().copied().filter(|h| !self.live_heap_handles.contains(h)).collect();
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_obj) });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let else_obj = self.lower_heap_result_arm(else_, result_ty)?;
        let consumed_by_else: Vec<ValueId> = live_after_then
            .iter()
            .copied()
            .filter(|h| !self.live_heap_handles.contains(h))
            .collect();
        for h in &consumed_by_then {
            if !consumed_by_else.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.push(op); // the ELSE arm releases what THEN moved out
            }
        }
        for h in &consumed_by_else {
            if !consumed_by_then.contains(h) {
                let op = self.drop_op_for(*h);
                self.ops.insert(else_marker_at, op); // the THEN arm releases what ELSE moved out
            }
        }
        self.ops.push(Op::EndIf { val: Some(else_obj) });
        Some(dst)
    }
}
