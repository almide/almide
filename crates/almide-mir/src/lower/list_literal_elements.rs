/// What ONE drop-class arm of [`LowerCtx::lower_record_list_element`] decided about an
/// element expression. The three states are exactly the three exits those arms already
/// had while they were an inline if-chain in that function, and — as in
/// `lower_owned_heap_field`'s group table — the two negatives are DISTINCT and must stay
/// so: `Decline` is an arm REFUSING this element (the caller returns `None` and its own
/// caller walls), while `Fallthrough` means the arm had nothing to say about this element
/// shape, so the generic `lower_owned_heap_field` tail still runs. Collapsing them either
/// walls a shape that used to lower, or lowers a refused shape by the wrong rule.
/// Extracted from `lower_record_list_element` (codopsy r2 #852 complexity sweep).
enum ListElemArm {
    /// The arm materialized the element as a fresh OWNED block; the caller tracks the
    /// handle and yields it (`Some(Some(obj))`).
    Built(ValueId),
    /// The arm refuses this element shape — the caller declines the whole literal.
    Decline,
    /// No arm applies — the caller continues with the generic owned-field path.
    Fallthrough,
}

impl LowerCtx {
    /// `xs[i]` as a list-literal ELEMENT — an OWNED handle on the element block.
    ///
    /// The container still owns the element, so the load is a BORROW and the
    /// `Dup` (rc_inc) is what makes the returned handle owned: the enclosing
    /// literal stores it and `Consume`s it, and that literal's own drop releases
    /// the reference. Exactly the ownership the let-bound spelling produces
    /// (`let c = xs[i]; acc + [c]`), which is why this shape is safe — it is the
    /// same `a`+`d` pair, just without the user having to write the binding
    /// (#888).
    ///
    /// The address arithmetic is the element-slot form the per-element for-in
    /// loop already uses: `handle + 12 + i*8`. A non-heap element or a container
    /// that does not lower to a single handle declines, leaving the caller's
    /// wall in place.
    fn lower_list_index_element(&mut self, e: &IrExpr) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let IrExprKind::IndexAccess { object, index } = &e.kind else { return None };
        if !crate::lower::is_heap_ty(&e.ty) {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let container = match self.lower_call_args(std::slice::from_ref(&**object)) {
            Ok(args) => match args.into_iter().next() {
                Some(crate::CallArg::Handle(v)) => v,
                _ => {
                    self.ops.truncate(ops_mark);
                    self.live_heap_handles.truncate(lhh_mark);
                    return None;
                }
            },
            Err(_) => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        let Some(idx) = self.lower_scalar_value(index) else {
            self.ops.truncate(ops_mark);
            self.live_heap_handles.truncate(lhh_mark);
            return None;
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![container] });
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let scaled = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: scaled, op: IntOp::Mul, a: idx, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: scaled });
        let borrowed = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(borrowed), args: vec![addr] });
        let owned = self.fresh_value();
        self.ops.push(Op::Dup { dst: owned, src: borrowed });
        Some(owned)
    }

    /// Lower one element of a record-list literal to an OWNED handle.
    ///
    /// `None` from the outer `Option` declines the whole literal, so the caller
    /// walls rather than storing a wrong shape. `Some(None)` means the element
    /// materialized itself and already registered its own handle, so the caller
    /// adds nothing.
    fn lower_record_list_element(
        &mut self,
        e: &IrExpr,
        forced_elem: Option<&Ty>,
        elem_ty: &Ty,
        kind: ListElemDrop,
    ) -> Option<Option<ValueId>> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        // When the element type is forced (a structural record LITERAL in a `List[Named]` context),
        // materialize the element AS the Named type so `try_lower_record_construct` lays it out by
        // the DECLARED field order (matching the `$__drop_list_<Named>` teardown). Field-by-name
        // assignment makes this order-correct regardless of the literal's source field order.
        let forced_e;
        let e_ref = match forced_elem {
            Some(ft) if matches!(e.kind, IrExprKind::Record { .. }) => {
                forced_e = IrExpr { ty: ft.clone(), ..e.clone() };
                &forced_e
            }
            _ => e,
        };
        // A BLOCK element — the heap-if-argument ANF (and its desugar
        // siblings) wraps an element in a Block carrying its `let`s
        // (`{ let t = if …; color(text(…), t) }`, the ceangal todo_item
        // chain, #881). Lower the statements as effects in an element-local
        // frame, then the TAIL as the element itself. The block's own heap
        // lets are freed within the frame (the built element co-owns what it
        // needs via its own Dup/moves), and the element object's tracking is
        // restored afterwards so the caller's uniform Consume still sees it.
        if matches!(e_ref.kind, IrExprKind::Block { .. }) {
            return self.lower_block_list_element(e_ref, forced_elem, elem_ty, kind);
        }
        // An INDEX-READ element (`acc = acc + [xs[i]]` — snaidhm's ttf
        // `expand_implied` rotate, ceangal's zip_view_rects; #888/#904). The
        // element TYPE was already accepted by the concat gate; only this
        // EXPRESSION shape had no arm, so the append declined, the enclosing
        // `for` fell back to model-one-iteration, and the whole heap-result
        // `if` walled. Writing it as `let c = xs[i]` then `acc + [c]` always
        // worked — this makes the inline spelling reach the same lowering.
        if matches!(e_ref.kind, IrExprKind::IndexAccess { .. }) {
            if let Some(obj) = self.lower_list_index_element(e_ref) {
                return self.track_list_element_handle(obj);
            }
            return None;
        }
        // A CTOR-class element (`some(1)`, `err("x")`) materializes through the Option/Result
        // ctor builder (a fresh OWNED wrapper block; the ctor arms leave tracking to callers,
        // so push it for the uniform Consume below). A Var/call element of the SAME type takes
        // `lower_owned_heap_field` (Dup / fresh CallFn result) — the drop class is TYPE-driven,
        // so both produce blocks the registered list drop frees exactly.
        match self.lower_classified_list_element(e_ref, elem_ty, &kind) {
            ListElemArm::Built(obj) => return self.track_list_element_handle(obj),
            ListElemArm::Decline => return None,
            ListElemArm::Fallthrough => {}
        }
        return Some(Some(self.lower_owned_heap_field(e_ref)?));
        Some(Some(self.lower_owned_heap_field(e_ref)?))
    }

    /// Register a freshly built element handle as live and yield it as
    /// [`Self::lower_record_list_element`]'s result. Extracted verbatim from that fn
    /// (codopsy r2 #852 complexity sweep) — it is the identical two lines every arm ran
    /// immediately before returning its object, so hoisting them here changes nothing
    /// about the emitted op sequence.
    fn track_list_element_handle(&mut self, obj: ValueId) -> Option<Option<ValueId>> {
        if !self.live_heap_handles.contains(&obj) {
            self.live_heap_handles.push(obj);
        }
        Some(Some(obj))
    }

    /// Route an element to the arm its DROP CLASS selects. Extracted from
    /// `lower_record_list_element` (codopsy r2 #852 complexity sweep): the arms were a
    /// chain of `if matches!(kind, …)` blocks there, and since the classes they test are
    /// pairwise disjoint, the chain and this `match` admit exactly the same elements in
    /// exactly the same order. A class with no arm (`Record`, `StrStr`, `MapMlo`,
    /// `MapHval`, `StrClosure`) falls through to the generic owned-field path, as before.
    fn lower_classified_list_element(
        &mut self,
        e_ref: &IrExpr,
        elem_ty: &Ty,
        kind: &ListElemDrop,
    ) -> ListElemArm {
        match kind {
            ListElemDrop::ListStr => self.lower_inner_str_list_element(e_ref, elem_ty),
            ListElemDrop::ScalarAggregate => self.lower_scalar_aggregate_element(e_ref),
            ListElemDrop::RecordInt(_) => self.lower_record_int_tuple_element(e_ref, elem_ty),
            ListElemDrop::StrInt
            | ListElemDrop::IntStr
            | ListElemDrop::StrVariant(_)
            | ListElemDrop::StrMapStr
            | ListElemDrop::StrMapSkv
            | ListElemDrop::StrListOpt => self.lower_heap_pair_tuple_element(e_ref),
            ListElemDrop::Closure => self.lower_lambda_list_element(e_ref, elem_ty),
            ListElemDrop::CtorFlat | ListElemDrop::CtorLenLoop => {
                self.lower_option_ctor_element(e_ref, elem_ty)
            }
            _ => ListElemArm::Fallthrough,
        }
    }

    /// The `ListStr` (`List[List[String]]`) element arm of
    /// [`Self::lower_record_list_element`], extracted verbatim (codopsy r2 #852 complexity
    /// sweep): it decides whether an inner string-list element builds through the str-list
    /// builder, declines the literal, or leaves the element to the generic owned-field path.
    fn lower_inner_str_list_element(&mut self, e_ref: &IrExpr, elem_ty: &Ty) -> ListElemArm {
        // An inner `List[String]` LITERAL element builds through the str-list
        // builder (fresh owned, tracked by it); a Var/call element of the exact
        // element type takes the generic owned-field path below. A type-rewritten
        // (never-err-lifted) element declines — the same guard as the ctor class.
        if matches!(e_ref.kind, IrExprKind::List { .. }) {
            if let Some(obj) = self.try_lower_str_list_literal(e_ref) {
                return ListElemArm::Built(obj);
            }
            return ListElemArm::Decline;
        }
        if e_ref.ty != *elem_ty {
            return ListElemArm::Decline;
        }
        ListElemArm::Fallthrough
    }

    /// The `ScalarAggregate` element arm of [`Self::lower_record_list_element`], extracted
    /// verbatim (codopsy r2 #852 complexity sweep): it decides which FLAT builder — the
    /// scalar-list slots one or the scalar-tuple one — materializes the element, and leaves
    /// any other element shape to the generic owned-field path.
    fn lower_scalar_aggregate_element(&mut self, e_ref: &IrExpr) -> ListElemArm {
        // An inner `List[<scalar>]` LITERAL element builds through the flat
        // slots builder (fresh owned; the uniform Consume below moves it in).
        if let IrExprKind::List { elements: iels } = &e_ref.kind {
            let iels = iels.clone();
            if let Some(obj) = self.try_lower_scalar_list_slots(&iels) {
                return ListElemArm::Built(obj);
            }
            return ListElemArm::Decline;
        }
        if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
            let tels = tels.clone();
            if let Some(obj) = self.try_lower_scalar_tuple_construct(&tels) {
                return ListElemArm::Built(obj);
            }
            return ListElemArm::Decline;
        }
        ListElemArm::Fallthrough
    }

    /// The `RecordInt` (`(<recursive-drop record>, <scalar>)`) element arm of
    /// [`Self::lower_record_list_element`], extracted verbatim (codopsy r2 #852 complexity
    /// sweep): it decides the record slot's LAYOUT TYPE before the tuple is built, and
    /// declines every element this class does not construct itself.
    fn lower_record_int_tuple_element(&mut self, e_ref: &IrExpr, elem_ty: &Ty) -> ListElemArm {
        // The tuple's record slot is a STRUCTURAL literal (`({name: …, age: …}, 1)`) —
        // FORCE it to the classified type (the forced_elem precedent, extended into the
        // tuple slot) so `lower_owned_heap_field`'s recursive-record arm constructs the
        // SAME layout the registered `$__drop_list_<R>_int` tears down. A non-literal
        // slot must already carry the exact classified type; anything else declines.
        if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
            let Ty::Tuple(tys) = &elem_ty else { return ListElemArm::Decline };
            let mut tels = tels.clone();
            if matches!(tels[0].kind, IrExprKind::Record { .. }) {
                tels[0].ty = tys[0].clone();
            } else if tels[0].ty != tys[0] {
                return ListElemArm::Decline;
            }
            if let Some(obj) = self.try_lower_tuple_construct(&tels) {
                return ListElemArm::Built(obj);
            }
        }
        ListElemArm::Decline
    }

    /// The heap-PAIR tuple element arm (`StrInt` / `IntStr` / `StrVariant` / `StrMapStr` /
    /// `StrListOpt`) of [`Self::lower_record_list_element`], extracted verbatim (codopsy r2
    /// #852 complexity sweep): it decides that a tuple LITERAL element goes through the
    /// general masked-tuple builder, and leaves any other element shape to the generic
    /// owned-field path.
    fn lower_heap_pair_tuple_element(&mut self, e_ref: &IrExpr) -> ListElemArm {
        // A `(String, Int)` / `(Int, String)` / `(String, <rich variant>)` TUPLE LITERAL
        // element builds through the general masked-tuple builder (String slot fresh
        // OWNED + moved in, the other slot a scalar store OR — for `StrVariant` — a
        // fresh OWNED variant ctor block via `lower_owned_heap_field`'s existing
        // ctor-call dispatch; `try_lower_tuple_construct` already handles arbitrary
        // heap/scalar slot mixes for other callers, so no new construction path is
        // needed here). The list's OWN drop (registered below via
        // `variant_drop_handles`) frees each tuple's slots recursively, so the tuple's
        // own `record_masks` entry never scope-end-fires — mirrored from the
        // `(Int, String)` precedent in calls_p2.rs/binds.rs.
        if let IrExprKind::Tuple { elements: tels } = &e_ref.kind {
            let tels = tels.clone();
            if let Some(obj) = self.try_lower_tuple_construct(&tels) {
                return ListElemArm::Built(obj);
            }
            return ListElemArm::Decline;
        }
        ListElemArm::Fallthrough
    }

    /// The `Closure` element arm of [`Self::lower_record_list_element`], extracted verbatim
    /// (codopsy r2 #852 complexity sweep): it decides that a LAMBDA element is lifted here,
    /// and that any other element must carry the list's exact element type to reach the
    /// generic owned-field path.
    fn lower_lambda_list_element(&mut self, e_ref: &IrExpr, elem_ty: &Ty) -> ListElemArm {
        // A LAMBDA literal element: lift it to a fresh `__lambda_*` fn + closure block
        // via the SAME proven mechanism a call-argument lambda already uses (calls_p2.rs).
        if let IrExprKind::Lambda { params, body, .. } = &e_ref.kind {
            if let Some(obj) = self.lift_lambda(params, body) {
                return ListElemArm::Built(obj);
            }
            return ListElemArm::Decline;
        }
        // A non-lambda element (a Var holding a closure / a call returning one) must
        // carry the list's element type; anything else declines rather than storing a
        // mismatched value into a closure-drop-typed slot.
        if e_ref.ty != *elem_ty {
            return ListElemArm::Decline;
        }
        ListElemArm::Fallthrough
    }

    /// The `CtorFlat` / `CtorLenLoop` element arm of [`Self::lower_record_list_element`],
    /// extracted verbatim (codopsy r2 #852 complexity sweep): it decides that an
    /// Option/Result ctor element materializes through the ctor builder, and that a
    /// non-ctor element must carry the list's exact element type to reach the generic
    /// owned-field path.
    fn lower_option_ctor_element(&mut self, e_ref: &IrExpr, elem_ty: &Ty) -> ListElemArm {
        if let Some(obj) = self.try_lower_option_ctor(e_ref, &elem_ty) {
            return ListElemArm::Built(obj);
        }
        // A non-ctor element (a Var / call) must CARRY the list's element type — a
        // never-err LIFTED effect call (`[step(), step()]`, autotry_construction) has
        // its call type rewritten to the RAW payload (Int), so lowering it here would
        // store a SCALAR where the registered drop expects an owned handle (invalid
        // wasm + an unacquired `m` witness — the PCC gate caught exactly this).
        // Decline → the caller walls, never a wrong byte.
        if e_ref.ty != *elem_ty {
            return ListElemArm::Decline;
        }
        ListElemArm::Fallthrough
    }

    /// The BLOCK-element arm of [`Self::lower_record_list_element`], extracted
    /// (codopsy r2, #852 — that fn is the crate's worst at cog 93, and this is
    /// its one self-contained frame-managing arm). Verbatim: the statements
    /// lower as effects in an element-local frame, the TAIL becomes the element,
    /// and the produced object's tracking is restored so the caller's uniform
    /// `Consume` still sees it. `None` (not `Some(None)`) on a declined stmt or
    /// tail, exactly as before — the caller treats that as "this element is not
    /// in the subset" and walls.
    fn lower_block_list_element(
        &mut self,
        e_ref: &IrExpr,
        forced_elem: Option<&Ty>,
        elem_ty: &Ty,
        kind: ListElemDrop,
    ) -> Option<Option<ValueId>> {
        let IrExprKind::Block { stmts, expr } = &e_ref.kind else { return None };
        let tail = expr.as_deref()?;
        let mark = self.live_heap_handles.len();
        for s in stmts {
            if let Err(e) = self.lower_stmt(s) {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!("[elem-block] stmt declined: {e:?}")
                });
                return None;
            }
        }
        let out = match self.lower_record_list_element(tail, forced_elem, elem_ty, kind) {
            Some(o) => o,
            None => {
                crate::trace::trace("ALMIDE_DBG_ELEM", || {
                    format!("[elem-block] tail declined: {:?}", tail.kind)
                });
                return None;
            }
        };
        if let Some(obj) = out {
            self.live_heap_handles.retain(|h| *h != obj);
        }
        self.drop_arm_locals(mark);
        if let Some(obj) = out {
            self.live_heap_handles.push(obj);
        }
        Some(out)
    }
}
