//! Cross-target fixture for `io.read_all` (#876): the self-hosted wasm body
//! (an `io.read_n_bytes` read-to-EOF loop over one growing `Bytes` buffer)
//! must byte-match the native intrinsic (`read_to_string`) for piped stdin.
//!
//! Stdin-reading programs cannot live in the spec suites (the test harness
//! does not pipe per-file stdin), so this is the executable evidence — the
//! same Command-with-stdin shape both targets run under.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_with_stdin(file: &std::path::Path, wasm: bool, stdin: &str) -> (i32, String) {
    let mut args = vec!["run", file.to_str().unwrap()];
    if wasm {
        args.push("--target");
        args.push("wasm");
    }
    let mut child = Command::new(almide())
        .args(&args)
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait almide");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
    )
}

const PROGRAM: &str = r#"import io

effect fn main() -> Result[Unit, String] = {
  let text = io.read_all()
  println("len=${int.to_string(string.len(text))}")
  for line in string.lines(string.trim(text)) {
    println("got: ${line}")
  }
  ok(())
}
"#;

#[test]
fn read_all_byte_matches_across_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("read_all_fixture.almd");
    std::fs::write(&file, PROGRAM).expect("write fixture");

    // Multi-chunk boundary: past one 4096-byte read so the self-host loops.
    let big_line = "x".repeat(9000);
    for stdin in ["hi\nthere\n", "", &format!("{big_line}\nend\n")] {
        let (native_code, native_out) = run_with_stdin(&file, false, stdin);
        assert_eq!(native_code, 0, "native leg failed for stdin len {}", stdin.len());

        if !wasmtime_available() {
            eprintln!("SKIP: wasmtime not on PATH — read_all parity enforced on Linux CI");
            return;
        }
        let (wasm_code, wasm_out) = run_with_stdin(&file, true, stdin);
        assert_eq!(wasm_code, 0, "wasm leg failed for stdin len {}", stdin.len());
        assert_eq!(
            native_out, wasm_out,
            "io.read_all output diverged for stdin len {}",
            stdin.len()
        );
    }
}
