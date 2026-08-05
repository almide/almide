//! #1117: `guard cond else process.exit(n)` must exit n on native run — for a
//! CONSTANT-false condition as well as a runtime-false one. Before the fix the
//! mir guard desugar built its early-return If AFTER the optimize-stage const
//! fold had run, and the render path dropped the else's exit call: stdout up
//! to the guard, exit 0 — a silent wrong-exit-code miscompile on run/build.
//!
//! Assertions are native-only and ABSOLUTE (the differential corpus gate can
//! not see a divergence both targets share). The wasm leg still loses the
//! exit code for the runtime-cond shape (silent exit 0) and walls on the
//! const-cond shape — tracked separately (see the issue filed from this test's
//! PR); extend these assertions to wasm when that leg lands.

use std::io::Write;
use std::process::Command;

static PROBE_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn run_native(source: &str) -> (Option<i32>, String) {
    let dir = std::env::temp_dir().join(format!("almide-guard-exit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = dir.join(format!("probe{seq}.almd"));
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(source.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("run almide");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn const_false_guard_else_exit_exits_with_code() {
    let (code, stdout) = run_native(
        r#"import process

effect fn main() -> Unit = {
  println("before-guard")
  guard false else process.exit(3)
  println("unreachable")
}
"#,
    );
    assert_eq!(code, Some(3), "const-false guard must exit 3, stdout: {stdout}");
    assert!(stdout.contains("before-guard"), "stdout: {stdout}");
    assert!(!stdout.contains("unreachable"), "stdout: {stdout}");
}

#[test]
fn folded_const_guard_else_exit_exits_with_code() {
    // `1 > 2` reaches the guard desugar as a LitBool via the optimize fold —
    // the exact matrix probe that measured exit 0.
    let (code, stdout) = run_native(
        r#"import process

effect fn main() -> Unit = {
  println("before-guard")
  guard 1 > 2 else { process.exit(3) }
  println("unreachable")
}
"#,
    );
    assert_eq!(code, Some(3), "folded const guard must exit 3, stdout: {stdout}");
    assert!(!stdout.contains("unreachable"), "stdout: {stdout}");
}

#[test]
fn runtime_false_guard_else_exit_exits_with_code() {
    let (code, stdout) = run_native(
        r#"import process

effect fn main() -> Unit = {
  println("before-guard")
  let n = list.len(process.args())
  guard n > 99 else process.exit(3)
  println("unreachable")
}
"#,
    );
    assert_eq!(code, Some(3), "runtime-false guard must exit 3, stdout: {stdout}");
    assert!(!stdout.contains("unreachable"), "stdout: {stdout}");
}

#[test]
fn const_true_guard_falls_through() {
    let (code, stdout) = run_native(
        r#"import process

effect fn main() -> Unit = {
  guard true else process.exit(3)
  println("reached")
}
"#,
    );
    assert_eq!(code, Some(0), "const-true guard must fall through, stdout: {stdout}");
    assert!(stdout.contains("reached"), "stdout: {stdout}");
}
