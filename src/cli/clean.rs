//! `almide clean`: empty every cache and build scratch dir the toolchain owns.

use crate::{project, err};
use super::incremental_cache_dir;

pub fn cmd_clean() {
    let mut cleaned = false;
    let dep_cache = project::cache_dir();
    if dep_cache.exists() {
        std::fs::remove_dir_all(&dep_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", dep_cache.display()));
        cleaned = true;
    }
    let inc_cache = incremental_cache_dir();
    if inc_cache.exists() {
        std::fs::remove_dir_all(&inc_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean incremental cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", inc_cache.display()));
        cleaned = true;
    }
    let compile_cache = std::path::PathBuf::from("target/compile");
    if compile_cache.exists() {
        std::fs::remove_dir_all(&compile_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean compile cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", compile_cache.display()));
        cleaned = true;
    }
    // The native build scratch dirs (#2500): `almide run` / `almide build`'s
    // shared dir (22 GB on the machine that filed it, and the home of the
    // stale rustc incremental session that failed one program shape forever)
    // and `almide build --target cdylib`'s. Each is emptied under its own
    // build lock, so a build in flight there finishes before its dir goes.
    for dir in [super::run::shared_run_project_dir(), std::env::temp_dir().join("almide-build-cdylib")] {
        if !dir.is_dir() {
            continue;
        }
        match super::run::clear_build_dir(&dir) {
            Ok(true) => {
                err(&format!("Cleaned {}", dir.display()));
                cleaned = true;
            }
            Ok(false) => {}
            Err(e) => {
                err(&format!("Failed to clean build cache: {}", e));
                std::process::exit(1);
            }
        }
    }
    // `almide test`'s per-test-file worker dirs (#2504): one dir per test-file
    // absolute path, 4,510 of them and 39 GB on the machine that filed it.
    // Same rule as above, applied to every worker dir: emptied under its own
    // lock, lockfile kept. The dirs themselves stay (empty), so a builder
    // already blocked on one keeps locking the same file.
    let workers = super::test_scratch::native_worker_cache();
    let mut emptied = 0usize;
    if let Ok(entries) = std::fs::read_dir(&workers) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            match super::run::clear_build_dir(&entry.path()) {
                Ok(true) => emptied += 1,
                Ok(false) => {}
                Err(e) => {
                    err(&format!("Failed to clean test worker cache: {}", e));
                    std::process::exit(1);
                }
            }
        }
    }
    if emptied > 0 {
        err(&format!("Cleaned {} ({} test worker dir(s))", workers.display(), emptied));
        cleaned = true;
    }
    // The prebuilt-runtime rlib dirs (#2504): one per runtime source × rustc
    // version × opt level, siblings in the temp dir, each already carrying a
    // build lock of its own. A dir a running build resolved earlier falls
    // back to the self-contained cargo path — slower, never wrong.
    let temp = std::env::temp_dir();
    let mut rtlibs = 0usize;
    if let Ok(entries) = std::fs::read_dir(&temp) {
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with(super::run::RTLIB_DIR_PREFIX)
                || !entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
            {
                continue;
            }
            match super::run::clear_build_dir(&entry.path()) {
                Ok(true) => rtlibs += 1,
                Ok(false) => {}
                Err(e) => {
                    err(&format!("Failed to clean runtime rlib cache: {}", e));
                    std::process::exit(1);
                }
            }
        }
    }
    if rtlibs > 0 {
        err(&format!(
            "Cleaned {}/{}* ({} runtime rlib dir(s))",
            temp.display(),
            super::run::RTLIB_DIR_PREFIX,
            rtlibs
        ));
        cleaned = true;
    }
    if !cleaned {
        err(&format!("No cache to clean"));
    }
}
