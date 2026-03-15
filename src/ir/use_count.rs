// ── Use-count computation (post-pass) ───────────────────────────

use std::collections::HashSet;
use super::*;

/// Walk the entire IR program and count variable uses, storing results in VarTable.
pub fn compute_use_counts(program: &mut IrProgram) {
    // Reset all counts
    for i in 0..program.var_table.len() {
        program.var_table.entries[i].use_count = 0;
    }

    // Count uses in all function bodies
    for func in &program.functions {
        count_uses_in_expr(&func.body, &mut program.var_table);
    }

    // Count uses in top-level let values
    for tl in &program.top_lets {
        count_uses_in_expr(&tl.value, &mut program.var_table);
    }
}

fn count_uses_in_expr(expr: &IrExpr, table: &mut VarTable) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            table.increment_use(*id);
        }
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::EmptyMap => {}

        IrExprKind::BinOp { left, right, .. } => {
            count_uses_in_expr(left, table);
            count_uses_in_expr(right, table);
        }
        IrExprKind::UnOp { operand, .. } => {
            count_uses_in_expr(operand, table);
        }
        IrExprKind::If { cond, then, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(then, table);
            count_uses_in_expr(else_, table);
        }
        IrExprKind::Match { subject, arms } => {
            count_uses_in_expr(subject, table);
            for arm in arms {
                if let Some(g) = &arm.guard { count_uses_in_expr(g, table); }
                count_uses_in_expr(&arm.body, table);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { count_uses_in_stmt(s, table); }
            if let Some(e) = expr { count_uses_in_expr(e, table); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => count_uses_in_expr(object, table),
                CallTarget::Computed { callee } => count_uses_in_expr(callee, table),
                _ => {}
            }
            for a in args { count_uses_in_expr(a, table); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { count_uses_in_expr(e, table); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { count_uses_in_expr(e, table); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            count_uses_in_expr(base, table);
            for (_, e) in fields { count_uses_in_expr(e, table); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                count_uses_in_expr(k, table);
                count_uses_in_expr(v, table);
            }
        }
        IrExprKind::Range { start, end, .. } => {
            count_uses_in_expr(start, table);
            count_uses_in_expr(end, table);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            count_uses_in_expr(object, table);
        }
        IrExprKind::IndexAccess { object, index } => {
            count_uses_in_expr(object, table);
            count_uses_in_expr(index, table);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            count_uses_in_expr(iterable, table);
            for s in body { count_uses_in_stmt(s, table); }
        }
        IrExprKind::While { cond, body } => {
            count_uses_in_expr(cond, table);
            for s in body { count_uses_in_stmt(s, table); }
        }
        IrExprKind::Lambda { body, .. } => {
            count_uses_in_expr(body, table);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    count_uses_in_expr(expr, table);
                }
            }
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => {
            count_uses_in_expr(expr, table);
        }
    }
}

fn count_uses_in_stmt(stmt: &IrStmt, table: &mut VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            count_uses_in_expr(index, table);
            count_uses_in_expr(value, table);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::Expr { expr } => {
            count_uses_in_expr(expr, table);
        }
        IrStmtKind::Guard { cond, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(else_, table);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Demote `var` to `let` for variables that are never reassigned.
/// This is a post-pass optimization that runs after compute_use_counts.
pub fn demote_unused_mut(program: &mut IrProgram) {
    let mut assigned_vars: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        collect_assigned_vars(&func.body, &mut assigned_vars);
    }
    for i in 0..program.var_table.len() {
        if program.var_table.entries[i].mutability == Mutability::Var
            && !assigned_vars.contains(&(i as u32))
        {
            program.var_table.entries[i].mutability = Mutability::Let;
        }
    }
}

fn collect_assigned_vars(expr: &IrExpr, assigned: &mut HashSet<u32>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { collect_assigned_vars_stmt(s, assigned); }
            if let Some(e) = expr { collect_assigned_vars(e, assigned); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_assigned_vars(cond, assigned);
            collect_assigned_vars(then, assigned);
            collect_assigned_vars(else_, assigned);
        }
        IrExprKind::Match { subject, arms } => {
            collect_assigned_vars(subject, assigned);
            for a in arms { collect_assigned_vars(&a.body, assigned); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_assigned_vars(iterable, assigned);
            for s in body { collect_assigned_vars_stmt(s, assigned); }
        }
        IrExprKind::While { cond, body } => {
            collect_assigned_vars(cond, assigned);
            for s in body { collect_assigned_vars_stmt(s, assigned); }
        }
        IrExprKind::Lambda { body, .. } => collect_assigned_vars(body, assigned),
        _ => {}
    }
}

fn collect_assigned_vars_stmt(stmt: &IrStmt, assigned: &mut HashSet<u32>) {
    match &stmt.kind {
        IrStmtKind::Assign { var, value } => {
            assigned.insert(var.0);
            collect_assigned_vars(value, assigned);
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            assigned.insert(target.0);
            collect_assigned_vars(index, assigned);
            collect_assigned_vars(value, assigned);
        }
        IrStmtKind::FieldAssign { target, value, .. } => {
            assigned.insert(target.0);
            collect_assigned_vars(value, assigned);
        }
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            collect_assigned_vars(value, assigned);
        }
        IrStmtKind::Expr { expr } => collect_assigned_vars(expr, assigned),
        IrStmtKind::Guard { cond, else_ } => {
            collect_assigned_vars(cond, assigned);
            collect_assigned_vars(else_, assigned);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Collect warnings for unused variables.
/// Skips: `_` prefixed names, function parameters, pattern bindings (span is None).
pub fn collect_unused_var_warnings(program: &IrProgram, file: &str) -> Vec<crate::diagnostic::Diagnostic> {
    // Collect all parameter VarIds to exclude them
    let mut param_ids: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        for p in &func.params {
            param_ids.insert(p.var.0);
        }
    }

    let mut warnings = Vec::new();
    for i in 0..program.var_table.len() {
        let info = &program.var_table.entries[i];

        // Skip _ prefixed (intentionally unused)
        if info.name.starts_with('_') { continue; }

        // Skip parameters
        if param_ids.contains(&(i as u32)) { continue; }

        // Skip variables without span (pattern bindings, loop vars, etc.)
        if info.span.is_none() { continue; }

        // Skip if used
        if info.use_count > 0 { continue; }

        let span = info.span.unwrap();
        let diag = crate::diagnostic::Diagnostic::warning(
            format!("unused variable '{}'", info.name),
            format!("Prefix with '_' to suppress: _{}", info.name),
            "",
        ).at(file, span.line);
        warnings.push(diag);
    }
    warnings
}

/// Classify a top-level let value: simple literals are `Const`, everything else is `Lazy`.
pub fn classify_top_let_kind(expr: &IrExpr) -> TopLetKind {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit => TopLetKind::Const,
        _ => TopLetKind::Lazy,
    }
}
