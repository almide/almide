//! #3489: nothing a program prints is read back as the harness's verdict.
//!
//! Two places read facts out of the program's own output by line spelling:
//! the wasm test leg counted passes by the `ok` lines on stdout (a test that
//! printed `ok` was a test), and `almide run --time-report` took the stderr
//! line starting `__ALMD_PROBE ` as the meter reading (and dropped it from the
//! program's stderr). The count now comes from the set of tests the runner is
//! synthesized over, and the meter reading travels on its own channel.
//!
//! Runs the `almide` binary (`ALMIDE_BIN`, else `target/release/almide`).

use std::path::Path;
use std::process::{Command, Output};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide").to_string_lossy().into_owned()
}

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide_bin()).args(args).current_dir(dir).output().expect("spawn almide")
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

const PRINTS_OK: &str = "test \"a\" {\n  println(\"ok\")\n  println(\"ok\")\n  assert_eq(1, 1)\n}\n\n\
test \"b\" {\n  assert_eq(2, 2)\n}\n";

/// The issue's repro: the human summary said `4 tests` while `--json` said 2.
#[test]
fn a_test_printing_ok_is_not_counted_as_a_test() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ok_test.almd"), PRINTS_OK).unwrap();
    let human = almide(dir.path(), &["test", "ok_test.almd"]);
    let out = text(&human);
    assert!(human.status.success(), "{out}");
    assert!(out.contains("1 via WASM"), "the file ran on the wasm leg, the leg under test:\n{out}");
    assert!(out.contains("2 tests in 1 file"), "two tests, whatever they print:\n{out}");

    let json = almide(dir.path(), &["test", "ok_test.almd", "--json"]);
    assert!(String::from_utf8_lossy(&json.stdout).contains("\"tests\":2"), "{}", text(&json));
}

/// The `--run` arithmetic read off the same count: a selected test printing
/// `ok` lowered the filtered-out number.
#[test]
fn a_selected_test_printing_ok_does_not_change_the_filtered_count() {
    // Labels no part of the `__test_almd_` prefix spells, so `--run` picks one.
    let src = PRINTS_OK.replace("test \"a\"", "test \"printer\"").replace("test \"b\"", "test \"quiet\"");
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ok_test.almd"), src).unwrap();
    let run = almide(dir.path(), &["test", "ok_test.almd", "--run", "printer"]);
    let out = text(&run);
    assert!(run.status.success(), "{out}");
    assert!(out.contains("1 via WASM"), "{out}");
    assert!(out.contains("1 test in 1 file (1 filtered out)"), "`printer` runs and `quiet` is filtered out:\n{out}");
}

const PROBE_SPELLING: &str = "effect fn main() -> Unit = {\n  eprintln(\"__ALMD_PROBE 999999999\")\n  eprintln(\"hello\")\n  println(\"out\")\n}\n";

/// `--time-report` must leave the program's stderr as the program wrote it.
#[test]
fn time_report_keeps_the_program_s_own_probe_spelled_line() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("p.almd"), PROBE_SPELLING).unwrap();
    let plain = almide(dir.path(), &["run", "p.almd"]);
    let report = almide(dir.path(), &["run", "--time-report", "p.almd"]);
    assert!(plain.status.success() && report.status.success(), "{}{}", text(&plain), text(&report));
    assert_eq!(report.stdout, plain.stdout, "stdout is the program's");
    let stderr = String::from_utf8_lossy(&report.stderr);
    let (program, meter) = stderr.split_once("time: ").unwrap_or_else(|| panic!("dual-time line missing:\n{stderr}"));
    assert_eq!(program, String::from_utf8_lossy(&plain.stderr), "the program's stderr, byte for byte, before the report line");
    assert!(meter.contains("deterministic (≈"), "the meter was read from the probe, not refused:\n{stderr}");
    // 999999999 units × CM-1 (3 ns) would read 3000.000ms.
    assert!(!meter.starts_with("3000.000ms"), "the program's own `__ALMD_PROBE 999999999` line is not the reading:\n{stderr}");
}
