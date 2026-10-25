//! `process.output` with stdin text to a child that exits without reading its input
//! answers on native, as on the embedded host, instead of killing the
//! program.
//!
//! A native `main` puts SIGPIPE back to its default disposition (#1950, so
//! `prog | head` stops quietly). The shared process core then wrote the
//! input into the pipe of a child that had already exited, and the signal
//! ended the whole program with no output. The embedded host, a Rust
//! process that ignores SIGPIPE, answered err: C-ALS-R5's "answers what the
//! native leg answers" did not hold. Found by the stdlib inspection's
//! parity campaign (`xtarget-fuzz run --focus process.exec_with_stdin`, the
//! predecessor of `process.output`'s `stdin` field).
#![cfg(unix)]
use std::process::Command;

const PROBE: &str = r#"import process

effect fn main() -> Unit = {
  let r = process.output(process.ProcessCommand { cmd: "/bin/echo", args: ["x"], stdin: string.repeat("日本語", 100000) })
  println("answered ${result.is_ok(r) or result.is_err(r)}")
}
"#;

fn run(wasm: bool) -> (String, Option<i32>) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("pipe.almd");
    std::fs::write(&file, PROBE).expect("source");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let o = cmd.output().expect("spawn");
    (String::from_utf8_lossy(&o.stdout).into_owned(), o.status.code())
}

#[test]
fn a_child_that_reads_no_input_does_not_kill_the_program() {
    for wasm in [false, true] {
        assert_eq!(run(wasm), ("answered true\n".to_string(), Some(0)), "wasm={wasm}");
    }
}
