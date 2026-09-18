//! #2065: the deadline includes pipes retained by an orphaned descendant.
#![cfg(unix)]
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn inherited_output_pipes_do_not_extend_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("deadline.almd");
    let app = dir.path().join("deadline");
    std::fs::write(
        &source,
        r#"
import process
import env
effect fn main() -> Unit = {
  let args = env.args()
  let script = args[0]
  let timeout = if args[1] == "fast" then 30000 else 100
  match process.exec_status_timeout("sh", ["-c", script], timeout) {
    ok(st) => println("ok ${st.code}: ${st.stdout}"),
    err(e) => println(e),
  }
}
"#,
    )
    .unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into());
    let built = Command::new(bin)
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for script in [
        "sleep 4 & sleep 3",    // both child and descendant still alive
        "sleep 4 & exit 0",     // direct child has already exited
        "sleep 4 >&2 & exit 0", // inherited stderr must also be bounded
    ] {
        let start = Instant::now();
        let output = Command::new(&app).args([script, "timeout"]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "exec timed out after 100ms",
            "{script}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "deadline extended by inherited pipes: {script}"
        );
    }
    let output = Command::new(&app)
        .args(["printf done; exit 7", "fast"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok 7: done");
}
