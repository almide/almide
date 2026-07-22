//! CaptureClonePass: pre-clone variables captured by move closures.
//!
//! In Rust, `move |...| { ... }` takes ownership of all captured variables.
//! When the same variable is captured by multiple closures, or used after a
//! closure, the second use causes E0382 (use of moved value).
//!
//! This pass wraps each lambda in a block that clones captured variables:
//!
//!   Before:  (lambda using `tag`)
//!     move |x| { f(x, tag) }
//!
//!   After:   (wrapped in block with pre-clone)
//!     { let __cap_5 = tag; move |x| { f(x, __cap_5) } }
//!
//! CloneInsertionPass (which runs after) adds .clone() to the Var references.
//! The net effect: each lambda captures its own clone, original stays alive.

use std::collections::HashSet;
use almide_ir::*;
use almide_lang::types::Ty;
use almide_base::intern::Sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct CaptureClonePass;

impl NanoPass for CaptureClonePass {
    fn name(&self) -> &str { "CaptureClone" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Every VarId this pass allocates is a `__cap_*` clone binding (one
        // alloc site); snapshot the table length and mark the new ids in
        // `always_clone_vars` afterwards — id-keyed, rename-proof.
        let vt_start = program.var_table.len();
        // Pre-analysis (Closure v2, P3): a Copy-type `var` local (Int/Float/Bool)
        // captured by a closure must become a shared `Rc<Cell<T>>` — a `move`
        // closure would capture a *copy* and silently drop the mutation. Compute
        // the set once here (before the wrap decision below), and publish it via
        // `codegen_annotations` so the Rust walker emits cell ops. As non-`Copy`
        // values these captures now also need the clone-wrap that `needs_clone_type`
        // skips for `Copy` — `SHARED_MUT` forces it, and `wrap_lambda_with_clones`
        // adds the `__cap` renames to the set so their reads/writes are cells too.
        let shared = detect_shared_mut(&program);
        for v in &shared { program.codegen_annotations.shared_mut_vars.insert(*v); }
        SHARED_MUT.with(|m| *m.borrow_mut() = shared);

        let mut changed = false;
        let IrProgram { functions, modules, var_table, codegen_annotations, .. } = &mut program;
        for func in functions.iter_mut() {
            let param_vars: HashSet<VarId> = func.params.iter().map(|p| p.var).collect();
            PARAM_BORROWS.with(|m| {
                *m.borrow_mut() = func.params.iter().map(|p| (p.var, p.borrow)).collect();
            });
            if transform_expr(&mut func.body, var_table, &param_vars) {
                changed = true;
            }
        }
        for module in modules.iter_mut() {
            for func in module.functions.iter_mut() {
                let param_vars: HashSet<VarId> = func.params.iter().map(|p| p.var).collect();
                PARAM_BORROWS.with(|m| {
                    *m.borrow_mut() = func.params.iter().map(|p| (p.var, p.borrow)).collect();
                });
                if transform_expr(&mut func.body, var_table, &param_vars) {
                    changed = true;
                }
            }
        }
        // `wrap_lambda_with_clones` added the `__cap_*` renames of shared-mut
        // captures to SHARED_MUT; persist them so their reads/writes are cells too.
        SHARED_MUT.with(|m| {
            for v in m.borrow().iter() { codegen_annotations.shared_mut_vars.insert(*v); }
            m.borrow_mut().clear();
        });
        PARAM_BORROWS.with(|m| m.borrow_mut().clear());
        for i in vt_start..program.var_table.len() {
            program.codegen_annotations.always_clone_vars.insert(VarId(i as u32));
        }
        PassResult { program, changed }
    }
}

use std::cell::RefCell;
thread_local! {
    static PARAM_BORROWS: RefCell<std::collections::HashMap<VarId, ParamBorrow>> =
        RefCell::new(std::collections::HashMap::new());
    /// Vars that must be lowered to a shared `Rc<Cell<T>>` on the Rust target
    /// (Copy-type `var` locals captured by a closure, plus their `__cap` renames).
    /// Populated per-run by `detect_shared_mut` + `wrap_lambda_with_clones`.
    /// (Closure v2, P3.)
    static SHARED_MUT: RefCell<HashSet<VarId>> = RefCell::new(HashSet::new());
}

/// Find every Copy-type (`Int`/`Float`/`Bool`) `Mutability::Var` local captured by
/// a closure. As a moved copy its mutation would be invisible to the enclosing
/// scope, so it must become a shared cell. (Closure v2, P3.)
fn detect_shared_mut(program: &IrProgram) -> HashSet<VarId> {
    // Module-level globals are NOT closure cells. They lower to `static` /
    // `thread_local!` storage (a mutated one becomes `ModuleRc`/`ModuleCell`) and
    // are already globally reachable, so a closure references them directly by their
    // storage class — it does not capture a heap cell. Marking a global `shared_mut`
    // double-classifies it (ModuleRc AND SharedMut) and the two emit conflicting
    // references: the closure body uses `G.with(…)` while the enclosing read uses a
    // lowercase `g.get()` that doesn't exist → `error[E0425]: cannot find value g`.
    // So exclude globals here; their mutability is handled by the ModuleRc path.
    let globals: HashSet<VarId> = program.top_lets.iter().map(|t| t.var)
        .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|t| t.var)))
        .collect();
    // Function parameters. An Almide source param is always immutable, so a param
    // marked `Mutability::Var` only ever got that way from the TCO pass rewriting a
    // tail-recursive fn into a loop (its params become the loop's mutable state).
    // That "mutation" is the per-iteration reassignment, NOT a mutation through a
    // closure — and a closure created inside the loop body must capture the current
    // iteration's value (a clone), not share one cell across iterations. So a
    // captured param needs a shared cell only if the closure ITSELF mutates it;
    // its bare `Var` flag (TCO artifact) must not trigger shared-mut. (Without this
    // exclusion the param read emits `.get()` on a value that was never cell-wrapped
    // — params have no `SharedMut::new` declaration site — yielding E0061/E0277.)
    let params: HashSet<VarId> = program.functions.iter().flat_map(|f| f.params.iter().map(|p| p.var))
        .chain(program.modules.iter().flat_map(|m| m.functions.iter().flat_map(|f| f.params.iter().map(|p| p.var))))
        .collect();
    struct LambdaWalker<'a> { vt: &'a VarTable, globals: &'a HashSet<VarId>, params: &'a HashSet<VarId>, out: HashSet<VarId> }
    impl almide_ir::visit::IrVisitor for LambdaWalker<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
                let param_set: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                // Vars this lambda mutates (assigned, or passed `&mut` to a method
                // like `list.push`). A non-Copy `var` mutated ONLY through a method
                // is recorded with `Mutability::Let` (it is never reassigned), so the
                // mutability flag alone misses it — hence the explicit mutation scan.
                let mut mutated = HashSet::new();
                collect_mutated_vars(body, &mut mutated);
                for v in almide_ir::free_vars::free_vars(body, &param_set) {
                    if self.globals.contains(&v) { continue; }
                    let info = self.vt.get(v);
                    // A captured var that is mutated through the closure must become a
                    // shared cell so the mutation is visible to the enclosing scope.
                    // Copy types lower to `Rc<Cell<T>>`, non-Copy to `SharedMut`
                    // (`Rc<RefCell<T>>`) — the walker picks by type. Without this a
                    // non-Copy capture went through `RcCow`, whose copy-on-write loses
                    // the mutation. (Closure v2: P3 = Copy, P6 = non-Copy.)
                    // For a function param, the `Var` flag is a TCO artifact, so only a
                    // genuine closure-body mutation counts (see `params` note above).
                    let needs_cell = if self.params.contains(&v) {
                        mutated.contains(&v)
                    } else {
                        matches!(info.mutability, Mutability::Var) || mutated.contains(&v)
                    };
                    if needs_cell {
                        self.out.insert(v);
                    }
                }
            }
            almide_ir::visit::walk_expr(self, expr);
        }
    }
    use almide_ir::visit::IrVisitor;
    let mut w = LambdaWalker { vt: &program.var_table, globals: &globals, params: &params, out: HashSet::new() };
    for f in &program.functions { w.visit_expr(&f.body); }
    for m in &program.modules { for f in &m.functions { w.visit_expr(&f.body); } }
    w.out
}

/// Collect VarIds an expression mutates: assignment targets and `&mut`-borrowed
/// vars (the form `list.push(v, …)` etc. take after `BorrowInsertionPass`).
fn collect_mutated_vars(expr: &IrExpr, out: &mut HashSet<VarId>) {
    struct M<'a> { out: &'a mut HashSet<VarId> }
    impl almide_ir::visit::IrVisitor for M<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Borrow { expr: inner, mutable: true, .. } = &e.kind {
                if let IrExprKind::Var { id } = &inner.kind { self.out.insert(*id); }
            }
            almide_ir::visit::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. } => { self.out.insert(*var); }
                IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                | IrStmtKind::FieldAssign { target, .. } => { self.out.insert(*target); }
                _ => {}
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    use almide_ir::visit::IrVisitor;
    M { out }.visit_expr(expr);
}

/// Collect all variables bound by a statement (Bind + BindDestructure).
fn collect_stmt_bindings(stmt: &IrStmt, out: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, .. } => { out.insert(*var); }
        IrStmtKind::BindDestructure { pattern, .. } => collect_pattern_bindings_into(pattern, out),
        _ => {}
    }
}

/// Collect variables bound by a pattern into a VarId set.
fn collect_pattern_bindings_into(pattern: &IrPattern, out: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { out.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for a in args { collect_pattern_bindings_into(a, out); }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { collect_pattern_bindings_into(e, out); }
        }
        IrPattern::Some { inner, .. } | IrPattern::Ok { inner, .. } | IrPattern::Err { inner, .. } => {
            collect_pattern_bindings_into(inner, out);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_bindings_into(p, out); }
            }
        }
        _ => {}
    }
}

/// Transform every child of a list of independent expressions (args, list
/// elements, ...). Uses `|=` (non-short-circuiting) so every element is
/// always visited regardless of earlier results.
fn transform_expr_list(exprs: &mut [IrExpr], vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for e in exprs { changed |= transform_expr(e, vt, scope_vars); }
    changed
}

/// Transform the `IrExpr` half of a `(Sym, IrExpr)` pair list (Record
/// fields, InlineRust args).
fn transform_expr_pairs(pairs: &mut [(Sym, IrExpr)], vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for (_, e) in pairs { changed |= transform_expr(e, vt, scope_vars); }
    changed
}

/// Transform both sides of a `(IrExpr, IrExpr)` pair list (MapLiteral entries).
fn transform_expr_kv_pairs(entries: &mut [(IrExpr, IrExpr)], vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for (k, v) in entries {
        changed |= transform_expr(k, vt, scope_vars);
        changed |= transform_expr(v, vt, scope_vars);
    }
    changed
}

/// Shared by `Call` and `TailCall`: only `Method`/`Computed` targets carry a
/// child `IrExpr` to descend into.
fn transform_call_target(target: &mut CallTarget, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    match target {
        CallTarget::Method { object, .. } => transform_expr(object, vt, scope_vars),
        CallTarget::Computed { callee } => transform_expr(callee, vt, scope_vars),
        CallTarget::Named { .. } | CallTarget::Module { .. } => false,
    }
}

fn transform_expr_block(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let IrExprKind::Block { stmts, expr: tail } = &mut expr.kind else { unreachable!() };
    // Collect vars defined in this block to extend scope
    let mut local_scope = scope_vars.clone();
    for stmt in stmts.iter() {
        collect_stmt_bindings(stmt, &mut local_scope);
    }
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= transform_stmt(stmt, vt, &local_scope);
    }
    if let Some(e) = tail {
        changed |= transform_expr(e, vt, &local_scope);
    }
    changed
}

fn transform_expr_match(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    let mut changed = transform_expr(subject, vt, scope_vars);
    for arm in arms {
        if let Some(g) = &mut arm.guard {
            changed |= transform_expr(g, vt, scope_vars);
        }
        changed |= transform_expr(&mut arm.body, vt, scope_vars);
    }
    changed
}

fn transform_expr_for_in(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let IrExprKind::ForIn { iterable, body, var, var_tuple, .. } = &mut expr.kind else { unreachable!() };
    let mut changed = transform_expr(iterable, vt, scope_vars);
    let mut loop_scope = scope_vars.clone();
    loop_scope.insert(*var);
    if let Some(vt_) = var_tuple { for v in vt_.iter() { loop_scope.insert(*v); } }
    // Collect vars defined in loop body so lambdas can see sibling bindings
    for s in body.iter() { collect_stmt_bindings(s, &mut loop_scope); }
    for s in body.iter_mut() { changed |= transform_stmt(s, vt, &loop_scope); }
    changed
}

fn transform_expr_while(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let IrExprKind::While { cond, body } = &mut expr.kind else { unreachable!() };
    let mut changed = transform_expr(cond, vt, scope_vars);
    let mut loop_scope = scope_vars.clone();
    // Collect vars defined in loop body so lambdas can see sibling bindings
    for s in body.iter() { collect_stmt_bindings(s, &mut loop_scope); }
    for s in body.iter_mut() { changed |= transform_stmt(s, vt, &loop_scope); }
    changed
}

fn transform_string_parts(parts: &mut [IrStringPart], vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let mut changed = false;
    for p in parts {
        if let IrStringPart::Expr { expr: e } = p {
            changed |= transform_expr(e, vt, scope_vars);
        }
    }
    changed
}

// A fused iterator chain (produced by stream fusion) hides its step and
// collector lambdas from the generic recursion in `transform_expr`. Without
// descending here, a `move` step closure that captures a non-Copy outer var
// (e.g. a map used inside `keys |> map(k => …get(g,k)…)`) never gets the
// pre-clone wrap, so the chain moves the var and a later use of it fails to
// compile (E0382). Recurse into the source and every embedded lambda.
// `replace_vars` already mirrors this shape, so a wrapped lambda's `__cap`
// renames carry through correctly.
fn transform_expr_iter_chain(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    let IrExprKind::IterChain { source, steps, collector, .. } = &mut expr.kind else { unreachable!() };
    let mut changed = transform_expr(source, vt, scope_vars);
    for step in steps.iter_mut() {
        match step {
            IterStep::Map { lambda } | IterStep::Filter { lambda }
            | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                changed |= transform_expr(lambda, vt, scope_vars);
            }
        }
    }
    match collector {
        IterCollector::Collect => {}
        IterCollector::Fold { init, lambda } => {
            changed |= transform_expr(init, vt, scope_vars);
            changed |= transform_expr(lambda, vt, scope_vars);
        }
        IterCollector::Any { lambda } | IterCollector::All { lambda }
        | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
            changed |= transform_expr(lambda, vt, scope_vars);
        }
    }
    changed
}

/// Walk the IR tree. When we find a Lambda that captures clone-worthy outer
/// variables, wrap it in a block with pre-clone bindings.
fn transform_expr(expr: &mut IrExpr, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    // First, recurse into children (bottom-up so inner lambdas are processed first).
    // Every arm uses `|` (non-short-circuiting bool-or), never `||` — all
    // children must always be visited so their captures get pre-cloned,
    // regardless of what an earlier sibling returned.
    let mut changed = match &mut expr.kind {
        IrExprKind::Block { .. } => transform_expr_block(expr, vt, scope_vars),
        IrExprKind::If { cond, then, else_ } => {
            transform_expr(cond, vt, scope_vars)
                | transform_expr(then, vt, scope_vars)
                | transform_expr(else_, vt, scope_vars)
        }
        IrExprKind::Match { .. } => transform_expr_match(expr, vt, scope_vars),
        IrExprKind::Lambda { body, params, .. } => {
            let mut inner_scope = scope_vars.clone();
            for (v, _) in params.iter() { inner_scope.insert(*v); }
            transform_expr(body, vt, &inner_scope)
        }
        IrExprKind::Call { target, args, .. } => {
            transform_call_target(target, vt, scope_vars) | transform_expr_list(args, vt, scope_vars)
        }
        IrExprKind::RuntimeCall { args, .. } => transform_expr_list(args, vt, scope_vars),
        IrExprKind::BinOp { left, right, .. } => {
            transform_expr(left, vt, scope_vars) | transform_expr(right, vt, scope_vars)
        }
        IrExprKind::UnOp { operand, .. } => transform_expr(operand, vt, scope_vars),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            transform_expr_list(elements, vt, scope_vars)
        }
        IrExprKind::Record { fields, .. } => transform_expr_pairs(fields, vt, scope_vars),
        IrExprKind::SpreadRecord { base, fields } => {
            transform_expr(base, vt, scope_vars) | transform_expr_pairs(fields, vt, scope_vars)
        }
        IrExprKind::ForIn { .. } => transform_expr_for_in(expr, vt, scope_vars),
        IrExprKind::While { .. } => transform_expr_while(expr, vt, scope_vars),
        IrExprKind::StringInterp { parts } => transform_string_parts(parts, vt, scope_vars),
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        // Single-expr wrappers introduced by earlier passes (BorrowInsertion,
        // BoxDeref, ToVec materialisation, async). A lambda nested inside one of
        // these — e.g. `list.join(&list.map(keys, k => …g…), …)` wraps the inner
        // `map` in `Borrow` — was invisible to the capture-clone walk, so its
        // captured non-Copy vars never got the pre-clone wrap and a later use of
        // the var failed to compile (E0382). `replace_vars` already descends these,
        // so the walk and the rename now agree.
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::ToVec { expr: e } | IrExprKind::Await { expr: e } => {
            transform_expr(e, vt, scope_vars)
        }
        IrExprKind::RustMacro { args, .. } => transform_expr_list(args, vt, scope_vars),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            transform_expr(e, vt, scope_vars) | transform_expr(f, vt, scope_vars)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            transform_expr(object, vt, scope_vars) | transform_expr(index, vt, scope_vars)
        }
        IrExprKind::Range { start, end, .. } => {
            transform_expr(start, vt, scope_vars) | transform_expr(end, vt, scope_vars)
        }
        IrExprKind::MapLiteral { entries } => transform_expr_kv_pairs(entries, vt, scope_vars),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => transform_expr(object, vt, scope_vars),
        IrExprKind::IterChain { .. } => transform_expr_iter_chain(expr, vt, scope_vars),
        IrExprKind::TailCall { target, args } => {
            transform_call_target(target, vt, scope_vars) | transform_expr_list(args, vt, scope_vars)
        }
        IrExprKind::RcWrap { expr: e, .. } => transform_expr(e, vt, scope_vars),
        IrExprKind::InlineRust { args, .. } => transform_expr_pairs(args, vt, scope_vars),
        // True leaves (no child `IrExpr`). Listed explicitly so a new
        // child-bearing IrExprKind is a compile error here, not a silently
        // dropped subtree (the native↔WASM capture-divergence class).
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::Var { .. }
        | IrExprKind::FnRef { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::RenderedCall { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => false,
    };

    // Now check: is this expr itself a Lambda with captured vars that need cloning?
    if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
        let param_set: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
        // Capture set via the single shared analysis (`almide_ir::free_vars`) — the
        // same one WASM ClosureConversion uses. Returns a VarId-sorted Vec, so the
        // resulting `__cap` bindings are emitted in deterministic order.
        let captures: Vec<VarId> = almide_ir::free_vars::free_vars(body, &param_set).into_iter()
            // Clone-worthy captures from the outer scope — plus shared-mut (`Rc<Cell>`)
            // captures, which are no longer `Copy` and so now need the clone-wrap that
            // `needs_clone_type` skips for `Copy` types. (Closure v2, P3.)
            .filter(|v| {
                if !scope_vars.contains(v) { return false; }
                needs_clone_type(&vt.get(*v).ty) || SHARED_MUT.with(|m| m.borrow().contains(v))
            })
            .collect();

        if !captures.is_empty() {
            // Wrap this lambda in a block: { let __cap = var; lambda_with_cap }
            wrap_lambda_with_clones(expr, &captures, vt);
            changed = true;
        }
    }

    changed
}

fn transform_stmt(stmt: &mut IrStmt, vt: &mut VarTable, scope_vars: &HashSet<VarId>) -> bool {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            transform_expr(index, vt, scope_vars) | transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            transform_expr(key, vt, scope_vars) | transform_expr(value, vt, scope_vars)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            transform_expr(a, vt, scope_vars) | transform_expr(b, vt, scope_vars)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            transform_expr(end, vt, scope_vars)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            transform_expr(len, vt, scope_vars)
        }
        IrStmtKind::Guard { cond, else_ } => {
            transform_expr(cond, vt, scope_vars) | transform_expr(else_, vt, scope_vars)
        }
        IrStmtKind::Expr { expr } => transform_expr(expr, vt, scope_vars),
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => false,
    }
}

/// Wrap a Lambda expression in a block that pre-clones captured variables.
///
/// Transforms:
///   move |params| { body using `x` }
/// Into:
///   { let __cap_N = x; move |params| { body using `__cap_N` } }
fn wrap_lambda_with_clones(expr: &mut IrExpr, captures: &[VarId], vt: &mut VarTable) {
    let mut stmts = Vec::new();
    let mut renames = std::collections::HashMap::new();

    for &var_id in captures {
        let ty = vt.get(var_id).ty.clone();
        let cap_name = format!("__cap_{}", var_id.0);
        let cap_var = vt.alloc(
            almide_base::intern::sym(&cap_name),
            ty.clone(),
            Mutability::Let,
            None,
        );
        renames.insert(var_id, cap_var);

        // The clone of a shared-mut capture is itself a shared cell (`Rc<Cell>`),
        // so reads/writes of `__cap` inside the closure go through `.get()`/`.set()`
        // too. (Closure v2, P3.)
        if SHARED_MUT.with(|m| m.borrow().contains(&var_id)) {
            SHARED_MUT.with(|m| { m.borrow_mut().insert(cap_var); });
        }

        // If the captured var is a fn param with a borrowed runtime
        // representation (`&[T]` / `&str` / `&T`), the bare `Var` IR
        // renders as the borrow — but `__cap_N: Vec<T>` / `String` / `T`
        // (the Almide-level owned type) expects an owned value. Materialise
        // the owned form explicitly so the `move |..|` closure can take it.
        let borrow = PARAM_BORROWS.with(|m| m.borrow().get(&var_id).copied());
        let bind_value = match borrow {
            Some(ParamBorrow::RefSlice) => IrExpr {
                kind: IrExprKind::ToVec {
                    expr: Box::new(IrExpr { kind: IrExprKind::Var { id: var_id }, ty: ty.clone(), span: None, def_id: None }),
                },
                ty: ty.clone(), span: None, def_id: None,
            },
            Some(ParamBorrow::RefStr) => IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Method {
                        object: Box::new(IrExpr { kind: IrExprKind::Var { id: var_id }, ty: ty.clone(), span: None, def_id: None }),
                        // Use `to_owned` instead of `to_string` to avoid
                        // StdlibLowering converting this into a module call
                        // (e.g. `int.to_string()`) when the Almide-level type
                        // differs from the Rust-level &str representation.
                        method: almide_base::intern::sym("to_owned"),
                    },
                    args: vec![],
                    type_args: vec![],
                },
                ty: ty.clone(), span: None, def_id: None,
            },
            _ => IrExpr { kind: IrExprKind::Var { id: var_id }, ty: ty.clone(), span: None, def_id: None },
        };

        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: cap_var,
                mutability: Mutability::Let,
                ty: ty.clone(),
                value: bind_value,
            },
            span: None,
        });
    }

    // Rename captured vars inside the lambda body
    if let IrExprKind::Lambda { body, .. } = &mut expr.kind {
        replace_vars(body, &renames);
    }

    // Wrap: { let __cap = var; ...; original_lambda }
    let lambda_expr = std::mem::replace(expr, IrExpr {
        kind: IrExprKind::Unit,
        ty: Ty::Unit,
        span: None, def_id: None,
    });
    let ty = lambda_expr.ty.clone();
    let span = lambda_expr.span;
    *expr = IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(lambda_expr)),
        },
        ty,
        span, def_id: None,
    };
}

// ── Variable replacement ──

fn replace_vars(expr: &mut IrExpr, renames: &std::collections::HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Var { id } => {
            if let Some(&new_id) = renames.get(id) { *id = new_id; }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => replace_vars(object, renames),
                CallTarget::Computed { callee } => replace_vars(callee, renames),
                _ => {}
            }
            for a in args { replace_vars(a, renames); }
        }
        IrExprKind::RuntimeCall { args, .. } => {
            for a in args { replace_vars(a, renames); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            replace_vars(left, renames); replace_vars(right, renames);
        }
        IrExprKind::UnOp { operand, .. } => replace_vars(operand, renames),
        IrExprKind::If { cond, then, else_ } => {
            replace_vars(cond, renames); replace_vars(then, renames); replace_vars(else_, renames);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { replace_vars_stmt(s, renames); }
            if let Some(e) = tail { replace_vars(e, renames); }
        }
        IrExprKind::Lambda { body, .. } => replace_vars(body, renames),
        IrExprKind::Match { subject, arms } => {
            replace_vars(subject, renames);
            for arm in arms {
                if let Some(g) = &mut arm.guard { replace_vars(g, renames); }
                replace_vars(&mut arm.body, renames);
            }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { replace_vars(e, renames); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { replace_vars(v, renames); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            replace_vars(base, renames);
            for (_, v) in fields { replace_vars(v, renames); }
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => replace_vars(object, renames),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            replace_vars(object, renames); replace_vars(index, renames);
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } => replace_vars(e, renames),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            replace_vars(e, renames); replace_vars(f, renames);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { replace_vars(e, renames); }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            replace_vars(iterable, renames);
            for s in body { replace_vars_stmt(s, renames); }
        }
        IrExprKind::While { cond, body } => {
            replace_vars(cond, renames);
            for s in body { replace_vars_stmt(s, renames); }
        }
        IrExprKind::Range { start, end, .. } => {
            replace_vars(start, renames); replace_vars(end, renames);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { replace_vars(k, renames); replace_vars(v, renames); }
        }
        IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e }
        | IrExprKind::ToVec { expr: e }
        | IrExprKind::Await { expr: e } => {
            replace_vars(e, renames);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { replace_vars(a, renames); }
        }
        IrExprKind::Fan { exprs } => {
            for e in exprs { replace_vars(e, renames); }
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            replace_vars(source, renames);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        replace_vars(lambda, renames);
                    }
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => {
                    replace_vars(init, renames);
                    replace_vars(lambda, renames);
                }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                    replace_vars(lambda, renames);
                }
            }
        }
        IrExprKind::TailCall { target, args } => {
            match target {
                CallTarget::Method { object, .. } => replace_vars(object, renames),
                CallTarget::Computed { callee } => replace_vars(callee, renames),
                CallTarget::Named { .. } | CallTarget::Module { .. } => {}
            }
            for a in args { replace_vars(a, renames); }
        }
        IrExprKind::RcWrap { expr: e, .. } => replace_vars(e, renames),
        IrExprKind::InlineRust { args, .. } => {
            for (_, a) in args { replace_vars(a, renames); }
        }
        // True leaves (no child `IrExpr`); `Var` is renamed above. Listed
        // explicitly so a new child-bearing IrExprKind is a compile error.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::FnRef { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::RenderedCall { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

fn replace_vars_stmt(stmt: &mut IrStmt, renames: &std::collections::HashMap<VarId, VarId>) {
    // A captured var that is WRITTEN inside the closure (Assign/IndexAssign/...)
    // must have its write *target* renamed too, not just its read sites —
    // otherwise the closure mutates the original var (capturing/moving it) instead
    // of its `__cap` clone. (Closure v2, P3.)
    fn rn(v: &mut VarId, renames: &std::collections::HashMap<VarId, VarId>) {
        if let Some(&new) = renames.get(v) { *v = new; }
    }
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            replace_vars(value, renames);
        }
        IrStmtKind::Assign { var, value } => { rn(var, renames); replace_vars(value, renames); }
        IrStmtKind::FieldAssign { target, value, .. } => { rn(target, renames); replace_vars(value, renames); }
        IrStmtKind::IndexAssign { target, index, value } => {
            rn(target, renames); replace_vars(index, renames); replace_vars(value, renames);
        }
        IrStmtKind::MapInsert { target, key, value } => {
            rn(target, renames); replace_vars(key, renames); replace_vars(value, renames);
        }
        IrStmtKind::ListSwap { target, a, b } => {
            rn(target, renames); replace_vars(a, renames); replace_vars(b, renames);
        }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
            rn(target, renames); replace_vars(end, renames);
        }
        IrStmtKind::ListCopySlice { dst, src, len } => {
            rn(dst, renames); rn(src, renames); replace_vars(len, renames);
        }
        IrStmtKind::Guard { cond, else_ } => {
            replace_vars(cond, renames); replace_vars(else_, renames);
        }
        IrStmtKind::Expr { expr } => replace_vars(expr, renames),
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { rn(var, renames); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn needs_clone_type(ty: &Ty) -> bool {
    // §4 stage 2c (#531): derived from THE copy-ness classifier — see the
    // projection table in almide_ir::top_let_storage (note the tuple cell).
    almide_ir::top_let_storage::capture_clone_wrap(ty)
}
