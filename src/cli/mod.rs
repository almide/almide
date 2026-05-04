/// CLI command implementations.

mod run;
mod build;
mod compile;
mod emit;
mod check;
mod commands;
mod install;
mod selfupdate;
mod ide;
mod fix;
mod docs_gen;

pub use run::{cmd_run, cmd_run_inner};
pub use build::cmd_build;
pub use compile::cmd_compile;
pub use emit::cmd_emit;
pub use check::{cmd_check, cmd_check_json, cmd_check_effects};
pub use commands::{cmd_init, cmd_test, cmd_test_json, cmd_test_wasm, cmd_test_ts, cmd_fmt, cmd_clean};
pub use install::cmd_install;
pub use selfupdate::cmd_self_update;
pub use ide::{cmd_ide_outline, cmd_ide_doc, cmd_ide_stdlib_snapshot};
pub use fix::cmd_fix;
pub use docs_gen::cmd_docs_gen;

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
rayon = "1.10"

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

/// Build a Cargo.toml string by inserting native deps into the [dependencies] section.
fn build_cargo_toml(base_toml: &str, native_deps: &[crate::project::NativeDep]) -> String {
    if native_deps.is_empty() {
        return base_toml.to_string();
    }
    let mut toml = base_toml.to_string();
    let mut extra_deps = String::new();
    for dep in native_deps {
        let dep_line = if dep.spec.starts_with('{') {
            format!("{} = {}\n", dep.name, dep.spec)
        } else {
            format!("{} = \"{}\"\n", dep.name, dep.spec)
        };
        extra_deps.push_str(&dep_line);
    }
    if let Some(pos) = toml.find("[dependencies]") {
        let insert_pos = toml[pos..].find('\n').map(|i| pos + i + 1).unwrap_or(toml.len());
        toml.insert_str(insert_pos, &extra_deps);
    } else {
        toml.push_str("\n[dependencies]\n");
        toml.push_str(&extra_deps);
    }
    toml
}

/// Copy native/*.rs files from source_root into src_dir and inject mod declarations into code.
fn inject_native_modules(code: &mut String, source_root: Option<&std::path::Path>, src_dir: &std::path::Path) -> Result<(), String> {
    let root = match source_root {
        Some(r) => r,
        None => return Ok(()),
    };
    let native_dir = root.join("native");
    if !native_dir.is_dir() { return Ok(()); }
    let mut mod_decls = String::new();
    if let Ok(entries) = std::fs::read_dir(&native_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "rs") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
                std::fs::write(src_dir.join(entry.file_name()), &content)
                    .map_err(|e| format!("failed to write native module {}: {}", stem, e))?;
                mod_decls.push_str(&format!("mod {};\n", stem));
            }
        }
    }
    if !mod_decls.is_empty() {
        if let Some(pos) = code.find("\nuse ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else if let Some(pos) = code.find("\nfn ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else {
            *code = format!("{}\n{}", mod_decls, code);
        }
    }
    Ok(())
}

/// Build generated Rust code as a cdylib shared library (.dylib/.so).
fn cargo_build_cdylib(rs_code: &str, project_dir: &std::path::Path, lib_name: &str, release: bool) -> Result<std::path::PathBuf, String> {
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    let cargo_toml = format!(r#"[package]
name = "almide-cdylib"
version = "0.1.0"
edition = "2021"

[lib]
name = "{}"
crate-type = ["cdylib"]
path = "src/lib.rs"

[profile.dev]
opt-level = 1

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#, lib_name.replace('-', "_"));
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;
    std::fs::write(src_dir.join("lib.rs"), rs_code)
        .map_err(|e| format!("failed to write lib.rs: {}", e))?;

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("build").current_dir(project_dir).arg("--quiet");
    if release { cmd.arg("--release"); }
    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let profile = if release { "release" } else { "debug" };
    let prefix = if cfg!(target_os = "macos") || cfg!(target_os = "linux") { "lib" } else { "" };
    let ext = if cfg!(target_os = "macos") { "dylib" } else if cfg!(target_os = "windows") { "dll" } else { "so" };
    let lib_filename = format!("{}{}.{}", prefix, lib_name.replace('-', "_"), ext);
    let lib_path = project_dir.join("target").join(profile).join(&lib_filename);
    if !lib_path.exists() {
        return Err(format!("expected library not found at {}", lib_path.display()));
    }

    // Copy to current directory
    let dest = std::path::Path::new(".").join(&lib_filename);
    std::fs::copy(&lib_path, &dest)
        .map_err(|e| format!("failed to copy library: {}", e))?;
    Ok(dest)
}

/// Build generated Rust code using cargo.
/// Returns the path to the built binary on success.
fn cargo_build_generated(rs_code: &str, project_dir: &std::path::Path, release: bool) -> Result<std::path::PathBuf, String> {
    cargo_build_generated_with_native(rs_code, project_dir, release, &[], None)
}

/// Build generated Rust code with optional native Rust dependencies and source files.
fn cargo_build_generated_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    release: bool,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let uses_matrix = rs_code.contains("almide_rt_matrix_");
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;

    // Build Cargo.toml: start with base template, append native deps
    let base_toml = if uses_matrix { GENERATED_CARGO_TOML_ML } else if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    let cargo_toml = build_cargo_toml(base_toml, native_deps);
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    let mut final_code = if uses_matrix {
        replace_matrix_runtime(rs_code)
    } else {
        rs_code.to_string()
    };

    inject_native_modules(&mut final_code, source_root, &src_dir)?;

    // Library modules may not define main — auto-generate an empty one
    if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
        final_code.push_str("\nfn main() {}\n");
    }

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
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(wrap_codegen_leak(stderr));
    }

    let profile = if release { "release" } else { "debug" };
    let exe = if cfg!(windows) { "almide-out.exe" } else { "almide-out" };
    let bin_path = project_dir.join("target").join(profile).join(exe);
    if !bin_path.exists() {
        return Err(format!("expected binary not found at {}", bin_path.display()));
    }
    Ok(bin_path)
}

/// Scrub rustc output when our codegen produces invalid Rust — replaces
/// generated paths with placeholders and prepends a bug-report banner so
/// users (and harness classifiers) don't mistake a compiler bug for a
/// user-facing language error. No-op when the output is clean.
fn wrap_codegen_leak(stderr: String) -> String {
    let mentions_main_rs = stderr.contains("src/main.rs");
    let leaks_rustc_code = contains_rustc_error_code(&stderr);
    if !(mentions_main_rs || leaks_rustc_code) {
        return stderr;
    }
    let scrubbed = stderr
        .replace("src/main.rs", "<generated.rs>")
        .replace("almide-out", "almide-generated");
    format!(
        "codegen produced invalid Rust — this is an Almide bug.\n\
         Please file a minimal repro at https://github.com/almide/almide/issues\n\
         \n\
         --- rustc output (edited to hide generated paths) ---\n\
         {}",
        scrubbed
    )
}

/// Build generated Rust code using cargo for test mode (--test harness).
/// Returns the path to the built test binary on success.
fn cargo_build_test(rs_code: &str, project_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    cargo_build_test_with_native(rs_code, project_dir, &[], None)
}

fn cargo_build_test_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let base_toml = if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    let cargo_toml = build_cargo_toml(base_toml, native_deps);
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    let mut final_code = rs_code.to_string();
    inject_native_modules(&mut final_code, source_root, &src_dir)?;

    // Library modules may not define main — auto-generate an empty one for test builds
    if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
        final_code.push_str("\nfn main() {}\n");
    }

    std::fs::write(src_dir.join("main.rs"), &final_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    // Use `cargo test --no-run` to build the test binary without running it
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("test").arg("--no-run").arg("--quiet").arg("--message-format=json")
        .current_dir(project_dir);

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        // --quiet suppresses cargo's own error display, but rustc messages
        // come through stdout as JSON (--message-format=json). Extract the
        // "rendered" field from each compiler-message so the user sees the
        // real error spans, not just "1 previous error; N warnings emitted".
        let mut rendered: Vec<String> = Vec::new();
        let verbose = std::env::var("ALMIDE_TEST_VERBOSE")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                    let level = json.get("message")
                        .and_then(|m| m.get("level"))
                        .and_then(|l| l.as_str())
                        .unwrap_or("");
                    if level == "error" || verbose {
                        if let Some(msg) = json.get("message")
                            .and_then(|m| m.get("rendered"))
                            .and_then(|r| r.as_str())
                        {
                            rendered.push(msg.to_string());
                        }
                    }
                }
            }
        }
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if rendered.is_empty() {
            stderr
        } else {
            format!("{}\n{}", rendered.join("\n"), stderr)
        };
        return Err(wrap_codegen_leak(combined));
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
///
/// Strategy: once we enter the matrix block (marked by `pub type AlmideMatrix
/// = Vec<Vec<f64>>`), skip every line until we find a `pub fn` whose name is
/// NOT prefixed with `almide_rt_matrix_`. That marks the end of the matrix
/// block. This is robust against doc comments (`///`), section dividers,
/// helper functions, etc. — it only cares about whether we're between matrix
/// `pub fn`s.
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
            // End of matrix block = first `pub fn` whose name is NOT
            // `almide_rt_matrix_`. Anything else (doc comment, blank line,
            // indented body, `// section` divider, inline helper fn) stays
            // skipped.
            let trimmed = line.trim_start();
            if trimmed.starts_with("pub fn ") && !trimmed.starts_with("pub fn almide_rt_matrix_") {
                in_matrix_block = false;
                // Fall through to emit this non-matrix line.
            } else {
                continue;
            }
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

/// Detect rustc-style `error[E\d{4}]` codes leaking through our checker.
/// Almide's diagnostic codes are 3 digits (E001..E099); rustc uses 4 digits
/// (E0001..E9999). A 4-digit code in the output unambiguously means our
/// codegen produced invalid Rust — flag it for the bug-report wrapper so
/// dojo classifiers don't mistake it for a user-facing language error.
fn contains_rustc_error_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = b"error[E";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Count consecutive digits after `error[E`
            let mut j = i + needle.len();
            let mut digits = 0;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits += 1;
                j += 1;
            }
            if digits >= 4 && j < bytes.len() && bytes[j] == b']' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::contains_rustc_error_code;

    #[test]
    fn detects_4_digit_rustc_code() {
        assert!(contains_rustc_error_code("error[E0599]: no method named 'foo' found"));
        assert!(contains_rustc_error_code("blah\nerror[E0382]: use of moved value\nblah"));
    }

    #[test]
    fn ignores_3_digit_almide_code() {
        assert!(!contains_rustc_error_code("error[E001]: type mismatch"));
        assert!(!contains_rustc_error_code("error[E013]: ..."));
    }

    #[test]
    fn ignores_no_brackets() {
        assert!(!contains_rustc_error_code("error: something went wrong"));
        assert!(!contains_rustc_error_code(""));
    }
}
