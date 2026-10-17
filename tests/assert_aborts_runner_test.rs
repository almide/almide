//! `testing.assert_aborts` (ADR-0029, C-384): the failing verdicts and the
//! child's output, on both legs of `almide test`.
//!
//! The passing paths are spec fixtures (`spec/stdlib/coverage_assert_aborts_test.almd`,
//! `spec/stdlib/testing_polarity_test.almd`, `spec/lang/assert_aborts_continues_test.almd`),
//! which the test lane runs on native and wasm. What a fixture cannot pin is a
//! test that is SUPPOSED to fail: each case below is a one-test file whose
//! `assert_aborts` must fail with D5's `expected:` / `found:` block. The native
//! leg runs through `almide test` and its reporter; the wasm leg renders the
//! test module and runs it on the embedded host in process, because the wasm
//! lane's reporter keeps only a failure block's first two lines. The
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

/// `almide test <file> --show-output [--target <leg>]` over `src` (the default
/// lane when `leg` is empty); (exit code, stdout + stderr).
fn run_test(src: &str, leg: &str, env: &[(&str, &str)]) -> (i32, String) {
    let dir = tempfile::Builder::new().prefix("assert_aborts").tempdir().expect("tempdir");
    let file = dir.path().join("case_test.almd");
    std::fs::write(&file, src).expect("write case");
    let mut cmd = Command::new(almide_bin());
    cmd.arg("test").arg(&file).arg("--show-output").current_dir(dir.path());
    if !leg.is_empty() {
        cmd.args(["--target", leg]);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The wasm leg in process: the test module `almide test --target wasm`
/// renders, run on the embedded host; (exit code, stdout + stderr).
fn run_wasm_leg(src: &str) -> (i32, String) {
    let ir = almide::wasm_leg::lower_to_ir_tests_with_deps("case_test.almd", src, &[], None).expect("lower the test module");
    let (bytes, _ops) = almide_wasm::emit_program_with_ops(&ir).expect("emit the test module");
    let r = almide_wasm_run::run_wasm_unbounded(&bytes).expect("run on the embedded host");
    (r.exit, format!("{}{}", r.stdout, r.stderr))
}

/// Both legs over one source.
fn run_both(src: &str) -> [(&'static str, i32, String); 2] {
    let (nc, nout) = run_test(src, "native", &[]);
    let (wc, wout) = run_wasm_leg(src);
    [("native", nc, nout), ("wasm", wc, wout)]
}

fn one_test(body: &str) -> String {
    format!("import process\nimport testing\n\ntest \"case\" {{\n{body}\n}}\n")
}

/// Each failing case: its call, then the `found:` value D5 names for it. The
/// native reporter re-renders the block (`found:    …`), so the values are
/// matched, not the field spacing.
const FAILING: &[(&str, &str)] = &[
    ("  testing.assert_aborts(\"boom\", () => 1 + 1)", "returned normally"),
    ("  testing.assert_aborts(\"boom\", () => panic(\"other\"))", "abort \"PANIC: other\""),
    ("  testing.assert_aborts(\"boom\", () => process.exit(3))", "exit code 3"),
];

#[test]
fn a_failing_assert_aborts_fails_its_test_with_the_expected_found_block() {
    for (call, found) in FAILING {
        for (leg, code, out) in run_both(&one_test(call)) {
            assert_ne!(code, 0, "{leg}: `{call}` must fail its test\n{out}");
            assert!(
                out.contains("case_test.almd:5") || out.contains("at: line 5"),
                "{leg}: `at:` names the call's line for `{call}`\n{out}"
            );
            assert!(out.contains("abort containing \"boom\""), "{leg}: the expected line is missing for `{call}`\n{out}");
            assert!(out.contains(found), "{leg}: `{found}` is missing for `{call}`\n{out}");
        }
    }
}

/// The wasm leg prints D5's block verbatim, header first.
#[test]
fn the_wasm_leg_prints_the_block_in_its_t18_field_order() {
    let (code, out) = run_wasm_leg(&one_test(FAILING[0].0));
    assert_eq!(code, 1, "{out}");
    let block = "Error: assertion failed\n  at: line 5\n  expected: abort containing \"boom\"\n  found: returned normally\n";
    assert!(out.contains(block), "the block is not D5's\n{out}");
}

/// An earlier `eprintln` of the expected text does not satisfy the match:
/// only the text from the last abort header counts (D4).
#[test]
fn only_the_text_from_the_last_abort_header_is_matched() {
    let call = "  testing.assert_aborts(\"boom\", () => {\n    eprintln(\"Error: boom\")\n    panic(\"other\")\n  })";
    for (leg, code, out) in run_both(&one_test(call)) {
        assert_ne!(code, 0, "{leg}: the earlier stderr line must not satisfy the match\n{out}");
        assert!(out.contains("abort \"PANIC: other\""), "{leg}\n{out}");
    }
}

/// The child's stdout and stderr appear only in the failure report (D3, D5):
/// a passing call prints nothing of its child, a failing one shows both.
#[test]
fn the_childs_output_is_shown_only_when_the_call_fails() {
    let passing = "  testing.assert_aborts(\"boom\", () => {\n    println(\"child stdout line\")\n    panic(\"boom\")\n  })";
    let failing = "  testing.assert_aborts(\"boom\", () => {\n    println(\"child stdout line\")\n    eprintln(\"child stderr line\")\n  })";
    for (leg, code, out) in run_both(&one_test(passing)) {
        assert_eq!(code, 0, "{leg}: the passing call fails\n{out}");
        assert!(!out.contains("child stdout line"), "{leg}: a passing child's stdout was forwarded\n{out}");
    }
    for (leg, code, out) in run_both(&one_test(failing)) {
        assert_ne!(code, 0, "{leg}\n{out}");
        assert!(out.contains("child stdout:"), "{leg}: the child's stdout section is missing\n{out}");
        assert!(out.contains("child stdout line"), "{leg}\n{out}");
        assert!(out.contains("child stderr:"), "{leg}: the child's stderr section is missing\n{out}");
        assert!(out.contains("child stderr line"), "{leg}\n{out}");
    }
}

/// On native the failing call fails its own test only: the rest of the file
/// runs (D3, "the rest of the file runs as usual"). The later test fails too,
/// so the report names it only if it ran.
#[test]
fn a_failing_call_does_not_end_the_native_test_binary() {
    let src = "import testing\n\ntest \"fails first\" {\n  testing.assert_aborts(\"boom\", () => 1)\n}\n\ntest \"runs after\" {\n  assert_eq(1, 2)\n}\n";
    let (code, out) = run_test(src, "native", &[]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("fails first"), "{out}");
    assert!(out.contains("runs after"), "the test after the failing one did not run\n{out}");
}

/// A stock runtime has no way to start a fresh instance, so under
/// `ALMIDE_TEST_WASM_RUNNER=wasmtime` a file using `assert_aborts` is a
/// declared limit of the host: the default lane runs it on native, with the
/// reason printed (D3, Q8).
#[test]
fn the_stock_wasm_runner_falls_back_to_native_with_a_reason() {
    let src = one_test("  testing.assert_aborts(\"boom\", () => panic(\"boom\"))");
    let (code, out) = run_test(&src, "", &[("ALMIDE_TEST_WASM_RUNNER", "wasmtime")]);
    assert_eq!(code, 0, "the file passes on its native fallback\n{out}");
    assert!(out.contains("testing.assert_aborts needs the embedded host"), "the fallback does not say why\n{out}");
    assert!(out.contains("1 via native fallback"), "the file did not run on native\n{out}");
}
