//! Unit 3: execution via the ported reference interpreter (the executable
//! spec, L2 of ARCHITECTURE.md §2).
//!
//! `run_file` assembles the interpreter's canonical cut — the exact sequence
//! the crate's own eval_test pins (parse → canonicalize → check →
//! `lower_program` → `almide_driver::link_ir`, which owns the
//! optimize→mono→ir_link order) — but through the FULL resolve/canonicalize
//! path so fixtures with stdlib imports get the same env the checker parity
//! gates validated. Stdlib calls execute through the interpreter's
//! self-host-registry bridge, exactly as the incumbent's 3-way oracle runs.

pub struct RunResult {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_file(path: &str, source_text: &str) -> Result<RunResult, String> {
    let tokens = almide::lexer::Lexer::tokenize(source_text);
    let mut parser = almide::parser::Parser::new(tokens).with_file(path);
    let mut program = parser.parse().map_err(|e| format!("parse: {e}"))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse errors: {}", parser.errors.len()));
    }

    let resolved = almide::resolve::resolve_imports_with_deps(path, &program, &[])
        .map_err(|e| format!("resolve: {e}"))?;

    let canon = almide::canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = almide::check::Checker::from_env(canon.env);
    checker.set_source(path, source_text);
    checker.diagnostics = canon.diagnostics;
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);
    let n_errors = diagnostics.iter().filter(|d| d.level == almide::diagnostic::Level::Error).count();
    if n_errors > 0 {
        return Err(format!("type errors: {n_errors}"));
    }

    let mut ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
    almide_driver::link_ir(&mut ir);
    let out = almide::interp::Interpreter::new(&ir).run_main();
    // Surface the distinguished-outcome reason (Unsupported carries it in the
    // status, not in stderr) so harnesses can report skip classes precisely.
    let stderr = match &out.status {
        almide::interp::RunStatus::Unsupported(r) => r.clone(),
        _ => out.stderr.clone(),
    };
    Ok(RunResult { exit: out.exit_code(), stdout: out.stdout, stderr })
}
