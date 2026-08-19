//! Unit 4 stage 1: the REAL checker behind a per-file salsa query.
//!
//! `check_file_json` reproduces the incumbent's
//! `resolve_and_typecheck_for_check` (almide@a877d2138, src/cli/check.rs:46-88)
//! faithfully — resolve imports (bundled stdlib), canonicalize,
//! `Checker::from_env`, `refresh_module_toplets` (#785), `infer_program`,
//! then the #862 per-module inference loop — with two adaptations, recorded:
//! process::exit sites become returned output, and stderr rendering becomes
//! returned JSON lines (`almide_diag::render::to_json`, the `--json` shape).
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
fn infer_module_capturing(
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
    let mut program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            return CheckOutput { fatal: Some(format!("{e}")), diags: Vec::new(), module_diags: Vec::new() };
        }
    };
    let parse_errors = std::mem::take(&mut parser.errors);

    let mut resolved = match almide::resolve::resolve_imports_with_deps(&path, &program, &[]) {
        Ok(r) => r,
        Err(e) => {
            return CheckOutput { fatal: Some(format!("{e}")), diags: Vec::new(), module_diags: Vec::new() };
        }
    };

    let canon = almide::canonicalize::canonicalize_program(
        &mut program,
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
