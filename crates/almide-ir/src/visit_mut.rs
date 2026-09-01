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
///
/// Mirrors [`crate::visit::walk_expr`] arm for arm: grouped by CHILD SHAPE
/// (no children / one / two / three / sequence / name-tagged / bespoke),
/// exhaustive with no wildcard so a new `IrExprKind` variant fails to
/// compile here until its shape is declared.
pub fn walk_expr_mut<V: IrMutVisitor>(v: &mut V, expr: &mut IrExpr) {
    match &mut expr.kind {
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
            v.visit_expr_mut(e);
        }

        // ── Two children, left to right ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::Range { start: a, end: b, .. }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::UnwrapOr { expr: a, fallback: b } => {
            v.visit_expr_mut(a);
            v.visit_expr_mut(b);
        }

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => {
            v.visit_expr_mut(cond);
            v.visit_expr_mut(then);
            v.visit_expr_mut(else_);
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs }
        | IrExprKind::Fan { exprs: xs } | IrExprKind::RuntimeCall { args: xs, .. }
        | IrExprKind::RustMacro { args: xs, .. } => walk_expr_mut_each(v, xs),

        // ── Name-tagged children (record fields, inline-Rust args) ──
        IrExprKind::Record { fields, .. } | IrExprKind::InlineRust { args: fields, .. } => {
            walk_expr_mut_fields(v, fields)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            v.visit_expr_mut(base);
            walk_expr_mut_fields(v, fields);
        }

        // ── Shapes with their own traversal order ──
        IrExprKind::Match { subject, arms } => walk_expr_mut_match(v, subject, arms),
        IrExprKind::Block { stmts, expr } => walk_expr_mut_block(v, stmts, expr.as_deref_mut()),
        IrExprKind::ForIn { iterable: lead, body, .. }
        | IrExprKind::While { cond: lead, body } => walk_expr_mut_loop_body(v, lead, body),
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            walk_expr_mut_call(v, target, args)
        }
        IrExprKind::MapLiteral { entries } => walk_expr_mut_map_entries(v, entries),
        IrExprKind::StringInterp { parts } => walk_expr_mut_string_interp(v, parts),
        IrExprKind::IterChain { source, steps, collector, .. } => {
            walk_expr_mut_iter_chain(v, source, steps, collector)
        }
    }
}

/// The "flat sequence of children" arm of [`walk_expr_mut`].
fn walk_expr_mut_each<V: IrMutVisitor>(v: &mut V, exprs: &mut [IrExpr]) {
    for e in exprs { v.visit_expr_mut(e); }
}

/// `Block` arm of [`walk_expr_mut`]: every statement, then the tail expression.
fn walk_expr_mut_block<V: IrMutVisitor>(v: &mut V, stmts: &mut [IrStmt], tail: Option<&mut IrExpr>) {
    for s in stmts { v.visit_stmt_mut(s); }
    if let Some(e) = tail { v.visit_expr_mut(e); }
}

/// `MapLiteral` arm of [`walk_expr_mut`]: each entry's key, then its value.
fn walk_expr_mut_map_entries<V: IrMutVisitor>(v: &mut V, entries: &mut [(IrExpr, IrExpr)]) {
    for (k, val) in entries { v.visit_expr_mut(k); v.visit_expr_mut(val); }
}

/// `Match` arm of [`walk_expr_mut`]: subject + per-arm pattern/guard/body.
fn walk_expr_mut_match<V: IrMutVisitor>(v: &mut V, subject: &mut IrExpr, arms: &mut [IrMatchArm]) {
    v.visit_expr_mut(subject);
    for arm in arms {
        v.visit_pattern_mut(&mut arm.pattern);
        if let Some(g) = &mut arm.guard { v.visit_expr_mut(g); }
        v.visit_expr_mut(&mut arm.body);
    }
}

/// `Call`/`TailCall` arm of [`walk_expr_mut`]: resolve the call target, then args.
fn walk_expr_mut_call<V: IrMutVisitor>(v: &mut V, target: &mut CallTarget, args: &mut [IrExpr]) {
    match target {
        CallTarget::Method { object, .. } => v.visit_expr_mut(object),
        CallTarget::Computed { callee } => v.visit_expr_mut(callee),
        _ => {}
    }
    for a in args { v.visit_expr_mut(a); }
}

/// `IterChain` arm of [`walk_expr_mut`]: source, then each step's lambda, then the collector.
fn walk_expr_mut_iter_chain<V: IrMutVisitor>(
    v: &mut V,
    source: &mut IrExpr,
    steps: &mut [IterStep],
    collector: &mut IterCollector,
) {
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

/// `ForIn`/`While` arms of [`walk_expr_mut`]: both are "visit the
/// loop-controlling expression (iterable / cond), then walk every body
/// statement" — identical shape, so they share one helper.
fn walk_expr_mut_loop_body<V: IrMutVisitor>(v: &mut V, lead: &mut IrExpr, body: &mut [IrStmt]) {
    v.visit_expr_mut(lead);
    for s in body { v.visit_stmt_mut(s); }
}

/// Name-tagged child loop shared by [`walk_expr_mut`] — `Record` /
/// `SpreadRecord` fields and `InlineRust` args all carry `(Sym, IrExpr)`
/// pairs whose name plays no part in traversal.
fn walk_expr_mut_fields<V: IrMutVisitor>(v: &mut V, fields: &mut [(Sym, IrExpr)]) {
    for (_, val) in fields { v.visit_expr_mut(val); }
}

/// `StringInterp` arm of [`walk_expr_mut`]: visit each interpolated sub-expression.
fn walk_expr_mut_string_interp<V: IrMutVisitor>(v: &mut V, parts: &mut [IrStringPart]) {
    for p in parts {
        if let IrStringPart::Expr { expr } = p { v.visit_expr_mut(expr); }
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
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
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
        IrPattern::Tuple { elements } => {
            for e in elements { v.visit_pattern_mut(e); }
        }
        IrPattern::List { elements, rest } => {
            for e in elements { v.visit_pattern_mut(e); }
            if let Some(r) = rest { v.visit_pattern_mut(r); }
        }
        IrPattern::As { inner, .. } => v.visit_pattern_mut(inner),
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            v.visit_pattern_mut(inner);
        }
    }
}
