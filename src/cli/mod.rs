/// CLI command implementations.

mod run;
mod build;
mod emit;
mod check;
mod commands;

pub use run::{cmd_run, cmd_run_inner};
pub use build::cmd_build;
pub use emit::cmd_emit;
pub use check::{cmd_check, cmd_check_json, cmd_check_effects};
pub use commands::{cmd_init, cmd_test, cmd_test_json, cmd_test_wasm, cmd_test_ts, cmd_fmt, cmd_clean};

use std::hash::{Hash, Hasher};

/// Check that all effects used in the program are allowed by [permissions].allow in almide.toml.
/// Returns Ok(()) if no violations, or Err with a description of violations.
pub fn check_permissions(ir: &almide::ir::IrProgram, permissions: &[String]) -> Result<(), String> {
    use almide::codegen::pass_effect_inference::{EffectInferencePass, Effect};
    use almide::codegen::pass::NanoPass;

    let result = EffectInferencePass.run(ir.clone(), almide::codegen::pass::Target::Rust);
    let ir_after = result.program;

    let allowed: std::collections::HashSet<Effect> = permissions.iter()
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
    for (name, fe) in &ir_after.effect_map.functions {
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
        return Err(format!("{} capability violation(s)", violations));
    }
    Ok(())
}

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

/// Cargo.toml template for generated Rust projects (without HTTP/TLS).
const GENERATED_CARGO_TOML: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

/// Cargo.toml template with HTTP/TLS dependencies (only when http runtime is used).
const GENERATED_CARGO_TOML_HTTP: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

[dependencies]
rustls = { version = "0.23", default-features = false, features = ["ring", "logging", "std", "tls12"] }
webpki-roots = "0.26"

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

const GENERATED_CARGO_TOML_ML: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

[dependencies]
rustls = { version = "0.23", default-features = false, features = ["ring", "logging", "std", "tls12"] }
webpki-roots = "0.26"
burn = { version = "0.16", features = ["ndarray"] }
ndarray = { version = "0.16", features = ["blas"] }

[target.'cfg(target_os = "macos")'.dependencies]
blas-src = { version = "0.10", features = ["accelerate"] }

[target.'cfg(not(target_os = "macos"))'.dependencies]
blas-src = { version = "0.10", features = ["openblas"] }
openblas-src = { version = "0.10", features = ["static"] }

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

const BURN_MATRIX_RUNTIME: &str = include_str!("../../runtime/rs/burn/matrix_burn.rs");

/// Build generated Rust code using cargo.
/// Returns the path to the built binary on success.
fn cargo_build_generated(rs_code: &str, project_dir: &std::path::Path, release: bool) -> Result<std::path::PathBuf, String> {
    let uses_matrix = rs_code.contains("almide_rt_matrix_");
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    let cargo_toml = if uses_matrix { GENERATED_CARGO_TOML_ML } else if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;
    let final_code = if uses_matrix {
        replace_matrix_runtime(rs_code)
    } else {
        rs_code.to_string()
    };
    std::fs::write(src_dir.join("main.rs"), &final_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("build").current_dir(project_dir);
    if release {
        cmd.arg("--release");
    }
    // Suppress cargo's chatty output
    cmd.arg("--quiet");

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let profile = if release { "release" } else { "debug" };
    let bin_path = project_dir.join("target").join(profile).join("almide-out");
    if !bin_path.exists() {
        return Err(format!("expected binary not found at {}", bin_path.display()));
    }
    Ok(bin_path)
}

/// Build generated Rust code using cargo for test mode (--test harness).
/// Returns the path to the built test binary on success.
fn cargo_build_test(rs_code: &str, project_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let cargo_toml = if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;
    std::fs::write(src_dir.join("main.rs"), rs_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    // Use `cargo test --no-run` to build the test binary without running it
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("test").arg("--no-run").arg("--quiet").arg("--message-format=json")
        .current_dir(project_dir);

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    // Parse the JSON output to find the test binary path
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-artifact") {
                if let Some(exe) = json.get("executable").and_then(|e| e.as_str()) {
                    return Ok(std::path::PathBuf::from(exe));
                }
            }
        }
    }

    Err("could not determine test binary path from cargo output".to_string())
}

/// Replace the Vec<Vec<f64>> matrix runtime with burn-backed implementation.
fn replace_matrix_runtime(rs_code: &str) -> String {
    let mut result = String::with_capacity(rs_code.len() + BURN_MATRIX_RUNTIME.len());
    let mut in_matrix_block = false;
    let mut inserted = false;

    for line in rs_code.lines() {
        if !in_matrix_block && line.contains("pub type AlmideMatrix = Vec<Vec<f64>>") {
            in_matrix_block = true;
            if !inserted {
                result.push_str("// ── burn-backed Matrix runtime (auto-inserted by almide build) ──\n");
                result.push_str(BURN_MATRIX_RUNTIME);
                result.push('\n');
                inserted = true;
            }
            continue;
        }
        if in_matrix_block {
            if line.starts_with("pub fn almide_rt_matrix_")
                || line.starts_with("    ")
                || line.starts_with("        ")
                || line.starts_with("// matrix")
                || line == "}" || line.is_empty() {
                continue;
            }
            in_matrix_block = false;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
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
