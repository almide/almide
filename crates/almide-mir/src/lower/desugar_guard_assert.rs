// the assert call-subject hoist of desugar_guard.rs, include!-spliced back at module
// level (a code move under the 800-line ceiling the codopsy gate holds this crate to).

/// ASSERT-SUBJECT HOIST (a pre-lowering program pass, shared chain like the
/// block-arg hoist above): a call-bearing operand of a statement- or
/// tail-position `assert` / `assert_eq` / `assert_ne` hoists into a `let`
/// bind spliced before the assert —
///
///   assert_eq(pick(5), ok(5))  ≡  let s = pick(5); assert_eq(s, ok(5))
///
/// This must run HERE, before the never-err/auto-wrap ABI rewrites: a bind is
/// a shape that family rewraps (the raw never-err callee result re-wrapped at
/// the site), while an inline call argument skips it and reaches the metered
/// wasm test leg as an unresolvable `if` condition — the call-bearing
/// linearization wall (the inline_assert probe). Evaluation order is
/// unchanged (left before right, both before the compare); the 2-arg assert
/// MESSAGE is untouched (it must stay lazy, failure-path only). No calls are
/// added or removed — the caps `mir == ir` invariant holds.
pub fn hoist_assert_call_subjects(program: &mut almide_ir::IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarId};

    fn operand_has_call(e: &IrExpr) -> bool {
        let mut found = false;
        struct C<'a> {
            found: &'a mut bool,
        }
        impl<'a> almide_ir::visit::IrVisitor for C<'a> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if matches!(
                    e.kind,
                    IrExprKind::Call { .. }
                        | IrExprKind::RuntimeCall { .. }
                        | IrExprKind::Try { .. }
                        | IrExprKind::Unwrap { .. }
                ) {
                    *self.found = true;
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        almide_ir::visit::IrVisitor::visit_expr(&mut C { found: &mut found }, e);
        found
    }

    /// When `e` is an assert whose comparable operands carry calls, hoist
    /// those operands into fresh binds (returned) and rewrite the args to
    /// read the bound Vars. The Bool condition of 1/2-arg `assert` hoists as
    /// a whole; the message arg (index 1 of 2-arg assert) never hoists.
    /// SCOPE: only RESULT-typed operands — the lifted-effect-call subject that
    /// needs the rewrap (`assert_eq(pick(5), ok(5))`). Other heap operand
    /// classes stay INLINE deliberately: the inline eq path materializes a
    /// fresh call result directly, while a hoisted Var must be in the tracked
    /// read-shape sets — hoisting an `Option[String]` operand REGRESSED
    /// map_higher_order_test onto the linearization wall.
    fn hoist_assert(e: &mut IrExpr, next: &mut u32) -> Vec<IrStmt> {
        use almide_lang::types::constructor::TypeConstructorId;
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut e.kind
        else {
            return Vec::new();
        };
        let hoistable: usize = match (name.as_str(), args.len()) {
            ("assert", 1 | 2) => 1, // the Bool cond only; a 2-arg msg stays lazy
            ("assert_eq" | "assert_ne", 2) => 2,
            _ => return Vec::new(),
        };
        let mut binds = Vec::new();
        for arg in args.iter_mut().take(hoistable) {
            if !matches!(
                arg.ty,
                almide_lang::types::Ty::Applied(TypeConstructorId::Result, _)
            ) || !operand_has_call(arg)
            {
                continue;
            }
            let var = VarId(*next);
            *next += 1;
            let ty = arg.ty.clone();
            let span = arg.span.clone();
            let value = std::mem::replace(
                arg,
                IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
            );
            binds.push(IrStmt {
                kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value },
                span,
            });
        }
        binds
    }

    struct H<'a> {
        next: &'a mut u32,
    }
    impl<'a> IrMutVisitor for H<'a> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Block { stmts, expr } = &mut e.kind else { return };
            let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
            for mut stmt in stmts.drain(..) {
                if let IrStmtKind::Expr { expr } = &mut stmt.kind {
                    out.extend(hoist_assert(expr, self.next));
                }
                out.push(stmt);
            }
            if let Some(tail) = expr.as_deref_mut() {
                out.extend(hoist_assert(tail, self.next));
            }
            *stmts = out;
        }
    }
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let mut next = crate::lower::desugar_var_seed();
        H { next: &mut next }.visit_expr_mut(&mut func.body);
    }
}
