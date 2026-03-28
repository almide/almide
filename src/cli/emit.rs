use crate::{parse_file, codegen, check, diagnostic, resolve, project, project_fetch};

pub fn cmd_emit(file: &str, target: &str, emit_ast: bool, emit_ir: bool, no_check: bool) {
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

    // Extract user-level import aliases (import pkg as alias, or implicit aliases for multi-segment imports)
    let import_aliases: Vec<(String, String)> = program.imports.iter().filter_map(|imp| {
        if let crate::ast::Decl::Import { path, alias, .. } = imp {
            if let Some(a) = alias {
                // For self-imports, the target is the canonical module name (last segment or package name),
                // not the dotted path, because resolved.modules stores canonical names
                let is_self_import = path.first().map(|s| s.as_str()) == Some("self");
                let target = if is_self_import && path.len() >= 2 {
                    path.last().map(|s| s.to_string()).unwrap_or_default()
                } else if is_self_import {
                    // import self as alias → target is the package name (loaded from resolved modules)
                    resolved.modules.iter()
                        .find(|(_, _, _, is_self)| *is_self)
                        .map(|(name, _, _, _)| name.clone())
                        .unwrap_or_else(|| path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
                } else {
                    path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")
                };
                Some((a.to_string(), target))
            } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                let last = path.last().expect("path.len() > 1 checked above").to_string();
                Some((last, path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")))
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    // Run checker if needed (always for emit_ir, otherwise when !no_check && !emit_ast)
    let run_check = emit_ir || (!no_check && !emit_ast);
    let mut checker_opt: Option<check::Checker> = None;
    if run_check {
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        for (alias, target) in &import_aliases {
            checker.register_alias(alias, target);
        }
        let diagnostics = checker.check_program(&mut program);
        let errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        if !errors.is_empty() {
            for d in &errors {
                eprintln!("{}", d.display_with_source(&source_text));
            }
            eprintln!("\n{} error(s) found", errors.len());
            std::process::exit(1);
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        checker_opt = Some(checker);
    }

    // Lower to IR if checker ran
    let mut ir_program = checker_opt.as_ref().map(|checker| {
        almide::lower::lower_program(&program, &checker.expr_types, &checker.env)
    });
    let mut module_irs = std::collections::HashMap::new();
    if let Some(checker) = &mut checker_opt {
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let mod_types = checker.check_module_bodies(mod_prog);
            let versioned = pkg_id.as_ref().map(|pid| pid.mod_name());
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &mod_types, &checker.env, versioned);
            let mod_ir = almide::lower::lower_program(mod_prog, &mod_types, &checker.env);
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
        match codegen::codegen(ir, t) {
            codegen::CodegenOutput::Source(code) => print!("{}", code),
            codegen::CodegenOutput::Binary(_) => unreachable!(),
        }
    }
}
