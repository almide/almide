// ── IR Visitor trait (read-only) ─────────────────────────────────
//
// Centralizes IR tree walking. Each analysis pass implements IrVisitor
// and overrides only the visit_* methods it needs. The walk_* functions
// handle exhaustive recursion into child nodes.
//
// Adding a new IrExprKind variant requires updating only walk_expr —
// all passes automatically traverse the new node's children.

use super::*;

/// Read-only visitor for IR trees.
///
/// Override `visit_expr` / `visit_stmt` / `visit_pattern` to add custom logic.
/// Call the corresponding `walk_*` function inside your override to recurse.
pub trait IrVisitor: Sized {
    fn visit_expr(&mut self, expr: &IrExpr) { walk_expr(self, expr); }
    fn visit_stmt(&mut self, stmt: &IrStmt) { walk_stmt(self, stmt); }
    fn visit_pattern(&mut self, pat: &IrPattern) { walk_pattern(self, pat); }
}

/// Walk all child expressions/statements/patterns of an expression.
pub fn walk_expr<V: IrVisitor>(v: &mut V, expr: &IrExpr) {
    match &expr.kind {
        // ── Leaf nodes ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::EmptyMap | IrExprKind::OptionNone
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::Hole
        | IrExprKind::Todo { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::ClosureCreate { .. } => {}

        // ── Operators ──
        IrExprKind::BinOp { left, right, .. } => {
            v.visit_expr(left);
            v.visit_expr(right);
        }
        IrExprKind::UnOp { operand, .. } => {
            v.visit_expr(operand);
        }

        // ── Control flow ──
        IrExprKind::If { cond, then, else_ } => {
            v.visit_expr(cond);
            v.visit_expr(then);
            v.visit_expr(else_);
        }
        IrExprKind::Match { subject, arms } => {
            v.visit_expr(subject);
            for arm in arms {
                v.visit_pattern(&arm.pattern);
                if let Some(g) = &arm.guard { v.visit_expr(g); }
                v.visit_expr(&arm.body);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { v.visit_stmt(s); }
            if let Some(e) = expr { v.visit_expr(e); }
        }

        // ── Loops ──
        IrExprKind::ForIn { iterable, body, .. } => {
            v.visit_expr(iterable);
            for s in body { v.visit_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            v.visit_expr(cond);
            for s in body { v.visit_stmt(s); }
        }

        // ── Calls ──
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            match target {
                CallTarget::Method { object, .. } => v.visit_expr(object),
                CallTarget::Computed { callee } => v.visit_expr(callee),
                _ => {}
            }
            for a in args { v.visit_expr(a); }
        }
        IrExprKind::RuntimeCall { args, .. } => {
            for a in args { v.visit_expr(a); }
        }

        // ── Collections ──
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { v.visit_expr(e); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, val) in fields { v.visit_expr(val); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            v.visit_expr(base);
            for (_, val) in fields { v.visit_expr(val); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, val) in entries { v.visit_expr(k); v.visit_expr(val); }
        }
        IrExprKind::Range { start, end, .. } => {
            v.visit_expr(start);
            v.visit_expr(end);
        }

        // ── Access ──
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            v.visit_expr(object);
        }
        IrExprKind::IndexAccess { object, index } => {
            v.visit_expr(object);
            v.visit_expr(index);
        }
        IrExprKind::MapAccess { object, key } => {
            v.visit_expr(object);
            v.visit_expr(key);
        }

        // ── Functions ──
        IrExprKind::Lambda { body, .. } => {
            v.visit_expr(body);
        }

        // ── Strings ──
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { v.visit_expr(expr); }
            }
        }

        // ── Wrappers (single child) ──
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Await { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::RcWrap { expr: e, .. }
        | IrExprKind::ToVec { expr: e } => {
            v.visit_expr(e);
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            v.visit_expr(e);
            v.visit_expr(f);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { v.visit_expr(a); }
        }
        IrExprKind::InlineRust { args, .. } => {
            for (_, a) in args { v.visit_expr(a); }
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            v.visit_expr(source);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        v.visit_expr(lambda);
                    }
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => { v.visit_expr(init); v.visit_expr(lambda); }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                    v.visit_expr(lambda);
                }
            }
        }
    }
}

/// Walk all child expressions/patterns of a statement.
pub fn walk_stmt<V: IrVisitor>(v: &mut V, stmt: &IrStmt) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => {
            v.visit_expr(value);
        }
        IrStmtKind::BindDestructure { pattern, value } => {
            v.visit_pattern(pattern);
            v.visit_expr(value);
        }
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            v.visit_expr(value);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            v.visit_expr(index);
            v.visit_expr(value);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            v.visit_expr(key);
            v.visit_expr(value);
        }
        IrStmtKind::Guard { cond, else_ } => {
            v.visit_expr(cond);
            v.visit_expr(else_);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            v.visit_expr(a);
            v.visit_expr(b);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            v.visit_expr(end);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            v.visit_expr(len);
        }
        IrStmtKind::Expr { expr } => {
            v.visit_expr(expr);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Walk all child patterns of a pattern.
pub fn walk_pattern<V: IrVisitor>(v: &mut V, pat: &IrPattern) {
    match pat {
        IrPattern::Wildcard | IrPattern::Bind { .. } | IrPattern::None => {}
        IrPattern::Literal { expr } => v.visit_expr(expr),
        IrPattern::Constructor { args, .. } => {
            for a in args { v.visit_pattern(a); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { v.visit_pattern(p); }
            }
        }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => {
            for e in elements { v.visit_pattern(e); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            v.visit_pattern(inner);
        }
    }
}
