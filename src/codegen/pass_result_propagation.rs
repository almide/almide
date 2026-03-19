//! ResultPropagation Nanopass: insert Try (?) around Result-returning calls in effect fns.
//!
//! In Almide's effect fn, Result-returning calls are auto-unwrapped.
//! This pass wraps them in IrExprKind::Try so the walker emits `?`.
//!
//! Exception: match subjects are NOT wrapped (you match on Ok/Err, not unwrap).

use crate::ir::*;
use crate::types::Ty;
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct ResultPropagationPass;

impl NanoPass for ResultPropagationPass {
    fn name(&self) -> &str { "ResultPropagation" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        for func in &mut program.functions {
            // Insert Try in effect fns, but NOT in test functions
            // Tests inspect Result values directly, so no auto-?
            if func.is_effect && !func.is_test {
                let returns_result = matches!(&func.ret_ty, Ty::Result(_, _));
                func.body = insert_try_body(func.body.clone(), returns_result);
            }
        }
    }
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
        IrExprKind::Try { expr: inner } if matches!(&inner.ty, Ty::Result(_, _)) => {
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
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params,
            body: Box::new(insert_try(*body, false)),
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
        // Leaf nodes — return as-is
        other => other,
    };

    let mut result = IrExpr { kind, ty: ty.clone(), span };

    // Wrap in Try if this is a Result-returning call (not in match subject)
    if should_wrap {
        // Unwrap the Result type for the Try expression
        let inner_ty = match &ty {
            Ty::Result(ok, _) => ok.as_ref().clone(),
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
            let new_value = insert_try(value, false);
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
    if !matches!(&expr.ty, Ty::Result(_, _)) {
        return false;
    }
    matches!(&expr.kind,
        IrExprKind::Call { .. }
    )
}
