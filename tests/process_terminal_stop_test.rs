//! #2540: `process.exec_status_timeout` runs the child in its own process
//! group (so a timeout can kill the whole tree, #2065), which makes it a
//! BACKGROUND job for the controlling terminal. A child that touches the
//! terminal is stopped by SIGTTOU / SIGTTIN and used to sit stopped until
//! the deadline, then come back as a plain `exec timed out` — nothing said
//! why. The stop is now answered at once with an error naming the signal
//! and `process.run`, the entry point that runs a child on the terminal
//! (`exec_attached` until #3379).
//!
//! No pty is needed: `kill -TTOU $$` stops the shell exactly as the kernel
//! does when a background job writes terminal settings.
#![cfg(unix)]
use std::process::Command;
use std::time::{Duration, Instant};

fn run(main_body: &str) -> (String, String, Duration) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("proc.almd");
    let app = dir.path().join("proc");
    std::fs::write(&source, format!("import process\neffect fn main() -> Unit = {{\n{main_body}\n}}\n"))
        .unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into());
    let built = Command::new(bin).arg("build").arg(&source).arg("-o").arg(&app).output().expect("build");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let start = Instant::now();
    let out = Command::new(&app).output().unwrap();
    let elapsed = start.elapsed();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        elapsed,
    )
}

#[test]
fn a_child_stopped_for_the_terminal_errs_at_once_instead_of_timing_out() {
    let (stdout, _, elapsed) = run(r#"  match process.exec_status_timeout("sh", ["-c", "kill -TTOU $$; echo resumed"], 20000) {
    ok(st) => println("ok " + int.to_string(st.code)),
    err(e) => println("err " + e),
  }"#);
    assert_eq!(
        stdout.trim(),
        "err process.exec_status_timeout(\"sh\", 20000): the child was stopped by SIGTTOU: \
         it tried to use the terminal from a background process group; \
         run terminal programs with process.run"
    );
    assert!(elapsed < Duration::from_secs(10), "waited {elapsed:?} — the stop was not detected");
}

#[test]
fn an_ordinary_child_still_completes_under_the_timeout() {
    let (stdout, _, _) = run(r#"  let st = process.exec_status_timeout("sh", ["-c", "echo out; echo err >&2; exit 4"], 20000)!
  println(int.to_string(st.code) + "|" + st.stdout + "|" + st.stderr)"#);
    assert_eq!(stdout, "4|out\n|err\n\n");
}

#[test]
fn run_shares_the_parents_stdio_and_returns_the_exit_code() {
    let (stdout, stderr, _) = run(r#"  println("before")
  let code = process.run("sh", ["-c", "echo child-out; echo child-err >&2; exit 3"])!
  println("code " + int.to_string(code))
  let missing = process.run("almide-no-such-binary-2540", [])
  match missing {
    ok(c) => println("unexpected ok " + int.to_string(c)),
    err(e) => println(e),
  }"#);
    // Inherited, in order: the parent's buffered line is flushed before the child runs.
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("before"));
    assert_eq!(lines.next(), Some("child-out"));
    assert_eq!(lines.next(), Some("code 3"));
    assert!(
        lines.next().unwrap_or("").starts_with("process.run(\"almide-no-such-binary-2540\"): "),
        "{stdout}"
    );
    assert_eq!(stderr, "child-err\n");
}

#[test]
fn run_in_runs_the_child_in_the_given_directory() {
    let (stdout, _, _) = run(r#"  println("before")
  let code = process.run_in("/", "sh", ["-c", "pwd; exit 5"])!
  println("code " + int.to_string(code))
  match process.run_in("/almide-no-such-dir-3379", "sh", ["-c", "exit 0"]) {
    ok(c) => println("unexpected ok " + int.to_string(c)),
    err(e) => println(e),
  }"#);
    let mut lines = stdout.lines();
    assert_eq!(lines.next(), Some("before"));
    assert_eq!(lines.next(), Some("/"));
    assert_eq!(lines.next(), Some("code 5"));
    assert!(
        lines.next().unwrap_or("").starts_with("process.run_in(\"/almide-no-such-dir-3379\", \"sh\"): "),
        "{stdout}"
    );
}

/// #3379: `exec_attached` shipped in 0.63.0..0.66.0, so it stays as a
/// deprecated alias of `run` (same intrinsic, same answer).
#[test]
fn the_deprecated_exec_attached_alias_still_runs() {
    let (stdout, _, _) = run(r#"  let code = process.exec_attached("sh", ["-c", "exit 6"])!
  println("code " + int.to_string(code))"#);
    assert_eq!(stdout, "code 6\n");
}
