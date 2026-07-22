use crate::{parse_file, canonicalize, check as check_mod, diagnostic, resolve, project, out, err};

/// `cmd_check`'s combined parse+checker error reporting and
/// `--deny-warnings` gate. Extracted verbatim — exits the process exactly
/// where the original code did.
fn report_check_errors_or_exit(
    parse_errors: &[diagnostic::Diagnostic],
    diagnostics: &[diagnostic::Diagnostic],
    warnings: &[&diagnostic::Diagnostic],
    source_text: &str,
    deny_warnings: bool,
) {
    let mut all_errors: Vec<&diagnostic::Diagnostic> = parse_errors.iter().collect();
    let checker_errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    all_errors.extend(checker_errors);
    if deny_warnings && !warnings.is_empty() {
        // Treat warnings as errors
        for d in &all_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
        let total = all_errors.len() + warnings.len();
        err(&format!("\n{} error(s) found (--deny-warnings: {} warning(s) treated as errors)", total, warnings.len()));
        std::process::exit(1);
    }
    if !all_errors.is_empty() {
        for d in &all_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
        err(&format!("\n{} error(s) found", all_errors.len()));
        std::process::exit(1);
    }
}

/// Resolve dependencies, resolve imports, canonicalize, and type-check an
/// already-parsed program — the common middle section shared by
/// `cmd_check`/`cmd_check_json`/`cmd_check_effects`, which had near-
/// identical copies of this pipeline. Exits the process on a dependency-
/// fetch or import-resolution failure (matching the original
/// `.unwrap_or_else(|e| {err;exit;})` chain at each call site). Doesn't
/// call `parse_file` itself so each caller keeps its own parse + any
/// pre-resolve early-exit check (`cmd_check_effects`'s parse-error gate)
/// at exactly its original point in the control flow.
fn resolve_and_typecheck_for_check(file: &str, program: &mut almide::ast::Program, source_text: &str) -> (Vec<diagnostic::Diagnostic>, check_mod::Checker) {
    let dep_paths = super::dep_paths_from_cwd_toml();

    let resolved = resolve::resolve_imports_with_deps(file, program, &dep_paths)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });

    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check_mod::Checker::from_env(canon.env);
    checker.set_source(file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);

    (diagnostics, checker)
}

pub fn cmd_check(file: &str, deny_warnings: bool) {
    let (mut program, source_text, parse_errors) = parse_file(file);
    let (diagnostics, checker) = resolve_and_typecheck_for_check(file, &mut program, &source_text);

    // Lower to IR for unused variable analysis (only if no parse or type errors)
    let has_type_errors = diagnostics.iter().any(|d| d.level == diagnostic::Level::Error);
    let unused_warnings = if parse_errors.is_empty() && !has_type_errors {
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
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
        err(&format!("{}", crate::diagnostic_render::display_with_source(d, &source_text)));
    }

    // Combine parse errors + checker errors
    report_check_errors_or_exit(&parse_errors, &diagnostics, &warnings, &source_text, deny_warnings);

    // Security Layer 2: check permissions if defined in almide.toml
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            if !proj.permissions.is_empty() {
                let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
                if let Err(_) = super::check_permissions(&ir, &proj.permissions) {
                    std::process::exit(1);
                }
            }
        }
    }

    err(&format!("No errors found"));
}

pub fn cmd_check_json(file: &str) {
    let (mut program, source_text, parse_errors) = parse_file(file);
    let (diagnostics, checker) = resolve_and_typecheck_for_check(file, &mut program, &source_text);

    // Output each diagnostic as JSON (one per line)
    for d in &parse_errors {
        out(&format!("{}", crate::diagnostic_render::to_json(d)));
    }
    for d in &diagnostics {
        out(&format!("{}", crate::diagnostic_render::to_json(d)));
    }

    // Lower to IR for unused variable warnings (skip if type errors)
    let has_type_errors = diagnostics.iter().any(|d| d.level == diagnostic::Level::Error);
    if parse_errors.is_empty() && !has_type_errors {
        let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
        let unused = almide::ir::collect_unused_var_warnings(&ir, file);
        for d in &unused {
            out(&format!("{}", crate::diagnostic_render::to_json(d)));
        }
    }
}

/// `cmd_check_effects`'s `[permissions].allow` enforcement block. Extracted
/// verbatim — reads only its parameters, exits the process exactly where
/// the original code did.
fn enforce_effect_permissions(
    proj: &project::Project,
    entries: &[(&String, &almide::codegen::pass_effect_inference::FunctionEffects)],
) {
    use almide::codegen::pass_effect_inference::Effect;
    let allowed: std::collections::HashSet<Effect> = proj.permissions.iter()
        .filter_map(|s| match s.as_str() {
            "IO" => Some(Effect::IO),
            "Net" => Some(Effect::Net),
            "Env" => Some(Effect::Env),
            "Time" => Some(Effect::Time),
            "Rand" => Some(Effect::Rand),
            "Fan" => Some(Effect::Fan),
            _ => None,
        })
        .collect();

    let mut violations = 0;
    for (name, fe) in entries {
        let forbidden: Vec<_> = fe.transitive.iter()
            .filter(|e| !allowed.contains(e))
            .collect();
        if !forbidden.is_empty() {
            err(&format!(
                "\nerror: capability violation in `{}`",
                name
            ));
            for e in &forbidden {
                err(&format!("  {} is not in [permissions].allow", e));
            }
            err(&format!(
                "  hint: add {} to [permissions].allow in almide.toml",
                forbidden.iter().map(|e| format!("\"{}\"", e)).collect::<Vec<_>>().join(", ")
            ));
            violations += 1;
        }
    }
    if violations > 0 {
        err(&format!("\n{} capability violation(s) found", violations));
        std::process::exit(1);
    }
    err(&format!("\nPermissions OK: all effects within [permissions].allow = {:?}", proj.permissions));
}

pub fn cmd_check_effects(file: &str) {
    let (mut program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for d in &parse_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, &source_text)));
        }
        err(&format!("\n{} parse error(s)", parse_errors.len()));
        std::process::exit(1);
    }

    let (diagnostics, checker) = resolve_and_typecheck_for_check(file, &mut program, &source_text);

    let errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    if !errors.is_empty() {
        for d in &errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, &source_text)));
        }
        err(&format!("\n{} error(s) found", errors.len()));
        std::process::exit(1);
    }

    // Lower to IR
    let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);

    // Run effect inference
    use almide::codegen::pass_effect_inference::{EffectInferencePass, EffectMap};
    use almide::codegen::pass::NanoPass;
    let result = EffectInferencePass.run(ir, almide::codegen::pass::Target::Rust);
    let ir = result.program;

    // Display results
    err(&format!("{}:\n", file));
    let mut entries: Vec<_> = ir.effect_map.functions.iter().collect();
    entries.sort_by_key(|(name, _)| (*name).clone());

    for (name, fe) in &entries {
        let effects = EffectMap::format_effects(&fe.transitive);
        let marker = if fe.is_effect { " (effect fn)" } else { "" };
        err(&format!("  {}  → {}{}", name, effects, marker));
    }

    let pure_count = entries.iter().filter(|(_, fe)| fe.transitive.is_empty()).count();
    let effect_count = entries.len() - pure_count;
    err(&format!("\n{} functions: {} pure, {} with effects", entries.len(), pure_count, effect_count));

    // Check permissions from almide.toml
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            if !proj.permissions.is_empty() {
                enforce_effect_permissions(&proj, &entries);
            }
        }
    }
}
