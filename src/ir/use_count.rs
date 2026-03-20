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
        IrExprKind::FnRef { .. } => {} // function reference, no VarId to track
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
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
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
        IrExprKind::MapAccess { object, key } => {
            count_uses_in_expr(object, table);
            count_uses_in_expr(key, table);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            count_uses_in_expr(iterable, table);
            // Collect vars defined inside the loop body
            let body_locals = collect_bound_vars(body);
            // Count body uses normally
            for s in body { count_uses_in_stmt(s, table); }
            // Extra count for outer vars used in loop body (they're used N times at runtime)
            bump_outer_vars_in_loop(body, &body_locals, table);
        }
        IrExprKind::While { cond, body } => {
            count_uses_in_expr(cond, table);
            let body_locals = collect_bound_vars(body);
            for s in body { count_uses_in_stmt(s, table); }
            bump_outer_vars_in_loop(body, &body_locals, table);
        }
        IrExprKind::Lambda { params, body } => {
            count_uses_in_expr(body, table);
            // Bump outer vars captured by lambda (closure captures move by default)
            let mut lambda_locals: HashSet<u32> = params.iter().map(|(v, _)| v.0).collect();
            // Also collect any bindings inside the lambda body
            if let IrExprKind::Block { stmts, .. } = &body.kind {
                lambda_locals.extend(collect_bound_vars(stmts));
            }
            bump_vars_in_expr(body, &lambda_locals, table);
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
        | IrExprKind::Await { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } => {
            count_uses_in_expr(expr, table);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { count_uses_in_expr(a, table); }
        }
        IrExprKind::RenderedCall { .. } => {}
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
        IrStmtKind::MapInsert { key, value, .. } => {
            count_uses_in_expr(key, table);
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

/// Collect VarIds that are bound (let/var) inside a list of statements.
fn collect_bound_vars(stmts: &[IrStmt]) -> HashSet<u32> {
    let mut locals = HashSet::new();
    for s in stmts {
        match &s.kind {
            IrStmtKind::Bind { var, .. } => { locals.insert(var.0); }
            IrStmtKind::BindDestructure { pattern, .. } => collect_pattern_vars(pattern, &mut locals),
            _ => {}
        }
    }
    locals
}

fn collect_pattern_vars(pat: &IrPattern, vars: &mut HashSet<u32>) {
    match pat {
        IrPattern::Bind { var, .. } => { vars.insert(var.0); }
        IrPattern::Constructor { args, .. } => { for a in args { collect_pattern_vars(a, vars); } }
        IrPattern::Tuple { elements } => { for e in elements { collect_pattern_vars(e, vars); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => collect_pattern_vars(inner, vars),
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &f.pattern { collect_pattern_vars(p, vars); } }
        }
        _ => {}
    }
}

/// Extra-count Var references in loop body for variables defined OUTSIDE the loop.
/// This makes use_count > 1 for outer vars, triggering clone insertion.
fn bump_outer_vars_in_loop(stmts: &[IrStmt], locals: &HashSet<u32>, table: &mut VarTable) {
    for s in stmts {
        bump_vars_in_stmt(s, locals, table);
    }
}

fn bump_vars_in_expr(expr: &IrExpr, locals: &HashSet<u32>, table: &mut VarTable) {
    match &expr.kind {
        IrExprKind::Var { id } if !locals.contains(&id.0) => {
            table.increment_use(*id);
        }
        // Recurse into sub-expressions but don't double-count nested loops
        // (they'll handle their own bumping)
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { bump_vars_in_stmt(s, locals, table); }
            if let Some(e) = expr { bump_vars_in_expr(e, locals, table); }
        }
        IrExprKind::If { cond, then, else_ } => {
            bump_vars_in_expr(cond, locals, table);
            bump_vars_in_expr(then, locals, table);
            bump_vars_in_expr(else_, locals, table);
        }
        IrExprKind::Match { subject, arms } => {
            bump_vars_in_expr(subject, locals, table);
            for a in arms { bump_vars_in_expr(&a.body, locals, table); }
        }
        IrExprKind::Call { args, .. } => { for a in args { bump_vars_in_expr(a, locals, table); } }
        IrExprKind::BinOp { left, right, .. } => {
            bump_vars_in_expr(left, locals, table);
            bump_vars_in_expr(right, locals, table);
        }
        IrExprKind::UnOp { operand, .. } => bump_vars_in_expr(operand, locals, table),
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { bump_vars_in_expr(expr, locals, table); } }
        }
        IrExprKind::Lambda { body, .. } => bump_vars_in_expr(body, locals, table),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr } => bump_vars_in_expr(expr, locals, table),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => bump_vars_in_expr(object, locals, table),
        IrExprKind::IndexAccess { object, index } => {
            bump_vars_in_expr(object, locals, table);
            bump_vars_in_expr(index, locals, table);
        }
        IrExprKind::MapAccess { object, key } => {
            bump_vars_in_expr(object, locals, table);
            bump_vars_in_expr(key, locals, table);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { bump_vars_in_expr(e, locals, table); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { bump_vars_in_expr(v, locals, table); }
        }
        _ => {}
    }
}

fn bump_vars_in_stmt(stmt: &IrStmt, locals: &HashSet<u32>, table: &mut VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => bump_vars_in_expr(value, locals, table),
        IrStmtKind::IndexAssign { index, value, .. } => {
            bump_vars_in_expr(index, locals, table);
            bump_vars_in_expr(value, locals, table);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            bump_vars_in_expr(key, locals, table);
            bump_vars_in_expr(value, locals, table);
        }
        IrStmtKind::FieldAssign { value, .. } => bump_vars_in_expr(value, locals, table),
        IrStmtKind::Expr { expr } => bump_vars_in_expr(expr, locals, table),
        IrStmtKind::Guard { cond, else_ } => {
            bump_vars_in_expr(cond, locals, table);
            bump_vars_in_expr(else_, locals, table);
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
        IrStmtKind::MapInsert { target, key, value } => {
            assigned.insert(target.0);
            collect_assigned_vars(key, assigned);
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

        let span = match info.span { Some(s) => s, None => continue };
        let diag = crate::diagnostic::Diagnostic::warning(
            format!("unused variable '{}'", info.name),
            format!("Prefix with '_' to suppress: _{}", info.name),
            "",
        ).at(file, span.line);
        warnings.push(diag);
    }
    warnings
}

/// Classify a top-level let value: constant-evaluable expressions are `Const`, everything else is `Lazy`.
pub fn classify_top_let_kind(expr: &IrExpr) -> TopLetKind {
    if is_const_expr(expr, &std::collections::HashSet::new()) { TopLetKind::Const } else { TopLetKind::Lazy }
}

/// Reclassify top-level lets using a two-pass approach:
/// Pass 1: classify without cross-references (already done during lowering).
/// Pass 2: with known const VarIds, reclassify Lazy → Const for expressions
///          that reference other const top-level lets (e.g., `4.0 * PI * PI`).
pub fn reclassify_top_lets(program: &mut IrProgram) {
    // Collect VarIds of top_lets already classified as Const
    let mut const_vars: std::collections::HashSet<u32> = program.top_lets.iter()
        .filter(|tl| matches!(tl.kind, TopLetKind::Const))
        .map(|tl| tl.var.0)
        .collect();

    // Iterate until fixpoint (typically 1-2 rounds)
    loop {
        let mut changed = false;
        for tl in &mut program.top_lets {
            if matches!(tl.kind, TopLetKind::Lazy) && is_const_expr(&tl.value, &const_vars) {
                tl.kind = TopLetKind::Const;
                const_vars.insert(tl.var.0);
                changed = true;
            }
        }
        if !changed { break; }
    }
}

/// Check if an expression can be evaluated at compile time (Rust `const`).
/// Recognizes: literals, unary/binary ops on const operands, references to known const vars.
fn is_const_expr(expr: &IrExpr, const_vars: &std::collections::HashSet<u32>) -> bool {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::LitStr { .. } => true,
        IrExprKind::UnOp { operand, .. } => is_const_expr(operand, const_vars),
        IrExprKind::BinOp { left, right, .. } => is_const_expr(left, const_vars) && is_const_expr(right, const_vars),
        IrExprKind::Var { id } => const_vars.contains(&id.0),
        _ => false,
    }
}
