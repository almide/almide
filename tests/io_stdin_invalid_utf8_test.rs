//! Invalid UTF-8 on stdin reads the same on every target (#4142, C-227).
//!
//! Native `io.read_all` / `io.read_line` read into a `String` and swallowed the
//! `InvalidData` error with `unwrap_or(0)`: a stream holding one invalid byte
//! read as "" (read_all) and a line holding one read as an empty line, while the
//! wasm emitter appended the raw bytes and handed back a String holding invalid
//! UTF-8. One rule now holds on native, on the embedded host
//! (`almide run --target wasm`) and on a stock runtime (`almide build --target
//! wasm` + wasmtime): the bytes decode as `String::from_utf8_lossy` does, one
//! U+FFFD per maximal invalid subpart. A read error (stdin is a directory) ends
//! the input on every leg.
//!
//! Stdin-reading programs cannot live in the spec suites (the conformance
//! harness pipes no per-fixture stdin), so this Command-with-stdin test is the
//! executable evidence — the tests/io_read_line_opt_test.rs pattern.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// One cursor: the String line reader, the Option line reader, the String line
/// reader again, then the rest; `string.len` shows the decoded char count, so a
/// String holding raw invalid bytes cannot pass for a decoded one.
const PROGRAM: &str = r#"import io

effect fn main() -> Unit = {
  let a = (io.read_line() ?? "")
  let b = io.read_line() ?? "NONE"
  let c = (io.read_line() ?? "")
  let rest = io.read_all()
  println("[${a}] [${b}] [${c}] [${rest}] ${int.to_string(string.len(a))} ${int.to_string(string.len(rest))}")
}
"#;

enum Input<'a> {
    Bytes(&'a [u8]),
    Dir,
}

fn run(cmd: &mut Command, input: &Input<'_>) -> (i32, String, String) {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = match input {
        Input::Bytes(b) => {
            let mut child = cmd.stdin(Stdio::piped()).spawn().expect("spawn");
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(b)
                .expect("write stdin");
            child.wait_with_output().expect("wait")
        }
        Input::Dir => {
            let dir = std::fs::File::open(std::env::temp_dir()).expect("open a directory as stdin");
            cmd.stdin(Stdio::from(dir)).output().expect("run")
        }
    };
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).expect("stdout must be valid UTF-8 on every leg"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (leg, exit, stdout, stderr) for every leg this machine can run.
fn legs(dir: &Path, input: &Input<'_>) -> Vec<(&'static str, i32, String, String)> {
    let file = dir.join("main.almd");
    let mut v = Vec::new();
    let (c, o, e) = run(
        Command::new(almide())
            .arg("run")
            .arg(&file)
            .current_dir(dir),
        input,
    );
    v.push(("native", c, o, e));
    let (c, o, e) = run(
        Command::new(almide())
            .args(["run"])
            .arg(&file)
            .args(["--target", "wasm"])
            .current_dir(dir),
        input,
    );
    v.push(("embedded", c, o, e));
    if wasmtime_available() {
        let wasm = dir.join("main.wasm");
        let b = Command::new(almide())
            .arg("build")
            .arg(&file)
            .args(["--target", "wasm", "-o"])
            .arg(&wasm)
            .current_dir(dir)
            .output()
            .expect("build");
        assert!(
            b.status.success(),
            "stock build failed: {}",
            String::from_utf8_lossy(&b.stderr)
        );
        let (c, o, e) = run(Command::new("wasmtime").arg(&wasm).current_dir(dir), input);
        v.push(("stock", c, o, e));
    } else {
        eprintln!("SKIP stock leg: wasmtime not on PATH");
    }
    v
}

const CASES: &[(&[u8], &str)] = &[
    // the issue's probe: one invalid byte inside a line and inside the rest
    (b"abc\xffdef", "[abc\u{FFFD}def] [NONE] [] [] 7 0\n"),
    (b"ab\xffc\nsecond", "[ab\u{FFFD}c] [second] [] [] 4 0\n"),
    // a truncated 3-byte lead, a truncated 4-byte lead before CRLF, a surrogate
    // (ED A0 80: three subparts), an overlong (C0 AF), a trailing truncated lead
    (
        b"ab\xe3\x81\ncd\xf0\x9f\x98\r\n\xed\xa0\x80x\xc0\xaf\n\xe3\x81",
        "[ab\u{FFFD}] [cd\u{FFFD}] [\u{FFFD}\u{FFFD}\u{FFFD}x\u{FFFD}\u{FFFD}] [\u{FFFD}] 3 1\n",
    ),
    // stray continuations; CRs before the LF are cut, the invalid bytes are not
    (
        b"x\r\r\n\xff\n\n\x80\x80\xf4\x90\x80\x80",
        "[x] [\u{FFFD}] [] [\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}] 1 6\n",
    ),
    // valid multibyte text is untouched
    (
        "é\n日本\n😀\nrest".as_bytes(),
        "[é] [日本] [😀] [rest] 1 4\n",
    ),
];

#[test]
fn invalid_utf8_stdin_decodes_lossily_on_every_leg() {
    let dir = tempfile::Builder::new()
        .prefix("t4142")
        .tempdir()
        .expect("tempdir");
    std::fs::write(dir.path().join("main.almd"), PROGRAM).expect("write");
    for (input, want) in CASES {
        for (leg, code, out, err) in legs(dir.path(), &Input::Bytes(input)) {
            assert_eq!(code, 0, "{leg}: exit for stdin {input:?}\n{err}");
            assert_eq!(&out, want, "{leg}: stdout for stdin {input:?}");
        }
    }
}

/// A read error (stdin is a directory: EISDIR) ends the input on every leg —
/// the same answer as an empty stdin, never a crash and never a leg-specific one.
#[test]
fn a_read_error_ends_stdin_on_every_leg() {
    let dir = tempfile::Builder::new()
        .prefix("t4142d")
        .tempdir()
        .expect("tempdir");
    std::fs::write(dir.path().join("main.almd"), PROGRAM).expect("write");
    for (leg, code, out, err) in legs(dir.path(), &Input::Dir) {
        assert_eq!(code, 0, "{leg}: exit with a directory as stdin\n{err}");
        assert_eq!(
            out, "[] [NONE] [] [] 0 0\n",
            "{leg}: stdout with a directory as stdin"
        );
    }
}
