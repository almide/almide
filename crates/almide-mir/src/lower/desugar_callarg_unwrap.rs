// Included by desugar_branch.rs: ordered ANF for explicit propagation.
// The prefix evaluates earlier operands before the extracted unwrap. Keeping
// those binds in the SAME enclosing block preserves scope and snapshots even
// when the unwrap's call mutates a global read by an earlier operand.
type CallargUnwrap = (Vec<IrStmt>, IrExpr, IrExpr);

fn take_unwrap_or_recurse(child: &IrExpr, tmp: VarId, nv: &mut u32) -> Option<CallargUnwrap> {
    if matches!(child.kind, IrExprKind::Unwrap { .. } | IrExprKind::Try { .. }) {
        let var = IrExpr { kind: IrExprKind::Var { id: tmp }, ty: child.ty.clone(), span: child.span, def_id: None };
        return Some((vec![], child.clone(), var));
    }
    extract_first_callarg_unwrap(child, tmp, nv)
}

/// Children are unconditional, evaluated left to right. Capture each earlier
/// value, then append the nested prefix and let the caller bind the unwrap.
fn ordered_unwrap_children(children: &mut [IrExpr], tmp: VarId, nv: &mut u32) -> Option<(Vec<IrStmt>, IrExpr)> {
    for i in 0..children.len() {
        let Some((nested, value, rest)) = take_unwrap_or_recurse(&children[i], tmp, nv) else { continue };
        let mut prefix = Vec::new();
        for prior in &mut children[..i] {
            let var = VarId(*nv); *nv += 1;
            prefix.push(IrStmt { kind: IrStmtKind::Bind {
                var, mutability: almide_ir::Mutability::Let, ty: prior.ty.clone(), value: prior.clone(),
            }, span: prior.span });
            *prior = IrExpr { kind: IrExprKind::Var { id: var }, ty: prior.ty.clone(), span: prior.span, def_id: None };
        }
        prefix.extend(nested);
        children[i] = rest;
        return Some((prefix, value));
    }
    None
}

/// Extract only unconditionally evaluated children; branch arms, lazy
/// fallbacks and the right operand of and/or remain in their own scopes.
fn extract_first_callarg_unwrap(e: &IrExpr, tmp: VarId, nv: &mut u32) -> Option<CallargUnwrap> {
    let mut out = e.clone();
    let (prefix, value) = match &mut out.kind {
        IrExprKind::Call { target, args, .. } => {
            let callee = match target {
                almide_ir::CallTarget::Computed { callee } => Some(callee),
                almide_ir::CallTarget::Method { object, .. } => Some(object),
                _ => None,
            };
            let mut children: Vec<_> = callee.as_ref().map(|c| vec![(***c).clone()]).unwrap_or_default();
            children.extend(args.iter().cloned());
            let lifted = ordered_unwrap_children(&mut children, tmp, nv)?;
            if let Some(callee) = callee { **callee = children.remove(0); }
            *args = children;
            lifted
        }
        IrExprKind::Tuple { elements } | IrExprKind::List { elements } => ordered_unwrap_children(elements, tmp, nv)?,
        IrExprKind::StringInterp { parts } => {
            let mut children: Vec<_> = parts.iter().filter_map(|p| match p {
                almide_ir::IrStringPart::Expr { expr } => Some(expr.clone()), _ => None,
            }).collect();
            let lifted = ordered_unwrap_children(&mut children, tmp, nv)?;
            let mut changed = children.into_iter();
            for p in parts { if let almide_ir::IrStringPart::Expr { expr } = p { *expr = changed.next().expect("one rewritten child per Expr part, in order"); } }
            lifted
        }
        IrExprKind::BinOp { op, left, right } => {
            if matches!(op, almide_ir::BinOp::And | almide_ir::BinOp::Or) {
                let (prefix, value, rest) = take_unwrap_or_recurse(left, tmp, nv)?;
                **left = rest;
                (prefix, value)
            } else {
                let mut children = [(**left).clone(), (**right).clone()];
                let lifted = ordered_unwrap_children(&mut children, tmp, nv)?;
                let [a,b] = children; **left = a; **right = b;
                lifted
            }
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            let mut children = [(**object).clone(), (**index).clone()];
            let lifted = ordered_unwrap_children(&mut children, tmp, nv)?;
            let [a,b] = children; **object = a; **index = b;
            lifted
        }
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
            // Do not extract the outer unwrap itself; its bind-position
            // propagation belongs to desugar_let_unwrap.
            let (prefix, value, rest) = extract_first_callarg_unwrap(expr, tmp, nv)?;
            **expr = rest;
            (prefix, value)
        }
        IrExprKind::UnOp { operand: child, .. }
        | IrExprKind::ResultOk { expr: child } | IrExprKind::ResultErr { expr: child }
        | IrExprKind::OptionSome { expr: child } | IrExprKind::UnwrapOr { expr: child, .. }
        | IrExprKind::If { cond: child, .. } | IrExprKind::Match { subject: child, .. }
        | IrExprKind::Member { object: child, .. } | IrExprKind::TupleIndex { object: child, .. } => {
            let (prefix, value, rest) = take_unwrap_or_recurse(child, tmp, nv)?;
            **child = rest;
            (prefix, value)
        }
        _ => return None,
    };
    Some((prefix, value, out))
}

/// ANF-LIFT an unwrap-`!` out of a CALL-ARGUMENT / operand / ctor-argument into a fresh `let tmp = e!`
/// so the existing [`desugar_let_unwrap`] then makes it lower (the `?`-early-return). The structural
/// twin of [`desugar_callarg_heap_if`] for `Unwrap` instead of a heap branch. Rewrites the FIRST
/// `f(a(), g(x)!, ..)` to `let a = a(); let tmp = g(x)!; f(a, tmp, ..)`,
/// including nested expressions and block tails. `None` if no unwrap is found. In BOTH the lowering and the `count_ir_calls` gate via [`desugar_heap_branches`].
pub fn desugar_callarg_unwrap(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        // A BARE expression body (`effect fn g(..) = ok(h(t,p)! + 1)`) — lift the unwrap into a fresh
        // block `{ let tmp = h(t,p)!; <body'> }` (the same shape desugar_callarg_heap_if's bare case uses).
        let tmp = VarId(*next_var);
        let mut nv = *next_var + 1;
        let (mut prefix, unwrap, new_body) = extract_first_callarg_unwrap(body, tmp, &mut nv)?;
        *next_var = nv;
        let lift = IrStmt {
            kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
            span: body.span,
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: { prefix.push(lift); prefix }, expr: Some(Box::new(new_body)) },
            ty: body.ty.clone(),
            span: body.span,
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
        let mut nv = *next_var + 1;
        let Some((prefix, unwrap, new_v)) = extract_first_callarg_unwrap(v, tmp, &mut nv) else { continue };
        *next_var = nv;
        let lift = IrStmt {
            kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
            span: s.span,
        };
        let new_stmt = IrStmt {
            kind: match &s.kind {
                IrStmtKind::Expr { .. } => IrStmtKind::Expr { expr: new_v },
                IrStmtKind::Bind { var, mutability, ty, .. } => IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: new_v },
                IrStmtKind::Assign { var, .. } => IrStmtKind::Assign { var: *var, value: new_v },
                other => other.clone(),
            },
            span: s.span,
        };
        let mut new_stmts: Vec<IrStmt> = stmts[..i].to_vec();
        new_stmts.extend(prefix);
        new_stmts.push(lift);
        new_stmts.push(new_stmt);
        new_stmts.extend(stmts[i + 1..].iter().cloned());
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
            ty: body.ty.clone(),
            span: body.span,
            def_id: body.def_id,
        });
    }
    if let Some(t) = tail.as_deref() {
        let mut nv = *next_var + 1;
        if let Some((prefix, unwrap, new_t)) = extract_first_callarg_unwrap(t, tmp, &mut nv) {
            *next_var = nv;
            let lift = IrStmt {
                kind: IrStmtKind::Bind { var: tmp, mutability: almide_ir::Mutability::Let, ty: unwrap.ty.clone(), value: unwrap },
                span: t.span,
            };
            let mut new_stmts = stmts.clone();
            new_stmts.extend(prefix);
            new_stmts.push(lift);
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_t)) },
                ty: body.ty.clone(),
                span: body.span,
                def_id: body.def_id,
            });
        }
    }
    None
}

