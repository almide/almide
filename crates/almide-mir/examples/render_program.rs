//! Render a REAL `.almd` program to a COMPLETE wasm module via the v1 MIR renderer
//! (`almide_mir::pipeline::try_render_wasm_source`) — the EXECUTION-side counterpart to
//! emit_cert_from_source (the verification side). Goal: a real program runs through the v1
//! pipeline and matches v0 byte-identical — ③ execution parity, the path to v0 replacement
//! (docs/roadmap/active/v1-kgi-kpi.md, Gap 3). Functions outside the MIR-lowering subset are
//! reported to stderr (the honest boundary), the rest rendered.
//!
//! The v1 pipeline itself lives in the LIBRARY (`almide_mir::pipeline`) so the `almide` CLI can
//! drive it (opt-in `--verified` codegen). This example is the thin driver around it: resolve the
//! input file's `import self.*` siblings (needs the `almide` crate — a dev-dependency) and print.
//!
//!   render_program <file.almd>   → emits the wat module to stdout

use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;

fn die(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// The cached dep source dirs for the project owning `path` — walk up to its `almide.toml`, parse
/// it, and `fetch_all_deps` (cache-hit ⇒ fast, no network; the SAME computation the `almide` driver
/// runs). Empty when there is no project, no deps, or a fetch failure (graceful).
fn dep_paths_for(path: &str) -> Vec<(almide::project::PkgId, std::path::PathBuf)> {
    let mut dir = std::path::Path::new(path).parent();
    while let Some(d) = dir {
        let toml = d.join("almide.toml");
        if toml.exists() {
            if let Ok(proj) = almide::project::parse_toml(&toml) {
                if let Ok(deps) = almide::project_fetch::fetch_all_deps(&proj) {
                    return deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect();
                }
            }
            return Vec::new();
        }
        dir = d.parent();
    }
    Vec::new()
}

/// Discover the input file's SIBLING `src/*.almd` modules so `import self.<submodule>` resolves
/// exactly as under `almide run`/`almide check` — reusing the CANONICAL driver discovery
/// (`almide::resolve`). A NON-cross-module file yields an empty set (byte-identical single-file
/// behavior). Resolution failure falls back to single-file mode (cross-module fns then wall
/// honestly), never aborting.
fn discover_self_modules(
    path: &str,
    prog: &almide_lang::ast::Program,
) -> Vec<(String, almide_lang::ast::Program, bool)> {
    let needs_resolve = prog.imports.iter().any(|d| {
        matches!(d, almide_lang::ast::Decl::Import { path, .. }
            if path.first().map(|s| {
                let s = s.as_str();
                s == "self" || !almide_lang::stdlib_info::is_stdlib_module(s)
            }).unwrap_or(false))
    });
    if !needs_resolve {
        return Vec::new();
    }
    let deps = dep_paths_for(path);
    match almide::resolve::resolve_imports_with_deps(path, prog, &deps) {
        Ok(resolved) => resolved
            .modules
            .into_iter()
            .map(|(name, p, _pkg, is_self)| (name, p, is_self))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--tests` renders through the TEST-mode pipeline (synthesized runner main for a
    // no-main test file) — the `almide test` wasm harness's entry.
    let test_mode = args.iter().any(|a| a == "--tests");
    args.retain(|a| a != "--tests");
    let path = args
        .into_iter()
        .next()
        .unwrap_or_else(|| die("usage: render_program [--tests] <file.almd>".into()));
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| die(format!("cannot read {path}: {e}")));
    // Resolve the input file's `import self.<submodule>` siblings (canonical driver discovery).
    let probe_tokens = Lexer::tokenize(&source);
    let probe_prog = Parser::new(probe_tokens)
        .parse()
        .unwrap_or_else(|e| die(format!("parse error: {e:?}")));
    let self_modules = discover_self_modules(&path, &probe_prog);
    let rendered = if test_mode {
        almide_mir::pipeline::try_render_wasm_source_tests(&source, &self_modules, true)
    } else {
        almide_mir::pipeline::try_render_wasm_source(&source, &self_modules, true)
    };
    match rendered {
        Ok(wat) => print!("{wat}"),
        Err(e) => die(format!("[render_program] {e:?}")),
    }
}
