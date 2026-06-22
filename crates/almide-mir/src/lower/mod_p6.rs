/// Detect + rewrite the LIST-ITERATOR heap-loop-carried pattern (oct_rec/bin_rec): a heap carried
/// param `cs` consumed in EVERY self-call ONLY as `list.drop(Var(cs), 1)`, with the body an outer
/// `match list.first(Var(cs)) { none => BASE, some(ch) => BODY }`. Returns the rewritten body (the
/// match → `if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`) + the fresh `idx`
/// VarId, and FLIPS `carried[ci]` to false (cs is now invariant — the iterator is `idx`, bumped per
/// self-call in `tco_rewrite`). `None` if the pattern does not hold. Cert-clean: the result is the
/// scalar-TCO loop over `idx` + the borrowed-stable `cs`; no heap back-edge merge.
fn try_list_iter_rewrite(
    fn_name: &str,
    body: &IrExpr,
    params: &[almide_ir::IrParam],
    fresh: u32,
) -> Option<(IrExpr, VarId, usize)> {
    // The body must be `match SUBJ { none => .., some(ch) => .. }` with SUBJ = `list.first(Var(cs))`.
    let IrExprKind::Match { subject, arms } = &body.kind else { return None };
    if arms.len() != 2 {
        return None;
    }
    let (cs_var, first_ty) = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "first" && args.len() == 1 =>
        {
            match &args[0].kind {
                IrExprKind::Var { id } => (*id, subject.ty.clone()),
                _ => return None,
            }
        }
        _ => return None,
    };
    // `cs` must be a param, and EVERY self-call must pass `list.drop(Var(cs), 1)` in its slot.
    let ci = params.iter().position(|p| p.var == cs_var)?;
    if !is_heap_ty(&params[ci].ty) {
        return None;
    }
    let is_drop1 = |e: &IrExpr| -> bool {
        matches!(&e.kind, IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
            if module.as_str() == "list" && func.as_str() == "drop" && args.len() == 2
                && matches!(&args[0].kind, IrExprKind::Var { id } if *id == cs_var)
                && matches!(&args[1].kind, IrExprKind::LitInt { value: 1 }))
    };
    // Collect EVERY self-call anywhere in the body (not just tail position) and require each to pass
    // `list.drop(cs,1)` in slot `ci` — so `cs` is a pure forward iterator with no other use.
    let mut ok = true;
    let mut any_self = false;
    {
        use almide_ir::visit::IrVisitor;
        struct W<'a> {
            fn_name: &'a str,
            ci: usize,
            is_drop1: &'a dyn Fn(&IrExpr) -> bool,
            ok: &'a mut bool,
            any: &'a mut bool,
        }
        impl IrVisitor for W<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                    if name.as_str() == self.fn_name {
                        *self.any = true;
                        if self.ci >= args.len() || !(self.is_drop1)(&args[self.ci]) {
                            *self.ok = false;
                        }
                    }
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        let mut w = W { fn_name, ci, is_drop1: &is_drop1, ok: &mut ok, any: &mut any_self };
        w.visit_expr(body);
    }
    if !ok || !any_self {
        return None;
    }
    // Parse the two arms: a `None` arm (the BASE) and a `Some(ch | _)` arm (the BODY). `ch` is a
    // scalar element bind (String element) — bound to `cs[idx]` (a borrow) in the rewrite.
    use almide_ir::IrPattern;
    let mut none_body: Option<&IrExpr> = None;
    let mut some_body: Option<(&IrExpr, Option<(VarId, Ty)>)> = None;
    for arm in arms {
        if arm.guard.is_some() {
            return None;
        }
        match &arm.pattern {
            IrPattern::None | IrPattern::Wildcard if none_body.is_none() => none_body = Some(&arm.body),
            IrPattern::Some { inner } if some_body.is_none() => {
                let bind = match inner.as_ref() {
                    IrPattern::Bind { var, ty } => Some((*var, ty.clone())),
                    IrPattern::Wildcard => None,
                    _ => return None,
                };
                some_body = Some((&arm.body, bind));
            }
            _ => return None,
        }
    }
    let none_body = none_body?;
    let (some_body, ch_bind) = some_body?;
    let idx = VarId(fresh);
    let elem_ty = match &first_ty {
        Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) if a.len() == 1 => {
            a[0].clone()
        }
        _ => return None,
    };
    // list.len(cs): clone the `list.first` subject node + retarget to `len`, typed Int.
    let len_call = match &subject.kind {
        IrExprKind::Call { target: CallTarget::Module { module, def_id, .. }, args, type_args } => {
            tco_ir(
                IrExprKind::Call {
                    target: CallTarget::Module {
                        module: *module,
                        func: almide_lang::intern::sym("len"),
                        def_id: *def_id,
                    },
                    args: args.clone(),
                    type_args: type_args.clone(),
                },
                Ty::Int,
            )
        }
        _ => return None,
    };
    // cond: `idx < list.len(cs)`
    let cond = tco_ir(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::Lt,
            left: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            right: Box::new(len_call),
        },
        Ty::Bool,
    );
    // then: `{ [let ch = cs[idx]]; SOME_BODY }` — the element BORROW.
    let mut then_stmts: Vec<IrStmt> = Vec::new();
    if let Some((ch_var, ch_ty)) = ch_bind {
        let elem = tco_ir(
            IrExprKind::IndexAccess {
                object: Box::new(tco_ir(IrExprKind::Var { id: cs_var }, params[ci].ty.clone())),
                index: Box::new(tco_ir(IrExprKind::Var { id: idx }, Ty::Int)),
            },
            elem_ty,
        );
        then_stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: ch_var,
                mutability: almide_ir::Mutability::Let,
                ty: ch_ty,
                value: elem,
            },
            span: None,
        });
    }
    let then_expr = tco_ir(
        IrExprKind::Block { stmts: then_stmts, expr: Some(Box::new(some_body.clone())) },
        body.ty.clone(),
    );
    let new_body = tco_ir(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_expr),
            else_: Box::new(none_body.clone()),
        },
        body.ty.clone(),
    );
    Some((new_body, idx, ci))
}

/// Find the FIRST heap-result `if`/`match` sitting in a call-ARGUMENT position anywhere within
/// `e` (recursing through nested calls), and return `(the branch, e with that branch replaced by
/// `Var(tmp)`)`. Each call's nested arguments are searched BEFORE the call's own direct args, so
/// `f(g(if..))` lifts the inner `if` first; the caller re-runs to a fixpoint to lift the rest.
/// Recursion is confined to `Call` nodes — a heap-branch that is NOT a call argument (e.g. a bare
/// `let s = if..`, or an `if`-arm interior) is left for the tail-duplication / per-arm machinery.
fn extract_first_callarg_branch(e: &IrExpr, tmp: VarId) -> Option<(IrExpr, IrExpr)> {
    // A TUPLE element may itself wrap a call-arg branch (`(value.str(if c then a else b), end)` — the
    // block_scalar/block_line return shape). Recurse into each element so the inner `if` is ANF-lifted
    // out (`let t = if c then a else b; (value.str(t), end)`), which `desugar_let_bound_heap_branch`
    // then tail-duplicates into a heap-result `if` with Tuple arms — both of which already lower.
    if let IrExprKind::Tuple { elements } = &e.kind {
        for (idx, el) in elements.iter().enumerate() {
            if let Some((branch, new_el)) = extract_first_callarg_branch(el, tmp) {
                let mut new_elements = elements.clone();
                new_elements[idx] = new_el;
                return Some((
                    branch,
                    IrExpr {
                        kind: IrExprKind::Tuple { elements: new_elements },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    },
                ));
            }
        }
        return None;
    }
    let IrExprKind::Call { target, args, type_args } = &e.kind else {
        return None;
    };
    let rebuild = |new_args: Vec<IrExpr>| IrExpr {
        kind: IrExprKind::Call {
            target: target.clone(),
            args: new_args,
            type_args: type_args.clone(),
        },
        ty: e.ty.clone(),
        span: e.span.clone(),
        def_id: e.def_id,
    };
    // (1) Innermost-first: a heap-branch nested inside a sub-call argument.
    for (idx, a) in args.iter().enumerate() {
        if let Some((branch, new_a)) = extract_first_callarg_branch(a, tmp) {
            let mut new_args = args.clone();
            new_args[idx] = new_a;
            return Some((branch, rebuild(new_args)));
        }
    }
    // (2) This call's own direct heap-branch argument.
    let arg_idx = args.iter().position(is_heap_branch)?;
    let branch = args[arg_idx].clone();
    let mut new_args = args.clone();
    new_args[arg_idx] = IrExpr {
        kind: IrExprKind::Var { id: tmp },
        ty: branch.ty.clone(),
        span: branch.span.clone(),
        def_id: None,
    };
    Some((branch, rebuild(new_args)))
}

/// ANF-LIFT a heap-result `if`/`match` out of a CALL-ARGUMENT into a fresh let-bind, so the
/// existing `desugar_let_bound_heap_branch` tail-duplication then makes it lower. Rewrites the
/// FIRST `f(.., if c then A else B, ..)` (including a nested `f(g(if..))` and the block's TAIL
/// expression `{ ..; f(if..) }`) to `let tmp = if c then A else B; f(.., tmp, ..)` (tmp = a fresh
/// `Var` of the arg's type). Returns `None` if no such call-arg exists. MUST be applied in BOTH
/// the lowering and the `count_ir_calls` gate via [`desugar_heap_branches`] (desugar-before-both)
/// so the duplicated calls stay 1:1 (mir == ir).
pub fn desugar_callarg_heap_if(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        // A BARE call/tuple body (not in a block) with a call-arg heap branch — `collect_block(..,
        // if list.is_empty(acc) then acc else acc+[""])`, a `block_line` if-arm reached via
        // `desugar_nested_branch_arms`. Lift the branch to a block `{ let tmp = if…; <body'> }`. The
        // fresh id comes from the FUNCTION-WIDE `next_var` counter, NOT `max_var_id(this arm)` — the arm
        // omits a sibling-arm var (`line`, used only in the else arm), so an arm-local max would alias
        // it and the renderer would read one arm's value in the other (block_line's `string.drop(v19)`).
        let tmp = VarId(*next_var);
        *next_var += 1;
        let (branch, new_body) = extract_first_callarg_branch(body, tmp)?;
        let lift = IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: almide_ir::Mutability::Let,
                ty: branch.ty.clone(),
                value: branch,
            },
            span: body.span.clone(),
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: vec![lift], expr: Some(Box::new(new_body)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    };
    let tmp = VarId(*next_var);
    *next_var += 1;
    // STATEMENT position: the first `Expr`/`Bind`/`Assign` whose value contains a call-arg branch.
    for (i, s) in stmts.iter().enumerate() {
        let value = match &s.kind {
            IrStmtKind::Expr { expr } => Some(expr),
            IrStmtKind::Bind { value, .. } => Some(value),
            IrStmtKind::Assign { value, .. } => Some(value),
            _ => None,
        };
        let Some(v) = value else { continue };
        let Some((branch, new_v)) = extract_first_callarg_branch(v, tmp) else {
            continue;
        };
        let lift = IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: almide_ir::Mutability::Let,
                ty: branch.ty.clone(),
                value: branch,
            },
            span: s.span.clone(),
        };
        let new_stmt = IrStmt {
            kind: match &s.kind {
                IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: new_v },
                IrStmtKind::Bind { var, mutability, ty, .. } => IrStmtKind::Bind {
                    var: *var,
                    mutability: *mutability,
                    ty: ty.clone(),
                    value: new_v,
                },
                IrStmtKind::Assign { var, .. } => IrStmtKind::Assign { var: *var, value: new_v },
                other => other.clone(),
            },
            span: s.span.clone(),
        };
        let mut new_stmts: Vec<IrStmt> = stmts[..i].to_vec();
        new_stmts.push(lift);
        new_stmts.push(new_stmt);
        new_stmts.extend(stmts[i + 1..].iter().cloned());
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    // TAIL position: `{ ..; f(if..) }` — the call is the block's return expression, not a
    // statement, so the lifted `let tmp = if..` is APPENDED and the rewritten call becomes the
    // new tail. The tail-duplication then pushes that tail into each arm.
    if let Some(t) = tail.as_deref() {
        if let Some((branch, new_t)) = extract_first_callarg_branch(t, tmp) {
            let lift = IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: almide_ir::Mutability::Let,
                    ty: branch.ty.clone(),
                    value: branch,
                },
                span: t.span.clone(),
            };
            let mut new_stmts = stmts.clone();
            new_stmts.push(lift);
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_t)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    None
}

/// Apply the call-arg ANF-lift ([`desugar_callarg_heap_if`]) and the heap-branch tail-duplication
/// ([`desugar_let_bound_heap_branch`]) repeatedly to a FIXPOINT — the exact rewrite sequence
/// `lower_body_into` performs before lowering. Both the lowering and the `count_ir_calls` caps gate
/// call this, so the duplicated calls are counted 1:1 (mir == ir) regardless of how many branches
/// a body lifts. Returns `None` if the body is already in normal form (no rewrite applied).
pub fn desugar_heap_branches(body: &IrExpr) -> Option<IrExpr> {
    // Seed a FUNCTION-WIDE fresh-VarId counter ABOVE every id in the whole body, then thread it through
    // the recursion so a lift inside one `if` arm never reuses an id live in a SIBLING arm (block_line's
    // `string.drop` read the then-arm's concat because an arm-local `max_var_id` aliased `line`).
    let mut next_var = max_var_id(body) + 1;
    desugar_heap_branches_inner(body, &mut next_var)
}

fn desugar_heap_branches_inner(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let mut cur: Option<IrExpr> = None;
    loop {
        let src = cur.as_ref().unwrap_or(body);
        if let Some(r) = desugar_if_arm_unwrap(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_flatten_let_block(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_inline_tail_accumulator(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_callarg_heap_if(src, next_var) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_let_bound_heap_branch(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_let_unwrap(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_nested_branch_arms(src, next_var) {
            cur = Some(r);
            continue;
        }
        return cur;
    }
}

/// Recurse the heap-branch desugar INTO an `if`/`match` arm and a block TAIL. After a let-bound
/// duplication the body becomes `Block{prefix; if c then {<nested branch>} else {…}}`, whose arm
/// blocks may still hide a call-arg `if` (`(value.str(if…), end)`) or another let-bound branch (the
/// block_scalar two-`if` shape). Normalizing those HERE — inside the SHARED `desugar_heap_branches`
/// both `lower_body_into` and the `count_ir_calls` caps gate call — keeps the duplicated calls 1:1
/// (mir == ir); doing it lowering-side only (in `lower_heap_result_arm`) would double-count.
fn desugar_nested_branch_arms(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    match &body.kind {
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_heap_branches_inner(then, next_var);
            let ne = desugar_heap_branches_inner(else_, next_var);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_heap_branches_inner(&a.body, next_var) {
                    Some(nb) => {
                        changed = true;
                        almide_ir::IrMatchArm {
                            pattern: a.pattern.clone(),
                            guard: a.guard.clone(),
                            body: nb,
                        }
                    }
                    None => a.clone(),
                })
                .collect();
            if !changed {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let nt = desugar_heap_branches_inner(tail, next_var)?;
            Some(IrExpr {
                kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        _ => None,
    }
}

pub fn desugar_let_bound_heap_branch(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find the first heap let-bound `if`/`match` bind.
    let (i, bind_var, bind_ty, branch) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. }
            if is_heap_ty(ty)
                && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) =>
        {
            Some((i, *var, ty.clone(), value))
        }
        _ => None,
    })?;
    // BOUNDED-DUPLICATION gate: refuse when the continuation itself carries another unresolved
    // heap let-bound `if`/`match`.
    // BOUNDED-DUPLICATION: the continuation is copied into BOTH arms, so each remaining heap let-bound
    // `if`/`match` in `rest` doubles the leaf-arm count as the fixpoint resolves them one at a time. A
    // FEW are fine (block_scalar = 2: `let joined = if…; let tmp = if…(value.str arg, ANF-lifted)`), so
    // allow up to 2 (≤ 2^3 = 8 leaves) and refuse beyond that to keep the duplication bounded.
    let rest_branch_binds = stmts[i + 1..]
        .iter()
        .filter(|s| {
            matches!(
                &s.kind,
                IrStmtKind::Bind { ty, value, .. }
                    if is_heap_ty(ty)
                        && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
            )
        })
        .count();
    if rest_branch_binds > 2 {
        return None;
    }
    let result_ty = &body.ty;
    let rest_stmts: Vec<IrStmt> = stmts[i + 1..].to_vec();
    let rest_tail: Option<Box<IrExpr>> = tail.clone();
    // Reduce a `match` to a nested literal-pattern `if` chain (the same `desugar_match_to_if`
    // the tail/scalar machinery uses) — a pure builder, so a throwaway default ctx suffices.
    let if_branch = match &branch.kind {
        IrExprKind::If { .. } => (*branch).clone(),
        IrExprKind::Match { subject, arms } => {
            LowerCtx::default().desugar_match_to_if(subject, arms, &branch.ty)?
        }
        _ => return None,
    };
    let rewritten_branch = LowerCtx::wrap_branch_arms(
        &if_branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
    );
    // The prefix statements `stmts[0..i]` stay; the rewritten branch is the new block TAIL.
    let prefix: Vec<IrStmt> = stmts[..i].to_vec();
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: prefix, expr: Some(Box::new(rewritten_branch)) },
        ty: result_ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `{ …; let v = e!; rest }` — an unwrap-`!` bound to a let (an EFFECT-fn early-return on Err) →
/// `{ …; match e { ok(v) => { rest }, err($x) => err($x) } }`. The `!` IS exactly this: evaluate `e`,
/// bind the Ok payload to `v` and continue, else return the Err from the enclosing fn. Pushing the
/// continuation into the ok-arm makes the `match` the block TAIL, so the err-arm `err($x)` IS the
/// function's return (byte-identical to v0's `?`-style propagation). A SCALAR Ok payload then lowers
/// via the proven scalar-payload value-match; a HEAP Ok payload stays the Camp-4 frontier. Eliminates
/// the unwrap-bound-to-let wall — the top cross-repo wall reason (toml, base64 decode_chunks, porta).
pub fn desugar_let_unwrap(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, bind_var, bind_ty, inner) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
            IrExprKind::Unwrap { expr } => Some((i, *var, ty.clone(), (**expr).clone())),
            _ => None,
        },
        _ => None,
    })?;
    // The unwrapped expr must be a `Result[T, E]` — `!` early-returns its `Err(E)`.
    let err_ty = match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[1].clone(),
        _ => return None,
    };
    let result_ty = body.ty.clone();
    let fresh = VarId(max_var_id(body) + 1);
    let mk = |kind: IrExprKind, ty: Ty| IrExpr {
        kind,
        ty,
        span: body.span.clone(),
        def_id: body.def_id,
    };
    // ok(v) => { <rest> }
    let cont = mk(
        IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
        result_ty.clone(),
    );
    let ok_arm = almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Ok {
            inner: Box::new(almide_ir::IrPattern::Bind { var: bind_var, ty: bind_ty }),
        },
        guard: None,
        body: cont,
    };
    // err($x) => err($x)  (the propagated error IS the function result)
    let err_var = mk(IrExprKind::Var { id: fresh }, err_ty.clone());
    let err_body = mk(IrExprKind::ResultErr { expr: Box::new(err_var) }, result_ty.clone());
    let err_arm = almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Err {
            inner: Box::new(almide_ir::IrPattern::Bind { var: fresh, ty: err_ty }),
        },
        guard: None,
        body: err_body,
    };
    let match_expr = mk(
        IrExprKind::Match { subject: Box::new(inner), arms: vec![ok_arm, err_arm] },
        result_ty.clone(),
    );
    Some(mk(
        IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(match_expr)) },
        result_ty,
    ))
}

/// `let v = { s…; tail }` — a let bound to a BLOCK — is FLATTENED to `s…; let v = tail`, hoisting the
/// inner statements into the enclosing block. REQUIRED so the inline-accumulator desugar sees the real
/// accumulator value: the let-bound-`if` pre-desugar turns `let new_acc = if c then { let b1=…; acc+
/// [..,b1] } else …` into a `let new_acc = { let b1=…; acc+[..,b1] }` arm, whose value is a Block (not a
/// ConcatList), so the inline skips it. Flattening yields `let b1=…; let new_acc = acc+[..,b1]` — now
/// the inline + the TCO admit it. VarIds are unique (no shadow), so hoisting the inner lets is sound.
/// base64 decode_chunks's nested byte-extraction; toml accumulators.
pub fn desugar_flatten_let_block(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, var, ty, mutability, inner_stmts, inner_tail) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, mutability } => match &value.kind {
                IrExprKind::Block { stmts: inner, expr: Some(it) } => {
                    Some((i, *var, ty.clone(), *mutability, inner.clone(), (**it).clone()))
                }
                _ => None,
            },
            _ => None,
        })?;
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.extend(inner_stmts);
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind { var, ty, value: inner_tail, mutability },
        span: None,
    });
    new_stmts.extend_from_slice(&stmts[i + 1..]);
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// Count the occurrences of `var` (as an `IrExprKind::Var`) inside `e` — a local use-count for the
/// inline desugar below (the global var_table.use_count is post-lowering, unavailable here).
fn count_var_uses(e: &IrExpr, var: VarId) -> usize {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C {
        var: VarId,
        n: usize,
    }
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    self.n += 1;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C { var, n: 0 };
    c.visit_expr(e);
    c.n
}

/// `{ …; let v = <heap expr>; recurse(.., v, ..) }` — a let-bound heap accumulator passed to the block
/// TAIL — is INLINED: substitute `v` with its value in the tail and drop the let, yielding
/// `recurse(.., <heap expr>, ..)`. REQUIRED for the TCO over a tail-duplicated accumulator: the
/// let-bound-if pre-desugar turns `let new_acc = if c then acc+[..] else acc+[..]; recurse(.., new_acc)`
/// into branched `{ let new_acc = acc+[..]; recurse(.., new_acc) }` arms, but the recursion then passes
/// `new_acc` (a Var) — which `is_self_append` (`Var(acc) + …`) does NOT recognize, so the TCO declines.
/// Inlining restores the DIRECT `recurse(.., acc+[..])` the TCO admits. GATED: the let is the LAST stmt
/// (no intervening reassignment of the value's vars), its value is heap, and `v` is used EXACTLY ONCE
/// in the tail (so no allocation is duplicated). Base64 decode_chunks / toml accumulators.
pub fn desugar_inline_tail_accumulator(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else {
        return None;
    };
    let i = stmts.len().checked_sub(1)?;
    let IrStmtKind::Bind { var, value, mutability: almide_ir::Mutability::Let, .. } = &stmts[i].kind
    else {
        return None;
    };
    if !is_heap_ty(&value.ty) {
        return None;
    }
    // Only a self-append-shaped value (`acc + …`) — the accumulator case the TCO needs; avoids
    // inlining an arbitrary call (which could move a side effect into the tail).
    if !matches!(
        &value.kind,
        IrExprKind::BinOp {
            op: almide_ir::BinOp::ConcatList | almide_ir::BinOp::ConcatStr,
            ..
        }
    ) {
        return None;
    }
    if count_var_uses(tail, *var) != 1 {
        return None;
    }
    let new_tail = almide_ir::substitute::substitute_var_in_expr(tail, *var, value);
    Some(IrExpr {
        kind: IrExprKind::Block {
            stmts: stmts[..i].to_vec(),
            expr: Some(Box::new(new_tail)),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `let v = if c then e! else d` — an unwrap-`!` INSIDE an `if` arm (base64 decode_chunks:
/// `let v1 = if remaining > 1 then char_to_val(..)! else 0`). LIFT the `!` out of the arm: wrap each
/// NON-unwrap arm in `ok(..)` and strip `!` from each unwrap arm, so the `if` becomes a `Result[T,E]`,
/// bind it to `$r`, then `let v = $r!` — a plain let-unwrap the [`desugar_let_unwrap`] pass then
/// handles (and the let-bound heap-result `if` `$r` the pre-TCO pass tail-duplicates). Composes the
/// two existing desugars to reach the unwrap-in-if-arm shape. v0's `!` early-returns identically.
pub fn desugar_if_arm_unwrap(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, bind_var, bind_ty, cond, then_e, else_e) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
                IrExprKind::If { cond, then, else_ }
                    if matches!(&then.kind, IrExprKind::Unwrap { .. })
                        || matches!(&else_.kind, IrExprKind::Unwrap { .. }) =>
                {
                    Some((i, *var, ty.clone(), (**cond).clone(), (**then).clone(), (**else_).clone()))
                }
                _ => None,
            },
            _ => None,
        })?;
    // The `Result[T, E]` type the arms unify to — take it from an unwrap arm's operand.
    let res_ty = match (&then_e.kind, &else_e.kind) {
        (IrExprKind::Unwrap { expr }, _) | (_, IrExprKind::Unwrap { expr }) => expr.ty.clone(),
        _ => return None,
    };
    let mk = |kind: IrExprKind, ty: Ty| IrExpr {
        kind,
        ty,
        span: body.span.clone(),
        def_id: body.def_id,
    };
    // strip `!` from an unwrap arm (already Result[T,E]); wrap a plain arm in `ok(..)`.
    let conv = |arm: IrExpr| -> IrExpr {
        match arm.kind {
            IrExprKind::Unwrap { expr } => *expr,
            _ => mk(IrExprKind::ResultOk { expr: Box::new(arm) }, res_ty.clone()),
        }
    };
    let new_if = mk(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(conv(then_e)),
            else_: Box::new(conv(else_e)),
        },
        res_ty.clone(),
    );
    let r_var = VarId(max_var_id(body) + 1);
    let r_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: r_var,
            ty: res_ty.clone(),
            value: new_if,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let v_value = mk(
        IrExprKind::Unwrap { expr: Box::new(mk(IrExprKind::Var { id: r_var }, res_ty)) },
        bind_ty.clone(),
    );
    let v_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: bind_var,
            ty: bind_ty,
            value: v_value,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.push(r_bind);
    new_stmts.push(v_bind);
    new_stmts.extend_from_slice(&stmts[i + 1..]);
    Some(mk(IrExprKind::Block { stmts: new_stmts, expr: tail.clone() }, body.ty.clone()))
}

/// The kind of a call's resolved target — used to make a walled `Call`'s reason
/// precise (the histogram then names which call SHAPE to admit next: a free
/// `Named` call vs a stdlib `Module` dispatch vs an unresolved `Method` vs a
/// `Computed` callee), so the coverage roadmap is evidence-based, not guessed.
pub(crate) fn call_target_kind(t: &CallTarget) -> &'static str {
    match t {
        CallTarget::Named { .. } => "Named",
        CallTarget::Module { .. } => "Module",
        CallTarget::Method { .. } => "Method",
        CallTarget::Computed { .. } => "Computed",
    }
}

pub(crate) fn kind_name(k: &IrExprKind) -> &'static str {
    // Named precisely so the corpus-wall `<other>` buckets break down into the
    // exact expression forms still to admit (an evidence-based roadmap, the same
    // discipline as `call_target_kind`). Unnamed kinds remain `<other>`.
    match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::MapAccess { .. } => "MapAccess",
        IrExprKind::Range { .. } => "Range",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::EmptyMap => "EmptyMap",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        IrExprKind::FnRef { .. } => "FnRef",
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::OptionNone => "OptionNone",
        IrExprKind::Try { .. } => "Try",
        IrExprKind::Unwrap { .. } => "Unwrap",
        IrExprKind::UnwrapOr { .. } => "UnwrapOr",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::Break => "Break",
        IrExprKind::Continue => "Continue",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::IterChain { .. } => "IterChain",
        IrExprKind::Await { .. } => "Await",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Deref { .. } => "Deref",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::ToVec { .. } => "ToVec",
        IrExprKind::BoxNew { .. } => "BoxNew",
        IrExprKind::SpreadRecord { .. } => "SpreadRecord",
        _ => "<other>",
    }
}
