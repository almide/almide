//! LICM (Loop-Invariant Code Motion) pass.
//!
//! Identifies expressions inside loops that depend only on variables defined
//! outside the loop and contain no side effects. Hoists them to `let` bindings
//! before the loop to avoid redundant re-evaluation.
//!
//! Target: all targets (target-independent optimization).

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct LICMPass;

impl NanoPass for LICMPass {
    fn name(&self) -> &str { "LICM" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if hoist_loops(&mut func.body, &mut program.var_table) {
                changed = true;
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if hoist_loops(&mut func.body, &mut module.var_table) {
                    changed = true;
                }
            }
        }
        PassResult { program, changed }
    }
}

/// Recursively walk the expression tree looking for loops, hoisting invariants.
/// Returns true if any hoisting was performed.
fn hoist_loops(expr: &mut IrExpr, vt: &mut VarTable) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts: Vec<IrStmt> = Vec::new();
            for mut stmt in std::mem::take(stmts) {
                if hoist_loops_stmt(&mut stmt, vt) {
                    changed = true;
                }
                // If this stmt is an Expr containing a loop, try to hoist invariants
                if let IrStmtKind::Expr { expr: ref mut loop_expr } = stmt.kind {
                    let hoisted = try_hoist_from_loop(loop_expr, vt);
                    if !hoisted.is_empty() {
                        changed = true;
                        new_stmts.extend(hoisted);
                    }
                }
                new_stmts.push(stmt);
            }
            *stmts = new_stmts;
            if let Some(e) = tail {
                if hoist_loops(e, vt) {
                    changed = true;
                }
            }
        }
        IrExprKind::DoBlock { stmts, expr: tail } => {
            let mut new_stmts: Vec<IrStmt> = Vec::new();
            for mut stmt in std::mem::take(stmts) {
                if hoist_loops_stmt(&mut stmt, vt) {
                    changed = true;
                }
                if let IrStmtKind::Expr { expr: ref mut loop_expr } = stmt.kind {
                    let hoisted = try_hoist_from_loop(loop_expr, vt);
                    if !hoisted.is_empty() {
                        changed = true;
                        new_stmts.extend(hoisted);
                    }
                }
                new_stmts.push(stmt);
            }
            *stmts = new_stmts;
            if let Some(e) = tail {
                if hoist_loops(e, vt) {
                    changed = true;
                }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if hoist_loops(cond, vt) { changed = true; }
            if hoist_loops(then, vt) { changed = true; }
            if hoist_loops(else_, vt) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if hoist_loops(subject, vt) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    if hoist_loops(g, vt) { changed = true; }
                }
                if hoist_loops(&mut arm.body, vt) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if hoist_loops(body, vt) { changed = true; }
        }
        // ForIn/While at the top level of an expression (not inside a Block stmt):
        // We can't hoist before them here since there's no statement list to insert into.
        // They're handled when they appear as Expr stmts inside blocks.
        IrExprKind::ForIn { body, iterable, .. } => {
            if hoist_loops(iterable, vt) { changed = true; }
            for s in body {
                if hoist_loops_stmt(s, vt) { changed = true; }
            }
        }
        IrExprKind::While { cond, body } => {
            if hoist_loops(cond, vt) { changed = true; }
            for s in body {
                if hoist_loops_stmt(s, vt) { changed = true; }
            }
        }
        _ => {}
    }
    changed
}

fn hoist_loops_stmt(stmt: &mut IrStmt, vt: &mut VarTable) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            hoist_loops(value, vt)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            hoist_loops(index, vt) | hoist_loops(value, vt)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            hoist_loops(key, vt) | hoist_loops(value, vt)
        }
        IrStmtKind::Guard { cond, else_ } => {
            hoist_loops(cond, vt) | hoist_loops(else_, vt)
        }
        IrStmtKind::Expr { expr } => hoist_loops(expr, vt),
        IrStmtKind::Comment { .. } => false,
    }
}

/// Given a loop expression (ForIn or While), extract loop-invariant expressions
/// from its body and return them as `let` binding statements to insert before
/// the loop. The original expressions in the body are replaced with Var references.
fn try_hoist_from_loop(expr: &mut IrExpr, vt: &mut VarTable) -> Vec<IrStmt> {
    let mut hoisted = Vec::new();

    match &mut expr.kind {
        IrExprKind::ForIn { var, var_tuple, body, .. } => {
            // Collect all VarIds defined inside the loop
            let mut loop_defined = HashSet::new();
            loop_defined.insert(*var);
            if let Some(vars) = var_tuple {
                for v in vars {
                    loop_defined.insert(*v);
                }
            }
            collect_defined_vars_stmts(body, &mut loop_defined);

            // Scan body statements for invariant expressions to hoist
            for stmt in body.iter_mut() {
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted);
            }
        }
        IrExprKind::While { cond: _, body } => {
            let mut loop_defined = HashSet::new();
            collect_defined_vars_stmts(body, &mut loop_defined);

            for stmt in body.iter_mut() {
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted);
            }
        }
        _ => {}
    }

    hoisted
}

/// Collect all VarIds that are bound OR assigned within a list of statements.
/// This includes `let` bindings AND `var` reassignments — any variable modified
/// inside the loop is NOT loop-invariant.
fn collect_defined_vars_stmts(stmts: &[IrStmt], defined: &mut HashSet<VarId>) {
    for stmt in stmts {
        match &stmt.kind {
            IrStmtKind::Bind { var, .. } => { defined.insert(*var); }
            IrStmtKind::Assign { var, .. } => {
                // `var x` assigned inside the loop — x is loop-modified
                defined.insert(*var);
            }
            IrStmtKind::Expr { expr } => collect_defined_vars_expr(expr, defined),
            IrStmtKind::Guard { cond, else_ } => {
                collect_defined_vars_expr(cond, defined);
                collect_defined_vars_expr(else_, defined);
            }
            _ => {}
        }
    }
}

fn collect_defined_vars_expr(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            collect_defined_vars_stmts(stmts, defined);
            if let Some(e) = tail { collect_defined_vars_expr(e, defined); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_defined_vars_expr(cond, defined);
            collect_defined_vars_expr(then, defined);
            collect_defined_vars_expr(else_, defined);
        }
        IrExprKind::ForIn { var, var_tuple, body, iterable } => {
            defined.insert(*var);
            if let Some(vars) = var_tuple {
                for v in vars { defined.insert(*v); }
            }
            collect_defined_vars_expr(iterable, defined);
            collect_defined_vars_stmts(body, defined);
        }
        IrExprKind::While { cond, body } => {
            collect_defined_vars_expr(cond, defined);
            collect_defined_vars_stmts(body, defined);
        }
        IrExprKind::Match { subject, arms } => {
            collect_defined_vars_expr(subject, defined);
            for arm in arms {
                collect_defined_vars_expr(&arm.body, defined);
            }
        }
        IrExprKind::Lambda { body, params, .. } => {
            for (v, _) in params { defined.insert(*v); }
            collect_defined_vars_expr(body, defined);
        }
        _ => {}
    }
}

/// Try to extract invariant sub-expressions from a statement's value.
/// If the value of a Bind or Expr statement is loop-invariant, hoist it.
fn extract_invariants_from_stmt(
    stmt: &mut IrStmt,
    loop_defined: &HashSet<VarId>,
    vt: &mut VarTable,
    hoisted: &mut Vec<IrStmt>,
) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => {
            try_hoist_expr(value, loop_defined, vt, hoisted);
        }
        IrStmtKind::Expr { expr } => {
            try_hoist_expr(expr, loop_defined, vt, hoisted);
        }
        // Don't hoist the whole RHS of assignments — the assignment itself
        // is a side effect (mutates a var). Only recurse into sub-expressions
        // if the RHS is complex enough to have hoistable sub-parts.
        IrStmtKind::Assign { .. } => {
            // Skip: assignment targets are loop-modified vars.
            // The RHS often references the target var (e.g., count = count + 1)
            // which is in loop_defined, so hoisting would be wrong anyway.
        }
        IrStmtKind::Guard { cond, else_ } => {
            try_hoist_expr(cond, loop_defined, vt, hoisted);
            try_hoist_expr(else_, loop_defined, vt, hoisted);
        }
        _ => {}
    }
}

/// If `expr` is loop-invariant and non-trivial, replace it with a Var reference
/// and push the original expression as a hoisted `let` binding.
/// Also recurses into sub-expressions to find hoistable parts.
fn try_hoist_expr(
    expr: &mut IrExpr,
    loop_defined: &HashSet<VarId>,
    vt: &mut VarTable,
    hoisted: &mut Vec<IrStmt>,
) {
    // Check if the whole expression is hoistable
    if is_hoistable(expr, loop_defined) {
        let ty = expr.ty.clone();
        let var = vt.alloc("__licm".to_string(), ty.clone(), Mutability::Let, None);
        let original = std::mem::replace(expr, IrExpr {
            kind: IrExprKind::Var { id: var },
            ty: ty.clone(),
            span: expr.span,
        });
        hoisted.push(IrStmt {
            kind: IrStmtKind::Bind {
                var,
                mutability: Mutability::Let,
                ty,
                value: original,
            },
            span: None,
        });
        return;
    }

    // Otherwise, recurse into sub-expressions to find hoistable parts
    match &mut expr.kind {
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => try_hoist_expr(object, loop_defined, vt, hoisted),
                CallTarget::Computed { callee } => try_hoist_expr(callee, loop_defined, vt, hoisted),
                _ => {}
            }
            for arg in args {
                try_hoist_expr(arg, loop_defined, vt, hoisted);
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            try_hoist_expr(left, loop_defined, vt, hoisted);
            try_hoist_expr(right, loop_defined, vt, hoisted);
        }
        IrExprKind::UnOp { operand, .. } => {
            try_hoist_expr(operand, loop_defined, vt, hoisted);
        }
        IrExprKind::If { cond, then, else_ } => {
            try_hoist_expr(cond, loop_defined, vt, hoisted);
            try_hoist_expr(then, loop_defined, vt, hoisted);
            try_hoist_expr(else_, loop_defined, vt, hoisted);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements {
                try_hoist_expr(e, loop_defined, vt, hoisted);
            }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields {
                try_hoist_expr(v, loop_defined, vt, hoisted);
            }
        }
        IrExprKind::Member { object, .. } => {
            try_hoist_expr(object, loop_defined, vt, hoisted);
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            try_hoist_expr(object, loop_defined, vt, hoisted);
            try_hoist_expr(index, loop_defined, vt, hoisted);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr: e } = part {
                    try_hoist_expr(e, loop_defined, vt, hoisted);
                }
            }
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } => {
            try_hoist_expr(e, loop_defined, vt, hoisted);
        }
        IrExprKind::Range { start, end, .. } => {
            try_hoist_expr(start, loop_defined, vt, hoisted);
            try_hoist_expr(end, loop_defined, vt, hoisted);
        }
        _ => {}
    }
}

/// An expression is hoistable if:
/// 1. All referenced variables are defined OUTSIDE the loop (not in `loop_defined`)
/// 2. It contains no calls to effect functions (side effects)
/// 3. It is not trivially cheap (skip Var, Lit*, Unit)
/// 4. It contains no control flow (loops, continue, break, return)
fn is_hoistable(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    if is_trivial(expr) {
        return false;
    }
    if has_effect_call(expr) {
        return false;
    }
    if has_control_flow(expr) {
        return false;
    }
    refs_are_outside_loop(expr, loop_defined)
}

/// Returns true if the expression contains loops, continue, break, or return.
/// These must never be hoisted out of their enclosing scope.
fn has_control_flow(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::ForIn { .. } | IrExprKind::While { .. } => true,
        IrExprKind::Continue | IrExprKind::Break => true,
        IrExprKind::BinOp { left, right, .. } => {
            has_control_flow(left) || has_control_flow(right)
        }
        IrExprKind::UnOp { operand, .. } => has_control_flow(operand),
        IrExprKind::Call { target, args, .. } => {
            let target_cf = match target {
                CallTarget::Method { object, .. } => has_control_flow(object),
                CallTarget::Computed { callee } => has_control_flow(callee),
                _ => false,
            };
            target_cf || args.iter().any(|a| has_control_flow(a))
        }
        IrExprKind::If { cond, then, else_ } => {
            has_control_flow(cond) || has_control_flow(then) || has_control_flow(else_)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            stmts.iter().any(|s| has_control_flow_stmt(s))
                || expr.as_ref().is_some_and(|e| has_control_flow(e))
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| has_control_flow(e))
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => {
            has_control_flow(e)
        }
        _ => false,
    }
}

fn has_control_flow_stmt(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. }
        | IrStmtKind::FieldAssign { value, .. } => has_control_flow(value),
        IrStmtKind::Expr { expr } => has_control_flow(expr),
        IrStmtKind::Guard { cond, else_ } => has_control_flow(cond) || has_control_flow(else_),
        _ => false,
    }
}

/// Returns true if the expression is trivially cheap (not worth hoisting).
fn is_trivial(expr: &IrExpr) -> bool {
    matches!(
        &expr.kind,
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::OptionNone
        | IrExprKind::FnRef { .. }
    )
}

/// Returns true if the expression contains any function call that could have side effects.
/// Conservatively, we consider Module calls to effect-capable modules and all
/// Method/Computed calls as potentially effectful.
fn has_effect_call(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let call_is_effectful = match target {
                // Module calls to known effectful modules
                CallTarget::Module { module, .. } => {
                    matches!(module.as_str(), "fs" | "path" | "http" | "url" | "env"
                        | "process" | "time" | "datetime" | "fan" | "log")
                }
                // Named calls to runtime effect functions
                CallTarget::Named { name } => {
                    name.starts_with("almide_rt_fs_")
                    || name.starts_with("almide_rt_http_")
                    || name.starts_with("almide_rt_env_")
                    || name.starts_with("almide_rt_time_")
                    || name.starts_with("almide_rt_log_")
                    || name.starts_with("almide_rt_fan_")
                    || name.starts_with("almide_rt_process_")
                    || name == "println"
                }
                // Method/Computed calls are conservatively considered effectful
                CallTarget::Method { .. } | CallTarget::Computed { .. } => true,
            };
            if call_is_effectful {
                return true;
            }
            args.iter().any(|a| has_effect_call(a))
        }
        IrExprKind::BinOp { left, right, .. } => {
            has_effect_call(left) || has_effect_call(right)
        }
        IrExprKind::UnOp { operand, .. } => has_effect_call(operand),
        IrExprKind::If { cond, then, else_ } => {
            has_effect_call(cond) || has_effect_call(then) || has_effect_call(else_)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            stmts.iter().any(|s| has_effect_call_stmt(s))
                || expr.as_ref().is_some_and(|e| has_effect_call(e))
        }
        IrExprKind::Match { subject, arms } => {
            has_effect_call(subject)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(|g| has_effect_call(g))
                        || has_effect_call(&a.body)
                })
        }
        IrExprKind::Lambda { body, .. } => has_effect_call(body),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            elements.iter().any(|e| has_effect_call(e))
        }
        IrExprKind::Record { fields, .. } => fields.iter().any(|(_, v)| has_effect_call(v)),
        IrExprKind::SpreadRecord { base, fields } => {
            has_effect_call(base) || fields.iter().any(|(_, v)| has_effect_call(v))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            has_effect_call(object)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            has_effect_call(object) || has_effect_call(index)
        }
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Try { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            has_effect_call(expr)
        }
        IrExprKind::StringInterp { parts } => {
            parts.iter().any(|p| matches!(p, IrStringPart::Expr { expr } if has_effect_call(expr)))
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().any(|(k, v)| has_effect_call(k) || has_effect_call(v))
        }
        IrExprKind::Range { start, end, .. } => {
            has_effect_call(start) || has_effect_call(end)
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            has_effect_call(iterable) || body.iter().any(|s| has_effect_call_stmt(s))
        }
        IrExprKind::While { cond, body } => {
            has_effect_call(cond) || body.iter().any(|s| has_effect_call_stmt(s))
        }
        IrExprKind::RustMacro { args, .. } => args.iter().any(|a| has_effect_call(a)),
        // Leaf nodes are pure
        _ => false,
    }
}

fn has_effect_call_stmt(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            has_effect_call(value)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            has_effect_call(index) || has_effect_call(value)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            has_effect_call(key) || has_effect_call(value)
        }
        IrStmtKind::Guard { cond, else_ } => {
            has_effect_call(cond) || has_effect_call(else_)
        }
        IrStmtKind::Expr { expr } => has_effect_call(expr),
        IrStmtKind::Comment { .. } => false,
    }
}

/// Returns true if all variable references in the expression are outside the loop
/// (i.e., none of them are in `loop_defined`).
fn refs_are_outside_loop(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => !loop_defined.contains(id),
        IrExprKind::Call { target, args, .. } => {
            let target_ok = match target {
                CallTarget::Method { object, .. } => refs_are_outside_loop(object, loop_defined),
                CallTarget::Computed { callee } => refs_are_outside_loop(callee, loop_defined),
                _ => true,
            };
            target_ok && args.iter().all(|a| refs_are_outside_loop(a, loop_defined))
        }
        IrExprKind::BinOp { left, right, .. } => {
            refs_are_outside_loop(left, loop_defined) && refs_are_outside_loop(right, loop_defined)
        }
        IrExprKind::UnOp { operand, .. } => refs_are_outside_loop(operand, loop_defined),
        IrExprKind::If { cond, then, else_ } => {
            refs_are_outside_loop(cond, loop_defined)
                && refs_are_outside_loop(then, loop_defined)
                && refs_are_outside_loop(else_, loop_defined)
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            stmts.iter().all(|s| refs_are_outside_loop_stmt(s, loop_defined))
                && expr.as_ref().map_or(true, |e| refs_are_outside_loop(e, loop_defined))
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(|e| refs_are_outside_loop(e, loop_defined))
        }
        IrExprKind::Record { fields, .. } => {
            fields.iter().all(|(_, v)| refs_are_outside_loop(v, loop_defined))
        }
        IrExprKind::SpreadRecord { base, fields } => {
            refs_are_outside_loop(base, loop_defined)
                && fields.iter().all(|(_, v)| refs_are_outside_loop(v, loop_defined))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            refs_are_outside_loop(object, loop_defined)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            refs_are_outside_loop(object, loop_defined)
                && refs_are_outside_loop(index, loop_defined)
        }
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Try { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } => {
            refs_are_outside_loop(expr, loop_defined)
        }
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
                _ => true,
            })
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| {
                refs_are_outside_loop(k, loop_defined) && refs_are_outside_loop(v, loop_defined)
            })
        }
        IrExprKind::Range { start, end, .. } => {
            refs_are_outside_loop(start, loop_defined)
                && refs_are_outside_loop(end, loop_defined)
        }
        IrExprKind::Lambda { body, params, .. } => {
            // Lambda params are local — don't count them as loop-defined.
            // But the lambda body's free variables still matter.
            // For simplicity, consider the whole lambda as not depending on loop vars
            // if its free variables don't reference loop-defined vars.
            // We need to exclude params from the check.
            let mut extended = loop_defined.clone();
            for (v, _) in params { extended.remove(v); }
            refs_are_outside_loop(body, &extended)
        }
        IrExprKind::Match { subject, arms } => {
            refs_are_outside_loop(subject, loop_defined)
                && arms.iter().all(|a| {
                    a.guard.as_ref().map_or(true, |g| refs_are_outside_loop(g, loop_defined))
                        && refs_are_outside_loop(&a.body, loop_defined)
                })
        }
        // Leaf nodes (LitInt, LitFloat, LitStr, LitBool, Unit, OptionNone, etc.)
        _ => true,
    }
}

fn refs_are_outside_loop_stmt(stmt: &IrStmt, loop_defined: &HashSet<VarId>) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            refs_are_outside_loop(index, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            refs_are_outside_loop(key, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::Guard { cond, else_ } => {
            refs_are_outside_loop(cond, loop_defined) && refs_are_outside_loop(else_, loop_defined)
        }
        IrStmtKind::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
        IrStmtKind::Comment { .. } => true,
    }
}
