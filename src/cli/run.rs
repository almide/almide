use std::process::Command;
use crate::try_compile;
use super::{hash64, cargo_build_generated, cargo_build_test};

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool, test_mode: bool) -> i32 {
    let rs_code = match try_compile(file, no_check) {
        Ok(code) => code,
        Err(_) => return 1,
    };

    let project_dir = std::env::temp_dir().join("almide-run");
    if let Err(e) = std::fs::create_dir_all(&project_dir) {
        eprintln!("Failed to create temp directory {}: {}", project_dir.display(), e);
        std::process::exit(1);
    }

    // test_mode: always use --test. Otherwise detect test-only files (no main function).
    let use_test_harness = test_mode || (!rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(") && !rs_code.contains("\npub fn main("));

    // Incremental: hash generated Rust code + test mode, skip cargo if unchanged
    let hash_input = format!("{}:test={}", &rs_code, use_test_harness);
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let cache = super::incremental_cache_dir();
    let hash_file = cache.join(format!("{}.hash", file.replace('/', "_").replace('.', "_")));

    // Determine expected binary path
    let bin_path = if use_test_harness {
        // For test mode, the binary path is determined by cargo
        // Check cache first; if hit, re-derive path from previous build
        project_dir.join("target").join("debug").join("almide-out")
    } else {
        project_dir.join("target").join("debug").join("almide-out")
    };

    let cache_hit = hash_file.exists()
        && bin_path.exists()
        && std::fs::read_to_string(&hash_file).ok().as_deref() == Some(&code_hash);

    let actual_bin = if cache_hit {
        bin_path
    } else {
        let result = if use_test_harness {
            cargo_build_test(&rs_code, &project_dir)
        } else {
            cargo_build_generated(&rs_code, &project_dir, false)
        };

        match result {
            Ok(p) => {
                // Save hash on successful compile
                let _ = std::fs::create_dir_all(&cache);
                let _ = std::fs::write(&hash_file, &code_hash);
                p
            }
            Err(e) => {
                eprintln!("Compile error:\n{}", e);
                return 1;
            }
        }
    };

    let status = Command::new(&actual_bin)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });

    status.code().unwrap_or(1)
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check, false));
}
