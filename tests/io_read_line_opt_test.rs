//! `io.read_line_opt` tells the end of input from an empty line (#2539).
//!
//! `io.read_line` answers "" both for a line that was only a newline and at
//! end of input, so an interactive loop that skips empty lines never sees the
//! end — it spins on "" forever. `read_line_opt` answers `none` at the end and
//! `some("")` for the empty line. The issue's input (`a`, an empty line, `b`
//! with no trailing newline) must read the same on native and on both wasm
//! legs, and it must compose with the rest of the stdin family on one cursor.
//!
//! Stdin-reading programs cannot live in the spec suites (the harness does not
//! pipe per-file stdin), so this Command-with-stdin test is the executable
//! evidence — the tests/io_stdin_seq_test.rs pattern.

use std::io::Write;
use std::process::{Command, Stdio};

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

const LOOP: &str = r#"import io

fn show(l: Option[String]) -> String = match l {
  some(s) => "some(\"${s}\")",
  none => "none",
}

effect fn main() -> Unit = {
  var i = 0
  while i < 5 {
    println(int.to_string(i) + ": " + show(io.read_line_opt()))
    i = i + 1
  }
}
"#;

/// One cursor: the Option reader, the String reader and the byte reader take
/// turns; CR is cut like read_line cuts it; a multibyte line survives.
const MIXED: &str = r#"import io

effect fn main() -> Unit = {
  let a = io.read_line_opt() ?? "NONE"
  let b = io.read_line()
  let c = io.read_byte()
  let d = io.read_line_opt() ?? "NONE"
  let e = io.read_line_opt() ?? "NONE"
  let f = io.read_line_opt() ?? "NONE"
  let g = io.read_line_opt() ?? "NONE"
  println("[${a}] [${b}] ${int.to_string(c)} [${d}] [${e}] [${f}] [${g}]")
}
"#;

/// The leg selector: "" native, else the wasm leg forced by its env flag.
const LEGS: [(&str, &str); 3] =
    [("native", ""), ("structural", "ALMIDE_WASM_STRUCTURAL"), ("incumbent", "ALMIDE_WASM_INCUMBENT")];

fn run(program: &str, leg: (&str, &str), stdin: &str) -> String {
    let dir = tempfile::Builder::new().prefix("t2539").tempdir().expect("tempdir");
    let file = dir.path().join("main.almd");
    std::fs::write(&file, program).expect("write");
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(&file);
    if !leg.1.is_empty() {
        cmd.args(["--target", "wasm"]).env(leg.1, "1");
    }
    let mut child = cmd
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    child.stdin.take().expect("stdin").write_all(stdin.as_bytes()).expect("write stdin");
    let out = child.wait_with_output().expect("wait almide");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{}: run failed (walled or diverged):\n{stderr}", leg.0);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn legs() -> Vec<(&'static str, &'static str)> {
    if wasmtime_available() {
        LEGS.to_vec()
    } else {
        eprintln!("wasmtime not on PATH — only the native leg is exercised");
        LEGS[..1].to_vec()
    }
}

#[test]
fn an_empty_line_is_some_empty_and_the_end_of_input_is_none() {
    let expected = "0: some(\"a\")\n1: some(\"\")\n2: some(\"b\")\n3: none\n4: none\n";
    for leg in legs() {
        assert_eq!(run(LOOP, leg, "a\n\nb"), expected, "{}", leg.0);
    }
}

#[test]
fn empty_stdin_is_none_from_the_first_read() {
    let expected = "0: none\n1: none\n2: none\n3: none\n4: none\n";
    for leg in legs() {
        assert_eq!(run(LOOP, leg, ""), expected, "{}", leg.0);
    }
}

#[test]
fn read_line_opt_shares_the_stdin_cursor_with_the_rest_of_the_family() {
    let expected = "[one] [two] 88 [three] [] [日本語] [NONE]\n";
    for leg in legs() {
        assert_eq!(run(MIXED, leg, "one\ntwo\nXthree\r\n\r\n日本語\r\r\n"), expected, "{}", leg.0);
    }
}
