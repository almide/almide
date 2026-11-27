// ── part of control.rs, include!-spliced (split for the 800-line budget) ──

impl LowerCtx {
    /// Desugar a `match subj { lit => body, …, _ => body }` to a nested `if subj == lit
    /// then body else …` IrExpr — so it EXECUTES via the if machinery (only the matched
    /// arm runs). `subj` is cloned into each `==`; a Var resolves to the same ValueId
    /// (no re-eval), and a non-scalar-lowerable subject makes the cond fail → the caller
    /// falls back to linearization. Returns `None` for non-literal patterns / guards /
    /// a non-exhaustive literal list (the linearization handles those).
    ///
    /// Handled SCALAR-subject shapes:
    /// - INT LITERAL arms + a trailing wildcard/binder catch-all;
    /// - a BOOL subject `match b { true => A, false => B }` (exhaustive over `{true,false}`
    ///   with no wildcard) → `if b then A else B`, where the `true`/`false` arms may appear
    ///   in either order;
    /// - a BINDER catch-all `x => body`, which BINDS `x` to the subject (a `let x = subj`
    ///   wrapped around `body`) so the arm body's references to `x` resolve — without the
    ///   bind, `x` would lower to a deferred 0 and the whole match silently miscompile.
    pub(crate) fn desugar_match_to_if(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        if arms.is_empty() {
            return None;
        }
        // A BOOL subject is exhaustive over `{true, false}` WITHOUT a wildcard: the literal
        // chain below would run off the end (`build_match_chain([])` → None). Desugar the
        // canonical 2-arm form `match b { true => A, false => B }` to `if b then A else B`
        // directly (arms in either order); other Bool shapes (a single wildcard/binder arm)
        // fall through to the generic chain.
        if matches!(subject.ty, Ty::Bool) {
            if let Some(if_expr) = self.desugar_bool_match(subject, arms, result_ty) {
                return Some(if_expr);
            }
        }
        self.build_match_chain(subject, arms, result_ty)
    }

    /// A 2-arm `match b { true => A, false => B }` (arms in either order, no guards) →
    /// `if b then A else B`. Returns `None` if the shape is not exactly the two Bool
    /// literals (e.g. a wildcard arm) — the caller then falls to `build_match_chain`.
    fn desugar_bool_match(
        &self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result_ty: &Ty,
    ) -> Option<IrExpr> {
        if arms.len() != 2 {
            return None;
        }
        let bool_lit = |arm: &IrMatchArm| -> Option<bool> {
            // A GUARDED bool arm (`true if g => ..`) is NOT an unconditional `true`; decline so
            // it falls to `build_match_chain`, which folds the guard into the condition.
            if arm.guard.is_some() {
                return None;
            }
            match &arm.pattern {
                IrPattern::Literal { expr } => match &expr.kind {
                    IrExprKind::LitBool { value } => Some(*value),
                    _ => None,
                },
                _ => None,
            }
        };
        let (b0, b1) = (bool_lit(&arms[0])?, bool_lit(&arms[1])?);
        // Must be exactly one `true` arm and one `false` arm.
        if b0 == b1 {
            return None;
        }
        let (then_arm, else_arm) = if b0 { (&arms[0], &arms[1]) } else { (&arms[1], &arms[0]) };
        Some(IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(subject.clone()),
                then: Box::new(then_arm.body.clone()),
                else_: Box::new(else_arm.body.clone()),
            },
            ty: result_ty.clone(),
            span: None,
            def_id: None,
        })
    }
}
