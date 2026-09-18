//! #2245: every native stdout write goes through ONE buffer that flushes per
//! line only when stdout is a terminal. `println` used to lower to Rust's
//! `println!` — one write syscall per line whatever stdout was attached to —
//! while `io.write` sat behind a separate buffer flushed per call to keep the
//! two handles in program order. What is asserted here is what a program can
//! observe: the emitted shape (the lowering names the buffer's macro, never
//! `println!`), program order across `println` / `io.write` / `io.print` on
//! both targets, the flushes that must not be lost (a panic, an `Err` out of
//! `main`), and a prompt reaching the reader before the program blocks on
//! stdin.
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn almide() -> &'static str { env!("CARGO_BIN_EXE_almide") }

fn write(dir: &Path, name: &str, src: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).unwrap();
    p
}

const ORDER: &str = "import io\n\neffect fn main() -> Unit = {\n  println(\"one\")\n  io.write(bytes.from_list([116, 119, 111, 10]))\n  io.print(\"three\\n\")\n  println(\"four\")\n  io.write_bytes([102, 105, 118, 101, 10])\n  println(\"six\")\n}\n";

#[test]
fn println_lowers_to_the_buffered_macro_and_never_to_rusts_println() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "order.almd", ORDER);
    let out = Command::new(almide()).arg(&file).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rs = String::from_utf8_lossy(&out.stdout);
    let user_code = rs.split("fn __almide_main").nth(1).expect("the user's main");
    assert!(user_code.contains("almide_println!("), "no buffered println in:\n{user_code}");
    assert!(!user_code.contains(" println!("), "a raw println! survived in:\n{user_code}");
    assert!(rs.contains("almide_stdout_finish();"), "the main wrapper flushes at exit");
    assert!(rs.contains("std::panic::set_hook"), "the main wrapper flushes on a panic");
}

#[test]
fn writes_through_println_io_write_and_io_print_keep_program_order_on_both_targets() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "order.almd", ORDER);
    let mut outs = Vec::new();
    for target in ["rust", "wasm"] {
        let out = Command::new(almide()).arg("run").arg(&file).args(["--target", target]).stdout(Stdio::piped()).output().unwrap();
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        outs.push(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    assert_eq!(outs[0], "one\ntwo\nthree\nfour\nfive\nsix\n");
    assert_eq!(outs[0], outs[1], "native and wasm disagree on the order");
}

#[test]
fn lines_printed_before_a_panic_and_before_an_err_out_of_main_reach_a_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let panics = write(dir.path(), "panic.almd", "effect fn main() -> Unit = {\n  println(\"before\")\n  assert_eq(1, 2)\n  println(\"after\")\n}\n");
    let out = Command::new(almide()).arg("run").arg(&panics).stdout(Stdio::piped()).output().unwrap();
    assert!(!out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n", "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let errs = write(dir.path(), "err.almd", "effect fn fail() -> Result[Int, String] = err(\"boom\")\n\neffect fn main() -> Unit = {\n  println(\"printed\")\n  let x = fail()!\n  println(\"${x}\")\n}\n");
    let out = Command::new(almide()).arg("run").arg(&errs).stdout(Stdio::piped()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "printed\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("Error: boom"));
}

/// A `println` prompt followed by `io.read_line()`: the prompt must be on the
/// pipe BEFORE the program blocks on stdin, or an interactive reader waits
/// forever. The reader here has a deadline; the answer is only sent after the
/// prompt arrived.
#[test]
fn a_prompt_printed_before_a_stdin_read_reaches_the_reader_before_the_read_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let prog = write(dir.path(), "prompt.almd", "import io\n\neffect fn main() -> Unit = {\n  println(\"name?\")\n  let n = io.read_line()\n  println(\"hi \" + n)\n}\n");
    let exe = dir.path().join("prompt");
    let built = Command::new(almide()).arg("build").arg(&prog).arg("-o").arg(&exe).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let mut child = Command::new(&exe).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines();
        let first = lines.next().map(|l| l.unwrap());
        tx.send(first).unwrap();
        let second = lines.next().map(|l| l.unwrap());
        tx.send(second).unwrap();
    });
    let prompt = rx.recv_timeout(Duration::from_secs(10)).expect("the prompt never arrived: stdout was not flushed before the stdin read");
    assert_eq!(prompt.as_deref(), Some("name?"));
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"ada\n").unwrap();
    drop(stdin);
    let reply = rx.recv_timeout(Duration::from_secs(10)).expect("no reply");
    assert_eq!(reply.as_deref(), Some("hi ada"));
    assert!(child.wait().unwrap().success());
}
