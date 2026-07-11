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

