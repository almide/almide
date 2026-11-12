// An `err(..)` arm of a `let`-bound match leaves the fn (#4640), on the
// certified native render. include!-spliced from mod_c_tail.rs.
//
// The checker joins a match arm written `err(e)` as `Never`: in
// `let x = match s { some(v) => v, none => err("no x") }` the second arm is an
// early return of the enclosing fn, and `x` has the other arms' type. That is
// exactly the `!` of the same match with every other arm wrapped in `ok(..)`:
//
// ```text
// let x = match s { p => v, q => err(e) }
//   ≡  let x = (match s { p => ok(v), q => err(e) })!
// let x = match r { ok(v) => v, err(e) => err(e) }
//   ≡  let x = r!
// ```
//
// The bind then takes the one `!` rule (or the `let x = e!` desugars), like a
// `!` the source spelled. A bind whose own type is a Result keeps its `err(..)`
// arm as a value and is left alone; lambda bodies are not entered (an arm
// there leaves the lambda, and the lambda's lowering owns that).

/// Rewrite every `let`-bound match with an `err(..)` arm into its `!` form.
pub fn desugar_err_arm_binds(body: &IrExpr) -> Option<IrExpr> {
    if !crate::lower::owned_tco_slots() {
        return None;
    }
    fn rewrite(e: IrExpr, changed: &mut bool) -> IrExpr {
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            return e;
        }
        let mut e = e.map_children(&mut |c| rewrite(c, changed));
        if let IrExprKind::Block { stmts, .. } = &mut e.kind {
            for s in stmts.iter_mut() {
                let IrStmtKind::Bind { ty, value, .. } = &mut s.kind else { continue };
                let IrExprKind::Match { subject, arms } = &value.kind else { continue };
                if let Some(bang) = err_arms_as_bang(subject, arms, ty) {
                    *value = bang;
                    *changed = true;
                }
            }
        }
        e
    }
    let mut changed = false;
    let out = rewrite(body.clone(), &mut changed);
    changed.then_some(out)
}

/// The `!` form of a match bound at type `ty`, or `None` when no arm is a bare
/// `err(..)` or the match is itself Result-valued.
fn err_arms_as_bang(subject: &IrExpr, arms: &[almide_ir::IrMatchArm], ty: &Ty) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    // A Result-valued match yields its `err(..)` arm as a value.
    if ty.is_result() {
        return None;
    }
    let err_ty = arms.iter().find_map(|a| match &a.body.kind {
        IrExprKind::ResultErr { expr } => Some(expr.ty.clone()),
        _ => None,
    })?;
    let res_ty = Ty::Applied(TypeConstructorId::Result, vec![ty.clone(), err_ty].into());
    let span = subject.span;
    // `match r { ok(x) => x, err(e) => err(e) }` is `r!` itself.
    if subject.ty == res_ty && arms.len() == 2 && arms.iter().all(|a| a.guard.is_none() && passes_through(a)) {
        return Some(IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(subject.clone()) }, ty: ty.clone(), span, def_id: None });
    }
    let arms = arms
        .iter()
        .map(|a| {
            let kind = match &a.body.kind {
                IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: expr.clone() },
                _ => IrExprKind::ResultOk { expr: Box::new(a.body.clone()) },
            };
            let body = IrExpr { kind, ty: res_ty.clone(), span: a.body.span, def_id: None };
            almide_ir::IrMatchArm { pattern: a.pattern.clone(), guard: a.guard.clone(), body }
        })
        .collect();
    let matched = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(subject.clone()), arms },
        ty: res_ty,
        span,
        def_id: None,
    };
    Some(IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(matched) }, ty: ty.clone(), span, def_id: None })
}

/// `ok(x) => x` or `err(e) => err(e)`: the arm hands its payload on unchanged.
fn passes_through(arm: &almide_ir::IrMatchArm) -> bool {
    use almide_ir::IrPattern;
    let is_var = |e: &IrExpr, v: &almide_ir::VarId| matches!(e.kind, IrExprKind::Var { id } if id == *v);
    match (&arm.pattern, &arm.body.kind) {
        (IrPattern::Ok { inner }, _) => matches!(inner.as_ref(), IrPattern::Bind { var, .. } if is_var(&arm.body, var)),
        (IrPattern::Err { inner }, IrExprKind::ResultErr { expr }) => {
            matches!(inner.as_ref(), IrPattern::Bind { var, .. } if is_var(expr, var))
        }
        _ => false,
    }
}
