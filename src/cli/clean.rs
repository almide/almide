//! `almide clean`: empty every cache and build scratch dir the toolchain owns.

use crate::{project, err};
use super::incremental_cache_dir;

pub fn cmd_clean() {
    let mut cleaned = false;
    cleaned |= remove_cache_dir(&project::cache_dir(), "cache");
    cleaned |= remove_cache_dir(&incremental_cache_dir(), "incremental cache");
    cleaned |= remove_cache_dir(&std::path::PathBuf::from("target/compile"), "compile cache");
    // The native build scratch dirs (#2500): `almide run` / `almide build`'s
    // shared dir (22 GB on the machine that filed it, and the home of the
    // stale rustc incremental session that failed one program shape forever)
    // and `almide build --target cdylib`'s. Each is emptied under its own
    // build lock, so a build in flight there finishes before its dir goes.
    for dir in [super::run::shared_run_project_dir(), std::env::temp_dir().join("almide-build-cdylib")] {
        if dir.is_dir() && clear_locked_dir(&dir, "build cache") {
            err(&format!("Cleaned {}", dir.display()));
            cleaned = true;
        }
    }
    // `almide test`'s per-test-file worker dirs (#2504): one dir per test-file
    // absolute path, 4,510 of them and 39 GB on the machine that filed it.
    // Same rule as above, applied to every worker dir: emptied under its own
    // lock, lockfile kept. The dirs themselves stay (empty), so a builder
    // already blocked on one keeps locking the same file.
    let workers = super::test_scratch::native_worker_cache();
    let emptied = clear_locked_subdirs(&workers, |_| true, "test worker cache");
    if emptied > 0 {
        err(&format!("Cleaned {} ({} test worker dir(s))", workers.display(), emptied));
        cleaned = true;
    }
    // The prebuilt-runtime rlib dirs (#2504): one per runtime source × rustc
    // version × opt level, siblings in the temp dir, each already carrying a
    // build lock of its own. A dir a running build resolved earlier falls
    // back to the self-contained cargo path — slower, never wrong.
    let temp = std::env::temp_dir();
    let is_rtlib = |name: &str| name.starts_with(super::run::RTLIB_DIR_PREFIX);
    let rtlibs = clear_locked_subdirs(&temp, is_rtlib, "runtime rlib cache");
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

/// Delete a whole cache dir if it exists, saying so; a failure ends the run.
/// `what` names the cache in the failure message.
fn remove_cache_dir(dir: &std::path::Path, what: &str) -> bool {
    if !dir.exists() {
        return false;
    }
    std::fs::remove_dir_all(dir)
        .unwrap_or_else(|e| { err(&format!("Failed to clean {}: {}", what, e)); std::process::exit(1); });
    err(&format!("Cleaned {}", dir.display()));
    true
}

/// Empty one build dir under its own lock (`run::clear_build_dir`): true when
/// something was removed. A failure ends the run, naming the cache as `what`.
fn clear_locked_dir(dir: &std::path::Path, what: &str) -> bool {
    match super::run::clear_build_dir(dir) {
        Ok(cleared) => cleared,
        Err(e) => {
            err(&format!("Failed to clean {}: {}", what, e));
            std::process::exit(1);
        }
    }
}

/// Empty every subdirectory of `parent` whose name `keep` accepts, each under
/// its own lock; returns how many had something removed. A missing or
/// unreadable `parent` empties nothing.
fn clear_locked_subdirs(parent: &std::path::Path, keep: impl Fn(&str) -> bool, what: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| keep(&entry.file_name().to_string_lossy()))
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|entry| clear_locked_dir(&entry.path(), what))
        .count()
}
