impl LowerCtx {
    /// A HEAP (String) `list.fold` body that CONDITIONALLY replaces the accumulator —
    /// `if cond then <new-string> else acc` (the `find_flag` shape). Updates the loop-carried
    /// String slot IN PLACE: the THEN arm produces a FRESH owned String, DROPS the old slot value,
    /// and `SetLocal`'s the slot to the new one; the (empty) ELSE arm leaves the slot untouched. So
    /// the slot owns EXACTLY ONE reference at the body's start and end in BOTH arms — the conditional-
    /// acquire invariant (drop-then-rebind in a guarded arm, OwnershipFilter.v's CondLoop). The
    /// unconditional append-accumulator update CANNOT lower this: its `else acc` would drop-then-store
    /// the FREED slot (use-after-free). Each per-arm operand temp (the `??` operand's materialized
    /// Option block) is dropped WITHIN the then-arm (before `Else`), so neither arm leaks.
    ///
    /// Returns `true` iff fully lowered. On a NON-conditional body (a plain `acc + s` concat) it
    /// returns `false` with NO ops emitted (the caller's concat/scalar path takes over). On a
    /// conditional body it cannot fully lower it TRUNCATES the ops it pushed + returns `false` (the
    /// caller falls through, fails, and the whole HOF rolls back — never a wrong-bytes slot).
    fn try_lower_cond_heap_acc_fold(
        &mut self,
        body: &IrExpr,
        acc_param: VarId,
        acc_local: ValueId,
    ) -> bool {
        use almide_ir::IrExprKind;
        // Unwrap a single-expression Block (the post-fusion `tail` is `{ <if> }`).
        let inner = match &body.kind {
            IrExprKind::Block { stmts, expr } if stmts.is_empty() => match expr {
                Some(e) => e.as_ref(),
                None => return false,
            },
            _ => body,
        };
        let (cond, then, else_) = match &inner.kind {
            IrExprKind::If { cond, then, else_ } => (cond.as_ref(), then.as_ref(), else_.as_ref()),
            _ => return false,
        };
        // The ELSE arm must be EXACTLY the accumulator param (`else acc`): that is what makes the
        // empty-else in-place update sound (the slot is genuinely unchanged when `cond` is false).
        match &else_.kind {
            IrExprKind::Var { id } if *id == acc_param => {}
            _ => return false,
        }
        // The shape matched — roll back our own ops on any sub-failure (so the caller's fall-through
        // path starts clean).
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        self.ops.push(Op::IfThen { cond: cond_v, dst: None });
        // THEN arm: a FRESH owned String (BARE — NOT Consumed/registered; the slot will own it).
        let arm_mark = self.live_heap_handles.len();
        let new_v = match self.lower_cond_acc_then_value(then) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        // Drop the OLD slot value, then move the new one in (the slot's single ref is preserved).
        let drop_op = self.drop_op_for(acc_local);
        self.ops.push(drop_op);
        self.ops.push(Op::SetLocal { local: acc_local, src: new_v });
        // Free the then-arm's operand temps (e.g. the `??` operand Option block) WITHIN the arm.
        self.drop_arm_locals(arm_mark);
        self.ops.push(Op::Else { val: None });
        // ELSE arm: empty — the slot is left as-is.
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// Lower the THEN arm of a conditional heap-acc fold to a BARE owned String (rc 1, NOT Consumed,
    /// NOT registered for scope-end drop — the accumulator slot becomes its sole owner via the
    /// caller's drop-old + `SetLocal`). Any operand temp it materializes IS registered (freed by the
    /// caller's per-arm `drop_arm_locals`). `None` ⇒ out of subset (the caller rolls back + walls).
    fn lower_cond_acc_then_value(&mut self, then: &IrExpr) -> Option<ValueId> {
        use almide_ir::{BinOp, IrExprKind};
        match &then.kind {
            // `e!` — strip the effect-propagation unwrap (identity on the Ok payload here).
            IrExprKind::Unwrap { expr } => self.lower_cond_acc_then_value(expr),
            // `<option/result> ?? <fallback>` (`list.get(args, i+1) ?? ""`) → a fresh owned String.
            IrExprKind::UnwrapOr { expr, fallback } => {
                self.try_lower_option_unwrap_or(expr, fallback, false)
            }
            // A string LITERAL → a fresh owned String block.
            IrExprKind::LitStr { value } => {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                    init: crate::Init::Str(value.clone()),
                });
                Some(dst)
            }
            // `a + b` string concat → a fresh owned String.
            IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => self.try_lower_concat_str(then),
            // `acc + [x]` list concat → a fresh owned list (the straight-line / loop append-
            // accumulator THEN arm). `try_lower_concat_list` reads the loop-carried `acc` slot as a
            // BORROW and emits a fresh owned list via `__list_concat`/`__list_concat_rc`; the caller's
            // drop-old + `SetLocal` then moves it into the slot (single-owned). It classifies the
            // result's recursive drop kind (heap_elem/value/…); the caller copies that onto the slot
            // so the per-iteration drop-old frees the right grain.
            IrExprKind::BinOp { op: BinOp::ConcatList, .. } => self.try_lower_concat_list(then),
            // A string INTERPOLATION → a fresh owned String.
            IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
            // A bare heap Var → ACQUIRE our own reference (`Dup`) so the slot owns it independently.
            IrExprKind::Var { id } => {
                let src = self.value_for(*id).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src });
                Some(dst)
            }
            // `pair.0` — a HEAP field of the borrowed tuple/record element (the
            // lookup_token fold: `if pair.1 == target then pair.0 else acc`): load
            // the slot's handle (a borrow of the container's field) then ACQUIRE
            // (`Dup`) so the accumulator slot owns its own reference.
            IrExprKind::TupleIndex { object, index } => {
                use crate::{IntOp, PrimKind};
                let obj = match &object.kind {
                    IrExprKind::Var { id } => self.value_for(*id).ok()?,
                    _ => return None,
                };
                if !self.materialized_aggregates.contains(&obj) {
                    return None;
                }
                let off = self.aggregate_index_offset_any(&object.ty, *index)?;
                let h = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![obj] });
                let offc = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: offc, value: off as i64 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: offc });
                let raw = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(raw), args: vec![addr] });
                let dst = self.fresh_value();
                self.ops.push(Op::Dup { dst, src: raw });
                Some(dst)
            }
            _ => None,
        }
    }

    /// Copy the RECURSIVE-DROP classification of one heap object onto another (the loop-carried slot
    /// inherits its feeder's drop grain). A `List[String]`/flat-element list drops via `DropListStr`,
    /// a `List[Value]` via `DropListValue`, a `(String,Value)`/`(String,String)`/`(Int,String)` tuple
    /// list / record / variant element list via its dedicated recursive drop — so the slot's
    /// per-iteration drop-old frees exactly the same grain its feeder allocated (no leak, no
    /// double-free). A plain String/scalar feeder is in no set ⇒ a no-op (the slot stays a flat `Drop`).
    pub(crate) fn copy_heap_drop_class(&mut self, from: ValueId, to: ValueId) {
        if self.heap_elem_lists.contains(&from) {
            self.heap_elem_lists.insert(to);
        }
        if self.value_elem_lists.contains(&from) {
            self.value_elem_lists.insert(to);
        }
        if self.str_value_elem_lists.contains(&from) {
            self.str_value_elem_lists.insert(to);
        }
        if self.str_str_elem_lists.contains(&from) {
            self.str_str_elem_lists.insert(to);
        }
        if self.list_list_str_lists.contains(&from) {
            self.list_list_str_lists.insert(to);
        }
        if self.value_handles.contains(&from) {
            self.value_handles.insert(to);
        }
        if let Some(k) = self.variant_drop_handles.get(&from).cloned() {
            self.variant_drop_handles.insert(to, k);
        }
    }

    /// STRAIGHT-LINE identity-else shadow rebind — `let acc = if cond then acc + [x] else acc` bound
    /// to a let/var OUTSIDE any loop (porta `serialize_opts`: 7 stacked optional-arg appends on one
    /// `args` slot). The ELSE arm is EXACTLY the accumulator var, so this is the PROVEN loop-carried
    /// `i(id)m` append-accumulator slot, just UNROLLED to a fixed straight-line sequence: the THEN arm
    /// produces a FRESH owned value, DROPS the old slot, and `SetLocal`s the slot to the new one; the
    /// (empty) ELSE leaves the slot untouched. So the slot owns EXACTLY ONE reference at each rebind's
    /// start and end in BOTH arms (the conditional-acquire invariant, OwnershipChecker.v
    /// `check_line_unroll_sound`, which quantifies over ALL iteration counts — including this
    /// unrolling). `ownership_certificate` folds each rebind to a `(id)` CLoop body, so the whole body
    /// reads `i(id)…(id)m` — each `(id)` the same rc-preserving unit the loop slot proves, accepted by
    /// the kernel-checked `check_cert_lc`.
    ///
    /// `acc_local` MUST be the OWNED, scope-tracked heap handle behind `acc` (the seed's `[]`/`""`),
    /// already in `live_heap_handles` — the caller aliases the new shadow to it and does NOT re-push,
    /// so the single scope-end drop (or tail move-out) still covers it. Returns `true` iff fully
    /// lowered; on a sub-failure it TRUNCATES its own ops and returns `false` (the caller walls).
    pub(crate) fn try_lower_line_cond_acc(
        &mut self,
        value: &IrExpr,
        acc_id: VarId,
        acc_local: ValueId,
    ) -> bool {
        use almide_ir::IrExprKind;
        let (cond, then, else_) = match &value.kind {
            IrExprKind::If { cond, then, else_ } => (cond.as_ref(), then.as_ref(), else_.as_ref()),
            _ => return false,
        };
        // The ELSE arm must be EXACTLY the accumulator var (`else acc`) — that is what makes the
        // empty-else in-place update sound (the slot is genuinely unchanged when `cond` is false).
        match &else_.kind {
            IrExprKind::Var { id } if *id == acc_id => {}
            _ => return false,
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let cond_v = match self.lower_scalar_value(cond) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        self.ops.push(Op::IfThen { cond: cond_v, dst: None });
        // THEN arm: a FRESH owned value (BARE — NOT Consumed/registered; the slot will own it).
        let arm_mark = self.live_heap_handles.len();
        let new_v = match self.lower_cond_acc_then_value(then) {
            Some(v) => v,
            None => {
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return false;
            }
        };
        // The slot inherits the fresh value's recursive-drop grain, so the drop-old below (and at
        // every later rebind, and at scope end) frees exactly the right element grain.
        self.copy_heap_drop_class(new_v, acc_local);
        // Drop the OLD slot value, then move the new one in (the slot's single ref is preserved).
        let drop_op = self.drop_op_for(acc_local);
        self.ops.push(drop_op);
        self.ops.push(Op::SetLocal { local: acc_local, src: new_v });
        // Free the then-arm's operand temps (e.g. a materialized list-literal operand) WITHIN the arm.
        self.drop_arm_locals(arm_mark);
        self.ops.push(Op::Else { val: None });
        self.ops.push(Op::EndIf { val: None });
        true
    }

    /// C1 DEFUNCTIONALIZATION for a TUPLE-accumulator `list.fold` whose accumulator is a 2-tuple
    /// `(List[T], Int)` — the wasm-bindgen `wasm_record_offsets` shape:
    ///
    /// ```text
    /// widths |> list.fold(([], 0), (state, w) => {
    ///   let (acc, off) = state
    ///   (acc + [off], off + w)
    /// })
    /// ```
    ///
    /// The accumulator carries a growing `List[T]` (the offsets) and an `Int` cursor. The scalar
    /// `result_ok` gate (`!is_heap_ty || String`) rejects a `(List[T], Int)` accumulator (heap +
    /// not-String), so this dedicated path carries TWO loop-state slots and builds the result tuple
    /// ONCE after the loop:
    ///
    ///   1. The LIST component — a heap slot SEEDED with a fresh EMPTY `List` (an i32 `Alloc`, NOT
    ///      registered for drop: the slot owns it). Each iteration the body's component0 `acc + [elem]`
    ///      is lowered to a FRESH owned list via `try_lower_concat_list` (reading the slot through
    ///      `value_of[acc]`), then the slot is DROPPED-OLD + `SetLocal`'d to the new list — the PROVEN
    ///      `i(id)m` append-accumulator slot (identical to the String-fold arm at the top of
    ///      `lower_defunc_list_hof_inner` and to `append_owned_sub_to_acc`).
    ///   2. The INT component — a STABLE scalar local, `SetLocal`'d each iteration with the body's
    ///      component1 `n + step`.
    ///
    /// The body's `let (acc, off) = state` destructure is NEVER materialized as a tuple block — `acc`
    /// binds DIRECTLY to the List slot and `off` to the Int slot (read the two slots in place). The
    /// body's final `(c0, c1)` tuple is NEVER materialized per iteration — it is destructured into the
    /// two slot updates. After the loop the result tuple `(List_slot, Int_slot)` is built ONCE via
    /// `try_lower_tuple_construct` (a masked block: slot0 a moved-in OWNED list handle, slot1 the Int),
    /// so the caller's Module-call bind path tracks it as a `materialized_aggregate` with the right
    /// heap-slot mask and its scope-end drop frees exactly the list + the block. The immediate
    /// `match pair { (offs, _) => offs }` extraction then borrows + acquires slot 0 (see
    /// `try_lower_tuple_extract_match`).
    ///
    /// SOUNDNESS — both slots are independently balanced per iteration:
    ///  - The List slot is the proven loop-carried `i(id)m`: seeded `i` (fresh empty), each iteration a
    ///    fresh concat (`i`), drop-old (`d`), `SetLocal` (folds the new `i` into the slot — `m`). After
    ///    the loop its single live value is MOVED into the tuple block's slot 0 (cert `m`); the block's
    ///    masked drop (or the move-out at the `match` extraction) frees it exactly once. No leak, no
    ///    double-free for any iteration count.
    ///  - The Int slot is a scalar — no ownership event (a plain `SetLocal`).
    ///
    /// STRICT GATE (any deviation → `None` → the caller rolls back → WALLs, never a wrong-bytes tuple):
    /// a 2-tuple `(List[T], Int)` accumulator, an empty-list-literal + scalar-int LITERAL init, a body
    /// that is exactly `{ let (a, b) = state; (a + [<elem>], <scalar-int>) }` (the destructure reads the
    /// state param; component0 a `ConcatList` reading the bound list var; component1 a scalar Int).
    fn try_lower_defunc_tuple_acc_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::IrStmtKind;
        use crate::PrimKind;

        // GATE — fold has exactly TWO params (state, element).
        if params.len() != 2 {
            return None;
        }
        let state_param = params[0].0;
        let elem_param = params[1].0;

        // GATE — the accumulator/result type is a 2-tuple `(List[T], Int)`.
        let tup_tys = match result_ty {
            Ty::Tuple(tys) if tys.len() == 2 => tys,
            _ => return None,
        };
        let list_elem_ty = match &tup_tys[0] {
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 => a[0].clone(),
            _ => return None,
        };
        if !matches!(tup_tys[1], Ty::Int) {
            return None;
        }
        // A SCALAR list element (`(List[Int], Int)` — `wasm_record_offsets`) OR a `String` element
        // (`(List[String], Int)`). Both are admitted: the scalar slot is a FLAT `__list_concat` + flat
        // `Drop`; the String slot is `__list_concat_rc` + the recursive `DropListStr` (marked
        // `heap_elem_lists`), and the SOURCE element of a heap (`List[String]`) source is read as the
        // slot's i32 HANDLE (`LoadHandle`, below) — reading it as an i64 scalar was the
        // `expected i32, found i64` invalid-wasm bug. A heap-FIELD aggregate element (a tuple/record
        // list) still defers (its masked recursive drop is out of this slice).
        let list_elem_scalar = !is_heap_ty(&list_elem_ty);
        let list_elem_str = matches!(list_elem_ty, Ty::String);
        if !list_elem_scalar && !list_elem_str {
            return None;
        }

        // GATE — the INIT is `(<empty-list-literal>, <int-literal>)`.
        let init_elems = match &init.kind {
            IrExprKind::Tuple { elements } if elements.len() == 2 => elements,
            _ => return None,
        };
        match &init_elems[0].kind {
            IrExprKind::List { elements } if elements.is_empty() => {}
            _ => return None,
        }
        let int_init_v = match &init_elems[1].kind {
            IrExprKind::LitInt { value } => *value,
            _ => return None,
        };

        // GATE — the BODY is `{ let (acc, n) = state; <interior pure lets…>; (c0, c1) }`: the FIRST
        // statement destructures the state param's two components, then ZERO OR MORE pure value-binding
        // statements (the element destructure `let (i,f)=fe`, a scalar `let off_s = int.to_string(off)`,
        // a heap `let store = match get_kind(f) {…}` — the wasm-bindgen `field_stores` shape), then a
        // 2-tuple tail. The interior statements are lowered as per-iteration EFFECTS (their heap temps
        // freed within the iteration frame); a statement the body subset cannot lower → None → walls.
        let (stmts, tail) = match &body.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } if !stmts.is_empty() => (stmts, tail.as_ref()),
            _ => return None,
        };
        let (acc_var, n_var) = match &stmts[0].kind {
            IrStmtKind::BindDestructure { pattern, value } => {
                // The destructured value must be the STATE param (the destructure reads the two slots,
                // not a fresh tuple).
                match &value.kind {
                    IrExprKind::Var { id } if *id == state_param => {}
                    _ => return None,
                }
                match pattern {
                    IrPattern::Tuple { elements } if elements.len() == 2 => {
                        let a = match &elements[0] {
                            IrPattern::Bind { var, ty }
                                if matches!(ty,
                                    Ty::Applied(TypeConstructorId::List, l) if l.len() == 1) =>
                            {
                                *var
                            }
                            _ => return None,
                        };
                        let b = match &elements[1] {
                            IrPattern::Bind { var, ty } if matches!(ty, Ty::Int) => *var,
                            _ => return None,
                        };
                        (a, b)
                    }
                    _ => return None,
                }
            }
            _ => return None,
        };
        // The tail must be a 2-tuple `(c0, c1)` whose component0 is a `acc + […]` ConcatList reading
        // the destructured list var, and component1 a scalar Int. (The component0 shape is re-checked
        // by `try_lower_concat_list` below; here we just require the tuple shape + a ConcatList whose
        // left reads `acc_var`, so a non-append body — e.g. `(other_list, …)` — defers.)
        let tail_elems = match &tail.kind {
            IrExprKind::Tuple { elements } if elements.len() == 2 => elements,
            _ => return None,
        };
        match &tail_elems[0].kind {
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, left, .. } => {
                match &left.kind {
                    IrExprKind::Var { id } if *id == acc_var => {}
                    _ => return None,
                }
            }
            _ => return None,
        }
        if !matches!(tail_elems[0].ty,
            Ty::Applied(TypeConstructorId::List, ref a) if a.len() == 1)
            || !matches!(tail_elems[1].ty, Ty::Int)
        {
            return None;
        }

        // ----- Lowering -----
        // Borrow the source list (evaluated once). A Var is borrowed; a fresh literal is materialized
        // into an owned temp dropped at the OUTER scope. A non-handle iterable is out of subset.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        // Seed the LIST slot: a fresh EMPTY `List` (an i32 Alloc dst). The loop-carried slot OWNS it —
        // reassigned via drop-old + SetLocal each iteration (the proven i(id)m slot). NOT registered
        // for drop here: the slot owns it; the loop's drop-old or the post-loop move-into-tuple frees
        // it exactly once. A `List[String]` slot is marked `heap_elem_lists` so EVERY drop of it
        // (drop-old) is the recursive `DropListStr` (frees its owned element Strings).
        let zero_len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero_len, value: 0 });
        let list_slot = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: list_slot,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: zero_len },
        });
        if list_elem_str {
            self.heap_elem_lists.insert(list_slot);
        }

        // Seed the INT slot: a STABLE mutable scalar local (ConstInt-seed then SetLocal to the init —
        // the proven loop-state model: distinct + reassignable across iterations).
        let int_slot = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: int_slot, value: 0 });
        let int_init_const = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: int_init_const, value: int_init_v });
        self.ops.push(Op::SetLocal { local: int_slot, src: int_init_const });

        // The loop index (stable mutable i64 local) + the +1 step constant.
        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // Load element[i] from the SOURCE list: addr = src_h + 12 + i*8. The READ WIDTH depends on the
        // SOURCE element type — a SCALAR source (`List[Int]`) is the i64 VALUE (`Load { width: 8 }`); a
        // HEAP source (`List[String]`) is the slot's String HANDLE (`LoadHandle` = i32 Ptr), read as a
        // BORROWED heap value the body copies (`acc + [w]`, `string.len(w)`). Hardcoding the i64 load
        // for a heap source declared the element local i64 while `string.len`/`__list_concat_rc` expect
        // an i32 handle — the `expected i32, found i64` invalid wasm. Mirrors `lower_defunc_list_hof_
        // inner`'s `src_heap` element read.
        let src_heap = matches!(&xs.ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]));
        let i8_v = self.fresh_value();
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let src_base = self.load_addr(h, 12);
        let src_addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: src_addr, op: IntOp::Add, a: src_base, b: i8_v });
        let elem = self.fresh_value();
        let read_kind = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: read_kind, dst: Some(elem), args: vec![src_addr] });

        // Bind the lambda params + the destructured state vars to the SLOTS (NO tuple state block):
        //   element param → elem (the loaded value/handle)
        //   acc (destructured list var) → the List slot
        //   n   (destructured int var)  → the Int slot
        // Captures need no binding — their VarIds already resolve through `value_of`.
        self.value_of.insert(elem_param, elem);
        // A HEAP source element is a BORROW (the source list owns it) — record it so the body's
        // `acc + [w]` rc-incs/copies it rather than moving it out (no double-free with the source's
        // own drop). A scalar element is a value copy, no ownership.
        if src_heap {
            self.param_values.insert(elem);
        }
        self.value_of.insert(acc_var, list_slot);
        self.value_of.insert(n_var, int_slot);
        // `acc` reads the List slot as a tracked nested-ownership list so a `acc + [x]` concat
        // borrows it (the slot owns it; the concat rc-incs/byte-copies into a fresh list).
        self.materialized_lists.insert(list_slot);

        // Lower the INTERIOR statements (`stmts[1..]` — `let (i,f)=fe`, `let off_s = int.to_string(off)`,
        // `let store = match get_kind(f) {…}`) as per-iteration EFFECTS, then BOTH tuple components,
        // reading the OLD slot values BEFORE updating either slot (the body semantics: `(acc + [store],
        // off + w)` reads `acc`/`off` as the iteration's incoming state). A heap temp an interior stmt
        // binds (the `store` String) is freed within the per-iteration frame by `drop_arm_locals`; the
        // concat already copied/rc-inc'd it into the new list, so it is single-owned + freed.
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let mut stmts_ok = true;
        for s in &stmts[1..] {
            // A `let store = if/match {…}` interior stmt binds a HEAP-result branch. `lower_stmt` →
            // `lower_bind` WALLS that (the function-scope `im·im·d` it cannot prove). HERE it is sound:
            // materialize the merged owned `dst` (per-arm `"im"`), bind `store`, and push it to
            // `live_heap_handles` so the PER-ITERATION `drop_arm_locals(body_mark)` frees it once —
            // the proven `i…d` per-iteration frame (NOT a function-scope drop). The concat `acc + [store]`
            // copies/rc-incs it into the new list before that drop. Other stmts use the normal `lower_stmt`.
            if let IrStmtKind::Bind { var, ty: bty, value, .. } = &s.kind {
                if is_heap_ty(bty)
                    && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
                {
                    let merged = match &value.kind {
                        IrExprKind::If { cond, then, else_ } => {
                            self.try_lower_heap_result_if(cond, then, else_, bty)
                        }
                        IrExprKind::Match { subject, arms } => self
                            .desugar_match_to_if(subject, arms, bty)
                            .and_then(|if_e| match &if_e.kind {
                                IrExprKind::If { cond, then, else_ } => {
                                    self.try_lower_heap_result_if(cond, then, else_, bty)
                                }
                                _ => None,
                            }),
                        _ => None,
                    };
                    match merged {
                        Some(dst) => {
                            self.value_of.insert(*var, dst);
                            if !self.live_heap_handles.contains(&dst) {
                                self.live_heap_handles.push(dst);
                            }
                            continue;
                        }
                        None => {
                            stmts_ok = false;
                            break;
                        }
                    }
                }
            }
            if self.lower_stmt(s).is_err() {
                stmts_ok = false;
                break;
            }
        }
        // component0: `acc + [elem]` → a FRESH owned list (reads the List slot). try_lower_concat_list
        // returns a bare ValueId (registered in the right recursive-drop set, NOT in live_heap_handles).
        let new_list = if stmts_ok { self.try_lower_concat_list(&tail_elems[0]) } else { None };
        // component1: `n + step` → a scalar value (reads the Int slot).
        let new_int = new_list.and_then(|_| self.lower_scalar_value(&tail_elems[1]));
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let (new_list, new_int) = match (new_list, new_int) {
            (Some(l), Some(n)) => (l, n),
            _ => return None,
        };
        // Free any stray per-iteration heap temp (the concat's arg temps) within the iteration frame.
        self.drop_arm_locals(body_mark);

        // Update the LIST slot: DROP-OLD the previous value, then SetLocal. The new list inherits the
        // slot's recursive-drop set (String → heap_elem_lists) so the slot's drop stays the right kind.
        if list_elem_str {
            self.heap_elem_lists.insert(new_list);
        }
        let drop_old = self.drop_op_for(list_slot);
        self.ops.push(drop_old);
        self.ops.push(Op::SetLocal { local: list_slot, src: new_list });

        // Update the INT slot (a scalar SetLocal — no ownership event).
        self.ops.push(Op::SetLocal { local: int_slot, src: new_int });

        // Advance the index and close the loop.
        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        // Build the RESULT tuple `(List_slot, Int_slot)` ONCE after the loop. The List slot's single
        // live value is MOVED into slot 0 (cert `m`); the Int slot's value is stored into slot 1. The
        // block is a masked aggregate (slot 0 heap), tracked by the caller's bind path for the
        // scope-end masked drop. The slot's recursive-drop set (heap_elem_lists for String) carries to
        // the block via `try_lower_tuple_construct`'s element handling? No — the tuple's masked drop is
        // a flat `DropListStr` rc_dec of slot 0; for a `List[String]` slot 0 the recursive element free
        // happens at the FINAL owner (the moved-out list at the `match` extraction / the caller), not
        // here, since the move-out keeps the inner list's rc ≥ 1 through the tuple's flat drop.
        //
        // `try_lower_tuple_construct` requires the slot value to be a real owned heap field — re-tag the
        // List slot as a live heap handle so its consume-into-the-tuple is the MOVE the construct emits.
        if !self.live_heap_handles.contains(&list_slot) {
            self.live_heap_handles.push(list_slot);
        }
        let tup = self.build_tuple_acc_result(list_slot, int_slot)?;
        Some(tup)
    }

    /// Build the `(List, Int)` result block of a tuple-accumulator fold: a 2-slot masked aggregate
    /// with slot 0 the MOVED-IN owned `list` handle (cert `m` — the loop slot's single live value) and
    /// slot 1 the scalar `int`. Tracked `record_masks = [0]` + `materialized_aggregates` so a later
    /// `t.0`/`(x,_)=>x` borrow reads the real slot and the scope-end drop frees slot 0 + the block.
    fn build_tuple_acc_result(&mut self, list: ValueId, int: ValueId) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: 2 });
        let dst = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len },
        });
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
        // slot 0 = the list handle (MOVED in — extend to i64 then store64, then Consume the source).
        let off0 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off0, value: 12 });
        let addr0 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr0, op: IntOp::Add, a: h, b: off0 });
        let lh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(lh), args: vec![list] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr0, lh] });
        self.ops.push(Op::Consume { v: list });
        self.live_heap_handles.retain(|x| *x != list);
        // slot 1 = the int value (store64).
        let off1 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: off1, value: 20 });
        let addr1 = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr1, op: IntOp::Add, a: h, b: off1 });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addr1, int] });
        // A masked aggregate: slot 0 is the heap list, slot 1 scalar. The caller's Module-bind path
        // re-derives this mask from the result type, but set it here too so the value is self-describing
        // (a nested-ownership `List[String]` slot 0 is handled by the move-out at the `match` extraction;
        // the flat masked `DropListStr` rc_dec of a still-shared list block is leak-safe — see the doc on
        // `try_lower_defunc_tuple_acc_fold`).
        self.record_masks.insert(dst, vec![0]);
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }
}
