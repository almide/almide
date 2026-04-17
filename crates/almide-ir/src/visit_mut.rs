// ── IR mutable visitor trait ─────────────────────────────────────────
//
// Mutable counterpart to `IrVisitor`. Lets passes rewrite IR nodes in place
// without rebuilding the full tree by hand. Replaces 200+ lines of manual
// match/clone boilerplate (e.g. rewrite_var_ids in pass_closure_conversion).
//
// Override visit_expr_mut / visit_stmt_mut / visit_pattern_mut to run custom
// logic. Call walk_expr_mut etc. inside your override to recurse.
//
// Adding a new IrExprKind variant requires updating only walk_expr_mut —
// all passes automatically traverse the new node's children.

use super::*;

/// Mutable visitor for IR trees.
///
/// Override `visit_expr_mut` / `visit_stmt_mut` / `visit_pattern_mut` to add
/// custom logic. Call the corresponding `walk_*_mut` function to recurse.
pub trait IrMutVisitor: Sized {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) { walk_expr_mut(self, expr); }
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) { walk_stmt_mut(self, stmt); }
    fn visit_pattern_mut(&mut self, pat: &mut IrPattern) { walk_pattern_mut(self, pat); }
}

/// Walk into all child expressions/statements/patterns of an expression.
pub fn walk_expr_mut<V: IrMutVisitor>(v: &mut V, expr: &mut IrExpr) {
    match &mut expr.kind {
        // ── Leaf nodes ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::EmptyMap | IrExprKind::OptionNone
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::Hole
        | IrExprKind::Todo { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::ClosureCreate { .. } => {}

        // ── Operators ──
        IrExprKind::BinOp { left, right, .. } => {
            v.visit_expr_mut(left);
            v.visit_expr_mut(right);
        }
        IrExprKind::UnOp { operand, .. } => {
            v.visit_expr_mut(operand);
        }

        // ── Control flow ──
        IrExprKind::If { cond, then, else_ } => {
            v.visit_expr_mut(cond);
            v.visit_expr_mut(then);
            v.visit_expr_mut(else_);
        }
        IrExprKind::Match { subject, arms } => {
            v.visit_expr_mut(subject);
            for arm in arms {
                v.visit_pattern_mut(&mut arm.pattern);
                if let Some(g) = &mut arm.guard { v.visit_expr_mut(g); }
                v.visit_expr_mut(&mut arm.body);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { v.visit_stmt_mut(s); }
            if let Some(e) = expr { v.visit_expr_mut(e); }
        }

        // ── Loops ──
        IrExprKind::ForIn { iterable, body, .. } => {
            v.visit_expr_mut(iterable);
            for s in body { v.visit_stmt_mut(s); }
        }
        IrExprKind::While { cond, body } => {
            v.visit_expr_mut(cond);
            for s in body { v.visit_stmt_mut(s); }
        }

        // ── Calls ──
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            match target {
                CallTarget::Method { object, .. } => v.visit_expr_mut(object),
                CallTarget::Computed { callee } => v.visit_expr_mut(callee),
                _ => {}
            }
            for a in args { v.visit_expr_mut(a); }
        }

        // ── Collections ──
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { v.visit_expr_mut(e); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, val) in fields { v.visit_expr_mut(val); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            v.visit_expr_mut(base);
            for (_, val) in fields { v.visit_expr_mut(val); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, val) in entries { v.visit_expr_mut(k); v.visit_expr_mut(val); }
        }
        IrExprKind::Range { start, end, .. } => {
            v.visit_expr_mut(start);
            v.visit_expr_mut(end);
        }

        // ── Access ──
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            v.visit_expr_mut(object);
        }
        IrExprKind::IndexAccess { object, index } => {
            v.visit_expr_mut(object);
            v.visit_expr_mut(index);
        }
        IrExprKind::MapAccess { object, key } => {
            v.visit_expr_mut(object);
            v.visit_expr_mut(key);
        }

        // ── Functions ──
        IrExprKind::Lambda { body, .. } => {
            v.visit_expr_mut(body);
        }

        // ── Strings ──
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { v.visit_expr_mut(expr); }
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
            v.visit_expr_mut(e);
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            v.visit_expr_mut(e);
            v.visit_expr_mut(f);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { v.visit_expr_mut(a); }
        }
        IrExprKind::InlineRust { args, .. } => {
            for (_, a) in args { v.visit_expr_mut(a); }
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            v.visit_expr_mut(source);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        v.visit_expr_mut(lambda);
                    }
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => { v.visit_expr_mut(init); v.visit_expr_mut(lambda); }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                    v.visit_expr_mut(lambda);
                }
            }
        }
    }
}

/// Walk into all child expressions/patterns of a statement.
pub fn walk_stmt_mut<V: IrMutVisitor>(v: &mut V, stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => {
            v.visit_expr_mut(value);
        }
        IrStmtKind::BindDestructure { pattern, value } => {
            v.visit_pattern_mut(pattern);
            v.visit_expr_mut(value);
        }
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            v.visit_expr_mut(value);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            v.visit_expr_mut(index);
            v.visit_expr_mut(value);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            v.visit_expr_mut(key);
            v.visit_expr_mut(value);
        }
        IrStmtKind::Guard { cond, else_ } => {
            v.visit_expr_mut(cond);
            v.visit_expr_mut(else_);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            v.visit_expr_mut(a);
            v.visit_expr_mut(b);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            v.visit_expr_mut(end);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            v.visit_expr_mut(len);
        }
        IrStmtKind::Expr { expr } => {
            v.visit_expr_mut(expr);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Walk into all child patterns of a pattern.
pub fn walk_pattern_mut<V: IrMutVisitor>(v: &mut V, pat: &mut IrPattern) {
    match pat {
        IrPattern::Wildcard | IrPattern::Bind { .. } | IrPattern::None => {}
        IrPattern::Literal { expr } => v.visit_expr_mut(expr),
        IrPattern::Constructor { args, .. } => {
            for a in args { v.visit_pattern_mut(a); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &mut f.pattern { v.visit_pattern_mut(p); }
            }
        }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => {
            for e in elements { v.visit_pattern_mut(e); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            v.visit_pattern_mut(inner);
        }
    }
}
