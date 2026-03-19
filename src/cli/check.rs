use crate::{parse_file, check as check_mod, diagnostic, resolve, project, project_fetch};

pub fn cmd_check(file: &str, deny_warnings: bool) {
    let (mut program, source_text, parse_errors) = parse_file(file);

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

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    let mut checker = check_mod::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);

    // Lower to IR for unused variable analysis (only if no parse errors)
    let unused_warnings = if parse_errors.is_empty() {
        let ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
        almide::ir::collect_unused_var_warnings(&ir, file)
    } else {
        Vec::new()
    };

    let mut warnings: Vec<&diagnostic::Diagnostic> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Warning)
        .collect();
    for d in &unused_warnings {
        warnings.push(d);
    }
    for d in &warnings {
        eprintln!("{}", d.display_with_source(&source_text));
    }

    // Combine parse errors + checker errors
    let mut all_errors: Vec<&diagnostic::Diagnostic> = parse_errors.iter().collect();
    let checker_errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    all_errors.extend(checker_errors);
    if deny_warnings && !warnings.is_empty() {
        // Treat warnings as errors
        for d in &all_errors {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        let total = all_errors.len() + warnings.len();
        eprintln!("\n{} error(s) found (--deny-warnings: {} warning(s) treated as errors)", total, warnings.len());
        std::process::exit(1);
    }
    if !all_errors.is_empty() {
        for d in &all_errors {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        eprintln!("\n{} error(s) found", all_errors.len());
        std::process::exit(1);
    }

    // Security Layer 2: check permissions if defined in almide.toml
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            if !proj.permissions.is_empty() {
                let ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
                let mut ir_mut = ir;
                use almide::codegen::pass_effect_inference::{EffectInferencePass, Effect};
                use almide::codegen::pass::NanoPass;
                EffectInferencePass.run(&mut ir_mut, almide::codegen::pass::Target::Rust);

                let allowed: std::collections::HashSet<Effect> = proj.permissions.iter()
                    .filter_map(|s| match s.as_str() {
                        "IO" => Some(Effect::IO),
                        "Net" => Some(Effect::Net),
                        "Env" => Some(Effect::Env),
                        "Time" => Some(Effect::Time),
                        "Rand" => Some(Effect::Rand),
                        "Fan" => Some(Effect::Fan),
                        "Log" => Some(Effect::Log),
                        _ => None,
                    })
                    .collect();

                let mut violations = 0;
                for (name, fe) in &ir_mut.effect_map.functions {
                    let forbidden: Vec<_> = fe.transitive.iter()
                        .filter(|e| !allowed.contains(e))
                        .collect();
                    if !forbidden.is_empty() {
                        eprintln!("error: capability violation in `{}`", name);
                        for e in &forbidden {
                            eprintln!("  {} is not in [permissions].allow", e);
                        }
                        violations += 1;
                    }
                }
                if violations > 0 {
                    eprintln!("\n{} capability violation(s)", violations);
                    std::process::exit(1);
                }
            }
        }
    }

    eprintln!("No errors found");
}

pub fn cmd_check_json(file: &str) {
    let (mut program, source_text, parse_errors) = parse_file(file);

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

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    let mut checker = check_mod::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);

    // Output each diagnostic as JSON (one per line)
    for d in &parse_errors {
        println!("{}", d.to_json());
    }
    for d in &diagnostics {
        println!("{}", d.to_json());
    }

    // Lower to IR for unused variable warnings
    if parse_errors.is_empty() {
        let ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
        let unused = almide::ir::collect_unused_var_warnings(&ir, file);
        for d in &unused {
            println!("{}", d.to_json());
        }
    }
}

pub fn cmd_check_effects(file: &str) {
    let (mut program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for d in &parse_errors {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        eprintln!("\n{} parse error(s)", parse_errors.len());
        std::process::exit(1);
    }

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

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    let mut checker = check_mod::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
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

    // Lower to IR
    let mut ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);

    // Run effect inference
    use almide::codegen::pass_effect_inference::{EffectInferencePass, EffectMap};
    use almide::codegen::pass::NanoPass;
    EffectInferencePass.run(&mut ir, almide::codegen::pass::Target::Rust);

    // Display results
    eprintln!("{}:\n", file);
    let mut entries: Vec<_> = ir.effect_map.functions.iter().collect();
    entries.sort_by_key(|(name, _)| (*name).clone());

    for (name, fe) in &entries {
        let effects = EffectMap::format_effects(&fe.transitive);
        let marker = if fe.is_effect { " (effect fn)" } else { "" };
        eprintln!("  {}  → {}{}", name, effects, marker);
    }

    let pure_count = entries.iter().filter(|(_, fe)| fe.transitive.is_empty()).count();
    let effect_count = entries.len() - pure_count;
    eprintln!("\n{} functions: {} pure, {} with effects", entries.len(), pure_count, effect_count);

    // Check permissions from almide.toml
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            if !proj.permissions.is_empty() {
                use almide::codegen::pass_effect_inference::Effect;
                let allowed: std::collections::HashSet<Effect> = proj.permissions.iter()
                    .filter_map(|s| match s.as_str() {
                        "IO" => Some(Effect::IO),
                        "Net" => Some(Effect::Net),
                        "Env" => Some(Effect::Env),
                        "Time" => Some(Effect::Time),
                        "Rand" => Some(Effect::Rand),
                        "Fan" => Some(Effect::Fan),
                        "Log" => Some(Effect::Log),
                        _ => None,
                    })
                    .collect();

                let mut violations = 0;
                for (name, fe) in &entries {
                    let forbidden: Vec<_> = fe.transitive.iter()
                        .filter(|e| !allowed.contains(e))
                        .collect();
                    if !forbidden.is_empty() {
                        eprintln!(
                            "\nerror: capability violation in `{}`",
                            name
                        );
                        for e in &forbidden {
                            eprintln!("  {} is not in [permissions].allow", e);
                        }
                        eprintln!(
                            "  hint: add {} to [permissions].allow in almide.toml",
                            forbidden.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", ")
                        );
                        violations += 1;
                    }
                }
                if violations > 0 {
                    eprintln!("\n{} capability violation(s) found", violations);
                    std::process::exit(1);
                }
                eprintln!("\nPermissions OK: all effects within [permissions].allow = {:?}", proj.permissions);
            }
        }
    }
}
