/// CLI command implementations.

use std::process::Command;
use std::hash::{Hash, Hasher};
use crate::{compile, compile_with_options, parse_file, find_rustc, emit_rust, emit_ts, check, diagnostic, resolve, fmt, project};

/// Compute a 64-bit hash of a byte slice (using DefaultHasher).
fn hash64(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Cache directory for incremental compilation.
fn incremental_cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".almide/cache")
}

/// Recursively collect .almd files that contain `test` blocks.
fn collect_test_files(dir: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();
    // Skip hidden directories, target/, node_modules/, etc.
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name.starts_with('.') || dir_name == "target" || dir_name == "node_modules" {
        return files;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_test_files(&path));
            } else if path.extension().map(|e| e == "almd").unwrap_or(false) {
                // Check if file contains a test block
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("\ntest ") || content.starts_with("test ") {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    files
}

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool) -> i32 {
    let rs_code = compile(file, no_check);

    let tmp_dir = std::env::temp_dir().join("almide-run");
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        eprintln!("Failed to create temp directory {}: {}", tmp_dir.display(), e);
        std::process::exit(1);
    }

    let file_stem = file.replace('/', "_").replace('.', "_");
    let rs_path = tmp_dir.join(format!("{}.rs", file_stem));
    let bin_path = tmp_dir.join(&file_stem);

    // Detect test-only files (no main function)
    let is_test_only = !rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(");

    // Incremental: hash generated Rust code + test mode, skip rustc if unchanged
    let hash_input = format!("{}:test={}", &rs_code, is_test_only);
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let cache = incremental_cache_dir();
    let hash_file = cache.join(format!("{}.hash", file.replace('/', "_").replace('.', "_")));

    let cache_hit = hash_file.exists()
        && bin_path.exists()
        && std::fs::read_to_string(&hash_file).ok().as_deref() == Some(&code_hash);

    if !cache_hit {
        if let Err(e) = std::fs::write(&rs_path, &rs_code) {
            eprintln!("Failed to write {}: {}", rs_path.display(), e);
            std::process::exit(1);
        }

        let mut rustc_cmd = Command::new(&find_rustc());
        rustc_cmd.arg(&rs_path)
            .arg("-o")
            .arg(&bin_path)
            .arg("-C").arg("overflow-checks=no")
            .arg("-C").arg("opt-level=1")
            .arg("-C").arg("incremental=")
            .arg("--edition").arg("2021");
        if is_test_only {
            rustc_cmd.arg("--test");
        }
        let rustc = rustc_cmd.output()
            .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

        if !rustc.status.success() {
            let stderr = String::from_utf8_lossy(&rustc.stderr);
            eprintln!("Compile error:\n{}", stderr);
            return 1;
        }

        // Save hash on successful compile
        let _ = std::fs::create_dir_all(&cache);
        let _ = std::fs::write(&hash_file, &code_hash);
    }

    let status = Command::new(&bin_path)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });

    status.code().unwrap_or(1)
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check));
}

pub fn cmd_init() {
    if std::path::Path::new("almide.toml").exists() {
        eprintln!("almide.toml already exists");
        std::process::exit(1);
    }
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "myapp".to_string());

    let toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", dir_name);

    if let Err(e) = std::fs::write("almide.toml", toml) {
        eprintln!("Failed to write almide.toml: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("src") {
        eprintln!("Failed to create src/: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("tests") {
        eprintln!("Failed to create tests/: {}", e);
        std::process::exit(1);
    }

    if !std::path::Path::new("src/main.almd").exists() {
        if let Err(e) = std::fs::write("src/main.almd", "effect fn main(args: List[String]) -> Result[Unit, String] = {\n  println(\"Hello, Almide!\")\n  ok(())\n}\n") {
            eprintln!("Failed to write src/main.almd: {}", e);
            std::process::exit(1);
        }
    }

    // Generate CLAUDE.md for AI-assisted development
    if !std::path::Path::new("CLAUDE.md").exists() {
        let claude_md = include_str!("../docs/CLAUDE_TEMPLATE.md");
        if let Err(e) = std::fs::write("CLAUDE.md", claude_md) {
            eprintln!("Failed to write CLAUDE.md: {}", e);
            std::process::exit(1);
        }
    }

    eprintln!("Initialized project in ./");
    eprintln!("  almide.toml");
    eprintln!("  src/main.almd");
    eprintln!("  tests/");
    eprintln!("  CLAUDE.md");
}

pub fn cmd_test(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                eprintln!("No .almd files with test blocks found in {}", file);
                std::process::exit(1);
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        // Default: recursively find all .almd files with test blocks in current directory
        let mut files = collect_test_files(std::path::Path::new("."));
        files.sort();
        if files.is_empty() {
            eprintln!("No .almd files with test blocks found.");
            std::process::exit(1);
        }
        files
    };

    let mut program_args: Vec<String> = Vec::new();
    if let Some(filter) = run_filter {
        // Pass filter to rustc test binary
        program_args.push(filter.to_string());
    }

    let mut failed = 0;
    for test_file in &test_files {
        eprintln!("Running {}", test_file);
        let code = cmd_run_inner(test_file, &program_args, no_check);
        if code != 0 {
            failed += 1;
        }
    }
    if failed > 0 {
        eprintln!("\n{}/{} test file(s) failed", failed, test_files.len());
        std::process::exit(1);
    }
    eprintln!("\nAll {} test file(s) passed", test_files.len());
}

pub fn cmd_build(file: &str, output: Option<&str>, target: Option<&str>, release: bool, fast: bool, unchecked_index: bool, no_check: bool) {
    let is_npm = matches!(target, Some("npm"));
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));

    if is_npm {
        let out_dir = output.unwrap_or("dist");
        cmd_build_npm(file, out_dir, no_check);
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

    let emit_options = emit_rust::EmitOptions { no_thread_wrap: is_wasm, fast_mode: unchecked_index };
    let wasm_target = if is_wasm { Some("wasm") } else { None };
    let (rs_code, _ir) = compile_with_options(file, no_check, &emit_options, wasm_target);

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
        rustc_cmd.arg("-C").arg("opt-level=2");
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

fn cmd_build_npm(file: &str, out_dir: &str, _no_check: bool) {
    let (mut program, source_text, _parse_errors) = parse_file(file);

    // Resolve dependencies
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
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

    let config = emit_ts::NpmConfig {
        name: pkg_name,
        version: pkg_version,
    };

    let output = emit_ts::emit_npm_package(&ir_program, &config);

    // Write files
    let out_path = std::path::Path::new(out_dir);
    std::fs::create_dir_all(out_path).unwrap_or_else(|e| {
        eprintln!("Failed to create {}: {}", out_dir, e);
        std::process::exit(1);
    });

    std::fs::write(out_path.join("package.json"), &output.package_json).unwrap_or_else(|e| {
        eprintln!("Failed to write package.json: {}", e);
        std::process::exit(1);
    });
    std::fs::write(out_path.join("index.js"), &output.index_js).unwrap_or_else(|e| {
        eprintln!("Failed to write index.js: {}", e);
        std::process::exit(1);
    });
    std::fs::write(out_path.join("index.d.ts"), &output.index_dts).unwrap_or_else(|e| {
        eprintln!("Failed to write index.d.ts: {}", e);
        std::process::exit(1);
    });

    // Write runtime files
    for (path, content) in &output.runtime_files {
        let full_path = out_path.join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                eprintln!("Failed to create directory: {}", e);
                std::process::exit(1);
            });
        }
        std::fs::write(&full_path, content).unwrap_or_else(|e| {
            eprintln!("Failed to write {}: {}", path, e);
            std::process::exit(1);
        });
    }

    eprintln!("Built npm package in {}/", out_dir);
    eprintln!("  package.json");
    eprintln!("  index.js");
    eprintln!("  index.d.ts");
    for (path, _) in &output.runtime_files {
        eprintln!("  {}", path);
    }
}

pub fn cmd_emit(file: &str, target: &str, emit_ast: bool, emit_ir: bool, no_check: bool) {
    let (mut program, source_text, _parse_errors) = parse_file(file);

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
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
                    path.last().unwrap().clone()
                } else if is_self_import {
                    // import self as alias → target is the package name (loaded from resolved modules)
                    resolved.modules.iter()
                        .find(|(_, _, _, is_self)| *is_self)
                        .map(|(name, _, _, _)| name.clone())
                        .unwrap_or_else(|| path.join("."))
                } else {
                    path.join(".")
                };
                Some((a.clone(), target))
            } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                let last = path.last().expect("path.len() > 1 checked above").clone();
                Some((last, path.join(".")))
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
        let code = match target {
            "rust" | "rs" => {
                let ir = ir_program.as_ref().expect("IR required for Rust codegen");
                emit_rust::emit_with_options(ir, &emit_rust::EmitOptions::default(), &import_aliases, &module_irs)
            }
            "rust-ir" => {
                let ir = ir_program.as_ref().expect("IR required for RustIR codegen");
                emit_rust::emit_via_rust_ir(ir)
            }
            "ts" | "typescript" => {
                let ir = ir_program.as_ref().expect("IR required for TS codegen");
                emit_ts::emit_with_modules(ir)
            }
            "js" | "javascript" => {
                let ir = ir_program.as_ref().expect("IR required for JS codegen");
                emit_ts::emit_js_with_modules(ir)
            }
            other => { eprintln!("Unknown target: {}. Use rust, ts, or js.", other); std::process::exit(1); }
        };
        print!("{}", code);
    }
}

pub fn cmd_check(file: &str, deny_warnings: bool) {
    let (mut program, source_text, parse_errors) = parse_file(file);

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
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

    let mut checker = check::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);

    let warnings: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Warning)
        .collect();
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

pub fn cmd_fmt(files: &[String], write_back: bool) {
    for file in files {
        let (program, _, _) = parse_file(file);
        let formatted = fmt::format_program(&program);
        if write_back {
            std::fs::write(file, &formatted)
                .unwrap_or_else(|e| { eprintln!("Failed to write {}: {}", file, e); std::process::exit(1); });
            eprintln!("Formatted {}", file);
        } else {
            print!("{}", formatted);
        }
    }
}

pub fn cmd_clean() {
    let mut cleaned = false;
    let dep_cache = project::cache_dir();
    if dep_cache.exists() {
        std::fs::remove_dir_all(&dep_cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", dep_cache.display());
        cleaned = true;
    }
    let inc_cache = incremental_cache_dir();
    if inc_cache.exists() {
        std::fs::remove_dir_all(&inc_cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean incremental cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", inc_cache.display());
        cleaned = true;
    }
    if !cleaned {
        eprintln!("No cache to clean");
    }
}
