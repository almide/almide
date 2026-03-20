//! Variable substitution for IR expressions and statements.
//!
//! Replaces all occurrences of a `VarId` with a given expression,
//! respecting variable shadowing in lambda parameters and loop bindings.

use super::*;

/// Substitute all occurrences of `var` with `replacement` in an expression.
/// Respects shadowing: if a lambda or for-in rebinds `var`, substitution stops.
pub fn substitute_var_in_expr(expr: &IrExpr, var: VarId, replacement: &IrExpr) -> IrExpr {
    let sub = |e: &IrExpr| substitute_var_in_expr(e, var, replacement);
    let sub_stmt = |s: &IrStmt| substitute_var_in_stmt(s, var, replacement);

    match &expr.kind {
        IrExprKind::Var { id } if *id == var => replacement.clone(),

        // ── Structural recursion ──
        IrExprKind::Call { target, args, type_args } => IrExpr {
            kind: IrExprKind::Call {
                target: substitute_var_in_target(target, var, replacement),
                args: args.iter().map(sub).collect(),
                type_args: type_args.clone(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::BinOp { op, left, right } => IrExpr {
            kind: IrExprKind::BinOp {
                op: *op,
                left: Box::new(sub(left)),
                right: Box::new(sub(right)),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::UnOp { op, operand } => IrExpr {
            kind: IrExprKind::UnOp { op: *op, operand: Box::new(sub(operand)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(sub(cond)),
                then: Box::new(sub(then)),
                else_: Box::new(sub(else_)),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(sub(subject)),
                arms: arms.iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern.clone(),
                    guard: arm.guard.as_ref().map(sub),
                    body: sub(&arm.body),
                }).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Block { stmts, expr: tail } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.iter().map(sub_stmt).collect(),
                expr: tail.as_ref().map(|e| Box::new(sub(e))),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::DoBlock { stmts, expr: tail } => IrExpr {
            kind: IrExprKind::DoBlock {
                stmts: stmts.iter().map(sub_stmt).collect(),
                expr: tail.as_ref().map(|e| Box::new(sub(e))),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Lambda { params, body } => {
            if params.iter().any(|(p, _)| *p == var) {
                expr.clone() // shadowed
            } else {
                IrExpr {
                    kind: IrExprKind::Lambda {
                        params: params.clone(),
                        body: Box::new(sub(body)),
                    },
                    ty: expr.ty.clone(), span: expr.span,
                }
            }
        }
        IrExprKind::ForIn { var: loop_var, var_tuple, iterable, body } => IrExpr {
            kind: IrExprKind::ForIn {
                var: *loop_var,
                var_tuple: var_tuple.clone(),
                iterable: Box::new(sub(iterable)),
                body: if *loop_var == var { body.clone() } else {
                    body.iter().map(sub_stmt).collect()
                },
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::While { cond, body } => IrExpr {
            kind: IrExprKind::While {
                cond: Box::new(sub(cond)),
                body: body.iter().map(sub_stmt).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },

        // ── Single-child wrappers ──
        IrExprKind::Member { object, field } => IrExpr {
            kind: IrExprKind::Member { object: Box::new(sub(object)), field: field.clone() },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::TupleIndex { object, index } => IrExpr {
            kind: IrExprKind::TupleIndex { object: Box::new(sub(object)), index: *index },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::IndexAccess { object, index } => IrExpr {
            kind: IrExprKind::IndexAccess {
                object: Box::new(sub(object)),
                index: Box::new(sub(index)),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::MapAccess { object, key } => IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(sub(object)),
                key: Box::new(sub(key)),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::OptionSome { expr: inner } => IrExpr {
            kind: IrExprKind::OptionSome { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::ResultOk { expr: inner } => IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::ResultErr { expr: inner } => IrExpr {
            kind: IrExprKind::ResultErr { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Try { expr: inner } => IrExpr {
            kind: IrExprKind::Try { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Await { expr: inner } => IrExpr {
            kind: IrExprKind::Await { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Clone { expr: inner } => IrExpr {
            kind: IrExprKind::Clone { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Deref { expr: inner } => IrExpr {
            kind: IrExprKind::Deref { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Borrow { expr: inner, as_str } => IrExpr {
            kind: IrExprKind::Borrow { expr: Box::new(sub(inner)), as_str: *as_str },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::BoxNew { expr: inner } => IrExpr {
            kind: IrExprKind::BoxNew { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::ToVec { expr: inner } => IrExpr {
            kind: IrExprKind::ToVec { expr: Box::new(sub(inner)) },
            ty: expr.ty.clone(), span: expr.span,
        },

        // ── Collection literals ──
        IrExprKind::List { elements } => IrExpr {
            kind: IrExprKind::List { elements: elements.iter().map(sub).collect() },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Tuple { elements } => IrExpr {
            kind: IrExprKind::Tuple { elements: elements.iter().map(sub).collect() },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Fan { exprs } => IrExpr {
            kind: IrExprKind::Fan { exprs: exprs.iter().map(sub).collect() },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Record { name, fields } => IrExpr {
            kind: IrExprKind::Record {
                name: name.clone(),
                fields: fields.iter().map(|(k, v)| (k.clone(), sub(v))).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::SpreadRecord { base, fields } => IrExpr {
            kind: IrExprKind::SpreadRecord {
                base: Box::new(sub(base)),
                fields: fields.iter().map(|(k, v)| (k.clone(), sub(v))).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::MapLiteral { entries } => IrExpr {
            kind: IrExprKind::MapLiteral {
                entries: entries.iter().map(|(k, v)| (sub(k), sub(v))).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::Range { start, end, inclusive } => IrExpr {
            kind: IrExprKind::Range {
                start: Box::new(sub(start)),
                end: Box::new(sub(end)),
                inclusive: *inclusive,
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::StringInterp { parts } => IrExpr {
            kind: IrExprKind::StringInterp {
                parts: parts.iter().map(|p| match p {
                    IrStringPart::Expr { expr: e } => IrStringPart::Expr { expr: sub(e) },
                    other => other.clone(),
                }).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },
        IrExprKind::RustMacro { name, args } => IrExpr {
            kind: IrExprKind::RustMacro {
                name: name.clone(),
                args: args.iter().map(sub).collect(),
            },
            ty: expr.ty.clone(), span: expr.span,
        },

        // ── True leaf nodes ──
        IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::EmptyMap | IrExprKind::OptionNone
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::RenderedCall { .. } => expr.clone(),
    }
}

/// Substitute a variable inside a call target (Method objects, Computed callees).
fn substitute_var_in_target(target: &CallTarget, var: VarId, replacement: &IrExpr) -> CallTarget {
    match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(substitute_var_in_expr(object, var, replacement)),
            method: method.clone(),
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(substitute_var_in_expr(callee, var, replacement)),
        },
        other => other.clone(),
    }
}

/// Substitute all occurrences of `var` with `replacement` in a statement.
pub fn substitute_var_in_stmt(stmt: &IrStmt, var: VarId, replacement: &IrExpr) -> IrStmt {
    let sub = |e: &IrExpr| substitute_var_in_expr(e, var, replacement);
    let kind = match &stmt.kind {
        IrStmtKind::Bind { var: v, mutability, ty, value } => IrStmtKind::Bind {
            var: *v, mutability: *mutability, ty: ty.clone(), value: sub(value),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern: pattern.clone(), value: sub(value),
        },
        IrStmtKind::Assign { var: v, value } => IrStmtKind::Assign {
            var: *v, value: sub(value),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target: *target, index: sub(index), value: sub(value),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target: *target, key: sub(key), value: sub(value),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target: *target, field: field.clone(), value: sub(value),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: sub(cond), else_: sub(else_),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: sub(expr) },
        IrStmtKind::Comment { .. } => stmt.kind.clone(),
    };
    IrStmt { kind, span: stmt.span }
}
