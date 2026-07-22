impl LowerCtx {
    /// Lower a fold-accumulator BODY that is a direct CALL (`(h, k) => step(h, k)` —
    /// the transformer layer fold) to a BARE fresh owned heap value: a Named user fn
    /// via `CallFn`, a pure Module fn via the routed self-host name. The result is
    /// NOT registered for scope-end drop — the caller's drop-old + `SetLocal` makes
    /// the loop slot its single owner. Unwraps a trivial `{ <call> }` block.
    fn try_lower_fold_acc_call(&mut self, body: &IrExpr) -> Option<ValueId> {
        use almide_ir::IrExprKind;
        let inner = match &body.kind {
            IrExprKind::Block { stmts, expr: Some(e) } if stmts.is_empty() => e.as_ref(),
            _ => body,
        };
        match &inner.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let lowered = self.lower_call_args(args).ok()?;
                let repr = repr_of(&inner.ty).ok()?;
                let dst = self.fresh_value();
                self.ops.push(Op::CallFn {
                    dst: Some(dst),
                    name: name.as_str().to_string(),
                    args: lowered,
                    result: Some(repr),
                });
                Some(dst)
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if crate::purity::is_pure(module.as_str(), func.as_str()) =>
            {
                let mark = self.live_heap_handles.len();
                let v = self.lower_pure_module_value_call(module, func, args, &inner.ty).ok()?;
                // lower_pure_module_value_call tracks its result for scope-end drop —
                // the slot must be the SINGLE owner, so untrack it (bare).
                if let Some(pos) = self.live_heap_handles.iter().rposition(|&h| h == v) {
                    self.live_heap_handles.remove(pos);
                }
                let _ = mark;
                Some(v)
            }
            _ => None,
        }
    }

    /// C1 defunc for a SCALAR-TUPLE accumulator fold — the argmax idiom:
    /// `enumerate(xs) |> fold((0, -1.0e308), (acc, entry) => { let (bi,bv)=acc;
    /// let (i,v)=entry; if v > bv then (i,v) else (bi,bv) })`. The accumulator's
    /// two SCALAR components live in two mutable locals (no tuple block per
    /// iteration); the body's tail must be a component-wise tuple (optionally
    /// under one `if`). After the loop the pair is materialized ONCE as a real
    /// 2-slot block (registered as a materialized aggregate, so a downstream
    /// `.0`/`.1` projection reads the real slot). Fully rolled back on any
    /// out-of-subset shape (the caller walls).
    #[allow(clippy::too_many_arguments)]
    fn try_lower_defunc_scalar_tuple_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        fuse_index: Option<VarId>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_ir::{IrPattern, IrStmtKind};
        // Accumulator type: a 2-tuple of scalars; the result is the same tuple.
        let (t1, t2) = match result_ty {
            Ty::Tuple(ts) if ts.len() == 2 && !is_heap_ty(&ts[0]) && !is_heap_ty(&ts[1]) => {
                (ts[0].clone(), ts[1].clone())
            }
            _ => return None,
        };
        let _ = (&t1, &t2);
        // init = (e1, e2), both scalar-lowerable.
        let IrExprKind::Tuple { elements: init_elems } = &init.kind else { return None };
        if init_elems.len() != 2 {
            return None;
        }
        // body = Block{ [let (a1, a2) = acc, ...maybe nothing else], tail }
        let acc_var = params[0].0;
        let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else { return None };
        if stmts.is_empty() {
            return None;
        }
        // stmts[0] must be the acc destructure; any FURTHER stmts (`let a = …; let rank = …`
        // — the best_pair_index preamble) lower per-iteration via the ordinary stmt
        // machinery inside the loop (their heap temps freed within the iteration).
        let extra_stmts = &stmts[1..];
        let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value } =
            &stmts[0].kind
        else {
            return None;
        };
        if pats.len() != 2 || !matches!(&value.kind, IrExprKind::Var { id } if *id == acc_var) {
            return None;
        }
        let a1 = match &pats[0] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        let a2 = match &pats[1] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        // tail: an if-TREE whose every leaf is a 2-tuple (`if a then (..) else if b
        // then (..) else (..)` — the find_chunk chain). PROJECT the tree per
        // component: the same conditions, each leaf replaced by its idx-th element
        // (conditions are pure scalar expressions — the scalar path admits nothing
        // effectful — so evaluating them once per component is value-identical).
        fn project(e: &IrExpr, idx: usize, comp_ty: &Ty) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::Tuple { elements } if elements.len() == 2 => {
                    Some(elements[idx].clone())
                }
                IrExprKind::If { cond, then, else_ } => {
                    let t = project(then, idx, comp_ty)?;
                    let el = project(else_, idx, comp_ty)?;
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: cond.clone(),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: comp_ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                _ => None,
            }
        }
        let proj1 = project(tail, 0, &t1)?;
        let proj2 = project(tail, 1, &t2)?;

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let vo_snapshot = self.value_of.clone();
        let mut fail = || -> Option<ValueId> {
            None
        };
        let _ = &mut fail;
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.value_of = vo_snapshot;
                return None;
            }};
        }

        // Seed the two component locals.
        let s1 = match self.lower_scalar_value(&init_elems[0]) {
            Some(v) => v,
            None => bail!(),
        };
        let s2 = match self.lower_scalar_value(&init_elems[1]) {
            Some(v) => v,
            None => bail!(),
        };
        self.value_of.insert(a1, s1);
        self.value_of.insert(a2, s2);

        // Borrow the source, read the length.
        let list_v = match self.lower_call_args(std::slice::from_ref(xs)).ok().and_then(|mut a| a.pop())
        {
            Some(CallArg::Handle(v)) => v,
            _ => bail!(),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![list_v] });
        let len_v = self.load_at_offset(h, 4, PrimKind::Load { width: 4 });

        let i_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: i_v, value: 0 });
        let one_v = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one_v, value: 1 });
        self.ops.push(Op::LoopStart);
        let cond_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: cond_v, op: IntOp::Lt, a: i_v, b: len_v });
        self.ops.push(Op::LoopBreakUnless { cond: cond_v });

        // elem = xs[i] (a scalar slot or a borrowed heap handle).
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base, b: i8_v });
        let src_heap = matches!(&xs.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 && is_heap_ty(&a[0]));
        let elem = self.fresh_value();
        let rk = if src_heap { PrimKind::LoadHandle } else { PrimKind::Load { width: 8 } };
        self.ops.push(Op::Prim { kind: rk, dst: Some(elem), args: vec![addr] });
        self.value_of.insert(params[1].0, elem);
        if let Some(iv) = fuse_index {
            self.value_of.insert(iv, i_v);
        }

        // Per-iteration preamble stmts, then per-component evaluation of the projected
        // trees (both read the PRE-update locals), then a simultaneous SetLocal pair.
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let updates: Option<(ValueId, ValueId)> = (|| {
            for st in extra_stmts {
                if self.lower_stmt(st).is_err() {
                    return None;
                }
            }
            let v1 = self.lower_scalar_value(&proj1)?;
            let v2 = self.lower_scalar_value(&proj2)?;
            Some((v1, v2))
        })();
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        let (n1, n2) = match updates {
            Some(p) => p,
            None => {
                self.value_of = vo_snapshot;
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }
        };
        // Free the iteration's owned temps (the `?? ""` copies) before the back-edge.
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::SetLocal { local: s1, src: n1 });
        self.ops.push(Op::SetLocal { local: s2, src: n2 });
        self.ops.push(Op::IntBinOp { dst: i_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::LoopEnd);

        // Materialize the resulting tuple ONCE (2 uniform slots) and register its
        // read shape so a downstream `.0`/`.1` loads the real slot.
        let two = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: two, value: 2 });
        let tup = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: tup,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: two },
        });
        let th = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
        let sl0 = self.load_addr(th, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![sl0, s1] });
        let sl1 = self.load_addr(th, 20);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![sl1, s2] });
        // NOT pushed to live_heap_handles — the CALLER tracks the returned value
        // exactly like a self-host combinator result (a second push double-drops).
        self.materialized_aggregates.insert(tup);
        self.last_call_had_unlifted_closure = false;
        Some(tup)
    }

    /// EXECUTE `let e = match <Option[(s1, s2)]> { some(p) => p, none => (f1, f2) }` —
    /// the let-BOUND scalar-tuple Option match (the fft `list.get(xs,k) ?? (0.0,0.0)`
    /// pick after the tuple-unwrap_or desugar). The let-bound heap-result match is
    /// normally unlowerable (per-arm move-out vs scope-end drop breaks the flat cert),
    /// but a SCALAR-TUPLE payload needs no per-arm alloc at all: merge each COMPONENT
    /// through the scalar IfThen skeleton (Some → the payload tuple's slot, None → the
    /// fallback component), then build ONE 2-slot block the binding owns — a single
    /// `i…d` object, cert-clean by construction. Returns the owned tuple ValueId, or
    /// `None` (fully rolled back) outside the exact shape.
    pub(crate) fn try_lower_scalar_tuple_option_match_bind(
        &mut self,
        subject: &IrExpr,
        arms: &[almide_ir::IrMatchArm],
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::{IrMatchArm, IrPattern};
        // Option[<2-scalar tuple>] subject, exactly two guard-free arms.
        let tuple_ty = match &subject.ty {
            Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => match &a[0] {
                Ty::Tuple(ts)
                    if ts.len() == 2 && !is_heap_ty(&ts[0]) && !is_heap_ty(&ts[1]) =>
                {
                    a[0].clone()
                }
                _ => return None,
            },
            _ => return None,
        };
        if arms.len() != 2 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        let find = |want_some: bool| -> Option<&IrMatchArm> {
            arms.iter().find(|a| match &a.pattern {
                IrPattern::Some { .. } => want_some,
                IrPattern::None | IrPattern::Wildcard => !want_some,
                _ => false,
            })
        };
        let some_arm = find(true)?;
        let none_arm = find(false)?;
        // some(p) => Var(p) (the payload passthrough) — the only admitted Some body.
        let p_var = match &some_arm.pattern {
            IrPattern::Some { inner } => match &**inner {
                IrPattern::Bind { var, .. } => *var,
                _ => return None,
            },
            _ => return None,
        };
        if !matches!(&some_arm.body.kind, IrExprKind::Var { id } if *id == p_var) {
            return None;
        }
        // none => (f1, f2) with scalar-lowerable components.
        let IrExprKind::Tuple { elements: fb } = &none_arm.body.kind else { return None };
        if fb.len() != 2 {
            return None;
        }
        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                return None;
            }};
        }
        // Materialize/borrow the Option subject (a self-host option call is tracked +
        // dropped at scope end by the caller's machinery; a Var is borrowed).
        let subj = match self.lower_call_args(std::slice::from_ref(subject)) {
            Ok(mut a) => match a.pop() {
                Some(CallArg::Handle(v)) => v,
                _ => bail!(),
            },
            Err(_) => bail!(),
        };
        let h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![subj] });
        let tag = {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 4 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let t = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(t), args: vec![addr] });
            t
        };
        // Component k: IfThen(tag) → payload.slot[k] (LoadHandle @12 then load64 @12/@20),
        // Else → fallback component (pure scalar).
        let mut comps: [ValueId; 2] = [ValueId(0), ValueId(0)];
        for (k, comp) in comps.iter_mut().enumerate() {
            let m = self.fresh_value();
            self.ops.push(Op::IfThen { cond: tag, dst: Some(m) });
            let ph = {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
                let p = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(p), args: vec![addr] });
                p
            };
            // ph is an i32 handle local — widen through Prim::Handle before i64 address math.
            let ph64 = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(ph64), args: vec![ph] });
            let slot = {
                let off = self.fresh_value();
                self.ops.push(Op::ConstInt { dst: off, value: 12 + (k as i64) * 8 });
                let addr = self.fresh_value();
                self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: ph64, b: off });
                let v = self.fresh_value();
                self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(v), args: vec![addr] });
                v
            };
            self.ops.push(Op::Else { val: Some(slot) });
            let fbv = match self.lower_scalar_value(&fb[k]) {
                Some(v) => v,
                None => bail!(),
            };
            self.ops.push(Op::EndIf { val: Some(fbv) });
            *comp = m;
        }
        // ONE owned 2-slot block for the binding (the single cert object).
        let two = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: two, value: 2 });
        let tup = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: tup,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: two },
        });
        let th = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(th), args: vec![tup] });
        for (k, comp) in comps.iter().enumerate() {
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 12 + (k as i64) * 8 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: th, b: off });
            self.ops.push(Op::Prim {
                kind: PrimKind::Store { width: 8 },
                dst: None,
                args: vec![addr, *comp],
            });
        }
        let _ = tuple_ty;
        Some(tup)
    }
}
