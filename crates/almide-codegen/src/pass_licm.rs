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
use super::pass::{NanoPass, PassResult, Target};

/// Maps (module_name, func_name) → set of param indices mutated in-place.
/// Built from `IrFunction.mutated_params` at the start of LICM.
type MutationMap = HashMap<(Sym, Sym), Vec<usize>>;

#[derive(Debug)]
pub struct LICMPass;

impl NanoPass for LICMPass {
    fn name(&self) -> &str { "LICM" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Every VarId this pass allocates is a `__licm_*` hoist binding (one
        // alloc site); snapshot + mark, replacing the name-prefix test in
        // CloneInsertion.
        let vt_start = program.var_table.len();
        // Build mutation map from IR (no source re-parsing)
        let mutation_map = build_mutation_map(&program);
        // Analyze purity: user functions (fixpoint) + stdlib modules (IR attrs)
        let mut pure_fns = analyze_pure_functions(&program);
        // Add pure stdlib module functions as "module.func" keys
        for module in &program.modules {
            for func in &module.functions {
                if !func.is_effect && !func.is_async
                    && func.mutated_params.is_empty()
                    && !has_mut_in_inline_rust(&func.attrs)
                {
                    pure_fns.insert(almide_base::intern::sym(
                        &format!("{}.{}", module.name, func.name),
                    ));
                }
            }
        }
        let mut changed = false;
        let IrProgram { functions, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            if hoist_loops(&mut func.body, var_table, &pure_fns, &mutation_map) {
                changed = true;
            }
        }
        for module in modules.iter_mut() {
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

/// Build mutation map from all IrFunctions in the program.
/// Keyed by (module_name, func_name) for module-scoped functions.
fn build_mutation_map(program: &IrProgram) -> MutationMap {
    let mut map = MutationMap::new();
    for module in &program.modules {
        for func in &module.functions {
            if !func.mutated_params.is_empty() {
                map.insert((module.name, func.name), func.mutated_params.clone());
            }
        }
    }
    // Also check top-level functions (user-defined with @mutating)
    for func in &program.functions {
        if !func.mutated_params.is_empty() {
            // Top-level functions: module name is empty
            map.insert((almide_base::intern::sym(""), func.name), func.mutated_params.clone());
        }
    }
    map
}

/// Recursively walk the expression tree looking for loops, hoisting invariants.
/// Returns true if any hoisting was performed.
fn hoist_loops(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts: Vec<IrStmt> = Vec::new();
            for mut stmt in std::mem::take(stmts) {
                if hoist_loops_stmt(&mut stmt, vt, pure_fns, mm) {
                    changed = true;
                }
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
                if hoist_loops(e, vt, pure_fns, mm) { changed = true; }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if hoist_loops(cond, vt, pure_fns, mm) { changed = true; }
            if hoist_loops(then, vt, pure_fns, mm) { changed = true; }
            if hoist_loops(else_, vt, pure_fns, mm) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if hoist_loops(subject, vt, pure_fns, mm) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard {
                    if hoist_loops(g, vt, pure_fns, mm) { changed = true; }
                }
                if hoist_loops(&mut arm.body, vt, pure_fns, mm) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if hoist_loops(body, vt, pure_fns, mm) { changed = true; }
        }
        IrExprKind::ForIn { body, iterable, .. } => {
            if hoist_loops(iterable, vt, pure_fns, mm) { changed = true; }
            for s in body { if hoist_loops_stmt(s, vt, pure_fns, mm) { changed = true; } }
        }
        IrExprKind::While { cond, body } => {
            if hoist_loops(cond, vt, pure_fns, mm) { changed = true; }
            for s in body { if hoist_loops_stmt(s, vt, pure_fns, mm) { changed = true; } }
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
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Hole
        | IrExprKind::Todo { .. } => {}
    }
    changed
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

fn try_hoist_from_loop(expr: &mut IrExpr, vt: &mut VarTable, pure_fns: &HashSet<Sym>, mm: &MutationMap) -> Vec<IrStmt> {
    let mut hoisted = Vec::new();

    match &mut expr.kind {
        IrExprKind::ForIn { var, var_tuple, body, .. } => {
            let mut loop_defined = HashSet::new();
            loop_defined.insert(*var);
            if let Some(vars) = var_tuple {
                for v in vars { loop_defined.insert(*v); }
            }
            collect_defined_vars_stmts(body, &mut loop_defined, mm);
            for stmt in body.iter_mut() {
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns, mm);
            }
        }
        IrExprKind::While { cond: _, body } => {
            let mut loop_defined = HashSet::new();
            collect_defined_vars_stmts(body, &mut loop_defined, mm);
            for stmt in body.iter_mut() {
                extract_invariants_from_stmt(stmt, &loop_defined, vt, &mut hoisted, pure_fns, mm);
            }
        }
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
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }

    hoisted
}

/// Collect all VarIds that are bound OR assigned within a list of statements.
/// This includes `let` bindings AND `var` reassignments — any variable modified
/// inside the loop is NOT loop-invariant.
fn collect_defined_vars_stmts(stmts: &[IrStmt], defined: &mut HashSet<VarId>, mm: &MutationMap) {
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
                collect_defined_vars_expr(index, defined, mm);
                collect_defined_vars_expr(value, defined, mm);
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                defined.insert(*target);
                collect_defined_vars_expr(value, defined, mm);
            }
            IrStmtKind::MapInsert { target, key, value } => {
                defined.insert(*target);
                collect_defined_vars_expr(key, defined, mm);
                collect_defined_vars_expr(value, defined, mm);
            }
            IrStmtKind::ListSwap { target, a, b } => {
                defined.insert(*target);
                collect_defined_vars_expr(a, defined, mm);
                collect_defined_vars_expr(b, defined, mm);
            }
            IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
                defined.insert(*target);
                collect_defined_vars_expr(end, defined, mm);
            }
            IrStmtKind::ListCopySlice { dst, len, .. } => {
                defined.insert(*dst);
                collect_defined_vars_expr(len, defined, mm);
            }
            IrStmtKind::Expr { expr } => collect_defined_vars_expr(expr, defined, mm),
            IrStmtKind::Guard { cond, else_ } => {
                collect_defined_vars_expr(cond, defined, mm);
                collect_defined_vars_expr(else_, defined, mm);
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

fn collect_defined_vars_expr(expr: &IrExpr, defined: &mut HashSet<VarId>, mm: &MutationMap) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            collect_defined_vars_stmts(stmts, defined, mm);
            if let Some(e) = tail { collect_defined_vars_expr(e, defined, mm); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_defined_vars_expr(cond, defined, mm);
            collect_defined_vars_expr(then, defined, mm);
            collect_defined_vars_expr(else_, defined, mm);
        }
        IrExprKind::ForIn { var, var_tuple, body, iterable } => {
            defined.insert(*var);
            if let Some(vars) = var_tuple {
                for v in vars { defined.insert(*v); }
            }
            collect_defined_vars_expr(iterable, defined, mm);
            collect_defined_vars_stmts(body, defined, mm);
        }
        IrExprKind::While { cond, body } => {
            collect_defined_vars_expr(cond, defined, mm);
            collect_defined_vars_stmts(body, defined, mm);
        }
        IrExprKind::Match { subject, arms } => {
            collect_defined_vars_expr(subject, defined, mm);
            for arm in arms {
                collect_defined_vars_expr(&arm.body, defined, mm);
            }
        }
        IrExprKind::Lambda { body, params, .. } => {
            for (v, _) in params { defined.insert(*v); }
            collect_defined_vars_expr(body, defined, mm);
        }
        // Mutating stdlib calls: `&mut {name}` in the bundled
        // `@inline_rust` template means the runtime mutates that
        // param in place (`list.push`, `map.insert`, ...). Mark the
        // caller-side Var as defined so LICM doesn't wrongly hoist
        // something that depends on it.
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
            // A mutated arg's heap backing is written in place; if that arg (or a
            // place rooted at it) is hoisted out of the loop as a CLONE, the
            // mutation is lost. Two mutator sources: user fns (per-param `mut`
            // flags in `mm`) and stdlib in-place mutators (`list.push`, …) whose
            // args[0] is keyed by the mangled runtime symbol (mm only covers user
            // fns). Without the latter `list.push(b.xs, …)` had `b.xs` hoisted to
            // a clone and never grew `b.xs` (#712, after #703).
            let stdlib_sym = format!("almide_rt_{}_{}", module.as_str(), func.as_str());
            let stdlib_mutates_arg0 =
                crate::pass_closure_conversion::is_inplace_mutator(&stdlib_sym);
            for (i, arg) in args.iter().enumerate() {
                let mutated = (i == 0 && stdlib_mutates_arg0)
                    || mm.get(&(*module, *func)).map_or(false, |mp| mp.contains(&i));
                if mutated {
                    // Mark the ROOT var (bare `out`, or `b` in `b.xs` / `b[i]`)
                    // loop-variant so LICM never hoists an expression on it.
                    let mut root = &arg.kind;
                    while let IrExprKind::Member { object, .. }
                        | IrExprKind::TupleIndex { object, .. }
                        | IrExprKind::IndexAccess { object, .. } = root
                    {
                        root = &object.kind;
                    }
                    if let IrExprKind::Var { id } = root {
                        defined.insert(*id);
                    }
                }
            }
        }
        // The wasm pipeline lowers `list.push` etc. to RuntimeCall BEFORE LICM, so
        // the Module arm above never sees them — handle the same stdlib in-place-
        // mutator escape here (#712 wasm: without it `b.xs` was hoisted to a clone
        // and the push never grew it, so the loop read len 0).
        IrExprKind::RuntimeCall { symbol, args } => {
            if crate::pass_closure_conversion::is_inplace_mutator(symbol.as_str()) {
                if let Some(arg0) = args.first() {
                    let mut root = &arg0.kind;
                    while let IrExprKind::Member { object, .. }
                        | IrExprKind::TupleIndex { object, .. }
                        | IrExprKind::IndexAccess { object, .. } = root
                    {
                        root = &object.kind;
                    }
                    if let IrExprKind::Var { id } = root {
                        defined.insert(*id);
                    }
                }
            }
        }
        // Explicit-preserve: only the scopes/loops/mutating calls above define
        // variables relevant to LICM. Every other node (including Call with a
        // non-Module target) defines nothing. Listing each variant turns a new
        // IrExprKind into a compile error instead of a silent miss.
        IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
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
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

include!("pass_licm_p2.rs");
include!("pass_licm_p3.rs");
