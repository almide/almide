//! `testing.assert_aborts` (ADR-0029, C-384): the failing verdicts and the
//! child's output, on both legs of `almide test`.
//!
//! The passing paths are spec fixtures (`spec/stdlib/coverage_assert_aborts_test.almd`,
//! `spec/stdlib/testing_polarity_test.almd`, `spec/lang/assert_aborts_continues_test.almd`),
//! which the test lane runs on native and wasm. What a fixture cannot pin is a
//! test that is SUPPOSED to fail: each case below is a one-test file whose
//! `assert_aborts` must fail, with D5's `expected:` / `found:` block. The
//! check-time rejections (E098 / E099) are diagnostics fixtures under
//! `tests/diagnostics/e098-*` and `tests/diagnostics/e099-*`.

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

/// `almide test <file> --target <leg>` over `src`; (exit code, stdout + stderr).
fn run_test(src: &str, leg: &str, env: &[(&str, &str)]) -> (i32, String) {
    let dir = tempfile::Builder::new().prefix("assert_aborts").tempdir().expect("tempdir");
    let file = dir.path().join("case_test.almd");
    std::fs::write(&file, src).expect("write case");
    let mut cmd = Command::new(almide_bin());
    cmd.arg("test").arg(&file).args(["--target", leg]).current_dir(dir.path());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn one_test(body: &str) -> String {
    format!("import process\nimport testing\n\ntest \"case\" {{\n{body}\n}}\n")
}

const LEGS: [&str; 2] = ["native", "wasm"];

/// Each failing case: its call, then the `found:` line D5 names for it.
const FAILING: &[(&str, &str)] = &[
    ("  testing.assert_aborts(\"boom\", () => 1 + 1)", "found: returned normally"),
    ("  testing.assert_aborts(\"boom\", () => panic(\"other\"))", "found: abort \"PANIC: other\""),
    ("  testing.assert_aborts(\"boom\", () => process.exit(3))", "found: exit code 3"),
];

#[test]
fn a_failing_assert_aborts_fails_its_test_with_the_expected_found_block() {
    for leg in LEGS {
        for (call, found) in FAILING {
            let (code, out) = run_test(&one_test(call), leg, &[]);
            assert_ne!(code, 0, "{leg}: `{call}` must fail its test\n{out}");
            assert!(out.contains("Error: assertion failed"), "{leg}: the D5 header is missing for `{call}`\n{out}");
            assert!(out.contains("at: line 5"), "{leg}: `at:` names the call's line for `{call}`\n{out}");
            assert!(
                out.contains("expected: abort containing \"boom\""),
                "{leg}: the expected line is missing for `{call}`\n{out}"
            );
            assert!(out.contains(found), "{leg}: `{found}` is missing for `{call}`\n{out}");
        }
    }
}

/// An earlier `eprintln` of the expected text does not satisfy the match:
/// only the text from the last abort header counts (D4).
#[test]
fn only_the_text_from_the_last_abort_header_is_matched() {
    let call = "  testing.assert_aborts(\"boom\", () => {\n    eprintln(\"Error: boom\")\n    panic(\"other\")\n  })";
    for leg in LEGS {
        let (code, out) = run_test(&one_test(call), leg, &[]);
        assert_ne!(code, 0, "{leg}: the earlier stderr line must not satisfy the match\n{out}");
        assert!(out.contains("found: abort \"PANIC: other\""), "{leg}\n{out}");
    }
}

/// The child's stdout and stderr appear only in the failure report (D3, D5):
/// a passing call prints nothing of its child, a failing one shows both.
#[test]
fn the_childs_output_is_shown_only_when_the_call_fails() {
    let passing = "  testing.assert_aborts(\"boom\", () => {\n    println(\"child stdout line\")\n    panic(\"boom\")\n  })";
    let failing = "  testing.assert_aborts(\"boom\", () => {\n    println(\"child stdout line\")\n    eprintln(\"child stderr line\")\n  })";
    for leg in LEGS {
        let (code, out) = run_test(&format!("{}\n", one_test(passing)), leg, &[]);
        assert_eq!(code, 0, "{leg}: the passing call fails\n{out}");
        assert!(!out.contains("child stdout line"), "{leg}: a passing child's stdout was forwarded\n{out}");
        let (code, out) = run_test(&one_test(failing), leg, &[]);
        assert_ne!(code, 0, "{leg}\n{out}");
        assert!(out.contains("child stdout:"), "{leg}: the child's stdout section is missing\n{out}");
        assert!(out.contains("child stdout line"), "{leg}\n{out}");
        assert!(out.contains("child stderr:"), "{leg}: the child's stderr section is missing\n{out}");
        assert!(out.contains("child stderr line"), "{leg}\n{out}");
    }
}

/// On native the failing call fails its own test only: the rest of the file
/// runs (D3, "the rest of the file runs as usual").
#[test]
fn a_failing_call_does_not_end_the_native_test_binary() {
    let src = "import testing\n\ntest \"fails\" {\n  testing.assert_aborts(\"boom\", () => 1)\n}\n\ntest \"runs after\" {\n  println(\"second test ran\")\n}\n";
    let (code, out) = run_test(src, "native", &[]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("second test ran"), "the test after the failing one did not run\n{out}");
}

/// A stock runtime has no way to start a fresh instance, so under
/// `ALMIDE_TEST_WASM_RUNNER=wasmtime` a file using `assert_aborts` is a
/// declared wall that runs on native, with the reason printed (D3, Q8).
#[test]
fn the_stock_wasm_runner_walls_to_native_with_a_reason() {
    let src = one_test("  testing.assert_aborts(\"boom\", () => panic(\"boom\"))");
    let (code, out) = run_test(&src, "wasm", &[("ALMIDE_TEST_WASM_RUNNER", "wasmtime"), ("ALMIDE_WALL_REASON", "1")]);
    assert_eq!(code, 0, "the file passes on its native fallback\n{out}");
    assert!(out.contains("assert_aborts"), "the wall does not say why\n{out}");
}
