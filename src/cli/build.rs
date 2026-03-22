use std::process::Command;
use crate::{compile_with_ir, parse_file, find_rustc, check, diagnostic, resolve, project, project_fetch};

pub fn cmd_build(file: &str, output: Option<&str>, target: Option<&str>, release: bool, fast: bool, _unchecked_index: bool, no_check: bool) {
    let is_npm = matches!(target, Some("npm"));
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));

    if is_npm {
        let out_dir = output.unwrap_or("dist");
        cmd_build_npm(file, out_dir, no_check);
        return;
    }

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        cmd_build_wasm_direct(file, output, no_check);
        return;
    }

    let default_output = if is_wasm {
        format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"))
    } else if std::path::Path::new("almide.toml").exists() {
        let toml_content = std::fs::read_to_string("almide.toml").unwrap_or_default();
        toml_content.lines()
            .find(|l| l.starts_with("name"))
            .and_then(|l| l.split('=').nth(1))
            .map(|s| s.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| file.strip_suffix(".almd").unwrap_or("a.out").to_string())
    } else {
        file.strip_suffix(".almd").unwrap_or("a.out").to_string()
    };
    let output_raw = output.unwrap_or(&default_output);

    // On Windows, auto-append .exe for native builds
    let output = if cfg!(target_os = "windows") && !is_wasm
        && !output_raw.ends_with(".exe") && !output_raw.ends_with(".wasm")
    {
        format!("{}.exe", output_raw)
    } else {
        output_raw.to_string()
    };

    let (rs_code, _ir) = compile_with_ir(file, no_check);

    let stem = output.strip_suffix(".wasm")
        .or_else(|| output.strip_suffix(".exe"))
        .unwrap_or(&output);
    let tmp_rs = format!("{}.rs", stem);
    if let Err(e) = std::fs::write(&tmp_rs, &rs_code) {
        eprintln!("Failed to write {}: {}", tmp_rs, e);
        std::process::exit(1);
    }

    let mut rustc_cmd = Command::new(&find_rustc());
    rustc_cmd.arg(&tmp_rs)
        .arg("-o")
        .arg(&output)
        .arg("-C").arg("overflow-checks=no")
        .arg("--edition").arg("2021");

    if is_wasm {
        rustc_cmd.arg("--target").arg("wasm32-wasip1")
            .arg("-C").arg("opt-level=s")
            .arg("-C").arg("lto=yes");
    } else if fast {
        rustc_cmd.arg("-C").arg("opt-level=3")
            .arg("-C").arg("target-cpu=native")
            .arg("-C").arg("llvm-args=-fp-contract=fast")
            .arg("-C").arg("lto=thin")
            .arg("-C").arg("codegen-units=1");
    } else if release {
        rustc_cmd.arg("-C").arg("opt-level=3");
    }

    let rustc = rustc_cmd.output()
        .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

    let _ = std::fs::remove_file(&tmp_rs);

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        eprintln!("Compile error:\n{}", stderr);
        std::process::exit(1);
    }

    eprintln!("Built {}", output);
}

/// Direct WASM emit: parse → check → lower → optimize → monomorphize → emit WASM binary.
fn cmd_build_wasm_direct(file: &str, output: Option<&str>, _no_check: bool) {
    let default_output = format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"));
    let output = output.unwrap_or(&default_output);

    let (mut program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!("{}", e.display_with_source(&source_text));
        }
        std::process::exit(1);
    }

    // Resolve dependencies
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

    // Type check
    let mut checker = check::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        for d in &diagnostics {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        std::process::exit(1);
    }

    // Lower to IR
    let mut ir_program = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);

    // Optimize
    almide::optimize::optimize_program(&mut ir_program);

    // Monomorphize
    almide::mono::monomorphize(&mut ir_program);

    // Run nanopass pipeline (TCO, effect inference, stream fusion, result propagation, fan lowering)
    let config = almide::codegen::target::configure(almide::codegen::pass::Target::Wasm);
    config.pipeline.run(&mut ir_program, almide::codegen::pass::Target::Wasm);

    // Emit WASM binary
    let bytes = almide::codegen::emit_wasm_binary(&ir_program);

    if let Err(e) = std::fs::write(output, &bytes) {
        eprintln!("Failed to write {}: {}", output, e);
        std::process::exit(1);
    }

    eprintln!("Built {} ({} bytes)", output, bytes.len());
}

fn cmd_build_npm(file: &str, out_dir: &str, _no_check: bool) {
    let (mut program, source_text, _parse_errors) = parse_file(file);

    // Resolve dependencies
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

    // Type check (always needed for IR lowering)
    let mut checker = check::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        for d in &diagnostics {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        std::process::exit(1);
    }

    // Lower to IR
    let mut ir_program = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
    for (name, mod_prog, pkg_id, _) in &resolved.modules {
        if almide::stdlib::is_stdlib_module(name) { continue; }
        let mod_types = checker.check_module_bodies(&mut mod_prog.clone());
        let versioned = pkg_id.as_ref().map(|pid| pid.mod_name());
        let mod_ir_module = almide::lower::lower_module(name, &mod_prog, &mod_types, &checker.env, versioned);
        ir_program.modules.push(mod_ir_module);
    }

    // Read package metadata from almide.toml (or use defaults)
    let (pkg_name, pkg_version) = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            (proj.package.name, proj.package.version)
        } else {
            (file.strip_suffix(".almd").unwrap_or("my-package").to_string(), "0.1.0".to_string())
        }
    } else {
        (file.strip_suffix(".almd").unwrap_or("my-package").to_string(), "0.1.0".to_string())
    };

    // Generate JS via v3 codegen
    almide::mono::monomorphize(&mut ir_program);
    let js_code = almide::codegen::emit(&mut ir_program, almide::codegen::pass::Target::TypeScript);

    let package_json = format!(
        r#"{{"name":"{}","version":"{}","main":"index.js","type":"module"}}"#,
        pkg_name, pkg_version
    );

    // Write files
    let out_path = std::path::Path::new(out_dir);
    std::fs::create_dir_all(out_path).unwrap_or_else(|e| {
        eprintln!("Failed to create {}: {}", out_dir, e);
        std::process::exit(1);
    });

    std::fs::write(out_path.join("package.json"), &package_json).unwrap_or_else(|e| {
        eprintln!("Failed to write package.json: {}", e);
        std::process::exit(1);
    });
    std::fs::write(out_path.join("index.js"), &js_code).unwrap_or_else(|e| {
        eprintln!("Failed to write index.js: {}", e);
        std::process::exit(1);
    });

    eprintln!("Built npm package in {}/", out_dir);
    eprintln!("  package.json");
    eprintln!("  index.js");
}
