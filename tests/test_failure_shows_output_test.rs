//! A failing test's report carries what the test printed (#2538).
//!
//! `almide test` used to render a recognised failure as the assertion alone and
//! drop everything the test wrote with `println` / `eprintln`, so the values a
//! user printed to find the bug never reached them — even with `-v`. Each leg
//! is forced in turn: native (`--target native`), wasm (`--target wasm`), and
//! the default lane (wasm first, native re-run for a failure).
//!
//! stdout is attributed per test (both harnesses bracket each test on stdout);
//! stderr carries no per-test boundary, so it is shown for the whole file, and
//! a passing file stays silent unless `--show-output` is passed.

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

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// The issue's repro, verbatim.
const FAILING: &str = "test \"eprintln inside a failing test\" {\n\
  eprintln(\"DEBUG: value is 42\")\n\
  println(\"STDOUT: value is 42\")\n\
  assert_eq(1, 2)\n\
}\n";

/// An assert in a plain `fn` ends the process (the T18 abort) instead of
/// unwinding — the failing test is the one that never printed its verdict.
const ABORTING: &str = "fn check(x: Int) -> Unit = {\n\
  assert_eq(x, 3)\n\
}\n\
\n\
test \"helper aborts\" {\n\
  println(\"STDOUT: before abort\")\n\
  eprintln(\"DEBUG: before abort\")\n\
  check(4)\n\
}\n";

const PASSING: &str = "test \"quiet pass\" {\n\
  println(\"PASSING-STDOUT\")\n\
  eprintln(\"PASSING-STDERR\")\n\
  assert_eq(1, 1)\n\
}\n";

fn run(files: &[(&str, &str)], args: &[&str]) -> (i32, String) {
    let dir = tempfile::Builder::new().prefix("t2538").tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"z\"\nversion = \"0.1.0\"\n")
        .expect("manifest");
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).expect("fixture");
    }
    let out = Command::new(almide_bin())
        .arg("test")
        .args(args)
        .current_dir(dir.path())
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn legs() -> Vec<Vec<&'static str>> {
    let mut legs = vec![vec!["--target", "native"]];
    if wasmtime_available() {
        legs.push(vec!["--target", "wasm"]);
        legs.push(vec![]);
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg and the default lane are not exercised");
    }
    legs
}

#[test]
fn a_failing_test_shows_its_stdout_and_stderr_on_every_leg() {
    for leg in legs() {
        let mut args = vec!["e_test.almd"];
        args.extend(&leg);
        let (code, out) = run(&[("e_test.almd", FAILING)], &args);
        assert_eq!(code, 1, "{leg:?}: the test fails\n{out}");
        assert!(out.contains("STDOUT: value is 42"), "{leg:?}: the test's stdout is missing\n{out}");
        assert!(out.contains("DEBUG: value is 42"), "{leg:?}: the test's stderr is missing\n{out}");
    }
}

#[test]
fn an_aborting_test_shows_what_it_printed_before_the_abort() {
    for leg in legs() {
        let mut args = vec!["a_test.almd"];
        args.extend(&leg);
        let (code, out) = run(&[("a_test.almd", ABORTING)], &args);
        assert_eq!(code, 1, "{leg:?}: the test fails\n{out}");
        assert!(out.contains("STDOUT: before abort"), "{leg:?}: the test's stdout is missing\n{out}");
        assert!(out.contains("DEBUG: before abort"), "{leg:?}: the test's stderr is missing\n{out}");
    }
}

#[test]
fn a_passing_file_is_silent_unless_show_output() {
    for leg in legs() {
        let mut args = vec!["p_test.almd"];
        args.extend(&leg);
        let (code, out) = run(&[("p_test.almd", PASSING)], &args);
        assert_eq!(code, 0, "{leg:?}\n{out}");
        assert!(!out.contains("PASSING-"), "{leg:?}: a passing test's output leaked\n{out}");

        args.push("--show-output");
        let (code, out) = run(&[("p_test.almd", PASSING)], &args);
        assert_eq!(code, 0, "{leg:?}\n{out}");
        assert!(out.contains("PASSING-STDOUT"), "{leg:?}: --show-output dropped stdout\n{out}");
        assert!(out.contains("PASSING-STDERR"), "{leg:?}: --show-output dropped stderr\n{out}");
    }
}
