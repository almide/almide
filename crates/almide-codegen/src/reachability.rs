//! Reachability-based dead-function elimination for WASM emit (#644).
//!
//! The post-compile `dce::eliminate_dead_code` cannot help when an UNREACHABLE
//! function's body references a native-only intrinsic (e.g. a matrix Q8 op with
//! no WASM runtime): emit panics while *compiling* that body, long before DCE
//! runs. So we compute reachability over the IR *before* compilation and stub
//! out the bodies that the entry surface can never reach. A module that merely
//! imports another whose unused fns touch native-only code then compiles for
//! WASM — matching the user's intuition ("I never call it") and the native
//! target, which already compiles it fine (Rust's own linker drops the dead fn).
//!
//! Soundness contract: this set is an OVER-approximation of the truly-reachable
//! functions — every name a call could resolve to is marked, so a function is
//! pruned only when NONE of its registered names is reachable. A false prune
//! would turn a working call into a runtime trap; a false keep merely compiles
//! one extra body (and post-compile DCE stubs it anyway). When in doubt, keep.

use std::collections::{HashSet, VecDeque};

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram};

/// Collect every function-name an expression tree could *call*: direct
/// `Named`/`Module` call targets, `RuntimeCall` symbols, and the function names
/// referenced as values (`FnRef`, lifted `ClosureCreate`). Module targets
/// contribute BOTH the bare `func` and the `module.func` qualified key, because
/// intra-module calls resolve via the qualified name while cross-module/lifted
/// references resolve via the bare name (see `emit_call` in calls.rs).
struct CallNameCollector<'a> {
    out: &'a mut HashSet<String>,
}

impl IrVisitor for CallNameCollector<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => {
                add_target(target, self.out);
            }
            IrExprKind::RuntimeCall { symbol, .. } => {
                self.out.insert(symbol.to_string());
            }
            IrExprKind::FnRef { name } => {
                self.out.insert(name.to_string());
            }
            IrExprKind::ClosureCreate { func_name, .. } => {
                self.out.insert(func_name.to_string());
            }
            _ => {}
        }
        // Always recurse for nested calls (args, lambda bodies, match arms, …).
        walk_expr(self, expr);
    }
}

fn add_target(target: &CallTarget, out: &mut HashSet<String>) {
    match target {
        CallTarget::Named { name } => {
            out.insert(name.to_string());
            // `mod.func` written as a bare Named (the dotted-name fast path in
            // emit_call) — record the bare tail too so it matches a fn registered
            // under either spelling.
            if let Some(dot) = name.as_str().rfind('.') {
                out.insert(name.as_str()[dot + 1..].to_string());
            }
        }
        CallTarget::Module { module, func, .. } => {
            out.insert(func.to_string());
            out.insert(format!("{}.{}", module, func));
        }
        // Method/Computed have no static callee name; their object/callee
        // sub-expressions are walked by the visitor (args + receiver).
        CallTarget::Computed { .. } | CallTarget::Method { .. } => {}
    }
}

/// All the names under which a function with source name `fname` in module
/// `module` (None for top-level `program.functions`) may be REGISTERED in
/// `func_map`, mirroring `register_func` calls in mod.rs. A function is reachable
/// iff any of these is in the reachable name set.
pub(crate) fn registered_keys(module: Option<&str>, fname: &str) -> Vec<String> {
    match module {
        None => vec![fname.to_string()],
        Some(m) => {
            let sanitized = fname.replace(' ', "_").replace('-', "_").replace('.', "_");
            let mod_ident = m.replace('.', "_");
            vec![
                fname.to_string(),                               // bare alias
                format!("{}.{}", m, fname),                      // qualified
                format!("almide_rt_{}_{}", mod_ident, sanitized), // mangled symbol
            ]
        }
    }
}

/// Collect the call-names directly referenced by a single root expression
/// (a top-level `let` initializer, which `__init_globals` runs at startup).
pub(super) fn names_called_in(expr: &IrExpr, out: &mut HashSet<String>) {
    CallNameCollector { out }.visit_expr(expr);
}

/// Compute the set of reachable function NAMES (all spellings) starting from
/// `roots`, following call edges through `program.functions` and every
/// `program.modules[*].functions` body. Lambda bodies are reached transitively
/// because their parent function's body contains the `Lambda`/`ClosureCreate`
/// node and the visitor walks into it.
pub(super) fn compute_reachable_fn_names(
    program: &IrProgram,
    roots: impl IntoIterator<Item = String>,
) -> HashSet<String> {
    let by_name = index_fn_bodies_by_name(program);
    bfs_reachable(&by_name, roots)
}

/// Function-body indexing phase of `compute_reachable_fn_names`, extracted
/// verbatim (cog>30 decomposition, sequential-phase pattern — the BFS
/// phase below only reads this index, never mutates it back). Indexes
/// every defined function body by all of its registered key spellings —
/// a single body can be enqueued via any of its names.
fn index_fn_bodies_by_name(program: &IrProgram) -> std::collections::HashMap<String, &IrExpr> {
    let mut by_name: std::collections::HashMap<String, &IrExpr> = std::collections::HashMap::new();
    for f in &program.functions {
        for k in registered_keys(None, f.name.as_str()) {
            by_name.entry(k).or_insert(&f.body);
        }
    }
    for m in &program.modules {
        let mname = m.name.to_string();
        for f in &m.functions {
            for k in registered_keys(Some(&mname), f.name.as_str()) {
                by_name.entry(k).or_insert(&f.body);
            }
        }
    }
    by_name
}

/// BFS-worklist phase of `compute_reachable_fn_names`, extracted verbatim
/// (cog>30 decomposition) — reads `by_name` read-only.
fn bfs_reachable(
    by_name: &std::collections::HashMap<String, &IrExpr>,
    roots: impl IntoIterator<Item = String>,
) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for r in roots {
        if reachable.insert(r.clone()) {
            queue.push_back(r);
        }
    }

    while let Some(name) = queue.pop_front() {
        let Some(body) = by_name.get(name.as_str()).copied() else { continue };
        let mut called: HashSet<String> = HashSet::new();
        CallNameCollector { out: &mut called }.visit_expr(body);
        for c in called {
            if reachable.insert(c.clone()) {
                queue.push_back(c);
            }
        }
    }

    reachable
}

/// Reachable-function name set for a whole program, using the SAME roots the WASM
/// emitter prunes by: `main`, exported `pub fn`s, tests (when not library mode),
/// and any fn named by a top-level-`let` initializer (run by `__init_globals`).
/// Shared by the emitter's body-compile gate AND the CLI's native-only-op
/// pre-check (lib.rs), so an unreachable native-only intrinsic the emitter stubs
/// is equally ignored by the pre-check — the build no longer fails on dead code
/// the program can never run (#644).
pub(crate) fn reachable_fn_names(program: &IrProgram) -> HashSet<String> {
    let has_main = program.functions.iter().any(|f| f.name.as_str() == "main" && !f.is_test);
    let has_tests = program.functions.iter().any(|f| f.is_test);
    let library_mode = !has_main && !has_tests;
    let mut roots: HashSet<String> = HashSet::new();
    if has_main {
        roots.insert("main".to_string());
    }
    for func in &program.functions {
        if func.is_test {
            if !library_mode {
                roots.insert(func.name.to_string());
            }
        } else if matches!(func.visibility, almide_ir::IrVisibility::Public) {
            roots.insert(func.name.to_string());
        }
    }
    for tl in &program.top_lets {
        names_called_in(&tl.value, &mut roots);
    }
    for m in &program.modules {
        for tl in &m.top_lets {
            names_called_in(&tl.value, &mut roots);
        }
    }
    compute_reachable_fn_names(program, roots)
}
