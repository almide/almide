//! What `almide test` counts as a passed file, on every lane.
//!
//! #4255: the exit status is not the verdict. `process.exit(0)` inside a test
//! ends the process with 0 before the remaining tests run, and the runner used
//! to read that 0 as "every test in the file passed" — a later failing test,
//! or one that never ran, turned green. The native leg now requires libtest's
//! per-test completion record (its `--logfile`, not the program's stdout) for
//! every selected test; the wasm leg's embedded host requires the runner
//! `main` to have returned.
//!
//! #4264: `--run <pattern>` is matched against each test's own name, as
//! written in `test "…"` — spaces, punctuation and non-ASCII scripts included
//! — and a pattern that selects nothing exits 5 instead of passing.
//!
//! Each case runs on three lanes: the default (wasm first, native re-run of a
//! failure), `--target wasm`, and the native harness alone (a `// wasm:skip`
//! file, the route a wasm wall takes).
//!
//! Runs the `almide` binary (`ALMIDE_BIN`, else the one cargo built).

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `almide test <name> <extra…>` over `source` in a fresh project dir, as
/// (exit code, stdout + stderr).
fn almide_test(name: &str, source: &str, extra: &[&str]) -> (i32, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"z\"\nversion = \"0.1.0\"\n").expect("manifest");
    std::fs::write(dir.path().join(name), source).expect("write fixture");
    let out = Command::new(almide_bin())
        .arg("test")
        .arg(name)
        .args(extra)
        .current_dir(dir.path())
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// The three lanes: (label, source prefix, extra args).
const LANES: [(&str, &str, &[&str]); 3] = [
    ("default", "", &[]),
    ("wasm", "", &["--target", "wasm"]),
    ("native", "// wasm:skip\n", &[]),
];

/// The issue's probe: the first test exits 0, the second fails.
const EXIT_ZERO_FIRST: &str = "import process\n\n\
test \"a: exits zero midway\" {\n  process.exit(0)\n}\n\n\
test \"b: plainly fails\" {\n  assert_eq(1, 2)\n}\n";

/// The exit in the LAST test, after one that passed: nothing after it fails,
/// so only a per-test completion record can tell.
const EXIT_ZERO_LAST: &str = "import process\n\n\
test \"a passes\" {\n  assert_eq(1, 1)\n}\n\n\
test \"b exits zero\" {\n  process.exit(0)\n}\n";

#[test]
fn process_exit_zero_inside_a_test_fails_the_file_on_every_lane() {
    for (lane, prefix, extra) in LANES {
        for (case, body, unfinished) in [
            ("first", EXIT_ZERO_FIRST, "a: exits zero midway"),
            ("last", EXIT_ZERO_LAST, "b exits zero"),
        ] {
            let src = format!("{prefix}{body}");
            let (code, out) = almide_test(&format!("exit0_{case}_test.almd"), &src, extra);
            assert_ne!(code, 0, "[{lane}/{case}] an exit(0) mid-run must not pass the file:\n{out}");
            assert!(!out.contains("All 1 test file(s) passed"), "[{lane}/{case}]\n{out}");
            assert!(out.contains(unfinished), "[{lane}/{case}] the report names `{unfinished}`:\n{out}");
        }
    }
}

/// The negative control: the same files without the exit pass (or fail on
/// their own assertion) exactly as before.
#[test]
fn a_file_that_runs_to_its_end_still_passes() {
    let src = "test \"a passes\" {\n  assert_eq(1, 1)\n}\n\ntest \"b passes\" {\n  assert_eq(2, 2)\n}\n";
    for (lane, prefix, extra) in LANES {
        let (code, out) = almide_test("plain_test.almd", &format!("{prefix}{src}"), extra);
        assert_eq!(code, 0, "[{lane}]\n{out}");
        assert!(out.contains("2 tests in 1 file"), "[{lane}]\n{out}");
    }
}

const NAMED: &str = "test \"record with secret fails\" {\n  assert_eq(1, 2)\n}\n\n\
test \"other passes\" {\n  assert_eq(1, 1)\n}\n\n\
test \"値を取る旗に値が無ければ断る\" {\n  assert_eq(3, 3)\n}\n";

#[test]
fn run_selects_a_test_by_its_own_name_on_every_lane() {
    for (lane, prefix, extra) in LANES {
        let src = format!("{prefix}{NAMED}");
        let with = |pattern: &str| {
            let args: Vec<&str> = ["--run", pattern].into_iter().chain(extra.iter().copied()).collect();
            almide_test("named_test.almd", &src, &args)
        };
        // A name with spaces selects exactly that (failing) test.
        let (code, out) = with("record with secret");
        assert_eq!(code, 1, "[{lane}] the selected test fails:\n{out}");
        assert!(out.contains("record with secret fails"), "[{lane}]\n{out}");
        // A non-ASCII name selects exactly that test.
        let (code, out) = with("値が無ければ");
        assert_eq!(code, 0, "[{lane}]\n{out}");
        assert!(out.contains("1 test in 1 file (2 filtered out)"), "[{lane}]\n{out}");
        // A pattern matching nothing fails the run (5, as "nothing to run").
        let (code, out) = with("zzz-nomatch");
        assert_eq!(code, 5, "[{lane}] a pattern selecting nothing is not a pass:\n{out}");
        assert!(out.contains("matched no test"), "[{lane}]\n{out}");
    }
}

/// `--json` (native) reads the same selection.
#[test]
fn json_selects_by_the_source_name() {
    let (code, out) = almide_test("named_test.almd", NAMED, &["--json", "--run", "値が無ければ"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"tests\":1"), "{out}");
    assert!(out.contains("\"filtered_out\":2"), "{out}");
}
