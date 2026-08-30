//! Sequential stdin composition across the whole read family (#1598's io
//! tail): `read_byte` / `read_line` / `read_n_bytes` / `read_all` share ONE
//! stdin cursor on the structural wasm leg (host op 35 — incremental take)
//! exactly as native's shared stdin handle, so interleaved reads see the
//! stream in order. Before this, the leg's only stdin op drained the whole
//! stream on first read: `read_n_bytes(3)` answered EVERYTHING and a second
//! read answered nothing, and read_line/read_byte walled to the incumbent.
//!
//! Stdin-reading programs cannot live in the spec suites (the harness does
//! not pipe per-file stdin), so this Command-with-stdin test is the
//! executable evidence, the io_read_all_test pattern.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const PROGRAM: &str = r#"import io

effect fn main() -> Unit = {
  let a = io.read_n_bytes(3)
  let b = io.read_byte()
  let l1 = io.read_line()!
  let l2 = io.read_line()!
  let rest = io.read_all()
  let e1 = io.read_byte()
  let e2 = io.read_line()!
  println("a=${a} b=${b} l1=${l1} l2=${l2} rest=${rest} e1=${e1} e2=[${e2}]")
}
"#;

/// CRLF + LF lines, a byte prefix, an unterminated tail, then EOF probes.
const STDIN: &str = "ABCDone\r\ntwo\nrest-bytes";

const EXPECTED: &str =
    "a=[65, 66, 67] b=68 l1=one l2=two rest=rest-bytes e1=-1 e2=[]\n";

fn run_with_stdin(file: &std::path::Path, wasm: bool, stdin: &str) -> (i32, String, String) {
    let mut args = vec!["run", file.to_str().unwrap()];
    if wasm {
        args.push("--target");
        args.push("wasm");
    }
    let mut child = Command::new(almide())
        .args(&args)
        .env("ALMIDE_WASM_STRUCTURAL", if wasm { "1" } else { "" })
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
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn stdin_reads_compose_on_one_cursor_native_and_structural() {
    let dir = std::env::temp_dir().join("almide-io-stdin-seq");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("seq.almd");
    std::fs::write(&file, PROGRAM).expect("write");

    let (code, native, stderr) = run_with_stdin(&file, false, STDIN);
    assert_eq!(code, 0, "native run failed:\n{stderr}");
    assert_eq!(native, EXPECTED, "native output drifted from the pinned contract");

    // Forced-structural: a wall is a hard error, so success proves the
    // structural leg itself served every read off its op-35 cursor.
    let (code, wasm, stderr) = run_with_stdin(&file, true, STDIN);
    assert_eq!(
        code, 0,
        "structural leg failed the stdin sequence (walled or diverged):\n{stderr}"
    );
    assert_eq!(wasm, native, "wasm/native stdin-sequence divergence");
}
