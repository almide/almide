//! Cross-target evidence for reading piped stdin as lines (#3620), spelled
//! `io.read_all()` then `string.lines`: the wasm legs (the embedded host and a
//! stock `wasmtime run` of the built artifact) must answer exactly what native
//! answers — the line split, the final unterminated line, `\r` handling, and
//! invalid UTF-8 read as U+FFFD.
//!
//! Stdin-reading programs cannot live in the spec suites (the harness does not
//! pipe per-file stdin), so this is the executable evidence.

use std::io::Write;
use std::path::{Path, PathBuf};
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

fn run_with_stdin(mut cmd: Command, stdin: &[u8]) -> (i32, String, String) {
    let mut child = cmd
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child.stdin.as_mut().expect("stdin").write_all(stdin).expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn almide_run(file: &Path, wasm: bool) -> Command {
    let mut c = Command::new(almide());
    c.args(["run", file.to_str().unwrap()]);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    c
}

const PROGRAM: &str = r#"import io

effect fn main() -> Unit = {
  let lines = string.lines(io.read_all())
  println("n=${int.to_string(list.len(lines))}")
  for l in lines {
    println("[${l}] ${int.to_string(string.len(l))}")
  }
}
"#;

const CASES: &[&[u8]] = &[
    b"",
    b"one",
    b"one\n",
    b"a\nb\nc",
    b"\n\n",
    b"crlf\r\nnext\r\n",
    b"double\r\r\nlone\rcr\r",
    b"multi \xce\xbb byte\n",
    b"bad \xff byte\n",
    b"ok first\nbad \xc3\n",
];

#[test]
fn stdin_lines_matches_native_on_every_wasm_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("stdin_lines.almd");
    std::fs::write(&file, PROGRAM).expect("write fixture");

    // A chunk-boundary case: lines straddling the 4096-byte read.
    let big = [b"y".repeat(4095), b"\r\n".to_vec(), b"z".repeat(5000), b"\n".to_vec()].concat();
    let mut cases: Vec<&[u8]> = CASES.to_vec();
    cases.push(&big);

    let stock = wasmtime_available();
    let wasm_file = dir.path().join("stdin_lines.wasm");
    if stock {
        let st = Command::new(almide())
            .args(["build", file.to_str().unwrap(), "--target", "wasm", "-o", wasm_file.to_str().unwrap()])
            .current_dir(repo_root())
            .status()
            .expect("build");
        assert!(st.success(), "wasm build failed");
    } else {
        eprintln!("SKIP stock-p1 leg: wasmtime not on PATH — enforced on Linux CI");
    }

    for stdin in cases {
        let native = run_with_stdin(almide_run(&file, false), stdin);
        assert_eq!(native.0, 0, "native failed for {stdin:?}: {}", native.2);
        let embedded = run_with_stdin(almide_run(&file, true), stdin);
        assert_eq!((embedded.0, &embedded.1), (native.0, &native.1), "embedded diverged for {stdin:?}: {}", embedded.2);
        if stock {
            let mut c = Command::new("wasmtime");
            c.args(["run", wasm_file.to_str().unwrap()]);
            let p1 = run_with_stdin(c, stdin);
            assert_eq!((p1.0, &p1.1), (native.0, &native.1), "stock-p1 diverged for {stdin:?}: {}", p1.2);
        }
    }
}
