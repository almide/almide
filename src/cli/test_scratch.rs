//! Per-invocation scratch root for `almide test` (#1877).
//!
//! The harness used to key its scratch artifacts — the wasm module it hands
//! to wasmtime and the native worker's cargo/rustc project dir — on the test
//! file's RELATIVE path, under one shared `$TMPDIR/almide-wasm-test` /
//! `$TMPDIR/almide-test`. Two invocations on same-named files from different
//! directories (`cd a && almide test x_test.almd` racing `cd b && almide test
//! x_test.almd`, or two parallel runs on copies of one file) therefore wrote
//! the SAME `x_test_almd.wasm`: whichever wrote first launched wasmtime on the
//! other's module and reported the other file's verdict — a failing file
//! printed "All 1 test file(s) passed" (reproduced 1/30 rounds on 0.61.1).
//!
//! Every artifact is now named by the hash of the file's ABSOLUTE path, so
//! a same-named sibling can never share a path, and the wasm module lives
//! under a root unique to this process (`almide-test-<pid>-<nonce>`), so
//! neither can another invocation. That root is removed when the command
//! finishes — including the `process::exit(1)` failure paths, which skip
//! destructors — unless `ALMIDE_KEEP_SCRATCH=1`, which keeps it and prints
//! where it is.
//!
//! The native worker dir stays PERSISTENT (`$TMPDIR/almide-test/native/`):
//! it is a build cache — its content-hashed binary names and per-dir flock
//! already make concurrent use of one dir correct (`build_native_cached`) —
//! and moving it under the per-run root cold-builds every native-fallback
//! file on every run: measured 14 s → 31 s for a warm `almide test spec/`.

use std::path::{Path, PathBuf};

pub(crate) struct TestScratch {
    /// Per-run root: `<temp>/almide-test-<pid>-<nonce>/`, removed on finish.
    root: PathBuf,
    /// Persistent native build cache: `<temp>/almide-test/native/`.
    native_cache: PathBuf,
    keep: bool,
}

impl TestScratch {
    pub(crate) fn new() -> Self {
        // pid + wall clock + an in-process counter: distinct across parallel
        // processes AND across two roots created in one process.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            super::hash64(format!("{}:{}:{}", std::process::id(), nanos, seq).as_bytes())
        };
        let root = std::env::temp_dir().join(format!("almide-test-{}-{:016x}", std::process::id(), nonce));
        std::fs::create_dir_all(root.join("wasm")).ok();
        let native_cache = std::env::temp_dir().join("almide-test").join("native");
        let keep = std::env::var_os("ALMIDE_KEEP_SCRATCH").is_some_and(|v| !v.is_empty() && v != "0");
        TestScratch { root, native_cache, keep }
    }

    /// The scratch `.wasm` module for one test file on the wasm leg:
    /// `<root>/wasm/<stem>-<abs-path hash>.wasm`.
    pub(crate) fn wasm_module_path(&self, test_file: &str) -> PathBuf {
        self.root.join("wasm").join(format!("{}.wasm", Self::file_key(test_file)))
    }

    /// The native worker's project dir for one test file (its own
    /// `src/main.rs`, so cold rustc builds parallelize instead of serializing
    /// on the shared dir's build lock), in the persistent cache:
    /// `<temp>/almide-test/native/<stem>-<abs-path hash>`.
    pub(crate) fn native_worker_dir(&self, test_file: &str) -> PathBuf {
        self.native_cache.join(Self::file_key(test_file))
    }

    /// `<stem>-<hash of the absolute path>`: the stem keeps a kept scratch
    /// dir readable, the hash is what makes two same-named files distinct.
    fn file_key(test_file: &str) -> String {
        let path = Path::new(test_file);
        let abs = std::fs::canonicalize(path)
            .unwrap_or_else(|_| std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf()));
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
        format!("{}-{:016x}", stem, super::hash64(abs.to_string_lossy().as_bytes()))
    }

    /// Remove the per-run root (or report it when kept). Idempotent; called
    /// explicitly before every `process::exit`, and again from `Drop` on the
    /// normal return path.
    pub(crate) fn finish(&self) {
        if self.keep {
            crate::err(&format!(
                "scratch kept (ALMIDE_KEEP_SCRATCH): {} (native build cache: {})",
                self.root.display(),
                self.native_cache.display()
            ));
        } else {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

impl Drop for TestScratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TestScratch;

    #[test]
    fn same_name_different_directories_never_share_a_path() {
        let s = TestScratch::new();
        let a = s.wasm_module_path("a/x_test.almd");
        let b = s.wasm_module_path("b/x_test.almd");
        assert_ne!(a, b);
        assert_ne!(s.native_worker_dir("a/x_test.almd"), s.native_worker_dir("b/x_test.almd"));
        assert!(a.starts_with(s.root.join("wasm")));
        // The native worker dir is the persistent cache, keyed on the
        // absolute path — the same across two runs of one file.
        let t = TestScratch::new();
        assert_eq!(s.native_worker_dir("a/x_test.almd"), t.native_worker_dir("a/x_test.almd"));
        assert!(!s.native_worker_dir("a/x_test.almd").starts_with(&s.root));
    }

    #[test]
    fn two_runs_never_share_a_root() {
        let a = TestScratch::new();
        let b = TestScratch::new();
        assert_ne!(a.root, b.root);
        assert!(a.root.is_dir() && b.root.is_dir());
        let root = a.root.clone();
        a.finish();
        assert!(!root.exists());
    }
}
