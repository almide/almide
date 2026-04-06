// ── Constant folding (post-pass) ─────────────────────────────────

use super::*;

/// Fold constant expressions in the IR program.
/// e.g. LitInt(1) + LitInt(2) → LitInt(3)
pub fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_expr(&mut f.body);
    }
    for tl in &mut program.top_lets {
        fold_expr(&mut tl.value);
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
        IrExprKind::Call { args, .. } => {
            for a in args { fold_expr(a); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
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
        IrExprKind::ForIn { iterable, body, .. } => {
            fold_expr(iterable);
            for s in body { fold_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            fold_expr(cond);
            for s in body { fold_stmt(s); }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::ToVec { expr } => fold_expr(expr),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            fold_expr(e);
            fold_expr(f);
        }
        IrExprKind::OptionalChain { expr: object, .. } => fold_expr(object),
        IrExprKind::RustMacro { args, .. } => {
            for a in args { fold_expr(a); }
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            fold_expr(source);
            for step in steps {
                match step {
                    super::IterStep::Map { lambda } | super::IterStep::Filter { lambda }
                    | super::IterStep::FlatMap { lambda } | super::IterStep::FilterMap { lambda } => {
                        fold_expr(lambda);
                    }
                }
            }
            match collector {
                super::IterCollector::Collect => {}
                super::IterCollector::Fold { init, lambda } => { fold_expr(init); fold_expr(lambda); }
                super::IterCollector::Any { lambda } | super::IterCollector::All { lambda }
                | super::IterCollector::Find { lambda } | super::IterCollector::Count { lambda } => {
                    fold_expr(lambda);
                }
            }
        }
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
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => fold_expr(object),
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { fold_expr(k); fold_expr(v); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { fold_expr(e); }
            }
        }
        _ => {}
    }

    // Now try to fold this node
    let folded = match &expr.kind {
        IrExprKind::BinOp { op, left, right } => {
            match (&left.kind, &right.kind) {
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
                        BinOp::Or => Some(IrExprKind::LitBool { value: *a || *b }),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        IrExprKind::UnOp { op, operand } => {
            match (&op, &operand.kind) {
                (UnOp::NegInt, IrExprKind::LitInt { value }) => Some(IrExprKind::LitInt { value: -value }),
                (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
                (UnOp::Not, IrExprKind::LitBool { value }) => Some(IrExprKind::LitBool { value: !value }),
                _ => None,
            }
        }
        _ => None,
    };

    if let Some(kind) = folded {
        expr.kind = kind;
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
        IrStmtKind::Guard { cond, else_ } => {
            fold_expr(cond);
            fold_expr(else_);
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
        IrStmtKind::Expr { expr } => fold_expr(expr),
        IrStmtKind::Comment { .. } => {}
    }
}
