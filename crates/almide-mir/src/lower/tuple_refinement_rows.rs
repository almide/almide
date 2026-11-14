// The ordered refinement chain over a tuple of BORROWED components, widened to the
// rows the column compilers decline: arm-scope binds at nested paths, String-literal
// and list-shape tests, and arm guards. include!-spliced from control_b.rs, sharing
// the lower module's imports.

impl LowerCtx {
    /// EXECUTE `match (a, b, …) { rows }` as an ORDERED chain when the plain refinement
    /// chain ([`Self::try_lower_tuple_refinement_match`]) declines — the per-constructor
    /// tuple sub-match the variant regroup builds over a ctor's payload fields:
    ///
    /// ```text
    /// EWith(s, arms) => match (s, arms) {
    ///   (EApp(EVar(tag), [x]), arms) if starts_with(tag, "%in:") => A,
    ///   (s, arms) => B,
    /// }
    /// ```
    ///
    /// Column specialization would clone `B` into every branch that can still reach it
    /// (and decline, since `B` binds); here each arm body lowers EXACTLY ONCE, under
    /// `if <row cond> then <binds; body> else <rest of the chain>`. A row's condition is
    /// the short-circuit conjunction of its component tests — a variant tag (its nested
    /// fields read only under that tag), an Int/Bool/String literal, a list length (its
    /// elements read only under that length) — and, for a guarded row, the guard
    /// evaluated under the row's binds. Every read is a BORROW of a block the subject
    /// owns (`Load`/`LoadHandle`, the `bind_variant_arm` discipline), so the only
    /// ownership events are the arm bodies' own, balanced across the merge exactly as
    /// the plain chain balances them.
    ///
    /// Admitted: every component a plain `Var`, each heap one already BORROWED (a
    /// regrouped payload field — an owned component aliased by a binder would be a
    /// second owner); the last row irrefutable and unguarded. `None` rolls back.
    pub(crate) fn try_lower_tuple_refinement_rows(
        &mut self,
        elements: &[IrExpr],
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let last = arms.last()?;
        if last.guard.is_some() || !refine_irrefutable(&last.pattern) {
            return None;
        }
        let mut elems: Vec<(ValueId, Ty)> = Vec::with_capacity(elements.len());
        for e in elements {
            let IrExprKind::Var { id } = &e.kind else { return None };
            let v = self.value_for(*id).ok()?;
            if is_heap_ty(&e.ty) && !self.param_values.contains(&v) {
                return None;
            }
            elems.push((v, e.ty.clone()));
        }
        let rows_ok = arms.iter().all(|a| match &a.pattern {
            IrPattern::Wildcard => true,
            IrPattern::Tuple { elements: ps } => {
                ps.len() == elems.len()
                    && ps.iter().zip(elems.iter()).all(|(p, (_, t))| self.refine_pat_ok(p, t))
            }
            _ => false,
        });
        if !rows_ok {
            return None;
        }
        let ops_mark = self.ops.len();
        let lifted_mark = self.lifted.len();
        let lhh_mark = self.live_heap_handles.len();
        let dst = self.refine_rows_chain(&elems, arms, result_ty);
        if dst.is_none() {
            crate::trace::trace("ALMIDE_DBG_NESTED_MATCH", || {
                format!("[tuple-rows] chain declined for {}", self.fn_name)
            });
            self.probe_rollback(ops_mark, lifted_mark, lhh_mark);
        }
        dst
    }

    /// The declared case `name` of the (non-generic) custom variant `ty`.
    fn refine_case(&self, ty: &Ty, name: &str) -> Option<VariantCaseLayout> {
        let tyname = self.custom_variant_type_name(ty)?;
        let layout = self.variant_layouts.by_type.get(&tyname)?;
        if !layout.generics.is_empty() {
            return None;
        }
        layout.case_by_ctor(name).cloned()
    }

    /// Is `p` (over a component of type `ty`) inside the row subset?
    fn refine_pat_ok(&self, p: &IrPattern, ty: &Ty) -> bool {
        match p {
            IrPattern::Wildcard | IrPattern::Bind { .. } => true,
            IrPattern::Literal { expr } => match &expr.kind {
                IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } => !is_heap_ty(ty),
                IrExprKind::LitStr { .. } => matches!(ty, Ty::String),
                _ => false,
            },
            IrPattern::Constructor { name, args } => {
                self.refine_case(ty, name).is_some_and(|case| {
                    case.fields.len() == args.len()
                        && args.iter().zip(case.fields.iter()).all(|(a, (_, ft))| self.refine_pat_ok(a, ft))
                })
            }
            IrPattern::List { elements, rest: None } => refine_list_elem_ty(ty)
                .is_some_and(|et| elements.iter().all(|e| self.refine_pat_ok(e, &et))),
            _ => false,
        }
    }

    /// The chain: `if cond1 then {binds1; body1} else if … else {binds_n; body_n}`.
    fn refine_rows_chain(
        &mut self,
        elems: &[(ValueId, Ty)],
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<ValueId> {
        let heap = is_heap_ty(result_ty);
        let lower_arm = |s: &mut Self, body: &IrExpr| -> Option<ValueId> {
            if heap {
                s.lower_heap_result_arm(body, result_ty)
            } else {
                s.lower_scalar_arm(body)
            }
        };
        let arm = arms.first()?;
        let pats: Vec<IrPattern> = match &arm.pattern {
            IrPattern::Wildcard => vec![IrPattern::Wildcard; elems.len()],
            IrPattern::Tuple { elements } => elements.clone(),
            _ => return None,
        };
        let pat_cond = self.refine_row_cond(elems, &pats)?;
        let cond = match (pat_cond, &arm.guard) {
            // An unguarded irrefutable row: it always runs, the rest is unreachable.
            (None, None) => {
                self.refine_row_binds(elems, &pats)?;
                return lower_arm(self, &arm.body);
            }
            (_, Some(g)) => self.refine_guard_cond(elems, &pats, pat_cond, g)?,
            (Some(c), None) => c,
        };
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        let outer: Vec<ValueId> = self.live_heap_handles.clone();
        self.refine_row_binds(elems, &pats)?;
        let then_v = lower_arm(self, &arm.body)?;
        let consumed_by_then = self.consumed_since(&outer);
        let else_marker_at = self.ops.len();
        self.ops.push(Op::Else { val: Some(then_v) });
        let live_after_then: Vec<ValueId> = self.live_heap_handles.clone();
        let rest_v = self.refine_rows_chain(elems, &arms[1..], result_ty)?;
        let consumed_by_else = self.consumed_since(&live_after_then);
        self.balance_arm_releases(&consumed_by_then, &consumed_by_else, else_marker_at);
        self.ops.push(Op::EndIf { val: Some(rest_v) });
        Some(dst)
    }

    /// A guarded row's condition: the guard evaluated under the row's binds, only once
    /// the pattern holds (`if pat then guard else 0` — the binds read nothing the
    /// pattern did not establish).
    fn refine_guard_cond(
        &mut self,
        elems: &[(ValueId, Ty)],
        pats: &[IrPattern],
        pat_cond: Option<ValueId>,
        guard: &IrExpr,
    ) -> Option<ValueId> {
        let Some(pc) = pat_cond else {
            self.refine_row_binds(elems, pats)?;
            return self.lower_scalar_arm(guard);
        };
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond: pc, dst: Some(dst) });
        self.refine_row_binds(elems, pats)?;
        let g = self.lower_scalar_arm(guard)?;
        self.ops.push(Op::Else { val: Some(g) });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::EndIf { val: Some(zero) });
        Some(dst)
    }

    /// The conjunction of a row's component tests (`None`: the row is irrefutable).
    fn refine_row_cond(&mut self, elems: &[(ValueId, Ty)], pats: &[IrPattern]) -> Option<Option<ValueId>> {
        let mut conj: Option<ValueId> = None;
        for (p, (v, ty)) in pats.iter().zip(elems.iter()) {
            if let Some(c) = self.refine_cond(*v, p, ty)? {
                conj = Some(self.refine_and(conj, c));
            }
        }
        Some(conj)
    }

    fn refine_and(&mut self, prev: Option<ValueId>, c: ValueId) -> ValueId {
        match prev {
            None => c,
            Some(p) => {
                let d = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: d, op: IntOp::Mul, a: p, b: c });
                d
            }
        }
    }

    /// The test of `p` over the component value `v` (`Some(None)`: always matches).
    fn refine_cond(&mut self, v: ValueId, p: &IrPattern, ty: &Ty) -> Option<Option<ValueId>> {
        match p {
            IrPattern::Wildcard | IrPattern::Bind { .. } => Some(None),
            IrPattern::Literal { expr } => self.refine_literal_cond(v, expr).map(Some),
            IrPattern::Constructor { name, args } => self.refine_ctor_cond(v, name, args, ty).map(Some),
            IrPattern::List { elements, rest: None } => self.refine_list_cond(v, elements, ty).map(Some),
            _ => None,
        }
    }

    fn refine_literal_cond(&mut self, v: ValueId, expr: &IrExpr) -> Option<ValueId> {
        let value = match &expr.kind {
            IrExprKind::LitInt { value } => *value,
            IrExprKind::LitBool { value } => *value as i64,
            // A String literal: the ordinary `s == "lit"` (the `string.eq` byte compare
            // over the borrowed handle), in its own arm frame for the literal's temp.
            IrExprKind::LitStr { .. } => {
                let s = self.refine_synth_var(v);
                let test = refine_expr(
                    IrExprKind::BinOp {
                        op: almide_ir::BinOp::Eq,
                        left: Box::new(refine_expr(IrExprKind::Var { id: s }, Ty::String)),
                        right: Box::new(expr.clone()),
                    },
                    Ty::Bool,
                );
                return self.lower_scalar_arm(&test);
            }
            _ => return None,
        };
        let lit = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: lit, value });
        let c = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: c, op: IntOp::Eq, a: v, b: lit });
        Some(c)
    }

    /// A fresh synthetic var naming the (already lowered) value `v`.
    fn refine_synth_var(&mut self, v: ValueId) -> VarId {
        let s = self.fresh_synth_var();
        self.value_of.insert(s, v);
        s
    }

    /// The tag test of a variant block, and — only under a matching tag (a flat-marker
    /// `if`, so an out-of-case slot is never read) — its refutable fields' tests.
    fn refine_ctor_cond(&mut self, v: ValueId, name: &str, args: &[IrPattern], ty: &Ty) -> Option<ValueId> {
        use crate::PrimKind;
        let case = self.refine_case(ty, name)?;
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![v] });
        let tagv = self.load_at_offset(h, layout::slot_offset(0) as i64, PrimKind::Load { width: 8 });
        let want = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: want, value: case.tag as i64 });
        let eq = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: eq, op: IntOp::Eq, a: tagv, b: want });
        let refut: Vec<usize> = (0..args.len()).filter(|i| !refine_irrefutable(&args[*i])).collect();
        if refut.is_empty() {
            return Some(eq);
        }
        self.refine_under(eq, |s| {
            let mut conj = None;
            for i in refut {
                let fty = case.fields[i].1.clone();
                let fv = s.refine_field(h, 1 + i, &fty);
                let c = s.refine_cond(fv, &args[i], &fty)??;
                conj = Some(s.refine_and(conj, c));
            }
            conj
        })
    }

    /// The length test of a list, and — only under a matching length — its refutable
    /// elements' tests.
    fn refine_list_cond(&mut self, v: ValueId, elements: &[IrPattern], ty: &Ty) -> Option<ValueId> {
        let et = refine_list_elem_ty(ty)?;
        let xs = self.refine_synth_var(v);
        let len = refine_expr(
            IrExprKind::Call {
                target: almide_ir::CallTarget::Module {
                    module: almide_lang::intern::sym("list"),
                    func: almide_lang::intern::sym("len"),
                    def_id: None,
                },
                args: vec![refine_expr(IrExprKind::Var { id: xs }, ty.clone())],
                type_args: Vec::new(),
            },
            Ty::Int,
        );
        let test = refine_expr(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::Eq,
                left: Box::new(len),
                right: Box::new(refine_expr(IrExprKind::LitInt { value: elements.len() as i64 }, Ty::Int)),
            },
            Ty::Bool,
        );
        let eq = self.lower_scalar_arm(&test)?;
        let refut: Vec<usize> = (0..elements.len()).filter(|i| !refine_irrefutable(&elements[*i])).collect();
        if refut.is_empty() {
            return Some(eq);
        }
        self.refine_under(eq, |s| {
            let mut conj = None;
            for i in refut {
                let ev = s.refine_list_elem(v, i, &et)?;
                let c = s.refine_cond(ev, &elements[i], &et)??;
                conj = Some(s.refine_and(conj, c));
            }
            conj
        })
    }

    /// `if cond then <inner> else 0` on the flat markers.
    fn refine_under(
        &mut self,
        cond: ValueId,
        inner: impl FnOnce(&mut Self) -> Option<ValueId>,
    ) -> Option<ValueId> {
        let dst = self.fresh_value();
        self.ops.push(Op::IfThen { cond, dst: Some(dst) });
        let c = inner(self)?;
        self.ops.push(Op::Else { val: Some(c) });
        let zero = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: zero, value: 0 });
        self.ops.push(Op::EndIf { val: Some(zero) });
        Some(dst)
    }

    /// Slot `slot` of the variant block behind handle `h`: a BORROWED handle for a heap
    /// field, the raw i64 for a scalar one (only ever a literal-test operand — a scalar
    /// BINDER reads through `bind_variant_arm`).
    fn refine_field(&mut self, h: ValueId, slot: usize, fty: &Ty) -> ValueId {
        use crate::PrimKind;
        let off = layout::slot_offset(slot) as i64;
        if is_heap_ty(fty) {
            let p = self.load_at_offset(h, off, PrimKind::LoadHandle);
            self.aggregate_blocks.insert(p, crate::render_native_rec::block_key(fty));
            self.param_values.insert(p);
            p
        } else {
            self.load_at_offset(h, off, PrimKind::Load { width: 8 })
        }
    }

    /// Element `i` of the heap-element list `v`, BORROWED at the bounds-checked
    /// `$elem_addr` (the list owns it — `try_lower_heap_list_elem_borrow`'s read).
    fn refine_list_elem(&mut self, v: ValueId, i: usize, et: &Ty) -> Option<ValueId> {
        use crate::PrimKind;
        if !is_heap_ty(et) {
            return None;
        }
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![v] });
        let idx = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: idx, value: i as i64 });
        let addr = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::ElemAddr, dst: Some(addr), args: vec![h, idx] });
        let dst = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(dst), args: vec![addr] });
        self.aggregate_blocks.insert(dst, crate::render_native_rec::block_key(et));
        self.param_values.insert(dst);
        Some(dst)
    }

    /// Bind every binder of a row (run where the row's condition holds).
    fn refine_row_binds(&mut self, elems: &[(ValueId, Ty)], pats: &[IrPattern]) -> Option<()> {
        for (p, (v, ty)) in pats.iter().zip(elems.iter()) {
            self.refine_binds(*v, p, ty)?;
        }
        Some(())
    }

    fn refine_binds(&mut self, v: ValueId, p: &IrPattern, ty: &Ty) -> Option<()> {
        match p {
            IrPattern::Wildcard | IrPattern::Literal { .. } => Some(()),
            // The component itself: a scalar value, or a handle already borrowed.
            IrPattern::Bind { var, .. } => {
                self.value_of.insert(*var, v);
                Some(())
            }
            IrPattern::Constructor { name, args } => self.refine_ctor_binds(v, name, args, ty),
            IrPattern::List { elements, rest: None } => {
                let et = refine_list_elem_ty(ty)?;
                for (i, e) in elements.iter().enumerate() {
                    if !refine_has_bind(e) {
                        continue;
                    }
                    let ev = self.refine_list_elem(v, i, &et)?;
                    match e {
                        IrPattern::Bind { var, ty: bty } => self.bind_borrowed_heap_slot(ev, *var, bty),
                        _ => self.refine_binds(ev, e, &et)?,
                    }
                }
                Some(())
            }
            _ => None,
        }
    }

    /// A variant block's binders: its direct `Bind` fields through `bind_variant_arm`
    /// (scalar by slot read, heap as a borrowed slot handle), its nested refutable fields
    /// recursively.
    fn refine_ctor_binds(&mut self, v: ValueId, name: &str, args: &[IrPattern], ty: &Ty) -> Option<()> {
        if !args.iter().any(refine_has_bind) {
            return Some(());
        }
        let case = self.refine_case(ty, name)?;
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: crate::PrimKind::Handle, dst: Some(h), args: vec![v] });
        let mut binds = Vec::new();
        for (i, a) in args.iter().enumerate() {
            match a {
                IrPattern::Bind { var, ty: bty } => binds.push((1 + i, *var, is_heap_ty(bty), bty.clone())),
                _ if refine_has_bind(a) => {
                    let fty = case.fields[i].1.clone();
                    let fv = self.refine_field(h, 1 + i, &fty);
                    self.refine_binds(fv, a, &fty)?;
                }
                _ => {}
            }
        }
        if !binds.is_empty() {
            self.bind_variant_arm(&VariantArmKind::Ctor { tag: case.tag as i64, binds }, h, v);
        }
        Some(())
    }
}

/// A pattern that matches every value of its type (no test needed).
fn refine_irrefutable(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => true,
        IrPattern::Tuple { elements } => elements.iter().all(refine_irrefutable),
        _ => false,
    }
}

/// Does `p` bind any variable?
fn refine_has_bind(p: &IrPattern) -> bool {
    match p {
        IrPattern::Bind { .. } => true,
        IrPattern::Constructor { args, .. } => args.iter().any(refine_has_bind),
        IrPattern::List { elements, .. } | IrPattern::Tuple { elements } => elements.iter().any(refine_has_bind),
        _ => false,
    }
}

/// The element type of a heap-element `List[T]`.
fn refine_list_elem_ty(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && is_heap_ty(&a[0]) => Some(a[0].clone()),
        _ => None,
    }
}

fn refine_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}
