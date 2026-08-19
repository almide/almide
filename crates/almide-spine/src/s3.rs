//! Unit 4 stage 1: the REAL checker behind a per-file salsa query.
//!
//! `check_file_json` reproduces the incumbent's `cmd_check_json`
//! (almide@a877d2138, src/cli/check.rs: parse_for_json → 
//! resolve_and_typecheck_for_check → JSON lines → IR lowering for
//! unused-variable warnings) faithfully. `diags` is exactly the oracle's
//! stdout line sequence: parse errors, checker diagnostics, then — when
//! there are no parse errors and no type errors — the unused-var warnings
//! from `lower_program` + `collect_unused_var_warnings`. Adaptations,
//! recorded: process::exit sites become returned output (`fatal` marks the
//! resolve/module exits whose stdout the oracle never reaches).
//!
//! PURITY CONTRACT: callers may only pass files whose imports are all
//! stdlib modules — local-module imports would make `resolve` read the file
//! system inside a query. The bench/parity harnesses prefilter on the parsed
//! import list (`stdlib::is_stdlib_module`); everything else is excluded
//! with a reason, never checked silently.
//!
//! `infer_module_capturing` below is a verbatim copy of
//! src/compile_driver.rs's helper (it cannot be imported: compile_driver
//! lives in the incumbent's CLI crate alongside codegen, which is not
//! ported yet). Diff-checked against the SHA at port time.

use crate::SourceFile;
use almide::diagnostic::Diagnostic;
use std::sync::atomic::{AtomicUsize, Ordering};

pub static FILE_CHECK_EXECUTIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CheckOutput {
    /// Fatal parse failure (parser returned Err) — mirrors the incumbent's
    /// immediate exit; message carried as the single entry.
    pub fatal: Option<String>,
    /// Entry-file diagnostics (parse errors first, then checker), as the
    /// `--json` wire lines.
    pub diags: Vec<String>,
    /// Imported-module error diagnostics (#862), as JSON lines with the
    /// module's own file attributed.
    pub module_diags: Vec<String>,
}

/// Verbatim copy of src/compile_driver.rs::infer_module_capturing
/// (almide@a877d2138) — see module doc for why it is copied, not imported.
pub(crate) fn infer_module_capturing(
    checker: &mut almide::check::Checker,
    name: &str,
    mod_prog: &mut almide::ast::Program,
    sources: &std::collections::HashMap<String, (String, String)>,
    out: &mut Vec<(String, String, Vec<Diagnostic>)>,
) {
    let Some((path, text)) = sources.get(name) else {
        // Bundled stdlib: compiled in and CI-gated, no user file to blame.
        checker.infer_module(mod_prog, name);
        return;
    };
    let saved_file = checker.source_file.clone();
    let saved_text = checker.source_text.clone();
    let before = checker.diagnostics.len();
    checker.set_source(path, text);
    checker.infer_module(mod_prog, name);
    let produced: Vec<Diagnostic> = checker.diagnostics[before..].to_vec();
    checker.source_file = saved_file;
    checker.source_text = saved_text;
    if !produced.is_empty() {
        out.push((path.clone(), text.clone(), produced));
    }
}

/// The full front end + check for one file, memoized per file.
#[salsa::tracked]
pub fn check_file_json(db: &dyn salsa::Database, file: SourceFile) -> CheckOutput {
    FILE_CHECK_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let path = file.path(db).clone();
    let source_text = file.text(db).clone();

    let tokens = almide::lexer::Lexer::tokenize(&source_text);
    let mut parser = almide::parser::Parser::new(tokens).with_file(&path);
    let parsed = parser.parse().ok();
    let parse_errors = std::mem::take(&mut parser.errors);
    let Some(mut program) = parsed else {
        // cmd_check_json's fatal-parse path: the accumulated parser errors
        // ARE the stdout (the Err value itself is discarded), exit 1.
        let diags = parse_errors.iter().map(almide::diagnostic_render::to_json).collect();
        return CheckOutput { fatal: None, diags, module_diags: Vec::new() };
    };

    let mut resolved = match almide::resolve::resolve_imports_with_deps(&path, &program, &[]) {
        Ok(r) => r,
        Err(e) => {
            return CheckOutput { fatal: Some(e), diags: Vec::new(), module_diags: Vec::new() };
        }
    };

    let canon = almide::canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = almide::check::Checker::from_env(canon.env);
    checker.set_source(&path, &source_text);
    checker.diagnostics = canon.diagnostics;
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);

    // #862 loop, faithful incl. the stdlib-vs-bundled skip and self-module
    // name switching for dependency packages.
    let sources = std::mem::take(&mut resolved.sources);
    let mut module_diags = Vec::new();
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) {
            continue;
        }
        let saved_self = checker.env.self_module_name;
        if let Some(pid) = pkg_id.as_ref() {
            checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
        }
        infer_module_capturing(&mut checker, name, mod_prog, &sources, &mut module_diags);
        checker.env.self_module_name = saved_self;
    }

    let mut diags: Vec<String> = Vec::new();
    for d in parse_errors.iter().chain(diagnostics.iter()) {
        diags.push(almide::diagnostic_render::to_json(d));
    }
    let has_type_errors = diagnostics.iter().any(|d| d.level == almide::diagnostic::Level::Error);
    if parse_errors.is_empty() && !has_type_errors {
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        for d in &almide::ir::collect_unused_var_warnings(&ir, &path) {
            diags.push(almide::diagnostic_render::to_json(d));
        }
    }
    let mut module_lines = Vec::new();
    for (mpath, _msrc, ds) in &module_diags {
        for d in ds {
            let mut d = d.clone();
            if d.file.is_none() {
                d.file = Some(mpath.clone());
            }
            module_lines.push(almide::diagnostic_render::to_json(&d));
        }
    }
    CheckOutput { fatal: None, diags, module_diags: module_lines }
}

/// Prefilter for the purity contract: true iff every explicit import names a
/// stdlib module (bundled or native), so `resolve` will not touch the file
/// system.
pub fn stdlib_only(program: &almide::ast::Program) -> bool {
    program.imports.iter().all(|d| {
        if let almide::ast::Decl::Import { path, .. } = d {
            path.len() == 1 && almide::stdlib::is_stdlib_module(path[0].as_str())
        } else {
            true
        }
    })
}

/// Stage 2, step 1: the #862 stdlib-module loop re-infers every bundled
/// stdlib module on every check (63% of per-file cost, measured by s4_probe)
/// and, for a stdlib-only entry, produces NO output: bundled modules carry
/// no user file to blame (the incumbent's own words — "compiled in and
/// CI-gated"), so `infer_module_capturing` pushes nothing, and any checker
/// mutations it makes come AFTER the entry's diagnostics are already taken.
/// The one remaining coupling candidate is the unused-var pass reading
/// `env`/`type_map` after the loop — which is exactly what the 1,062-file
/// parity manifest adjudicates. This query is v1 minus that loop; adopting
/// it requires the full parity gate to stay green.
#[salsa::tracked]
pub fn check_file_json_v2(db: &dyn salsa::Database, file: SourceFile) -> CheckOutput {
    FILE_CHECK_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let path = file.path(db).clone();
    let source_text = file.text(db).clone();

    let tokens = almide::lexer::Lexer::tokenize(&source_text);
    let mut parser = almide::parser::Parser::new(tokens).with_file(&path);
    let parsed = parser.parse().ok();
    let parse_errors = std::mem::take(&mut parser.errors);
    let Some(mut program) = parsed else {
        let diags = parse_errors.iter().map(almide::diagnostic_render::to_json).collect();
        return CheckOutput { fatal: None, diags, module_diags: Vec::new() };
    };

    let resolved = match almide::resolve::resolve_imports_with_deps(&path, &program, &[]) {
        Ok(r) => r,
        Err(e) => {
            return CheckOutput { fatal: Some(e), diags: Vec::new(), module_diags: Vec::new() };
        }
    };

    let canon = almide::canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = almide::check::Checker::from_env(canon.env);
    checker.set_source(&path, &source_text);
    checker.diagnostics = canon.diagnostics;
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);

    let mut diags: Vec<String> = Vec::new();
    for d in parse_errors.iter().chain(diagnostics.iter()) {
        diags.push(almide::diagnostic_render::to_json(d));
    }
    let has_type_errors = diagnostics.iter().any(|d| d.level == almide::diagnostic::Level::Error);
    if parse_errors.is_empty() && !has_type_errors {
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        for d in &almide::ir::collect_unused_var_warnings(&ir, &path) {
            diags.push(almide::diagnostic_render::to_json(d));
        }
    }
    CheckOutput { fatal: None, diags, module_diags: Vec::new() }
}

/// Stage 2, step 2: per-import-set env template. The module half of
/// canonicalization plus `Checker::from_env` + `refresh_module_toplets`
/// depends only on the resolved module set (constant bundled sources), so it
/// is computed once per distinct module list and CLONED per file check —
/// removing the second tax layer (canon 25.5% of per-file cost, s4_probe).
/// Cache legitimacy: a pure function of embedded constants + the module
/// list; the salsa dependency edges still flow through the file's text via
/// resolve. Byte-equivalence adjudicated by the oracle parity gate.
/// One cached env template: the module-half checker plus its diagnostics.
type EnvTemplate = std::sync::Arc<(almide::check::Checker, Vec<Diagnostic>)>;
type TemplateMap = std::collections::HashMap<String, EnvTemplate>;
static TEMPLATE_CACHE: std::sync::Mutex<Option<TemplateMap>> = std::sync::Mutex::new(None);

#[salsa::tracked]
pub fn check_file_json_v3(db: &dyn salsa::Database, file: SourceFile) -> CheckOutput {
    FILE_CHECK_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
    let path = file.path(db).clone();
    let source_text = file.text(db).clone();

    let tokens = almide::lexer::Lexer::tokenize(&source_text);
    let mut parser = almide::parser::Parser::new(tokens).with_file(&path);
    let parsed = parser.parse().ok();
    let parse_errors = std::mem::take(&mut parser.errors);
    let Some(mut program) = parsed else {
        let diags = parse_errors.iter().map(almide::diagnostic_render::to_json).collect();
        return CheckOutput { fatal: None, diags, module_diags: Vec::new() };
    };

    let resolved = match almide::resolve::resolve_imports_with_deps(&path, &program, &[]) {
        Ok(r) => r,
        Err(e) => {
            return CheckOutput { fatal: Some(e), diags: Vec::new(), module_diags: Vec::new() };
        }
    };

    let key: String = resolved.modules.iter().map(|(n, _, _, _)| n.as_str()).collect::<Vec<_>>().join(",");
    let template = {
        let mut guard = TEMPLATE_CACHE.lock().expect("template cache mutex poisoned");
        let map = guard.get_or_insert_with(Default::default);
        if let Some(t) = map.get(&key) {
            t.clone()
        } else {
            let canon = almide::canonicalize::canonicalize_modules_env(
                resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
            );
            let mut checker = almide::check::Checker::from_env(canon.env);
            almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
            let t = std::sync::Arc::new((checker, canon.diagnostics));
            map.insert(key.clone(), t.clone());
            t
        }
    };

    let mut checker = template.0.clone();
    let mut canon_diags = template.1.clone();
    almide::canonicalize::canonicalize_entry_onto(&mut checker.env, &mut canon_diags, &program);
    checker.set_source(&path, &source_text);
    checker.diagnostics = canon_diags;
    let diagnostics = checker.infer_program(&mut program);

    let mut diags: Vec<String> = Vec::new();
    for d in parse_errors.iter().chain(diagnostics.iter()) {
        diags.push(almide::diagnostic_render::to_json(d));
    }
    let has_type_errors = diagnostics.iter().any(|d| d.level == almide::diagnostic::Level::Error);
    if parse_errors.is_empty() && !has_type_errors {
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        for d in &almide::ir::collect_unused_var_warnings(&ir, &path) {
            diags.push(almide::diagnostic_render::to_json(d));
        }
    }
    CheckOutput { fatal: None, diags, module_diags: Vec::new() }
}
