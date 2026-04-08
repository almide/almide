use std::process::Command;
use crate::try_compile;
use super::{hash64, cargo_build_generated_with_native, cargo_build_test_with_native};

/// Compile an .almd file to a native binary, returning the path to the executable.
/// Uses incremental caching: if the generated Rust code hasn't changed, skips cargo build.
pub fn compile_to_binary(file: &str, no_check: bool, test_mode: bool) -> Result<std::path::PathBuf, String> {
    let rs_code = try_compile(file, no_check).map_err(|_| "compile failed".to_string())?;

    let project_dir = std::env::temp_dir().join("almide-run");
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    let use_test_harness = test_mode || (!rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(") && !rs_code.contains("\npub fn main("));

    let hash_input = format!("{}:test={}", &rs_code, use_test_harness);
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let cache = super::incremental_cache_dir();
    let hash_file = cache.join(format!("{}.hash", file.replace('/', "_").replace('.', "_")));

    // Per-file binary: use file hash as name to avoid collisions during parallel test runs
    let bin_name = format!("almide-{}", &code_hash[..12]);
    let bin_path = project_dir.join("target").join("debug").join(&bin_name);

    let cache_hit = hash_file.exists()
        && bin_path.exists()
        && std::fs::read_to_string(&hash_file).ok().as_deref() == Some(&code_hash);

    if cache_hit {
        return Ok(bin_path);
    }

    // Load native deps from almide.toml if present
    let parsed = std::path::Path::new("almide.toml").exists()
        .then(|| crate::project::parse_toml(std::path::Path::new("almide.toml")).ok())
        .flatten();
    let native_deps = parsed.as_ref().map(|p| p.native_deps.as_slice()).unwrap_or(&[]);
    let source_root = if native_deps.is_empty() { None } else { Some(std::path::Path::new(".")) };

    let result = if use_test_harness {
        cargo_build_test_with_native(&rs_code, &project_dir, native_deps, source_root)
    } else {
        cargo_build_generated_with_native(&rs_code, &project_dir, false, native_deps, source_root)
    };

    match result {
        Ok(built_path) => {
            // Copy built binary to per-file cached path
            let _ = std::fs::copy(&built_path, &bin_path);
            let _ = std::fs::create_dir_all(&cache);
            let _ = std::fs::write(&hash_file, &code_hash);
            Ok(bin_path)
        }
        Err(e) => Err(e),
    }
}

/// Run a compiled binary with the given args, returning exit code.
pub fn run_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    let status = Command::new(bin)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });
    status.code().unwrap_or(1)
}

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool, test_mode: bool) -> i32 {
    match compile_to_binary(file, no_check, test_mode) {
        Ok(bin) => run_binary(&bin, program_args),
        Err(e) => {
            eprintln!("Compile error:\n{}", e);
            1
        }
    }
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check, false));
}
