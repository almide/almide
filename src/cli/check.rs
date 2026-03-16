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

    eprintln!("No errors found");
}
