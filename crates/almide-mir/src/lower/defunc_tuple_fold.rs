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

    /// C1 defunc for a `(scalar, Option[scalar])` accumulator fold — the wav
    /// find_chunk_at scanner: `fold(range, (pos, none), (state, _) => { let (p, found)
    /// = state; match found { some(_) => state, none => <if-tree over (p', none|some)> } })`.
    /// The Option component runs as TWO scalar locals (tag: 0=none/1=some, payload);
    /// every tail leaf is projected per SUB-component (the match-over-found becomes an
    /// `if tag != 0`). After the loop the Option materializes ONCE — a cap-1 block whose
    /// len field is OVERWRITTEN with the tag (len-as-tag, no branch) and which this
    /// SCOPE owns; the result tuple holds it as a BORROWED slot (view semantics — the
    /// downstream `.1` projection Dup-acquires). Fully rolled back outside the shape.
    ///
    /// The body lowers ONCE per iteration as a UNIT control tree (Block stmts and `if`
    /// conds emitted a single time); each tuple LEAF computes all three component values
    /// and SetLocals them together. (The earlier per-sub-component PROJECTION re-lowered
    /// shared preambles up to 3× — value-identical because scalar-only, but it emitted
    /// 3× the CallFn ops and permanently tripped the corpus `mir <= ir` caps gate.)
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_lower_defunc_opt_tuple_fold(
        &mut self,
        xs: &IrExpr,
        params: &[(VarId, Ty)],
        body: &IrExpr,
        init: &IrExpr,
        fuse_index: Option<VarId>,
        result_ty: &Ty,
    ) -> Option<ValueId> {
        use crate::{IntOp, PrimKind};
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_ir::{BinOp, IrPattern, IrStmtKind};
        // (scalar, Option[scalar]) accumulator only.
        match result_ty {
            Ty::Tuple(ts)
                if ts.len() == 2
                    && !is_heap_ty(&ts[0])
                    && matches!(&ts[1],
                        Ty::Applied(TypeConstructorId::Option, a)
                            if a.len() == 1 && !is_heap_ty(&a[0])) => {}
            _ => return None,
        }
        // Seed: (e0, none).
        let IrExprKind::Tuple { elements: init_elems } = &init.kind else { return None };
        if init_elems.len() != 2 || !matches!(init_elems[1].kind, IrExprKind::OptionNone) {
            return None;
        }
        let acc_var = params[0].0;
        let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else { return None };
        if stmts.len() != 1 {
            return None;
        }
        let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value } =
            &stmts[0].kind
        else {
            return None;
        };
        if pats.len() != 2 || !matches!(&value.kind, IrExprKind::Var { id } if *id == acc_var) {
            return None;
        }
        let p_var = match &pats[0] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        let found_var = match &pats[1] {
            IrPattern::Bind { var, .. } => *var,
            _ => return None,
        };
        // Synthetic vars standing for the tag/payload locals inside projected trees.
        let base = crate::lower::max_var_id(body).max(crate::lower::max_var_id(init)) + 1;
        let ft = VarId(base);
        let fv = VarId(base + 1);

        #[derive(Clone, Copy, PartialEq)]
        enum Comp {
            C0,
            C1Tag,
            C1Val,
        }
        fn subst_var(e: &IrExpr, from: VarId, to: VarId) -> IrExpr {
            let mut out = e.clone();
            fn walk(e: IrExpr, from: VarId, to: VarId) -> IrExpr {
                let mut e = e.map_children(&mut |c| walk(c, from, to));
                if let IrExprKind::Var { id } = &mut e.kind {
                    if *id == from {
                        *id = to;
                    }
                }
                e
            }
            out = walk(out, from, to);
            out
        }
        fn int_expr(kind: IrExprKind, like: &IrExpr) -> IrExpr {
            IrExpr { kind, ty: Ty::Int, span: like.span.clone(), def_id: like.def_id }
        }
        fn tag_of(e: &IrExpr, found_var: VarId, ft: VarId) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::OptionNone => Some(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                IrExprKind::OptionSome { .. } => {
                    Some(int_expr(IrExprKind::LitInt { value: 1 }, e))
                }
                IrExprKind::Var { id } if *id == found_var => {
                    Some(int_expr(IrExprKind::Var { id: ft }, e))
                }
                _ => None,
            }
        }
        fn val_of(e: &IrExpr, found_var: VarId, fv: VarId) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::OptionNone => Some(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                IrExprKind::OptionSome { expr } => Some((**expr).clone()),
                IrExprKind::Var { id } if *id == found_var => {
                    Some(int_expr(IrExprKind::Var { id: fv }, e))
                }
                _ => None,
            }
        }
        fn project(
            e: &IrExpr,
            comp: Comp,
            acc_var: VarId,
            p_var: VarId,
            found_var: VarId,
            ft: VarId,
            fv: VarId,
        ) -> Option<IrExpr> {
            match &e.kind {
                IrExprKind::Tuple { elements } if elements.len() == 2 => match comp {
                    Comp::C0 => Some(elements[0].clone()),
                    Comp::C1Tag => tag_of(&elements[1], found_var, ft),
                    Comp::C1Val => val_of(&elements[1], found_var, fv),
                },
                IrExprKind::Var { id } if *id == acc_var => Some(match comp {
                    Comp::C0 => int_expr(IrExprKind::Var { id: p_var }, e),
                    Comp::C1Tag => int_expr(IrExprKind::Var { id: ft }, e),
                    Comp::C1Val => int_expr(IrExprKind::Var { id: fv }, e),
                }),
                IrExprKind::If { cond, then, else_ } => {
                    let t = project(then, comp, acc_var, p_var, found_var, ft, fv)?;
                    let el = project(else_, comp, acc_var, p_var, found_var, ft, fv)?;
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: cond.clone(),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let t = project(tail, comp, acc_var, p_var, found_var, ft, fv)?;
                    Some(IrExpr {
                        kind: IrExprKind::Block {
                            stmts: stmts.clone(),
                            expr: Some(Box::new(t)),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                // `match found { some(b) => X, none => Y }` → `if ft != 0 then X[b:=fv] else Y`.
                IrExprKind::Match { subject, arms }
                    if matches!(&subject.kind, IrExprKind::Var { id } if *id == found_var)
                        && arms.len() == 2
                        && arms.iter().all(|a| a.guard.is_none()) =>
                {
                    let some_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Some { .. }))?;
                    let none_arm = arms
                        .iter()
                        .find(|a| matches!(a.pattern, IrPattern::None | IrPattern::Wildcard))?;
                    let some_body = match &some_arm.pattern {
                        IrPattern::Some { inner } => match &**inner {
                            IrPattern::Bind { var, .. } => subst_var(&some_arm.body, *var, fv),
                            IrPattern::Wildcard => some_arm.body.clone(),
                            _ => return None,
                        },
                        _ => return None,
                    };
                    let t = project(&some_body, comp, acc_var, p_var, found_var, ft, fv)?;
                    let el = project(&none_arm.body, comp, acc_var, p_var, found_var, ft, fv)?;
                    let cond = int_expr(
                        IrExprKind::BinOp {
                            op: BinOp::Neq,
                            left: Box::new(int_expr(IrExprKind::Var { id: ft }, e)),
                            right: Box::new(int_expr(IrExprKind::LitInt { value: 0 }, e)),
                        },
                        e,
                    );
                    Some(IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(t),
                            else_: Box::new(el),
                        },
                        ty: Ty::Int,
                        span: e.span.clone(),
                        def_id: e.def_id,
                    })
                }
                _ => None,
            }
        }
        // The tail must be a projectable component tree (the gate) — checked up front so
        // the single-pass emitter below never leaves partial control flow on a decline.
        if project(tail, Comp::C0, acc_var, p_var, found_var, ft, fv).is_none()
            || project(tail, Comp::C1Tag, acc_var, p_var, found_var, ft, fv).is_none()
            || project(tail, Comp::C1Val, acc_var, p_var, found_var, ft, fv).is_none()
        {
            return None;
        }

        let ops_mark = self.ops.len();
        let lhh_mark = self.live_heap_handles.len();
        let vo_snapshot = self.value_of.clone();
        macro_rules! bail {
            () => {{
                self.ops.truncate(ops_mark);
                self.live_heap_handles.truncate(lhh_mark);
                self.value_of = vo_snapshot;
                return None;
            }};
        }
        // Locals: s0 = seed.0; tag = 0; val = 0.
        let s0 = match self.lower_scalar_value(&init_elems[0]) {
            Some(v) => v,
            None => bail!(),
        };
        let tloc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: tloc, value: 0 });
        let vloc = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: vloc, value: 0 });
        self.value_of.insert(p_var, s0);
        self.value_of.insert(ft, tloc);
        self.value_of.insert(fv, vloc);

        // Source loop (same skeleton as the scalar-tuple fold).
        let list_v = match self
            .lower_call_args(std::slice::from_ref(xs))
            .ok()
            .and_then(|mut a| a.pop())
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
        let eight = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: eight, value: 8 });
        let i8_v = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: i8_v, op: IntOp::Mul, a: i_v, b: eight });
        let base_addr = self.load_addr(h, 12);
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: base_addr, b: i8_v });
        let elem = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(elem), args: vec![addr] });
        self.value_of.insert(params[1].0, elem);
        if let Some(iv) = fuse_index {
            self.value_of.insert(iv, i_v);
        }
        let body_mark = self.live_heap_handles.len();
        self.in_frame += 1;
        self.in_defunc_body += 1;
        self.scalar_loop_depth += 1;
        let emitted =
            self.emit_opt_tuple_fold_body(tail, acc_var, found_var, ft, fv, s0, tloc, vloc);
        self.scalar_loop_depth -= 1;
        self.in_defunc_body -= 1;
        self.in_frame -= 1;
        if emitted.is_none() {
            bail!();
        }
        self.drop_arm_locals(body_mark);
        self.ops.push(Op::IntBinOp { dst: i_v, op: IntOp::Add, a: i_v, b: one_v });
        self.ops.push(Op::LoopEnd);

        // Materialize the Option ONCE: a cap-1 len-as-tag block — store the payload,
        // then OVERWRITE len(@4) with the tag (0 → none, 1 → some; cap stays 1).
        let one2 = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: one2, value: 1 });
        let opt = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: opt,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: crate::Init::DynList { len: one2 },
        });
        let oh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(oh), args: vec![opt] });
        let pslot = self.load_addr(oh, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![pslot, vloc] });
        let lslot = self.load_addr(oh, 4);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![lslot, tloc] });
        // The SCOPE owns the Option block; the tuple below only borrows it.
        self.live_heap_handles.push(opt);

        // The (scalar, Option) result tuple — slot1 is the BORROWED Option handle.
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
        let s0slot = self.load_addr(th, 12);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s0slot, s0] });
        let s1slot = self.load_addr(th, 20);
        self.ops.push(Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args: vec![s1slot, oh] });
        self.materialized_aggregates.insert(tup);
        self.last_call_had_unlifted_closure = false;
        Some(tup)
    }

    /// SINGLE-PASS body emitter for the (scalar, Option[scalar]) fold: walk the tail as
    /// a UNIT control tree — Block statements and `if` conditions lower exactly ONCE —
    /// and at each tuple LEAF compute all three component values (scalar, tag, payload)
    /// before SetLocal-ing the three loop-carried locals together. A `state` leaf (the
    /// unchanged accumulator) emits nothing. The shape was pre-validated by `project`
    /// (the gate), so a `None` here only rolls back through the caller's marks.
    #[allow(clippy::too_many_arguments)]
    fn emit_opt_tuple_fold_body(
        &mut self,
        e: &IrExpr,
        acc_var: VarId,
        found_var: VarId,
        ft: VarId,
        fv: VarId,
        s0: ValueId,
        tloc: ValueId,
        vloc: ValueId,
    ) -> Option<()> {
        use almide_ir::IrPattern;
        match &e.kind {
            // The unchanged-accumulator leaf (`some(_) => state`): all three locals keep
            // their values — no ops.
            IrExprKind::Var { id } if *id == acc_var => Some(()),
            // A tuple LEAF `(e0, none | some(x) | found)` — compute all three component
            // values FIRST (they read the OLD locals), then SetLocal together.
            IrExprKind::Tuple { elements } if elements.len() == 2 => {
                let n0 = self.lower_scalar_value(&elements[0])?;
                let (nt, nv) = match &elements[1].kind {
                    IrExprKind::OptionNone => {
                        let z0 = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: z0, value: 0 });
                        let z1 = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: z1, value: 0 });
                        (z0, z1)
                    }
                    IrExprKind::OptionSome { expr } => {
                        let one = self.fresh_value();
                        self.ops.push(Op::ConstInt { dst: one, value: 1 });
                        let v = self.lower_scalar_value(expr)?;
                        (one, v)
                    }
                    IrExprKind::Var { id } if *id == found_var => (tloc, vloc),
                    _ => return None,
                };
                self.ops.push(Op::SetLocal { local: s0, src: n0 });
                self.ops.push(Op::SetLocal { local: tloc, src: nt });
                self.ops.push(Op::SetLocal { local: vloc, src: nv });
                Some(())
            }
            // A shared preamble Block: statements lower ONCE; per-iteration heap locals
            // (a `let id = bytes_to_string(…)` String) are freed within the frame.
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let mark = self.live_heap_handles.len();
                self.in_frame += 1;
                let mut ok = true;
                for st in stmts {
                    if self.lower_stmt(st).is_err() {
                        ok = false;
                        break;
                    }
                }
                let r = if ok {
                    self.emit_opt_tuple_fold_body(tail, acc_var, found_var, ft, fv, s0, tloc, vloc)
                } else {
                    None
                };
                self.drop_arm_locals(mark);
                self.in_frame -= 1;
                r
            }
            // `if cond then A else B` — the cond lowers ONCE (its transient temps freed
            // in the cond frame); each arm recurses as a unit arm (no merged value).
            IrExprKind::If { cond, then, else_ } => {
                let c = self.lower_heap_result_cond(cond)?;
                self.ops.push(Op::IfThen { cond: c, dst: None });
                let t = self.emit_opt_tuple_fold_body(then, acc_var, found_var, ft, fv, s0, tloc, vloc);
                self.ops.push(Op::Else { val: None });
                let el = t.and_then(|_| {
                    self.emit_opt_tuple_fold_body(else_, acc_var, found_var, ft, fv, s0, tloc, vloc)
                });
                self.ops.push(Op::EndIf { val: None });
                el
            }
            // `match found { some(b) => X, none => Y }` — the tag local IS the cond
            // (0 = none / 1 = some); the some-arm binder rebinds to the payload var.
            IrExprKind::Match { subject, arms }
                if matches!(&subject.kind, IrExprKind::Var { id } if *id == found_var)
                    && arms.len() == 2
                    && arms.iter().all(|a| a.guard.is_none()) =>
            {
                let some_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Some { .. }))?;
                let none_arm = arms
                    .iter()
                    .find(|a| matches!(a.pattern, IrPattern::None | IrPattern::Wildcard))?;
                let some_body = match &some_arm.pattern {
                    IrPattern::Some { inner } => match &**inner {
                        IrPattern::Bind { var, .. } => {
                            crate::lower::subst_var_ir(&some_arm.body, *var, fv)
                        }
                        IrPattern::Wildcard => some_arm.body.clone(),
                        _ => return None,
                    },
                    _ => return None,
                };
                self.ops.push(Op::IfThen { cond: tloc, dst: None });
                let t = self.emit_opt_tuple_fold_body(
                    &some_body, acc_var, found_var, ft, fv, s0, tloc, vloc,
                );
                self.ops.push(Op::Else { val: None });
                let el = t.and_then(|_| {
                    self.emit_opt_tuple_fold_body(
                        &none_arm.body, acc_var, found_var, ft, fv, s0, tloc, vloc,
                    )
                });
                self.ops.push(Op::EndIf { val: None });
                el
            }
            _ => None,
        }
    }
}
