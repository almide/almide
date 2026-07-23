use crate::{parse_file, canonicalize, codegen, check, diagnostic, resolve, out, out_no_nl, err};

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

/// Flags for [`cmd_emit`] — bundled into one struct (was 7 positional
/// params, a max-params violation) so the function signature stays under
/// the params threshold. Field names match `Commands::Emit`'s clap fields
/// 1:1, so the call site in `main.rs` builds it directly from the
/// destructured match arm.
pub struct EmitArgs<'a> {
    pub file: &'a str,
    pub target: &'a str,
    pub emit_ast: bool,
    pub emit_ir: bool,
    pub emit_dialect: bool,
    pub no_check: bool,
    pub repr_c: bool,
}

/// `cmd_emit`'s `--dialect` output path. Extracted verbatim.
fn emit_dialect_output(ir_program: &Option<almide::ir::IrProgram>, target: &str) {
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
}

/// `cmd_emit`'s `--emit-ir` output path. Extracted verbatim.
fn emit_ir_output(ir_program: Option<almide::ir::IrProgram>) {
    let ir = ir_program.expect("checker must have run for emit_ir");
    let json = serde_json::to_string_pretty(&ir)
        .unwrap_or_else(|e| { err(&format!("JSON serialize error: {}", e)); std::process::exit(1); });
    out(&format!("{}", json));
}

/// `cmd_emit`'s `--emit-ast` output path. Extracted verbatim.
fn emit_ast_output(program: &almide::ast::Program) {
    let json = serde_json::to_string_pretty(program)
        .unwrap_or_else(|e| { err(&format!("JSON serialize error: {}", e)); std::process::exit(1); });
    out(&format!("{}", json));
}

/// `cmd_emit`'s default codegen output path (Rust/WGSL source). Extracted
/// verbatim.
fn emit_codegen_output(ir_program: &mut Option<almide::ir::IrProgram>, target: &str, repr_c: bool) {
    let ir = ir_program.as_mut().expect("IR required for codegen");
    almide::ir_link::ir_link(ir);
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

pub fn cmd_emit(args: EmitArgs) {
    let EmitArgs { file, target, emit_ast, emit_ir, emit_dialect, no_check, repr_c } = args;
    let (mut program, source_text, parse_errors) = parse_file(file);
    exit_on_parse_errors(&parse_errors, &source_text);

    let dep_paths = super::dep_paths_from_cwd_toml();

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
        // Same per-module type-check + lower steps as try_compile_with_ir's
        // module loop in main.rs — shared via lower_one_user_module so the
        // two drivers can't silently drift apart.
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            crate::lower_one_user_module(checker, name, mod_prog, pkg_id, &mut module_irs, &mut ir_program);
        }
    }

    // Monomorphize row-polymorphic functions
    if let Some(ref mut ir) = ir_program {
        almide::mono::monomorphize(ir);
    }

    if emit_dialect {
        emit_dialect_output(&ir_program, target);
        return;
    }
    if emit_ir {
        emit_ir_output(ir_program);
        return;
    }
    if emit_ast {
        emit_ast_output(&program);
        return;
    }
    emit_codegen_output(&mut ir_program, target, repr_c);
}
