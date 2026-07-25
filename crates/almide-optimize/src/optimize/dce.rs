/// Pass 2: Dead Code Elimination — remove unused bindings with pure values.

use std::collections::HashSet;

use almide_ir::*;

pub(super) fn eliminate_dead_code(program: &mut IrProgram) {
    for f in &mut program.functions {
        let mutated = mutation_targets(&f.body);
        dce_expr(&mut f.body, &program.var_table, &mutated);
    }
    for tl in &mut program.top_lets {
        let mutated = mutation_targets(&tl.value);
        dce_expr(&mut tl.value, &program.var_table, &mutated);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            let mutated = mutation_targets(&f.body);
            dce_expr(&mut f.body, &m.var_table, &mutated);
        }
        for tl in &mut m.top_lets {
            let mutated = mutation_targets(&tl.value);
            dce_expr(&mut tl.value, &m.var_table, &mutated);
        }
    }
}

/// Variables written in place somewhere in this body. A write is not counted
/// by `use_count` (it is not a read), but it still NAMES the binding in the
/// emitted code, so such a binding is never dead (#857).
fn mutation_targets(body: &IrExpr) -> HashSet<u32> {
    let mut out = HashSet::new();
    collect_assigned_vars(body, &mut out);
    out
}

fn dce_expr(expr: &mut IrExpr, var_table: &VarTable, mutated: &HashSet<u32>) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts.iter_mut() { dce_stmt(s, var_table, mutated); }
            dce_stmts_keeping(stmts, var_table, mutated);
            if let Some(t) = tail { dce_expr(t, var_table, mutated); }
        }
        IrExprKind::If { cond, then, else_ } => {
            dce_expr(cond, var_table, mutated);
            dce_expr(then, var_table, mutated);
            dce_expr(else_, var_table, mutated);
        }
        IrExprKind::Match { subject, arms } => {
            dce_expr(subject, var_table, mutated);
            for a in arms { dce_expr(&mut a.body, var_table, mutated); }
        }
        IrExprKind::Lambda { body, .. } => dce_expr(body, var_table, mutated),
        IrExprKind::ForIn { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table, mutated); }
            dce_stmts_keeping(body, var_table, mutated);
        }
        IrExprKind::While { body, .. } => {
            for s in body.iter_mut() { dce_stmt(s, var_table, mutated); }
            dce_stmts_keeping(body, var_table, mutated);
        }
        _ => {}
    }
}

fn dce_stmt(stmt: &mut IrStmt, var_table: &VarTable, mutated: &HashSet<u32>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => dce_expr(value, var_table, mutated),
        IrStmtKind::Expr { expr } => dce_expr(expr, var_table, mutated),
        IrStmtKind::Guard { cond, else_ } => {
            dce_expr(cond, var_table, mutated);
            dce_expr(else_, var_table, mutated);
        }
        _ => {}
    }
}

/// Remove `let x = <pure>` statements where x has use_count == 0 and is never
/// written in place. Callers with no mutation analysis of their own use
/// [`dce_stmts`]; the enclosing-body variant is [`dce_stmts_keeping`].
pub(crate) fn dce_stmts(stmts: &mut Vec<IrStmt>, var_table: &VarTable) {
    let mut mutated = HashSet::new();
    for stmt in stmts.iter() {
        if let IrStmtKind::Bind { value, .. } = &stmt.kind {
            collect_assigned_vars(value, &mut mutated);
        }
        if let IrStmtKind::Expr { expr } = &stmt.kind {
            collect_assigned_vars(expr, &mut mutated);
        }
        match &stmt.kind {
            IrStmtKind::Assign { var, .. }
            | IrStmtKind::IndexAssign { target: var, .. }
            | IrStmtKind::MapInsert { target: var, .. }
            | IrStmtKind::FieldAssign { target: var, .. }
            | IrStmtKind::ListSwap { target: var, .. }
            | IrStmtKind::ListReverse { target: var, .. }
            | IrStmtKind::ListRotateLeft { target: var, .. } => { mutated.insert(var.0); }
            IrStmtKind::ListCopySlice { dst, .. } => { mutated.insert(dst.0); }
            _ => {}
        }
    }
    dce_stmts_keeping(stmts, var_table, &mutated);
}

fn dce_stmts_keeping(stmts: &mut Vec<IrStmt>, var_table: &VarTable, mutated: &HashSet<u32>) {
    stmts.retain(|stmt| {
        match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => {
                if var_table.use_count(*var) == 0
                    && !mutated.contains(&var.0)
                    && is_pure(value)
                    && !contains_call(value)
                {
                    return false; // remove
                }
                true
            }
            // Bare expression statements are always kept — even if the expression
            // type is Unit it may have side effects (e.g. extern function calls).
            IrStmtKind::Expr { .. } => true,
            _ => true,
        }
    });
}

/// Returns true if an expression tree contains any Call/TailCall node.
/// Used as a safety check to prevent elimination of side-effectful expressions.
fn contains_call(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { .. } | IrExprKind::TailCall { .. } => true,
        IrExprKind::Block { stmts, expr: tail } => {
            stmts.iter().any(|s| match &s.kind {
                IrStmtKind::Bind { value, .. } => contains_call(value),
                IrStmtKind::Expr { expr } => contains_call(expr),
                _ => false,
            }) || tail.as_ref().map_or(false, |e| contains_call(e))
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } => contains_call(expr),
        IrExprKind::If { cond, then, else_ } => {
            contains_call(cond) || contains_call(then) || contains_call(else_)
        }
        _ => false,
    }
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
