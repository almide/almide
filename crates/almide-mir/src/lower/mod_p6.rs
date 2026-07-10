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
    // A heap CONCAT operand may wrap a call-arg branch (`"len=" + (if c then a else b)` — a returned
    // String/List concat whose operand is a heap branch). Recurse into each operand so the inner
    // branch is ANF-lifted (`let t = if …; lhs + t`), which `desugar_let_bound_heap_branch` then
    // tail-duplicates into a heap-result `if` with concat arms — both already lower
    // (`try_lower_heap_result_if` + `try_lower_concat_str`/`try_lower_concat_list`). `count_ir_calls`
    // counts each Concat node as one call on the SAME desugared tree, so mir==ir is preserved.
    if let IrExprKind::BinOp {
        op: bop @ (almide_ir::BinOp::ConcatStr | almide_ir::BinOp::ConcatList),
        left,
        right,
    } = &e.kind
    {
        let mk = |nl: Box<IrExpr>, nr: Box<IrExpr>| IrExpr {
            kind: IrExprKind::BinOp { op: *bop, left: nl, right: nr },
            ty: e.ty.clone(),
            span: e.span.clone(),
            def_id: e.def_id,
        };
        if let Some((branch, nl)) = extract_first_callarg_branch(left, tmp) {
            return Some((branch, mk(Box::new(nl), right.clone())));
        }
        if let Some((branch, nr)) = extract_first_callarg_branch(right, tmp) {
            return Some((branch, mk(left.clone(), Box::new(nr))));
        }
        let var_of = |b: &IrExpr| IrExpr {
            kind: IrExprKind::Var { id: tmp },
            ty: b.ty.clone(),
            span: b.span.clone(),
            def_id: None,
        };
        if is_heap_branch(left) {
            return Some((left.as_ref().clone(), mk(Box::new(var_of(left)), right.clone())));
        }
        if is_heap_branch(right) {
            return Some((right.as_ref().clone(), mk(left.clone(), Box::new(var_of(right)))));
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

/// Find the FIRST unwrap-`!` ([`IrExprKind::Unwrap`]) NESTED as a CHILD of a container — a `Call`
/// argument, a `BinOp` operand, a `Tuple` element, or an `ok`/`err`/`Some` ctor argument — and
/// return (the `e!` to hoist, the container with that child replaced by `Var(tmp)`). NOT `e` itself
/// (a top-level `e!` is [`desugar_let_unwrap`]'s job). The hoist + that pass turn `f(.., g(x)!, ..)`
/// / `ok(int.parse(s)!)` into the proven match-based early-return.
fn extract_first_callarg_unwrap(e: &IrExpr, tmp: VarId) -> Option<(IrExpr, IrExpr)> {
    fn take_or_recurse(child: &IrExpr, tmp: VarId) -> Option<(IrExpr, IrExpr)> {
        if matches!(&child.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. }) {
            let var = IrExpr {
                kind: IrExprKind::Var { id: tmp },
                ty: child.ty.clone(),
                span: child.span.clone(),
                def_id: None,
            };
            return Some((child.clone(), var));
        }
        extract_first_callarg_unwrap(child, tmp)
    }
    let mk = |kind: IrExprKind| IrExpr { kind, ty: e.ty.clone(), span: e.span.clone(), def_id: e.def_id };
    match &e.kind {
        IrExprKind::Call { target, args, type_args } => {
            for (idx, a) in args.iter().enumerate() {
                if let Some((u, na)) = take_or_recurse(a, tmp) {
                    let mut v = args.clone();
                    v[idx] = na;
                    return Some((u, mk(IrExprKind::Call { target: target.clone(), args: v, type_args: type_args.clone() })));
                }
            }
            None
        }
        IrExprKind::BinOp { op, left, right } => {
            if let Some((u, nl)) = take_or_recurse(left, tmp) {
                return Some((u, mk(IrExprKind::BinOp { op: *op, left: Box::new(nl), right: right.clone() })));
            }
            if let Some((u, nr)) = take_or_recurse(right, tmp) {
                return Some((u, mk(IrExprKind::BinOp { op: *op, left: left.clone(), right: Box::new(nr) })));
            }
            None
        }
        IrExprKind::Tuple { elements } => {
            for (idx, el) in elements.iter().enumerate() {
                if let Some((u, ne)) = take_or_recurse(el, tmp) {
                    let mut v = elements.clone();
                    v[idx] = ne;
                    return Some((u, mk(IrExprKind::Tuple { elements: v })));
                }
            }
            None
        }
        IrExprKind::ResultOk { expr } => take_or_recurse(expr, tmp).map(|(u, ne)| (u, mk(IrExprKind::ResultOk { expr: Box::new(ne) }))),
        IrExprKind::ResultErr { expr } => take_or_recurse(expr, tmp).map(|(u, ne)| (u, mk(IrExprKind::ResultErr { expr: Box::new(ne) }))),
        IrExprKind::OptionSome { expr } => take_or_recurse(expr, tmp).map(|(u, ne)| (u, mk(IrExprKind::OptionSome { expr: Box::new(ne) }))),
        IrExprKind::Unwrap { expr } => extract_first_callarg_unwrap(expr, tmp)
            .map(|(u, ne)| (u, mk(IrExprKind::Unwrap { expr: Box::new(ne) }))),
        IrExprKind::Try { expr } => extract_first_callarg_unwrap(expr, tmp)
            .map(|(u, ne)| (u, mk(IrExprKind::Try { expr: Box::new(ne) }))),
        _ => None,
    }
}

/// ANF-LIFT an unwrap-`!` out of a CALL-ARGUMENT / operand / ctor-argument into a fresh `let tmp = e!`
/// so the existing [`desugar_let_unwrap`] then makes it lower (the `?`-early-return). The structural
/// twin of [`desugar_callarg_heap_if`] for `Unwrap` instead of a heap branch. Rewrites the FIRST
/// `f(.., g(x)!, ..)` (incl. nested + the block TAIL) to `let tmp = g(x)!; f(.., tmp, ..)`. `None` if
/// none. In BOTH the lowering and the `count_ir_calls` gate via [`desugar_heap_branches`].
pub fn desugar_callarg_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        // A BARE expression body (`effect fn g(..) = ok(h(t,p)! + 1)`) — lift the unwrap into a fresh
        // block `{ let tmp = h(t,p)!; <body'> }` (the same shape desugar_callarg_heap_if's bare case uses).
        let tmp = VarId(*next_var);
        let (unwrap, new_body) = extract_first_callarg_unwrap(body, tmp)?;
        *next_var += 1;
        let lift = IrStmt {
            kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
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
    for (i, s) in stmts.iter().enumerate() {
        let value = match &s.kind {
            IrStmtKind::Expr { expr } => Some(expr),
            IrStmtKind::Bind { value, .. } => Some(value),
            IrStmtKind::Assign { value, .. } => Some(value),
            _ => None,
        };
        let Some(v) = value else { continue };
        let Some((unwrap, new_v)) = extract_first_callarg_unwrap(v, tmp) else { continue };
        *next_var += 1;
        let lift = IrStmt {
            kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
            span: s.span.clone(),
        };
        let new_stmt = IrStmt {
            kind: match &s.kind {
                IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: new_v },
                IrStmtKind::Bind { var, mutability, ty, .. } => IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: new_v },
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
    if let Some(t) = tail.as_deref() {
        if let Some((unwrap, new_t)) = extract_first_callarg_unwrap(t, tmp) {
            *next_var += 1;
            let lift = IrStmt {
                kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
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

/// Desugar `match opt { some("lit1") => A1, …, none/_ => D }` — an `Option[String]`
/// subject whose Some patterns carry LITERAL payloads (the almide-grammar CLI
/// dispatch `match list.get(args, 1) { some("tree-sitter") => …, _ => usage }`) —
/// into the EXECUTABLE 2-arm form the variant match already lowers:
///   `match opt { some($p) => { if $p == "lit1" then A1 else … else D }, none => D }`.
/// String equality is a `BinOp` (not a call) and the duplicated default sits in a
/// BRANCH (only one side runs), and the count gate counts the SAME desugared tree
/// (desugar-before-both) — so `mir == ir` stays exact. Unit-typed matches only (the
/// grammar dispatch shape); a value match keeps its existing walls.
pub fn desugar_option_str_literal_match(body: &mut IrExpr) {
    use almide_ir::{walk_expr_mut, IrMatchArm, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            if !matches!(expr.ty, Ty::Unit) {
                return;
            }
            let IrExprKind::Match { subject, arms } = &expr.kind else { return };
            let is_opt_str = matches!(&subject.ty,
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(a[0], Ty::String));
            if !is_opt_str || arms.len() < 2 {
                return;
            }
            let (default, lits) = match arms.split_last() {
                Some((last, rest))
                    if matches!(last.pattern, IrPattern::Wildcard | IrPattern::None)
                        && last.guard.is_none() =>
                {
                    (last, rest)
                }
                _ => return,
            };
            let mut cases: Vec<(String, IrExpr)> = Vec::new();
            for a in lits {
                if a.guard.is_some() {
                    return;
                }
                let IrPattern::Some { inner } = &a.pattern else { return };
                let IrPattern::Literal { expr: lit_e } = &**inner else { return };
                let IrExprKind::LitStr { value } = &lit_e.kind else { return };
                cases.push((value.clone(), a.body.clone()));
            }
            let p = VarId(self.next_var);
            self.next_var += 1;
            let pvar = |ty: Ty| IrExpr {
                kind: IrExprKind::Var { id: p },
                ty,
                span: None,
                def_id: None,
            };
            // Build the innermost-first if-chain: … else D.
            let mut chain = default.body.clone();
            for (lit, arm_body) in cases.into_iter().rev() {
                let cond = IrExpr {
                    kind: IrExprKind::BinOp {
                        op: almide_ir::BinOp::Eq,
                        left: Box::new(pvar(Ty::String)),
                        right: Box::new(IrExpr {
                            kind: IrExprKind::LitStr { value: lit },
                            ty: Ty::String,
                            span: None,
                            def_id: None,
                        }),
                    },
                    ty: Ty::Bool,
                    span: None,
                    def_id: None,
                };
                chain = IrExpr {
                    kind: IrExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(arm_body),
                        else_: Box::new(chain),
                    },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                };
            }
            let new_arms = vec![
                IrMatchArm {
                    pattern: IrPattern::Some {
                        inner: Box::new(IrPattern::Bind { var: p, ty: Ty::String }),
                    },
                    guard: None,
                    body: chain,
                },
                IrMatchArm { pattern: IrPattern::None, guard: None, body: default.body.clone() },
            ];
            let subject = subject.clone();
            *expr = IrExpr {
                kind: IrExprKind::Match { subject, arms: new_arms },
                ty: Ty::Unit,
                span: expr.span.clone(),
                def_id: expr.def_id,
            };
        }
    }
    let mut s = S { next_var: max_var_id(body) + 1 };
    s.visit_expr_mut(body);
}

/// Desugar a STATEMENT/let-bind effect-`!` (`Unwrap`) into a NESTED-MATCH continuation — the standard
/// monadic do-desugar — so a CAN-ERR effect-`!` propagates without a mid-function early-return (which the
/// v1 MIR has no Op for):
///
///   { before; let x = f()!; after }  →  { before; match f() { err(e) => err(e), ok(x) => { after } } }
///   { before;     f()!    ; after }  →  { before; match f() { err(e) => err(e), ok(_) => { after } } }
///
/// The continuation (`after`) nests in the Ok-arm; the Err-arm reconstructs `err(e)` at the enclosing
/// fn's `Result[_, String]` type. The fn's tail becomes the (nested) match, which the EXISTING
/// heap-result-`match` tail lowering already handles — VERIFIED to byte-match for scalar/String/Value/
/// record Ok (porta.start's every shape). Call-count-INVARIANT (`f()` appears once before and after; the
/// continuation nests in ONE arm — no duplication; `err(e)` is a constructor, not a call), so `mir == ir`
/// holds without the count gate re-running it. Only a TOP-LEVEL stmt `!` (a `let x = f()!` Bind value or
/// a bare `f()!` Expr stmt); a tail `f()!` is the fn's return (tail.rs pass-through), and a `!` nested in
/// an operand is handled by `desugar_callarg_unwrap`. Closes the porta.start / dojo effect-monad wall
/// WITHOUT the major Return-op subsystem.
pub fn desugar_effect_unwrap(body: &IrExpr, unit_main: bool) -> Option<IrExpr> {
    let mut next_var = max_var_id(body) + 1;
    desugar_effect_unwrap_inner(body, &mut next_var, unit_main)
}

/// GUARD-ELSE → conditional (Phase A of the v1→v0 parity plan). `guard cond else E; rest`
/// is a CONDITIONAL EARLY EXIT: when `!cond`, `E` becomes the result (a function early
/// return, or a loop `continue`/`break`). v1 has no early-return op, so DEFERRING it
/// (always-continue) silently miscompiled the `!cond` path (`guard len>0 else err();
/// ok(x)` returned ok for the empty input). This PURE IR rewrite restructures the block
/// so the SAME control flow runs through the proven `if` machinery:
///
///   `{ pre…; guard cond else E; rest…; tail }`
///     → `{ pre…; if cond then { rest…; tail } else E' }`
///
/// where `E'` is `E` verbatim EXCEPT a `continue` becomes `()`: `guard cond else continue;
/// rest` means "skip the rest of THIS iteration when `!cond`", which is exactly `if cond
/// then { rest } else ()` — eliminating the `continue` node so the scalar-loop path (which
/// declines a body with `continue`/`break`) accepts it. A `break` stays verbatim (it needs
/// real loop-exit — the residual guard-break shapes wall until break support lands). A
/// function-body guard's `E` (an `err(…)`/value) makes the `if` the block TAIL → the proven
/// heap/scalar-result-`if` handles the early-return value.
///
/// CALL-COUNT-INVARIANT: `cond`, `E`, and each `rest` statement appear EXACTLY ONCE before
/// and after (no duplication — `rest` nests in ONE arm), so `mir == ir` holds and the caps
/// gate stays exact without re-running. Recurses into every block (function body, loop
/// bodies, `if`/`match` arms) and handles CHAINED guards (a second guard in `rest` is
/// rewritten by the recursive call on the constructed then-block).
pub fn desugar_guard(body: &IrExpr) -> Option<IrExpr> {
    let mut changed = false;
    let rewritten = desugar_guard_rec(body.clone(), &mut changed);
    changed.then_some(rewritten)
}

/// Apply the FULL pre-lowering desugar fixpoint — the EXACT sequence (and order)
/// `lower_body_into` runs before it lowers a body: guard → beta-reduce → tuple-unwrap-or →
/// effect-unwrap → heap-branches, restarting from the top after each rewrite. The MIR the
/// lowering emits reflects this fully-desugared tree, so any consumer that must count calls /
/// interps 1:1 against the MIR (the caps `mir == ir` gate, the interp-coverage count in
/// classify_corpus) MUST read the SAME tree — otherwise a tail-duplicating rewrite (a
/// let-bound `match` pushing its continuation into each arm) duplicates a call in the MIR that
/// the under-desugared count never sees (the `option.unwrap_or((tuple)); f(r.0)` mir>ir breach).
/// This is the single "desugar-before-both" source of truth; callers use it instead of
/// hand-picking a subset of the desugars.
/// COMPACT IR pretty-printer for desugar debugging (env-gated via `DBG_DESUGAR_FN`). Shows the
/// tree structure — `Block`, `Match`/`If` with per-arm patterns, `Call` targets, `Try`/`Unwrap`,
/// ctors, `Var`/`Lit` — concise enough to `diff` two desugared bodies (e.g. a derived-Codec
/// `decode` vs the proven separate-bind form) and pinpoint where they diverge. NOT used at
/// runtime; a pure diagnostic reachable through `dump_desugared_ir`.
pub fn dump_ir(e: &IrExpr) -> String {
    fn go(e: &IrExpr, ind: usize, out: &mut String) {
        use almide_ir::{IrExprKind as K, IrStmtKind as S};
        let pad = "  ".repeat(ind);
        match &e.kind {
            K::Block { stmts, expr } => {
                out.push_str(&format!("{pad}Block\n"));
                for s in stmts {
                    match &s.kind {
                        S::Bind { var, value, .. } => {
                            out.push_str(&format!("{pad}  let v{}=\n", var.0));
                            go(value, ind + 2, out);
                        }
                        S::Expr { expr } => {
                            out.push_str(&format!("{pad}  expr\n"));
                            go(expr, ind + 2, out);
                        }
                        other => out.push_str(&format!("{pad}  stmt({other:?})\n")),
                    }
                }
                if let Some(t) = expr {
                    out.push_str(&format!("{pad}  tail\n"));
                    go(t, ind + 2, out);
                }
            }
            K::Match { subject, arms } => {
                out.push_str(&format!("{pad}Match\n{pad}  subj\n"));
                go(subject, ind + 2, out);
                for a in arms {
                    out.push_str(&format!("{pad}  arm {:?} =>\n", a.pattern));
                    go(&a.body, ind + 2, out);
                }
            }
            K::If { cond, then, else_ } => {
                out.push_str(&format!("{pad}If\n{pad}  cond\n"));
                go(cond, ind + 2, out);
                out.push_str(&format!("{pad}  then\n"));
                go(then, ind + 2, out);
                out.push_str(&format!("{pad}  else\n"));
                go(else_, ind + 2, out);
            }
            K::Call { target, args, .. } => {
                out.push_str(&format!("{pad}Call({})\n", crate::lower::call_target_kind(target)));
                for a in args {
                    go(a, ind + 1, out);
                }
            }
            K::Try { expr } => {
                out.push_str(&format!("{pad}Try\n"));
                go(expr, ind + 1, out);
            }
            K::Unwrap { expr } => {
                out.push_str(&format!("{pad}Unwrap\n"));
                go(expr, ind + 1, out);
            }
            K::ResultOk { expr } => {
                out.push_str(&format!("{pad}Ok\n"));
                go(expr, ind + 1, out);
            }
            K::ResultErr { expr } => {
                out.push_str(&format!("{pad}Err\n"));
                go(expr, ind + 1, out);
            }
            K::Record { name, fields } => {
                out.push_str(&format!("{pad}Record({:?}, {} fields)\n", name, fields.len()));
                for f in fields {
                    go(&f.1, ind + 1, out);
                }
            }
            K::Var { id } => out.push_str(&format!("{pad}v{}\n", id.0)),
            K::UnwrapOr { expr, fallback } => {
                out.push_str(&format!("{pad}UnwrapOr\n{pad}  val\n"));
                go(expr, ind + 2, out);
                out.push_str(&format!("{pad}  else\n"));
                go(fallback, ind + 2, out);
            }
            K::IndexAccess { object, index } => {
                out.push_str(&format!("{pad}Index\n"));
                go(object, ind + 1, out);
                go(index, ind + 1, out);
            }
            K::Member { object, field } => {
                out.push_str(&format!("{pad}Member(.{})\n", field.as_str()));
                go(object, ind + 1, out);
            }
            other => out.push_str(&format!("{pad}{}\n", crate::lower::kind_name(other))),
        }
    }
    let mut s = String::new();
    go(e, 0, &mut s);
    s
}

/// Env-gated desugar dump: when `DBG_DESUGAR_FN == fn_name`, print the fully-desugared body so the
/// derived-Codec `decode` chain can be diffed against the proven separate-bind form. No-op otherwise.
pub fn dump_desugared_ir(fn_name: &str, body: &IrExpr) {
    if std::env::var("DBG_DESUGAR_FN").is_ok_and(|v| v == fn_name) {
        if std::env::var("DBG_DESUGAR_RAW").is_ok() {
            eprintln!("=== RAW {fn_name} ===\n{:#?}", desugar_all(body, fn_name == "main"));
        } else {
            eprintln!("=== DESUGARED {fn_name} ===\n{}", dump_ir(&desugar_all(body, fn_name == "main")));
        }
    }
}

/// Resolve a UFCS / derived-method `Call`/`TailCall` whose target is `CallTarget::Method
/// { object, method }` to the concrete free function it names — the SAME resolution the v0
/// emitter does at emit time (`emit_wasm/calls_p2.rs`'s `Method` catch-all): a `Ty::Named(T)`
/// receiver → the derived/user fn `T.method` (`p.encode()` → `Person.encode(p)`), an
/// already-qualified `method` (contains '.') → that name verbatim. The receiver becomes the
/// FIRST argument, matching v0 (`emit_expr(object)` then the args). An unresolvable Method
/// (a non-`Named` receiver with a bare method — e.g. a stdlib UFCS the frontend left as Method)
/// is LEFT in place, so it still walls honestly rather than resolving to a bogus name. Run in
/// BOTH the real lowering (`lower_body_into`) and the `count_ir_calls` gate (`desugar_all`) so
/// `mir == ir` holds by construction. Returns `None` when no Method was rewritten.
pub fn desugar_method_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use almide_lang::types::Ty;

    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // Post-order: resolve the receiver / args (which may themselves be method calls)
            // BEFORE rewriting this node, so a chained `a.f().g()` resolves inside-out.
            walk_expr_mut(self, e);
            let (target, args) = match &mut e.kind {
                IrExprKind::Call { target, args, .. } => (target, args),
                IrExprKind::TailCall { target, args } => (target, args),
                _ => return,
            };
            let name = match &*target {
                CallTarget::Method { object, method } => {
                    if method.as_str().contains('.') {
                        Some(method.as_str().to_string())
                    } else if let Ty::Named(n, _) = &object.ty {
                        Some(format!("{}.{}", n.as_str(), method.as_str()))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(name) = name {
                if let CallTarget::Method { object, .. } = target {
                    let obj = (**object).clone();
                    let mut new_args = Vec::with_capacity(args.len() + 1);
                    new_args.push(obj);
                    new_args.append(args);
                    *args = new_args;
                }
                *target = CallTarget::Named {
                    name: almide_lang::intern::sym(&name),
                };
                self.changed = true;
            }
        }
    }

    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

pub fn desugar_all(body: &IrExpr, unit_main: bool) -> IrExpr {
    let mut cur = body.clone();
    loop {
        if let Some(r) = desugar_method_calls(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_guard(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_beta_reduce(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_tuple_unwrap_or(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_effect_unwrap(&cur, unit_main) {
            cur = r;
            continue;
        }
        if unit_main {
            if let Some(r) = desugar_unit_main_err_arms(&cur) {
                cur = r;
                continue;
            }
        }
        if let Some(r) = desugar_to_option_calls(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_heap_branches(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_scalar_tuple_literal_match(&cur) {
            cur = r;
            continue;
        }
        break;
    }
    cur
}

/// Rewrite the FIRST `guard` in a loop-body statement list into an `if` statement:
/// `[pre…, Guard{cond,else}, rest…]` → `[pre…, Expr(if cond then { rest… } else E')]`
/// (`continue` → `()`, `break`/value verbatim). The `if`'s then-block is recursively
/// desugared so a chained guard in `rest` is handled. No guard → the list is returned
/// unchanged (recursion into each statement's sub-exprs already happened via the caller's
/// `map_children`, so only the list-level restructuring remains here).
fn rewrite_guard_stmt_list(
    body: Vec<almide_ir::IrStmt>,
    changed: &mut bool,
) -> Vec<almide_ir::IrStmt> {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind};
    let Some(i) = body
        .iter()
        .position(|s| matches!(s.kind, IrStmtKind::Guard { .. }))
    else {
        return body;
    };
    *changed = true;
    let mut pre = body;
    let rest = pre.split_off(i + 1);
    let guard = pre.pop().expect("guard at index i");
    let (cond, else_, gspan) = match guard.kind {
        IrStmtKind::Guard { cond, else_ } => (cond, else_, guard.span),
        _ => unreachable!("position() found a Guard"),
    };
    // The then-body is `{ rest… }` (Unit); recurse so a further guard in it is rewritten.
    let then_block = desugar_guard_rec(
        IrExpr {
            kind: IrExprKind::Block { stmts: rest, expr: None },
            ty: Ty::Unit,
            span: gspan,
            def_id: None,
        },
        changed,
    );
    let else_branch = match &else_.kind {
        IrExprKind::Continue => IrExpr {
            kind: IrExprKind::Unit,
            ty: Ty::Unit,
            span: else_.span,
            def_id: else_.def_id,
        },
        _ => else_,
    };
    let if_expr = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_block),
            else_: Box::new(else_branch),
        },
        ty: Ty::Unit,
        span: gspan,
        def_id: None,
    };
    pre.push(IrStmt { kind: IrStmtKind::Expr { expr: if_expr }, span: gspan });
    pre
}

fn desugar_guard_rec(e: IrExpr, changed: &mut bool) -> IrExpr {
    use almide_ir::{IrExprKind, IrStmtKind};
    // Bottom-up: rewrite every child expression first (arm bodies, bind values — all
    // reachable through `map_children`), so a guard nested inside a sub-expression is
    // handled before this node's own restructuring.
    let e = e.map_children(&mut |c| desugar_guard_rec(c, changed));
    // A LOOP BODY is a `Vec<IrStmt>` (NOT a `Block` expr — `map_children` maps each
    // statement's sub-exprs but never restructures the list), so a `guard … else continue`
    // inside `for`/`while` needs its OWN statement-list rewrite: `[pre…, Guard, rest…]` →
    // `[pre…, Expr(if cond then { rest… } else E')]`. Same continue→() / break-verbatim rule.
    if let IrExprKind::ForIn { var, var_tuple, iterable, body } = e.kind {
        let new_body = rewrite_guard_stmt_list(body, changed);
        return IrExpr {
            kind: IrExprKind::ForIn { var, var_tuple, iterable, body: new_body },
            ty: e.ty,
            span: e.span,
            def_id: e.def_id,
        };
    }
    if let IrExprKind::While { cond, body } = e.kind {
        let new_body = rewrite_guard_stmt_list(body, changed);
        return IrExpr {
            kind: IrExprKind::While { cond, body: new_body },
            ty: e.ty,
            span: e.span,
            def_id: e.def_id,
        };
    }
    let IrExprKind::Block { stmts, expr } = &e.kind else {
        return e;
    };
    let Some(i) = stmts
        .iter()
        .position(|s| matches!(s.kind, IrStmtKind::Guard { .. }))
    else {
        return e;
    };
    *changed = true;
    let ty = e.ty.clone();
    let span = e.span.clone();
    let def_id = e.def_id;
    let mut pre = stmts.clone();
    let rest = pre.split_off(i + 1); // rest = stmts[i+1..]
    let guard = pre.pop().expect("guard at index i"); // remove the guard; pre = stmts[0..i]
    let (cond, else_) = match guard.kind {
        IrStmtKind::Guard { cond, else_ } => (cond, else_),
        _ => unreachable!("position() found a Guard"),
    };
    // The THEN branch (`cond` true) is the continuation `{ rest…; tail }`, same result
    // type as this block. Recurse so a FURTHER guard inside `rest` is rewritten too.
    let then_block = desugar_guard_rec(
        IrExpr {
            kind: IrExprKind::Block { stmts: rest, expr: expr.clone() },
            ty: ty.clone(),
            span,
            def_id,
        },
        changed,
    );
    // The ELSE branch (`cond` false) is `E`, except `continue` → `()` (see doc).
    let else_branch = match &else_.kind {
        IrExprKind::Continue => IrExpr {
            kind: IrExprKind::Unit,
            ty: Ty::Unit,
            span: else_.span,
            def_id: else_.def_id,
        },
        _ => else_,
    };
    let if_expr = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_block),
            else_: Box::new(else_branch),
        },
        ty: ty.clone(),
        span,
        def_id,
    };
    IrExpr {
        kind: IrExprKind::Block { stmts: pre, expr: Some(Box::new(if_expr)) },
        ty,
        span,
        def_id,
    }
}

fn desugar_effect_unwrap_inner(body: &IrExpr, next_var: &mut u32, unit_main: bool) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::Ty;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    for (i, s) in stmts.iter().enumerate() {
        // A TOP-LEVEL effect-`!` — the WHOLE stmt value is `Unwrap { f() }`:
        //   let-bind `let x = f()!` → the Ok-arm binds `x` to the payload
        //   bare stmt `f()!`        → the Ok-arm discards the (Unit/None) payload (`_`)
        // The desugar is repr-AGNOSTIC: it admits Option-`!` and Result-`!` uniformly — the same
        // `err(e) => err(e), ok(x) => cont` skeleton lowers through the shared variant-match path for
        // both (Option=none/some, Result=err/ok positionally, one len-as-tag repr). HOLE-1 (the
        // record-Ok recursive-drop leak) is gated NOT here but at the match-LOWERING mechanism
        // (`try_lower_variant_value_match` excludes a record-Ok subject from the str-result/
        // `heap_elem_lists` tracking via `is_record_result_ty`, so it rolls back → walls cleanly),
        // because identifying a record-Ok payload needs the record-layout registry the desugar lacks.
        // `Try` is the frontend's auto-`?` on an UN-annotated bind of a declared-Result
        // effect call (`let v = declared_result()`) — the same monadic coercion as a
        // spelled-out `!`, so both desugar identically. (A Try left unhandled emitted a
        // bare dst-less `(call $f)` whose Result handle stayed on the wasm stack —
        // effect_assign_unwrap's `unannotated=` leg, 2026-07-03.)
        let (inner, ok_pat) = match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => (
                    (**expr).clone(),
                    IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: *var, ty: ty.clone() }) },
                ),
                _ => continue,
            },
            IrStmtKind::Expr { expr } => match &expr.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                    ((**expr).clone(), IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) })
                }
                _ => continue,
            },
            _ => continue,
        };
        // An Option-`!` is admitted for a SCALAR Some payload only (the sized_conversion
        // family — no drop on either arm); a heap Some payload's bind/drop discipline is a
        // later extension: leave the raw `!` so it walls honestly.
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &inner.ty {
            if a.len() != 1 || is_heap_ty(&a[0]) {
                continue;
            }
        }
        // The continuation = the rest of the block `{ stmts[i+1..]; tail }`, typed as the whole body
        // (it produces the fn's return). RECURSE so a LATER `!` in the continuation also desugars.
        let cont = IrExpr {
            kind: IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        };
        let cont = desugar_effect_unwrap_inner(&cont, next_var, unit_main).unwrap_or(cont);
        let m = build_unwrap_match(inner, ok_pat, cont, body, next_var, unit_main);
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(m)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    // No TOP-LEVEL stmt-`!` in this block — RECURSE INTO THE TAIL. A tail `if`/`match`/block is a
    // RETURN position whose arm bodies may carry a stmt-`!` (porta_init's `else { … fs.write(p,c)!;
    // … }`, signal_instance's nested if-arms). `desugar_tail_effect_unwrap` navigates that control
    // flow and applies the SAME gated stmt-`!` rewrite inside each arm/tail block.
    if let Some(t) = tail.as_deref() {
        if let Some(nt) = desugar_tail_effect_unwrap(t, next_var, unit_main) {
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    None
}

/// Build the nested-match `match inner { err(e) => err(e), ok(<ok_pat>) => <cont> }` at the enclosing
/// fn's `Result[_, String]` return type (`body.ty`). The err-arm is a REAL move-out `err(e) => err(e)`
/// (NOT a strip): a fresh `e: String` bound by the Err-pattern, reconstructed as `ResultErr` at the
/// return type. The continuation nests ONLY in the ok-arm. Shared by the stmt-position and the
/// tail/return-position (`desugar_tail_effect_unwrap`) rewrites.
/// Build the UNIT-MAIN fail-arm body: v0's main wrapper prints `Error: <msg>` to STDERR and
/// exits 1 — main is declared `-> Unit` (the void convention: an `err(…)` arm value would be
/// silently DISCARDED), so the arm aborts through the prim floor instead:
/// `prim.die(prim.handle(<full line>))` ($__die = STDERR write + proc_exit(1), the
/// int_rotate/math_int precedent). `msg` is either the static line (Option-`!`: "Error:
/// none\n") or a bound payload Var (Result-`!`: a `let $m = "Error: " + e + "\n"` bind
/// precedes the die and the die consumes $m's handle — the arm's scope-end drop after the
/// never-returning die balances the cert, exactly the checked-overflow abort shape).
/// Build the whole unit-main die BLOCK for a DYNAMIC String payload `e`:
/// `{ let $m = "Error: " + e + "\n"; prim.die(prim.handle($m)) }` — the v0 main-err line.
fn build_main_die_line(payload: IrExpr, at: &IrExpr, next_var: &mut u32) -> IrExpr {
    use almide_ir::BinOp;
    use almide_lang::types::Ty;
    let m_var = VarId(*next_var);
    *next_var += 1;
    let lit = |v: &str| IrExpr {
        kind: IrExprKind::LitStr { value: v.into() },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    let concat = |a: IrExpr, b: IrExpr| IrExpr {
        kind: IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(a), right: Box::new(b) },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    let line = concat(concat(lit("Error: "), payload), lit("\n"));
    let m_ref = IrExpr {
        kind: IrExprKind::Var { id: m_var },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    // SPLIT form (`let $h = prim.handle($m); prim.die($h)`): a NESTED `prim.handle(<Var>)`
    // inside a match ARM declines at the arm's prim lowering (the top-level form is fine) —
    // the split is the shape the arm path proves.
    let h_var = VarId(*next_var);
    *next_var += 1;
    let handle_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_lang::intern::sym("prim"),
                func: almide_lang::intern::sym("handle"),
                def_id: None,
            },
            args: vec![m_ref],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span: at.span.clone(),
        def_id: None,
    };
    let h_ref =
        IrExpr { kind: IrExprKind::Var { id: h_var }, ty: Ty::Int, span: at.span.clone(), def_id: None };
    let die_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_lang::intern::sym("prim"),
                func: almide_lang::intern::sym("die"),
                def_id: None,
            },
            args: vec![h_ref],
            type_args: Vec::new(),
        },
        ty: Ty::Unit,
        span: at.span.clone(),
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: m_var,
                        ty: Ty::String,
                        value: line,
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: at.span.clone(),
                },
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: h_var,
                        ty: Ty::Int,
                        value: handle_call,
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: at.span.clone(),
                },
            ],
            expr: Some(Box::new(die_call)),
        },
        ty: Ty::Unit,
        span: at.span.clone(),
        def_id: at.def_id,
    }
}

/// UNIT-MAIN auto-? residue: the FRONTEND builds `match f() { ok(v) => …, err(e) => err(e) }`
/// directly (no `Unwrap` node reaches the MIR desugar), and in the VOID main the `err(e)` arm
/// value is DISCARDED — the failure path silently exited 0 (v0: `Error: <msg>` + exit 1).
/// Rewrite every bare-`ResultErr` arm body under a UNIT-typed match to the same die block the
/// `!` desugar builds. Sound: a user cannot type-check `err(e)` as a Unit match arm, so every
/// such arm IS the auto-? artifact (main's early error return). Gated to main by the caller.
pub fn desugar_unit_main_err_arms(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::Ty;
    struct Rw {
        next_var: u32,
        changed: bool,
    }
    impl IrMutVisitor for Rw {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { arms, .. } = &mut e.kind else { return };
            if !matches!(e.ty, Ty::Unit) {
                return;
            }
            for arm in arms.iter_mut() {
                if let IrExprKind::ResultErr { expr } = &arm.body.kind {
                    if matches!(expr.ty, Ty::String) {
                        let payload = (**expr).clone();
                        let at = arm.body.clone();
                        arm.body = build_main_die_line(payload, &at, &mut self.next_var);
                        self.changed = true;
                    }
                }
            }
        }
    }
    let mut rw = Rw { next_var: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    rw.visit_expr_mut(&mut out);
    rw.changed.then_some(out)
}

fn build_main_die(msg: IrExpr, body: &IrExpr) -> IrExpr {
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let handle = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("prim"), func: sym("handle"), def_id: None },
            args: vec![msg],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span: body.span.clone(),
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("prim"), func: sym("die"), def_id: None },
            args: vec![handle],
            type_args: Vec::new(),
        },
        ty: Ty::Unit,
        span: body.span.clone(),
        def_id: None,
    }
}

fn build_unwrap_match(
    inner: IrExpr,
    ok_pat: almide_ir::IrPattern,
    cont: IrExpr,
    body: &IrExpr,
    next_var: &mut u32,
    unit_main: bool,
) -> IrExpr {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    // An OPTION subject (`int.to_int8_checked(v)!` — the sized_conversion family): the fail
    // arm is `none => err("none")` (v0's unwrap-of-none message, oracle: `Error: none`,
    // exit 1), and the continuation binds under the SOME pattern — Option's len-as-tag
    // polarity is OPPOSITE Result's (Some = len 1, Err = len 1), so reusing the Ok/Err
    // skeleton would fire the fail arm on the SUCCESS value.
    if matches!(&inner.ty, Ty::Applied(TypeConstructorId::Option, _)) {
        let none_body = if unit_main {
            // main is void — abort with v0's whole line instead of a discarded err().
            build_main_die(
                IrExpr {
                    kind: IrExprKind::LitStr { value: "Error: none\n".into() },
                    ty: Ty::String,
                    span: body.span.clone(),
                    def_id: None,
                },
                body,
            )
        } else {
            IrExpr {
                kind: IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: "none".into() },
                        ty: Ty::String,
                        span: body.span.clone(),
                        def_id: None,
                    }),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            }
        };
        let none_arm = IrMatchArm { pattern: IrPattern::None, guard: None, body: none_body };
        let some_pat = match ok_pat {
            IrPattern::Ok { inner } => IrPattern::Some { inner },
            other => other,
        };
        let some_arm = IrMatchArm { pattern: some_pat, guard: None, body: cont };
        return IrExpr {
            kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![none_arm, some_arm] },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        };
    }
    let e_var = VarId(*next_var);
    *next_var += 1;
    let err_body = if unit_main {
        // main is void — build the full line (`let $m = "Error: " + e + "\n"`) then abort.
        let e_ref = IrExpr {
            kind: IrExprKind::Var { id: e_var },
            ty: Ty::String,
            span: body.span.clone(),
            def_id: None,
        };
        build_main_die_line(e_ref, body, next_var)
    } else {
        IrExpr {
            kind: IrExprKind::ResultErr {
                expr: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: e_var },
                    ty: Ty::String,
                    span: body.span.clone(),
                    def_id: None,
                }),
            },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        }
    };
    let err_arm = IrMatchArm {
        pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: e_var, ty: Ty::String }) },
        guard: None,
        body: err_body,
    };
    let ok_arm = IrMatchArm { pattern: ok_pat, guard: None, body: cont };
    IrExpr {
        kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![err_arm, ok_arm] },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    }
}

/// Recurse the effect-`!` desugar into RETURN/TAIL positions — an `if`/`match` arm body or a nested
/// block tail — so a stmt-`!` inside a branch (porta_init's `else { … fs.write(p, c)!; … }`,
/// signal_instance's nested if/else arm blocks) desugars to the same nested-match continuation. The
/// err-arm `err(e) => err(e)` propagates to the ENCLOSING fn's `Result[_, String]` return; the
/// continuation nests only in the ok-arm. Each arm/tail recurses INDEPENDENTLY (no duplication), so
/// `count_ir_calls` stays exact (`f()` once, continuation in one arm, `err(e)` is a ctor). HOLE-1
/// (admitted Ok-payload reprs only) is enforced inside the block path (`desugar_effect_unwrap_inner`).
/// Returns `None` if no `!` is reachable in a return position of `tail` (the body keeps its form).
fn desugar_tail_effect_unwrap(tail: &IrExpr, next_var: &mut u32, unit_main: bool) -> Option<IrExpr> {
    match &tail.kind {
        // A nested block — its own stmts/tail may carry a stmt-`!`.
        IrExprKind::Block { .. } => desugar_effect_unwrap_inner(tail, next_var, unit_main),
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_tail_effect_unwrap(then, next_var, unit_main);
            let ne = desugar_tail_effect_unwrap(else_, next_var, unit_main);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_tail_effect_unwrap(&a.body, next_var, unit_main) {
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
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        _ => None,
    }
}

/// HOLE-1 GATE: is the `!`-subject's `Result[Ok, Err]` type one whose Ok payload the nested-match
/// continuation can BIND + DROP soundly in this brick? Admit ONLY: `Err == String` (the err-arm
/// reconstructs `err(e: String)`) AND Ok ∈ { scalar / Unit (the `result_heap_err_bind` len-as-tag
/// path, Err-String drop only), String / Value (the `str_heap_bind` flat / value-result drops),
/// List[Value] + the four tuple-Ok shapes (their dedicated RECURSIVE result-drops) }. A heap RECORD /
/// Option-of-record / List[String] / List[Int] / nested Ok payload has NO real recursive drop here —
/// the match-lowering's type dispatch would fall through to the FLAT `heap_elem_lists`/`DropListStr`
/// cert (control_p2.rs:330-332), which LEAKS that payload's nested heap. So REFUSE it: the fn then
/// walls cleanly on the raw `!` (an honest `Unsupported` > a gate-invisible leak).
fn effect_unwrap_admitted(result_ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::Result, a) = result_ty else {
        return false;
    };
    if a.len() != 2 || !matches!(a[1], Ty::String) {
        return false;
    }
    let ok = &a[0];
    // scalar / Unit Ok — result_heap_err_bind (the Ok side frees nothing; only Err-String drops).
    if !is_heap_ty(ok) {
        return true;
    }
    // String / Value Ok — the existing str_heap_bind flat / DropResultValue drops.
    if matches!(ok, Ty::String) || is_value_ty(ok) {
        return true;
    }
    // List[<non-heap scalar>] (List[Int]/Float/Bool/…) or `Bytes` Ok — a FLAT block of INLINE
    // scalars (fs.read_bytes' `Result[List[Int], String]`, the `load` shape). The by-type dispatch
    // (control_p2.rs:334-376) routes it to the `else => heap_elem_lists` flat `DropListStr`, which
    // frees the block with NO nested-heap leak — the elements are inline scalars, identical
    // soundness to the existing String-Ok flat path. GATE HARD: a `List[String]/List[record]/
    // List[Value]/nested` element IS heap, so the flat drop would LEAK every element — those keep
    // walling (`is_heap_ty(&le[0])` excludes them; a `List[Value]` Ok takes the dedicated recursive
    // `is_result_listval_ty` branch below, never this flat one).
    if let Ty::Applied(TypeConstructorId::List, le) = ok {
        if le.len() == 1 && !is_heap_ty(&le[0]) {
            return true;
        }
    }
    if matches!(ok, Ty::Applied(TypeConstructorId::Bytes, _) | Ty::Bytes) {
        return true;
    }
    // List[Value] Ok + the (String,Int)/(Value,Int)/(List[String],Int)/(List[Value],Int) tuple-Ok
    // shapes — each has a dedicated RECURSIVE result-drop the match-lowering routes to soundly.
    is_result_listval_ty(result_ty)
        || is_str_int_result_ty(result_ty)
        || is_value_int_result_ty(result_ty)
        || is_list_str_int_result_ty(result_ty)
        || is_list_value_int_result_ty(result_ty)
}

/// Does `e` carry a STATEMENT/let-position effect-`!` (an `Unwrap` bound to a `let` or run as an
/// `Expr` stmt, or a tail/return-position `!`) anywhere a branch reaches, AND is EVERY such `!`'s Ok
/// payload HOLE-1-admitted (`effect_unwrap_admitted`)? Returns `(has_unwrap, all_admitted)`. Used by
/// the statement-control continuation-lift to fire ONLY when a branch genuinely needs the lift (some
/// `!` is present) and the lift's downstream tail effect-unwrap will lower soundly (no unproven-drop
/// Ok payload). A NON-admitted `!` flips `all_admitted` so the lift refuses → the raw `!` walls
/// cleanly (an honest `Unsupported` > a gate-invisible leak).
fn collect_arm_unwrap_admit(e: &IrExpr, has: &mut bool, all_admitted: &mut bool) {
    match &e.kind {
        IrExprKind::Unwrap { expr } => {
            *has = true;
            if !effect_unwrap_admitted(&expr.ty) {
                *all_admitted = false;
            }
            collect_arm_unwrap_admit(expr, has, all_admitted);
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Bind { value, .. } => collect_arm_unwrap_admit(value, has, all_admitted),
                    IrStmtKind::Expr { expr } => collect_arm_unwrap_admit(expr, has, all_admitted),
                    IrStmtKind::Assign { value, .. } => collect_arm_unwrap_admit(value, has, all_admitted),
                    _ => {}
                }
            }
            if let Some(t) = expr {
                collect_arm_unwrap_admit(t, has, all_admitted);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_arm_unwrap_admit(cond, has, all_admitted);
            collect_arm_unwrap_admit(then, has, all_admitted);
            collect_arm_unwrap_admit(else_, has, all_admitted);
        }
        IrExprKind::Match { subject, arms } => {
            collect_arm_unwrap_admit(subject, has, all_admitted);
            for a in arms {
                collect_arm_unwrap_admit(&a.body, has, all_admitted);
            }
        }
        _ => {}
    }
}

/// Push the continuation `{ after_stmts; tail }` into a single branch ARM, FLATTENING the arm's own
/// block so a stmt-position `!` in the arm becomes a TOP-LEVEL stmt of the produced block (reachable
/// by `desugar_effect_unwrap_inner` / `desugar_let_unwrap` once the enclosing branch is in tail
/// position). The arm's own tail expr (if any, and not a trivial Unit) is demoted to an `Expr` stmt
/// before the continuation. Typed at the enclosing fn's return `result_ty`.
fn append_arm_continuation(
    arm: &IrExpr,
    after_stmts: &[IrStmt],
    tail: &Option<Box<IrExpr>>,
    result_ty: &Ty,
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();
    match &arm.kind {
        IrExprKind::Block { stmts: bs, expr: be } => {
            stmts.extend(bs.iter().cloned());
            if let Some(be) = be {
                if !matches!(be.kind, IrExprKind::Unit) {
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Expr { expr: (**be).clone() },
                        span: be.span.clone(),
                    });
                }
            }
        }
        // A trivial Unit arm (`else ()`) contributes no statement — just the continuation.
        IrExprKind::Unit => {}
        _ => stmts.push(IrStmt {
            kind: IrStmtKind::Expr { expr: arm.clone() },
            span: arm.span.clone(),
        }),
    }
    stmts.extend(after_stmts.iter().cloned());
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: tail.clone() },
        ty: result_ty.clone(),
        span: arm.span.clone(),
        def_id: None,
    }
}

/// STATEMENT-CONTROL continuation-lift (the #76 statement-control continuation-lift, deferred from
/// the tail-only `desugar_tail_effect_unwrap`): a block `{ before; S; after }` where `S` is a UNIT
/// `Expr`-statement `if`/`match` whose arm transitively carries a stmt/let effect-`!` (`Unwrap`) and
/// `after` (the remaining stmts + tail) is NON-EMPTY. The stmt-`!` cannot desugar in place (the v1
/// MIR has no mid-function early-return Op) and `desugar_tail_effect_unwrap` only navigates TAIL
/// control flow — so `S` in statement position with a continuation is unreachable. LIFT it: push
/// `after` into EACH of `S`'s arm tails (the proven `desugar_let_bound_heap_branch` tail-DUPLICATION
/// discipline), turning `S` into the block TAIL. The existing tail effect-unwrap then rewrites the
/// `!` into `match f() { err(e) => err(e), ok(x) => { after } }` — the err-arm returning early from
/// the fn, the continuation nesting ONLY in the ok-arm. Lives in the SHARED `desugar_heap_branches`
/// so the duplicated `after` is counted 1:1 by `count_ir_calls` in BOTH the caps gate and the
/// lowering (mir == ir). HOLE-1: fire only when every triggering `!`-subject's Ok payload has a
/// proven drop (`effect_unwrap_admitted`); otherwise leave `S` untouched so the raw `!` walls.
fn desugar_stmt_control_unwrap(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    for (i, s) in stmts.iter().enumerate() {
        // S = a UNIT `Expr`-statement that is an `if`/`match` (a branch run for effect).
        let IrStmtKind::Expr { expr: s_expr } = &s.kind else {
            continue;
        };
        if !matches!(&s_expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) {
            continue;
        }
        if !matches!(s_expr.ty, Ty::Unit) {
            continue;
        }
        // `after` must be NON-EMPTY (a real continuation to lift past `S`).
        let after_stmts = &stmts[i + 1..];
        if after_stmts.is_empty() && tail.is_none() {
            continue;
        }
        // Fire ONLY for a branch that carries a stmt/let `!`, and ONLY when every such `!`'s Ok
        // payload is HOLE-1-admitted (else the lift would feed an unprovable-drop Ok into the tail
        // effect-unwrap — refuse so the raw `!` walls cleanly).
        let (mut has, mut all_admitted) = (false, true);
        collect_arm_unwrap_admit(s_expr, &mut has, &mut all_admitted);
        if !has || !all_admitted {
            continue;
        }
        // Push `after` into each arm's tail. The lifted branch is typed at the fn return `body.ty`.
        let new_s = match &s_expr.kind {
            IrExprKind::If { cond, then, else_ } => IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(append_arm_continuation(then, after_stmts, tail, &body.ty)),
                    else_: Box::new(append_arm_continuation(else_, after_stmts, tail, &body.ty)),
                },
                ty: body.ty.clone(),
                span: s_expr.span.clone(),
                def_id: s_expr.def_id,
            },
            IrExprKind::Match { subject, arms } => {
                let new_arms: Vec<almide_ir::IrMatchArm> = arms
                    .iter()
                    .map(|a| almide_ir::IrMatchArm {
                        pattern: a.pattern.clone(),
                        guard: a.guard.clone(),
                        body: append_arm_continuation(&a.body, after_stmts, tail, &body.ty),
                    })
                    .collect();
                IrExpr {
                    kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                    ty: body.ty.clone(),
                    span: s_expr.span.clone(),
                    def_id: s_expr.def_id,
                }
            }
            _ => unreachable!(),
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(new_s)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    None
}

/// Is `e` a PURE, freely-duplicable match subject — a `Var` or a literal? `build_match_chain`
/// inlines such a subject into EACH literal arm's `==` test (no re-eval cost / effect). A
/// non-pure subject (a CALL) inlined per arm would be EVALUATED once per arm — wrong if it has
/// effects, wasteful always, and it makes the MIR carry N subject-calls where the source had one
/// (a `mir > ir` caps-gate breach). [`desugar_match_subject_hoist`] lifts those to a single eval.
fn is_pure_match_subject(e: &IrExpr) -> bool {
    matches!(
        &e.kind,
        IrExprKind::Var { .. }
            | IrExprKind::LitInt { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. }
    )
}

/// HOIST a non-pure (call) subject of a NON-VARIANT `match` (the `build_match_chain` literal-arm
/// shape) into a single `let __m = subject` and rewrite the match to dispatch on `Var(__m)` — so
/// the subject call is EVALUATED ONCE, not duplicated into each arm's `==` test. This is both a
/// correctness fix (a side-effecting subject must run once) and the alignment that keeps the caps
/// gate exact: `count_ir_calls` then sees ONE subject call (matching the MIR's one), so `mir <= ir`
/// holds for a resolved cross-module/self-pkg call subject (`match q.kind(x) { "a" => .., .. }`).
/// Applied in the SHARED [`desugar_heap_branches`] so the lowering and the count gate agree.
/// FIRES ONLY for a non-variant match (Int/String/Bool literal arms) whose subject is non-pure —
/// a variant (Option/Result/ADT) match already evaluates its subject once (`bind_subject` /
/// `try_lower_variant_value_match`), so it is left untouched (no v0-corpus shape changes). Recurses
/// into block stmts / tails / if & match arms so a nested such match is hoisted too.
fn desugar_match_subject_hoist(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    use almide_lang::types::Ty;
    // A variant subject (Option/Result/user ADT) goes through the variant path (single-eval),
    // NOT build_match_chain — leave it alone.
    let is_variant_subject = |ty: &Ty| {
        matches!(ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(ty, Ty::Named(..) | Ty::Variant { .. })
    };
    if let IrExprKind::Match { subject, arms } = &body.kind {
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        // A COMPUTED-call (funcref / `Op::CallIndirect`) subject cannot lower INLINE through the
        // variant path (which materializes only a Var / Named / Module subject), so hoist it to a
        // `let $t = f(x); match $t` even for an Option/Result subject — the bind's heap-result
        // CallIndirect + seeded read-shape then makes the match lower (the `fan.map` traverse `match
        // f(x) { ok/err }` shape).
        let is_computed_call = matches!(
            &subject.kind,
            IrExprKind::Call { target: almide_ir::CallTarget::Computed { .. }, .. }
        ) || matches!(
            &subject.kind,
            // `fan.map` (a compiler intrinsic lowered to a self-host Result call) as a match subject —
            // hoist it so the bind seeds its cap-as-tag read-shape, then `match $t { ok/err }` lowers
            // (its auto-`!` desugars to exactly this match).
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, .. }
                if module.as_str() == "fan" && func.as_str() == "map"
        ) || matches!(
            &subject.kind,
            // `regex.find(...)` (a self-host Option[String] call) as a match subject —
            // hoist so the bind seeds its materialized-Option read-shape, then
            // `match $t { some/none }` lowers (the regex-corpus match shape).
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, .. }
                if module.as_str() == "regex" && (func.as_str() == "find" || func.as_str() == "captures")
        );
        if (has_literal_arm
            && !is_pure_match_subject(subject)
            && !is_variant_subject(&subject.ty))
            || is_computed_call
        {
            let tmp = VarId(*next_var);
            *next_var += 1;
            let tmp_var = IrExpr {
                kind: IrExprKind::Var { id: tmp },
                ty: subject.ty.clone(),
                span: subject.span.clone(),
                def_id: None,
            };
            // The match dispatching on the hoisted `Var(tmp)` (arms unchanged — they reference the
            // subject only through the desugar's `subject.clone()`, now the cheap Var).
            let new_match = IrExpr {
                kind: IrExprKind::Match { subject: Box::new(tmp_var), arms: arms.clone() },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            };
            let bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: almide_ir::Mutability::Let,
                    ty: subject.ty.clone(),
                    value: (**subject).clone(),
                },
                span: body.span.clone(),
            };
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(new_match)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    // Recurse into the structural positions a match can hide in.
    match &body.kind {
        IrExprKind::Block { stmts, expr } => {
            // Recurse into each stmt's value (Bind / Expr / Assign — the value-bearing stmts a
            // match can sit in) by cloning the stmt and replacing its value via `map_children`.
            for (i, s) in stmts.iter().enumerate() {
                let v = match &s.kind {
                    IrStmtKind::Expr { expr } => Some(expr),
                    IrStmtKind::Bind { value, .. } => Some(value),
                    IrStmtKind::Assign { value, .. } => Some(value),
                    _ => None,
                };
                if let Some(v) = v {
                    if let Some(nv) = desugar_match_subject_hoist(v, next_var) {
                        let mut ns = stmts.clone();
                        ns[i].kind = match s.kind.clone() {
                            IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: nv },
                            IrStmtKind::Bind { var, mutability, ty, .. } => {
                                IrStmtKind::Bind { var, mutability, ty, value: nv }
                            }
                            IrStmtKind::Assign { var, .. } => IrStmtKind::Assign { var, value: nv },
                            other => other,
                        };
                        return Some(IrExpr {
                            kind: IrExprKind::Block { stmts: ns, expr: expr.clone() },
                            ty: body.ty.clone(),
                            span: body.span.clone(),
                            def_id: body.def_id,
                        });
                    }
                }
            }
            if let Some(t) = expr {
                if let Some(nt) = desugar_match_subject_hoist(t, next_var) {
                    return Some(IrExpr {
                        kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                        ty: body.ty.clone(),
                        span: body.span.clone(),
                        def_id: body.def_id,
                    });
                }
            }
            None
        }
        IrExprKind::If { cond, then, else_ } => {
            if let Some(nt) = desugar_match_subject_hoist(then, next_var) {
                return Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: Box::new(nt),
                        else_: else_.clone(),
                    },
                    ty: body.ty.clone(),
                    span: body.span.clone(),
                    def_id: body.def_id,
                });
            }
            if let Some(ne) = desugar_match_subject_hoist(else_, next_var) {
                return Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: then.clone(),
                        else_: Box::new(ne),
                    },
                    ty: body.ty.clone(),
                    span: body.span.clone(),
                    def_id: body.def_id,
                });
            }
            None
        }
        IrExprKind::Match { subject, arms } => {
            for (i, a) in arms.iter().enumerate() {
                if let Some(nb) = desugar_match_subject_hoist(&a.body, next_var) {
                    let mut na = arms.clone();
                    na[i].body = nb;
                    return Some(IrExpr {
                        kind: IrExprKind::Match { subject: subject.clone(), arms: na },
                        ty: body.ty.clone(),
                        span: body.span.clone(),
                        def_id: body.def_id,
                    });
                }
            }
            None
        }
        _ => None,
    }
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
    let rewritten = desugar_heap_branches_inner(body, &mut next_var)?;
    // EXPONENTIAL-BLOW-UP guard: each `let s = <heap branch>; rest` duplicates `rest`
    // into both arms, so N chained branch binds yield 2^N copies. Real programs chain
    // 2–4 deep (16 copies — fine); an adversarial/generated 20-chain would be a
    // million-node body (a compile-time hang, not a wrong answer). Past the cap the
    // rewrite is DISCARDED — the un-desugared bind then WALLS in `lower_bind`
    // (an honest refusal, never a hang).
    const MAX_DESUGARED_NODES: usize = 200_000;
    if count_expr_nodes(&rewritten) > MAX_DESUGARED_NODES {
        return None;
    }
    Some(rewritten)
}

/// Node count of an expression tree (the blow-up guard metric).
fn count_expr_nodes(e: &IrExpr) -> usize {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C(usize);
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 += 1;
            walk_expr(self, e);
        }
    }
    let mut c = C(0);
    c.visit_expr(e);
    c.0
}

fn desugar_heap_branches_inner(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let mut cur: Option<IrExpr> = None;
    loop {
        let src = cur.as_ref().unwrap_or(body);
        // FIRST: hoist a non-pure (call) match subject to a single eval, so the literal-arm chain
        // dispatches on a cheap Var instead of duplicating the call per arm — a correctness fix
        // (single eval) and the alignment that keeps `mir <= ir` for a resolved cross-module/self-pkg
        // call subject. Runs before the call-arg lifts so they see the hoisted (Var-subject) form.
        if let Some(r) = desugar_match_subject_hoist(src, next_var) {
            cur = Some(r);
            continue;
        }
        // Route a non-empty map literal through `map.from_list` so it materializes a real map (else a
        // deferred-Opaque empty block silently miscompiles every subsequent map op).
        if let Some(r) = desugar_map_literal(src) {
            cur = Some(r);
            continue;
        }
        // Inline a `fan.race`/`fan.any` over a literal thunk list (avoids an unrepresentable
        // List[funcref]) into a plain match-over-a-call chain.
        if let Some(r) = desugar_fan_race_any(src, next_var) {
            cur = Some(r);
            continue;
        }
        // Regroup a guarded/literal Option/Result match into ctor-dispatch + a payload sub-match, so
        // the guarded-variant case reduces to the two already-proven pieces.
        if let Some(r) = desugar_grouped_variant_match(src, next_var) {
            cur = Some(r);
            continue;
        }
        // Hoist a LITERAL record/tuple interpolation part (`"${(1, \"x\", true)}"`) to a
        // temp binding so the part becomes a materialized Var the EXPAND display folds
        // (a literal part is never a tracked block, so it fell to the unlinked
        // `compound.to_string` wall).
        if let Some(r) = desugar_interp_literal_aggregate_hoist(src, next_var) {
            cur = Some(r);
            continue;
        }
        // Lower a match over a TUPLE subject into element index-tests + an if-chain (also handles the
        // tuple sub-match a multi-field variant regroup produces).
        if let Some(r) = desugar_tuple_match(src) {
            cur = Some(r);
            continue;
        }
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
        if let Some(r) = desugar_callarg_unwrap(src, next_var) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_let_bound_heap_branch(src) {
            cur = Some(r);
            continue;
        }
        // `{ …; let r = e!; ok(r) }` ≡ `{ …; e }` (unwrap-rewrap identity) — collapse BEFORE the
        // let-unwrap continuation desugar, so read_message's `ok(parse_and_wrap(body)!)` arms become
        // bare tail-call arms instead of a heap-Option continuation match.
        if let Some(r) = desugar_unwrap_rewrap_identity(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_let_unwrap(src) {
            cur = Some(r);
            continue;
        }
        // Collapse the scopeless `Block { stmts: [], expr: e }` wrappers `desugar_let_unwrap` leaves
        // behind (one per `?`-bind field of the derived variant decode), so the nested monadic matches
        // lower like the hand-written form instead of walling on the `Block`-wrapped arm.
        if let Some(r) = desugar_flatten_empty_block(src) {
            cur = Some(r);
            continue;
        }
        // effect-`!` inside a `for` loop body → loop-carried error-flag + post-loop dispatch (the
        // effect-monad-in-loop frontier; a PURE IR→IR desugar over the proven loop-slot + heap-if).
        if let Some(r) = desugar_loop_unwrap(src, next_var) {
            cur = Some(r);
            continue;
        }
        // STATEMENT-CONTROL continuation-lift: a UNIT `if`/`match` STATEMENT carrying a stmt/let `!`
        // followed by a non-empty continuation. Lift `after` into each arm (tail-duplication) so the
        // branch becomes the block TAIL — the tail effect-unwrap then resolves the `!`. Runs in this
        // SHARED desugar so the duplicated `after` is counted 1:1 by the caps gate (mir == ir).
        if let Some(r) = desugar_stmt_control_unwrap(src) {
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
            // Recurse into BOTH the block's `let`-bind STMT values AND its tail. A HOF call binding
            // `let case_lines = cases |> list.flat_map((entry) => { … let cond = if … ; … })` hides a
            // let-bound heap `if` inside the lambda arg — only reachable by descending the bind value.
            // The stmt-value recursion uses the FOCUSED `desugar_lambda_let_branches` (let-bound-branch
            // duplication ONLY, into nested if/match/block/lambda) — NOT the full `desugar_heap_branches
            // _inner` fixpoint, whose function-body-tuned passes regress an already-lowerable bind value
            // (julia `gen_variant_types`'s `let case_lines = <flat_map of match {…}+[""]>` walled when run
            // through the full fixpoint). The tail KEEPS the full fixpoint (the existing nested-arm path).
            let mut changed = false;
            let new_stmts: Vec<IrStmt> = stmts
                .iter()
                .map(|s| match &s.kind {
                    IrStmtKind::Bind { var, mutability, ty, value } => {
                        match desugar_lambda_let_branches(value) {
                            Some(nv) => {
                                changed = true;
                                IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: *var,
                                        mutability: *mutability,
                                        ty: ty.clone(),
                                        value: nv,
                                    },
                                    span: s.span.clone(),
                                }
                            }
                            None => s.clone(),
                        }
                    }
                    _ => s.clone(),
                })
                .collect();
            let nt = desugar_heap_branches_inner(tail, next_var);
            if nt.is_some() {
                changed = true;
            }
            if !changed {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::Block {
                    stmts: new_stmts,
                    expr: Some(Box::new(nt.unwrap_or_else(|| (**tail).clone()))),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        // A `list.map`/`flat_map`/… CALL carrying an INLINE-LAMBDA arg whose body hides a LET-BOUND
        // heap `if`/`match` (the bindgen `gen_pack_variant` outer flat_map's `let cond = if idx==0
        // then "if" else "elseif"`). Apply ONLY the let-bound-branch tail-duplication INSIDE the
        // lambda body (`desugar_lambda_let_branches`) — NOT the full `desugar_heap_branches_inner`
        // fixpoint, whose other passes (match-subject-hoist, call-arg `if`/unwrap lifts) are tuned for
        // the FUNCTION-body lowering path and would mangle an already-lowerable defunc-lambda shape (a
        // `match {…} + [""]` body regressed when run through the full fixpoint). This stays a STRICT
        // no-op for a lambda WITHOUT a let-bound heap-branch (julia `gen_variant_types` is untouched).
        // Applied BEFORE both the defunc lowering and the `count_ir_calls` gate (desugar-before-both:
        // both see the IDENTICAL duplicated lambda body, mir==ir 1:1 by construction). Params/lambda_id
        // are preserved so the defunc inliner binds the same params.
        // SKIP a `list.fold` lambda: the tuple-accumulator fold (`try_lower_defunc_tuple_acc_fold`)
        // lowers its OWN multi-statement body — a `let store = if/match` INTERIOR stmt is materialized
        // by `lower_bind` (the heap-result-`if` merged-owned-value path) reading the slots directly.
        // Tail-DUPLICATING that `let store = if` here would turn the body `{ let store=if; (acc+[store],
        // n+1) }` into `if c then {(acc+[A],n+1)} else {…}` — an if-of-TUPLES the tuple-fold gate (which
        // requires a `(c0, c1)` tuple tail) cannot match → it declines to the self-host `fold_hacc`.
        // (map/flat_map's str-acc DOES handle the if-of-tuples-equivalent via its unit-append `if`, so
        // those keep the desugar.)
        IrExprKind::Call { target, args, type_args }
            if !matches!(target,
                CallTarget::Module { module, func, .. }
                    if module.as_str() == "list" && func.as_str() == "fold") =>
        {
            let mut changed = false;
            let new_args: Vec<IrExpr> = args
                .iter()
                .map(|a| match &a.kind {
                    IrExprKind::Lambda { params, body: lam_body, lambda_id } => {
                        match desugar_lambda_let_branches(lam_body) {
                            Some(nb) => {
                                changed = true;
                                IrExpr {
                                    kind: IrExprKind::Lambda {
                                        params: params.clone(),
                                        body: Box::new(nb),
                                        lambda_id: *lambda_id,
                                    },
                                    ty: a.ty.clone(),
                                    span: a.span.clone(),
                                    def_id: a.def_id,
                                }
                            }
                            None => a.clone(),
                        }
                    }
                    _ => a.clone(),
                })
                .collect();
            if !changed {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::Call {
                    target: target.clone(),
                    args: new_args,
                    type_args: type_args.clone(),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        // A `(<flat_map call>) + [tail]` ConcatList/ConcatStr — the bindgen `gen_pack_variant` /
        // `gen_variant_struct` outer shape `(cases |> list.flat_map(…)) + ["${indent}}"]`. The HOF
        // call whose lambda hides a let-bound heap-branch sits in a BinOp OPERAND, unreachable by the
        // `Call`/`Block`/arm cases above. Recurse into BOTH operands so the flat_map's lambda-let-if is
        // tail-duplicated (otherwise the outer flat_map declines → the concat walls `heap-result BinOp`).
        IrExprKind::BinOp { op, left, right } => {
            let nl = desugar_nested_branch_arms(left, next_var);
            let nr = desugar_nested_branch_arms(right, next_var);
            if nl.is_none() && nr.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::BinOp {
                    op: *op,
                    left: Box::new(nl.unwrap_or_else(|| (**left).clone())),
                    right: Box::new(nr.unwrap_or_else(|| (**right).clone())),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            })
        }
        _ => None,
    }
}

/// FOCUSED let-bound-heap-branch desugar for a DEFUNC-LAMBDA BODY (`(entry) => { … let cond = if …;
/// … }`). Applies ONLY the let-bound-branch tail-duplication (`desugar_let_bound_heap_branch`),
/// recursing through the body's nested structure (Block stmts + tail, if/match arms, and INNER HOF
/// lambdas) — NOT the full `desugar_heap_branches_inner` fixpoint, whose match-subject-hoist /
/// call-arg lift passes are tuned for the function-body lowering and REGRESS an already-lowerable
/// defunc-lambda shape (a `match {…} + [""]` body). A STRICT no-op when the lambda body has no
/// let-bound heap-branch. Returns `Some(rewritten)` when a duplication fired, `None` otherwise.
///
/// SOUNDNESS: identical to `desugar_let_bound_heap_branch` — a PURE IR→IR tail-duplication (each arm
/// binds its value + runs the continuation, per-arm `i…d` balance, only one arm runs = v0-identical,
/// NO cert/Coq change). Applied desugar-before-both (the shared desugar runs over the lambda body for
/// BOTH the defunc lowering and the caps `count_ir_calls` gate), so mir==ir 1:1 holds by construction.
fn desugar_lambda_let_branches(body: &IrExpr) -> Option<IrExpr> {
    // Fixpoint: apply the let-bound-branch duplication at THIS position until it stops firing (the
    // continuation may expose a second let-bound branch — the bounded gate caps the depth), then
    // recurse structurally so a duplication INSIDE an arm / inner lambda is reached too.
    let mut cur: Option<IrExpr> = None;
    loop {
        let src = cur.as_ref().unwrap_or(body);
        if let Some(r) = desugar_let_bound_heap_branch(src) {
            cur = Some(r);
            continue;
        }
        // Inline a SINGLE-USE let-bound match subject INSIDE the lambda body too (`(case) => { let p =
        // json.get(case,"payload"); match p { … } }`) — turning the let-bound-Var variant-match subject
        // into the inline-subject form the str-acc handler C1-lowers (vs the funcref-dropping C2-lift).
        // Value-preserving (single use ⇒ one eval); STRICT-gated so it is a no-op otherwise.
        if let Some(r) = desugar_inline_single_use_match_subject(src) {
            cur = Some(r);
            continue;
        }
        break;
    }
    let src_owned = cur;
    let src = src_owned.as_ref().unwrap_or(body);
    // Recurse structurally (let-branch desugar only) into the parts that may host a defunc lambda or a
    // nested let-bound branch. A change anywhere — here or the top-level duplication above — yields Some.
    let recursed = match &src.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let mut changed = false;
            let new_stmts: Vec<IrStmt> = stmts
                .iter()
                .map(|s| match &s.kind {
                    IrStmtKind::Bind { var, mutability, ty, value } => {
                        match desugar_lambda_let_branches(value) {
                            Some(nv) => {
                                changed = true;
                                IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: *var,
                                        mutability: *mutability,
                                        ty: ty.clone(),
                                        value: nv,
                                    },
                                    span: s.span.clone(),
                                }
                            }
                            None => s.clone(),
                        }
                    }
                    _ => s.clone(),
                })
                .collect();
            let nt = desugar_lambda_let_branches(tail);
            if nt.is_some() {
                changed = true;
            }
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Block {
                        stmts: new_stmts,
                        expr: Some(Box::new(nt.unwrap_or_else(|| (**tail).clone()))),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_lambda_let_branches(then);
            let ne = desugar_lambda_let_branches(else_);
            if nt.is_none() && ne.is_none() {
                None
            } else {
                Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                        else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            }
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_lambda_let_branches(&a.body) {
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
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        // An inner HOF call (`get_arr(pl,"fields") |> list.flat_map((pe) => { … })`): recurse into its
        // lambda args so a let-bound branch in a NESTED loop body is reached too. SKIP a `list.fold`
        // lambda — the tuple-accumulator fold lowers its OWN multi-statement body (a `let store =
        // if/match` interior stmt materialized by `lower_bind`); tail-duplicating it here would turn the
        // `(acc+[store], n+step)` tuple body into an if-of-tuples the tuple-fold gate cannot match.
        IrExprKind::Call { target, args, type_args }
            if !matches!(target,
                CallTarget::Module { module, func, .. }
                    if module.as_str() == "list" && func.as_str() == "fold") =>
        {
            let mut changed = false;
            let new_args: Vec<IrExpr> = args
                .iter()
                .map(|a| match &a.kind {
                    IrExprKind::Lambda { params, body: lam_body, lambda_id } => {
                        match desugar_lambda_let_branches(lam_body) {
                            Some(nb) => {
                                changed = true;
                                IrExpr {
                                    kind: IrExprKind::Lambda {
                                        params: params.clone(),
                                        body: Box::new(nb),
                                        lambda_id: *lambda_id,
                                    },
                                    ty: a.ty.clone(),
                                    span: a.span.clone(),
                                    def_id: a.def_id,
                                }
                            }
                            None => a.clone(),
                        }
                    }
                    _ => a.clone(),
                })
                .collect();
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: target.clone(),
                        args: new_args,
                        type_args: type_args.clone(),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        _ => None,
    };
    // Some if EITHER the top-level duplication fired OR a structural recursion changed something.
    match (src_owned, recursed) {
        (_, Some(r)) => Some(r),
        (Some(s), None) => Some(s),
        (None, None) => None,
    }
}

pub fn desugar_let_bound_heap_branch(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find the first heap let-bound `if`/`match` bind — OR a SCALAR-typed one whose
    // arms carry an error operator (`let v = if c then boom(x)! else boom(y)!` — the
    // effect-fn auto-`?` puts a `Try` INSIDE each arm; the scalar path silently
    // stripped it and bound the raw Result handle, the via_if class the proven
    // checker's leak line exposed). Tail-duplicating pushes the continuation into
    // each arm, where the ordinary monadic `!` desugar handles the Try faithfully.
    fn arm_has_error_op(e: &IrExpr) -> bool {
        fn direct(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } => true,
                IrExprKind::Block { expr: Some(t), .. } => direct(t),
                _ => false,
            }
        }
        match &e.kind {
            IrExprKind::If { then, else_, .. } => direct(then) || direct(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| direct(&a.body)),
            _ => false,
        }
    }
    let (i, bind_var, bind_ty, branch) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. }
            if (is_heap_ty(ty) || arm_has_error_op(value))
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
    // allow up to 3 (≤ 2^4 = 16 leaves) and refuse beyond that to keep the duplication bounded. The
    // wasm-bindgen `generate_esm` shape stacks 4 top-level optional-list `if`s (matrix/bytes/import_shim/
    // shim_noop, each `if cond then [LITERALS] else []`); the FIRST sees rest = 3, so the old `> 2` gate
    // bailed → the merged let-bound `if` walled. Raising to `> 3` lets the proven per-arm-balanced
    // tail-duplication resolve all 4 (each leaf independently binds + drops its own list — corpus-wall
    // re-verifies every arm). The leaves are mostly literal-list concats (cheap), so 16× is tolerable for
    // a build-time codegen tool; deeper stacks still WALL rather than blow up.
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
    if rest_branch_binds > 3 {
        return None;
    }
    let result_ty = &body.ty;
    let rest_stmts: Vec<IrStmt> = stmts[i + 1..].to_vec();
    let rest_tail: Option<Box<IrExpr>> = tail.clone();
    // Push the continuation `{ rest }` behind the per-arm bind of `bind_var`. A literal-pattern
    // `match` (or `if`) reduces to a nested `if` chain via `desugar_match_to_if` and uses
    // `wrap_branch_arms`; a CUSTOM-VARIANT / non-literal `match` (`match s.shape { Circle(_) =>
    // "circle", … }`) — which `desugar_match_to_if` declines — keeps its `Match` and pushes the
    // continuation into each arm via `wrap_match_arms` (the proven tail custom-variant match then
    // runs each arm). Both are call-count-invariant, so `mir == ir` holds.
    let rewritten_branch = match &branch.kind {
        IrExprKind::If { .. } => LowerCtx::wrap_branch_arms(
            branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
        ),
        IrExprKind::Match { subject, arms } => {
            match LowerCtx::default().desugar_match_to_if(subject, arms, &branch.ty) {
                Some(if_branch) => LowerCtx::wrap_branch_arms(
                    &if_branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
                ),
                None => LowerCtx::wrap_match_arms(
                    subject, arms, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
                ),
            }
        }
        _ => return None,
    };
    // The prefix statements `stmts[0..i]` stay; the rewritten branch is the new block TAIL.
    let prefix: Vec<IrStmt> = stmts[..i].to_vec();
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: prefix, expr: Some(Box::new(rewritten_branch)) },
        ty: result_ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `{ …; let r = e!; ok(r) }` ≡ `{ …; e }` — the UNWRAP-REWRAP IDENTITY. `e!` unwraps `Ok(x)→x` /
/// propagates `Err`, and `ok(r)` re-wraps it UNCHANGED, so the trailing `let r = e!; ok(r)` collapses to
/// `e` (a `Result`-typed tail). This is the layer-3 simplifier for porta read_message's Content-Length
/// arms (`{ … let r = parse_and_wrap(body)!; ok(r) }` → `{ … parse_and_wrap(body) }`): the `!`-in-a-
/// heap-result-`if`-arm becomes a bare tail-call arm the real-recursive arm path already lowers. Gated:
/// the `Bind` is the LAST stmt, the tail is exactly `ok(Var r)` over that var, and `e` is `Result`-typed
/// (so the re-wrap is a true identity). Recurses via the shared pipeline (`desugar_nested_branch_arms`),
/// so a deeply-nested arm is reached. Pure IR→IR; NO certificate/Coq change.
pub fn desugar_unwrap_rewrap_identity(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else {
        return None;
    };
    // The tail must be `ok(Var r)`.
    let IrExprKind::ResultOk { expr: tail_inner } = &tail.kind else {
        return None;
    };
    let IrExprKind::Var { id: r } = &tail_inner.kind else {
        return None;
    };
    // The LAST stmt must be `let r = e!` (`Bind` of an `Unwrap`) over the SAME var.
    let last = stmts.last()?;
    let IrStmtKind::Bind { var, value, .. } = &last.kind else {
        return None;
    };
    if var != r {
        return None;
    }
    let IrExprKind::Unwrap { expr: e } = &value.kind else {
        return None;
    };
    // `e` must be `Result`-typed (`e!` then `ok(r)` is the identity only when `e` is the SAME `Result`).
    if !e.ty.is_result() {
        return None;
    }
    // `r` is bound at the last stmt and used only in the tail `ok(r)` — removing the bind + collapsing
    // the tail to `e` references it nowhere, so the rewrite is sound.
    let mut new_stmts = stmts[..stmts.len() - 1].to_vec();
    let new_tail = (**e).clone();
    // Drop a now-empty trailing block to just `e` would change the node kind needlessly; keep the Block
    // wrapper (its tail is `e`), which lowers identically.
    let _ = &mut new_stmts;
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_tail)) },
        ty: body.ty.clone(),
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
    // A single-var `let v = e!` (Bind-of-Unwrap) OR a destructure `let (a, b) = e!` (BindDestructure
    // whose VALUE is Result-typed — the `!` lowers to a bare Result-typed Call here, not a kept Unwrap
    // node, so key on the TYPE). Both early-return the `Err(E)` and bind/destructure the `Ok(T)`.
    enum Target {
        Single { var: VarId, ty: Ty },
        Destructure { pattern: almide_ir::IrPattern },
    }
    let (i, target, inner) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
            // `!` (Unwrap) and `?` (Try) both propagate the `Err(E)` in an effect fn — the SAME
            // early-return this desugar builds. The derive-generated field binds arrive as `Try`
            // (`let _e0 = value.as_int(..)?`) — handle both so they lower to the match, not a
            // heap-result Try left for `lower_call_args` to wall.
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                Some((i, Target::Single { var: *var, ty: ty.clone() }, (**expr).clone()))
            }
            _ => None,
        },
        IrStmtKind::BindDestructure { pattern, value } => {
            // `let (a,b) = e!` / `let (a,b) = e?`. The `!`/`?` is EITHER kept as an `Unwrap`/`Try`
            // node (inner = its Result expr) OR already stripped to a bare Result-typed value (inner
            // = the value) — handle all three so a destructure-let-unwrap never reaches lowering as a
            // Result-destructured-as-a-tuple. The derived variant decode's `let (_tag, _payload) =
            // value.tagged_variant(v)?` is exactly the kept-`Try` case.
            let inner = match &value.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => Some((**expr).clone()),
                _ if value.ty.is_result() => Some(value.clone()),
                _ => None,
            }?;
            Some((i, Target::Destructure { pattern: pattern.clone() }, inner))
        }
        _ => None,
    })?;
    // The unwrapped expr must be a `Result[T, E]` — `!` early-returns its `Err(E)`, binds `Ok(T)`.
    let (ok_ty, err_ty) = match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => (a[0].clone(), a[1].clone()),
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
    // ok(<bind>) => { <rest> }. A destructure becomes `ok($p2) => { let (a,b) = $p2; <rest> }` — the
    // hand-written direct-match form that already lowers (a Result destructured directly as a tuple
    // otherwise silently miscompiled: the wrapper @12/@16 was read as the tuple fields).
    let (ok_pattern, cont_stmts): (almide_ir::IrPattern, Vec<IrStmt>) = match target {
        Target::Single { var, ty } => {
            (almide_ir::IrPattern::Bind { var, ty }, stmts[i + 1..].to_vec())
        }
        Target::Destructure { pattern } => {
            let p2 = VarId(max_var_id(body) + 2);
            let destr = IrStmt {
                kind: IrStmtKind::BindDestructure {
                    pattern,
                    value: mk(IrExprKind::Var { id: p2 }, ok_ty.clone()),
                },
                span: body.span.clone(),
            };
            let mut cs = vec![destr];
            cs.extend(stmts[i + 1..].iter().cloned());
            (almide_ir::IrPattern::Bind { var: p2, ty: ok_ty.clone() }, cs)
        }
    };
    let cont = mk(
        IrExprKind::Block { stmts: cont_stmts, expr: tail.clone() },
        result_ty.clone(),
    );
    let ok_arm = almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Ok { inner: Box::new(ok_pattern) },
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

// ─────────── effect-`!` inside a `for` loop body → loop-carried error flag ───────────
//
// A `for x in xs { … e! … }` in a function returning `Result[T, E]` cannot early-return the
// `Err(E)` from inside the loop in the FLAT certificate (a mid-loop `return` makes the loop's
// owned iterable + iteration transients conditionally dropped — a double-drop the flat checker
// rejects). This is the effect-monad-in-loop frontier.
//
// REWRITE (a PURE IR→IR desugar — "desugar-before-both", no new MIR op, no certificate/Coq
// change; it reuses the PROVEN loop-carried scalar/heap slot reassignment + heap-result-`if`):
//
//   { <pre>; for x in xs { BODY }; <post> }
//     ↓
//   { <pre>;
//     var __ef = false;            // err flag (scalar loop-carried)
//     var __ev = <empty E>;        // err accumulator (heap loop-carried slot, reassigned on Err)
//     for x in xs { if not __ef then { BODY' } else () };
//     if __ef then err(__ev) else { <post> } }
//
// where BODY' replaces each `let v = e!` / `e!` with `match e { ok(v) => <rest>, err($x) =>
// { __ef = true; __ev = $x } }`. BYTE-IDENTICAL to early-return: once `__ef` is set the per-
// iteration `if not __ef` guard skips ALL remaining effects (no later `e!`/`println` runs), the
// loop terminates by exhausting `xs` (a BOUNDED iteration — this is why it is `for` ONLY: a
// `while` could spin forever once its progress update is skipped), and the post-loop dispatch
// yields the propagated `Err`. Per-iteration transients are dropped by the normal iter-scope
// (there is NO early exit), so no special leak handling is needed.
//
// FAIL-SAFE: any `!` the rewrite cannot place a clean continuation behind is left untouched, so
// the function still WALLS at lowering (never a silent miscompile).

/// Does `e` contain an effect-`!` (`Unwrap`) anywhere in its subtree?
fn expr_has_unwrap(e: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct U(bool);
    impl IrVisitor for U {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Unwrap { .. }) {
                self.0 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut u = U(false);
    u.visit_expr(e);
    u.0
}

/// Does statement `s` contain an effect-`!` anywhere in its subtree?
fn stmt_has_unwrap(s: &IrStmt) -> bool {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    struct U(bool);
    impl IrVisitor for U {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Unwrap { .. }) {
                self.0 = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut u = U(false);
    walk_stmt(&mut u, s);
    u.0
}

fn loop_uw_node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

/// The `err($x) => { __ef = true; __ev = $x }` arm (a fresh `$x` allocated from `nv`).
fn loop_uw_err_arm(ef: VarId, ev: VarId, err_ty: &Ty, nv: &mut u32) -> almide_ir::IrMatchArm {
    let x = VarId(*nv);
    *nv += 1;
    let set_flag = IrStmt {
        kind: IrStmtKind::Assign {
            var: ef,
            value: loop_uw_node(IrExprKind::LitBool { value: true }, Ty::Bool),
        },
        span: None,
    };
    // Store an OWNED copy (`$x ++ ""`, a fresh String) — NOT the borrowed match payload. The
    // loop-carried slot must OWN its value so the post-loop move-out is not a double-free of the
    // subject's reference; the concat allocates a fresh String, severing the borrow. This is what
    // turns the slot's ownership certificate into the PROVEN `i(id)m` loop-slot shape (storing the
    // bare borrow certifies as the unsound `idm` = init/drop/move-a-dead-ref). `err_ty` is gated
    // to `String` by the caller, so `ConcatStr` typechecks and yields the same bytes as `$x`.
    let owned = loop_uw_node(
        IrExprKind::BinOp {
            op: almide_ir::BinOp::ConcatStr,
            left: Box::new(loop_uw_node(IrExprKind::Var { id: x }, err_ty.clone())),
            right: Box::new(loop_uw_node(
                IrExprKind::LitStr { value: String::new() },
                Ty::String,
            )),
        },
        err_ty.clone(),
    );
    let set_val = IrStmt {
        kind: IrStmtKind::Assign { var: ev, value: owned },
        span: None,
    };
    almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Err {
            inner: Box::new(almide_ir::IrPattern::Bind { var: x, ty: err_ty.clone() }),
        },
        guard: None,
        body: loop_uw_node(
            IrExprKind::Block { stmts: vec![set_flag, set_val], expr: None },
            Ty::Unit,
        ),
    }
}

/// `let v = e!` / `Expr(e!)` whose `!` propagates `Result[_, err_ty]` → `(ok_pattern, inner)`.
fn loop_uw_unwrap_stmt(s: &IrStmt, err_ty: &Ty) -> Option<(almide_ir::IrPattern, IrExpr)> {
    use almide_lang::types::constructor::TypeConstructorId;
    let (ok_pat, inner): (almide_ir::IrPattern, IrExpr) = match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
            IrExprKind::Unwrap { expr } => {
                (almide_ir::IrPattern::Bind { var: *var, ty: ty.clone() }, (**expr).clone())
            }
            _ => return None,
        },
        IrStmtKind::Expr { expr } => match &expr.kind {
            IrExprKind::Unwrap { expr: inner } => {
                (almide_ir::IrPattern::Wildcard, (**inner).clone())
            }
            _ => return None,
        },
        _ => return None,
    };
    match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && &a[1] == err_ty => {
            Some((ok_pat, inner))
        }
        _ => None,
    }
}

/// Rewrite a UNIT-typed loop-body remainder `e`, replacing each effect-`!` with a flag-setting
/// `match`. Returns `None` (the whole desugar declines, leaving the `!` to WALL) if any `!` sits
/// in a position where a clean continuation cannot be captured.
fn loop_uw_rewrite(e: &IrExpr, ef: VarId, ev: VarId, err_ty: &Ty, nv: &mut u32) -> Option<IrExpr> {
    if !expr_has_unwrap(e) {
        return Some(e.clone());
    }
    match &e.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // First DIRECT `let v=e!` / `Expr(e!)`: push the rest of the block into its ok-arm.
            for (i, s) in stmts.iter().enumerate() {
                if let Some((ok_pat, inner)) = loop_uw_unwrap_stmt(s, err_ty) {
                    // Everything BEFORE the `!` must be `!`-free (else its continuation is wrong).
                    if stmts[..i].iter().any(stmt_has_unwrap) {
                        return None;
                    }
                    let rest = loop_uw_node(
                        IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
                        Ty::Unit,
                    );
                    let rest2 = loop_uw_rewrite(&rest, ef, ev, err_ty, nv)?;
                    let ok_arm = almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::Ok { inner: Box::new(ok_pat) },
                        guard: None,
                        body: rest2,
                    };
                    let err_arm = loop_uw_err_arm(ef, ev, err_ty, nv);
                    let m = loop_uw_node(
                        IrExprKind::Match { subject: Box::new(inner), arms: vec![ok_arm, err_arm] },
                        Ty::Unit,
                    );
                    return Some(loop_uw_node(
                        IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(m)) },
                        Ty::Unit,
                    ));
                }
            }
            // No direct `!` stmt: the `!` is nested in a TERMINAL `if`/`match` (the tail, or the
            // last stmt) — recurse into it. Everything else must be `!`-free.
            if let Some(t) = tail {
                if stmts.iter().all(|s| !stmt_has_unwrap(s)) {
                    let nt = loop_uw_rewrite(t, ef, ev, err_ty, nv)?;
                    return Some(loop_uw_node(
                        IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                        Ty::Unit,
                    ));
                }
                return None;
            }
            // No tail: the unwrap must be in the LAST stmt (an `Expr(if/match)`), rest `!`-free.
            let last = stmts.len().checked_sub(1)?;
            if stmts[..last].iter().any(stmt_has_unwrap) {
                return None;
            }
            if let IrStmtKind::Expr { expr } = &stmts[last].kind {
                let ne = loop_uw_rewrite(expr, ef, ev, err_ty, nv)?;
                let mut ns = stmts[..last].to_vec();
                ns.push(IrStmt { kind: IrStmtKind::Expr { expr: ne }, span: stmts[last].span.clone() });
                return Some(loop_uw_node(
                    IrExprKind::Block { stmts: ns, expr: None },
                    Ty::Unit,
                ));
            }
            None
        }
        IrExprKind::If { cond, then, else_ } => {
            if expr_has_unwrap(cond) {
                return None;
            }
            let nt = loop_uw_rewrite(then, ef, ev, err_ty, nv)?;
            let ne = loop_uw_rewrite(else_, ef, ev, err_ty, nv)?;
            Some(loop_uw_node(
                IrExprKind::If { cond: cond.clone(), then: Box::new(nt), else_: Box::new(ne) },
                e.ty.clone(),
            ))
        }
        IrExprKind::Match { subject, arms } => {
            if expr_has_unwrap(subject) {
                return None;
            }
            let mut new_arms = Vec::with_capacity(arms.len());
            for a in arms {
                if a.guard.as_ref().is_some_and(expr_has_unwrap) {
                    return None;
                }
                let nb = loop_uw_rewrite(&a.body, ef, ev, err_ty, nv)?;
                new_arms.push(almide_ir::IrMatchArm {
                    pattern: a.pattern.clone(),
                    guard: a.guard.clone(),
                    body: nb,
                });
            }
            Some(loop_uw_node(
                IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                e.ty.clone(),
            ))
        }
        // A bare trailing `e!` (Unit-typed): `match e { ok(_) => (), err($x) => { flag } }`.
        IrExprKind::Unwrap { expr } => {
            use almide_lang::types::constructor::TypeConstructorId;
            if expr_has_unwrap(expr) {
                return None;
            }
            match &expr.ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && &a[1] == err_ty => {}
                _ => return None,
            }
            let ok_arm = almide_ir::IrMatchArm {
                pattern: almide_ir::IrPattern::Ok {
                    inner: Box::new(almide_ir::IrPattern::Wildcard),
                },
                guard: None,
                body: loop_uw_node(IrExprKind::Unit, Ty::Unit),
            };
            let err_arm = loop_uw_err_arm(ef, ev, err_ty, nv);
            Some(loop_uw_node(
                IrExprKind::Match { subject: expr.clone(), arms: vec![ok_arm, err_arm] },
                Ty::Unit,
            ))
        }
        // An `!` in a kind we do not rewrite — decline (fail-safe wall).
        _ => None,
    }
}

/// See the module comment above: rewrite the FIRST `for` loop (in a `Result[T, E]`-returning block)
/// whose body contains an effect-`!` into the loop-carried error-flag form.
pub fn desugar_loop_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // The enclosing result must be `Result[T, E]`. `E` is gated to `String`: the accumulator's
    // owned-copy (`$x ++ ""`, see `loop_uw_err_arm`) and `""` seed are String-specific, and a
    // String error is the effect-fn norm (it covers every porta wall). A non-String `E` declines
    // (the `!` is left to WALL — never a silent miscompile).
    let err_ty = match &body.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String) => {
            a[1].clone()
        }
        _ => return None,
    };
    let empty_err = tco_empty_for(&err_ty)?;
    // The FIRST `for` loop whose body holds an `!`. (`while` is excluded — see the module comment.)
    let loop_idx = stmts.iter().position(|s| match &s.kind {
        IrStmtKind::Expr { expr } => matches!(
            &expr.kind,
            IrExprKind::ForIn { body: lbody, .. } if lbody.iter().any(stmt_has_unwrap)
        ),
        _ => false,
    })?;
    let IrStmtKind::Expr { expr: loop_expr } = &stmts[loop_idx].kind else {
        return None;
    };
    let IrExprKind::ForIn { var, var_tuple, iterable, body: lbody } = &loop_expr.kind else {
        return None;
    };
    let ef = VarId(*next_var);
    let ev = VarId(*next_var + 1);
    *next_var += 2;
    // Rewrite the loop body's `!`s (declining the whole pass if any cannot be cleanly placed).
    let body_block =
        loop_uw_node(IrExprKind::Block { stmts: lbody.clone(), expr: None }, Ty::Unit);
    let rewritten = loop_uw_rewrite(&body_block, ef, ev, &err_ty, next_var)?;
    // Guard the iteration: `if not __ef then { <rewritten> } else ()`.
    let not_ef = loop_uw_node(
        IrExprKind::UnOp {
            op: almide_ir::UnOp::Not,
            operand: Box::new(loop_uw_node(IrExprKind::Var { id: ef }, Ty::Bool)),
        },
        Ty::Bool,
    );
    let guard_if = loop_uw_node(
        IrExprKind::If {
            cond: Box::new(not_ef),
            then: Box::new(rewritten),
            else_: Box::new(loop_uw_node(IrExprKind::Unit, Ty::Unit)),
        },
        Ty::Unit,
    );
    let new_loop = loop_uw_node(
        IrExprKind::ForIn {
            var: *var,
            var_tuple: var_tuple.clone(),
            iterable: iterable.clone(),
            body: vec![IrStmt { kind: IrStmtKind::Expr { expr: guard_if }, span: None }],
        },
        Ty::Unit,
    );
    // `<stmts before loop>; var __ef=false; var __ev=<empty>; <new_loop>`.
    let mut new_stmts: Vec<IrStmt> = stmts[..loop_idx].to_vec();
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ef,
            mutability: almide_ir::Mutability::Var,
            ty: Ty::Bool,
            value: loop_uw_node(IrExprKind::LitBool { value: false }, Ty::Bool),
        },
        span: None,
    });
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind {
            var: ev,
            mutability: almide_ir::Mutability::Var,
            ty: err_ty.clone(),
            value: empty_err,
        },
        span: None,
    });
    new_stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: new_loop }, span: None });
    // Post-loop dispatch: `if __ef then err(__ev) else { <post-stmts>; <orig tail> }`.
    let post = loop_uw_node(
        IrExprKind::Block { stmts: stmts[loop_idx + 1..].to_vec(), expr: tail.clone() },
        body.ty.clone(),
    );
    let err_result = loop_uw_node(
        IrExprKind::ResultErr {
            expr: Box::new(loop_uw_node(IrExprKind::Var { id: ev }, err_ty.clone())),
        },
        body.ty.clone(),
    );
    let new_tail = loop_uw_node(
        IrExprKind::If {
            cond: Box::new(loop_uw_node(IrExprKind::Var { id: ef }, Ty::Bool)),
            then: Box::new(err_result),
            else_: Box::new(post),
        },
        body.ty.clone(),
    );
    Some(loop_uw_node(
        IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_tail)) },
        body.ty.clone(),
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

/// Flatten a scopeless `Block { stmts: [], expr: e }` to `e`, EVERYWHERE it appears (a match-arm body,
/// an `if` branch, a nested block tail). An empty-statement block binds nothing, so it opens no drop
/// scope — it is observationally `e`, but the trust-spine's arm/branch lowering keys on the concrete
/// tail kind (a bare `Match`/`Ok` lowers; a `Block` wrapping it takes a different path that can wall).
/// The desugared derived variant decode (`let _e0 = as_int(..)?; …; ok(Ctor(..))`) leaves one such
/// wrapper per field-bind after `desugar_let_unwrap` rewrites each `?` bind to a match — this collapses
/// them so the nested monadic matches lower like the hand-written form. Run in BOTH the lowering and the
/// `count_ir_calls` gate; an empty block has no calls, so `mir == ir` is unaffected.
pub fn desugar_flatten_empty_block(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, expr: Some(inner) } = &e.kind {
                if stmts.is_empty() {
                    let inner = (**inner).clone();
                    *e = inner;
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// Group the per-constructor arms of an `Option`/`Result` `match` whose `some`/`ok`/`err` arms carry
/// GUARDS or LITERAL payloads into a payload SUB-MATCH — `match x { some(n) if g => A, some(0) => B,
/// some(_) => C, none => D }` becomes `match x { some($p) => match $p { n if g => A, 0 => B, _ => C },
/// none => D }`. The trust-spine lowers the OUTER (variant-tag dispatch, scalar payload bind) and the
/// INNER (scalar guard/literal chain via `build_match_chain`) separately — each proven — but NOT the
/// guarded-VARIANT combination directly (`try_lower_variant_value_match` gates out guards; the
/// heap-result path walls). Regrouping is sound because a variant's constructors are DISJOINT: a
/// `none` arm can never intercept a `some` value, so collecting all `some` arms in order preserves
/// arm order + fall-through byte-for-byte. Runs in BOTH the lowering and the `count_ir_calls` gate.
/// Hoist LITERAL record/tuple STRING-INTERPOLATION parts (`"${(1, \"x\", true)}"`,
/// `"${P{x: 1}}"`) to temp bindings at the enclosing STATEMENT level, so each part
/// becomes a materialized `Var` the EXPAND-fold display can read (a literal part is
/// never a tracked block — `aggregate_part_expandable` requires a Var — so it fell
/// to the unlinked `compound.to_string` wall). `println("${(1, 2)}")` becomes
/// `{ let $t = (1, 2); println("${$t}") }` — the binds are PREPENDED to the
/// statement (a Block in call-arg position would itself wall), and a literal
/// construction is effect-free so the hoist preserves evaluation order. A part the
/// display still cannot expand keeps the same wall it had; the bind rides the
/// ordinary materialized-aggregate ownership (`i` + scope-end `d`).
pub fn desugar_interp_literal_aggregate_hoist(
    body: &IrExpr,
    next_var: &mut u32,
) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, IrStringPart, Mutability, VarId};

    // Rewrite every literal-aggregate interp part INSIDE `e` to a fresh Var,
    // collecting the hoisted binds (in evaluation order).
    fn rewrite_expr(e: &mut IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>, changed: &mut bool) {
        // Do NOT descend into nested Blocks — their own statement lists are the
        // hoist points for their contents (handled by rewrite_block below).
        if matches!(e.kind, IrExprKind::Block { .. }) {
            return;
        }
        if let IrExprKind::StringInterp { parts } = &mut e.kind {
            for p in parts.iter_mut() {
                let IrStringPart::Expr { expr } = p else { continue };
                if !matches!(expr.kind, IrExprKind::Record { .. } | IrExprKind::Tuple { .. }) {
                    continue;
                }
                let tmp = VarId(*next);
                *next += 1;
                binds.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: tmp,
                        mutability: Mutability::Let,
                        ty: expr.ty.clone(),
                        value: expr.clone(),
                    },
                    span: expr.span.clone(),
                });
                *expr = IrExpr {
                    kind: IrExprKind::Var { id: tmp },
                    ty: expr.ty.clone(),
                    span: expr.span.clone(),
                    def_id: None,
                };
                *changed = true;
            }
        }
        // Recurse into children manually (skipping Block, handled above).
        use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
        struct Kids<'a> {
            next: &'a mut u32,
            binds: &'a mut Vec<IrStmt>,
            changed: &'a mut bool,
        }
        impl IrMutVisitor for Kids<'_> {
            fn visit_expr_mut(&mut self, c: &mut IrExpr) {
                rewrite_expr(c, self.next, self.binds, self.changed);
            }
        }
        let mut k = Kids { next, binds, changed };
        walk_expr_mut(&mut k, e);
    }

    fn rewrite_block(e: &mut IrExpr, next: &mut u32, changed: &mut bool) {
        // First recurse structurally so INNER blocks hoist into themselves.
        use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
        struct B<'a> {
            next: &'a mut u32,
            changed: &'a mut bool,
        }
        impl IrMutVisitor for B<'_> {
            fn visit_expr_mut(&mut self, c: &mut IrExpr) {
                if matches!(c.kind, IrExprKind::Block { .. }) {
                    rewrite_block(c, self.next, self.changed);
                } else {
                    walk_expr_mut(self, c);
                }
            }
        }
        let IrExprKind::Block { stmts, expr } = &mut e.kind else { return };
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut st in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut st.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                    rewrite_expr(value, next, &mut binds, changed);
                }
                IrStmtKind::Expr { expr } => {
                    rewrite_expr(expr, next, &mut binds, changed);
                }
                _ => {}
            }
            // Nested blocks inside this statement's exprs hoist into themselves.
            {
                let mut b = B { next, changed };
                match &mut st.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                        b.visit_expr_mut(value)
                    }
                    IrStmtKind::Expr { expr } => b.visit_expr_mut(expr),
                    _ => {}
                }
            }
            out.extend(binds);
            out.push(st);
        }
        *stmts = out;
        if let Some(tail) = expr {
            let mut binds = Vec::new();
            rewrite_expr(tail, next, &mut binds, changed);
            let mut b = B { next, changed };
            b.visit_expr_mut(tail);
            stmts.extend(binds);
        }
    }

    let mut out = body.clone();
    let mut changed = false;
    if matches!(out.kind, IrExprKind::Block { .. }) {
        rewrite_block(&mut out, next_var, &mut changed);
    } else {
        // A non-block body (`fn f() = "${(1, 2)}"`): hoist into a wrapping Block
        // (allowed in tail position).
        let mut binds = Vec::new();
        let mut tail = out.clone();
        rewrite_expr(&mut tail, next_var, &mut binds, &mut changed);
        if changed {
            out = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(tail.clone())) },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            };
        }
    }
    if changed { Some(out) } else { None }
}


/// A `match` over a TUPLE LITERAL of SCALAR components whose every arm is a tuple pattern of
/// scalar literals / binds / wildcards (`match (a, b) { (true, true) => "tt", … }` —
/// bool_pair, the truth-table class) — rewrite to the PROVEN hoist + if-chain form:
///   `{ let $t0 = a; let $t1 = b; if $t0 == true and $t1 == true then <arm0> else if … else
///   <last arm> }`
/// First-match semantics IS the if-chain order; the LAST arm becomes the unconditional else
/// (sound: the frontend enforces exhaustiveness, so a value reaching the last test matches
/// it — v0's own codegen compiles `_` the same way). Components hoist ONCE (evaluation
/// order/count preserved); a Bind component prefixes the arm body (`(x, true) => f(x)` →
/// `{ let x = $t0; f(x) }`). No calls duplicated (mir == ir holds).
pub fn desugar_scalar_tuple_literal_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{BinOp, IrPattern};
    use almide_lang::types::Ty;
    struct V {
        next: u32,
        changed: bool,
    }
    fn admits_arm(p: &IrPattern, n: usize) -> bool {
        matches!(p, IrPattern::Tuple { elements }
            if elements.len() == n
                && elements.iter().all(|c| matches!(c,
                    IrPattern::Wildcard
                        | IrPattern::Bind { .. }
                        | IrPattern::Literal { .. })))
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { subject, arms } = &e.kind else { return };
            let IrExprKind::Tuple { elements } = &subject.kind else { return };
            if elements.is_empty()
                || elements.iter().any(|c| is_heap_ty(&c.ty))
                || arms.len() < 2
                || arms.iter().any(|a| a.guard.is_some())
                || arms.iter().any(|a| !admits_arm(&a.pattern, elements.len()))
            {
                return;
            }
            let span = e.span.clone();
            // Hoist each component ONCE into a fresh scalar temp.
            let mut stmts = Vec::with_capacity(elements.len());
            let mut temp_refs = Vec::with_capacity(elements.len());
            for c in elements {
                let t = VarId(self.next);
                self.next += 1;
                stmts.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: t,
                        ty: c.ty.clone(),
                        value: c.clone(),
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: span.clone(),
                });
                temp_refs.push(IrExpr {
                    kind: IrExprKind::Var { id: t },
                    ty: c.ty.clone(),
                    span: span.clone(),
                    def_id: None,
                });
            }
            // One arm → (condition over the temps, body with bind prefixes).
            let arm_parts: Vec<(Option<IrExpr>, IrExpr)> = arms
                .iter()
                .map(|a| {
                    let IrPattern::Tuple { elements: pats } = &a.pattern else { unreachable!() };
                    let mut cond: Option<IrExpr> = Option::None;
                    let mut binds: Vec<IrStmt> = Vec::new();
                    for (i, pat) in pats.iter().enumerate() {
                        match pat {
                            IrPattern::Literal { expr } => {
                                let eq = IrExpr {
                                    kind: IrExprKind::BinOp {
                                        op: BinOp::Eq,
                                        left: Box::new(temp_refs[i].clone()),
                                        right: Box::new(expr.clone()),
                                    },
                                    ty: Ty::Bool,
                                    span: span.clone(),
                                    def_id: None,
                                };
                                cond = Some(match cond.take() {
                                    Some(c) => IrExpr {
                                        kind: IrExprKind::BinOp {
                                            op: BinOp::And,
                                            left: Box::new(c),
                                            right: Box::new(eq),
                                        },
                                        ty: Ty::Bool,
                                        span: span.clone(),
                                        def_id: None,
                                    },
                                    Option::None => eq,
                                });
                            }
                            IrPattern::Bind { var, ty } => binds.push(IrStmt {
                                kind: IrStmtKind::Bind {
                                    var: *var,
                                    ty: ty.clone(),
                                    value: temp_refs[i].clone(),
                                    mutability: almide_ir::Mutability::Let,
                                },
                                span: span.clone(),
                            }),
                            IrPattern::Wildcard => {}
                            _ => unreachable!(),
                        }
                    }
                    let body_e = if binds.is_empty() {
                        a.body.clone()
                    } else {
                        IrExpr {
                            kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(a.body.clone())) },
                            ty: a.body.ty.clone(),
                            span: span.clone(),
                            def_id: a.body.def_id,
                        }
                    };
                    (cond, body_e)
                })
                .collect();
            // Right-fold into the if-chain; the FIRST unconditional arm (or the last arm)
            // terminates the chain as the else (later arms are unreachable by first-match).
            let mut chain: Option<IrExpr> = Option::None;
            for (cond, body_e) in arm_parts.into_iter().rev() {
                chain = Some(match (cond, chain.take()) {
                    (_, Option::None) | (Option::None, _) => body_e,
                    (Some(c), Some(rest)) => IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(c),
                            then: Box::new(body_e),
                            else_: Box::new(rest),
                        },
                        ty: e.ty.clone(),
                        span: span.clone(),
                        def_id: e.def_id,
                    },
                });
            }
            *e = IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(chain.unwrap())) },
                ty: e.ty.clone(),
                span: span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut v = V { next: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}


/// Rewrite `r?` (`ToOption`) over a `Result[Int, String]` operand into the SELF-HOST bridge
/// call `result.to_option(r)` — a REAL IR Call node, so every position (bind / call-arg /
/// tail) lowers through the proven Module-call machinery and the caps `mir == ir` count sees
/// the call on BOTH sides by construction (desugar-before-both). `result.to_option` is pure
/// (prim reads + an Option ctor), registered, and `is_self_host_option_module_fn`-seeded, so
/// a later `match`/`??` over the bound result reads a real materialized Option. ToOption was
/// previously fully deferred (the strict-value wall) — a pure widening.
pub fn desugar_to_option_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, CallTarget, IrMutVisitor};
    use almide_lang::intern::sym;
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::ToOption { expr } = &e.kind else { return };
            let admits = matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::Result, a)
                    if a.len() == 2 && matches!(a[0], Ty::Int) && matches!(a[1], Ty::String))
                && matches!(&e.ty,
                    Ty::Applied(TypeConstructorId::Option, oa)
                        if oa.len() == 1 && matches!(oa[0], Ty::Int));
            if !admits {
                return;
            }
            e.kind = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("result"),
                    func: sym("to_option"),
                    def_id: None,
                },
                args: vec![(**expr).clone()],
                type_args: Vec::new(),
            };
            self.changed = true;
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

pub fn desugar_grouped_variant_match(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V<'a> {
        next: &'a mut u32,
        changed: bool,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(new_arms) = group_option_result_arms(subject, arms, self.next) {
                    e.kind = IrExprKind::Match {
                        subject: subject.clone(),
                        arms: new_arms,
                    };
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V {
        next: next_var,
        changed: false,
    };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// The grouping transform for [`desugar_grouped_variant_match`]. `None` when the subject is not an
/// `Option`/`Result`, an arm is a top-level catch-all (`_`/binder — not a pure constructor dispatch),
/// a payload pattern is nested (a later brick), or NO arm carries a guard/literal (the plain variant
/// match already lowers — leave it untouched so nothing regresses).
fn group_option_result_arms(
    subject: &IrExpr,
    arms: &[almide_ir::IrMatchArm],
    next_var: &mut u32,
) -> Option<Vec<almide_ir::IrMatchArm>> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    // A constructor "slot" key + its ONE payload's type (None for a nullary ctor). Handles Option
    // (Some/None), Result (Ok/Err), and a SINGLE-FIELD user variant (`Word(String)`); a multi-field
    // ctor, a record-variant, or a nested payload aborts (a later brick).
    #[derive(Clone, PartialEq, Eq)]
    enum CKey {
        Some_,
        None_,
        Ok_,
        Err_,
        User(String),
    }
    // A column pattern the sub-match can re-dispatch on: a scalar leaf (Bind /
    // Literal / Wildcard) or a NESTED user-ctor pattern (`err(Overflow(msg))` —
    // the Result-with-variant-payload class: the inner match over the bound
    // payload var re-dispatches on the variant tag, which the custom-variant
    // machinery lowers once the payload bind is seeded).
    let plain_col =
        |p: &IrPattern| matches!(p, IrPattern::Bind { .. } | IrPattern::Literal { .. } | IrPattern::Wildcard);
    let scalar_col = |p: &IrPattern| {
        plain_col(p)
            || matches!(p, IrPattern::Constructor { args, .. }
                if args.iter().all(plain_col))
            // A nested BUILTIN wrapper (`some(some(n))`, `some(ok(v))`, `some(none)` — the
            // match_exhaustive nested-Option/Result class): the inner match over the bound
            // payload re-dispatches on the wrapper's own len/cap tag, which the ordinary
            // Option/Result machinery lowers once the payload bind is seeded.
            || matches!(p, IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner }
                if plain_col(inner))
            || matches!(p, IrPattern::None)
            // A RECORD-variant pattern (`ok(Tag { name, c })` — the derived-Codec roundtrip
            // class): the inner match re-dispatches the record-variant pattern over the bound
            // payload var — the custom-variant machinery the `describe`-style direct matches
            // already lower. Every named field must carry an explicit plain sub-pattern.
            || matches!(p, IrPattern::RecordPattern { fields, .. }
                if fields.iter().all(|f| matches!(&f.pattern, Some(fp) if plain_col(fp))))
    };
    let is_nested_ctor = |p: &IrPattern| {
        matches!(p,
            IrPattern::Constructor { .. }
                | IrPattern::RecordPattern { .. }
                | IrPattern::Some { .. }
                | IrPattern::None
                | IrPattern::Ok { .. }
                | IrPattern::Err { .. })
    };
    // `(key, field_patterns)` for one arm — `None` (bail) for a top-level catch-all/binder, a
    // record-variant, or a nested column. Field arity: 0 (nullary), 1 (Some/Ok/Err/single-field), or
    // N (a multi-field user ctor `KV(String, Int)` → grouped via a TUPLE payload sub-match).
    let parse = |p: &IrPattern| -> Option<(CKey, Vec<IrPattern>)> {
        match p {
            IrPattern::Some { inner } if scalar_col(inner) => Some((CKey::Some_, vec![(**inner).clone()])),
            IrPattern::None => Some((CKey::None_, vec![])),
            IrPattern::Ok { inner } if scalar_col(inner) => Some((CKey::Ok_, vec![(**inner).clone()])),
            IrPattern::Err { inner } if scalar_col(inner) => Some((CKey::Err_, vec![(**inner).clone()])),
            // A USER-variant subject keeps the STRICT columns: its nested-ctor arms
            // (`Node(Leaf(a), Leaf(b))` then `Node(l, r)` — #610 fall-through
            // refinement) already lower via the custom-variant machinery, and
            // regrouping them here would shadow that working path.
            IrPattern::Constructor { name, args } if args.iter().all(plain_col) => {
                Some((CKey::User(name.clone()), args.clone()))
            }
            _ => Option::None,
        }
    };
    // A TRAILING `_` catch-all (`_ => assert(false)` — the codec-roundtrip class) regroups:
    // its body becomes each multi-arm bucket's inner fallback AND the outer last arm (an
    // `ok(<unmatched ctor>)` value must fall through the INNER match; an `err(_)` through
    // the OUTER). Body duplication is admissible — the count gate reads this same desugared
    // tree on both sides (the tail-duplication precedent). A guarded/binder catch-all bails.
    let (ctor_arms, trailing_wild): (&[IrMatchArm], Option<&IrMatchArm>) = match arms.split_last()
    {
        Some((last, rest))
            if matches!(last.pattern, IrPattern::Wildcard) && last.guard.is_none() =>
        {
            (rest, Some(last))
        }
        _ => (arms, Option::None),
    };
    // Ordered per-ctor buckets (first-occurrence order — the constructors are DISJOINT so outer arm
    // order is immaterial). Each entry: (key, Vec<(field_patterns, guard, body)>).
    let mut groups: Vec<(CKey, Vec<(Vec<IrPattern>, Option<IrExpr>, IrExpr)>)> = Vec::new();
    let mut any_guard_or_lit = false;
    for arm in ctor_arms {
        let (key, fields) = parse(&arm.pattern)?;
        if arm.guard.is_some()
            || fields.iter().any(|p| matches!(p, IrPattern::Literal { .. }))
            || fields.iter().any(is_nested_ctor)
        {
            any_guard_or_lit = true;
        }
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push((fields, arm.guard.clone(), arm.body.clone())),
            Option::None => groups.push((key, vec![(fields, arm.guard.clone(), arm.body.clone())])),
        }
    }
    // Nothing to gain (a plain `some(x)/none` / `Ctor(x)` shape already lowers) — leave untouched.
    if !any_guard_or_lit {
        return Option::None;
    }
    let subject_ty = subject.ty.clone();
    // The type of field `c` of a ctor group: Option/Result from the subject; a user ctor from a
    // Literal (its `expr.ty`) / Bind (its `ty`) in that column across the group's arms.
    let field_ty = |key: &CKey, c: usize, bucket: &[(Vec<IrPattern>, Option<IrExpr>, IrExpr)]| -> Option<Ty> {
        match key {
            CKey::Some_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 => Some(a[0].clone()),
                _ => Option::None,
            },
            CKey::Ok_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[0].clone()),
                _ => Option::None,
            },
            CKey::Err_ => match &subject_ty {
                Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => Some(a[1].clone()),
                _ => Option::None,
            },
            CKey::None_ => Option::None,
            CKey::User(_) => bucket.iter().find_map(|(pats, _, _)| match pats.get(c) {
                Some(IrPattern::Bind { ty, .. }) => Some(ty.clone()),
                Some(IrPattern::Literal { expr }) => Some(expr.ty.clone()),
                _ => Option::None,
            }),
        }
    };
    let rebuild = |key: &CKey, args: Vec<IrPattern>| -> IrPattern {
        match key {
            CKey::Some_ => IrPattern::Some { inner: Box::new(args.into_iter().next().unwrap()) },
            CKey::None_ => IrPattern::None,
            CKey::Ok_ => IrPattern::Ok { inner: Box::new(args.into_iter().next().unwrap()) },
            CKey::Err_ => IrPattern::Err { inner: Box::new(args.into_iter().next().unwrap()) },
            CKey::User(name) => IrPattern::Constructor { name: name.clone(), args },
        }
    };
    let mut new_arms = Vec::with_capacity(groups.len());
    for (key, bucket) in groups {
        let arity = bucket[0].0.len();
        let needs_inner = arity >= 1
            && (bucket.len() > 1
                || bucket.iter().any(|(pats, g, _)| {
                    g.is_some()
                        || pats.iter().any(|p| matches!(p, IrPattern::Literal { .. }))
                        || pats.iter().any(is_nested_ctor)
                }));
        if !needs_inner {
            // A single arm for this ctor (a lone `some(x)`/`none`/`Ctor(a, b)` with no guard/literal)
            // — keep verbatim. A nullary ctor with a guard/duplicate cannot sub-match → bail.
            if bucket.len() != 1 {
                return Option::None;
            }
            let (fields, guard, body) = bucket.into_iter().next().unwrap();
            new_arms.push(IrMatchArm { pattern: rebuild(&key, fields), guard, body });
            continue;
        }
        // Bind each field to a fresh var; the sub-match subject is that var (1 field) or a TUPLE of
        // them (N fields — lowered by `desugar_tuple_match`), and each arm re-matches the fields.
        let mut field_tys = Vec::with_capacity(arity);
        let mut binds = Vec::with_capacity(arity);
        for c in 0..arity {
            let ty = field_ty(&key, c, &bucket)?;
            let v = VarId(*next_var);
            *next_var += 1;
            field_tys.push(ty.clone());
            binds.push((v, ty));
        }
        let sub_subject = if arity == 1 {
            IrExpr {
                kind: IrExprKind::Var { id: binds[0].0 },
                ty: field_tys[0].clone(),
                span: subject.span.clone(),
                def_id: None,
            }
        } else {
            IrExpr {
                kind: IrExprKind::Tuple {
                    elements: binds
                        .iter()
                        .map(|(v, ty)| IrExpr {
                            kind: IrExprKind::Var { id: *v },
                            ty: ty.clone(),
                            span: subject.span.clone(),
                            def_id: None,
                        })
                        .collect(),
                },
                ty: Ty::Tuple(field_tys.clone()),
                span: subject.span.clone(),
                def_id: None,
            }
        };
        let mut inner_arms: Vec<IrMatchArm> = bucket
            .into_iter()
            .map(|(fields, guard, body)| IrMatchArm {
                pattern: if arity == 1 {
                    fields.into_iter().next().unwrap()
                } else {
                    IrPattern::Tuple { elements: fields }
                },
                guard,
                body,
            })
            .collect();
        // The trailing catch-all falls through INTO this ctor's sub-match (an
        // `ok(<other ctor>)` subject must reach it, not vanish).
        if let Some(w) = trailing_wild {
            inner_arms.push(IrMatchArm {
                pattern: IrPattern::Wildcard,
                guard: Option::None,
                body: w.body.clone(),
            });
        }
        let body_ty = inner_arms[0].body.ty.clone();
        let sub = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(sub_subject),
                arms: inner_arms,
            },
            ty: body_ty,
            span: subject.span.clone(),
            def_id: None,
        };
        let ctor_args = binds
            .into_iter()
            .map(|(v, ty)| IrPattern::Bind { var: v, ty })
            .collect();
        new_arms.push(IrMatchArm {
            pattern: rebuild(&key, ctor_args),
            guard: Option::None,
            body: sub,
        });
    }
    if new_arms.is_empty() {
        return Option::None;
    }
    if let Some(w) = trailing_wild {
        new_arms.push(w.clone());
    }
    Some(new_arms)
}

/// Desugar `fan.race` / `fan.any` over a LITERAL thunk list by INLINING each thunk's body — avoiding a
/// `List[funcref]` (unrepresentable in v1) entirely. On wasm the fan combinators are deterministic:
///   `fan.race([() => t0, () => t1, …])`  ≡  `t0`           (the FIRST thunk settles first)
///   `fan.any([() => t0, () => t1, …])`   ≡  `match t0 { ok(v) => ok(v), err(_) => <any of the rest> }`
///                                             (the FIRST Ok in list order; the last thunk's result is
///                                              the fallback if every earlier one errs)
/// Each `t_i` is a no-param lambda whose body is a `Result[T, String]` (an effect fn call). The inlined
/// form is a plain match-over-a-call chain, all in v1's subset. A NON-literal thunk list (`let ts =
/// […]; fan.race(ts)`) has no inlinable bodies → left for the call-site purity wall.
pub fn desugar_fan_race_any(body: &IrExpr, _next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrMatchArm, IrPattern};
    struct V {
        changed: bool,
    }
    // Extract the no-param thunk bodies of a `fan.race`/`fan.any` LITERAL-list call, or `None` if the
    // expr is not such a call (a non-literal thunk list has no inlinable bodies → declines).
    fn fan_bodies(e: &IrExpr, want: &str) -> Option<Vec<IrExpr>> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind
        else {
            return None;
        };
        if module.as_str() != "fan" || func.as_str() != want {
            return None;
        }
        let [arg] = &args[..] else { return None };
        let IrExprKind::List { elements } = &arg.kind else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        let mut bodies = Vec::with_capacity(elements.len());
        for el in elements {
            let IrExprKind::Lambda { params, body, .. } = &el.kind else {
                return None;
            };
            if !params.is_empty() {
                return None;
            }
            bodies.push((**body).clone());
        }
        Some(bodies)
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // PRE-order: `match fan.any([() => t0, …]) { ok(pat) => okbody, err(epat) => errbody }` —
            // INLINE the outer arms into each thunk level (avoiding the intermediate Result + a
            // match-over-match). Each thunk `t_i` runs, an Ok takes `okbody`, an Err falls to the NEXT
            // thunk; the LAST thunk's Err takes the original `errbody` (the all-errored fallback). Only
            // one arm ever executes, so duplicating `okbody` per level is dead-code-safe.
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if arms.len() == 2 {
                    if let Some(bodies) = fan_bodies(subject, "any") {
                        let ty = e.ty.clone();
                        let ok_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Ok { .. }));
                        let err_arm = arms.iter().find(|a| matches!(a.pattern, IrPattern::Err { .. }));
                        if let (Some(ok_arm), Some(err_arm)) = (ok_arm, err_arm) {
                            // The ALL-FAILED fallback is v0's fixed `fan.any: all candidates failed`
                            // Err (NOT the last thunk's own error): run the outer `err` arm's body with
                            // its bound var substituted by that literal (a `Wildcard` err pattern just
                            // runs the body). Then wrap each thunk: `match t_i { ok(pat) => okbody,
                            // err(_) => rest }` — an Ok short-circuits, an Err falls through in order.
                            let msg = IrExpr {
                                kind: IrExprKind::LitStr {
                                    value: "fan.any: all candidates failed".to_string(),
                                },
                                ty: almide_lang::types::Ty::String,
                                span: None,
                                def_id: None,
                            };
                            let mut acc = match &err_arm.pattern {
                                IrPattern::Err { inner } => match &**inner {
                                    IrPattern::Bind { var, .. } => {
                                        almide_ir::substitute_var_in_expr(&err_arm.body, *var, &msg)
                                    }
                                    _ => err_arm.body.clone(),
                                },
                                _ => err_arm.body.clone(),
                            };
                            for tb in bodies.into_iter().rev() {
                                acc = IrExpr {
                                    kind: IrExprKind::Match {
                                        subject: Box::new(tb),
                                        arms: vec![
                                            ok_arm.clone(),
                                            IrMatchArm {
                                                pattern: IrPattern::Err {
                                                    inner: Box::new(IrPattern::Wildcard),
                                                },
                                                guard: None,
                                                body: acc,
                                            },
                                        ],
                                    },
                                    ty: ty.clone(),
                                    span: None,
                                    def_id: None,
                                };
                            }
                            *e = acc;
                            self.changed = true;
                            walk_expr_mut(self, e);
                            return;
                        }
                    }
                }
            }
            walk_expr_mut(self, e);
            // POST-order: `fan.race([() => t0, …])` — the FIRST thunk's body (deterministic head).
            if let Some(bodies) = fan_bodies(e, "race") {
                *e = bodies.into_iter().next().unwrap();
                self.changed = true;
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// Desugar a NON-EMPTY map literal `["k": v, …]` into `map.from_list([(k, v), …])` — the trust-spine
/// materializes a map literal as a DEFERRED-Opaque (empty) block, so a subsequent `map.len` / `map.get`
/// / `map.keys` would SILENTLY read the empty block (v0=2, v1=0 — a miscompile). `map.from_list`
/// builds the REAL map from a `List[(K, V)]` (byte-verified), so routing the literal through it both
/// fixes the miscompile AND opens map-literal usage. v0 is untouched (this is a v1-lowering rewrite).
/// The EMPTY literal `[:]` is already materialized correctly, so it is left alone.
pub fn desugar_map_literal(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::MapLiteral { entries } = &e.kind else { return };
            if entries.is_empty() {
                return;
            }
            let (k_ty, v_ty) = match &e.ty {
                Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => (a[0].clone(), a[1].clone()),
                _ => return,
            };
            let tuple_ty = Ty::Tuple(vec![k_ty, v_ty]);
            let elements: Vec<IrExpr> = entries
                .iter()
                .map(|(k, v)| IrExpr {
                    kind: IrExprKind::Tuple {
                        elements: vec![k.clone(), v.clone()],
                    },
                    ty: tuple_ty.clone(),
                    span: e.span.clone(),
                    def_id: None,
                })
                .collect();
            let list_expr = IrExpr {
                kind: IrExprKind::List { elements },
                ty: Ty::Applied(TypeConstructorId::List, vec![tuple_ty]),
                span: e.span.clone(),
                def_id: None,
            };
            e.kind = IrExprKind::Call {
                target: almide_ir::CallTarget::Module {
                    module: almide_lang::intern::sym("map"),
                    func: almide_lang::intern::sym("from_list"),
                    def_id: None,
                },
                args: vec![list_expr],
                type_args: vec![],
            };
            self.changed = true;
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// Desugar a `match` over a TUPLE subject into element accesses + a linear guard/`if` chain — `match t
/// { ("a", 1) => A, ("a", _) => B, (_, _) => C }` becomes `if t.0 == "a" && t.1 == 1 then A else if
/// t.0 == "a" then B else C`. Each column's LITERAL becomes an `== `-test on `t.<c>`, each BIND is
/// substituted by `t.<c>` in the guard + body, and a trailing all-wildcard/binder arm is the `else`.
/// The trust-spine already lowers tuple index (`t.0`) + the heap-result `if` chain; the TUPLE-pattern
/// match itself was the gap. Requires a pure (`Var`) subject (element re-reads are effect-free) + a
/// trailing catch-all (exhaustiveness); a nested column pattern bails (a later brick).
pub fn desugar_tuple_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if let Some(chain) = rewrite_tuple_match(subject, arms) {
                    *e = chain;
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

fn rewrite_tuple_match(subject: &IrExpr, arms: &[almide_ir::IrMatchArm]) -> Option<IrExpr> {
    use almide_ir::{substitute_var_in_expr, BinOp, IrPattern};
    use almide_lang::types::Ty;
    let Ty::Tuple(elem_tys) = &subject.ty else {
        return None;
    };
    let n = elem_tys.len();
    if n == 0 || arms.is_empty() {
        return None;
    }
    // The column source. A `Var` subject is re-read per column via a side-effect-free `t.<c>` index; a
    // TUPLE LITERAL of pure elements (`match ($a, $b) { .. }` — what a multi-field variant regroup
    // produces) uses each element directly. Any other subject (a call) is left to
    // `desugar_match_subject_hoist` to bind first.
    let pure_elems: Option<Vec<IrExpr>> = match &subject.kind {
        IrExprKind::Tuple { elements }
            if elements.len() == n
                && elements.iter().all(|e| {
                    matches!(
                        &e.kind,
                        IrExprKind::Var { .. }
                            | IrExprKind::LitInt { .. }
                            | IrExprKind::LitBool { .. }
                            | IrExprKind::LitFloat { .. }
                    )
                }) =>
        {
            Some(elements.clone())
        }
        _ => None,
    };
    if pure_elems.is_none() && !matches!(&subject.kind, IrExprKind::Var { .. }) {
        return None;
    }
    let result_ty = arms[0].body.ty.clone();
    // `t.<c>` (Var subject) or the c-th tuple-literal element.
    let elem = |c: usize| match &pure_elems {
        Some(elems) => elems[c].clone(),
        None => IrExpr {
            kind: IrExprKind::TupleIndex {
                object: Box::new(subject.clone()),
                index: c,
            },
            ty: elem_tys[c].clone(),
            span: subject.span.clone(),
            def_id: None,
        },
    };
    // Recursively fold the arms into a right-nested `if`/`else` chain.
    fn build(
        arms: &[almide_ir::IrMatchArm],
        n: usize,
        subject: &IrExpr,
        elem: &dyn Fn(usize) -> IrExpr,
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        let (first, rest) = arms.split_first()?;
        // Build the literal `==` tests and the bind substitution for this arm.
        let mut conds: Vec<IrExpr> = Vec::new();
        let mut subst: Vec<(VarId, IrExpr)> = Vec::new();
        match &first.pattern {
            // A whole-tuple catch-all: `_` binds nothing, a binder maps to the whole subject.
            IrPattern::Wildcard => {}
            IrPattern::Bind { var, .. } => subst.push((*var, subject.clone())),
            // A `(c0, c1, ..)` tuple pattern: each scalar column contributes a test or a bind.
            IrPattern::Tuple { elements } if elements.len() == n => {
                for (c, col) in elements.iter().enumerate() {
                    match col {
                        IrPattern::Literal { expr } => conds.push(IrExpr {
                            kind: IrExprKind::BinOp {
                                op: BinOp::Eq,
                                left: Box::new(elem(c)),
                                right: Box::new(expr.clone()),
                            },
                            ty: Ty::Bool,
                            span: None,
                            def_id: None,
                        }),
                        IrPattern::Bind { var, .. } => subst.push((*var, elem(c))),
                        IrPattern::Wildcard => {}
                        _ => return None, // a nested column — a later brick
                    }
                }
            }
            _ => return None,
        }
        // Apply the bind substitution to the guard + body.
        let apply = |e: &IrExpr| -> IrExpr {
            let mut out = e.clone();
            for (v, rep) in &subst {
                out = substitute_var_in_expr(&out, *v, rep);
            }
            out
        };
        let body = apply(&first.body);
        if let Some(g) = &first.guard {
            conds.push(apply(g));
        }
        if conds.is_empty() {
            // A trivially-true arm (all binds/wildcards, no guard) — the catch-all terminator.
            return Some(body);
        }
        // cond = conds[0] && conds[1] && ...
        let cond = conds
            .into_iter()
            .reduce(|a, b| IrExpr {
                kind: IrExprKind::BinOp {
                    op: BinOp::And,
                    left: Box::new(a),
                    right: Box::new(b),
                },
                ty: Ty::Bool,
                span: None,
                def_id: None,
            })
            .unwrap();
        let else_ = build(rest, n, subject, elem, result_ty)?;
        Some(IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(body),
                else_: Box::new(else_),
            },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        })
    }
    build(arms, n, subject, &elem, &result_ty)
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

/// Is `var`'s ONLY occurrence in `e` the SUBJECT of a VARIANT (Option/Result/ADT) `match` with NO
/// literal-pattern arm? Walks `e` and, for the one `Var(var)`, requires it sits directly under such a
/// `Match { subject }`. A use anywhere else (a match ARM, an arg, an operand) returns false — so the
/// inline below never moves the bound value into a position that would re-evaluate it or change
/// ownership. CRUCIAL: the VARIANT-subject + NO-literal-arm gate keeps this DISJOINT from
/// `desugar_match_subject_hoist`, which deliberately HOISTS a LITERAL-arm match's call subject into a
/// `let` — inlining THAT back would ping-pong the fixpoint forever (a stack overflow). The two desugars
/// own non-overlapping match shapes.
fn sole_use_is_match_subject(e: &IrExpr, var: VarId) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let is_inlinable_variant_match = |arms: &[almide_ir::IrMatchArm], subj_ty: &Ty| -> bool {
        let variant_subject = matches!(subj_ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(subj_ty, Ty::Named(..) | Ty::Variant { .. });
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        variant_subject && !has_literal_arm
    };
    struct C<'a> {
        var: VarId,
        ok: bool,        // the one occurrence is an inlinable-variant-match subject
        bad: bool,       // a non-(inlinable-match-subject) occurrence was seen
        subjects: Vec<usize>, // ptr-identity of subjects of inlinable variant matches
        pred: &'a dyn Fn(&[almide_ir::IrMatchArm], &Ty) -> bool,
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if (self.pred)(arms, &subject.ty) {
                    self.subjects.push(subject.as_ref() as *const IrExpr as usize);
                }
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    if self.subjects.contains(&(e as *const IrExpr as usize)) {
                        self.ok = true;
                    } else {
                        self.bad = true;
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C { var, ok: false, bad: false, subjects: Vec::new(), pred: &is_inlinable_variant_match };
    c.visit_expr(e);
    c.ok && !c.bad
}

/// INLINE a SINGLE-USE let-bound MATCH SUBJECT: `{ …; let p = <expr>; …; match p { … } }` →
/// `{ …; …; match <expr> { … } }` WHEN `p`'s ONLY occurrence (across the remaining block stmts + tail)
/// is that match's subject. This turns a let-bound-Var variant-match subject (`let p = json.get(case,
/// "payload"); match p { some(pl) => …inner flat_map(pf => f(pf, capture))… }`) into the INLINE-subject
/// form the variant-match str-acc handler lowers via C1 (materializes `<expr>` fresh/owned + C1-inlines
/// the inner capturing flat_map) — instead of the let-bound Var routing the inner flat_map to the
/// funcref-dropping C2-lift (which now WALLS, the value-position-HOF guard). The bindgen / wasm-bindgen
/// `gen_pack_variant` / `emit_variant_helpers` value-position-HOF blocker.
///
/// VALUE-PRESERVING + ownership-neutral: `<expr>` is evaluated EXACTLY ONCE either way (the SINGLE-use
/// gate ⇒ no duplicated evaluation, no duplicated allocation), and moving it into the (sole) subject
/// position is exactly what the inline-subject source would have produced — the variant-match handler
/// materializes/owns it identically. STRICT: `p` is a `let` (not `var`), used EXACTLY ONCE, and that one
/// use is the match SUBJECT (`sole_use_is_match_subject`). A multi-use `p`, or a use elsewhere, declines
/// (NO inline — duplicating the value's evaluation would change semantics/ownership). A pure IR→IR
/// rewrite applied desugar-before-both, so the lowering + the `count_ir_calls` caps gate see the same
/// tree (mir == ir by construction — the call moves position, it is not duplicated).
pub fn desugar_inline_single_use_match_subject(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find a `let p = <expr>` whose `p` is used EXACTLY ONCE — as a match subject — in everything that
    // FOLLOWS it (the later stmts + the tail). (A use BEFORE the bind is impossible — `p` is not yet in
    // scope — so counting the suffix is the whole live range.)
    let (i, p, value) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, value, mutability: almide_ir::Mutability::Let, .. } => {
            let rest_stmts = &stmts[i + 1..];
            let uses: usize = rest_stmts.iter().map(|s| count_var_uses_in_stmt(s, *var)).sum::<usize>()
                + tail.as_ref().map(|t| count_var_uses(t, *var)).unwrap_or(0);
            if uses != 1 {
                return None;
            }
            // The sole use must be a match subject in EXACTLY the position it occurs (the rest stmts OR
            // the tail). Check both — exactly one holds (uses == 1).
            let in_rest = rest_stmts.iter().any(|s| stmt_sole_use_is_match_subject(s, *var));
            let in_tail = tail.as_ref().map(|t| sole_use_is_match_subject(t, *var)).unwrap_or(false);
            if in_rest || in_tail {
                Some((i, *var, value.clone()))
            } else {
                None
            }
        }
        _ => None,
    })?;
    // Substitute `p` → `<expr>` in the FOLLOWING stmts + tail, drop the bind. `<expr>` lands exactly in
    // the (sole) match-subject slot.
    let mut new_stmts: Vec<IrStmt> = stmts[..i].to_vec();
    for s in &stmts[i + 1..] {
        new_stmts.push(almide_ir::substitute::substitute_var_in_stmt(s, p, &value));
    }
    let new_tail = tail
        .as_ref()
        .map(|t| Box::new(almide_ir::substitute::substitute_var_in_expr(t, p, &value)));
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: new_tail },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `count_var_uses` over a STATEMENT's value-bearing children (Bind/Assign/Expr/…).
fn count_var_uses_in_stmt(s: &IrStmt, var: VarId) -> usize {
    use almide_ir::visit::{walk_stmt, IrVisitor};
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
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C { var, n: 0 };
    walk_stmt(&mut c, s);
    c.n
}

/// `sole_use_is_match_subject` over a STATEMENT's value-bearing children (same variant-only gate).
fn stmt_sole_use_is_match_subject(s: &IrStmt, var: VarId) -> bool {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let is_inlinable_variant_match = |arms: &[almide_ir::IrMatchArm], subj_ty: &Ty| -> bool {
        let variant_subject = matches!(subj_ty, Ty::Applied(TC::Option | TC::Result, _))
            || matches!(subj_ty, Ty::Named(..) | Ty::Variant { .. });
        let has_literal_arm = arms
            .iter()
            .any(|a| matches!(a.pattern, almide_ir::IrPattern::Literal { .. }));
        variant_subject && !has_literal_arm
    };
    struct C<'a> {
        var: VarId,
        ok: bool,
        bad: bool,
        subjects: Vec<usize>,
        pred: &'a dyn Fn(&[almide_ir::IrMatchArm], &Ty) -> bool,
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Match { subject, arms } = &e.kind {
                if (self.pred)(arms, &subject.ty) {
                    self.subjects.push(subject.as_ref() as *const IrExpr as usize);
                }
            }
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.var {
                    if self.subjects.contains(&(e as *const IrExpr as usize)) {
                        self.ok = true;
                    } else {
                        self.bad = true;
                    }
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut c = C { var, ok: false, bad: false, subjects: Vec::new(), pred: &is_inlinable_variant_match };
    walk_stmt(&mut c, s);
    c.ok && !c.bad
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

/// BETA-REDUCE a DIRECT lambda application (`(λ(p) => body)(arg)` — the pipe-into-
/// lambda projection `fold(...) |> ((pair) => pair.0)`, argmax): rewrite to
/// `{ let p = arg; body }` so the ordinary bind + scalar-field machinery lowers it
/// (the Computed-callee call is otherwise unanalyzable → deferred/walled). Each arg
/// is bound ONCE (no duplication; call-count only DECREASES, so the caps gate's
/// `mir ≤ ir` is preserved). Bottom-up over the whole body; `None` = no change.
pub fn desugar_beta_reduce(body: &IrExpr) -> Option<IrExpr> {
    fn rewrite(e: IrExpr, changed: &mut bool) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed));
        if let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &e.kind {
            if let IrExprKind::Lambda { params, body, .. } = &callee.kind {
                if params.len() == args.len() {
                    *changed = true;
                    let stmts: Vec<almide_ir::IrStmt> = params
                        .iter()
                        .zip(args.iter())
                        .map(|((var, ty), arg)| almide_ir::IrStmt {
                            kind: almide_ir::IrStmtKind::Bind {
                                var: *var,
                                mutability: almide_ir::Mutability::Let,
                                ty: ty.clone(),
                                value: arg.clone(),
                            },
                            span: None,
                        })
                        .collect();
                    return IrExpr {
                        kind: IrExprKind::Block { stmts, expr: Some(body.clone()) },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    };
                }
            }
        }
        e
    }
    let mut changed = false;
    let out = rewrite(body.clone(), &mut changed);
    changed.then_some(out)
}

/// Desugar `opt ?? fallback` over an `Option[<all-scalar tuple>]` (`list.get(xs, k) ??
/// (0.0, 0.0)` — the fft element pick) into `match opt { some($p) => $p, none => fallback }`,
/// which the proven variant-value-match machinery lowers (Option-tuple payload borrow @12,
/// subject dropped after the arms). Without this the UnwrapOr path treats the tuple payload
/// as a SCALAR (an i32 handle in an i64 slot — invalid wasm the engine rejects). Bottom-up;
/// `None` = no change.
pub fn desugar_tuple_unwrap_or(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_scalar_tuple(ty: &Ty) -> bool {
        matches!(ty, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
    }
    fn rewrite(e: IrExpr, changed: &mut bool, next: &mut u32) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed, next));
        // Both surface forms: the `??` operator (UnwrapOr) AND the explicit
        // `option.unwrap_or(opt, fb)` module call (the pipe form).
        let parts: Option<(&IrExpr, &IrExpr)> = match &e.kind {
            IrExprKind::UnwrapOr { expr, fallback } => Some((expr, fallback)),
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "option" && func.as_str() == "unwrap_or"
                    && args.len() == 2 =>
            {
                Some((&args[0], &args[1]))
            }
            _ => None,
        };
        if let Some((expr, fallback)) = parts {
            let is_opt_tuple = matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::Option, a)
                    if a.len() == 1 && is_scalar_tuple(&a[0]));
            if is_opt_tuple {
                *changed = true;
                let p = almide_ir::VarId(*next);
                *next += 1;
                let arms = vec![
                    almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::Some {
                            inner: Box::new(almide_ir::IrPattern::Bind { var: p, ty: e.ty.clone() }),
                        },
                        guard: None,
                        body: IrExpr {
                            kind: IrExprKind::Var { id: p },
                            ty: e.ty.clone(),
                            span: e.span.clone(),
                            def_id: e.def_id,
                        },
                    },
                    almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::None,
                        guard: None,
                        body: fallback.clone(),
                    },
                ];
                return IrExpr {
                    kind: IrExprKind::Match { subject: Box::new(expr.clone()), arms },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
            }
        }
        e
    }
    let mut changed = false;
    let mut next = crate::lower::max_var_id(body) + 1;
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}
