//! LICM (Loop-Invariant Code Motion) pass.
//!
//! Identifies expressions inside loops that depend only on variables defined
//! outside the loop and contain no side effects. Hoists them to `let` bindings
//! before the loop to avoid redundant re-evaluation.
//!
//! Target: all targets (target-independent optimization).

use std::collections::HashSet;
use crate::intern::Sym;
use crate::ir::*;
use crate::generated::arg_transforms;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct LICMPass;

impl NanoPass for LICMPass {
    fn name(&self) -> &str { "LICM" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Analyze user function purity (fixpoint computation)
        let pure_fns = analyze_pure_functions(&program);
        let mut changed = false;
        for func in &mut program.functions {
            if hoist_loops(&mut func.body, &mut program.var_table, &pure_fns) {
                changed = true;
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if hoist_loops(&mut func.body, &mut module.var_table, &pure_fns) {
                    changed = true;
                }
            }
        }
        PassResult { program, changed }
    }
}

/// Recursively walk the expression tree looking for loops, hoisting invariants.
/// Returns true if any hoisting was performed.
fn hoist_loops(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts: Vec<IrStmt> = Vec::new();
            for mut stmt in std::mem::take(stmts) {
                if hoist_loops_stmt(&mut stmt, vt, pure_fns) {
                    changed = true;
                }
                // If this stmt is an Expr containing a loop, try to hoist invariants
                if let IrStmtKind::Expr { expr: ref mut loop_expr } = stmt.kind {
                    let hoisted = try_hoist_from_loop(loop_expr, vt, pure_fns);
                    if !hoisted.is_empty() {
                        changed = true;
                        new_stmts.extend(hoisted);
                    }
                }
                new_stmts.push(stmt);
            }
            *stmts = new_stmts;
            if let Some(e) = tail {
                if hoist_loops(e, vt, pure_fns) {
                    changed = true;
                }
            }
        }

        IrExprKind::If { cond, then, else_ } => {
            if hoist_loops(cond, vt, pure_fns) { changed = true; }
            if hoist_loops(then, vt, pure_fns) { changed = true; }
            if hoist_loops(else_, vt, pure_fns) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if hoist_loops(subject, vt, pure_fns) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    if hoist_loops(g, vt, pure_fns) { changed = true; }
                }
                if hoist_loops(&mut arm.body, vt, pure_fns) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if hoist_loops(body, vt, pure_fns) { changed = true; }
        }
        // ForIn/While at the top level of an expression (not inside a Block stmt):
        // We can't hoist before them here since there's no statement list to insert into.
        // They're handled when they appear as Expr stmts inside blocks.
        IrExprKind::ForIn { body, iterable, .. } => {
            if hoist_loops(iterable, vt, pure_fns) { changed = true; }
            for s in body {
                if hoist_loops_stmt(s, vt, pure_fns) { changed = true; }
            }
        }
        IrExprKind::While { cond, body } => {
            if hoist_loops(cond, vt, pure_fns) { changed = true; }
            for s in body {
                if hoist_loops_stmt(s, vt, pure_fns) { changed = true; }
            }
        }
        _ => {}
    }
    changed
}

fn hoist_loops_stmt(stmt: &mut IrStmt, vt: &mut VarTable, pure_fns: &HashSet<Sym>) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            hoist_loops(value, vt, pure_fns)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            hoist_loops(index, vt, pure_fns) | hoist_loops(value, vt, pure_fns)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            hoist_loops(key, vt, pure_fns) | hoist_loops(value, vt, pure_fns)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            hoist_loops(a, vt, pure_fns) | hoist_loops(b, vt, pure_fns)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            hoist_loops(end, vt, pure_fns)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            hoist_loops(len, vt, pure_fns)
        }
        IrStmtKind::Guard { cond, else_ } => {
            hoist_loops(cond, vt, pure_fns) | hoist_loops(else_, vt, pure_fns)
        }
        IrStmtKind::Expr { expr } => hoist_loops(expr, vt, pure_fns),
        IrStmtKind::Comment { .. } => false,
    }
}

/// Given a loop expression (ForIn or While), extract loop-invariant expressions
/// from its body and return them as `let` binding statements to insert before
/// the loop. The original expressions in the body are replaced with Var references.
fn try_hoist_from_loop(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>) -> Vec<IrStmt> {
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
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns);
            }
        }
        IrExprKind::While { cond: _, body } => {
            let mut loop_defined = HashSet::new();
            collect_defined_vars_stmts(body, &mut loop_defined);

            for stmt in body.iter_mut() {
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns);
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
            IrStmtKind::BindDestructure { pattern, .. } => {
                collect_pattern_defined_vars(pattern, defined);
            }
            IrStmtKind::Assign { var, .. } => {
                // `var x` assigned inside the loop — x is loop-modified
                defined.insert(*var);
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                // `xs[i] = v` — the list/array variable is mutated
                defined.insert(*target);
                collect_defined_vars_expr(index, defined);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                defined.insert(*target);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::MapInsert { target, key, value } => {
                defined.insert(*target);
                collect_defined_vars_expr(key, defined);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::ListSwap { target, a, b } => {
                defined.insert(*target);
                collect_defined_vars_expr(a, defined);
                collect_defined_vars_expr(b, defined);
            }
            IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
                defined.insert(*target);
                collect_defined_vars_expr(end, defined);
            }
            IrStmtKind::ListCopySlice { dst, len, .. } => {
                defined.insert(*dst);
                collect_defined_vars_expr(len, defined);
            }
            IrStmtKind::Expr { expr } => collect_defined_vars_expr(expr, defined),
            IrStmtKind::Guard { cond, else_ } => {
                collect_defined_vars_expr(cond, defined);
                collect_defined_vars_expr(else_, defined);
            }
            IrStmtKind::Comment { .. } => {}
        }
    }
}

/// Collect VarIds bound by an IrPattern (tuple destructuring, constructor patterns, etc.).
fn collect_pattern_defined_vars(pat: &IrPattern, defined: &mut HashSet<VarId>) {
    match pat {
        IrPattern::Bind { var, .. } => { defined.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_pattern_defined_vars(a, defined); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { collect_pattern_defined_vars(e, defined); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_defined_vars(inner, defined);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_defined_vars(p, defined); }
            }
        }
        _ => {}
    }
}

fn collect_defined_vars_expr(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
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
        // Mutating stdlib calls: first BorrowMut arg is the target variable
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } => {
            if let Some(info) = arg_transforms::lookup(module, func) {
                for (i, arg) in args.iter().enumerate() {
                    if info.args.get(i) == Some(&arg_transforms::ArgTransform::BorrowMut) {
                        if let IrExprKind::Var { id } = &arg.kind {
                            defined.insert(*id);
                        }
                    }
                }
            }
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
    pure_fns: &HashSet<Sym>,
) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => {
            try_hoist_expr(value, loop_defined, vt, hoisted, pure_fns);
        }
        IrStmtKind::Expr { expr } => {
            try_hoist_expr(expr, loop_defined, vt, hoisted, pure_fns);
        }
        // Don't hoist the whole RHS of assignments — the assignment itself
        // is a side effect (mutates a var). Only recurse into sub-expressions
        // if the RHS is complex enough to have hoistable sub-parts.
        IrStmtKind::Assign { value, .. } => {
            // The assignment itself stays in the loop, but sub-expressions of
            // the RHS may be hoistable (e.g., total = total + square(n) → hoist square(n)).
            try_hoist_expr(value, loop_defined, vt, hoisted, pure_fns);
        }
        IrStmtKind::Guard { cond, else_ } => {
            try_hoist_expr(cond, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(else_, loop_defined, vt, hoisted, pure_fns);
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
    pure_fns: &HashSet<Sym>,
) {
    // Check if the whole expression is hoistable
    if is_hoistable(expr, loop_defined, pure_fns) {
        let ty = expr.ty.clone();
        let var = vt.alloc("__licm".into(), ty.clone(), Mutability::Let, None);
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
                CallTarget::Method { object, .. } => try_hoist_expr(object, loop_defined, vt, hoisted, pure_fns),
                CallTarget::Computed { callee } => try_hoist_expr(callee, loop_defined, vt, hoisted, pure_fns),
                _ => {}
            }
            for arg in args {
                try_hoist_expr(arg, loop_defined, vt, hoisted, pure_fns);
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            try_hoist_expr(left, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(right, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::UnOp { operand, .. } => {
            try_hoist_expr(operand, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::If { cond, then, else_ } => {
            try_hoist_expr(cond, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(then, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(else_, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements {
                try_hoist_expr(e, loop_defined, vt, hoisted, pure_fns);
            }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields {
                try_hoist_expr(v, loop_defined, vt, hoisted, pure_fns);
            }
        }
        IrExprKind::Member { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            try_hoist_expr(object, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            try_hoist_expr(object, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(index, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr: e } = part {
                    try_hoist_expr(e, loop_defined, vt, hoisted, pure_fns);
                }
            }
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } => {
            try_hoist_expr(e, loop_defined, vt, hoisted, pure_fns);
        }
        IrExprKind::Range { start, end, .. } => {
            try_hoist_expr(start, loop_defined, vt, hoisted, pure_fns);
            try_hoist_expr(end, loop_defined, vt, hoisted, pure_fns);
        }
        _ => {}
    }
}

/// An expression is hoistable if:
/// 1. All referenced variables are defined OUTSIDE the loop (not in `loop_defined`)
/// 2. It contains no calls to effect functions (side effects)
/// 3. It is not trivially cheap (skip Var, Lit*, Unit)
/// 4. It contains no control flow (loops, continue, break, return)
fn is_hoistable(expr: &IrExpr, loop_defined: &HashSet<VarId>, pure_fns: &HashSet<Sym>) -> bool {
    if is_trivial(expr) {
        return false;
    }
    if !is_pure(expr, pure_fns) {
        return false;
    }
    if has_control_flow(expr) {
        return false;
    }
    if has_assignment(expr) {
        return false;
    }
    refs_are_outside_loop(expr, loop_defined)
}

/// Returns true if the expression contains variable assignments.
/// Assignments are side effects that must not be hoisted out of loops.
fn has_assignment(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Assign { .. } | IrStmtKind::FieldAssign { .. } | IrStmtKind::IndexAssign { .. }) || has_assignment_stmt(s))
                || tail.as_ref().map_or(false, |e| has_assignment(e))
        }
        IrExprKind::If { cond, then, else_ } => has_assignment(cond) || has_assignment(then) || has_assignment(else_),
        IrExprKind::Match { subject, arms } => has_assignment(subject) || arms.iter().any(|a| has_assignment(&a.body)),
        _ => false,
    }
}

fn has_assignment_stmt(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Assign { .. } | IrStmtKind::FieldAssign { .. } | IrStmtKind::IndexAssign { .. } => true,
        IrStmtKind::Expr { expr } => has_assignment(expr),
        IrStmtKind::Bind { value, .. } => has_assignment(value),
        _ => false,
    }
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
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| has_control_flow_stmt(s))
                || expr.as_ref().is_some_and(|e| has_control_flow(e))
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| has_control_flow(e))
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::OptionalChain { expr: e, .. } => {
            has_control_flow(e)
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            has_control_flow(e) || has_control_flow(f)
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

/// Returns true if the expression is trivially cheap or should not be hoisted.
/// Lambda is included because closures rely on call-site context for type
/// inference in Rust — hoisting them to a standalone `let` binding strips
/// that context and causes `rustc` type annotation errors.
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
        | IrExprKind::Lambda { .. }
    )
}
/// Returns true if the expression is pure (no function calls, no I/O, no mutation).
/// Only pure expressions can be hoisted out of loops.
/// Conservative: any function call makes the expression impure.
fn is_pure(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    match &expr.kind {
        // Leaf nodes: always pure
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        // Function calls: pure if target is known-pure and all args are pure.
        IrExprKind::Call { target, args, .. } => {
            let call_pure = match target {
                CallTarget::Module { module, func } => is_pure_stdlib_call(module, func),
                CallTarget::Named { name } => pure_fns.contains(name),
                _ => false,
            };
            call_pure && args.iter().all(|a| is_pure(a, pure_fns))
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // Operators: pure if operands are pure
        IrExprKind::BinOp { left, right, .. } => is_pure(left, pure_fns) && is_pure(right, pure_fns),
        IrExprKind::UnOp { operand, .. } => is_pure(operand, pure_fns),

        // Collection constructors: pure if elements are pure
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter().all(|e| is_pure(e, pure_fns)),
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| is_pure(v, pure_fns)),
        IrExprKind::SpreadRecord { base, fields } => is_pure(base, pure_fns) && fields.iter().all(|(_, v)| is_pure(v, pure_fns)),
        IrExprKind::MapLiteral { entries } => entries.iter().all(|(k, v)| is_pure(k, pure_fns) && is_pure(v, pure_fns)),
        IrExprKind::Range { start, end, .. } => is_pure(start, pure_fns) && is_pure(end, pure_fns),

        // Access: pure if sub-exprs are pure
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => is_pure(object, pure_fns),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            is_pure(object, pure_fns) && is_pure(index, pure_fns)
        }

        // Wrappers: pure if inner is pure
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::ToVec { expr } => is_pure(expr, pure_fns),
        IrExprKind::UnwrapOr { expr, fallback } => is_pure(expr, pure_fns) && is_pure(fallback, pure_fns),

        // String interpolation: pure if all parts are pure
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => is_pure(expr, pure_fns),
                _ => true,
            })
        }

        // Everything else: conservatively impure
        _ => false,
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
        IrExprKind::Block { stmts, expr } => {
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
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            refs_are_outside_loop(object, loop_defined)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            refs_are_outside_loop(object, loop_defined)
                && refs_are_outside_loop(index, loop_defined)
        }
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } => {
            refs_are_outside_loop(expr, loop_defined)
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            refs_are_outside_loop(expr, loop_defined)
                && refs_are_outside_loop(fallback, loop_defined)
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
        IrStmtKind::ListSwap { a, b, .. } => {
            refs_are_outside_loop(a, loop_defined) && refs_are_outside_loop(b, loop_defined)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            refs_are_outside_loop(end, loop_defined)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            refs_are_outside_loop(len, loop_defined)
        }
        IrStmtKind::Guard { cond, else_ } => {
            refs_are_outside_loop(cond, loop_defined) && refs_are_outside_loop(else_, loop_defined)
        }
        IrStmtKind::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
        IrStmtKind::Comment { .. } => true,
    }
}

/// Derive purity of a stdlib call from its TOML-generated metadata.
/// The `pure_` field is auto-derived at build time:
/// pure = not effect, not impure, no BorrowMut args.
fn is_pure_stdlib_call(module: &str, func: &str) -> bool {
    arg_transforms::lookup(module, func).is_some_and(|info| info.pure_)
}

// ── User function purity analysis (fixpoint) ──────────────────

/// Analyze all user functions and return the set of names that are pure.
/// A function is pure if its body contains no impure operations.
/// Uses fixpoint iteration: mark impure functions, propagate, repeat until stable.
fn analyze_pure_functions(program: &IrProgram) -> HashSet<Sym> {
    use crate::intern::sym;

    // Collect all function names
    let mut all_fns: HashSet<Sym> = HashSet::new();
    let mut fn_bodies: Vec<(Sym, &IrExpr)> = Vec::new();
    for func in &program.functions {
        all_fns.insert(func.name);
        fn_bodies.push((func.name, &func.body));
    }
    for module in &program.modules {
        for func in &module.functions {
            all_fns.insert(func.name);
            fn_bodies.push((func.name, &func.body));
        }
    }

    // Start: assume all functions are pure
    let mut pure_set = all_fns.clone();

    // Fixpoint: remove functions whose body is impure, repeat until stable
    loop {
        let mut changed = false;
        for &(name, body) in &fn_bodies {
            if !pure_set.contains(&name) { continue; }
            if !expr_is_pure_with(body, &pure_set) {
                pure_set.remove(&name);
                changed = true;
            }
        }
        if !changed { break; }
    }

    pure_set
}

/// Check if an expression is pure given a current set of known-pure user functions.
/// Similar to `is_pure` but works on immutable IR (no VarTable needed).
fn expr_is_pure_with(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        IrExprKind::Call { target, args, .. } => {
            let call_pure = match target {
                CallTarget::Module { module, func } => is_pure_stdlib_call(module, func),
                CallTarget::Named { name } => pure_fns.contains(name),
                _ => false,
            };
            call_pure && args.iter().all(|a| expr_is_pure_with(a, pure_fns))
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        IrExprKind::BinOp { left, right, .. } => expr_is_pure_with(left, pure_fns) && expr_is_pure_with(right, pure_fns),
        IrExprKind::UnOp { operand, .. } => expr_is_pure_with(operand, pure_fns),
        IrExprKind::If { cond, then, else_ } => {
            expr_is_pure_with(cond, pure_fns) && expr_is_pure_with(then, pure_fns) && expr_is_pure_with(else_, pure_fns)
        }
        IrExprKind::Match { subject, arms } => {
            expr_is_pure_with(subject, pure_fns) && arms.iter().all(|a| expr_is_pure_with(&a.body, pure_fns))
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| stmt_is_pure_with(s, pure_fns))
                && expr.as_ref().map_or(true, |e| expr_is_pure_with(e, pure_fns))
        }
        IrExprKind::Lambda { body, .. } => expr_is_pure_with(body, pure_fns),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(|e| expr_is_pure_with(e, pure_fns))
        }
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| expr_is_pure_with(v, pure_fns)),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => expr_is_pure_with(object, pure_fns),
        IrExprKind::IndexAccess { object, index } => {
            expr_is_pure_with(object, pure_fns) && expr_is_pure_with(index, pure_fns)
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => expr_is_pure_with(e, pure_fns),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            expr_is_pure_with(e, pure_fns) && expr_is_pure_with(f, pure_fns)
        }
        IrExprKind::Range { start, end, .. } => expr_is_pure_with(start, pure_fns) && expr_is_pure_with(end, pure_fns),
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => expr_is_pure_with(expr, pure_fns),
                _ => true,
            })
        }
        // ForIn, While, Fan, Await, etc. — conservatively impure
        _ => false,
    }
}

fn stmt_is_pure_with(stmt: &IrStmt, pure_fns: &HashSet<Sym>) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            expr_is_pure_with(value, pure_fns)
        }
        // Assignments are mutations → impure
        IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::FieldAssign { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => false,
        IrStmtKind::Expr { expr } => expr_is_pure_with(expr, pure_fns),
        IrStmtKind::Guard { cond, else_ } => {
            expr_is_pure_with(cond, pure_fns) && expr_is_pure_with(else_, pure_fns)
        }
        IrStmtKind::Comment { .. } => true,
    }
}
