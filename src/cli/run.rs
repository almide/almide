use std::process::Command;
use crate::{compile, try_compile, find_rustc};
use super::{hash64, incremental_cache_dir};

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool) -> i32 {
    let rs_code = match try_compile(file, no_check) {
        Ok(code) => code,
        Err(_) => return 1,
    };

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
