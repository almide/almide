// ── Auto-? insertion (desugaring) ─────────────────────────────────────
//
// In effect fn bodies, the checker types user effect fn calls as
// Result[T, String] and auto_unwrap strips Result in let/var/match.
// This pass bridges the IR gap: Call.ty = Result[T, String] while
// Bind.ty = T. It wraps Result-typed calls in Try nodes, producing
// the T that bindings expect.
//
// Moved from codegen (pass_result_propagation.rs Phase 3) to lowering
// because this is desugaring, not code generation.

use std::collections::HashSet;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};

/// Insert auto-? (Try nodes) in all effect fn bodies of the program.
pub fn insert_auto_try(program: &mut IrProgram) {
    for func in &mut program.functions {
        if func.is_effect && !func.is_test {
            let returns_result = func.ret_ty.is_result();
            func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
        }
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            if func.is_effect {
                let returns_result = func.ret_ty.is_result();
                func.body = insert_try_body(std::mem::take(&mut func.body), returns_result);
            }
        }
    }
}

fn match_has_result_arms(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }))
}

fn collect_result_match_vars(stmts: &[IrStmt], tail: Option<&IrExpr>) -> HashSet<u32> {
    let mut vars = HashSet::new();
    for s in stmts { collect_result_match_vars_stmt(s, &mut vars); }
    if let Some(e) = tail { collect_result_match_vars_expr(e, &mut vars); }
    vars
}

fn collect_result_match_vars_expr(expr: &IrExpr, vars: &mut HashSet<u32>) {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            if match_has_result_arms(arms) {
                if let IrExprKind::Var { id } = &subject.kind {
                    vars.insert(id.0);
                }
            }
            for arm in arms { collect_result_match_vars_expr(&arm.body, vars); }
            collect_result_match_vars_expr(subject, vars);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { collect_result_match_vars_stmt(s, vars); }
            if let Some(e) = tail { collect_result_match_vars_expr(e, vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_result_match_vars_expr(cond, vars);
            collect_result_match_vars_expr(then, vars);
            collect_result_match_vars_expr(else_, vars);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_result_match_vars_expr(iterable, vars);
            for s in body { collect_result_match_vars_stmt(s, vars); }
        }
        IrExprKind::While { cond, body } => {
            collect_result_match_vars_expr(cond, vars);
            for s in body { collect_result_match_vars_stmt(s, vars); }
        }
        _ => {}
    }
}

fn collect_result_match_vars_stmt(stmt: &IrStmt, vars: &mut HashSet<u32>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_result_match_vars_expr(value, vars),
        IrStmtKind::Expr { expr } => collect_result_match_vars_expr(expr, vars),
        IrStmtKind::Guard { cond, else_ } => {
            collect_result_match_vars_expr(cond, vars);
            collect_result_match_vars_expr(else_, vars);
        }
        _ => {}
    }
}

fn insert_try_body(expr: IrExpr, fn_returns_result: bool) -> IrExpr {
    if fn_returns_result {
        match expr.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let skip_unwrap = collect_result_match_vars(&stmts, Some(&tail));
                let stmts = stmts.into_iter()
                    .map(|s| insert_try_stmt_with_skip(s, &skip_unwrap))
                    .collect();
                let tail = insert_try(*tail, false);
                let tail = strip_tail_try(tail);
                return IrExpr {
                    kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
                    ty: expr.ty, span: expr.span, def_id: None,
                };
            }
            _ => {
                let result = insert_try(expr, false);
                return strip_tail_try(result);
            }
        }
    }
    if let IrExprKind::Block { stmts, expr: tail } = expr.kind {
        let skip_unwrap = collect_result_match_vars(&stmts, tail.as_deref());
        let stmts = stmts.into_iter()
            .map(|s| insert_try_stmt_with_skip(s, &skip_unwrap))
            .collect();
        let tail = tail.map(|e| Box::new(insert_try(*e, false)));
        return IrExpr {
            kind: IrExprKind::Block { stmts, expr: tail },
            ty: expr.ty, span: expr.span, def_id: None,
        };
    }
    insert_try(expr, false)
}

fn insert_try_stmt_with_skip(stmt: IrStmt, skip: &HashSet<u32>) -> IrStmt {
    if let IrStmtKind::Bind { var, .. } = &stmt.kind {
        if skip.contains(&var.0) {
            if let IrStmtKind::Bind { var, mutability, ty, value } = stmt.kind {
                let new_value = insert_try(value, false);
                let unwrapped = match new_value.kind {
                    IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
                    _ => new_value,
                };
                return IrStmt {
                    kind: IrStmtKind::Bind { var, mutability, ty, value: unwrapped },
                    span: stmt.span,
                };
            }
        }
    }
    insert_try_stmt(stmt)
}

fn strip_tail_try(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
        IrExprKind::Match { subject, arms } => {
            let arms = arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern, guard: arm.guard,
                body: strip_tail_try(arm.body),
            }).collect();
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: expr.ty, span: expr.span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(strip_tail_try(*then)),
                else_: Box::new(strip_tail_try(*else_)),
            },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(strip_tail_try(*tail))) },
            ty: expr.ty, span: expr.span, def_id: None,
        },
        _ => expr,
    }
}

fn insert_try(expr: IrExpr, in_match_subject: bool) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let should_wrap = !in_match_subject && is_result_call(&expr);

    let kind = match expr.kind {
        IrExprKind::Block { stmts, expr: e } => IrExprKind::Block {
            stmts: stmts.into_iter().map(insert_try_stmt).collect(),
            expr: e.map(|e| Box::new(insert_try(*e, false))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(insert_try(*cond, false)),
            then: Box::new(insert_try(*then, false)),
            else_: Box::new(insert_try(*else_, false)),
        },
        IrExprKind::Match { subject, arms } => {
            let arms_match_result = arms.iter().any(|a|
                matches!(&a.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }));
            IrExprKind::Match {
                subject: Box::new(insert_try(*subject, arms_match_result)),
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(|g| insert_try(g, false)),
                    body: insert_try(arm.body, false),
                }).collect(),
            }
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
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body, lambda_id,
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
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_try(*expr, false)),
            field,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(insert_try(*iterable, false)),
            body: body.into_iter().map(insert_try_stmt).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(insert_try(*cond, false)),
            body: body.into_iter().map(insert_try_stmt).collect(),
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
        IrExprKind::Unwrap { expr: inner } => IrExprKind::Unwrap {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::Try { expr: inner } => IrExprKind::Try {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::ToOption { expr: inner } => IrExprKind::ToOption {
            expr: Box::new(insert_try(*inner, true)),
        },
        IrExprKind::UnwrapOr { expr: inner, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_try(*inner, true)),
            fallback: Box::new(insert_try(*fallback, false)),
        },
        other => other,
    };

    let mut result = IrExpr { kind, ty: ty.clone(), span, def_id: None };

    if should_wrap {
        let inner_ty = match &ty {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            _ => ty,
        };
        result = IrExpr {
            kind: IrExprKind::Try { expr: Box::new(result) },
            ty: inner_ty,
            span, def_id: None,
        };
    }

    result
}

fn insert_try_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => {
            let mut new_value = insert_try(value, false);
            // A binding explicitly annotated `Result[..]` keeps its Result. The
            // auto-? top-level wrap unwraps a result-returning call to its ok
            // type, which would leave `let r: Result[..] = call()` bound to a bare
            // value — so `r ?? d` / `r == ok(v)` (e.g. on a `fan.timeout` result)
            // no longer type-check. Undo that wrap when the declared type is Result.
            if ty.is_result() {
                let nv_ty = new_value.ty.clone();
                let nv_span = new_value.span;
                new_value = match new_value.kind {
                    IrExprKind::Try { expr: inner } if inner.ty.is_result() => *inner,
                    other => IrExpr { kind: other, ty: nv_ty, span: nv_span, def_id: None },
                };
            }
            if !matches!(&new_value.kind, IrExprKind::Try { .. })
                && is_result_value(&new_value)
                && !ty.is_result()
            {
                let inner_ty = match &new_value.ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => new_value.ty.clone(),
                };
                let span = new_value.span;
                new_value = IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(new_value) },
                    ty: inner_ty,
                    span, def_id: None,
                };
            }
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

fn is_result_call(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind, IrExprKind::Call { .. })
}

fn is_result_value(expr: &IrExpr) -> bool {
    expr.ty.is_result() && matches!(&expr.kind,
        IrExprKind::Call { .. }
        | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. }
    )
}
