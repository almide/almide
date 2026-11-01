//! STDLIB TEST COVERAGE LEDGER — every public stdlib function is called by at
//! least one executable test on BOTH legs (native Rust and wasm), or it is
//! named in `proofs/stdlib-test-coverage.txt` with the leg it is missing.
//!
//! The function list is the inspection ledger's (`proofs/stdlib-inspection.toml`,
//! itself held to the stdlib's public surface by tests/stdlib_inspection_test.rs),
//! so a new public function enters this gate the moment it exists — and fails
//! it until a test calls it on both legs.
//!
//! WHAT COUNTS AS A CALL. Each program is checked and lowered by the compiler
//! (`almide::wasm_leg::lower_entry_unlinked`), and the call targets are read off
//! the IR before any linking or inlining — so a UFCS call (`s.trim()`), a pipe
//! stage (`xs |> list.map(f)`) and a plain call all count, which a grep of the
//! source text cannot see. Only code reachable from a `test` block, `main`, an
//! abort site or a top-level `let` counts, and only a call whose value the run
//! OBSERVES (`Observe`): one that reaches an assertion, a byte-compared output
//! or a decision that gates one, or that has no value to read. Calling a
//! function and dropping its result is not a test of it. A few calls are erased
//! by lowering (the time constructors, the snapshot and abort assertions);
//! those are read from the program's tokens, on observed lines only
//! (`qualified_spellings`).
//!
//! WHICH PROGRAMS RUN ON WHICH LEG — the same sets CI executes:
//!   - a `spec/**` file with a `test` block (the rule `almide test` uses to find
//!     one) runs on NATIVE (`almide test spec/ --target rust`), and on WASM
//!     (`almide test spec/ --target wasm`) unless it declares `// wasm:skip` in
//!     its first three lines or is a native-fallback file
//!     (`proofs/wasm-fallback-baseline.txt`, ratcheted by check-wasm-fallback.sh);
//!   - a program in `proofs/output-parity-baseline.txt` runs on BOTH legs and is
//!     byte-compared (proofs/output-parity.sh);
//!   - every `spec/wasm_cross/*.almd` runs on BOTH legs and is byte-compared
//!     (tests/wasm_runtime_cross_target.rs) — a tracked divergence
//!     (`// @xt-allow:`) counts on native only; likewise every
//!     `spec/embedded_cross/*.almd` (tests/embedded_cross_test.rs) and the
//!     `spec/serve_cross` fixtures tests/http_serve_cross_test.rs drives.
//!
//! THE LEDGER. One row per function missing a leg: `module.fn :: legs :: why`,
//! where `legs` is `native`, `wasm` or `both`. It is EQUALITY-PINNED: a missing
//! leg with no row fails (a function lost its test, or a new one arrived
//! without one), and a row whose leg is now covered fails as STALE (shrink it in
//! the same change). `MAX_ROWS` must equal the row count and may only fall.
//! Every row's `why` names the issue that tracks it (`#NNNN`); a function that
//! `proofs/target-availability.toml` declares unavailable on the embedded leg
//! (the leg `almide test --target wasm` runs) must cite that declaration's
//! `issue`, so a missing wasm test and a declared missing wasm service cannot
//! drift apart.
//!
//! `ALMIDE_STDLIB_COVERAGE_REPORT=<path>` writes the per-function table
//! (function → native / wasm, with the first program that covers each leg);
//! `ALMIDE_UPDATE_STDLIB_COVERAGE=1` rewrites the ledger's rows from the
//! measurement (keeping each row's reason) and then fails, so the diff is reviewed.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use almide::ir::visit::{walk_expr, IrVisitor};
use almide::ir::{CallTarget, IrExpr, IrExprKind, IrProgram};

const LEDGER: &str = "proofs/stdlib-test-coverage.txt";
const INSPECTION: &str = "proofs/stdlib-inspection.toml";
const FALLBACK: &str = "proofs/wasm-fallback-baseline.txt";
const PARITY: &str = "proofs/output-parity-baseline.txt";
const AVAILABILITY: &str = "proofs/target-availability.toml";

/// The ledger's row count. It may only fall: a row is removed when a test
/// covers the leg it names.
const MAX_ROWS: usize = 0;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rows_of(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

fn public_functions(root: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(INSPECTION)).expect("inspection ledger");
    text.lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|l| l.trim_end_matches('"').to_string())
        .collect()
}

/// `collect_test_files` (src/cli/mod.rs): `.almd` files with a test block,
/// skipping hidden, `target` and `node_modules` directories and symlinks.
fn test_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if (name.starts_with('.') && name != ".") || name == "target" || name == "node_modules" {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let is_link = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        if path.is_dir() && !is_link {
            test_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "almd")
            && let Ok(text) = std::fs::read_to_string(&path)
            && (text.contains("\ntest ") || text.starts_with("test "))
        {
            out.push(path);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Leg {
    Native,
    Wasm,
}

/// The calls whose arguments a run OBSERVES. In every program: the assertion
/// family (a failing one fails the test) and `panic` (an abort is a verdict);
/// every `testing.*` fn is an assertion. In a program whose output a cross
/// net byte-compares (`compared`), also everything that writes that output.
const ASSERT_SINKS: &[&str] = &["assert", "assert_eq", "assert_ne", "panic"];
const OUTPUT_SINKS: &[&str] = &["println", "eprintln", "print", "eprint", "io.print", "io.eprint", "io.write", "process.exit"];

/// What a program's tests OBSERVE. A stdlib call counts only when its value
/// reaches an observation — an assertion's argument, a byte-compared output,
/// or a branch, match or loop that decides whether one runs — through `let`s,
/// user-fn parameters and returns (a fixpoint over the program), or when the
/// call has nothing to observe (its value is `Unit`, or `Unit` behind a `!`
/// whose err fails the test). A call whose result nobody reads is not a test
/// of it. The analysis is deliberately coarse where it is generous: a lambda's
/// body is observed when the call it is passed to is, and a branch condition is
/// observed when either arm holds an observation.
struct Observe<'a> {
    stdlib: &'a BTreeSet<String>,
    compared: bool,
    fns: HashMap<&'a str, &'a almide::ir::IrFunction>,
    /// User fns whose body holds an observation, transitively.
    sink_fns: HashSet<&'a str>,
    obs_vars: HashSet<almide::ir::VarId>,
    obs_fns: HashSet<&'a str>,
    reached: Vec<&'a str>,
    seen: HashSet<&'a str>,
    changed: bool,
    /// Inside an abort site: whichever call aborts IS the observation.
    force: bool,
    called: BTreeSet<String>,
    reached_lines: BTreeSet<usize>,
    observed_lines: BTreeSet<usize>,
}

fn call_name(target: &CallTarget) -> Option<String> {
    match target {
        CallTarget::Module { module, func, .. } => Some(format!("{}.{}", module.as_str(), func.as_str())),
        CallTarget::Method { method, .. } => Some(method.as_str().to_string()),
        CallTarget::Named { name } => Some(name.as_str().to_string()),
        CallTarget::Computed { .. } => None,
    }
}

/// Does `e` hold an observation (a sink call, or a call into a user fn that does)?
struct HasSink<'o, 'a> {
    o: &'o Observe<'a>,
    found: bool,
}

impl IrVisitor for HasSink<'_, '_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if self.found {
            return;
        }
        if let IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } = &e.kind
            && let Some(n) = call_name(target)
            && (self.o.is_sink(&n) || self.o.sink_fns.contains(n.as_str()))
        {
            self.found = true;
            return;
        }
        walk_expr(self, e);
    }
}

/// Walks each direct child of an expression with one observation flag.
struct Children<'o, 'a> {
    o: &'o mut Observe<'a>,
    obs: bool,
}

impl IrVisitor for Children<'_, '_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        let obs = self.obs;
        self.o.expr(e, obs);
    }
    fn visit_stmt(&mut self, s: &almide::ir::IrStmt) {
        self.o.stmt(s);
    }
}

fn pattern_vars(p: &almide::ir::IrPattern, out: &mut Vec<almide::ir::VarId>) {
    use almide::ir::IrPattern as P;
    match p {
        P::Bind { var, .. } => out.push(*var),
        P::As { var, inner, .. } => {
            out.push(*var);
            pattern_vars(inner, out)
        }
        P::Constructor { args: ps, .. } | P::Tuple { elements: ps } => ps.iter().for_each(|q| pattern_vars(q, out)),
        P::List { elements, rest } => {
            elements.iter().for_each(|q| pattern_vars(q, out));
            if let Some(r) = rest {
                pattern_vars(r, out)
            }
        }
        P::Some { inner } | P::Ok { inner } | P::Err { inner } => pattern_vars(inner, out),
        P::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(q) = &f.pattern {
                    pattern_vars(q, out)
                }
            }
        }
        P::Wildcard | P::Literal { .. } | P::None => {}
    }
}

impl<'a> Observe<'a> {
    fn new(ir: &'a IrProgram, stdlib: &'a BTreeSet<String>, compared: bool) -> Self {
        let fns: HashMap<&str, &almide::ir::IrFunction> = ir.functions.iter().map(|f| (f.name.as_str(), f)).collect();
        let mut o = Observe {
            stdlib,
            compared,
            fns,
            sink_fns: HashSet::new(),
            obs_vars: HashSet::new(),
            obs_fns: HashSet::new(),
            reached: Vec::new(),
            seen: HashSet::new(),
            changed: false,
            force: false,
            called: BTreeSet::new(),
            reached_lines: BTreeSet::new(),
            observed_lines: BTreeSet::new(),
        };
        // sink_fns: a least fixpoint over the call graph.
        loop {
            let mut grew = false;
            for f in &ir.functions {
                let name = f.name.as_str();
                if o.sink_fns.contains(name) {
                    continue;
                }
                let mut h = HasSink { o: &o, found: false };
                h.visit_expr(&f.body);
                if h.found {
                    o.sink_fns.insert(name);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        o
    }

    fn is_sink(&self, name: &str) -> bool {
        ASSERT_SINKS.contains(&name)
            || name.starts_with("testing.")
            || (self.compared && OUTPUT_SINKS.contains(&name))
    }

    fn has_sink(&self, e: &IrExpr) -> bool {
        let mut h = HasSink { o: self, found: false };
        h.visit_expr(e);
        h.found
    }

    fn stmts_have_sink(&self, ss: &[almide::ir::IrStmt]) -> bool {
        let mut h = HasSink { o: self, found: false };
        ss.iter().for_each(|s| h.visit_stmt(s));
        h.found
    }

    fn mark_var(&mut self, v: almide::ir::VarId) {
        if self.obs_vars.insert(v) {
            self.changed = true;
        }
    }

    fn reach(&mut self, name: &str, obs: bool) -> Option<&'a almide::ir::IrFunction> {
        let (k, f) = self.fns.get_key_value(name).map(|(k, f)| (*k, *f))?;
        if self.seen.insert(k) {
            self.reached.push(k);
        }
        if obs && self.obs_fns.insert(k) {
            self.changed = true;
        }
        Some(f)
    }

    fn param_obs(&self, f: &almide::ir::IrFunction, i: usize) -> bool {
        f.params.get(i).is_none_or(|p| self.obs_vars.contains(&p.var))
    }

    fn expr(&mut self, e: &IrExpr, obs: bool) {
        let obs = obs || self.force;
        if let Some(sp) = &e.span {
            self.reached_lines.insert(sp.line);
            if obs {
                self.observed_lines.insert(sp.line);
            }
        }
        match &e.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                let name = call_name(target);
                let recv = match target {
                    CallTarget::Method { object, .. } => Some(&**object),
                    CallTarget::Computed { callee } => {
                        self.expr(callee, obs);
                        None
                    }
                    _ => None,
                };
                let Some(name) = name else {
                    args.iter().for_each(|a| self.expr(a, obs));
                    return;
                };
                if self.is_sink(&name) {
                    recv.into_iter().chain(args.iter()).for_each(|a| self.expr(a, true));
                    return;
                }
                if let Some(f) = self.reach(&name, obs) {
                    for (i, a) in recv.into_iter().chain(args.iter()).enumerate() {
                        let o = self.param_obs(f, i);
                        self.expr(a, o);
                    }
                    return;
                }
                if self.stdlib.contains(&name) && (obs || e.ty == almide::types::Ty::Unit) {
                    self.called.insert(name);
                }
                recv.into_iter().chain(args.iter()).for_each(|a| self.expr(a, obs));
            }
            IrExprKind::FnRef { name } => {
                if self.reach(name.as_str(), obs).is_none() && obs && self.stdlib.contains(name.as_str()) {
                    self.called.insert(name.as_str().to_string());
                }
            }
            IrExprKind::Var { id } => {
                if obs {
                    self.mark_var(*id)
                }
            }
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().for_each(|s| self.stmt(s));
                if let Some(t) = expr {
                    self.expr(t, obs)
                }
            }
            IrExprKind::If { cond, then, else_ } => {
                let c = obs || self.has_sink(then) || self.has_sink(else_);
                self.expr(then, obs);
                self.expr(else_, obs);
                self.expr(cond, c);
            }
            IrExprKind::Match { subject, arms } => {
                let mut s = obs;
                for arm in arms {
                    let mut vs = Vec::new();
                    pattern_vars(&arm.pattern, &mut vs);
                    s |= self.has_sink(&arm.body)
                        || arm.guard.as_ref().is_some_and(|g| self.has_sink(g))
                        || vs.iter().any(|v| self.obs_vars.contains(v));
                }
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.expr(g, s);
                    }
                    self.expr(&arm.body, obs);
                }
                self.expr(subject, s);
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                body.iter().for_each(|st| self.stmt(st));
                let s = self.stmts_have_sink(body)
                    || self.obs_vars.contains(var)
                    || var_tuple.as_ref().is_some_and(|vs| vs.iter().any(|v| self.obs_vars.contains(v)));
                self.expr(iterable, s);
            }
            IrExprKind::While { cond, body } => {
                body.iter().for_each(|st| self.stmt(st));
                let s = self.stmts_have_sink(body);
                self.expr(cond, s);
            }
            // `expr!`: a `Unit` value behind it has nothing to read, and its err
            // fails the test or ends the run.
            IrExprKind::Try { expr } => {
                let o = obs || e.ty == almide::types::Ty::Unit;
                self.expr(expr, o);
            }
            _ => walk_expr(&mut Children { o: self, obs }, e),
        }
    }

    fn stmt(&mut self, s: &almide::ir::IrStmt) {
        use almide::ir::IrStmtKind as K;
        match &s.kind {
            K::Bind { var, value, .. } | K::Assign { var, value } => {
                let o = self.obs_vars.contains(var);
                self.expr(value, o);
            }
            K::BindDestructure { pattern, value } => {
                let mut vs = Vec::new();
                pattern_vars(pattern, &mut vs);
                let o = vs.iter().any(|v| self.obs_vars.contains(v));
                self.expr(value, o);
            }
            K::IndexAssign { target, index, value } | K::MapInsert { target, key: index, value } => {
                let o = self.obs_vars.contains(target);
                self.expr(index, o);
                self.expr(value, o);
            }
            K::FieldAssign { target, value, .. } => {
                let o = self.obs_vars.contains(target);
                self.expr(value, o);
            }
            // A guard decides whether the rest of the body runs.
            K::Guard { cond, else_ } => {
                self.expr(cond, true);
                self.expr(else_, false);
            }
            K::Expr { expr } => self.expr(expr, false),
            _ => almide::ir::visit::walk_stmt(&mut Children { o: self, obs: false }, s),
        }
    }

    /// One pass from the roots: tests, `main`, abort sites, top-level lets.
    fn pass(&mut self, ir: &'a IrProgram) {
        self.reached.clear();
        self.seen.clear();
        for tl in &ir.top_lets {
            let o = self.obs_vars.contains(&tl.var);
            self.expr(&tl.value, o);
        }
        for f in &ir.functions {
            let n = f.name.as_str();
            if f.is_test || n == "main" || n.starts_with("__almd_abort_site_") {
                self.reach(n, false);
            }
        }
        let mut i = 0;
        while i < self.reached.len() {
            let name = self.reached[i];
            i += 1;
            let f = self.fns[name];
            let site = name.starts_with("__almd_abort_site_");
            let tail = self.obs_fns.contains(name) || (self.compared && name == "main");
            let saved = self.force;
            self.force = site;
            self.expr(&f.body, tail);
            self.force = saved;
        }
    }
}

/// The stdlib calls a program's tests observe, and the source lines an
/// observed expression of a reached fn sits on (for the token half).
fn observed_calls(ir: &IrProgram, stdlib: &BTreeSet<String>, compared: bool) -> (BTreeSet<String>, BTreeSet<usize>, BTreeSet<usize>) {
    let mut o = Observe::new(ir, stdlib, compared);
    loop {
        o.changed = false;
        o.pass(ir);
        if !o.changed {
            break;
        }
    }
    o.called.clear();
    o.reached_lines.clear();
    o.observed_lines.clear();
    o.pass(ir);
    (o.called, o.observed_lines, o.reached_lines)
}

/// Calls the lowering ERASES leave no IR call to read: the ADR-0001 time
/// constructors (`duration.ms(5)` becomes Int arithmetic,
/// crates/almide-frontend/src/lower/calls_time.rs), `testing.assert_snapshot`
/// and `testing.assert_aborts` (desugared into abort sites, lower/calls.rs and
/// lower/calls_abort_site.rs). So the IR's calls are joined by the program's
/// qualified spellings read from its TOKENS — `<module> . <fn>` naming a public
/// function, which a comment or a string literal cannot spell — counted only
/// on a line where an OBSERVED expression of a reached fn sits (the erased
/// form keeps the call's span), or, for an assertion itself, a reached line.
fn qualified_spellings(
    text: &str,
    stdlib: &BTreeSet<String>,
    observed: &BTreeSet<usize>,
    reached: &BTreeSet<usize>,
) -> BTreeSet<String> {
    use almide::lexer::{Lexer, TokenType};
    let toks = Lexer::tokenize(text);
    let mut out = BTreeSet::new();
    for w in toks.windows(3) {
        if w[0].token_type == TokenType::Ident && w[1].token_type == TokenType::Dot && w[2].token_type == TokenType::Ident {
            let name = format!("{}.{}", w[0].value, w[2].value);
            let line = w[0].line;
            let counts = if name.starts_with("testing.") { reached.contains(&line) } else { observed.contains(&line) };
            if counts && stdlib.contains(&name) {
                out.insert(name);
            }
        }
    }
    out
}

struct Program {
    rel: String,
    legs: Vec<Leg>,
    /// A cross net byte-compares this program's output on its legs, so what
    /// it prints is observed.
    compared: bool,
}

fn programs(root: &Path) -> Vec<Program> {
    let listed = |file: &str| -> BTreeSet<String> {
        let text = std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        rows_of(&text).map(|l| l.split(" :: ").next().unwrap_or(l).trim().to_string()).collect()
    };
    let fallback = listed(FALLBACK);
    let parity = listed(PARITY);
    let mut files = Vec::new();
    test_files(&root.join("spec"), &mut files);
    let mut out: BTreeMap<String, BTreeSet<Leg>> = BTreeMap::new();
    let mut compared: BTreeSet<String> = BTreeSet::new();
    for path in files {
        let rel = path.strip_prefix(root).expect("under root").to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&path).expect("readable");
        let skip = text.lines().take(3).any(|l| l.contains("// wasm:skip"));
        let legs = out.entry(rel.clone()).or_default();
        legs.insert(Leg::Native);
        if !skip && !fallback.contains(&rel) {
            legs.insert(Leg::Wasm);
        }
    }
    for rel in parity {
        compared.insert(rel.clone());
        let legs = out.entry(rel).or_default();
        legs.insert(Leg::Native);
        legs.insert(Leg::Wasm);
    }
    // Whole-directory cross nets: each fixture runs on both legs and must
    // match. tests/wasm_runtime_cross_target.rs (spec/wasm_cross; a tracked
    // divergence, `// @xt-allow:`, is not a passing wasm run, so it counts on
    // native only) and tests/embedded_cross_test.rs (spec/embedded_cross, the
    // embedded host).
    for dir in ["spec/wasm_cross", "spec/embedded_cross"] {
        let mut fixtures: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .unwrap_or_else(|e| panic!("{dir}: {e}"))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "almd"))
            .collect();
        fixtures.sort();
        for path in fixtures {
            let rel = path.strip_prefix(root).expect("under root").to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&path).expect("readable");
            compared.insert(rel.clone());
            let legs = out.entry(rel).or_default();
            legs.insert(Leg::Native);
            if !text.lines().any(|l| l.trim().starts_with("// @xt-allow:")) {
                legs.insert(Leg::Wasm);
            }
        }
    }
    // The server net (tests/http_serve_cross_test.rs) drives the
    // spec/serve_cross fixtures its source names, on both legs.
    let serve = std::fs::read_to_string(root.join("tests/http_serve_cross_test.rs")).expect("serve net");
    for rel in serve.split('"').filter(|s| s.starts_with("spec/serve_cross/") && s.ends_with(".almd")) {
        compared.insert(rel.to_string());
        let legs = out.entry(rel.to_string()).or_default();
        legs.insert(Leg::Native);
        legs.insert(Leg::Wasm);
    }
    let probe = std::env::var("ALMIDE_STDLIB_COVERAGE_PROBE").ok();
    out.into_iter()
        .filter(|(rel, _)| probe.as_deref().is_none_or(|p| rel == p))
        .map(|(rel, legs)| Program { compared: compared.contains(&rel), rel, legs: legs.into_iter().collect() })
        .collect()
}

/// fn → leg → the first program (in path order) that calls it on that leg.
type Coverage = BTreeMap<String, BTreeMap<Leg, String>>;

fn measure(root: &Path, stdlib: &BTreeSet<String>) -> (Coverage, Vec<String>, usize) {
    let progs = programs(root);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let chunk = progs.len().div_ceil(threads).max(1);
    // Each program's lowering is independent; the front holds no shared state.
    let results: Vec<(usize, Result<BTreeSet<String>, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = progs
            .chunks(chunk)
            .enumerate()
            .map(|(ci, part)| {
                s.spawn(move || {
                    part.iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let text = std::fs::read_to_string(root.join(&p.rel)).unwrap_or_default();
                            let path = root.join(&p.rel).to_string_lossy().to_string();
                            let r = std::panic::catch_unwind(|| almide::wasm_leg::lower_entry_unlinked(&path, &text))
                                .unwrap_or_else(|_| Err("front panicked".into()));
                            let r = r.map(|ir| {
                                let (mut called, observed, reached) = observed_calls(&ir, stdlib, p.compared);
                                called.extend(qualified_spellings(&text, stdlib, &observed, &reached));
                                if std::env::var("ALMIDE_STDLIB_COVERAGE_PROBE").is_ok() {
                                    eprintln!("probe {}: observed {:?}", p.rel, called);
                                }
                                called
                            });
                            (ci * chunk + i, r)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });
    let mut cov: Coverage = BTreeMap::new();
    let mut unlowered = Vec::new();
    let mut results = results;
    results.sort_by_key(|(i, _)| *i);
    for (i, r) in results {
        let p = &progs[i];
        match r {
            Ok(called) => {
                for f in called {
                    let legs = cov.entry(f).or_default();
                    for leg in &p.legs {
                        legs.entry(*leg).or_insert_with(|| p.rel.clone());
                    }
                }
            }
            Err(e) => unlowered.push(format!("{}: {}", p.rel, e.lines().next().unwrap_or(""))),
        }
    }
    (cov, unlowered, progs.len())
}

/// fn → the `issue` of its `[[unavailable]]` row in the availability matrix,
/// for the rows whose `legs` include the embedded leg.
fn embedded_unavailable(root: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(root.join(AVAILABILITY)).expect("availability matrix");
    let mut out = BTreeMap::new();
    for block in text.split("\n[[unavailable]]\n").skip(1) {
        let field = |k: &str| {
            block.lines().find_map(|l| l.strip_prefix(&format!("{k} = \"")).and_then(|r| r.strip_suffix('"')).map(str::to_string))
        };
        let legs = block.lines().find_map(|l| l.strip_prefix("legs = [")).unwrap_or("");
        if let (Some(f), true) = (field("fn"), legs.contains("\"embedded\"")) {
            out.insert(f, field("issue").unwrap_or_default());
        }
    }
    out
}

fn cites_issue(why: &str) -> bool {
    why.split('#').skip(1).any(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

fn legs_word(native: bool, wasm: bool) -> Option<&'static str> {
    match (native, wasm) {
        (true, true) => None,
        (false, true) => Some("native"),
        (true, false) => Some("wasm"),
        (false, false) => Some("both"),
    }
}

fn report(path: &str, stdlib: &BTreeSet<String>, cov: &Coverage, unlowered: &[String], programs: usize) {
    let mut per_module: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    let mut rows = String::new();
    let (mut n, mut w, mut both) = (0, 0, 0);
    for f in stdlib {
        let c = cov.get(f);
        let nat = c.and_then(|c| c.get(&Leg::Native));
        let was = c.and_then(|c| c.get(&Leg::Wasm));
        let m = per_module.entry(f.split('.').next().unwrap_or("")).or_default();
        m[0] += 1;
        if nat.is_some() {
            m[1] += 1;
            n += 1;
        }
        if was.is_some() {
            m[2] += 1;
            w += 1;
        }
        if nat.is_some() && was.is_some() {
            m[3] += 1;
            both += 1;
        }
        let cell = |x: Option<&String>| x.map(|p| format!("yes (`{p}`)")).unwrap_or_else(|| "**no**".into());
        rows.push_str(&format!("| `{f}` | {} | {} |\n", cell(nat), cell(was)));
    }
    let total = stdlib.len();
    let mut out = format!(
        "# Stdlib test coverage (native / wasm)\n\nMeasured by tests/stdlib_test_coverage_test.rs from the compiler's IR \
         (calls reachable from test blocks / main / abort sites / top lets whose value the test observes) over {programs} programs.\n\n\
         | | functions |\n|---|---:|\n| public | {total} |\n| native covered | {n} |\n| wasm covered | {w} |\n\
         | both legs | {both} |\n| missing a leg | {} |\n\n## Per module\n\n| module | fns | native | wasm | both |\n|---|---:|---:|---:|---:|\n",
        total - both
    );
    for (m, c) in &per_module {
        out.push_str(&format!("| {m} | {} | {} | {} | {} |\n", c[0], c[1], c[2], c[3]));
    }
    out.push_str("\n## Per function\n\n| function | native | wasm |\n|---|---|---|\n");
    out.push_str(&rows);
    if !unlowered.is_empty() {
        out.push_str("\n## Programs the front did not lower (their calls are not counted)\n\n");
        for u in unlowered {
            out.push_str(&format!("- {u}\n"));
        }
    }
    std::fs::write(path, out).expect("write report");
}

#[test]
fn every_public_stdlib_fn_is_tested_on_both_legs() {
    let root = root();
    let stdlib = public_functions(&root);
    assert!(stdlib.len() > 1000, "the inspection ledger names {} functions — the list went blind", stdlib.len());
    let (cov, unlowered, programs) = measure(&root, &stdlib);
    assert!(cov.len() > 100, "only {} functions measured as called — the sweep went blind", cov.len());

    if let Ok(path) = std::env::var("ALMIDE_STDLIB_COVERAGE_REPORT") {
        report(&path, &stdlib, &cov, &unlowered, programs);
    }

    let missing: BTreeMap<&str, &str> = stdlib
        .iter()
        .filter_map(|f| {
            let c = cov.get(f);
            let has = |leg| c.is_some_and(|c| c.contains_key(&leg));
            legs_word(has(Leg::Native), has(Leg::Wasm)).map(|w| (f.as_str(), w))
        })
        .collect();

    let ledger_text = std::fs::read_to_string(root.join(LEDGER)).expect("coverage ledger");
    let mut listed: BTreeMap<String, (String, String)> = BTreeMap::new();
    for row in rows_of(&ledger_text) {
        let parts: Vec<&str> = row.split(" :: ").map(str::trim).collect();
        assert!(parts.len() >= 2, "malformed ledger row (want `module.fn :: legs :: why`): {row}");
        let prev = listed.insert(parts[0].to_string(), (parts[1].to_string(), parts.get(2).unwrap_or(&"").to_string()));
        assert!(prev.is_none(), "duplicate ledger row: {}", parts[0]);
    }

    if std::env::var("ALMIDE_UPDATE_STDLIB_COVERAGE").is_ok() {
        let header: String = ledger_text.lines().take_while(|l| l.starts_with('#') || l.trim().is_empty()).map(|l| format!("{l}\n")).collect();
        let mut body = String::new();
        for (f, legs) in &missing {
            let why = listed.get(*f).map(|(_, w)| w.as_str()).filter(|w| !w.is_empty()).unwrap_or("no test calls it on this leg yet");
            body.push_str(&format!("{f} :: {legs} :: {why}\n"));
        }
        std::fs::write(root.join(LEDGER), format!("{header}{body}")).expect("write ledger");
        panic!("ledger rewritten with {} row(s) — review the diff and set MAX_ROWS", missing.len());
    }

    let mut problems = Vec::new();
    for (f, legs) in &missing {
        match listed.get(*f) {
            None => problems.push(format!("  + {f} has no test on {} and no ledger row", match *legs {
                "both" => "either leg".to_string(),
                l => format!("the {l} leg"),
            })),
            Some((l, _)) if l != legs => problems.push(format!("  ~ {f}: the row says `{l}`, measured missing `{legs}`")),
            _ => {}
        }
    }
    let unavailable = embedded_unavailable(&root);
    for (f, (_, why)) in &listed {
        if !cites_issue(why) {
            problems.push(format!("  ? {f}: the row's reason names no tracking issue (`#NNNN`): {why:?}"));
        }
        if let Some(issue) = unavailable.get(f)
            && !issue.is_empty()
            && !why.contains(issue.as_str())
        {
            problems.push(format!(
                "  ? {f}: {AVAILABILITY} declares it unavailable on the embedded leg under {issue}; the row must cite {issue}"
            ));
        }
    }
    for f in listed.keys() {
        if !missing.contains_key(f.as_str()) {
            if stdlib.contains(f) {
                problems.push(format!("  - {f} is now tested on both legs — remove its row (STALE)"));
            } else {
                problems.push(format!("  - {f} is not a public stdlib function — remove its row (STALE)"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "stdlib test coverage ({LEDGER}) out of date:\n{}\n\nA public stdlib fn needs a test that calls it on BOTH \
         legs: a spec/stdlib/*_test.almd (run by `almide test spec/` on native and `--target wasm`), or a \
         spec/wasm_cross / spec/embedded_cross fixture (byte-compared on both legs).",
        problems.join("\n")
    );
    // At MAX_ROWS = 0 clippy 1.97's deny-level absurd_extreme_comparisons reads
    // `<=` as `==`; the check stays `<=` so it keeps its message once a row exists.
    #[allow(clippy::absurd_extreme_comparisons)]
    assert!(
        listed.len() <= MAX_ROWS,
        "{LEDGER} has {} rows but MAX_ROWS is {MAX_ROWS}: the ledger only shrinks",
        listed.len()
    );
    assert_eq!(listed.len(), MAX_ROWS, "{LEDGER} shrank to {} rows — lower MAX_ROWS to match", listed.len());
}
