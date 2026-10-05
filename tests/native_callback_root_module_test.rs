//! `almide test` in a package whose `native/*.rs` calls back into the
//! package's own Almide modules (#3424).
//!
//! The native code spells the names a consumer of the package links
//! (`crate::almide_rt_tf_0v0_entry` for the root module, `..._1extra_...` for
//! a sub-module). Two defects met in one transcript:
//!
//! 1. A test that FAILED on the wasm leg was re-run natively (to tell a
//!    failing test from a wasm miscompile), and when that re-run could not
//!    BUILD, its rustc error replaced the failed assertion as the file's
//!    report.
//! 2. The native build of a test file copied `native/` into the crate but not
//!    the modules it calls: the root module was loaded only when the test
//!    file imported it, and the package's own modules lowered unversioned —
//!    so every native build of the package failed with E0425.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

const HOST_RS: &str = "\
pub fn call_entry(x: i64) -> i64 { crate::almide_rt_tf_0v0_entry(x) }
pub fn call_triple(x: i64) -> i64 { crate::almide_rt_tf_0v0_1extra_triple(x) }
";

/// The issue's package: a root module and two sub-modules the test file does
/// not import (`extra` is reached only from `native/host.rs`), and one test
/// file importing `self.calc`.
fn package(tag: &str, tests: &str, host_rs: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3424-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("almide.toml"), "[package]\nname = \"tf\"\nversion = \"0.1.0\"\n");
    write(root.join("src/mod.almd"), "pub fn entry(x: Int) -> Int = x + 1\n");
    write(root.join("src/calc.almd"), "pub fn double(x: Int) -> Int = x * 2\n");
    write(root.join("src/extra.almd"), "pub fn triple(x: Int) -> Int = x * 3\n");
    write(root.join("native/host.rs"), host_rs);
    write(root.join("src/double_test.almd"), &format!("import self.calc as c\n{tests}"));
    root
}

const PASSING: &str = "test \"passes\" { assert(c.double(2) == 4) }\n";
const FAILING: &str = "test \"passes\" { assert(c.double(2) == 4) }\ntest \"fails\" { assert(c.double(2) == 5) }\n";

/// `almide test src/double_test.almd [extra...]` in `root`: (success, stderr).
fn almide_test(root: &Path, extra: &[&str]) -> (bool, String) {
    let out = Command::new(almide_bin())
        .current_dir(root)
        .args(["test", "src/double_test.almd"])
        .args(extra)
        .output()
        .expect("spawn almide test");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn assert_reports_the_failed_assertion(ok: bool, out: &str, lane: &str) {
    assert!(!ok, "{lane}: a failing test passed the run:\n{out}");
    assert!(out.contains("FAILED: src/double_test.almd"), "{lane}: no FAILED line:\n{out}");
    assert!(out.contains("fails"), "{lane}: the failing test is not named:\n{out}");
    assert!(out.contains("assertion failed"), "{lane}: the assertion is not reported:\n{out}");
    assert!(!out.contains("E0425"), "{lane}: a rustc error stood in for the verdict:\n{out}");
}

#[test]
fn a_failing_test_reports_its_assertion_not_a_rustc_error() {
    if !available() {
        return;
    }
    let root = package("fail", FAILING, HOST_RS);
    let (ok, out) = almide_test(&root, &[]);
    assert_reports_the_failed_assertion(ok, &out, "default lane");
    let (ok, out) = almide_test(&root, &["--target", "rust"]);
    assert_reports_the_failed_assertion(ok, &out, "--target rust");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_native_lane_builds_the_modules_the_native_code_calls() {
    if !available() {
        return;
    }
    let root = package("pass", PASSING, HOST_RS);
    let (ok, out) = almide_test(&root, &["--target", "rust"]);
    assert!(ok, "--target rust: the native build of a passing test failed:\n{out}");
    assert!(!out.contains("E0425"), "--target rust: rustc error:\n{out}");
    let (ok, out) = almide_test(&root, &[]);
    assert!(ok, "default lane: a passing test failed:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_package_whose_native_code_does_not_call_back_is_unchanged() {
    if !available() {
        return;
    }
    let root = package("plain", PASSING, "pub fn number() -> i64 { 7 }\n");
    for lane in [&[][..], &["--target", "rust"][..]] {
        let (ok, out) = almide_test(&root, lane);
        assert!(ok, "{lane:?}: a passing test failed:\n{out}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Defect 1 on its own: whatever stops the native re-run from building, a
/// test that ran on wasm and failed is reported as that failure, with the
/// build error after it as a note.
#[test]
fn a_native_rerun_that_cannot_build_does_not_replace_the_wasm_verdict() {
    if !available() {
        return;
    }
    let host = format!("{HOST_RS}pub fn broken() -> i64 {{ crate::no_such_fn() }}\n");
    let root = package("unbuilt", FAILING, &host);
    let (ok, out) = almide_test(&root, &[]);
    assert!(!ok, "a failing test passed the run:\n{out}");
    assert!(out.contains("FAILED: src/double_test.almd"), "no FAILED line:\n{out}");
    assert!(out.contains("assertion failed"), "the assertion is not reported:\n{out}");
    let verdict = out.find("assertion failed").unwrap();
    let note = out.find("could not build").unwrap_or_else(|| panic!("the native build failure is not named:\n{out}"));
    assert!(verdict < note, "the build error comes before the verdict:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}
