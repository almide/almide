use crate::{parse_file, canonicalize, codegen, check, diagnostic, resolve, project, project_fetch, out, out_no_nl, err};

/// Print parse errors (if any) and exit. Extracted verbatim from `cmd_emit`'s
/// leading parse-error gate — a pure diagnostics-formatting step with no
/// shared mutable state.
fn exit_on_parse_errors(parse_errors: &[diagnostic::Diagnostic], source_text: &str) {
    if !parse_errors.is_empty() {
        for e in parse_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(e, source_text)));
        }
        err(&format!("\n{} parse error(s) found", parse_errors.len()));
        std::process::exit(1);
    }
}

/// Run the checker (when required) and report/exit on type errors, returning
/// the checker for later IR lowering. Extracted verbatim from `cmd_emit`'s
/// checker-setup block — reads only its parameters, exits the process
/// exactly where the original code did.
fn run_checker_for_emit(
    file: &str,
    source_text: &str,
    program: &mut almide::ast::Program,
    resolved: &resolve::ResolvedModules,
    run_check: bool,
) -> Option<check::Checker> {
    if !run_check {
        return None;
    }
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    let errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    if !errors.is_empty() {
        for d in &errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
        err(&format!("\n{} error(s) found", errors.len()));
        std::process::exit(1);
    }
    for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
        err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
    }
    Some(checker)
}

pub fn cmd_emit(file: &str, target: &str, emit_ast: bool, emit_ir: bool, emit_dialect: bool, no_check: bool, repr_c: bool) {
    let (mut program, source_text, parse_errors) = parse_file(file);
    exit_on_parse_errors(&parse_errors, &source_text);

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); })
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });

    // Run checker if needed (always for emit_ir, otherwise when !no_check && !emit_ast)
    let run_check = emit_ir || emit_dialect || (!no_check && !emit_ast);
    let mut checker_opt: Option<check::Checker> = run_checker_for_emit(file, &source_text, &mut program, &resolved, run_check);

    // Pre-register versioned names before root lowering
    if let Some(checker) = &mut checker_opt {
        crate::register_versioned_module_names(checker, &resolved.modules);
    }
    // Lower to IR if checker ran
    let mut ir_program = checker_opt.as_ref().map(|checker| {
        almide::lower::lower_program(&program, &checker.env, &checker.type_map)
    });
    let mut module_irs = std::collections::HashMap::new();
    if let Some(checker) = &mut checker_opt {
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            crate::lower_one_user_module(checker, name, mod_prog, pkg_id, &mut module_irs, &mut ir_program);
        }
    }

    // Monomorphize row-polymorphic functions
    if let Some(ref mut ir) = ir_program {
        almide::mono::monomorphize(ir);
    }

    if emit_dialect {
        let ir = ir_program.as_ref().expect("checker must have run for emit_dialect");
        let module = almide_dialect::lower::lower_program(ir);
        let errors = almide_dialect::verify::verify_module(&module);
        if !errors.is_empty() {
            for e in &errors {
                err(&format!("dialect verify: {} (in {})", e.message, e.context));
            }
        }
        if target == "rust" || target == "rs" {
            out_no_nl(&format!("{}", almide_dialect::emit_rust::emit_module(&module)));
        } else {
            out_no_nl(&format!("{}", almide_dialect::dump::dump_module(&module)));
        }
        return;
    }
    if emit_ir {
        let ir = ir_program.expect("checker must have run for emit_ir");
        let json = serde_json::to_string_pretty(&ir)
            .unwrap_or_else(|e| { err(&format!("JSON serialize error: {}", e)); std::process::exit(1); });
        out(&format!("{}", json));
    } else if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { err(&format!("JSON serialize error: {}", e)); std::process::exit(1); });
        out(&format!("{}", json));
    } else {
        let ir = ir_program.as_mut().expect("IR required for codegen");
        almide::ir_link::ir_link(ir);
        {
            let t = match target {
                "rust" | "rs" => codegen::pass::Target::Rust,
                "wgsl" => codegen::pass::Target::Wgsl,
                other => { err(&format!("Unknown target: {}. Use rust, wgsl.", other)); std::process::exit(1); }
            };
            let opts = codegen::CodegenOptions { repr_c, allow_unverified: false };
            match codegen::codegen_with(ir, t, &opts) {
                codegen::CodegenOutput::Source(code) => out_no_nl(&format!("{}", code)),
                codegen::CodegenOutput::Binary(_) => unreachable!(),
            }
        }
    }
}
