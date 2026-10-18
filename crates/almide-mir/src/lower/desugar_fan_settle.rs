// The fan block's arm classification and its sequential settle (C-199,
// ADR-0024; the record form C-386, #1388). `include!`d by mod_c_tail.rs after
// desugar_fan.rs; it shares that module's scope and imports.

/// One fan arm as the v1 settle sees it: a RAW value (bound once, used as is),
/// a REAL Result whose `!` the join runs after every arm (C-199), or a SPLIT
/// scoped thunk — its `pre` statements run in the arm's turn, its `post`
/// statements (the arm's own `!`s, then inert binds) in the arm's join turn,
/// and `value` is the arm's Ok value.
enum FanElem {
    Plain(IrExpr),
    Unwrap(IrExpr, Ty),
    Split { pre: Vec<IrStmt>, post: Vec<IrStmt>, value: IrExpr },
}

/// Classify a fan arm, or `None` to decline (the function stays honestly walled).
/// An `ok(e)` element (the frontend's lift of an infallible-at-IR call, e.g. an
/// `@extern` effect hook) can never be the first Err, so its `!` IS `e`. A
/// scoped `e!` arm over a REAL Result call (the record form's `name: f(x)!`,
/// #1388) is a REAL Result the join unwraps — its `!` must not run before the
/// later arms do. An in-place `() => ok(body)` thunk arm (#3462 — an arm whose
/// body holds a nested fan) splits (`fan_split_thunk`). Count-invariant: each
/// arm appears once.
fn fan_classify_arm(x: &IrExpr) -> Option<FanElem> {
    if let Some(inner) = fan_ok_inner(x) {
        return Some(FanElem::Plain(inner));
    }
    if let IrExprKind::Unwrap { expr: inner } = &x.kind {
        if let Some(t) = fan_real_result_ok_ty(inner) {
            return Some(FanElem::Unwrap((**inner).clone(), t));
        }
    }
    if let Some(t) = fan_phantom_ok_ty(x) {
        let mut nx = x.clone();
        nx.ty = t;
        return Some(FanElem::Plain(nx));
    }
    if let Some(t) = fan_real_result_ok_ty(x) {
        return Some(FanElem::Unwrap(x.clone(), t));
    }
    let (body, _) = fan_thunk_body(x)?;
    fan_split_thunk(body)
}

/// Split a scoped thunk's body `{ pre…; post…; ok(value) }` at its first `!`.
/// Sound when every `post` statement is `let v = $r!` over a variable or an
/// inert bind, and `value` is inert (variables, literals, tuples and records
/// of them): then the thunk's observable work is all in `pre`, which runs in
/// the arm's turn, and its Err is the first failing `post` `!` — which the
/// join reaches in the arm's turn, after every earlier arm's `!`, so the
/// lowest-index Err in source order still wins. A nested fan arm (its settle
/// block, flattened) is exactly this shape. Anything else declines.
fn fan_split_thunk(body: &IrExpr) -> Option<FanElem> {
    let (stmts, tail) = fan_thunk_stmts(body)?;
    let IrExprKind::ResultOk { expr: value } = &tail.kind else { return None };
    let err_ty = body.ty.result_err_ty()?;
    let cut = stmts.iter().position(stmt_has_unwrap).unwrap_or(stmts.len());
    let post = stmts[cut..].to_vec();
    let post_ok = post.iter().all(|s| fan_post_stmt_ok(s, &err_ty));
    (post_ok && fan_inert(value)).then(|| FanElem::Split { pre: stmts[..cut].to_vec(), post, value: (**value).clone() })
}

/// The thunk body as flat statements and a tail: `ok({ s…; e })` reads as
/// `{ s…; ok(e) }`, and a bind whose value is a block holding a `!` is
/// flattened into the block's statements (VarIds are shadowing-free).
fn fan_thunk_stmts(body: &IrExpr) -> Option<(Vec<IrStmt>, IrExpr)> {
    let (stmts, tail) = match &body.kind {
        IrExprKind::Block { stmts, expr: Some(t) } => (stmts.clone(), (**t).clone()),
        IrExprKind::ResultOk { expr } => match &expr.kind {
            IrExprKind::Block { stmts, expr: Some(t) } => {
                let ok = IrExprKind::ResultOk { expr: t.clone() };
                (stmts.clone(), IrExpr { kind: ok, ..body.clone() })
            }
            _ => (Vec::new(), body.clone()),
        },
        _ => return None,
    };
    let mut flat = Vec::with_capacity(stmts.len());
    let mut todo: std::collections::VecDeque<IrStmt> = stmts.into();
    while let Some(s) = todo.pop_front() {
        match fan_flatten_block_bind(&s) {
            Some(parts) => parts.into_iter().rev().for_each(|p| todo.push_front(p)),
            None => flat.push(s),
        }
    }
    Some((flat, tail))
}

/// `let p = { s…; e }` with a `!` inside → `s…; let p = e`.
fn fan_flatten_block_bind(s: &IrStmt) -> Option<Vec<IrStmt>> {
    let mut kind = s.kind.clone();
    let value = match &mut kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => value,
        _ => return None,
    };
    let IrExprKind::Block { stmts, expr: Some(tail) } = &value.kind else { return None };
    if !expr_has_unwrap(value) {
        return None;
    }
    let mut out = stmts.clone();
    *value = (**tail).clone();
    out.push(IrStmt { kind, span: s.span.clone() });
    Some(out)
}

/// A `post` statement: `let v = $r!` whose Err is the thunk's, or an inert bind.
fn fan_post_stmt_ok(s: &IrStmt, err_ty: &Ty) -> bool {
    match &s.kind {
        IrStmtKind::Bind { value, .. } => match &value.kind {
            IrExprKind::Unwrap { expr } => {
                matches!(expr.kind, IrExprKind::Var { .. }) && expr.ty.result_err_ty().as_ref() == Some(err_ty)
            }
            _ => fan_inert(value),
        },
        IrStmtKind::BindDestructure { value, .. } => fan_inert(value),
        _ => false,
    }
}

/// Variables, literals, tuples / records of them, and operators that cannot
/// trap (no division, remainder or power): no effect, so evaluating them in
/// the arm's join turn instead of its run turn is unobservable.
fn fan_inert(e: &IrExpr) -> bool {
    use almide_ir::BinOp as B;
    match &e.kind {
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } => true,
        IrExprKind::Tuple { elements } => elements.iter().all(fan_inert),
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| fan_inert(v)),
        IrExprKind::UnOp { operand, .. } => fan_inert(operand),
        IrExprKind::BinOp { op, left, right } => {
            let traps = matches!(op, B::DivInt | B::DivFloat | B::ModInt | B::ModFloat | B::PowInt | B::PowFloat)
                || matches!(op, B::MulMatrix | B::AddMatrix | B::SubMatrix | B::ScaleMatrix);
            !traps && fan_inert(left) && fan_inert(right)
        }
        _ => false,
    }
}

/// The mixed / real-Result fan block (C-199, ADR-0024): `fan { e1; e2 } ≡
/// { let $r1 = e1; let $r2 = e2; let $v1 = $r1!; let $v2 = $r2!; ($v1, $v2) }`.
/// Every arm runs in list order BEFORE any `!`, and the `!`s run in list order,
/// so the lowest-index Err wins. A Plain arm is raw (bound once, used as is); a
/// Split arm contributes its `pre` to the first phase and its `post` to the
/// second. The existing bind-`!` machinery lowers each unwrap. Count-invariant.
fn fan_settle_block(arms: Vec<FanElem>, fan: &IrExpr, next_var: &mut u32) -> IrExpr {
    use almide_ir::{Mutability, VarId};
    let mut fresh = |ty: &Ty, value: IrExpr, stmts: &mut Vec<IrStmt>| -> IrExpr {
        let var = VarId(*next_var);
        *next_var += 1;
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: ty.clone(), value },
            span: None,
        });
        IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: None, def_id: None }
    };
    let mut stmts: Vec<IrStmt> = Vec::with_capacity(arms.len() * 2);
    let held: Vec<FanElem> = arms
        .into_iter()
        .map(|a| match a {
            FanElem::Plain(x) => FanElem::Plain(fresh(&x.ty.clone(), x, &mut stmts)),
            FanElem::Unwrap(x, t) => FanElem::Unwrap(fresh(&x.ty.clone(), x, &mut stmts), t),
            FanElem::Split { pre, post, value } => {
                stmts.extend(pre);
                FanElem::Split { pre: Vec::new(), post, value }
            }
        })
        .collect();
    let elements: Vec<IrExpr> = held
        .into_iter()
        .map(|a| match a {
            FanElem::Plain(r) => r,
            FanElem::Unwrap(r, t) => {
                let span = fan.span.clone();
                let unwrap = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(r) }, ty: t.clone(), span, def_id: None };
                fresh(&t, unwrap, &mut stmts)
            }
            FanElem::Split { post, value, .. } => {
                stmts.extend(post);
                value
            }
        })
        .collect();
    let tuple = IrExpr { kind: IrExprKind::Tuple { elements }, ty: fan.ty.clone(), span: fan.span.clone(), def_id: fan.def_id };
    IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(tuple)) }, ty: fan.ty.clone(), span: fan.span.clone(), def_id: fan.def_id }
}

/// A single-arm fan IS its arm: the raw value, the arm's `!`, or a split
/// thunk's statements in order with its value as the tail.
fn fan_single_arm(arm: FanElem, fan: &IrExpr) -> IrExpr {
    let kind = match arm {
        FanElem::Plain(nx) => return nx,
        FanElem::Unwrap(r, _) => IrExprKind::Unwrap { expr: Box::new(r) },
        FanElem::Split { mut pre, post, value } => {
            pre.extend(post);
            IrExprKind::Block { stmts: pre, expr: Some(Box::new(value)) }
        }
    };
    IrExpr { kind, ty: fan.ty.clone(), span: fan.span.clone(), def_id: fan.def_id }
}
