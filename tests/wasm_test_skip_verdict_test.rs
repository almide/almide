//! What a skip and a missing host import mean on `almide test`'s wasm lanes.
//!
//! #4325: `almide test <file> --target wasm` on a file the wasm leg could not
//! lower printed `0 passed, 0 failed, 1 skipped` and exited 0 — a negative
//! control that should fail on wasm passed, and any gate reading the exit
//! status had that blind spot. The rule now (docs/specs/cli.md): a skip stays
//! green only when at least one test ran in the same invocation; a run that
//! executed nothing exits 5, whatever kind of skip caused it, and
//! `--allow-no-tests` opts out as for every other zero.
//!
//! #4324: a program importing an `@extern(wasm, module, name)` function the
//! embedded host does not serve failed instantiation with "unknown import",
//! and once the native re-run passed the default lane reported it as a
//! "wasm-only miscompile". It is the refusal `almide run --target wasm` gives
//! the same program: the default lane runs the file on native and says why;
//! `--target wasm` fails it, as `run --target wasm` does.
//!
//! The native leg's verdict on each file is asserted beside the wasm one.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `almide test <args…>` in `dir`, as (exit code, stdout + stderr).
fn almide_test(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(almide_bin()).arg("test").args(args).current_dir(dir).output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"z\"\nversion = \"0.1.0\"\n").expect("manifest");
    for (name, src) in files {
        std::fs::write(dir.path().join(name), src).expect("fixture");
    }
    dir
}

/// A shape the wasm leg refuses on purpose (an unreachable typed hole, C-268).
const WALLED: &str = "fn later(n: Int) -> Int = todo(\"later\")\n\n\
test \"an unreachable typed hole\" {\n  let n = 1\n  if n > 5 then assert_eq(later(n), 0) else ()\n  assert_eq(n, 1)\n}\n";

const DECLARED: &str = "// wasm:skip — pinned by tests/wasm_test_skip_verdict_test.rs\n\
test \"declared\" {\n  assert_eq(1, 1)\n}\n";

const RUNS: &str = "test \"runs\" {\n  assert_eq(2, 2)\n}\n";

/// Imports a host function only a JS host serves; the test reaches the call
/// on a branch it does not take, so the module imports it and native runs.
const NO_HOST: &str = "@extern(wasm, \"obsid\", \"set_state\")\nfn set_state(s: String) -> Unit\n\n\
fn maybe(n: Int) -> Unit = if n > 5 then set_state(\"x\") else ()\n\n\
test \"does not reach the host\" {\n  maybe(1)\n  assert_eq(1, 1)\n}\n";

#[test]
fn a_run_that_skipped_every_file_ran_nothing_and_exits_5() {
    for (name, src, kind) in [("walled_test.almd", WALLED, "1 walled"), ("declared_test.almd", DECLARED, "1 declared")] {
        let dir = project(&[(name, src)]);
        let (code, out) = almide_test(dir.path(), &[name, "--target", "wasm"]);
        assert_eq!(code, 5, "[{name}] a run that executed no test is not a pass:\n{out}");
        assert!(out.contains("no test ran: 1 file(s) were skipped"), "[{name}] the message says why:\n{out}");
        assert!(out.contains(&format!("1 skipped: {kind}")), "[{name}] the summary names the kind:\n{out}");
        assert!(out.contains("0 tests in 1 file, 1 file skipped"), "[{name}] skipped versus run:\n{out}");
        // The opt-out every zero has.
        let (code, out) = almide_test(dir.path(), &[name, "--target", "wasm", "--allow-no-tests"]);
        assert_eq!(code, 0, "[{name}] --allow-no-tests opts out:\n{out}");
        // Native runs the same file's test.
        let (code, out) = almide_test(dir.path(), &[name, "--target", "rust"]);
        assert_eq!(code, 0, "[{name}] native runs it:\n{out}");
        assert!(out.contains("1 test in 1 file"), "[{name}]\n{out}");
    }
}

#[test]
fn a_skip_beside_a_test_that_ran_stays_green() {
    let dir = project(&[("walled_test.almd", WALLED), ("declared_test.almd", DECLARED), ("runs_test.almd", RUNS)]);
    let (code, out) = almide_test(dir.path(), &[".", "--target", "wasm"]);
    assert_eq!(code, 0, "one test ran, so the skips are benign:\n{out}");
    assert!(out.contains("1 passed, 0 failed, 2 skipped: 1 walled, 1 declared"), "{out}");
    assert!(out.contains("1 test in 3 files, 2 files skipped"), "{out}");
}

#[test]
fn a_missing_host_import_is_not_a_miscompile() {
    let dir = project(&[("web_test.almd", NO_HOST)]);
    // Native: the extern is never reached, the test passes.
    let (code, out) = almide_test(dir.path(), &["web_test.almd", "--target", "rust"]);
    assert_eq!(code, 0, "native runs the file:\n{out}");
    // Default lane: the refusal `run --target wasm` gives, then native.
    let (code, out) = almide_test(dir.path(), &["web_test.almd"]);
    assert_eq!(code, 0, "the native fallback runs the file:\n{out}");
    assert!(!out.contains("miscompile") && !out.contains("WASM TRAP"), "nothing was miscompiled:\n{out}");
    assert!(
        out.contains("imports `obsid.set_state`") && out.contains("has no host for it") && out.contains("--host js"),
        "the note names the import and what serves it:\n{out}"
    );
    assert!(out.contains("its tests run on the native fallback"), "{out}");
    assert!(out.contains("1 test in 1 file"), "{out}");
    // `--target wasm`: the verdict `run --target wasm` gives.
    let (code, out) = almide_test(dir.path(), &["web_test.almd", "--target", "wasm"]);
    assert_eq!(code, 1, "the wasm lane cannot run this file:\n{out}");
    assert!(out.contains("FAIL web_test.almd (the embedded wasm host cannot run it)"), "{out}");
    assert!(out.contains("imports `obsid.set_state`") && out.contains("// wasm:skip"), "{out}");
    assert!(!out.contains("miscompile"), "{out}");
    let main_src = NO_HOST.replace("test \"does not reach the host\" {\n  maybe(1)\n  assert_eq(1, 1)\n}\n", "fn main() -> Unit = maybe(1)\n");
    std::fs::write(dir.path().join("main.almd"), main_src).expect("main");
    let out = Command::new(almide_bin()).args(["run", "main.almd", "--target", "wasm"]).current_dir(dir.path()).output().expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("imports `obsid.set_state`") && stderr.contains("has no host for it"), "`run --target wasm` gives the same refusal:\n{stderr}");
}
