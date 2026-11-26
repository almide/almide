// Included by mod_b_tail.rs (the pre-lower desugar chain's passes).

/// #3144 on the native render: a `Never`-typed operand in a VALUE position —
/// `g() + panic(m)`, `"${f()} ${process.exit(3)}"`, `add(g(), die(m))`,
/// `[g(), bail(4)!]`, `let n: Int = panic(m)` — runs the operands to its left,
/// left to right (C-192), then aborts at it (C-219). The expression it sits
/// in never produces a value, so it becomes `{ <the operands to its left, as
/// statements>; <the diverging operand> }`, itself `Never`: the shape every
/// later pass already lowers (a diverging statement), and the one the
/// enclosing expression then sees as its own diverging operand. A binding,
/// assignment or field write whose value diverges ends its block there — the
/// statements after it are unreachable, and they read the binding that is
/// never made. A short-circuit `a and d` / `a or d` with a diverging right
/// operand becomes the `if` it means (`if a then d else false` /
/// `if a then true else d`), which `desugar_diverging_value_branches` splits.
///
/// Only an expression with a `Never`-typed operand is rewritten, so a program
/// without one lowers exactly as before.
fn desugar_diverging_operands(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};

    fn never(e: &IrExpr) -> bool {
        matches!(e.ty, Ty::Never)
    }
    fn stmt(expr: IrExpr) -> IrStmt {
        let span = expr.span;
        IrStmt { kind: IrStmtKind::Expr { expr }, span }
    }
    /// `{ before…; d }`, typed `Never`. An operand with no effect to run (a
    /// variable, a literal) is not kept; with none left it is `d` itself.
    fn sequence(before: Vec<IrExpr>, d: IrExpr, e: &IrExpr) -> IrExpr {
        let runs = |o: &IrExpr| !matches!(o.kind, IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Unit
            | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. });
        let stmts: Vec<IrStmt> = before.into_iter().filter(runs).map(stmt).collect();
        // Nothing runs before it: the diverging operand itself.
        if stmts.is_empty() {
            return d;
        }
        IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(d)) },
            ty: Ty::Never,
            span: e.span,
            def_id: None,
        }
    }
    /// The first diverging operand of a strictly left-to-right list, with the
    /// operands before it.
    fn first_diverging(ops: Vec<&IrExpr>, e: &IrExpr) -> Option<IrExpr> {
        let k = ops.iter().position(|o| never(o))?;
        Some(sequence(ops[..k].iter().map(|o| (*o).clone()).collect(), ops[k].clone(), e))
    }
    fn lit_bool(value: bool) -> IrExpr {
        IrExpr { kind: IrExprKind::LitBool { value }, ty: Ty::Bool, span: None, def_id: None }
    }
    fn rewrite(e: &IrExpr) -> Option<IrExpr> {
        // Already the diverging form (a call to a `Never` fn, a block ending
        // in one): nothing to propagate.
        if never(e) && !matches!(e.kind, IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } | IrExprKind::StringInterp { .. }
            | IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }) {
            return None;
        }
        match &e.kind {
            IrExprKind::BinOp { op: almide_ir::BinOp::And, left, right } if !never(left) && never(right) => Some(IrExpr {
                kind: IrExprKind::If { cond: left.clone(), then: right.clone(), else_: Box::new(lit_bool(false)) },
                ..e.clone()
            }),
            IrExprKind::BinOp { op: almide_ir::BinOp::Or, left, right } if !never(left) && never(right) => Some(IrExpr {
                kind: IrExprKind::If { cond: left.clone(), then: Box::new(lit_bool(true)), else_: right.clone() },
                ..e.clone()
            }),
            IrExprKind::BinOp { op: almide_ir::BinOp::And | almide_ir::BinOp::Or, left, .. } => first_diverging(vec![left], e),
            IrExprKind::BinOp { left, right, .. } => first_diverging(vec![left, right], e),
            IrExprKind::UnOp { operand, .. } => first_diverging(vec![operand], e),
            IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, args, .. } => first_diverging(args.iter().collect(), e),
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => first_diverging(elements.iter().collect(), e),
            IrExprKind::MapLiteral { entries } => first_diverging(entries.iter().flat_map(|(k, v)| [k, v]).collect(), e),
            IrExprKind::Record { fields, .. } => first_diverging(fields.iter().map(|(_, v)| v).collect(), e),
            IrExprKind::StringInterp { parts } => first_diverging(
                parts.iter().filter_map(|p| match p { IrStringPart::Expr { expr } => Some(expr), IrStringPart::Lit { .. } => None }).collect(),
                e,
            ),
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } | IrExprKind::OptionSome { expr } => first_diverging(vec![expr], e),
            IrExprKind::If { cond, .. } if never(cond) => first_diverging(vec![cond], e),
            IrExprKind::Match { subject, .. } if never(subject) => first_diverging(vec![subject], e),
            IrExprKind::If { .. } | IrExprKind::Match { .. } => diverging_arms(e),
            // `{ d }` with nothing before the diverging tail is `d` (a lambda
            // body `(x) => "${x} ${panic(m)}"` reduces to it, and the lifter
            // takes a diverging call as the body it is).
            IrExprKind::Block { stmts, expr: Some(tail) } if stmts.is_empty() && never(tail) && !never(e) => Some((**tail).clone()),
            IrExprKind::Block { stmts, expr } => truncate(stmts, expr.as_deref(), e).or_else(|| {
                guard_branches(stmts).map(|stmts| IrExpr { kind: IrExprKind::Block { stmts, expr: expr.clone() }, ..e.clone() })
            }),
            // A loop body is a statement list of its own.
            IrExprKind::ForIn { var, var_tuple, iterable, body } => guard_branches(body).map(|body| IrExpr {
                kind: IrExprKind::ForIn { var: *var, var_tuple: var_tuple.clone(), iterable: iterable.clone(), body },
                ..e.clone()
            }),
            IrExprKind::While { cond, body } => guard_branches(body).map(|body| IrExpr {
                kind: IrExprKind::While { cond: cond.clone(), body },
                ..e.clone()
            }),
            _ => None,
        }
    }
    /// An arm that is a block ending in a diverging expression diverges
    /// itself, whatever the type it was checked at (an effect fn's arm is
    /// lifted to its `Result`, `{ println(m); ok(process.exit(1)) }`, whose
    /// `ok(…)` the operand rule above turned into the diverging block):
    /// typed `Never`, it is the arm `desugar_diverging_value_branches` splits.
    fn diverging_arms(e: &IrExpr) -> Option<IrExpr> {
        fn retype(arm: &mut IrExpr) -> bool {
            let ends = matches!(&arm.kind, IrExprKind::Block { expr: Some(tail), .. } if never(tail));
            if ends && !never(arm) {
                arm.ty = Ty::Never;
                return true;
            }
            false
        }
        let mut out = e.clone();
        let changed = match &mut out.kind {
            IrExprKind::If { then, else_, .. } => retype(then) | retype(else_),
            IrExprKind::Match { arms, .. } => arms.iter_mut().fold(false, |c, a| retype(&mut a.body) | c),
            _ => false,
        };
        changed.then_some(out)
    }
    /// `guard c else d` whose `d` diverges is `if c then () else d`: the
    /// path past it is the one `c` holds on, with nothing to return early —
    /// so it lowers as the statement branch it is, inside a loop body too
    /// (`for x in xs { guard x > 0 else { panic(m) } … }`).
    fn guard_branches(stmts: &[IrStmt]) -> Option<Vec<IrStmt>> {
        let ends = |d: &IrExpr| never(d) || matches!(&d.kind, IrExprKind::Block { expr: Some(t), .. } if never(t));
        if !stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { else_, .. } if ends(else_))) {
            return None;
        }
        let stmts = stmts.iter().map(|s| match &s.kind {
            IrStmtKind::Guard { cond, else_ } if ends(else_) => IrStmt {
                kind: IrStmtKind::Expr {
                    expr: IrExpr {
                        kind: IrExprKind::If {
                            cond: Box::new(cond.clone()),
                            then: Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }),
                            else_: Box::new(else_.clone()),
                        },
                        ty: Ty::Unit,
                        span: s.span,
                        def_id: None,
                    },
                },
                span: s.span,
            },
            _ => s.clone(),
        }).collect();
        Some(stmts)
    }
    /// A block whose binding / assignment value diverges ends at it.
    fn truncate(stmts: &[IrStmt], tail: Option<&IrExpr>, e: &IrExpr) -> Option<IrExpr> {
        let value = |s: &IrStmt| match &s.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
            | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => Some(value.clone()),
            _ => None,
        };
        let i = stmts.iter().position(|s| value(s).is_some_and(|v| never(&v)))?;
        let d = value(&stmts[i]).expect("position found a value");
        // Nothing after the divergence runs, the tail included.
        let _ = tail;
        Some(IrExpr {
            kind: IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(d)) },
            ty: e.ty.clone(),
            span: e.span,
            def_id: e.def_id,
        })
    }
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let Some(out) = rewrite(e) {
                *e = out;
                self.changed = true;
            }
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// A statement `if` whose arm is only an inert value has `()` there; and a
/// value `if` whose arms end on the same inert value —
/// `if c then { println(m); ok(()) } else ok(())`, the shape a
/// `guard c else ok(())` in an effect fn desugars to — is the statement
/// branch followed by that value: `{ if c then { println(m) } else (); ok(()) }`.
/// The value reads nothing an arm could change (a literal, `()`, `none`, or
/// `ok` / `err` / `some` of one), so it is the same whichever arm ran, and the
/// statement branch lowers where the value branch walled (a call-bearing arm
/// over a Result-typed join).
fn desugar_common_value_tail(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    fn inert(e: &IrExpr) -> Option<String> {
        Some(match &e.kind {
            IrExprKind::Unit => "()".to_string(),
            IrExprKind::OptionNone => "none".to_string(),
            IrExprKind::LitInt { value } => format!("i{value}"),
            IrExprKind::LitBool { value } => format!("b{value}"),
            IrExprKind::LitStr { value } => format!("s{value:?}"),
            IrExprKind::ResultOk { expr } => format!("ok({})", inert(expr)?),
            IrExprKind::ResultErr { expr } => format!("err({})", inert(expr)?),
            IrExprKind::OptionSome { expr } => format!("some({})", inert(expr)?),
            _ => return None,
        })
    }
    /// An arm's statements and its tail value.
    fn split(arm: &IrExpr) -> (Vec<IrStmt>, &IrExpr) {
        match &arm.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => (stmts.clone(), tail),
            _ => (Vec::new(), arm),
        }
    }
    fn unit_arm(stmts: Vec<IrStmt>, span: Option<almide_ir::Span>) -> IrExpr {
        let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
        if stmts.is_empty() {
            return unit;
        }
        IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(unit)) }, ty: Ty::Unit, span, def_id: None }
    }
    fn rewrite(e: &IrExpr) -> Option<IrExpr> {
        // A statement `if` (typed Unit) discards its arms' values: an arm that
        // is only an inert value is `()` (the effect ABI strips one arm's
        // `ok(())` and leaves the other's: `if c then { println(m) } else ok(())`).
        if matches!(e.ty, Ty::Unit) {
            let IrExprKind::If { cond, then, else_ } = &e.kind else { return None };
            // Never an `err(…)`: whatever stripped its sibling, an err is
            // the fn's own answer, not a discarded value.
            let vacant = |a: &IrExpr| a.ty != Ty::Unit && !matches!(a.kind, IrExprKind::ResultErr { .. }) && inert(a).is_some();
            if !vacant(then) && !vacant(else_) {
                return None;
            }
            let unit = |a: &IrExpr| if vacant(a) { unit_arm(Vec::new(), a.span) } else { a.clone() };
            return Some(IrExpr {
                kind: IrExprKind::If { cond: cond.clone(), then: Box::new(unit(then)), else_: Box::new(unit(else_)) },
                ..e.clone()
            });
        }
        if matches!(e.ty, Ty::Never) {
            return None;
        }
        let IrExprKind::If { cond, then, else_ } = &e.kind else { return None };
        let ((ts, tt), (es, et)) = (split(then), split(else_));
        let (Some(a), Some(b)) = (inert(tt), inert(et)) else { return None };
        // The value must be the arms' own type (a `Never` arm is not split here).
        if a != b || tt.ty != e.ty || et.ty != e.ty || (ts.is_empty() && es.is_empty()) {
            return None;
        }
        let value = tt.clone();
        let branch = IrExpr {
            kind: IrExprKind::If { cond: cond.clone(), then: Box::new(unit_arm(ts, then.span)), else_: Box::new(unit_arm(es, else_.span)) },
            ty: Ty::Unit,
            span: e.span,
            def_id: None,
        };
        Some(IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: branch }, span: e.span }],
                expr: Some(Box::new(value)),
            },
            ty: e.ty.clone(),
            span: e.span,
            def_id: e.def_id,
        })
    }
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let Some(out) = rewrite(e) {
                *e = out;
                self.changed = true;
            }
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}
