//! v1 execution pipeline: a real `.almd` **source** program → a COMPLETE wasm module (WAT text)
//! via the v1 MIR renderer — the library form of the `render_program` example's `main()`, so the
//! `almide` CLI can drive the v1 path (opt-in `--verified` codegen) with a v0 fallback.
//!
//! The ONLY caller-supplied input beyond the source is `self_modules` — the resolved cross-module
//! `import self.<submodule>` siblings (the caller runs the canonical driver discovery, which uses
//! the `almide` crate and therefore cannot live in this library). Everything downstream of that
//! resolution lives here.
//!
//! Totality: every failure path returns `Err(LowerError::Unsupported(..))` (a clean WALL), NEVER a
//! process abort — so a caller can fall back to v0 codegen when v1 declines.

use crate::lower::LowerError;
use crate::render_wasm::try_render_wasm_program;
use crate::MirProgram;
use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_optimize::{mono, optimize};
use std::collections::HashMap;

/// The mangled flat name a user-module function gets when resolved to a user `CallFn`
/// (`bindgen` + `get_str` → `almide_rt_bindgen_get_str`) — the v1 analogue of v0's
/// `ir_link_flatten` module-fn renaming, and the call-site target this resolution emits.
fn user_module_fn_name(module: &str, func: &str) -> String {
    format!("almide_rt_{}_{}", module.replace('.', "_"), func.replace('.', "_"))
}

/// Resolve a USER-package/-module call (`bindgen.get_str(…)` via `import self as bindgen`,
/// `self.classifier.classify(…)`) to a real user `CallFn`. WITHOUT this, the MIR lowering
/// sees `CallTarget::Module { module: "bindgen", … }` and walls it as an "effectful/impure
/// stdlib Module call" — but `bindgen` is a USER module whose function is right here in
/// `ir.modules` (thanks to the sibling-link). This rewrites the CALL TARGET only (no IR-level
/// flatten — that would collide the per-module VarId regions; the sibling DEFINITIONS are
/// lowered separately to MIR with the same mangled name):
///   • a `CallTarget::Module { m, f }` where `m` is a user module that defines `f` becomes
///     `CallTarget::Named { name: "almide_rt_<m>_<f>" }` — an ORDINARY user call.
/// SOUNDNESS (caps): the resolved name carries NO dot, so the transitive caps gate treats it
/// as a user call (analyzed via the in-profile map / tainted if unknown), NOT as a pure
/// dotted stdlib call (`is_known_free`). A self-pkg call to an EFFECTFUL user fn therefore
/// surfaces its capability transitively, exactly like any direct user call. A STDLIB module
/// (`string`, bundled `json`, …) is NOT rewritten. No-op when there are no linked user modules.
/// A module whose functions should link like ordinary user siblings.
/// User modules always; a BUNDLED stdlib module qualifies too when ALL its
/// fns are pure Almide (no @intrinsic / @inline_rust / @wasm_intrinsic /
/// @extern, no hole bodies) — `import path` / `import args` then lower as
/// real linked modules instead of walling as "unlinked stdlib call".
/// Intrinsic-bearing bundled modules (json, …) keep registry-backed dispatch.
pub(crate) fn is_linkable_module(m: &almide_ir::IrModule) -> bool {
    let n = m.name.as_str();
    if !almide_lang::stdlib_info::is_any_stdlib(n) {
        return true;
    }
    almide_lang::stdlib_info::is_bundled_module(n)
        && m.functions.iter().all(is_pure_almide_fn)
}

/// A function with a real Almide body and no host boundary — linkable as an
/// ordinary sibling fn.
fn is_pure_almide_fn(f: &almide_ir::IrFunction) -> bool {
    f.extern_attrs.is_empty()
        && !f
            .attrs
            .iter()
            .any(|a| matches!(a.name.as_str(), "intrinsic" | "inline_rust" | "wasm_intrinsic"))
        && !matches!(f.body.kind, almide_ir::IrExprKind::Hole)
}

/// Every `module.fn` the self-host registry already serves. A bundled module's
/// own Almide body must NOT shadow a registered self-host: the registry entry is
/// the proven, rc-audited implementation the renderer links.
fn registry_served_names() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::OnceLock;
    static NAMES: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        crate::render_wasm::self_host_runtime()
            .iter()
            .flat_map(|(_, pairs)| pairs.iter().map(|(_, call_name)| *call_name))
            .collect()
    })
}

/// The function names of `m` that resolve like ordinary user siblings.
///
/// A wholly-pure module contributes all of them ([`is_linkable_module`]). An
/// INTRINSIC-BEARING bundled module (`list`, `string`, …) contributes its
/// pure-Almide extensions only — `list.split_at`/`iterate`/`bundled_probe` have
/// real Almide bodies and no registry entry, so before this they resolved to
/// nothing and walled as "unlinked stdlib call" even though their source ships
/// in the binary. Its intrinsic-backed fns keep registry-backed dispatch.
pub fn linkable_module_fns(m: &almide_ir::IrModule) -> std::collections::HashSet<String> {
    let all = |m: &almide_ir::IrModule| {
        m.functions.iter().map(|f| f.name.as_str().to_string()).collect()
    };
    let n = m.name.as_str();
    if !almide_lang::stdlib_info::is_any_stdlib(n) {
        return all(m);
    }
    if !almide_lang::stdlib_info::is_bundled_module(n) {
        return std::collections::HashSet::new();
    }
    if m.functions.iter().all(is_pure_almide_fn) {
        return all(m);
    }
    let served = registry_served_names();
    m.functions
        .iter()
        .filter(|f| is_pure_almide_fn(f))
        .map(|f| f.name.as_str().to_string())
        .filter(|f| !served.contains(format!("{n}.{f}").as_str()))
        .collect()
}

/// The package a resolved sibling module belongs to, if it is a DEPENDENCY's
/// submodule.
///
/// The MIR pipeline re-runs the frontend from source, so it has to re-derive
/// what the CLI driver knows from the resolver: a dependency's modules are
/// registered under package-qualified names (`ceangal.render`), and inside such
/// a module `import self.layout` must resolve against the PACKAGE, not against
/// the module itself. Without that, `resolve_import_canonical` built the fqn
/// `ceangal.render.layout`, found it unregistered, fell back to the bare leaf
/// `layout`, and the sibling call's signature was never found — the call typed
/// `Unknown` and the function walled with "Unknown type reached MIR lowering",
/// which the consumer then saw as an unlinked
/// `almide_rt_ceangal_render_render_at` (#904). The package's OWN build never
/// hit this: standalone, its modules are named bare, so the leaf fallback
/// happened to be right.
///
/// A `self` module (the project's own `src/*.almd`) keeps its leaf-name
/// registration and needs no package scope.
fn dependency_package_of(name: &str, is_self: bool) -> Option<&str> {
    if is_self {
        return None;
    }
    name.split_once('.').map(|(pkg, _)| pkg)
}

fn resolve_user_module_calls(ir: &mut almide_ir::IrProgram) {
    use almide_ir::{walk_expr_mut, CallTarget, IrExprKind, IrMutVisitor};
    use almide_lang::intern::sym;
    let user_mods: std::collections::HashMap<String, std::collections::HashSet<String>> = ir
        .modules
        .iter()
        .map(|m| (m.name.as_str().to_string(), linkable_module_fns(m)))
        .filter(|(_, fns)| !fns.is_empty())
        .collect();
    if user_mods.is_empty() {
        return; // single-file / stdlib-only — strict no-op.
    }
    struct Rw<'a> {
        user_mods: &'a std::collections::HashMap<String, std::collections::HashSet<String>>,
        root_fns: std::collections::HashSet<String>,
        /// The module whose body is being rewritten, if any. A dependency's
        /// modules are registered under their PACKAGE-QUALIFIED names
        /// (`ceangal.layout`), but a call BETWEEN them still names the sibling
        /// the way that package's own source does — bare `layout.layout`. With
        /// no enclosing scope the bare name matched no user module, the call
        /// stayed a `CallTarget::Module`, and the MIR walled it as an
        /// "effectful/impure stdlib Module call layout.layout" (#904).
        enclosing: Option<String>,
    }
    impl Rw<'_> {
        /// The user module a call's `module` segment refers to: an exact match,
        /// or — inside a package — the sibling under the enclosing module's own
        /// package prefix.
        fn resolve_module<'m>(&'m self, m: &'m str, f: &str) -> Option<&'m str> {
            if self.user_mods.get(m).is_some_and(|fs| fs.contains(f)) {
                return Some(m);
            }
            let prefix = self.enclosing.as_deref()?.rsplit_once('.')?.0;
            let qualified = format!("{prefix}.{m}");
            let (k, _) = self
                .user_mods
                .get_key_value(&qualified)
                .filter(|(_, fs)| fs.contains(f))?;
            Some(k.as_str())
        }
    }
    impl IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Call { target, .. } = &mut e.kind {
                if let CallTarget::Module { module, func, .. } = target {
                    let (m, f) = (module.as_str(), func.as_str());
                    if let Some(owner) = self.resolve_module(m, f) {
                        *target = CallTarget::Named { name: sym(&user_module_fn_name(owner, f)) };
                    }
                } else if let CallTarget::Named { name } = target {
                    // A BARE Named call to a fn that lives in exactly ONE linked user module: the
                    // frontend resolves an `import self as g` call to the bare name when the target is
                    // the package's own module — rewrite to the module fn's mangled def name. Ambiguity
                    // (two modules defining the name, or a root fn shadowing it) leaves the call
                    // untouched — the unlinked gate then walls it honestly instead of guessing.
                    let f = name.as_str();
                    if !self.root_fns.contains(f) {
                        let mut owners = self.user_mods.iter().filter(|(_, fs)| fs.contains(f));
                        if let (Some((m, _)), None) = (owners.next(), owners.next()) {
                            *target = CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                        }
                    }
                }
            }
        }
    }
    let root_fns: std::collections::HashSet<String> =
        ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    let mut rw = Rw { user_mods: &user_mods, root_fns, enclosing: None };
    for func in &mut ir.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut ir.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut ir.modules {
        rw.enclosing = Some(m.name.as_str().to_string());
        for func in &mut m.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }
}

/// The #1052 pre-inference import audit: every non-stdlib import must have a
/// resolved module in `modules` (matched on the full name or its first/last
/// dot-segment — CLI resolvers register both bare and package-qualified
/// spellings). `import self as pkg` aliases the file's own package and needs
/// no module. Returns the one-line feature wall for the first unsatisfied
/// import, `None` when every import is covered.
fn unresolved_import_wall(
    prog: &almide_lang::ast::Program,
    modules: &[(String, almide_lang::ast::Program, bool)],
) -> Option<LowerError> {
    for imp in &prog.imports {
        let almide_lang::ast::Decl::Import { path, span, .. } = imp else { continue };
        let Some(root) = path.first() else { continue };
        let wanted = if root.as_str() == "self" {
            match path.get(1) {
                Some(sibling) => sibling.as_str(),
                None => continue,
            }
        } else {
            if almide_lang::stdlib_info::is_stdlib_module(root.as_str()) {
                continue;
            }
            root.as_str()
        };
        let satisfied = modules.iter().any(|(n, _, _)| {
            n == wanted
                || n.split('.').next() == Some(wanted)
                || n.rsplit('.').next() == Some(wanted)
        });
        if !satisfied {
            let spelled = path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".");
            let kind = if root.as_str() == "self" { "package sibling" } else { "dependency module" };
            return Some(LowerError::at(
                *span,
                format!(
                    "import {spelled} — {kind} not resolved by this render's front-end \
                     (feature wall, not a type error; cf. #943 for the linking-stage wall)"
                ),
            ));
        }
    }
    None
}

/// Lower `.almd` source to a linked `IrProgram` (`parse → check → lower → optimize → mono →
/// ir_link`) — the SAME frontend cut point emit_cert_from_source uses. `modules` are the resolved
/// cross-module siblings (empty ⇒ the single-file path); each is inferred + `lower_module`d into
/// `ir.modules` so a cross-module record/variant type reaches `build_record_layouts`. A parse or
/// type error is a clean WALL (`Err`), never an abort.
fn source_to_ir_with(
    source: &str,
    modules: &[(String, almide_lang::ast::Program, bool)],
) -> Result<almide_ir::IrProgram, LowerError> {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens);
    let mut prog = parser
        .parse()
        .map_err(|e| LowerError::Unsupported(format!("parse error: {e:?}")))?;
    // `Parser::parse()` is a recovery parser: it can return `Ok` with a
    // partial `Program` (unparseable top-level items dropped) while still
    // recording the failures in `.errors` — the CLI's own `parse_file`
    // checks this separately (main.rs). Skipping this check here would
    // silently compile a truncated program instead of walling honestly.
    if !parser.errors.is_empty() {
        let messages: Vec<String> = parser.errors.iter().map(|d| d.display()).collect();
        return Err(LowerError::Unsupported(format!(
            "parse error: {}",
            messages.join("\n")
        )));
    }
    // #1052: an import this render was NOT handed a module for can never
    // type-check — every reference through it would surface as "undefined
    // function" and the wall would land in the "type errors" bucket, the one
    // category the walled-real ledgers audit as empty-by-construction. A
    // missing module is a FEATURE gap of the caller (the native rung passes no
    // siblings), so classify it as one, before inference, without the cascade.
    // Adjacent but distinct from #943: that wall HAS the module and fails to
    // link it. Stdlib imports are typed from stdlib info and need no module.
    if let Some(wall) = unresolved_import_wall(&prog, modules) {
        return Err(wall);
    }
    let canon = canonicalize::canonicalize_program(
        &prog,
        modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
    );
    let mut checker = Checker::from_env(canon.env);
    // #785 parity with the CLI drivers: module top-let types must be fully
    // inferred BEFORE the entry program reads them. Without this pre-pass a
    // cross-module reader of a generic-ctor top-let (`let MAYBE = some(Cfg
    // {…})`) sees the registration seed `Option[Unknown]`, the match payload
    // binding stays Unknown, and the whole program walls.
    for (name, mod_prog, is_self_mod) in modules {
        if almide_lang::stdlib_info::is_stdlib_module(name)
            && !almide_lang::stdlib_info::is_bundled_module(name)
        {
            continue;
        }
        let saved_self = checker.env.self_module_name;
        checker.env.self_module_name =
            dependency_package_of(name, *is_self_mod).map(almide_lang::intern::sym);
        checker.refresh_module_top_lets(mod_prog, name);
        checker.env.self_module_name = saved_self;
    }
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .map(|d| d.message.clone())
        .collect();
    if !errors.is_empty() {
        return Err(LowerError::Unsupported(format!("type errors: {errors:?}")));
    }
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);

    // Lower each resolved sibling MODULE into `ir.modules` — the SAME sequence the real driver runs
    // after `lower_program` (infer_module → per-module import table → lower_module → push). Bundled
    // stdlib modules carried by `resolve` are skipped (their defs come from the runtime/self-host
    // registry); only real user siblings contribute their type_decls + fns.
    for (name, mod_prog, is_self) in modules {
        if almide_lang::stdlib_info::is_stdlib_module(name)
            && !almide_lang::stdlib_info::is_bundled_module(name)
        {
            continue;
        }
        let mut mod_prog = mod_prog.clone();
        // Scope the module to its PACKAGE while it is inferred and lowered, so
        // `import self.<sibling>` inside a dependency resolves to the
        // package-qualified sibling the resolver registered (#904).
        let saved_self = checker.env.self_module_name;
        checker.env.self_module_name =
            dependency_package_of(name, *is_self).map(almide_lang::intern::sym);
        checker.infer_module(&mut mod_prog, name);
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name.as_str());
        let (mod_table, _) = almide_frontend::import_table::build_import_table(
            &mod_prog,
            Some(import_table_name),
            &checker.env.user_modules,
        );
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mod_ir = almide_frontend::lower::lower_module(
            name,
            &mod_prog,
            &checker.env,
            &checker.type_map,
            None,
        );
        checker.env.import_table = saved_table;
        checker.env.self_module_name = saved_self;
        ir.modules.push(mod_ir);
    }

    resolve_user_module_calls(&mut ir);

    almide_driver::link_ir(&mut ir);
    // Transparent-newtype erasure LAST (post-link, pre-lowering): `mod type X = String`
    // ctor calls/patterns/Ty tags become the inner type (see newtype_erase.rs).
    crate::lower::erase_transparent_newtypes(&mut ir);
    // Record default-field fill (Opts {} materializes its declared defaults) — the
    // SAME program-level pass classify runs (desugar-before-both).
    crate::lower::fill_record_defaults(&mut ir);
    // Pure call-bearing GLOBAL inits inline at their use sites (the lazy-static value
    // semantics — see inline_pure_call_globals; shared with classify: desugar-before-both).
    crate::lower::inline_pure_call_globals(&mut ir);
    // #806 step 2: small pure-scalar fns inline as reduced expressions at their
    // call sites (shared with classify: desugar-before-both).
    crate::lower::inline_small_scalar_fns(&mut ir);
    // C-132 move-mode write-back: `mut` param fns return their mutated buffer and
    // call sites assign it back — the SAME rewrite the v0 wasm pipeline runs
    // (almide_ir::mut_param), applied pre-lowering so both v1 legs and the caps
    // counter see one tree. Rewritten fns drop `mutated_params` (the wall keys on
    // it); excluded shapes (multi-mut-param, same-name, non-Unit effect) keep it
    // and keep walling.
    almide_ir::mut_param::lower_mut_params_move_mode(&mut ir);
    // Guard → if restructure at the fn-body tail chain (conditional early return
    // expressed without early-return control flow — see desugar_guard.rs; shared
    // with classify: desugar-before-both).
    crate::lower::desugar_fn_body_guards(&mut ir);
    // Tail err-raise ifs normalize to the proven bind-position `!` shape (fed by the
    // guard restructure above; shared with classify: desugar-before-both).
    crate::lower::normalize_tail_err_raise_ifs(&mut ir);
    // Block call-arguments absorb their call (shared with classify: desugar-before-both).
    crate::lower::hoist_block_call_args(&mut ir);
    // Call-bearing assert subjects bind first (shared with classify: desugar-before-both;
    // must precede the never-err/auto-wrap classification so the bind rewraps like a
    // user-written `let`).
    crate::lower::hoist_assert_call_subjects(&mut ir);
    crate::lower::desugar_loop_early_returns(&mut ir);
    crate::lower::hoist_spread_call_bases(&mut ir);
    crate::lower::hoist_record_literal_args(&mut ir);
    // Debug aid: `ALMIDE_DUMP_IR=<substr>` dumps the post-chain body of matching fns.
    if let Ok(pat) = std::env::var("ALMIDE_DUMP_IR") {
        for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter())) {
            if f.name.as_str().contains(&pat) {
                crate::trace::trace("ALMIDE_DUMP_IR", || format!(
                    "=== ALMIDE_DUMP_IR {} ===\n{:#?}", f.name.as_str(), f.body));
            }
        }
    }
    Ok(ir)
}

/// Single-file convenience (no cross-module siblings) — the bundled-runtime / drop-source
/// re-lowering paths, which never carry `import self.*`.
fn source_to_ir(source: &str) -> Result<almide_ir::IrProgram, LowerError> {
    source_to_ir_with(source, &[])
}


/// `verbose` gates the honest per-function "outside the lowering subset" diagnostics to stderr.
///
/// Returns `Ok(wat)` when the WHOLE program lowers (every function in-subset, `main` present, no
/// unlinked call), else `Err(LowerError::Unsupported(..))` — a clean WALL the caller can fall back
/// from (v0 codegen). NEVER a wrong module: honest-wall.
/// Resolve the BUNDLED stdlib modules a single-file program needs — the
/// standalone-harness subset of the CLI resolver (`src/resolve.rs`): every
/// auto-import bundled module, plus every explicitly imported bundled module,
/// each with its bundled dependencies, depth-first in import order (the same
/// visit order `load_bundled_module` produces — deterministic, deduped).
/// Callers with a real resolver (the CLI) never need this; the wasmgen
/// harnesses do, so a fixture with `import path` renders instead of walling.
pub fn bundled_self_modules(source: &str) -> Vec<(String, almide_lang::ast::Program, bool)> {
    use std::collections::HashSet;
    fn add(
        name: &str,
        out: &mut Vec<(String, almide_lang::ast::Program, bool)>,
        seen: &mut HashSet<String>,
    ) {
        if seen.contains(name) {
            return;
        }
        let Some(src) = almide_lang::stdlib_info::bundled_source(name) else { return };
        let Some(prog) = almide_lang::parse_cached(src) else { return };
        let prog = prog.clone();
        seen.insert(name.to_string());
        for imp in &prog.imports {
            if let almide_lang::ast::Decl::Import { path, .. } = imp {
                if let Some(dep) = path.first() {
                    add(dep.as_str(), out, seen);
                }
            }
        }
        out.push((name.to_string(), prog, false));
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    // The top source is caller-owned (not 'static) — parse directly instead
    // of through the 'static-keyed AST cache.
    let tokens = almide_lang::lexer::Lexer::tokenize(source);
    let mut parser = almide_lang::parser::Parser::new(tokens);
    if let Ok(prog) = parser.parse() {
        for imp in &prog.imports {
            if let almide_lang::ast::Decl::Import { path, .. } = imp {
                if let Some(root) = path.first() {
                    add(root.as_str(), &mut out, &mut seen);
                }
            }
        }
    }
    for name in almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED {
        add(name, &mut out, &mut seen);
    }
    out
}

pub fn try_render_wasm_source(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
) -> Result<String, LowerError> {
    try_render_wasm_source_impl(source, self_modules, verbose, RenderMode::Run)
}

/// LIBRARY-mode variant for `almide build --target wasm` (#881): a module with
/// `pub fn` exports and NO `main` renders with a SYNTHESIZED empty `main`, so
/// `_start` runs the global-init chain and nothing else — the v0 export ABI
/// (`_start` + `memory` + one named export per public fn) that web hosts like
/// ceangal's runtime call. `almide run` keeps the wall: running a main-less
/// module natively is a compile error (rustc E0601), and the wasm leg must
/// fail the same way rather than silently succeeding at nothing.
pub fn try_render_wasm_source_library(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
) -> Result<String, LowerError> {
    try_render_wasm_source_impl(source, self_modules, verbose, RenderMode::Library)
}

/// TEST-mode variant for the `almide test` wasm harness: when the file has NO `main`,
/// its `test "…"` fns are promoted to ordinary effect fns (renamed `__almd_test_<i>` —
/// the raw names carry spaces/unicode no WAT identifier admits) and a runner `main` is
/// synthesized with v0's `__test_runner` protocol (`test: <name> ... ` / `ok` per test,
/// assert failure = controlled halt with a non-zero exit). A file WITH `main` renders
/// exactly like [`try_render_wasm_source`] — both legs run main only, the v0 protocol.
/// Programs with top-let globals WALL in test mode (v0 re-inits globals before EVERY
/// test; the v1 `_start` inits once — shipping that silently would leak one test's
/// mutations into the next).
pub fn try_render_wasm_source_tests(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
) -> Result<String, LowerError> {
    try_render_wasm_source_impl(source, self_modules, verbose, RenderMode::Tests)
}

/// How the caller intends to use the rendered module — decides main synthesis.
#[derive(Clone, Copy, PartialEq)]
enum RenderMode {
    /// `almide run` / the cross-target gates: the program must carry `main`.
    Run,
    /// `almide test`: test fns promoted, a runner `main` synthesized.
    Tests,
    /// `almide build`: a main-less module with `pub fn` exports gets an empty
    /// synthesized `main` (the v0 library ABI — #881).
    Library,
}

fn try_render_wasm_source_impl(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
    mode: RenderMode,
) -> Result<String, LowerError> {
    crate::charge_probe::reset_budget_used();
    // STRICT VALUE MODE spans the WHOLE render, not just the IR phase. `strict_values()`
    // is read by MIR *lowering*, which runs in `try_render_wasm_source_impl_rest` below —
    // so a guard scoped to `build_ir_with_drops` would be restored before the only code
    // that consults it ever runs, and every deferred `Op::Const` ZERO would render as an
    // executable 0 instead of walling. That is exactly what happened: the flag used to be
    // a process-global the IR phase `store(true)`d and never reset, so lowering inherited
    // strict mode by leak; converting it to a scoped guard silently moved the boundary and
    // re-opened the silently-wrong-value class F2 closed (`result.unwrap_or_else(err(…),
    // (_) => captured_float)` printed 0 on wasm against 100 on native — nightly fuzz
    // finding, seed 1785217538023450905). Own it here, at the entrypoint that spans both
    // phases, so the scope matches what the mode actually protects.
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let mut ir = build_ir_with_drops(source, self_modules, mode == RenderMode::Tests)?;
    if mode == RenderMode::Library {
        synthesize_library_main(&mut ir);
    }
    try_render_wasm_source_impl_rest(&mut ir, verbose)
}

/// LIBRARY mode (#881): a module with `pub fn` exports and no `main` gets an
/// EMPTY `fn main() -> Unit` so `_start` exists and runs only the global-init
/// chain — the v0 export-module ABI web hosts call (`_start`, `memory`, one
/// named export per public fn). A module with NEITHER a main NOR any public
/// fn is left alone: the honest "no main in the IR" wall downstream is the
/// right answer for a program with nothing to run and nothing to export.
fn synthesize_library_main(ir: &mut almide_ir::IrProgram) {
    if ir.functions.iter().any(|f| f.name.as_str() == "main") {
        return;
    }
    let has_exports = ir.functions.iter().any(|f| {
        !f.is_test
            && !f.generics.as_ref().map_or(false, |g| !g.is_empty())
            && matches!(f.visibility, almide_ir::IrVisibility::Public)
    });
    if !has_exports {
        return;
    }
    ir.functions.push(almide_ir::IrFunction {
        name: almide_lang::intern::sym("main"),
        params: vec![],
        ret_ty: almide_lang::types::Ty::Unit,
        body: almide_ir::IrExpr {
            kind: almide_ir::IrExprKind::Unit,
            ty: almide_lang::types::Ty::Unit,
            span: Default::default(),
            def_id: None,
        },
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: almide_ir::IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        module_origin: None,
        mutated_params: vec![],
    });
}

/// Phase 1: synthesize the recursive-drop / repr source text this program's linked
/// types need, splice it into the source, and re-lower (v1-trust-spine-only — v0
/// manages its own memory). In `test_mode`, promote `test "…"` fns to a synthesized
/// runner `main`. Returns the FINAL linked `IrProgram` the rest of the pipeline
/// (globals, layouts, MIR lowering) continues from.
fn build_ir_with_drops(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    test_mode: bool,
) -> Result<almide_ir::IrProgram, LowerError> {
    // STRICT VALUE MODE is owned by the caller, not by this phase. Nothing between here
    // and the return reads `strict_values()`: this builds the linked IR, and the mode
    // gates MIR *op* lowering, which runs after. A guard here would look like the
    // protection and be none.
    let ir = source_to_ir_with(source, self_modules)?;
    // ADT brick 5b: GENERATE the recursive-drop fns (`__drop_<T>`) for nested-variant types and
    // re-lower with them in scope. v1-trust-spine-only — v0 manages its own memory. Two-pass.
    let anon_recs = crate::lower::collect_recursive_anon_records(&ir);
    let mut all_type_decls = ir.type_decls.clone();
    for m in &ir.modules {
        all_type_decls.extend(m.type_decls.iter().cloned());
    }
    // A GENERIC user variant instantiated with concrete args as a `List[<...>]` LITERAL element
    // type (`List[Either[Int,String]]`) needs a SHADOW `type <inst>` + `$__drop_<inst>`/
    // `$__drop_list_<inst>` generated for THIS SPECIFIC instantiation — the raw declaration
    // (`Either[L,R]`) carries unresolved type-parameter placeholders the drop generator can't
    // classify (see `is_rich_variant_ty`'s doc comment, mod_p2.rs). Built from a PRE-relower
    // `VariantLayouts` (this program's OWN declared generics/cases, before the drops text is
    // appended) so discovery sees the ORIGINAL `Left(1)`/`Right("y")` list literal. The shadow
    // type declaration text is prepended to `drops` below; the shadow `IrTypeDecl` is spliced
    // into `all_type_decls` so the SAME `generate_variant_drop_sources` call already below
    // covers it too (no separate/duplicate drop-generation call).
    let pre_relower_variant_layouts = crate::lower::build_variant_layouts(&all_type_decls);
    let generic_variant_list_insts =
        crate::lower::discover_generic_variant_list_instantiations(&ir, &pre_relower_variant_layouts);
    let (generic_variant_type_decl_src, generic_variant_synthetic_decls) =
        crate::lower::generate_generic_variant_instantiation_type_decls(
            &generic_variant_list_insts,
            &pre_relower_variant_layouts,
        );
    // The SHADOW decls exist only to hang `$__drop_<inst>` on — they must NOT reach the
    // REPR generator: their synthetic ctor names (`C__Either_Int_String_0`) would render
    // into `${…}` output (native prints the REAL `Left(…)`), and their emitted
    // `__repr_<inst>` would collide with the instantiation-keyed repr of the same name
    // that the interp section generates from the REAL generic decl. Snapshot the
    // shadow-free list for the repr call below.
    let repr_type_decls = all_type_decls.clone();
    all_type_decls.extend(generic_variant_synthetic_decls);
    let uses_result_opt_str = crate::lower::program_uses_result_option_str(&ir);
    // First-class function values need the UNIFORM closure-block release
    // (`$__drop_closure` — self-describing recursive drop, DropVariant "closure").
    let closure_drop =
        if crate::lower::program_uses_closures(&ir) { crate::lower::CLOSURE_DROP_SRC } else { "" };
    // A `List[<Fn>]` LITERAL (`[(x)=>x+1, (x)=>x*2]`) routes its scope-end drop to the
    // generated `$__drop_list_closure` (per-element `$__drop_closure` — required, not a
    // blind rc_dec, since a captured heap slot would otherwise leak). Needs
    // `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` already guarantees
    // whenever a closure LIST exists (the list's elements are Lambda exprs).
    let list_closure_drop = if crate::lower::program_uses_closure_list(&ir) {
        crate::lower::LIST_CLOSURE_DROP_SRC
    } else {
        ""
    };
    // A `Map[String, <Fn>]` (the closure-valued map — mclo class) routes its scope-end
    // drop to `$__drop_map_mclo` (per-value `$__drop_closure` over the split layout).
    // Needs `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` guarantees
    // whenever a closure-valued map exists (its values are Fn-typed exprs).
    let map_mclo_drop = if crate::lower::program_uses_map_closure(&ir) {
        crate::lower::MAP_MCLO_DROP_SRC
    } else {
        ""
    };
    // A `List[(String, <Fn>)]` pairs literal (the closure-valued map's from_list
    // input) routes its scope-end drop to `$__drop_list_str_clo` (per-tuple: key
    // rc_dec + `$__drop_closure` on the value slot).
    let list_str_clo_drop = if crate::lower::program_uses_str_clo_pairs(&ir) {
        crate::lower::LIST_STR_CLO_DROP_SRC
    } else {
        ""
    };
    // An `Option[(String, String)]` (the if-merged `some((s1, s2))` ctor) routes
    // its scope-end drop to `$__drop_opt_str_str`.
    let opt_str_str_drop = if crate::lower::program_uses_opt_str_str(&ir) {
        crate::lower::OPT_STR_STR_DROP_SRC
    } else {
        ""
    };
    // A `List[Option/Result]` literal with owned-handle-slot elements routes its drop to the
    // generated `$__drop_list_lenlist` (the shared `lenlist_elem_class` decides both sides).
    let lenlist_drop = if crate::lower::program_uses_lenlist_elem_lists(&ir) {
        crate::lower::LENLIST_DROP_SRC
    } else {
        ""
    };
    // `__drop_list_str` (a `List[String]` record OR variant ctor field, OR a closure's
    // nested-heap capture — `CLOSURE_DROP_SRC`'s `__drop_closure_loop` unconditionally
    // references it once ANY closure exists, since a capture's concrete type isn't known
    // at this gate without re-running `lift_lambda`'s own free-vars scan; conservatively
    // widened on `program_uses_closures` rather than precisely detecting a List[String]
    // capture — always correct, occasionally includes an unused routine) — SHARED between
    // the record and variant drop generators, so it is emitted ONCE here rather than by
    // either generator inline (two independent copies would be a duplicate-fn compile
    // error).
    let list_str_drop = if crate::lower::program_uses_list_str_drop_field(&all_type_decls)
        || crate::lower::program_uses_anon_list_str_record(&ir, &all_type_decls)
        || crate::lower::program_uses_closures(&ir)
    {
        crate::lower::LIST_STR_DROP_SRC
    } else {
        ""
    };
    // `Result[List[Int], List[String]]` (result.collect) routes its drop to the
    // TAG-AWARE `$__drop_res_ilsl` (Err → recursive string free; Ok → flat).
    let res_ilsl_drop = if crate::lower::program_uses_res_intlist_strlist(&ir) {
        crate::lower::RES_ILSL_DROP_SRC
    } else {
        ""
    };
    // An `Option[(String, <scalar>)]` (map.find's result, or a plain `some((s, n))` ctor)
    // routes its drop to the TAG-AWARE `$__drop_opt_str_int` (Some → recursive String-slot
    // free; None → nothing) — a blind flat `rc_dec` of the Option's payload slot would only
    // free the TUPLE's own refcount, leaking its String. Type-driven gate (#840): the old
    // `map.find` name-heuristic missed the literal-ctor producer and left the routed call
    // dangling in the WAT.
    let opt_str_int_drop = if crate::lower::program_uses_opt_str_scalar(&ir) {
        crate::lower::OPT_STR_INT_DROP_SRC
    } else {
        ""
    };
    let drops = format!(
        "{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
        generic_variant_type_decl_src,
        crate::lower::generate_variant_drop_sources(&all_type_decls),
        crate::lower::generate_record_drop_sources(&all_type_decls, &anon_recs, uses_result_opt_str),
        crate::lower::generate_variant_repr_sources(&repr_type_decls, &crate::lower::collect_interp_anon_records(&ir), &crate::lower::collect_interp_repr_containers(&ir)),
        crate::lower::generate_krec_sources(&ir, &all_type_decls),
        closure_drop,
        res_ilsl_drop,
        lenlist_drop,
        list_str_drop,
        list_closure_drop,
        map_mclo_drop,
        list_str_clo_drop,
        opt_str_str_drop,
        opt_str_int_drop,
    );
    // The generated drops free a `Value` field via value_core's INTERNAL `__drop_value` — bring
    // value_core's source into scope for the re-lower's type check; the auto-link dedups it.
    let needs_value_core = drops.contains("__drop_value")
        || drops.contains("__drop_list_value")
        // A generated repr calls value_core's JSON serializer by its IMPL name
        // (`value_stringify` — the `${Value}` / Value-field C-060 reprs).
        || drops.contains("value_stringify");
    let value_core_src: &str = if needs_value_core {
        include_str!("../../../stdlib/value_core.almd")
    } else {
        ""
    };
    crate::trace::trace("ALMIDE_DUMP_DROPS", || format!("=== ALMIDE_DUMP_DROPS ===\n{drops}\n=== end ==="));
    let mut ir = if drops.trim().is_empty() {
        ir
    } else {
        source_to_ir_with(&format!("{source}\n{value_core_src}\n{drops}"), self_modules)?
    };
    if test_mode {
        synthesize_test_runner_main(&mut ir)?;
    }
    // #881: module-level top-let ids are per-region — make them globally
    // unique BEFORE any layout/slot phase keys a map by the raw id (the
    // globals union and the mutable-slot map both key raw ids).
    disambiguate_module_global_regions(&mut ir);
    Ok(ir)
}

include!("pipeline_test_runner.rs");
include!("pipeline_global_slots.rs");
include!("pipeline_b.rs");
include!("pipeline_c.rs");
