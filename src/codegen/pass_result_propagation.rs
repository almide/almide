//! ResultPropagation Nanopass: insert Try (?) around Result-returning calls in effect fns.
//!
//! In Almide's effect fn, Result-returning calls are auto-unwrapped.
//! This pass wraps them in IrExprKind::Try so the walker emits `?`.
//!
//! Exception: match subjects are NOT wrapped (you match on Ok/Err, not unwrap).

use std::collections::HashMap;
use crate::ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ResultPropagationPass;

impl NanoPass for ResultPropagationPass {
    fn name(&self) -> &str { "ResultPropagation" }

    fn targets(&self) -> Option<Vec<Target>> {
        None // Run for all targets
    }

    fn run(&self, mut program: IrProgram, target: Target) -> PassResult {
        let wrap_non_result = matches!(target, Target::Rust | Target::Wasm);

        // Phase 1: Lift effect fn return types from T to Result<T, String>.
        // Collect fn_name → new Result type so call sites can be updated.
        let mut lifted_fns: HashMap<String, Ty> = HashMap::new();
        for func in &mut program.functions {
            if func.is_effect && !func.is_test && wrap_non_result && !func.ret_ty.is_result() {
                let orig = std::mem::replace(&mut func.ret_ty, Ty::Unit);
                func.ret_ty = Ty::result(orig, Ty::String);
                func.body = wrap_tail_in_ok(std::mem::take(&mut func.body));
                lifted_fns.insert(func.name.clone(), func.ret_ty.clone());
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if func.is_effect && !func.is_test && wrap_non_result && !func.ret_ty.is_result() {
                    let orig = std::mem::replace(&mut func.ret_ty, Ty::Unit);
                    func.ret_ty = Ty::result(orig, Ty::String);
                    func.body = wrap_tail_in_ok(std::mem::take(&mut func.body));
                    lifted_fns.insert(func.name.clone(), func.ret_ty.clone());
                }
            }
        }

        // Phase 2: Update call-site types for lifted functions, then insert Try.
        let mut retyped_vars: HashMap<u32, Ty> = HashMap::new();
        for func in &mut program.functions {
            if !lifted_fns.is_empty() {
                func.body = update_call_types(std::mem::take(&mut func.body), &lifted_fns);
            }
            if func.is_effect && !func.is_test {
                let returns_result = func.ret_ty.is_result();
                func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
                collect_retyped_vars(&func.body, &mut retyped_vars);
                if !retyped_vars.is_empty() {
                    func.body = fix_var_types(std::mem::take(&mut func.body), &retyped_vars);
                    retyped_vars.clear();
                }
            } else if func.is_test {
                if !lifted_fns.is_empty() {
                    func.body = insert_try_for_lifted(std::mem::take(&mut func.body), &lifted_fns);
                }
                func.body = insert_try_in_fan(std::mem::take(&mut func.body));
            }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if !lifted_fns.is_empty() {
                    func.body = update_call_types(std::mem::take(&mut func.body), &lifted_fns);
                }
                if func.is_effect && !func.is_test {
                    let returns_result = func.ret_ty.is_result();
                    func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
                    collect_retyped_vars(&func.body, &mut retyped_vars);
                    if !retyped_vars.is_empty() {
                        func.body = fix_var_types(std::mem::take(&mut func.body), &retyped_vars);
                        retyped_vars.clear();
                    }
                }
            }
        }

        PassResult { program, changed: true }
    }
}

/// Wrap the tail expression of an effect fn body in Ok(...).
/// Called when ret_ty is lifted from T to Result<T, String>.
fn wrap_tail_in_ok(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    match expr.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let wrapped = wrap_tail_in_ok(*tail);
            IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(wrapped)) },
                ty: Ty::result(ty, Ty::String), span,
            }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(wrap_tail_in_ok(*then)),
                else_: Box::new(wrap_tail_in_ok(*else_)),
            },
            ty: Ty::result(ty, Ty::String), span,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject,
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern, guard: arm.guard,
                    body: wrap_tail_in_ok(arm.body),
                }).collect(),
            },
            ty: Ty::result(ty, Ty::String), span,
        },
        // Already Result — don't double-wrap
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => expr,
        // Everything else: wrap in Ok(expr)
        other => {
            let result_ty = Ty::result(ty.clone(), Ty::String);
            IrExpr {
                kind: IrExprKind::ResultOk {
                    expr: Box::new(IrExpr { kind: other, ty, span }),
                },
                ty: result_ty, span,
            }
        }
    }
}

/// Update Call expression types for functions whose return type was lifted to Result.
fn update_call_types(expr: IrExpr, lifted: &HashMap<String, Ty>) -> IrExpr {
    let ty = expr.ty;
    let span = expr.span;
    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let fn_name = match &target {
                CallTarget::Named { name } => Some(name.clone()),
                CallTarget::Module { module, func } => Some(format!("{}.{}", module, func)),
                CallTarget::Method { method, .. } => Some(method.clone()),
                _ => None,
            };
            let args = args.into_iter().map(|a| update_call_types(a, lifted)).collect();
            let final_ty = fn_name.as_ref().and_then(|n| lifted.get(n)).cloned().unwrap_or(ty);
            return IrExpr { kind: IrExprKind::Call { target, args, type_args }, ty: final_ty, span };
        }
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| update_call_types_stmt(s, lifted)).collect(),
            expr: e.map(|e| Box::new(update_call_types(*e, lifted))),
        },
        IrExprKind::DoBlock { stmts, expr: e } => IrExprKind::DoBlock {
            stmts: stmts.into_iter().map(|s| update_call_types_stmt(s, lifted)).collect(),
            expr: e.map(|e| Box::new(update_call_types(*e, lifted))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(update_call_types(*cond, lifted)),
            then: Box::new(update_call_types(*then, lifted)),
            else_: Box::new(update_call_types(*else_, lifted)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(update_call_types(*subject, lifted)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| update_call_types(g, lifted)),
                body: update_call_types(arm.body, lifted),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(update_call_types(*iterable, lifted)),
            body: body.into_iter().map(|s| update_call_types_stmt(s, lifted)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(update_call_types(*cond, lifted)),
            body: body.into_iter().map(|s| update_call_types_stmt(s, lifted)).collect(),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(update_call_types(*inner, lifted)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(update_call_types(*inner, lifted)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(update_call_types(*inner, lifted)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: update_call_types(expr, lifted) },
                other => other,
            }).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| update_call_types(e, lifted)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, update_call_types(v, lifted))).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(update_call_types(*left, lifted)),
            right: Box::new(update_call_types(*right, lifted)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(update_call_types(*operand, lifted)),
        },
        other => other,
    };
    IrExpr { kind, ty, span }
}

fn update_call_types_stmt(stmt: IrStmt, lifted: &HashMap<String, Ty>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } =>
            IrStmtKind::Bind { var, mutability, ty, value: update_call_types(value, lifted) },
        IrStmtKind::Expr { expr } =>
            IrStmtKind::Expr { expr: update_call_types(expr, lifted) },
        IrStmtKind::Assign { var, value } =>
            IrStmtKind::Assign { var, value: update_call_types(value, lifted) },
        IrStmtKind::Guard { cond, else_ } =>
            IrStmtKind::Guard { cond: update_call_types(cond, lifted), else_: update_call_types(else_, lifted) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

/// Insert Try around calls to lifted effect fns in test functions.
/// Test functions don't go through insert_try_body, but they still need to
/// unwrap Result values from lifted effect fn calls.
fn insert_try_for_lifted(expr: IrExpr, lifted: &HashMap<String, Ty>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    match expr.kind {
        IrExprKind::Call { ref target, ref args, .. } => {
            let lifted_ty = match target {
                CallTarget::Named { name } => lifted.get(name),
                CallTarget::Module { module, func } => lifted.get(&format!("{}.{}", module, func)),
                CallTarget::Method { method, .. } => lifted.get(method),
                _ => None,
            };
            if let Some(result_ty) = lifted_ty {
                let inner_ty = match result_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty.clone(),
                };
                let call_with_result_ty = IrExpr { kind: expr.kind, ty: result_ty.clone(), span };
                // In tests, use .unwrap() instead of ? (tests return (), not Result)
                return IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Method {
                            object: Box::new(call_with_result_ty),
                            method: "unwrap".to_string(),
                        },
                        args: vec![],
                        type_args: vec![],
                    },
                    ty: inner_ty, span,
                };
            }
            // Recurse into args to find nested lifted calls
            let _ = args; // avoid unused warning from ref above
            match expr.kind {
                IrExprKind::Call { target, args, type_args } => {
                    let args = args.into_iter().map(|a| insert_try_for_lifted(a, lifted)).collect();
                    IrExpr { kind: IrExprKind::Call { target, args, type_args }, ty, span }
                }
                _ => unreachable!()
            }
        }
        IrExprKind::Block { stmts, expr: tail } => {
            let stmts = stmts.into_iter().map(|s| insert_try_for_lifted_stmt(s, lifted)).collect();
            let tail = tail.map(|e| Box::new(insert_try_for_lifted(*e, lifted)));
            IrExpr { kind: IrExprKind::Block { stmts, expr: tail }, ty, span }
        }
        IrExprKind::DoBlock { stmts, expr: tail } => {
            let stmts = stmts.into_iter().map(|s| insert_try_for_lifted_stmt(s, lifted)).collect();
            let tail = tail.map(|e| Box::new(insert_try_for_lifted(*e, lifted)));
            IrExpr { kind: IrExprKind::DoBlock { stmts, expr: tail }, ty, span }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(insert_try_for_lifted(*cond, lifted)),
                then: Box::new(insert_try_for_lifted(*then, lifted)),
                else_: Box::new(insert_try_for_lifted(*else_, lifted)),
            }, ty, span,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(insert_try_for_lifted(*subject, lifted)),
                arms: arms.into_iter().map(|a| IrMatchArm {
                    pattern: a.pattern, guard: a.guard,
                    body: insert_try_for_lifted(a.body, lifted),
                }).collect(),
            }, ty, span,
        },
        _ => expr,
    }
}

fn insert_try_for_lifted_stmt(stmt: IrStmt, lifted: &HashMap<String, Ty>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let new_value = insert_try_for_lifted(value, lifted);
            let new_ty = if matches!(&new_value.kind, IrExprKind::Try { .. }) { new_value.ty.clone() } else { ty };
            IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
        }
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_try_for_lifted(expr, lifted) },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_try_for_lifted(value, lifted) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

/// Insert Try in function body — skip final expression if fn returns Result.
fn insert_try_body(expr: IrExpr, fn_returns_result: bool) -> IrExpr {
    if fn_returns_result {
        match expr.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let stmts = stmts.into_iter().map(insert_try_stmt).collect();
                let tail = insert_try(*tail, false);
                let tail = strip_tail_try(tail);
                return IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
                    ty: expr.ty, span: expr.span,
                };
            }
            _ => {
                let result = insert_try(expr, false);
                return strip_tail_try(result);
            }
        }
    }
    insert_try(expr, false)
}

/// Recursively strip Try from tail positions of a Result-returning expression.
/// Handles: direct Try, Match arms, If branches, Block tails.
fn strip_tail_try(expr: IrExpr) -> IrExpr {
    match expr.kind {
        // Direct Try on a Result-returning call — unwrap it
        IrExprKind::Try { expr: inner } if inner.ty.is_result() => {
            *inner
        }
        // Match: strip Try from each arm body
        IrExprKind::Match { subject, arms } => {
            let arms = arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard,
                body: strip_tail_try(arm.body),
            }).collect();
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: expr.ty, span: expr.span }
        }
        // If: strip Try from then/else branches
        IrExprKind::If { cond, then, else_ } => {
            IrExpr {
                kind: IrExprKind::If {
                    cond,
                    then: Box::new(strip_tail_try(*then)),
                    else_: Box::new(strip_tail_try(*else_)),
                },
                ty: expr.ty, span: expr.span,
            }
        }
        // Block: strip Try from tail expression
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            IrExpr {
                kind: IrExprKind::Block { stmts, expr: Some(Box::new(strip_tail_try(*tail))) },
                ty: expr.ty, span: expr.span,
            }
        }
        _ => expr,
    }
}

/// Recursively insert Try around Result-returning calls.
/// `in_match_subject` prevents wrapping match subjects.
fn insert_try(expr: IrExpr, in_match_subject: bool) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    // First, check if THIS expression is a Result-returning call that should be wrapped
    let should_wrap = !in_match_subject && is_result_call(&expr);

    let kind = match expr.kind {
        // Recurse into sub-expressions
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| insert_try_stmt(s)).collect(),
            expr: e.map(|e| Box::new(insert_try(*e, false))),
        },
        IrExprKind::DoBlock { stmts, expr: e } => IrExprKind::DoBlock {
            stmts: stmts.into_iter().map(|s| insert_try_stmt(s)).collect(),
            expr: e.map(|e| Box::new(insert_try(*e, false))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try(*cond, false)),
            then: Box::new(insert_try(*then, false)),
            else_: Box::new(insert_try(*else_, false)),
        },
        // Match: subject is NOT wrapped, but arm bodies ARE
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(insert_try(*subject, true)), // don't wrap subject
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| insert_try(g, false)),
                body: insert_try(arm.body, false),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_try(*left, false)),
            right: Box::new(insert_try(*right, false)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(insert_try(*operand, false)),
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target,
            args: args.into_iter().map(|a| insert_try(a, false)).collect(),
            type_args,
        },
        // Don't recurse into lambdas — they are independent scopes,
        // not part of the effect fn's error propagation chain.
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params,
            body,
            lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, insert_try(v, false))).collect(),
        },
        IrExprKind::OptionSome { expr: inner } => IrExprKind::OptionSome {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(insert_try(*inner, false)),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(insert_try(*object, false)),
            field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try(*iterable, false)),
            body: body.into_iter().map(|s| insert_try_stmt(s)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try(*cond, false)),
            body: body.into_iter().map(|s| insert_try_stmt(s)).collect(),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_try(expr, false) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_try(e, false)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(insert_try(*base, false)),
            fields: fields.into_iter().map(|(k, v)| (k, insert_try(v, false))).collect(),
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(insert_try(*object, false)),
            index: Box::new(insert_try(*index, false)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_try(*object, false)),
            index,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone {
            expr: Box::new(insert_try(*expr, false)),
        },
        IrExprKind::Deref { expr } => IrExprKind::Deref {
            expr: Box::new(insert_try(*expr, false)),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_try(k, false), insert_try(v, false))).collect(),
        },
        // Leaf nodes — return as-is
        other => other,
    };

    let mut result = IrExpr { kind, ty: ty.clone(), span };

    // Wrap in Try if this is a Result-returning call (not in match subject)
    if should_wrap {
        // Unwrap the Result type for the Try expression
        let inner_ty = match &ty {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            _ => ty,
        };
        result = IrExpr {
            kind: IrExprKind::Try { expr: Box::new(result) },
            ty: inner_ty,
            span,
        };
    }

    result
}

fn insert_try_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let mut new_value = insert_try(value, false);
            // If the value wasn't already wrapped by insert_try (e.g. ok()/err() which
            // are not Call nodes), wrap it here at the binding site.
            if !matches!(&new_value.kind, IrExprKind::Try { .. }) && is_result_value(&new_value) {
                let inner_ty = match &new_value.ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => new_value.ty.clone(),
                };
                let span = new_value.span;
                new_value = IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(new_value) },
                    ty: inner_ty,
                    span,
                };
            }
            // If the value was wrapped in Try, update the binding type
            let new_ty = if matches!(&new_value.kind, IrExprKind::Try { .. }) {
                new_value.ty.clone()
            } else {
                ty
            };
            IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
        }
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: insert_try(value, false),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: insert_try(expr, false),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try(cond, false),
            else_: insert_try(else_, false),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

/// Check if an expression is a Result-returning function call.
fn is_result_call(expr: &IrExpr) -> bool {
    if !expr.ty.is_result() {
        return false;
    }
    matches!(&expr.kind,
        IrExprKind::Call { .. }
    )
}

/// Check if an expression is a Result-producing value (call, ok(), err()) that should be
/// auto-unwrapped when used as a let binding value in an effect fn.
fn is_result_value(expr: &IrExpr) -> bool {
    if !expr.ty.is_result() {
        return false;
    }
    matches!(&expr.kind,
        IrExprKind::Call { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
    )
}

/// Collect VarId→unwrapped type mappings from bindings whose values are Try-wrapped.
fn collect_retyped_vars(expr: &IrExpr, map: &mut HashMap<u32, Ty>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } | IrExprKind::DoBlock { stmts, expr: tail } => {
            for s in stmts { collect_retyped_vars_stmt(s, map); }
            if let Some(e) = tail { collect_retyped_vars(e, map); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_retyped_vars(cond, map);
            collect_retyped_vars(then, map);
            collect_retyped_vars(else_, map);
        }
        IrExprKind::Match { subject, arms } => {
            collect_retyped_vars(subject, map);
            for arm in arms { collect_retyped_vars(&arm.body, map); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_retyped_vars(iterable, map);
            for s in body { collect_retyped_vars_stmt(s, map); }
        }
        IrExprKind::While { cond, body } => {
            collect_retyped_vars(cond, map);
            for s in body { collect_retyped_vars_stmt(s, map); }
        }
        _ => {}
    }
}

fn collect_retyped_vars_stmt(stmt: &IrStmt, map: &mut HashMap<u32, Ty>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => {
            if matches!(&value.kind, IrExprKind::Try { .. }) {
                // The binding value is Try-wrapped; record unwrapped type
                map.insert(var.0, value.ty.clone());
            }
            collect_retyped_vars(value, map);
        }
        IrStmtKind::Expr { expr } => collect_retyped_vars(expr, map),
        IrStmtKind::Guard { cond, else_ } => {
            collect_retyped_vars(cond, map);
            collect_retyped_vars(else_, map);
        }
        _ => {}
    }
}

/// Walk the IR tree, updating Var reference types for VarIds in the retyped map.
fn fix_var_types(expr: IrExpr, map: &HashMap<u32, Ty>) -> IrExpr {
    let ty = expr.ty;
    let span = expr.span;
    let kind = match expr.kind {
        IrExprKind::Var { id } => {
            if let Some(new_ty) = map.get(&id.0) {
                return IrExpr { kind: IrExprKind::Var { id }, ty: new_ty.clone(), span };
            }
            IrExprKind::Var { id }
        }
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| fix_var_types_stmt(s, map)).collect(),
            expr: e.map(|e| Box::new(fix_var_types(*e, map))),
        },
        IrExprKind::DoBlock { stmts, expr: e } => IrExprKind::DoBlock {
            stmts: stmts.into_iter().map(|s| fix_var_types_stmt(s, map)).collect(),
            expr: e.map(|e| Box::new(fix_var_types(*e, map))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(fix_var_types(*cond, map)),
            then: Box::new(fix_var_types(*then, map)),
            else_: Box::new(fix_var_types(*else_, map)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(fix_var_types(*subject, map)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| fix_var_types(g, map)),
                body: fix_var_types(arm.body, map),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(fix_var_types(*left, map)),
            right: Box::new(fix_var_types(*right, map)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(fix_var_types(*operand, map)),
        },
        IrExprKind::Call { target, args, type_args } => {
            let target = match target {
                CallTarget::Method { object, method } =>
                    CallTarget::Method { object: Box::new(fix_var_types(*object, map)), method },
                CallTarget::Computed { callee } =>
                    CallTarget::Computed { callee: Box::new(fix_var_types(*callee, map)) },
                other => other,
            };
            IrExprKind::Call {
                target,
                args: args.into_iter().map(|a| fix_var_types(a, map)).collect(),
                type_args,
            }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(fix_var_types(*iterable, map)),
            body: body.into_iter().map(|s| fix_var_types_stmt(s, map)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(fix_var_types(*cond, map)),
            body: body.into_iter().map(|s| fix_var_types_stmt(s, map)).collect(),
        },
        IrExprKind::ResultOk { expr: inner } => IrExprKind::ResultOk {
            expr: Box::new(fix_var_types(*inner, map)),
        },
        IrExprKind::ResultErr { expr: inner } => IrExprKind::ResultErr {
            expr: Box::new(fix_var_types(*inner, map)),
        },
        IrExprKind::OptionSome { expr: inner } => IrExprKind::OptionSome {
            expr: Box::new(fix_var_types(*inner, map)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(fix_var_types(*inner, map)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: fix_var_types(expr, map) },
                other => other,
            }).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| fix_var_types(e, map)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| fix_var_types(e, map)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, fix_var_types(v, map))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(fix_var_types(*base, map)),
            fields: fields.into_iter().map(|(k, v)| (k, fix_var_types(v, map))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(fix_var_types(*object, map)),
            field,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(fix_var_types(*object, map)),
            index: Box::new(fix_var_types(*index, map)),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(fix_var_types(*object, map)),
            index,
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (fix_var_types(k, map), fix_var_types(v, map))).collect(),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(fix_var_types(*object, map)),
            key: Box::new(fix_var_types(*key, map)),
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone {
            expr: Box::new(fix_var_types(*expr, map)),
        },
        IrExprKind::Deref { expr } => IrExprKind::Deref {
            expr: Box::new(fix_var_types(*expr, map)),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| fix_var_types(e, map)).collect(),
        },
        // Leaf nodes — return as-is
        other => other,
    };
    IrExpr { kind, ty, span }
}

fn fix_var_types_stmt(stmt: IrStmt, map: &HashMap<u32, Ty>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } =>
            IrStmtKind::Bind { var, mutability, ty, value: fix_var_types(value, map) },
        IrStmtKind::Assign { var, value } =>
            IrStmtKind::Assign { var, value: fix_var_types(value, map) },
        IrStmtKind::Expr { expr } =>
            IrStmtKind::Expr { expr: fix_var_types(expr, map) },
        IrStmtKind::Guard { cond, else_ } =>
            IrStmtKind::Guard { cond: fix_var_types(cond, map), else_: fix_var_types(else_, map) },
        IrStmtKind::IndexAssign { target, index, value } =>
            IrStmtKind::IndexAssign { target, index: fix_var_types(index, map), value: fix_var_types(value, map) },
        IrStmtKind::FieldAssign { target, field, value } =>
            IrStmtKind::FieldAssign { target, field, value: fix_var_types(value, map) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

/// Insert Try only inside Fan blocks in test functions.
/// Fan auto-unwraps Results (type checker already adjusted types),
/// so WASM needs Try nodes to actually perform the unwrap.
fn insert_try_in_fan(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let kind = match expr.kind {
        IrExprKind::Fan { exprs } => {
            // Inside fan: insert Try around Result-returning calls
            IrExprKind::Fan {
                exprs: exprs.into_iter().map(|e| insert_try(e, false)).collect(),
            }
        }
        // Recurse into structural nodes to find nested fan blocks
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(insert_try_in_fan_stmt).collect(),
            expr: e.map(|e| Box::new(insert_try_in_fan(*e))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try_in_fan(*cond)),
            then: Box::new(insert_try_in_fan(*then)),
            else_: Box::new(insert_try_in_fan(*else_)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(insert_try_in_fan(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| insert_try_in_fan(g)),
                body: insert_try_in_fan(arm.body),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try_in_fan(*iterable)),
            body: body.into_iter().map(insert_try_in_fan_stmt).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try_in_fan(*cond)),
            body: body.into_iter().map(insert_try_in_fan_stmt).collect(),
        },
        IrExprKind::DoBlock { stmts, expr: e } => IrExprKind::DoBlock {
            stmts: stmts.into_iter().map(insert_try_in_fan_stmt).collect(),
            expr: e.map(|e| Box::new(insert_try_in_fan(*e))),
        },
        other => other,
    };
    IrExpr { kind, ty, span }
}

fn insert_try_in_fan_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let new_value = insert_try_in_fan(value);
            let new_ty = if matches!(&new_value.kind, IrExprKind::Fan { .. }) {
                // Fan was processed: if inner expressions got Try'd, update binding type
                new_value.ty.clone()
            } else {
                ty
            };
            IrStmtKind::Bind { var, mutability, ty: new_ty, value: new_value }
        }
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_try_in_fan(expr) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: insert_try_in_fan(cond),
            else_: insert_try_in_fan(else_),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: insert_try_in_fan(value),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}
