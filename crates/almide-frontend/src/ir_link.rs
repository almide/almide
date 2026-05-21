// ── IR Linker (Phase 1) ──────────────────────────────────────────────
//
// Explicit merge point for dependency modules. Currently performs:
// 1. Collect used_stdlib_modules across all modules (for runtime inclusion)
// 2. Validate module structure
//
// Phase 2 (future): flatten modules into root program, removing the need
// for walker's per-module prefix rendering. Requires updating all call
// targets and the walker simultaneously.

use almide_ir::*;
use std::collections::HashSet;

/// Prepare the IR for codegen. Currently collects cross-module metadata.
/// Future: merge modules into root (flatten).
pub fn ir_link(program: &mut IrProgram) {
    // Collect stdlib modules used across root + all dependency modules.
    // This replaces the text-search runtime inclusion in codegen.
    let mut stdlib_modules = std::mem::take(&mut program.used_stdlib_modules);
    for module in &program.modules {
        for func in &module.functions {
            scan_expr_stdlib(&func.body, &mut stdlib_modules);
        }
        for tl in &module.top_lets {
            scan_expr_stdlib(&tl.value, &mut stdlib_modules);
        }
    }
    program.used_stdlib_modules = stdlib_modules;
}

/// Scan an expression tree for stdlib module references.
fn scan_expr_stdlib(expr: &IrExpr, used: &mut HashSet<String>) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Module { module, .. } = target {
                used.insert(module.to_string());
            }
            if let CallTarget::Method { object, .. } = target {
                scan_expr_stdlib(object, used);
            }
            for a in args { scan_expr_stdlib(a, used); }
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            if let Some(rest) = symbol.as_str().strip_prefix("almide_rt_") {
                if let Some(pos) = rest.find('_') {
                    used.insert(rest[..pos].to_string());
                }
            }
            for a in args { scan_expr_stdlib(a, used); }
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { scan_stmt_stdlib(s, used); }
            if let Some(e) = tail { scan_expr_stdlib(e, used); }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_expr_stdlib(cond, used);
            scan_expr_stdlib(then, used);
            scan_expr_stdlib(else_, used);
        }
        IrExprKind::Match { subject, arms } => {
            scan_expr_stdlib(subject, used);
            for arm in arms {
                if let Some(g) = &arm.guard { scan_expr_stdlib(g, used); }
                scan_expr_stdlib(&arm.body, used);
            }
        }
        IrExprKind::Lambda { body, .. } => scan_expr_stdlib(body, used),
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_expr_stdlib(iterable, used);
            for s in body { scan_stmt_stdlib(s, used); }
        }
        IrExprKind::While { cond, body } => {
            scan_expr_stdlib(cond, used);
            for s in body { scan_stmt_stdlib(s, used); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            scan_expr_stdlib(left, used);
            scan_expr_stdlib(right, used);
        }
        IrExprKind::UnOp { operand, .. } => scan_expr_stdlib(operand, used),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { scan_expr_stdlib(e, used); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { scan_expr_stdlib(v, used); }
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
        | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Member { object: e, .. } => scan_expr_stdlib(e, used),
        IrExprKind::UnwrapOr { expr: e, fallback } => {
            scan_expr_stdlib(e, used);
            scan_expr_stdlib(fallback, used);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { scan_expr_stdlib(expr, used); }
            }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            scan_expr_stdlib(base, used);
            for (_, v) in fields { scan_expr_stdlib(v, used); }
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::Range { start: object, end: index, .. } => {
            scan_expr_stdlib(object, used);
            scan_expr_stdlib(index, used);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { scan_expr_stdlib(k, used); scan_expr_stdlib(v, used); }
        }
        _ => {}
    }
}

fn scan_stmt_stdlib(stmt: &IrStmt, used: &mut HashSet<String>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => scan_expr_stdlib(value, used),
        IrStmtKind::Expr { expr } => scan_expr_stdlib(expr, used),
        IrStmtKind::Guard { cond, else_ } => {
            scan_expr_stdlib(cond, used);
            scan_expr_stdlib(else_, used);
        }
        _ => {}
    }
}
