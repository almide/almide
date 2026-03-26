//! Generic bottom-up IR transform utilities.

use crate::ir::*;

/// Apply `try_transform` bottom-up to every node in an expression tree.
/// Children are transformed first, then `try_transform` is called on the result.
/// If it returns `Some(new)`, the node is replaced; otherwise kept as-is.
pub(super) fn recursive_transform(
    expr: IrExpr,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrExpr {
    let transformed = transform_children(expr, f);
    f(transformed.clone()).unwrap_or(transformed)
}

/// Transform all children of an expression, leaving the node itself unchanged.
fn transform_children(
    expr: IrExpr,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    // Use a macro to avoid borrow-checker issues with closures capturing &mut f.
    macro_rules! rec {
        ($e:expr) => { recursive_transform($e, f) };
    }
    macro_rules! rec_stmt {
        ($s:expr) => { transform_stmt($s, f) };
    }

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(rec!(*object)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rec!(*callee)),
                },
                other => other,
            },
            args: args.into_iter().map(|e| rec!(e)).collect(),
            type_args,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rec!(*left)), right: Box::new(rec!(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rec!(*operand)),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rec!(*cond)),
            then: Box::new(rec!(*then)),
            else_: Box::new(rec!(*else_)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rec!(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rec!(g)),
                body: rec!(arm.body),
            }).collect(),
        },
        IrExprKind::Block { stmts, expr: tail } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rec_stmt!(s)).collect(),
            expr: tail.map(|e| Box::new(rec!(*e))),
        },

        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rec!(*body)), lambda_id,
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rec!(*iterable)),
            body: body.into_iter().map(|s| rec_stmt!(s)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rec!(*cond)),
            body: body.into_iter().map(|s| rec_stmt!(s)).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rec!(v))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rec!(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rec!(v))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rec!(k), rec!(v))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rec!(*start)), end: Box::new(rec!(*end)), inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rec!(*object)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rec!(*object)), index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rec!(*object)), index: Box::new(rec!(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rec!(*object)), key: Box::new(rec!(*key)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr: e } => IrStringPart::Expr { expr: rec!(e) },
                other => other,
            }).collect(),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|e| rec!(e)).collect(),
        },
        // Single-child wrappers
        IrExprKind::ResultOk { expr: e } => IrExprKind::ResultOk { expr: Box::new(rec!(*e)) },
        IrExprKind::ResultErr { expr: e } => IrExprKind::ResultErr { expr: Box::new(rec!(*e)) },
        IrExprKind::OptionSome { expr: e } => IrExprKind::OptionSome { expr: Box::new(rec!(*e)) },
        IrExprKind::Try { expr: e } => IrExprKind::Try { expr: Box::new(rec!(*e)) },
        IrExprKind::Await { expr: e } => IrExprKind::Await { expr: Box::new(rec!(*e)) },
        IrExprKind::Clone { expr: e } => IrExprKind::Clone { expr: Box::new(rec!(*e)) },
        IrExprKind::Deref { expr: e } => IrExprKind::Deref { expr: Box::new(rec!(*e)) },
        IrExprKind::Borrow { expr: e, as_str, mutable } => IrExprKind::Borrow { expr: Box::new(rec!(*e)), as_str, mutable },
        IrExprKind::BoxNew { expr: e } => IrExprKind::BoxNew { expr: Box::new(rec!(*e)) },
        IrExprKind::ToVec { expr: e } => IrExprKind::ToVec { expr: Box::new(rec!(*e)) },
        // Leaf nodes — pass through
        other => other,
    };
    IrExpr { kind, ty, span }
}

/// Transform all expressions inside a statement.
pub(super) fn transform_stmt(
    stmt: IrStmt,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrStmt {
    macro_rules! rec {
        ($e:expr) => { recursive_transform($e, f) };
    }
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rec!(value),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rec!(value),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rec!(value) },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: rec!(index), value: rec!(value),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: rec!(key), value: rec!(value),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: rec!(value),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rec!(cond), else_: rec!(else_),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rec!(expr) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}
