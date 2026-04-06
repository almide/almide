use crate::{parse_file, canonicalize, codegen, check, diagnostic, resolve, project, project_fetch};

pub fn cmd_emit(file: &str, target: &str, emit_ast: bool, emit_ir: bool, no_check: bool, repr_c: bool) {
    let (mut program, source_text, _parse_errors) = parse_file(file);

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
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
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    // Run checker if needed (always for emit_ir, otherwise when !no_check && !emit_ast)
    let run_check = emit_ir || (!no_check && !emit_ast);
    let mut checker_opt: Option<check::Checker> = None;
    if run_check {
        let canon = canonicalize::canonicalize_program(
            &program,
            resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
        );
        let mut checker = check::Checker::from_env(canon.env);
        checker.set_source(file, &source_text);
        checker.diagnostics = canon.diagnostics;
        let diagnostics = checker.infer_program(&mut program);
        let errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        if !errors.is_empty() {
            for d in &errors {
                eprintln!("{}", crate::diagnostic_render::display_with_source(d, &source_text));
            }
            eprintln!("\n{} error(s) found", errors.len());
            std::process::exit(1);
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", crate::diagnostic_render::display_with_source(d, &source_text));
        }
        checker_opt = Some(checker);
    }

    // Lower to IR if checker ran
    let mut ir_program = checker_opt.as_ref().map(|checker| {
        almide::lower::lower_program(&program, &checker.env, &checker.type_map)
    });
    let mut module_irs = std::collections::HashMap::new();
    if let Some(checker) = &mut checker_opt {
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let saved_self = checker.env.self_module_name;
            if let Some(pid) = pkg_id.as_ref() {
                checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
            }
            checker.infer_module(mod_prog, name);
            let versioned = pkg_id.as_ref().map(|pid| {
                let base = pid.mod_name();
                if let Some(suffix) = name.strip_prefix(&pid.name) {
                    format!("{}{}", base, suffix)
                } else {
                    base
                }
            });
            let self_name = checker.env.self_module_name.map(|s| s.to_string());
            let import_table_name = self_name.as_deref().unwrap_or(name);
            let (mod_table, _) = almide::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
            let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
            let mod_ir = almide::lower::lower_program(mod_prog, &checker.env, &checker.type_map);
            checker.env.import_table = saved_table;
            checker.env.self_module_name = saved_self;
            module_irs.insert(name.clone(), mod_ir);
            if let Some(ref mut ir) = ir_program {
                ir.modules.push(mod_ir_module);
            }
        }
    }

    // Monomorphize row-polymorphic functions
    if let Some(ref mut ir) = ir_program {
        almide::mono::monomorphize(ir);
    }

    if emit_ir {
        let ir = ir_program.expect("checker must have run for emit_ir");
        let json = serde_json::to_string_pretty(&ir)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else {
        let t = match target {
            "rust" | "rs" => codegen::pass::Target::Rust,
            "ts" | "typescript" => codegen::pass::Target::TypeScript,
            other => { eprintln!("Unknown target: {}. Use rust, ts.", other); std::process::exit(1); }
        };
        let ir = ir_program.as_mut().expect("IR required for codegen");
        let opts = codegen::CodegenOptions { repr_c };
        match codegen::codegen_with(ir, t, &opts) {
            codegen::CodegenOutput::Source(code) => print!("{}", code),
            codegen::CodegenOutput::Binary(_) => unreachable!(),
        }
    }
}
