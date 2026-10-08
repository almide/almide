//! LICM (Loop-Invariant Code Motion) pass.
//!
//! Identifies expressions inside loops that depend only on variables defined
//! outside the loop and contain no side effects. Hoists them to `let` bindings
//! before the loop to avoid redundant re-evaluation.
//!
//! Target: all targets (target-independent optimization).

use std::collections::{HashMap, HashSet};
use almide_base::intern::Sym;
use almide_ir::*;
use almide_ir::mut_args::{place_root, CallWrites, MutParamTable};
use almide_ir::visit::{walk_expr, IrVisitor};
use super::pass::{NanoPass, PassResult, Target};

/// What every call writes through `mut` parameters, and the module whose fns
/// are being scanned so a bare call resolves where it was written (#3452).
struct MutationMap {
    table: MutParamTable,
    scope: Option<Sym>,
}

#[derive(Debug)]
pub struct LICMPass;

impl NanoPass for LICMPass {
    fn name(&self) -> &str { "LICM" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    /// Judges purity on the resolved call targets (a `value.int` still spelled
    /// as a Module call hoisted where its resolved form does not), and the
    /// hoisted `__licm_*` binds are counted and borrowed like any other local
    /// by the ownership passes that follow.
    fn depends_on(&self) -> Vec<&'static str> { vec!["ResolveCalls", "RegionWindow", "BoxDeref"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["EggSaturation", "BorrowInsertion", "CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Every VarId this pass allocates is a `__licm_*` hoist binding (one
        // alloc site); snapshot + mark, replacing the name-prefix test in
        // CloneInsertion.
        let vt_start = program.var_table.len();
        // Build mutation map from IR (no source re-parsing)
        let mut mutation_map = MutationMap { table: MutParamTable::of(&program), scope: None };
        // Analyze purity: user functions (fixpoint) + stdlib modules (IR attrs)
        let mut pure_fns = analyze_pure_functions(&program);
        // Add pure stdlib module functions as "module.func" keys
        collect_pure_stdlib_module_fns(&program, &mut pure_fns);
        let mut changed = false;
        let IrProgram { functions, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            if hoist_loops(&mut func.body, var_table, &pure_fns, &mutation_map) {
                changed = true;
            }
        }
        for module in modules.iter_mut() {
            mutation_map.scope = Some(module.name);
            for func in module.functions.iter_mut() {
                if hoist_loops(&mut func.body, var_table, &pure_fns, &mutation_map) {
                    changed = true;
                }
            }
        }
        for i in vt_start..program.var_table.len() {
            program.codegen_annotations.always_clone_vars.insert(VarId(i as u32));
        }
        PassResult { program, changed }
    }
}

/// Stdlib-module purity scan phase of `LICMPass::run`, extracted verbatim
/// (cog>30 decomposition, pattern 1 — `pure_fns` is a write-only
/// accumulator, same safety class as `check_needs_ownership`'s `needs`).
fn collect_pure_stdlib_module_fns(program: &IrProgram, pure_fns: &mut HashSet<Sym>) {
    for module in &program.modules {
        for func in &module.functions {
            // The attribute criteria alone are NOT sufficient: a non-effect
            // module fn with no mut params can still mutate module globals
            // (`var` state). `analyze_pure_functions` already ran the body
            // fixpoint over every module fn under its bare name, so require
            // that verdict too — otherwise a stateful cross-module call in a
            // loop gets hoisted and N calls collapse into one (ceangal's
            // scroll physics; almide#846). The verdict is keyed by bare name, so
            // an `@extern` module fn sharing its name with a pure fn elsewhere
            // would pass it — externs are excluded here too, not only from the
            // fixpoint (see `analyze_pure_functions`).
            if !func.is_effect
                && func.extern_attrs.is_empty()
                && func.mutated_params.is_empty()
                && !has_mut_in_inline_rust(&func.attrs)
                && pure_fns.contains(&func.name)
            {
                pure_fns.insert(almide_base::intern::sym(
                    &format!("{}.{}", module.name, func.name),
                ));
            }
        }
    }
}

/// Recursively walk the expression tree looking for loops, hoisting invariants.
/// Returns true if any hoisting was performed.
/// `IrExprKind::Block` case of `hoist_loops`, extracted verbatim (cog>30
/// decomposition, pattern 2: uniform match arms, mirrors the
/// `lower_expr`/`infer_expr_inner` extraction shape).
fn hoist_loops_block(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> bool {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    let mut changed = false;
    let mut new_stmts: Vec<IrStmt> = Vec::new();
    for mut stmt in std::mem::take(stmts) {
        changed |= hoist_loops_stmt(&mut stmt, vt, pure_fns, mm);
        if let IrStmtKind::Expr { expr: ref mut loop_expr } = stmt.kind {
            let hoisted = try_hoist_from_loop(loop_expr, vt, pure_fns, mm);
            if !hoisted.is_empty() {
                changed = true;
                new_stmts.extend(hoisted);
            }
        }
        new_stmts.push(stmt);
    }
    *stmts = new_stmts;
    if let Some(e) = tail {
        changed |= hoist_loops(e, vt, pure_fns, mm);
    }
    changed
}

fn hoist_loops(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> bool {
    match &mut expr.kind {
        IrExprKind::Block { .. } => hoist_loops_block(expr, vt, pure_fns, mm),
        IrExprKind::If { cond, then, else_ } => {
            hoist_loops(cond, vt, pure_fns, mm)
                | hoist_loops(then, vt, pure_fns, mm)
                | hoist_loops(else_, vt, pure_fns, mm)
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = hoist_loops(subject, vt, pure_fns, mm);
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    changed |= hoist_loops(g, vt, pure_fns, mm);
                }
                changed |= hoist_loops(&mut arm.body, vt, pure_fns, mm);
            }
            changed
        }
        IrExprKind::Lambda { body, .. } => hoist_loops(body, vt, pure_fns, mm),
        IrExprKind::ForIn { body, iterable, .. } => {
            let mut changed = hoist_loops(iterable, vt, pure_fns, mm);
            for s in body { changed |= hoist_loops_stmt(s, vt, pure_fns, mm); }
            changed
        }
        IrExprKind::While { cond, body } => {
            let mut changed = hoist_loops(cond, vt, pure_fns, mm);
            for s in body { changed |= hoist_loops_stmt(s, vt, pure_fns, mm); }
            changed
        }
        // Explicit-preserve: hoisting is selective — these node kinds are
        // not descended for loop discovery here. Listing every remaining
        // variant makes a new IrExprKind a compile error, not a silent drop.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. }
        | IrExprKind::OptionNone | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Hole
        | IrExprKind::Todo { .. } => false,
    }
}

fn hoist_loops_stmt(stmt: &mut IrStmt, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            hoist_loops(value, vt, pure_fns, mm)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            hoist_loops(index, vt, pure_fns, mm) | hoist_loops(value, vt, pure_fns, mm)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            hoist_loops(key, vt, pure_fns, mm) | hoist_loops(value, vt, pure_fns, mm)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            hoist_loops(a, vt, pure_fns, mm) | hoist_loops(b, vt, pure_fns, mm)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            hoist_loops(end, vt, pure_fns, mm)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            hoist_loops(len, vt, pure_fns, mm)
        }
        IrStmtKind::Guard { cond, else_ } => {
            hoist_loops(cond, vt, pure_fns, mm) | hoist_loops(else_, vt, pure_fns, mm)
        }
        IrStmtKind::Expr { expr } => hoist_loops(expr, vt, pure_fns, mm),
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => false,
    }
}

/// `ForIn { var, var_tuple, body, .. }` arm of [`try_hoist_from_loop`].
fn try_hoist_from_for_in(var: VarId, var_tuple: &mut Option<Vec<VarId>>, body: &mut [IrStmt], vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> Vec<IrStmt> {
    let mut hoisted = Vec::new();
    let mut loop_defined = HashSet::new();
    loop_defined.insert(var);
    if let Some(vars) = var_tuple {
        for v in vars { loop_defined.insert(*v); }
    }
    collect_defined_vars_stmts(body, &mut loop_defined, mm);
    for stmt in body.iter_mut() {
        extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns, mm);
    }
    hoisted
}

/// `While { body, .. }` arm of [`try_hoist_from_loop`].
fn try_hoist_from_while(cond: &IrExpr, body: &mut [IrStmt], vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> Vec<IrStmt> {
    let mut hoisted = Vec::new();
    let mut loop_defined = HashSet::new();
    collect_call_written_vars(cond, &mut loop_defined, mm);
    collect_defined_vars_stmts(body, &mut loop_defined, mm);
    for stmt in body.iter_mut() {
        extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns, mm);
    }
    hoisted
}

fn try_hoist_from_loop(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> Vec<IrStmt> {
    match &mut expr.kind {
        IrExprKind::ForIn { var, var_tuple, body, .. } => try_hoist_from_for_in(*var, var_tuple, body, vt, pure_fns, mm),
        IrExprKind::While { cond, body } => try_hoist_from_while(cond, body, vt, pure_fns, mm),
        // Explicit-preserve: only loop heads (ForIn/While) drive hoisting
        // here; every other node kind yields no hoisted bindings. Listing
        // each variant turns a new IrExprKind into a compile error.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => Vec::new(),
    }
}

/// Collect all VarIds that are bound OR assigned within a list of statements.
/// This includes `let` bindings AND `var` reassignments — any variable modified
/// inside the loop is NOT loop-invariant — and every variable a call anywhere
/// in the statements writes through a `mut` parameter.
fn collect_defined_vars_stmts(stmts: &[IrStmt], defined: &mut HashSet<VarId>, mm: &MutationMap) {
    collect_scope_defined_vars(stmts, defined);
    let mut calls = CallWrittenVars { defined, mm };
    for stmt in stmts {
        calls.visit_stmt(stmt);
    }
}

/// The variables the calls in `expr` write (a `while` condition is
/// re-evaluated every iteration, so its calls are part of the loop).
fn collect_call_written_vars(expr: &IrExpr, defined: &mut HashSet<VarId>, mm: &MutationMap) {
    CallWrittenVars { defined, mm }.visit_expr(expr);
}

/// Every variable a call writes in place, at ANY depth — a call bound by a
/// `let`, nested in another call's argument, or in a closure body counts
/// as much as a call statement. A written arg's heap backing changes; if an
/// expression on it (or on a place rooted at it) is hoisted out of the loop,
/// the loop reads the value from before the write. Three sources:
/// - the callee's `mut` parameters, whatever the call's shape. A bare call
///   to a same-module fn (`bump(s)`) was the hole: only module-qualified
///   calls consulted the table, so `s.count` was hoisted past `bump(s)` and
///   native read a stale value where wasm did not (#3452);
/// - stdlib in-place mutators by runtime symbol (`list.push`, the bytes
///   writers, whose `mut` annotations are incomplete — see
///   `is_inplace_mutator`); without it `list.push(b.xs, …)` had `b.xs`
///   hoisted to a clone that never grew (#712). The wasm pipeline lowers
///   them to `RuntimeCall` before LICM;
/// - a callee nobody can identify (an unresolved method, a call through a fn
///   value, a bare name no table holds): every argument that names a place.
struct CallWrittenVars<'a> {
    defined: &'a mut HashSet<VarId>,
    mm: &'a MutationMap,
}

impl IrVisitor for CallWrittenVars<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                self.call(target, args);
            }
            IrExprKind::RuntimeCall { symbol, args } if is_inplace_mutator(symbol.as_str()) => {
                self.mark(args.first());
            }
            _ => {}
        }
        walk_expr(self, expr);
    }
}

impl CallWrittenVars<'_> {
    fn call(&mut self, target: &CallTarget, args: &[IrExpr]) {
        let runtime_sym = match target {
            CallTarget::Module { module, func, .. } => format!("almide_rt_{}_{}", module.as_str(), func.as_str()),
            CallTarget::Named { name } => name.as_str().to_string(),
            CallTarget::Method { .. } | CallTarget::Computed { .. } => String::new(),
        };
        if is_inplace_mutator(&runtime_sym) {
            self.mark(args.first());
        }
        match self.mm.table.writes(target, self.mm.scope) {
            CallWrites::Positions(idxs) => {
                for i in idxs {
                    self.mark(args.get(i));
                }
            }
            // A runtime symbol called bare is answered above, by the
            // runtime's own `&mut` surface.
            CallWrites::Unknown if runtime_sym.starts_with("almide_rt_") => {}
            CallWrites::Unknown => {
                if let CallTarget::Method { object, .. } = target {
                    self.mark(Some(object));
                }
                for arg in args {
                    self.mark(Some(arg));
                }
            }
        }
    }

    /// Mark the ROOT var of a written place (bare `out`, or `b` in `b.xs` /
    /// `b[i]`) loop-variant so LICM never hoists an expression on it.
    fn mark(&mut self, arg: Option<&IrExpr>) {
        if let Some(id) = arg.and_then(place_root) {
            self.defined.insert(id);
        }
    }
}

/// The variables the statements bind or assign, scopes and loops included.
fn collect_scope_defined_vars(stmts: &[IrStmt], defined: &mut HashSet<VarId>) {
    for stmt in stmts {
        match &stmt.kind {
            IrStmtKind::Bind { var, .. } => { defined.insert(*var); }
            IrStmtKind::BindDestructure { pattern, .. } => {
                collect_pattern_defined_vars(pattern, defined);
            }
            IrStmtKind::Assign { var, .. } => {
                // `var x` assigned inside the loop — x is loop-modified
                defined.insert(*var);
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                // `xs[i] = v` — the list/array variable is mutated
                defined.insert(*target);
                collect_defined_vars_expr(index, defined);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                defined.insert(*target);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::MapInsert { target, key, value } => {
                defined.insert(*target);
                collect_defined_vars_expr(key, defined);
                collect_defined_vars_expr(value, defined);
            }
            IrStmtKind::ListSwap { target, a, b } => {
                defined.insert(*target);
                collect_defined_vars_expr(a, defined);
                collect_defined_vars_expr(b, defined);
            }
            IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
                defined.insert(*target);
                collect_defined_vars_expr(end, defined);
            }
            IrStmtKind::ListCopySlice { dst, len, .. } => {
                defined.insert(*dst);
                collect_defined_vars_expr(len, defined);
            }
            IrStmtKind::Expr { expr } => collect_defined_vars_expr(expr, defined),
            IrStmtKind::Guard { cond, else_ } => {
                collect_defined_vars_expr(cond, defined);
                collect_defined_vars_expr(else_, defined);
            }
            IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
            IrStmtKind::Comment { .. } => {}
        }
    }
}

/// Collect VarIds bound by an IrPattern (tuple destructuring, constructor patterns, etc.).
fn collect_pattern_defined_vars(pat: &IrPattern, defined: &mut HashSet<VarId>) {
    match pat {
        IrPattern::Bind { var, .. } => { defined.insert(*var); }
        IrPattern::As { var, inner, .. } => {
            defined.insert(*var);
            collect_pattern_defined_vars(inner, defined);
        }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_pattern_defined_vars(a, defined); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { collect_pattern_defined_vars(e, defined); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_defined_vars(inner, defined);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_defined_vars(p, defined); }
            }
        }
        // Explicit-preserve (zero behavior change): these patterns bind no
        // vars that the current analysis tracks. NOTE: `List` does bind vars
        // inside `elements` and was already dropped by the prior `_ => {}` —
        // preserving that exact behavior here, but now a NEW IrPattern variant
        // is a compile error instead of a silent miss. See residual risk.
        IrPattern::Wildcard | IrPattern::Literal { .. }
        | IrPattern::None | IrPattern::List { .. } => {}
    }
}

/// `IrExprKind::Block` case of `collect_defined_vars_expr`, extracted
/// verbatim (cog>30 decomposition, pattern 1 — `defined` is a write-only
/// accumulator).
fn collect_defined_vars_block(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    let IrExprKind::Block { stmts, expr: tail } = &expr.kind else { unreachable!() };
    collect_scope_defined_vars(stmts, defined);
    if let Some(e) = tail { collect_defined_vars_expr(e, defined); }
}

/// `IrExprKind::ForIn` case of `collect_defined_vars_expr`, extracted
/// verbatim (cog>30 decomposition).
fn collect_defined_vars_for_in(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    let IrExprKind::ForIn { var, var_tuple, body, iterable } = &expr.kind else { unreachable!() };
    defined.insert(*var);
    if let Some(vars) = var_tuple {
        for v in vars { defined.insert(*v); }
    }
    collect_defined_vars_expr(iterable, defined);
    collect_scope_defined_vars(body, defined);
}

/// `IrExprKind::Match` case of `collect_defined_vars_expr`, extracted
/// verbatim (cog>30 decomposition).
fn collect_defined_vars_match(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
    collect_defined_vars_expr(subject, defined);
    for arm in arms {
        collect_defined_vars_expr(&arm.body, defined);
    }
}

fn collect_defined_vars_expr(expr: &IrExpr, defined: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Block { .. } => collect_defined_vars_block(expr, defined),
        IrExprKind::If { cond, then, else_ } => {
            collect_defined_vars_expr(cond, defined);
            collect_defined_vars_expr(then, defined);
            collect_defined_vars_expr(else_, defined);
        }
        IrExprKind::ForIn { .. } => collect_defined_vars_for_in(expr, defined),
        IrExprKind::While { cond, body } => {
            collect_defined_vars_expr(cond, defined);
            collect_scope_defined_vars(body, defined);
        }
        IrExprKind::Match { .. } => collect_defined_vars_match(expr, defined),
        IrExprKind::Lambda { body, params, .. } => {
            for (v, _) in params { defined.insert(*v); }
            collect_defined_vars_expr(body, defined);
        }
        // Explicit-preserve: only the scopes/loops above define variables
        // here. What a call writes is collected at every depth by
        // `CallWrittenVars`, not by this walk. Listing each variant turns a
        // new IrExprKind into a compile error instead of a silent miss.
        IrExprKind::Call { .. } | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

include!("pass_licm_hoist.rs");
include!("pass_licm_purity.rs");

/// The in-place `&mut` stdlib mutator surface, by RUNTIME SYMBOL — LICM's
/// purity question ("does discarding this call's result still mutate a
/// captured var?"). Moved here from the retired wasm-only closure-conversion
/// pass (#930): this file is its only live consumer. Distinct on purpose from
/// `almide-mir`'s `is_inplace_mutator` (module.fn names, the receiver-COW
/// question) — different vocabularies for different layers.
pub(crate) fn is_inplace_mutator(symbol: &str) -> bool {
    // ONLY the runtime fns that take `&mut` on `args[0]` (verified against
    // runtime/rs/src/*.rs). The `list.set/insert/sort/reverse`, `map.set/remove`,
    // and all `set.*` ops return a NEW value (pure) — calling them in a closure and
    // discarding the result is a no-op, not a captured mutation.
    matches!(symbol,
        "almide_rt_list_push" | "almide_rt_list_pop" | "almide_rt_list_clear"
        | "almide_rt_map_insert" | "almide_rt_map_delete" | "almide_rt_map_clear"
        | "almide_rt_string_push" | "almide_rt_string_push_char" | "almide_rt_string_clear"
    )
    // Bytes builders mutate their buffer in place (the runtime takes `&mut`): push,
    // clear, fill, copy_within, copy_from, set_at, as_mut_ptr, plus every
    // append_*/set_*/write_*. Matched by shape — the read side is
    // read_*/get/slice/len/… (disjoint). This is the complete &mut set in
    // runtime/rs/src/bytes.rs (mirrors almide-mir's lower/calls.rs list); note
    // bytes' stdlib `mut` annotations are incomplete (only push/set_at/copy_within),
    // so we cannot key off the `mut` keyword here and instead encode the runtime's
    // actual mutation surface. `copy_from` was missing until #955: its global dst
    // rendered as `&mut G.with(|c| (**c.borrow()).clone())` — a mutation of a
    // discarded clone, silently wrong on native while wasm wrote through.
    || symbol.strip_prefix("almide_rt_bytes_").is_some_and(|m| {
        matches!(m, "push" | "clear" | "fill" | "copy_within" | "copy_from" | "set_at" | "as_mut_ptr")
            || m.starts_with("append_") || m.starts_with("set_") || m.starts_with("write_")
    })
}
