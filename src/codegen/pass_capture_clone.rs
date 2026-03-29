//! CaptureClonePass: pre-clone variables captured by move closures.
//!
//! In Rust, `move |...| { ... }` takes ownership of all captured variables.
//! When the same variable is captured by multiple closures, or used after a
//! closure, the second use causes E0382 (use of moved value).
//!
//! This pass wraps each lambda in a block that clones captured variables:
//!
//!   Before:  (lambda using `tag`)
//!     move |x| { f(x, tag) }
//!
//!   After:   (wrapped in block with pre-clone)
//!     { let __cap_5 = tag; move |x| { f(x, __cap_5) } }
//!
//! CloneInsertionPass (which runs after) adds .clone() to the Var references.
//! The net effect: each lambda captures its own clone, original stays alive.

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct CaptureClonePass;

impl NanoPass for CaptureClonePass {
    fn name(&self) -> &str { "CaptureClone" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            let param_vars: HashSet<VarId> = func.params.iter().map(|p| p.var).collect();
            if transform_expr(&mut func.body, &mut program.var_table, &param_vars) {
                changed = true;
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let param_vars: HashSet<VarId> = func.params.iter().map(|p| p.var).collect();
                if transform_expr(&mut func.body, &mut module.var_table, &param_vars) {
                    changed = true;
                }
            }
        }
        PassResult { program, changed }
    }
}

/// Collect all variables bound by a statement (Bind + BindDestructure).
fn collect_stmt_bindings(stmt: &IrStmt, out: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, .. } => { out.insert(*var); }
        IrStmtKind::BindDestructure { pattern, .. } => collect_pattern_bindings_into(pattern, out),
        _ => {}
    }
}

/// Collect variables bound by a pattern into a VarId set.
fn collect_pattern_bindings_into(pattern: &IrPattern, out: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { out.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_pattern_bindings_into(a, out); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { collect_pattern_bindings_into(e, out); }
        }
        IrPattern::Some { inner, .. } | IrPattern::Ok { inner, .. } | IrPattern::Err { inner, .. } => {
            collect_pattern_bindings_into(inner, out);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_bindings_into(p, out); }
            }
        }
        _ => {}
    }
}

/// Walk the IR tree. When we find a Lambda that captures clone-worthy outer
/// variables, wrap it in a block with pre-clone bindings.
fn transform_expr(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let mut changed = false;

    // First, recurse into children (bottom-up so inner lambdas are processed first)
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Collect vars defined in this block to extend scope
            let mut local_scope = scope_vars.clone();
            for stmt in stmts.iter() {
                collect_stmt_bindings(stmt, &mut local_scope);
            }
            for stmt in stmts.iter_mut() {
                if transform_stmt(stmt, vt, &local_scope) { changed = true; }
            }
            if let Some(e) = tail {
                if transform_expr(e, vt, &local_scope) { changed = true; }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if transform_expr(cond, vt, scope_vars) { changed = true; }
            if transform_expr(then, vt, scope_vars) { changed = true; }
            if transform_expr(else_, vt, scope_vars) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if transform_expr(subject, vt, scope_vars) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    if transform_expr(g, vt, scope_vars) { changed = true; }
                }
                if transform_expr(&mut arm.body, vt, scope_vars) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, params, .. } => {
            let mut inner_scope = scope_vars.clone();
            for (v, _) in params.iter() { inner_scope.insert(*v); }
            if transform_expr(body, vt, &inner_scope) { changed = true; }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => { if transform_expr(object, vt, scope_vars) { changed = true; } }
                CallTarget::Computed { callee } => { if transform_expr(callee, vt, scope_vars) { changed = true; } }
                _ => {}
            }
            for a in args { if transform_expr(a, vt, scope_vars) { changed = true; } }
        }
        IrExprKind::BinOp { left, right, .. } => {
            if transform_expr(left, vt, scope_vars) { changed = true; }
            if transform_expr(right, vt, scope_vars) { changed = true; }
        }
        IrExprKind::UnOp { operand, .. } => {
            if transform_expr(operand, vt, scope_vars) { changed = true; }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { if transform_expr(e, vt, scope_vars) { changed = true; } }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { if transform_expr(v, vt, scope_vars) { changed = true; } }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            if transform_expr(base, vt, scope_vars) { changed = true; }
            for (_, v) in fields { if transform_expr(v, vt, scope_vars) { changed = true; } }
        }
        IrExprKind::ForIn { iterable, body, var, var_tuple, .. } => {
            if transform_expr(iterable, vt, scope_vars) { changed = true; }
            let mut loop_scope = scope_vars.clone();
            loop_scope.insert(*var);
            if let Some(vt_) = var_tuple { for v in vt_.iter() { loop_scope.insert(*v); } }
            // Collect vars defined in loop body so lambdas can see sibling bindings
            for s in body.iter() { collect_stmt_bindings(s, &mut loop_scope); }
            for s in body { if transform_stmt(s, vt, &loop_scope) { changed = true; } }
        }
        IrExprKind::While { cond, body } => {
            if transform_expr(cond, vt, scope_vars) { changed = true; }
            let mut loop_scope = scope_vars.clone();
            // Collect vars defined in loop body so lambdas can see sibling bindings
            for s in body.iter() { collect_stmt_bindings(s, &mut loop_scope); }
            for s in body { if transform_stmt(s, vt, &loop_scope) { changed = true; } }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p {
                    if transform_expr(e, vt, scope_vars) { changed = true; }
                }
            }
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => {
            if transform_expr(e, vt, scope_vars) { changed = true; }
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            if transform_expr(e, vt, scope_vars) { changed = true; }
            if transform_expr(f, vt, scope_vars) { changed = true; }
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            if transform_expr(object, vt, scope_vars) { changed = true; }
            if transform_expr(index, vt, scope_vars) { changed = true; }
        }
        IrExprKind::Range { start, end, .. } => {
            if transform_expr(start, vt, scope_vars) { changed = true; }
            if transform_expr(end, vt, scope_vars) { changed = true; }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                if transform_expr(k, vt, scope_vars) { changed = true; }
                if transform_expr(v, vt, scope_vars) { changed = true; }
            }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            if transform_expr(object, vt, scope_vars) { changed = true; }
        }
        _ => {}
    }

    // Now check: is this expr itself a Lambda with captured vars that need cloning?
    if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
        let param_set: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
        let mut free_vars = HashSet::new();
        collect_free_vars(body, &param_set, &mut free_vars);

        // Filter to only clone-worthy types from outer scope
        let captures: Vec<VarId> = free_vars.into_iter()
            .filter(|v| scope_vars.contains(v) && needs_clone_type(&vt.get(*v).ty))
            .collect();

        if !captures.is_empty() {
            // Wrap this lambda in a block: { let __cap = var; lambda_with_cap }
            wrap_lambda_with_clones(expr, &captures, vt);
            changed = true;
        }
    }

    changed
}

fn transform_stmt(stmt: &mut IrStmt, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            transform_expr(index, vt, scope_vars) | transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            transform_expr(key, vt, scope_vars) | transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::Guard { cond, else_ } => {
            transform_expr(cond, vt, scope_vars) | transform_expr(else_, vt, scope_vars)
        }
        IrStmtKind::Expr { expr } => transform_expr(expr, vt, scope_vars),
        IrStmtKind::Comment { .. } => false,
    }
}

/// Wrap a Lambda expression in a block that pre-clones captured variables.
///
/// Transforms:
///   move |params| { body using `x` }
/// Into:
///   { let __cap_N = x; move |params| { body using `__cap_N` } }
fn wrap_lambda_with_clones(expr: &mut IrExpr, captures: &[VarId], vt: &mut VarTable) {
    let mut stmts = Vec::new();
    let mut renames = std::collections::HashMap::new();

    for &var_id in captures {
        let ty = vt.get(var_id).ty.clone();
        let cap_name = format!("__cap_{}", var_id.0);
        let cap_var = vt.alloc(
            crate::intern::sym(&cap_name),
            ty.clone(),
            Mutability::Let,
            None,
        );
        renames.insert(var_id, cap_var);

        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: cap_var,
                mutability: Mutability::Let,
                ty: ty.clone(),
                value: IrExpr {
                    kind: IrExprKind::Var { id: var_id },
                    ty,
                    span: None,
                },
            },
            span: None,
        });
    }

    // Rename captured vars inside the lambda body
    if let IrExprKind::Lambda { body, .. } = &mut expr.kind {
        replace_vars(body, &renames);
    }

    // Wrap: { let __cap = var; ...; original_lambda }
    let lambda_expr = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::Unit,
        ty: Ty::Unit,
        span: None,
    });
    let ty = lambda_expr.ty.clone();
    let span = lambda_expr.span;
    *expr = IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(lambda_expr)),
        },
        ty,
        span,
    };
}

// ── Free variable collection ──

fn collect_free_vars(expr: &IrExpr, bound: &HashSet<VarId>, free: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            if !bound.contains(id) { free.insert(*id); }
        }
        IrExprKind::Lambda { params, body, .. } => {
            let mut inner_bound = bound.clone();
            for (v, _) in params { inner_bound.insert(*v); }
            collect_free_vars(body, &inner_bound, free);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            let mut local_bound = bound.clone();
            for stmt in stmts {
                collect_free_vars_stmt(stmt, &local_bound, free);
                if let IrStmtKind::Bind { var, .. } = &stmt.kind {
                    local_bound.insert(*var);
                }
            }
            if let Some(e) = tail { collect_free_vars(e, &local_bound, free); }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => collect_free_vars(object, bound, free),
                CallTarget::Computed { callee } => collect_free_vars(callee, bound, free),
                _ => {}
            }
            for a in args { collect_free_vars(a, bound, free); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_free_vars(left, bound, free);
            collect_free_vars(right, bound, free);
        }
        IrExprKind::UnOp { operand, .. } => collect_free_vars(operand, bound, free),
        IrExprKind::If { cond, then, else_ } => {
            collect_free_vars(cond, bound, free);
            collect_free_vars(then, bound, free);
            collect_free_vars(else_, bound, free);
        }
        IrExprKind::Match { subject, arms } => {
            collect_free_vars(subject, bound, free);
            for arm in arms {
                let mut arm_bound = bound.clone();
                collect_pattern_bindings(&arm.pattern, &mut arm_bound);
                if let Some(g) = &arm.guard { collect_free_vars(g, &arm_bound, free); }
                collect_free_vars(&arm.body, &arm_bound, free);
            }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            collect_free_vars(iterable, bound, free);
            let mut loop_bound = bound.clone();
            loop_bound.insert(*var);
            if let Some(vt) = var_tuple { for v in vt { loop_bound.insert(*v); } }
            for s in body { collect_free_vars_stmt(s, &loop_bound, free); }
        }
        IrExprKind::While { cond, body } => {
            collect_free_vars(cond, bound, free);
            for s in body { collect_free_vars_stmt(s, bound, free); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { collect_free_vars(e, bound, free); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { collect_free_vars(v, bound, free); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            collect_free_vars(base, bound, free);
            for (_, v) in fields { collect_free_vars(v, bound, free); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            collect_free_vars(object, bound, free);
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            collect_free_vars(object, bound, free);
            collect_free_vars(index, bound, free);
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => {
            collect_free_vars(e, bound, free);
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            collect_free_vars(e, bound, free);
            collect_free_vars(f, bound, free);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { collect_free_vars(e, bound, free); }
            }
        }
        IrExprKind::Range { start, end, .. } => {
            collect_free_vars(start, bound, free);
            collect_free_vars(end, bound, free);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                collect_free_vars(k, bound, free);
                collect_free_vars(v, bound, free);
            }
        }
        _ => {}
    }
}

fn collect_free_vars_stmt(stmt: &IrStmt, bound: &HashSet<VarId>, free: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            collect_free_vars(value, bound, free);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_free_vars(index, bound, free);
            collect_free_vars(value, bound, free);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            collect_free_vars(key, bound, free);
            collect_free_vars(value, bound, free);
        }
        IrStmtKind::Guard { cond, else_ } => {
            collect_free_vars(cond, bound, free);
            collect_free_vars(else_, bound, free);
        }
        IrStmtKind::Expr { expr } => collect_free_vars(expr, bound, free),
        IrStmtKind::Comment { .. } => {}
    }
}

fn collect_pattern_bindings(pattern: &IrPattern, bound: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { bound.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_pattern_bindings(a, bound); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { collect_pattern_bindings(e, bound); }
        }
        IrPattern::Some { inner, .. } | IrPattern::Ok { inner, .. } | IrPattern::Err { inner, .. } => {
            collect_pattern_bindings(inner, bound);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_bindings(p, bound); }
            }
        }
        _ => {}
    }
}

// ── Variable replacement ──

fn replace_vars(expr: &mut IrExpr, renames: &std::collections::HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Var { id } => {
            if let Some(&new_id) = renames.get(id) { *id = new_id; }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => replace_vars(object, renames),
                CallTarget::Computed { callee } => replace_vars(callee, renames),
                _ => {}
            }
            for a in args { replace_vars(a, renames); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            replace_vars(left, renames); replace_vars(right, renames);
        }
        IrExprKind::UnOp { operand, .. } => replace_vars(operand, renames),
        IrExprKind::If { cond, then, else_ } => {
            replace_vars(cond, renames); replace_vars(then, renames); replace_vars(else_, renames);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { replace_vars_stmt(s, renames); }
            if let Some(e) = tail { replace_vars(e, renames); }
        }
        IrExprKind::Lambda { body, .. } => replace_vars(body, renames),
        IrExprKind::Match { subject, arms } => {
            replace_vars(subject, renames);
            for arm in arms {
                if let Some(g) = &mut arm.guard { replace_vars(g, renames); }
                replace_vars(&mut arm.body, renames);
            }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { replace_vars(e, renames); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { replace_vars(v, renames); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            replace_vars(base, renames);
            for (_, v) in fields { replace_vars(v, renames); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => replace_vars(object, renames),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            replace_vars(object, renames); replace_vars(index, renames);
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => replace_vars(e, renames),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            replace_vars(e, renames); replace_vars(f, renames);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { replace_vars(e, renames); }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            replace_vars(iterable, renames);
            for s in body { replace_vars_stmt(s, renames); }
        }
        IrExprKind::While { cond, body } => {
            replace_vars(cond, renames);
            for s in body { replace_vars_stmt(s, renames); }
        }
        IrExprKind::Range { start, end, .. } => {
            replace_vars(start, renames); replace_vars(end, renames);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { replace_vars(k, renames); replace_vars(v, renames); }
        }
        _ => {}
    }
}

fn replace_vars_stmt(stmt: &mut IrStmt, renames: &std::collections::HashMap<VarId, VarId>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            replace_vars(value, renames);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            replace_vars(index, renames); replace_vars(value, renames);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            replace_vars(key, renames); replace_vars(value, renames);
        }
        IrStmtKind::Guard { cond, else_ } => {
            replace_vars(cond, renames); replace_vars(else_, renames);
        }
        IrStmtKind::Expr { expr } => replace_vars(expr, renames),
        IrStmtKind::Comment { .. } => {}
    }
}

fn needs_clone_type(ty: &Ty) -> bool {
    matches!(ty,
        Ty::String | Ty::Applied(_, _) |
        Ty::Record { .. } | Ty::OpenRecord { .. } |
        Ty::Named(_, _) |
        Ty::Variant { .. } | Ty::Fn { .. } |
        Ty::TypeVar(_)
    )
}
