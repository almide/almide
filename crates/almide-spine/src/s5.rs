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

/// Front half of `run_file`: parse → check → lower → link, returning the
/// linked IR (unit 6's emission gate shares this exact pipeline so the
/// interpreter and the wasm backend judge the SAME IR).
pub fn lower_to_ir(path: &str, source_text: &str) -> Result<almide::ir::IrProgram, String> {
    let tokens = almide::lexer::Lexer::tokenize(source_text);
    let mut parser = almide::parser::Parser::new(tokens).with_file(path);
    let mut program = parser.parse().map_err(|e| format!("parse: {e}"))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse errors: {}", parser.errors.len()));
    }

    let mut resolved = almide::resolve::resolve_imports_with_deps(path, &program, &[])
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
    // Lower every resolved module into the program before linking — the
    // incumbent's lower_one_user_module loop (src/compile_driver.rs:172-222,
    // essential steps replicated with attribution; pkg versioning is inert
    // for stdlib-only entries). Without this, calls into bundled PURE-Almide
    // modules reach the interpreter as unresolved bridge lookups.
    let sources = std::mem::take(&mut resolved.sources);
    let mut module_diags = Vec::new();
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) {
            continue;
        }
        // Bridge-vs-link boundary: a module containing ANY bodyless decl
        // (`= _`) is a self-host SURFACE — its implementations live behind
        // the interpreter's registry bridge, and lowering the surface would
        // shadow the bridge with garbage stubs (found by probe: string.slice
        // inside a linked stub returned the codepoint). Only fully
        // self-contained modules (every fn has a real body — url, html) are
        // lowered and linked; everything else stays bridge-resolved.
        let has_bodyless = mod_prog.decls.iter().any(|d| matches!(d, almide::ast::Decl::Fn { body: None, .. }));
        if has_bodyless {
            continue;
        }
        let saved_self = checker.env.self_module_name;
        if let Some(pid) = pkg_id.as_ref() {
            checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
        }
        crate::s3::infer_module_capturing(&mut checker, name, mod_prog, &sources, &mut module_diags);
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name);
        let (mod_table, _) = almide::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mod_ir_module = almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, None);
        checker.env.import_table = saved_table;
        checker.env.self_module_name = saved_self;
        ir.modules.push(mod_ir_module);
    }
    almide_driver::link_ir(&mut ir);
    Ok(ir)
}

pub fn run_file(path: &str, source_text: &str) -> Result<RunResult, String> {
    let ir = lower_to_ir(path, source_text)?;
    let out = almide::interp::Interpreter::new(&ir).run_main();
    // Surface the distinguished-outcome reason (Unsupported carries it in the
    // status, not in stderr) so harnesses can report skip classes precisely.
    let stderr = match &out.status {
        almide::interp::RunStatus::Unsupported(r) => r.clone(),
        _ => out.stderr.clone(),
    };
    Ok(RunResult { exit: out.exit_code(), stdout: out.stdout, stderr })
}
