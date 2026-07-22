impl LowerCtx {

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

    /// The RECORD-accumulator sibling of [`Self::try_lower_defunc_tuple_acc_fold`] — the
    /// playground `wrap_lists` shape:
    ///
    ///   `blocks |> list.fold({ out: [], in_ul: false }, (st, b) => { …; { out: <opened> +
    ///   [render(b)], in_ul: <scalar> } })`
    ///
    /// The accumulator is a 2-field record with exactly ONE `List[scalar|String]` field and ONE
    /// scalar (Int/Bool) field. Unlike the tuple shape there is NO destructure statement — the
    /// body reads the state through MEMBER projections (`st.out` / `st.in_ul`), so the gate
    /// SUBSTITUTES those projections with two synthetic vars bound to the slots (any OTHER use
    /// of the state param — a bare `st`, a spread — declines). The loop skeleton, per-iteration
    /// balance, and the drop-old + `SetLocal` slot discipline are the tuple path's, verbatim;
    /// the tail's list component may read an INTERIOR heap-`if` binding (`opened`, itself built
    /// from slot copies by the proven heap-result-arm machinery) rather than the slot directly —
    /// sound because the slot update is drop-old + own-new regardless of the new list's
    /// derivation. The result block is field-ORDERED per the record layout with the list slot
    /// masked, so the post-fold `if result.in_ul then result.out + [..] else result.out`
    /// projects real slots (the (B)-mechanism Member-arm widening).
    fn try_lower_defunc_record_acc_fold(
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

        if params.len() != 2 {
            return None;
        }
        let state_param = params[0].0;
        let elem_param = params[1].0;

        // GATE — a 2-field record acc: one List[scalar|String] field + one scalar field.
        let (fnames, ftys) = self.aggregate_field_tys(result_ty)?;
        if fnames.len() != 2 || ftys.len() != 2 {
            return None;
        }
        let list_idx = ftys.iter().position(|t|
            matches!(t, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1))?;
        let scalar_idx = 1 - list_idx;
        let list_elem_ty = match &ftys[list_idx] {
            Ty::Applied(TypeConstructorId::List, a) => a[0].clone(),
            _ => return None,
        };
        if is_heap_ty(&ftys[scalar_idx]) {
            return None;
        }
        let list_elem_scalar = !is_heap_ty(&list_elem_ty);
        let list_elem_str = matches!(list_elem_ty, Ty::String);
        if !list_elem_scalar && !list_elem_str {
            return None;
        }
        let list_fname = fnames[list_idx];
        let scalar_fname = fnames[scalar_idx];

        // GATE — the INIT is a record literal `{ <list>: [], <scalar>: <Int/Bool literal> }`.
        let init_fields = match &init.kind {
            IrExprKind::Record { fields, .. } if fields.len() == 2 => fields,
            _ => return None,
        };
        let init_by = |n: almide_lang::intern::Sym| init_fields.iter().find(|(fn_, _)| *fn_ == n);
        match &init_by(list_fname)?.1.kind {
            IrExprKind::List { elements } if elements.is_empty() => {}
            _ => return None,
        }
        let scalar_init_v = match &init_by(scalar_fname)?.1.kind {
            IrExprKind::LitInt { value } => *value,
            IrExprKind::LitBool { value } => *value as i64,
            _ => return None,
        };

        // SUBSTITUTE the state-param MEMBER projections with two synthetic vars; any OTHER
        // use of the state param declines (a bare `st` / spread would need the record block).
        let acc_var = VarId(crate::lower::max_var_id(body) + 1);
        let n_var = VarId(acc_var.0 + 1);
        let body = substitute_state_members(body, state_param, list_fname, acc_var, scalar_fname, n_var)?;
        let body = &body;

        // GATE — the BODY is `{ <interior lets…>; { <list>: <l> + […], <scalar>: <s> } }`.
        let (stmts, tail) = match &body.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => (stmts.as_slice(), tail.as_ref()),
            // A stmt-less lambda body IS the record literal (`(st, b) => { out: …, in_ul: … }`
            // parses as the record, not a block).
            IrExprKind::Record { .. } => (&[][..], body),
            _ => return None,
        };
        let tail_fields = match &tail.kind {
            IrExprKind::Record { fields, .. } if fields.len() == 2 => fields,
            _ => return None,
        };
        let tail_by = |n: almide_lang::intern::Sym| tail_fields.iter().find(|(fn_, _)| *fn_ == n);
        let tail_list = &tail_by(list_fname)?.1;
        let tail_scalar = &tail_by(scalar_fname)?.1;
        match &tail_list.kind {
            IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {}
            _ => return None,
        }
        if is_heap_ty(&tail_scalar.ty) {
            return None;
        }

        // ----- Lowering (the tuple path's skeleton) -----
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok()?.into_iter().next()? {
            CallArg::Handle(v) => v,
            _ => return None,
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

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

        let scalar_slot = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: scalar_slot, value: 0 });
        let scalar_init_const = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: scalar_init_const, value: scalar_init_v });
        self.ops.push(Op::SetLocal { local: scalar_slot, src: scalar_init_const });

        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });

        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

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

        self.value_of.insert(elem_param, elem);
        if src_heap {
            self.param_values.insert(elem);
            // A user-VARIANT source element (`b: Block`) is a real tagged block the body
            // matches on (`match b { Bullet(_) => … }`) — seed its read shape like a fn
            // param so the scalar match executes on the tag.
            self.seed_variant_param(elem, &params[1].1);
        }
        self.value_of.insert(acc_var, list_slot);
        self.value_of.insert(n_var, scalar_slot);
        self.materialized_lists.insert(list_slot);

        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let mut stmts_ok = true;
        for s in stmts {
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
                            // An interior `List[String]` binding (`opened`) must READ as a
                            // tracked list (the tail concat borrows it) and DROP recursively
                            // (its element Strings are owned refs; a flat free would leak
                            // them each iteration).
                            if matches!(bty, Ty::Applied(TypeConstructorId::List, a)
                                if a.len() == 1 && matches!(a[0], Ty::String))
                            {
                                self.materialized_lists.insert(dst);
                                self.heap_elem_lists.insert(dst);
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
        let new_list = if stmts_ok { self.try_lower_concat_list(tail_list) } else { None };
        let new_scalar = new_list.and_then(|_| self.lower_scalar_value(tail_scalar));
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let (new_list, new_scalar) = match (new_list, new_scalar) {
            (Some(l), Some(n)) => (l, n),
            _ => return None,
        };
        self.drop_arm_locals(body_mark);

        if list_elem_str {
            self.heap_elem_lists.insert(new_list);
        }
        let drop_old = self.drop_op_for(list_slot);
        self.ops.push(drop_old);
        self.ops.push(Op::SetLocal { local: list_slot, src: new_list });
        self.ops.push(Op::SetLocal { local: scalar_slot, src: new_scalar });

        let next_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: next_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::SetLocal { local: i_v, src: next_v });
        self.ops.push(Op::LoopEnd);

        if !self.live_heap_handles.contains(&list_slot) {
            self.live_heap_handles.push(list_slot);
        }
        self.build_record_acc_result(list_slot, scalar_slot, list_idx, scalar_idx, result_ty)
    }

    /// Build the record-accumulator fold's result block — [`Self::build_tuple_acc_result`]
    /// with the two slots at their DECLARED field indices (the record layout order), so the
    /// post-fold `result.<field>` projections read the right slots. `record_masks` = the list
    /// slot's index; tracked `materialized_aggregates` for the (B)-widened Member arm.
    fn build_record_acc_result(
        &mut self,
        list: ValueId,
        scalar: ValueId,
        list_idx: usize,
        scalar_idx: usize,
        result_ty: &Ty,
    ) -> Option<ValueId> {
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
        let offl = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: offl, value: 12 + 8 * list_idx as i64 });
        let addrl = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addrl, op: IntOp::Add, a: h, b: offl });
        let lh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(lh), args: vec![list] });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addrl, lh] });
        self.ops.push(Op::Consume { v: list });
        self.live_heap_handles.retain(|x| *x != list);
        let offs = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: offs, value: 12 + 8 * scalar_idx as i64 });
        let addrs = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addrs, op: IntOp::Add, a: h, b: offs });
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![addrs, scalar] });
        // DROP ROUTING — the flat masked drop (`record_masks`) rc_decs the list SLOT only,
        // LEAKING its element Strings whenever the field is merely BORROWED after the fold
        // (`list.len(result.out)`) rather than moved out (the tuple path's fixture always
        // moved out; a 100k leak-loop under a 4MB cap caught the borrowed case OOMing).
        // Route the block's scope-end drop through the GENERATED anon/named record drop
        // (`$__drop_anonrec_<hash>` / `$__drop_<R>` — its `__drop_list_str` sweep is
        // LAST-REF gated, so a moved-out copy keeps the list + strings alive). The record
        // literal in the fold's init guarantees the generator collected this record type.
        match self.record_or_anon_drop_type_name(result_ty) {
            Some(name) => {
                self.variant_drop_handles.insert(dst, name);
            }
            None => {
                self.record_masks.insert(dst, vec![list_idx]);
            }
        }
        self.materialized_aggregates.insert(dst);
        Some(dst)
    }
}

/// Substitute `st.<list_field>` / `st.<scalar_field>` (Member projections of the fold's state
/// param) with the two synthetic slot vars, FAILING (`None`) if the state param is used in any
/// OTHER form (a bare `st`, a spread, a different field) — those would need the record block
/// the record-acc fold deliberately never materializes.
fn substitute_state_members(
    body: &IrExpr,
    state: VarId,
    list_field: almide_lang::intern::Sym,
    acc_var: VarId,
    scalar_field: almide_lang::intern::Sym,
    n_var: VarId,
) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct S {
        state: VarId,
        list_field: almide_lang::intern::Sym,
        acc_var: VarId,
        scalar_field: almide_lang::intern::Sym,
        n_var: VarId,
        bad: bool,
    }
    // Pure decision, no traversal state — which slot var replaces `st.<field>`, or
    // None if `field` is neither state field (the "other field" bad case). Extracted
    // out of the trait method below so the recursive walk's own branching (the state-
    // threaded traversal itself, left untouched per the "no mechanical decomposition
    // of state-threading walkers" rule) isn't tangled up with this unrelated pure
    // field-to-var mapping.
    fn member_replacement_var(
        field: almide_lang::intern::Sym,
        list_field: almide_lang::intern::Sym,
        scalar_field: almide_lang::intern::Sym,
        acc_var: VarId,
        n_var: VarId,
    ) -> Option<VarId> {
        if field == list_field {
            Some(acc_var)
        } else if field == scalar_field {
            Some(n_var)
        } else {
            None
        }
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if let IrExprKind::Member { object, field } = &e.kind {
                if matches!(&object.kind, IrExprKind::Var { id } if *id == self.state) {
                    match member_replacement_var(
                        *field,
                        self.list_field,
                        self.scalar_field,
                        self.acc_var,
                        self.n_var,
                    ) {
                        Some(v) => {
                            e.kind = IrExprKind::Var { id: v };
                            return; // no need to walk the replaced node
                        }
                        None => {
                            self.bad = true;
                            return;
                        }
                    }
                }
            }
            if matches!(&e.kind, IrExprKind::Var { id } if *id == self.state) {
                self.bad = true;
                return;
            }
            walk_expr_mut(self, e);
        }
    }
    let mut out = body.clone();
    let mut s = S { state, list_field, acc_var, scalar_field, n_var, bad: false };
    s.visit_expr_mut(&mut out);
    if s.bad {
        None
    } else {
        Some(out)
    }
}
