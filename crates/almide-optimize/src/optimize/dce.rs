/// Pass 2: Dead Code Elimination — remove unused bindings with pure values.

use almide_ir::*;

pub(super) fn eliminate_dead_code(program: &mut IrProgram) {
    for f in &mut program.functions {
        dce_expr(&mut f.body, &program.var_table);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            dce_expr(&mut f.body, &m.var_table);
        }
    }
}

fn dce_expr(expr: &mut IrExpr, var_table: &VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(stmts, var_table);
            if let Some(t) = tail { dce_expr(t, var_table); }
        }
        IrExprKind::If { cond, then, else_ } => {
            dce_expr(cond, var_table);
            dce_expr(then, var_table);
            dce_expr(else_, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            dce_expr(subject, var_table);
            for a in arms { dce_expr(&mut a.body, var_table); }
        }
        IrExprKind::Lambda { body, .. } => dce_expr(body, var_table),
        IrExprKind::ForIn { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(body, var_table);
        }
        IrExprKind::While { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table); }
            dce_stmts(body, var_table);
        }
        _ => {}
    }
}

fn dce_stmt(stmt: &mut IrStmt, var_table: &VarTable) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => dce_expr(value, var_table),
        IrStmtKind::Expr { expr } => dce_expr(expr, var_table),
        IrStmtKind::Guard { cond, else_ } => {
            dce_expr(cond, var_table);
            dce_expr(else_, var_table);
        }
        _ => {}
    }
}

/// Remove `let x = <pure>` statements where x has use_count == 0.
pub(crate) fn dce_stmts(stmts: &mut Vec<IrStmt>, var_table: &VarTable) {
    stmts.retain(|stmt| {
        match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => {
                if var_table.use_count(*var) == 0 && is_pure(value) {
                    return false; // remove
                }
                true
            }
            _ => true,
        }
    });
}

/// An expression is pure if evaluating it has no side effects.
/// Conservative: anything we're unsure about is treated as impure.
fn is_pure(expr: &IrExpr) -> bool {
    match &expr.kind {
        // Literals are always pure
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::OptionNone | IrExprKind::EmptyMap => true,

        // Variable references are pure
        IrExprKind::Var { .. } => true,

        // Operators on pure operands are pure
        IrExprKind::BinOp { left, right, .. } => is_pure(left) && is_pure(right),
        IrExprKind::UnOp { operand, .. } => is_pure(operand),

        // Collection constructors with pure elements
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(is_pure)
        }
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| is_pure(v)),
        IrExprKind::Range { start, end, .. } => is_pure(start) && is_pure(end),

        // Wrapping pure values
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => is_pure(expr),

        // Member/index on pure base
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => is_pure(object),

        // Lambda is pure (it's just a value, not invoked)
        IrExprKind::Lambda { .. } => true,

        // String interpolation with pure parts
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Lit { .. } => true,
                IrStringPart::Expr { expr } => is_pure(expr),
            })
        }

        // Everything else (calls, blocks, loops, if, match, etc.) is conservatively impure
        _ => false,
    }
}
