// ── Use-count computation (post-pass) ───────────────────────────

use std::borrow::Cow;
use std::collections::HashSet;
use super::*;
use crate::visit::{walk_expr, IrVisitor};

/// Walk the entire IR program and count variable uses, storing results
/// in VarTable. When invoked post `UnifyVarTablesPass` every
/// module-scope VarId indexes into `program.var_table`, so the walk
/// covers module functions / top_lets as well; when invoked before
/// (e.g. the optimizer's `optimize_program`) each module still owns
/// its own table and we walk it into the module-local table.
pub fn compute_use_counts(program: &mut IrProgram) {
    reset_use_counts(&mut program.var_table);
    count_uses_in_decls(&program.functions, &program.top_lets, &mut program.var_table);

    // Modules: if the module still owns a populated VarTable (i.e.
    // unify hasn't run yet or the module happens to be empty), count
    // into the module's own table — that's what pre-unify callers
    // like `almide_optimize::optimize_program` rely on. After unify,
    // the module's table is empty and its VarIds live in
    // `program.var_table`, so we walk there instead.
    for module in program.modules.iter_mut() {
        if module.var_table.entries.is_empty() {
            count_uses_in_decls(&module.functions, &module.top_lets, &mut program.var_table);
        } else {
            reset_use_counts(&mut module.var_table);
            count_uses_in_decls(&module.functions, &module.top_lets, &mut module.var_table);
        }
    }
}

/// Zero out every use-count slot in `table`, in place.
fn reset_use_counts(table: &mut VarTable) {
    for i in 0..table.len() {
        table.entries[i].use_count = 0;
    }
}

/// Count uses in every function body and top-level let value, into `table`.
fn count_uses_in_decls(functions: &[IrFunction], top_lets: &[IrTopLet], table: &mut VarTable) {
    for func in functions {
        count_uses_in_expr(&func.body, table);
    }
    for tl in top_lets {
        count_uses_in_expr(&tl.value, table);
    }
}

/// Grouped by CHILD SHAPE, mirroring [`crate::visit::walk_expr`]: every variant
/// with the same traversal shape shares one arm (or-pattern binding renames line
/// the field names up). Exhaustive with no wildcard, so a new `IrExprKind`
/// variant is a compile error here until its shape is declared.
fn count_uses_in_expr(expr: &IrExpr, table: &mut VarTable) {
    match &expr.kind {
        IrExprKind::Var { id } => table.increment_use(*id),

        // ── No children (FnRef / ClosureCreate name a function, not a VarId) ──
        IrExprKind::FnRef { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::EmptyMap
        | IrExprKind::EnvLoad { .. } | IrExprKind::ClosureCreate { .. } => {}

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionalChain { expr: e, .. }
        | IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::RcWrap { expr: e, .. } | IrExprKind::ToVec { expr: e } => {
            count_uses_in_expr(e, table);
        }

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::Range { start: a, end: b, .. }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::UnwrapOr { expr: a, fallback: b } => {
            count_uses_in_expr(a, table);
            count_uses_in_expr(b, table);
        }

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(then, table);
            count_uses_in_expr(else_, table);
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs }
        | IrExprKind::Fan { exprs: xs } | IrExprKind::RuntimeCall { args: xs, .. }
        | IrExprKind::RustMacro { args: xs, .. } => count_uses_in_each(xs, table),

        // ── Name-tagged children (record fields, inline-Rust args) ──
        IrExprKind::Record { fields, .. } | IrExprKind::InlineRust { args: fields, .. } => {
            count_uses_in_fields(fields, table)
        }
        IrExprKind::SpreadRecord { base, fields } => {
            count_uses_in_expr(base, table);
            count_uses_in_fields(fields, table);
        }

        // ── Shapes with their own counting rule ──
        IrExprKind::Match { subject, arms } => count_uses_in_match(subject, arms, table),
        IrExprKind::Block { stmts, expr } => count_uses_in_block(stmts, expr.as_deref(), table),
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            count_uses_in_call(target, args, table)
        }
        IrExprKind::MapLiteral { entries } => count_uses_in_map_entries(entries, table),
        // ForIn/While: count the loop body once, then extra-count every
        // outer-scope var referenced in it (both need the same treatment —
        // a loop body runs N times, so outer captures are used N times too).
        // The ForIn's own binders are rebound on every iteration, so they
        // are body-locals, not outer captures — bumping them made every
        // `for v in xs { f(v) }` clone `v` for a body that consumed it once
        // (#1673).
        IrExprKind::ForIn { iterable, body, var, var_tuple } => {
            let mut loop_vars = vec![*var];
            if let Some(t) = var_tuple { loop_vars.extend(t.iter().copied()); }
            count_uses_in_loop_body(iterable, body, &loop_vars, table)
        }
        IrExprKind::While { cond, body } => count_uses_in_loop_body(cond, body, &[], table),
        IrExprKind::Lambda { params, body, .. } => count_uses_in_lambda(params, body, table),
        IrExprKind::StringInterp { parts } => count_uses_in_string_interp(parts, table),
        IrExprKind::IterChain { source, steps, collector, .. } => {
            count_uses_in_iter_chain(source, steps, collector, table)
        }
    }
}

/// The "flat sequence of children" arm of [`count_uses_in_expr`].
fn count_uses_in_each(exprs: &[IrExpr], table: &mut VarTable) {
    for e in exprs { count_uses_in_expr(e, table); }
}

/// `Block` arm of [`count_uses_in_expr`]: statements, then the tail expression.
fn count_uses_in_block(stmts: &[IrStmt], tail: Option<&IrExpr>, table: &mut VarTable) {
    for s in stmts { count_uses_in_stmt(s, table); }
    if let Some(e) = tail { count_uses_in_expr(e, table); }
}

/// `MapLiteral` arm of [`count_uses_in_expr`]: each entry's key, then its value.
fn count_uses_in_map_entries(entries: &[(IrExpr, IrExpr)], table: &mut VarTable) {
    for (k, v) in entries {
        count_uses_in_expr(k, table);
        count_uses_in_expr(v, table);
    }
}

/// `Match` arm of [`count_uses_in_expr`]: subject + each arm's guard/body.
fn count_uses_in_match(subject: &IrExpr, arms: &[IrMatchArm], table: &mut VarTable) {
    count_uses_in_expr(subject, table);
    for arm in arms {
        if let Some(g) = &arm.guard { count_uses_in_expr(g, table); }
        count_uses_in_expr(&arm.body, table);
    }
}

/// `Call`/`TailCall` arm of [`count_uses_in_expr`]: resolve the call target, then args.
fn count_uses_in_call(target: &CallTarget, args: &[IrExpr], table: &mut VarTable) {
    match target {
        CallTarget::Method { object, .. } => count_uses_in_expr(object, table),
        CallTarget::Computed { callee } => count_uses_in_expr(callee, table),
        _ => {}
    }
    for a in args { count_uses_in_expr(a, table); }
}

/// `ForIn`/`While` arms of [`count_uses_in_expr`]: count the loop-controlling
/// expression, then the body normally, then extra-count outer-scope vars
/// referenced in the body (a loop body runs N times, so outer captures used
/// inside it are used N times too — this is what triggers clone insertion).
fn count_uses_in_loop_body(lead: &IrExpr, body: &[IrStmt], loop_vars: &[VarId], table: &mut VarTable) {
    count_uses_in_expr(lead, table);
    let mut body_locals = collect_bound_vars(body);
    body_locals.extend(loop_vars.iter().map(|v| v.0));
    for s in body { count_uses_in_stmt(s, table); }
    bump_outer_vars_in_loop(body, &body_locals, table);
}

/// `Record`/`SpreadRecord` field-value loop shared by [`count_uses_in_expr`].
fn count_uses_in_fields(fields: &[(Sym, IrExpr)], table: &mut VarTable) {
    for (_, e) in fields { count_uses_in_expr(e, table); }
}

/// `StringInterp` arm of [`count_uses_in_expr`]: count each interpolated sub-expression.
fn count_uses_in_string_interp(parts: &[IrStringPart], table: &mut VarTable) {
    for part in parts {
        if let IrStringPart::Expr { expr } = part {
            count_uses_in_expr(expr, table);
        }
    }
}

/// `Lambda` arm of [`count_uses_in_expr`]: count the body normally, then
/// extra-count outer-scope vars captured by the closure (captures move by
/// default, so a var used inside the lambda body is used once per call).
fn count_uses_in_lambda(params: &[(VarId, Ty)], body: &IrExpr, table: &mut VarTable) {
    count_uses_in_expr(body, table);
    let mut lambda_locals: HashSet<u32> = params.iter().map(|(v, _)| v.0).collect();
    if let IrExprKind::Block { stmts, .. } = &body.kind {
        lambda_locals.extend(collect_bound_vars(stmts));
    }
    bump_vars_in_expr(body, &lambda_locals, table);
}

/// `IterChain` arm of [`count_uses_in_expr`]: source, then each step's lambda,
/// then the collector.
fn count_uses_in_iter_chain(
    source: &IrExpr,
    steps: &[IterStep],
    collector: &IterCollector,
    table: &mut VarTable,
) {
    count_uses_in_expr(source, table);
    for step in steps {
        match step {
            IterStep::Map { lambda } | IterStep::Filter { lambda }
            | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                count_uses_in_expr(lambda, table);
            }
            IterStep::Take { n } => count_uses_in_expr(n, table),
            IterStep::Enumerate => {}
        }
    }
    match collector {
        IterCollector::Collect | IterCollector::Sum { .. } | IterCollector::Len => {}
        IterCollector::Fold { init, lambda } => {
            count_uses_in_expr(init, table);
            count_uses_in_expr(lambda, table);
        }
        IterCollector::Any { lambda } | IterCollector::All { lambda }
        | IterCollector::Find { lambda } | IterCollector::Count { lambda }
        | IterCollector::FindIndex { lambda } | IterCollector::FindMap { lambda } => {
            count_uses_in_expr(lambda, table);
        }
    }
}

fn count_uses_in_stmt(stmt: &IrStmt, table: &mut VarTable) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            count_uses_in_expr(index, table);
            count_uses_in_expr(value, table);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            count_uses_in_expr(key, table);
            count_uses_in_expr(value, table);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            count_uses_in_expr(value, table);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            count_uses_in_expr(a, table);
            count_uses_in_expr(b, table);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            count_uses_in_expr(end, table);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            count_uses_in_expr(len, table);
        }
        IrStmtKind::Expr { expr } => {
            count_uses_in_expr(expr, table);
        }
        IrStmtKind::Guard { cond, else_ } => {
            count_uses_in_expr(cond, table);
            count_uses_in_expr(else_, table);
        }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => {
            table.increment_use(*var);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Collect VarIds that are bound (let/var) inside a list of statements.
fn collect_bound_vars(stmts: &[IrStmt]) -> HashSet<u32> {
    let mut locals = HashSet::new();
    for s in stmts {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                locals.insert(var.0);
                collect_bound_vars_in_expr(value, &mut locals);
            }
            IrStmtKind::BindDestructure { pattern, value, .. } => {
                collect_pattern_vars(pattern, &mut locals);
                collect_bound_vars_in_expr(value, &mut locals);
            }
            IrStmtKind::Expr { expr } => collect_bound_vars_in_expr(expr, &mut locals),
            IrStmtKind::Guard { cond, else_ } => {
                collect_bound_vars_in_expr(cond, &mut locals);
                collect_bound_vars_in_expr(else_, &mut locals);
            }
            _ => {}
        }
    }
    locals
}

/// Collect VarIds bound inside expressions (match arm patterns, nested blocks).
fn collect_bound_vars_in_expr(expr: &IrExpr, vars: &mut HashSet<u32>) {
    match &expr.kind {
        IrExprKind::Match { subject, arms } => {
            collect_bound_vars_in_expr(subject, vars);
            for arm in arms {
                collect_pattern_vars(&arm.pattern, vars);
                if let Some(g) = &arm.guard { collect_bound_vars_in_expr(g, vars); }
                collect_bound_vars_in_expr(&arm.body, vars);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            vars.extend(collect_bound_vars(stmts));
            if let Some(e) = expr { collect_bound_vars_in_expr(e, vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_bound_vars_in_expr(cond, vars);
            collect_bound_vars_in_expr(then, vars);
            collect_bound_vars_in_expr(else_, vars);
        }
        IrExprKind::ForIn { iterable, body, var, var_tuple, .. } => {
            collect_bound_vars_in_expr(iterable, vars);
            vars.insert(var.0);
            if let Some(vt) = var_tuple { for v in vt { vars.insert(v.0); } }
            vars.extend(collect_bound_vars(body));
        }
        IrExprKind::While { cond, body } => {
            collect_bound_vars_in_expr(cond, vars);
            vars.extend(collect_bound_vars(body));
        }
        _ => {}
    }
}

fn collect_pattern_vars(pat: &IrPattern, vars: &mut HashSet<u32>) {
    match pat {
        IrPattern::Bind { var, .. } => { vars.insert(var.0); }
        IrPattern::Constructor { args, .. } => { for a in args { collect_pattern_vars(a, vars); } }
        IrPattern::Tuple { elements } => { for e in elements { collect_pattern_vars(e, vars); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => collect_pattern_vars(inner, vars),
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &f.pattern { collect_pattern_vars(p, vars); } }
        }
        _ => {}
    }
}

/// Extra-count Var references in loop body for variables defined OUTSIDE the loop.
/// This makes use_count > 1 for outer vars, triggering clone insertion.
fn bump_outer_vars_in_loop(stmts: &[IrStmt], locals: &HashSet<u32>, table: &mut VarTable) {
    let mut b = OuterBump { locals: Cow::Borrowed(locals), table };
    for s in stmts {
        b.visit_stmt(s);
    }
}

/// Bump every `Var` reference under `expr` whose id is not in `locals`.
fn bump_vars_in_expr(expr: &IrExpr, locals: &HashSet<u32>, table: &mut VarTable) {
    OuterBump { locals: Cow::Borrowed(locals), table }.visit_expr(expr);
}

/// The extra count for a body that runs many times (a loop body, a closure
/// body): every reference to a var bound OUTSIDE it, at any depth and in
/// EVERY node kind — through the exhaustive `walk_expr`, not a hand-picked
/// subset of shapes. The hand-picked walk skipped iterator chains, so a fused
/// `list.find(ids, f)` in a loop body left `ids` single-use, it moved on the
/// first iteration and rustc refused the program (E0382, #3208); a match
/// guard, a method receiver and inline-Rust / macro args were skipped the
/// same way. A nested closure's own params and the lets in its body are fresh
/// per call, so they are locals of that closure, never bumped.
struct OuterBump<'a, 't> {
    locals: Cow<'a, HashSet<u32>>,
    table: &'t mut VarTable,
}

impl IrVisitor for OuterBump<'_, '_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Var { id } => {
                if !self.locals.contains(&id.0) {
                    self.table.increment_use(*id);
                }
            }
            IrExprKind::Lambda { params, body, .. } => {
                let mut inner = self.locals.clone().into_owned();
                inner.extend(params.iter().map(|(v, _)| v.0));
                collect_bound_vars_in_expr(body, &mut inner);
                OuterBump { locals: Cow::Owned(inner), table: self.table }.visit_expr(body);
            }
            _ => walk_expr(self, expr),
        }
    }
}

/// Demote `var` to `let` for variables that are never reassigned.
/// This is a post-pass optimization that runs after compute_use_counts.
///
/// Only an `Assign`-family statement counts as a write here. A caller that can
/// resolve callees should use [`demote_unused_mut_with`], which also counts a
/// var handed to a `mut` param.
pub fn demote_unused_mut(program: &mut IrProgram) {
    demote_unused_mut_with(program, &|_| None);
}

/// [`demote_unused_mut`], counting as written every var passed at a `mut`
/// param position of its callee (`callee_mut` answers the positions a call
/// target writes, from the callee's declaration).
///
/// A `var` changed only through `list.push(xs, 1)` or a user `add(mut xs)` is
/// still a var that changes: demoting it to `let` told native's capture pass
/// the closure could keep a snapshot, while wasm (which counts those calls)
/// made it shared storage, so a closure read `0` on native and `1` on wasm
/// (#2952).
pub fn demote_unused_mut_with(program: &mut IrProgram, callee_mut: &dyn Fn(&CallTarget) -> Option<Vec<usize>>) {
    let mut assigned_vars: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        collect_assigned_vars(&func.body, &mut assigned_vars);
        collect_mut_arg_targets(&func.body, callee_mut, &mut assigned_vars);
    }
    for i in 0..program.var_table.len() {
        if program.var_table.entries[i].mutability == Mutability::Var
            && !assigned_vars.contains(&(i as u32))
        {
            program.var_table.entries[i].mutability = Mutability::Let;
        }
    }
}

/// Every variable a call in `expr` hands to a `mut` param position (as the
/// argument itself, or as the record a field argument is read from).
pub fn collect_mut_arg_targets(
    expr: &IrExpr,
    callee_mut: &dyn Fn(&CallTarget) -> Option<Vec<usize>>,
    out: &mut HashSet<u32>,
) {
    use crate::visit::{walk_expr, IrVisitor};
    struct C<'a> {
        callee_mut: &'a dyn Fn(&CallTarget) -> Option<Vec<usize>>,
        out: &'a mut HashSet<u32>,
    }
    impl IrVisitor for C<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target, args, .. } = &e.kind {
                for i in (self.callee_mut)(target).unwrap_or_default() {
                    let place = args.get(i).map(|a| match &a.kind {
                        IrExprKind::Member { object, .. } => &object.kind,
                        k => k,
                    });
                    if let Some(IrExprKind::Var { id }) = place {
                        self.out.insert(id.0);
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    C { callee_mut, out }.visit_expr(expr);
}

/// Every variable that appears as the TARGET of an in-place mutation
/// statement anywhere in `expr` (reassignment, index/field/map write, list
/// swap/reverse/rotate/copy-slice).
///
/// These positions are NOT counted by `compute_use_counts` — a mutation is a
/// write, not a read — but they still emit a reference to the binding in the
/// generated code (`almide_index_set!(ys, …)`). Any consumer that decides a
/// binding is dead from `use_count == 0` alone must consult this set too, or
/// it strips a `let` the write-back still names (#857, E0425).
pub fn collect_assigned_vars(expr: &IrExpr, assigned: &mut HashSet<u32>) {
    use crate::visit::{IrVisitor, walk_stmt};
    struct AssignCollector<'a> { assigned: &'a mut HashSet<u32> }
    impl IrVisitor for AssignCollector<'_> {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Assign { var, .. }
                | IrStmtKind::IndexAssign { target: var, .. }
                | IrStmtKind::MapInsert { target: var, .. }
                | IrStmtKind::FieldAssign { target: var, .. }
                | IrStmtKind::ListSwap { target: var, .. }
                | IrStmtKind::ListReverse { target: var, .. }
                | IrStmtKind::ListRotateLeft { target: var, .. } => {
                    self.assigned.insert(var.0);
                }
                IrStmtKind::ListCopySlice { dst, .. } => {
                    self.assigned.insert(dst.0);
                }
                _ => {}
            }
            walk_stmt(self, stmt);
        }
    }
    AssignCollector { assigned }.visit_expr(expr);
}

/// Collect warnings for unused variables.
/// Skips: `_` prefixed names, function parameters, pattern bindings (span is None).
pub fn collect_unused_var_warnings(program: &IrProgram, file: &str) -> Vec<almide_base::Diagnostic> {
    let (param_ids, mutated) = collect_params_and_mutations(program);
    let named_callees = collect_named_callee_names(program);
    (0..program.var_table.len())
        .filter_map(|i| {
            unused_var_warning(
                &program.var_table.entries[i], i as u32, &param_ids, &mutated, &named_callees, file,
            )
        })
        .collect()
}

/// Callee names of every `CallTarget::Named` call in the program. A call to a
/// LOCAL fn-typed value (`let f = adder(3); f(5)`) lowers with a `Named`
/// target — a Sym, not a VarId — so `compute_use_counts` cannot credit the
/// binding and `f` reads as unused (#1158). Matching by NAME over-suppresses
/// under shadowing (a local named after a real free fn), which is the safe
/// direction for a warning — the #857 mutated-set precedent.
fn collect_named_callee_names(program: &IrProgram) -> HashSet<String> {
    use crate::visit::{walk_expr, IrVisitor};
    struct C(HashSet<String>);
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } = &e.kind {
                if let CallTarget::Named { name } = target {
                    self.0.insert(name.as_str().to_string());
                }
            }
            walk_expr(self, e);
        }
    }
    let mut c = C(HashSet::new());
    for func in &program.functions {
        c.visit_expr(&func.body);
    }
    for tl in &program.top_lets {
        c.visit_expr(&tl.value);
    }
    c.0
}

/// The two exclusion sets consulted by [`unused_var_warning`]: every function
/// parameter's VarId, and every VarId written in place (`ys[i] = v`, `m[k] = v`,
/// `r.f = v`, …). `use_count` misses the latter because a write is not a read,
/// so without this the warning tells the author to rename to `_ys` for a binding
/// the program genuinely writes (#857).
fn collect_params_and_mutations(program: &IrProgram) -> (HashSet<u32>, HashSet<u32>) {
    let mut param_ids: HashSet<u32> = HashSet::new();
    let mut mutated: HashSet<u32> = HashSet::new();
    for func in &program.functions {
        for p in &func.params {
            param_ids.insert(p.var.0);
        }
        collect_assigned_vars(&func.body, &mut mutated);
    }
    for tl in &program.top_lets {
        collect_assigned_vars(&tl.value, &mut mutated);
    }
    (param_ids, mutated)
}

/// A binding earns an "unused variable" warning only when it is user-named
/// (no `_` prefix, which means intentionally unused), is not a parameter,
/// carries a source span (pattern bindings and loop vars have none), is never
/// read, and is never written in place.
fn unused_var_warning(
    info: &VarInfo,
    id: u32,
    param_ids: &HashSet<u32>,
    mutated: &HashSet<u32>,
    named_callees: &HashSet<String>,
    file: &str,
) -> Option<almide_base::Diagnostic> {
    if info.name.starts_with('_') || param_ids.contains(&id) { return None; }
    if info.use_count > 0 || mutated.contains(&id) { return None; }
    // A fn-typed binding invoked by name (`f(5)`) is a use `use_count` cannot
    // see — the callee is a name-only `CallTarget::Named` (#1158).
    if named_callees.contains(info.name.as_str()) { return None; }
    let span = info.span?;
    Some(almide_base::Diagnostic::warning(
        format!("unused variable '{}'", info.name),
        format!("Prefix with '_' to suppress: _{}", info.name),
        "",
    ).at(file, span.line))
}

/// Classify a top-level let value: constant-evaluable expressions are `Const`, everything else is `Lazy`.
pub fn classify_top_let_kind(expr: &IrExpr) -> TopLetKind {
    if is_const_expr(expr, &std::collections::HashSet::new()) { TopLetKind::Const } else { TopLetKind::Lazy }
}

/// Reclassify top-level lets using a two-pass approach:
/// Pass 1: classify without cross-references (already done during lowering).
/// Pass 2: with known const VarIds, reclassify Lazy → Const for expressions
///          that reference other const top-level lets (e.g., `4.0 * PI * PI`).
pub fn reclassify_top_lets(program: &mut IrProgram) {
    // Collect VarIds of top_lets already classified as Const
    let mut const_vars: std::collections::HashSet<u32> = program.top_lets.iter()
        .filter(|tl| matches!(tl.kind, TopLetKind::Const))
        .map(|tl| tl.var.0)
        .collect();

    // Iterate until fixpoint (typically 1-2 rounds)
    loop {
        let mut changed = false;
        for tl in &mut program.top_lets {
            if matches!(tl.kind, TopLetKind::Lazy) && is_const_expr(&tl.value, &const_vars) {
                tl.kind = TopLetKind::Const;
                const_vars.insert(tl.var.0);
                changed = true;
            }
        }
        if !changed { break; }
    }
}

/// Check if an expression can be evaluated at compile time (Rust `const`).
/// Recognizes: literals, unary/binary ops on const operands, references to known const vars.
fn is_const_expr(expr: &IrExpr, const_vars: &std::collections::HashSet<u32>) -> bool {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit => true,
        // String requires heap allocation — not const-compatible in Rust
        IrExprKind::LitStr { .. } => false,
        IrExprKind::UnOp { operand, .. } => is_const_expr(operand, const_vars),
        // Integer `/` and `%` render as the almide_div!/almide_mod! totality macros
        // (abort with `Error: <msg>` + exit 1), which are not const-evaluable Rust —
        // a top-level let containing one must take the Lazy (runtime) path, where the
        // abort fires at startup exactly like wasm's top-let evaluation. Plain
        // literal-by-nonzero-literal division is folded by pass_const_fold before
        // classification, so `10 / 2` still reaches codegen as a Const literal.
        IrExprKind::BinOp { op: crate::BinOp::DivInt | crate::BinOp::ModInt, .. } => false,
        IrExprKind::BinOp { left, right, .. } => is_const_expr(left, const_vars) && is_const_expr(right, const_vars),
        IrExprKind::Var { id } => const_vars.contains(&id.0),
        _ => false,
    }
}

#[cfg(test)]
mod mut_arg_write_tests {
    use super::*;
    use almide_base::intern::sym;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Unknown, span: None, def_id: None }
    }

    /// The #2952 GATE, enumerated from the stdlib declarations: a `var` whose
    /// only write is a stdlib call's `mut` param stays a `var`. Demoted, native
    /// took a closure capture of it as a snapshot where wasm shared it.
    #[test]
    fn a_var_written_only_through_a_stdlib_mut_param_stays_a_var() {
        let resolve = |t: &CallTarget| match t {
            CallTarget::Module { module, func, .. } => {
                crate::mut_args::stdlib_mut_positions(module.as_str(), func.as_str())
            }
            _ => None,
        };
        let mut demoted = Vec::new();
        for (module, func, idxs) in crate::mut_args::stdlib_mut_fns() {
            let mut var_table = VarTable::new();
            let v = var_table.alloc(sym("xs"), Ty::Unknown, Mutability::Var, None);
            let arity = idxs.iter().max().copied().unwrap_or(0) + 1;
            let args: Vec<IrExpr> = (0..arity)
                .map(|i| if i == idxs[0] { e(IrExprKind::Var { id: v }) } else { e(IrExprKind::LitInt { value: 0 }) })
                .collect();
            let call = e(IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(&func), def_id: None },
                args,
                type_args: vec![],
            });
            let body = e(IrExprKind::Block { stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: call }, span: None }], expr: None });
            let main = IrFunction {
                name: sym("main"),
                params: vec![],
                ret_ty: Ty::Unit,
                body,
                is_effect: true,
                is_test: false,
                generics: None,
                extern_attrs: vec![],
                export_attrs: vec![],
                attrs: vec![],
                visibility: IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
                def_id: None,
                mutated_params: vec![], // fresh-fn: test fixture, the call site is what is under test
                module_origin: None,
            };
            let mut program = IrProgram { functions: vec![main], var_table, ..Default::default() };
            demote_unused_mut_with(&mut program, &resolve);
            if program.var_table.get(v).mutability != Mutability::Var {
                demoted.push(format!("{module}.{func}"));
            }
        }
        assert!(demoted.is_empty(), "a var written only through these was demoted to let: {demoted:?}");
    }
}

#[cfg(test)]
mod loop_bump_tests {
    use super::*;
    use almide_base::intern::sym;

    fn e(kind: IrExprKind) -> IrExpr {
        IrExpr { kind, ty: Ty::Unknown, span: None, def_id: None }
    }

    fn var(id: VarId) -> IrExpr {
        e(IrExprKind::Var { id })
    }

    fn lambda(param: VarId, body: IrExpr) -> Box<IrExpr> {
        Box::new(e(IrExprKind::Lambda { params: vec![(param, Ty::Unknown)], body: Box::new(body), lambda_id: None }))
    }

    /// `for j in [] { <read> }`, counted: the use counts of every var.
    fn counts_in_loop(table: VarTable, j: VarId, read: IrExpr) -> VarTable {
        let body = vec![IrStmt { kind: IrStmtKind::Expr { expr: read }, span: None }];
        let lp = e(IrExprKind::ForIn { var: j, var_tuple: None, iterable: Box::new(e(IrExprKind::List { elements: vec![] })), body });
        let mut table = table;
        count_uses_in_expr(&lp, &mut table);
        table
    }

    /// #3208: an outer list read once inside a loop body is read on every
    /// iteration, whichever node kind holds the read — an iterator chain's
    /// source, a fold seed, a match guard, an inline-Rust arg. A chain
    /// callback's own param is fresh per call and keeps its single count.
    #[test]
    fn an_outer_read_in_a_loop_body_counts_twice_in_every_node_kind() {
        let mut t = VarTable::new();
        let ids = t.alloc(sym("ids"), Ty::Unknown, Mutability::Let, None);
        let j = t.alloc(sym("j"), Ty::Unknown, Mutability::Let, None);
        let x = t.alloc(sym("x"), Ty::Unknown, Mutability::Let, None);
        let find = |src: IrExpr| e(IrExprKind::IterChain {
            source: Box::new(src),
            consume: true,
            steps: vec![],
            collector: IterCollector::Find { lambda: lambda(x, var(x)) },
        });
        let fold_seed = e(IrExprKind::IterChain {
            source: Box::new(e(IrExprKind::List { elements: vec![] })),
            consume: true,
            steps: vec![],
            collector: IterCollector::Fold { init: Box::new(var(ids)), lambda: lambda(x, var(x)) },
        });
        let guard = e(IrExprKind::Match {
            subject: Box::new(var(j)),
            arms: vec![IrMatchArm { pattern: IrPattern::Wildcard, guard: Some(var(ids)), body: e(IrExprKind::Unit) }],
        });
        let inline = e(IrExprKind::InlineRust { template: String::new(), args: vec![(sym("a"), var(ids))] });
        for (name, read) in [("chain source", find(var(ids))), ("fold seed", fold_seed), ("match guard", guard), ("inline rust", inline)] {
            let counted = counts_in_loop(t.clone(), j, read);
            assert!(counted.use_count(ids) >= 2, "{name}: an outer read in a loop body counted once");
        }
        let counted = counts_in_loop(t.clone(), j, find(var(ids)));
        assert_eq!(counted.use_count(x), 1, "a chain callback's own param was counted as an outer read");
    }
}
