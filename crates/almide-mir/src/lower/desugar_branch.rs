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
        // An `if` COND / `match` SUBJECT is evaluated unconditionally FIRST, so an unwrap
        // there lifts soundly (the desugared assert shape `if f(x)! == 42 then () else die`
        // reaches its unwrap through the cond). ARMS are conditional — never descended.
        IrExprKind::If { cond, then, else_ } => take_or_recurse(cond, tmp).map(|(u, nc)| {
            (u, mk(IrExprKind::If { cond: Box::new(nc), then: then.clone(), else_: else_.clone() }))
        }),
        IrExprKind::Match { subject, arms } => take_or_recurse(subject, tmp).map(|(u, ns)| {
            (u, mk(IrExprKind::Match { subject: Box::new(ns), arms: arms.clone() }))
        }),
        // Field/element access objects (`f(x)!.field`, `xs[g()!]`) and the `??` operand —
        // all unconditionally evaluated subpositions (the `??` FALLBACK is conditional).
        IrExprKind::Member { object, field } => take_or_recurse(object, tmp).map(|(u, no)| {
            (u, mk(IrExprKind::Member { object: Box::new(no), field: *field }))
        }),
        IrExprKind::TupleIndex { object, index } => take_or_recurse(object, tmp).map(|(u, no)| {
            (u, mk(IrExprKind::TupleIndex { object: Box::new(no), index: *index }))
        }),
        IrExprKind::IndexAccess { object, index } => {
            if let Some((u, no)) = take_or_recurse(object, tmp) {
                return Some((u, mk(IrExprKind::IndexAccess { object: Box::new(no), index: index.clone() })));
            }
            take_or_recurse(index, tmp).map(|(u, ni)| {
                (u, mk(IrExprKind::IndexAccess { object: object.clone(), index: Box::new(ni) }))
            })
        }
        IrExprKind::MapAccess { object, key } => {
            if let Some((u, no)) = take_or_recurse(object, tmp) {
                return Some((u, mk(IrExprKind::MapAccess { object: Box::new(no), key: key.clone() })));
            }
            take_or_recurse(key, tmp).map(|(u, nk)| {
                (u, mk(IrExprKind::MapAccess { object: object.clone(), key: Box::new(nk) }))
            })
        }
        IrExprKind::UnwrapOr { expr, fallback } => take_or_recurse(expr, tmp).map(|(u, ne)| {
            (u, mk(IrExprKind::UnwrapOr { expr: Box::new(ne), fallback: fallback.clone() }))
        }),
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

/// Apply the call-arg ANF-lift ([`desugar_callarg_heap_if`]) and the heap-branch tail-duplication
/// ([`desugar_let_bound_heap_branch`]) repeatedly to a FIXPOINT — the exact rewrite sequence
/// `lower_body_into` performs before lowering. Both the lowering and the `count_ir_calls` caps gate
/// call this, so the duplicated calls are counted 1:1 (mir == ir) regardless of how many branches
/// a body lifts. Returns `None` if the body is already in normal form (no rewrite applied).
pub fn desugar_heap_branches(
    body: &IrExpr,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    // Seed a FUNCTION-WIDE fresh-VarId counter ABOVE every id in the whole body, then thread it through
    // the recursion so a lift inside one `if` arm never reuses an id live in a SIBLING arm (block_line's
    // `string.drop` read the then-arm's concat because an arm-local `max_var_id` aliased `line`).
    let mut next_var = max_var_id(body) + 1;
    let rewritten = desugar_heap_branches_inner(body, &mut next_var, layouts)?;
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

fn desugar_heap_branches_inner(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
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
        if let Some(r) = desugar_grouped_variant_match(src, next_var, layouts) {
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
        // Compile a tuple-of-VARIANTS match while it is still a VALUE match (binder-free
        // literal arms) — AFTER the call-arg lift above has pulled it out of an argument
        // position (`println(match (Red, Green) {…})` → `let tmp = match …; println(tmp)`,
        // the r5 in-arg shape) but BEFORE the let-bound tail-duplication below pushes
        // `let tmp = …; <rest>` continuations into its arms (duplicated binder-carrying
        // bodies the column compilers must decline). Both also run in the outer chains
        // (idempotent there).
        if let Some(r) = desugar_tuple_variant_match(src) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_tuple_variant_match_deep(src, layouts) {
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
        // `break` inside a `for`/`while` body → the `__bk` flag form (whole-arm breaks only;
        // see `desugar_loop_break`). Runs in this SHARED desugar (count-invariant flag ops).
        if let Some(r) = desugar_loop_break(src, next_var) {
            cur = Some(r);
            continue;
        }
        // A UNIT `if` conditionally reassigning ONE heap var → SSA-ify to a let-bound
        // value-`if` (the lp5 wrong-value class; see `desugar_unit_if_heap_reassign`).
        if let Some(r) = desugar_unit_if_heap_reassign(src, next_var) {
            cur = Some(r);
            continue;
        }
        // STATEMENT-CONTROL continuation-lift: a UNIT `if`/`match` STATEMENT carrying a stmt/let `!`
        // followed by a non-empty continuation. Lift `after` into each arm (tail-duplication) so the
        // branch becomes the block TAIL — the tail effect-unwrap then resolves the `!`. Runs in this
        // SHARED desugar so the duplicated `after` is counted 1:1 by the caps gate (mir == ir).
        if let Some(r) = desugar_stmt_control_unwrap(src, layouts) {
            cur = Some(r);
            continue;
        }
        if let Some(r) = desugar_nested_branch_arms(src, next_var, layouts) {
            cur = Some(r);
            continue;
        }
        return cur;
    }
}
