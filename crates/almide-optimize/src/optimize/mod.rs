/// IR optimization passes.
///
/// Pipeline position: Lower -> IR -> **optimize()** -> mono() -> codegen
///
/// Pass 1: Constant folding -- evaluate compile-time-known expressions.
/// Pass 2: Dead code elimination -- remove unused bindings with pure values.
/// Pass 3: Constant propagation -- replace vars bound to literals with the literal.

mod dce;
mod propagate;

use almide_ir::*;

// ── Public entry point ──────────────────────────────────────────

/// Run all optimization passes on an IR program.
/// Requires use-counts to be computed (done by `lower_program`).
pub fn optimize_program(program: &mut IrProgram) {
    // Pass 1: constant folding (bottom-up rewrite)
    constant_fold(program);

    // Recompute use-counts after folding may have eliminated references
    compute_use_counts(program);

    // Pass 2: dead code elimination
    dce::eliminate_dead_code(program);

    // Pass 3: constant propagation (replace vars bound to literals with the literal)
    propagate::constant_propagate(program);

    // Recompute after propagation may have reduced use-counts
    compute_use_counts(program);

    // Pass 4: dead code elimination again (propagation may create new dead bindings)
    dce::eliminate_dead_code(program);

    // Recompute use-counts for downstream consumers (mono, borrow analysis)
    compute_use_counts(program);
}

// ── Pass 1: Constant Folding ────────────────────────────────────

fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_expr(&mut f.body);
    }
    for tl in &mut program.top_lets {
        fold_expr(&mut tl.value);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            fold_expr(&mut f.body);
        }
        for tl in &mut m.top_lets {
            fold_expr(&mut tl.value);
        }
    }
}

fn fold_expr(expr: &mut IrExpr) {
    // Recurse first (bottom-up)
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            fold_expr(left);
            fold_expr(right);
        }
        IrExprKind::UnOp { operand, .. } => fold_expr(operand),
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { fold_stmt(s); }
            if let Some(t) = tail { fold_expr(t); }
        }
        IrExprKind::If { cond, then, else_ } => {
            fold_expr(cond);
            fold_expr(then);
            fold_expr(else_);
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
                fold_expr(object);
            }
            for a in args { fold_expr(a); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { fold_expr(e); }
        }
        IrExprKind::Lambda { body, .. } => fold_expr(body),
        IrExprKind::Match { subject, arms } => {
            fold_expr(subject);
            for a in arms {
                if let Some(g) = &mut a.guard { fold_expr(g); }
                fold_expr(&mut a.body);
            }
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e } => fold_expr(e),
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            fold_expr(base);
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::Range { start, end, .. } => {
            fold_expr(start);
            fold_expr(end);
        }
        IrExprKind::IndexAccess { object, index } => {
            fold_expr(object);
            fold_expr(index);
        }
        IrExprKind::MapAccess { object, key } => {
            fold_expr(object);
            fold_expr(key);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            fold_expr(object);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { fold_expr(k); fold_expr(v); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { fold_expr(e); }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            fold_expr(iterable);
            for s in body { fold_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            fold_expr(cond);
            for s in body { fold_stmt(s); }
        }
        _ => {}
    }

    // Now try to fold this node
    let replacement = try_fold(expr);
    if let Some(new_expr) = replacement {
        *expr = new_expr;
    }
}

/// Try to reduce an expression to a simpler form.
/// Returns Some(replacement) if the node can be folded.
fn try_fold(expr: &IrExpr) -> Option<IrExpr> {
    match &expr.kind {
        // ── Arithmetic / string / bool on literals ──
        IrExprKind::BinOp { op, left, right } => {
            let folded_kind = match (&left.kind, &right.kind) {
                (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => {
                    match op {
                        BinOp::AddInt => Some(IrExprKind::LitInt { value: a.wrapping_add(*b) }),
                        BinOp::SubInt => Some(IrExprKind::LitInt { value: a.wrapping_sub(*b) }),
                        BinOp::MulInt => Some(IrExprKind::LitInt { value: a.wrapping_mul(*b) }),
                        BinOp::DivInt if *b != 0 => Some(IrExprKind::LitInt { value: a / b }),
                        BinOp::ModInt if *b != 0 => Some(IrExprKind::LitInt { value: a % b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => {
                    match op {
                        BinOp::AddFloat => Some(IrExprKind::LitFloat { value: a + b }),
                        BinOp::SubFloat => Some(IrExprKind::LitFloat { value: a - b }),
                        BinOp::MulFloat => Some(IrExprKind::LitFloat { value: a * b }),
                        BinOp::DivFloat if *b != 0.0 => Some(IrExprKind::LitFloat { value: a / b }),
                        _ => None,
                    }
                }
                (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) => {
                    match op {
                        BinOp::ConcatStr => Some(IrExprKind::LitStr { value: format!("{}{}", a, b) }),
                        _ => None,
                    }
                }
                (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => {
                    match op {
                        BinOp::And => Some(IrExprKind::LitBool { value: *a && *b }),
                        BinOp::Or  => Some(IrExprKind::LitBool { value: *a || *b }),
                        _ => None,
                    }
                }
                _ => None,
            };
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span })
        }

        // ── Unary on literals ──
        IrExprKind::UnOp { op, operand } => {
            let folded_kind = match (op, &operand.kind) {
                (UnOp::NegInt,   IrExprKind::LitInt   { value }) => Some(IrExprKind::LitInt   { value: -value }),
                (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
                (UnOp::Not,      IrExprKind::LitBool  { value }) => Some(IrExprKind::LitBool  { value: !value }),
                _ => None,
            };
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span })
        }

        // ── if true then a else b -> a,  if false then a else b -> b ──
        IrExprKind::If { cond, then, else_ } => {
            match &cond.kind {
                IrExprKind::LitBool { value: true }  => Some(then.as_ref().clone()),
                IrExprKind::LitBool { value: false } => Some(else_.as_ref().clone()),
                _ => None,
            }
        }

        _ => None,
    }
}

fn fold_stmt(stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => fold_expr(value),
        IrStmtKind::IndexAssign { index, value, .. } => {
            fold_expr(index);
            fold_expr(value);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            fold_expr(key);
            fold_expr(value);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            fold_expr(a);
            fold_expr(b);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            fold_expr(end);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            fold_expr(len);
        }
        IrStmtKind::Guard { cond, else_ } => {
            fold_expr(cond);
            fold_expr(else_);
        }
        IrStmtKind::Expr { expr } => fold_expr(expr),
        IrStmtKind::Comment { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::dce::dce_stmts;
    use almide_lang::types::Ty;

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None }
    }

    fn lit_str(v: &str) -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: v.to_string() }, ty: Ty::String, span: None }
    }

    fn lit_bool(v: bool) -> IrExpr {
        IrExpr { kind: IrExprKind::LitBool { value: v }, ty: Ty::Bool, span: None }
    }

    #[test]
    fn fold_int_add() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(lit_int(1)),
                right: Box::new(lit_int(2)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 3 }));
    }

    #[test]
    fn fold_str_concat() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::ConcatStr,
                left: Box::new(lit_str("a")),
                right: Box::new(lit_str("b")),
            },
            ty: Ty::String,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitStr { ref value } if value == "ab"));
    }

    #[test]
    fn fold_not_true() {
        let mut e = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::Not,
                operand: Box::new(lit_bool(true)),
            },
            ty: Ty::Bool,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitBool { value: false }));
    }

    #[test]
    fn fold_if_true() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(true)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 10 }));
    }

    #[test]
    fn fold_if_false() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(false)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 20 }));
    }

    #[test]
    fn dce_removes_unused_pure_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert!(stmts.is_empty());
    }

    #[test]
    fn dce_keeps_used_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        var_table.increment_use(x);

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn dce_keeps_impure_unused_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0, but value is a call (impure)

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Named { name: "expensive".into() },
                            args: vec![],
                            type_args: vec![],
                        },
                        ty: Ty::Int,
                        span: None,
                    },
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1); // kept because call may have side effects
    }
}
