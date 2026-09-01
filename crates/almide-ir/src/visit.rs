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
///
/// Arms are grouped by CHILD SHAPE rather than by node family: every
/// variant whose traversal is "visit one child" shares one arm, every
/// "visit two children" variant shares the next, and so on. Or-pattern
/// binding renames (`Member { object: e, .. }`) make the shapes line up.
/// The match stays exhaustive with no wildcard, so a new `IrExprKind`
/// variant is still a compile error here until its shape is declared.
pub fn walk_expr<V: IrVisitor>(v: &mut V, expr: &IrExpr) {
    match &expr.kind {
        // ── No children ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::EmptyMap | IrExprKind::OptionNone
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::Hole
        | IrExprKind::Todo { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::ClosureCreate { .. } => {}

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. } | IrExprKind::Lambda { body: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionalChain { expr: e, .. }
        | IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::RcWrap { expr: e, .. } | IrExprKind::ToVec { expr: e } => {
            v.visit_expr(e);
        }

        // ── Two children, left to right ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::Range { start: a, end: b, .. }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::UnwrapOr { expr: a, fallback: b } => {
            v.visit_expr(a);
            v.visit_expr(b);
        }

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => {
            v.visit_expr(cond);
            v.visit_expr(then);
            v.visit_expr(else_);
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs }
        | IrExprKind::Fan { exprs: xs } | IrExprKind::RuntimeCall { args: xs, .. }
        | IrExprKind::RustMacro { args: xs, .. } => walk_expr_each(v, xs),

        // ── Name-tagged children (record fields, inline-Rust args) ──
        IrExprKind::Record { fields, .. } | IrExprKind::InlineRust { args: fields, .. } => {
            walk_expr_fields(v, fields)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            v.visit_expr(base);
            walk_expr_fields(v, fields);
        }

        // ── Shapes with their own traversal order ──
        IrExprKind::Match { subject, arms } => walk_expr_match(v, subject, arms),
        IrExprKind::Block { stmts, expr } => walk_expr_block(v, stmts, expr.as_deref()),
        IrExprKind::ForIn { iterable: lead, body, .. }
        | IrExprKind::While { cond: lead, body } => walk_expr_loop_body(v, lead, body),
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            walk_expr_call(v, target, args)
        }
        IrExprKind::MapLiteral { entries } => walk_expr_map_entries(v, entries),
        IrExprKind::StringInterp { parts } => walk_expr_string_interp(v, parts),
        IrExprKind::IterChain { source, steps, collector, .. } => {
            walk_expr_iter_chain(v, source, steps, collector)
        }
    }
}

/// The "flat sequence of children" arm of [`walk_expr`].
fn walk_expr_each<V: IrVisitor>(v: &mut V, exprs: &[IrExpr]) {
    for e in exprs { v.visit_expr(e); }
}

/// `Block` arm of [`walk_expr`]: every statement, then the tail expression.
fn walk_expr_block<V: IrVisitor>(v: &mut V, stmts: &[IrStmt], tail: Option<&IrExpr>) {
    for s in stmts { v.visit_stmt(s); }
    if let Some(e) = tail { v.visit_expr(e); }
}

/// `MapLiteral` arm of [`walk_expr`]: each entry's key, then its value.
fn walk_expr_map_entries<V: IrVisitor>(v: &mut V, entries: &[(IrExpr, IrExpr)]) {
    for (k, val) in entries { v.visit_expr(k); v.visit_expr(val); }
}

/// `Match` arm of [`walk_expr`]: subject + per-arm pattern/guard/body.
fn walk_expr_match<V: IrVisitor>(v: &mut V, subject: &IrExpr, arms: &[IrMatchArm]) {
    v.visit_expr(subject);
    for arm in arms {
        v.visit_pattern(&arm.pattern);
        if let Some(g) = &arm.guard { v.visit_expr(g); }
        v.visit_expr(&arm.body);
    }
}

/// `Call`/`TailCall` arm of [`walk_expr`]: resolve the call target, then args.
fn walk_expr_call<V: IrVisitor>(v: &mut V, target: &CallTarget, args: &[IrExpr]) {
    match target {
        CallTarget::Method { object, .. } => v.visit_expr(object),
        CallTarget::Computed { callee } => v.visit_expr(callee),
        _ => {}
    }
    for a in args { v.visit_expr(a); }
}

/// `IterChain` arm of [`walk_expr`]: source, then each step's lambda, then the collector.
fn walk_expr_iter_chain<V: IrVisitor>(
    v: &mut V,
    source: &IrExpr,
    steps: &[IterStep],
    collector: &IterCollector,
) {
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

/// `ForIn`/`While` arms of [`walk_expr`]: both are "visit the loop-controlling
/// expression (iterable / cond), then walk every body statement" — identical
/// shape, so they share one helper.
fn walk_expr_loop_body<V: IrVisitor>(v: &mut V, lead: &IrExpr, body: &[IrStmt]) {
    v.visit_expr(lead);
    for s in body { v.visit_stmt(s); }
}

/// Name-tagged child loop shared by [`walk_expr`] — `Record` /
/// `SpreadRecord` fields and `InlineRust` args all carry `(Sym, IrExpr)`
/// pairs whose name plays no part in traversal.
fn walk_expr_fields<V: IrVisitor>(v: &mut V, fields: &[(Sym, IrExpr)]) {
    for (_, val) in fields { v.visit_expr(val); }
}

/// `StringInterp` arm of [`walk_expr`]: visit each interpolated sub-expression.
fn walk_expr_string_interp<V: IrVisitor>(v: &mut V, parts: &[IrStringPart]) {
    for p in parts {
        if let IrStringPart::Expr { expr } = p { v.visit_expr(expr); }
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
        IrStmtKind::RcInc { .. } => {}
        IrStmtKind::RcDec { .. } => {}
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
        IrPattern::Tuple { elements } => {
            for e in elements { v.visit_pattern(e); }
        }
        IrPattern::List { elements, rest } => {
            for e in elements { v.visit_pattern(e); }
            if let Some(r) = rest { v.visit_pattern(r); }
        }
        IrPattern::As { inner, .. } => v.visit_pattern(inner),
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            v.visit_pattern(inner);
        }
    }
}
