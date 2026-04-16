//! `almide docs-gen --check` regression & drift-detection tests.
//!
//! The check runs in the repo root (which is the test's CWD by default
//! in cargo). It must pass on a clean checkout — if this test breaks,
//! either `llms.txt` or a source-of-truth input has drifted and the
//! other side needs a matching update before merge.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn docs_gen_check_passes_on_clean_checkout() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(almide())
        .args(["docs-gen", "--check"])
        .current_dir(repo_root)
        .output()
        .expect("run almide docs-gen --check");
    assert!(
        out.status.success(),
        "docs-gen --check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn docs_gen_without_check_is_not_yet_implemented() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let out = Command::new(almide())
        .args(["docs-gen"])
        .current_dir(repo_root)
        .output()
        .expect("run almide docs-gen");
    assert!(!out.status.success(), "plain `docs-gen` should exit non-zero until regen is implemented");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("only supports `--check`"),
        "expected NYI message, got:\n{}", stderr
    );
}
