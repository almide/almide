//! An imported module's own type errors must reach the importer (#862).
//!
//! `canonicalize_program` registers a user module's SIGNATURES but never
//! inferred its bodies on the check path, and the build drivers appended
//! `infer_module`'s diagnostics to a `checker.diagnostics` nobody read. The
//! result: `almide check`/`build`/`test` on a file importing a module with an
//! E006 (effect fn called from a pure fn) all reported success. Found in the
//! wild — `base64`'s `decode_chunks` carried that error for weeks while its own
//! test suite stayed green.
//!
//! The locks here:
//!   1. `almide check` on the IMPORTER fails, citing the module's file.
//!   2. `almide build` on the importer fails the same way.
//!   3. `almide test` on an importing test file fails (it used to print
//!      "All 1 test file(s) passed").
//!   4. A CLEAN module still checks, builds, and tests green — the gate is not
//!      a blanket rejection of imports.

use std::path::Path;
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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// (combined output, exit code) of an `almide …` invocation run inside `dir`.
fn run_in(dir: &Path, args: &[&str]) -> (String, i32) {
    let output = Command::new(almide_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (combined, output.status.code().unwrap_or(-1))
}

/// A scratch dir holding `<name>mod.almd` (the imported module) plus the two
/// importers used by every case below.
fn scratch(name: &str, module_body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue862-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir scratch");
    std::fs::write(dir.join("themod.almd"), module_body).expect("write module");
    std::fs::write(
        dir.join("importer.almd"),
        "import themod\n\neffect fn main() -> Unit = {\n  println(\"hi\")\n}\n",
    )
    .expect("write importer");
    std::fs::write(
        dir.join("importer_test.almd"),
        "import themod\n\ntest \"uses good part\" {\n  assert_eq(1, 1)\n}\n",
    )
    .expect("write importer test");
    dir
}

const BAD_MODULE: &str = "\
effect fn boom() -> Result[Int, String] = ok(1)
fn pure_caller() -> Result[Int, String] = boom()
";

const GOOD_MODULE: &str = "\
effect fn boom() -> Result[Int, String] = ok(1)
effect fn caller() -> Result[Int, String] = boom()
";

#[test]
fn check_reports_an_imported_modules_type_error() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch("check", BAD_MODULE);
    let (out, code) = run_in(&dir, &["check", "importer.almd"]);
    assert_ne!(code, 0, "check must fail on a bad imported module:\n{out}");
    assert!(
        out.contains("E006"),
        "the module's own diagnostic must be shown:\n{out}"
    );
    assert!(
        out.contains("themod.almd"),
        "the error must be attributed to the MODULE's file:\n{out}"
    );
}

#[test]
fn build_reports_an_imported_modules_type_error() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch("build", BAD_MODULE);
    let (out, code) = run_in(&dir, &["build", "importer.almd", "-o", "out"]);
    assert_ne!(code, 0, "build must fail on a bad imported module:\n{out}");
    assert!(out.contains("E006"), "expected the module's E006:\n{out}");
}

#[test]
fn test_fails_when_an_imported_module_has_type_errors() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch("test", BAD_MODULE);
    let (out, code) = run_in(&dir, &["test", "importer_test.almd"]);
    assert_ne!(
        code, 0,
        "almide test used to print 'All 1 test file(s) passed' here:\n{out}"
    );
    assert!(out.contains("E006"), "expected the module's E006:\n{out}");
}

#[test]
fn a_clean_imported_module_still_passes_every_leg() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch("clean", GOOD_MODULE);
    let (out, code) = run_in(&dir, &["check", "importer.almd"]);
    assert_eq!(code, 0, "clean module must check green:\n{out}");
    let (out, code) = run_in(&dir, &["test", "importer_test.almd"]);
    assert_eq!(code, 0, "clean module must test green:\n{out}");
}
