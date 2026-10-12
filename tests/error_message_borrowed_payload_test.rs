//! `error.message(r)` borrows `r` and hands back its Err payload. The wasm leg
//! returned that payload as an owned String without retaining it, so the
//! caller held a reference nobody had counted:
//! - a temporary source (`error.message(error.context(r, "hello"))`) was
//!   freed after the call with the payload inside it, so the printed message
//!   read freed memory (`    o: D` for `hello: D`);
//! - a live variable lost its payload when the message was dropped, so every
//!   later read of `r` printed freed memory.
//!
//! Found by the counted stdlib inspection's parity campaign for
//! `error.context` / `error.message` (seed 1791780367). The expected output
//! is native's, which was already right.
use std::process::Command;

const SOURCE: &str = r#"fn tag(r: Result[Int, String]) -> String = error.message(r)

fn main() -> Unit = {
  let x1: Result[Int, String] = err("D")
  println(error.message(error.context(x1, "hello")))
  let r: Result[Int, String] = err("payload " + int.to_string(42))
  let m1 = error.message(r)
  println(m1)
  let m2 = error.message(r)
  println(m2)
  println(tag(r))
  println(error.message(r))
  let rs: List[Result[Int, String]] = [err("a" + "b"), ok(1), err("c" + "d")]
  let ms = rs |> list.map((q) => error.message(q))
  println("${ms}")
  println("${rs |> list.map((q) => error.message(q))}")
  println(error.message(error.context(error.context(r, "outer"), "top")))
  let ok1: Result[Int, String] = ok(3)
  println("[${error.message(ok1)}]")
  println("${r}")
}
"#;

const EXPECTED: &str = r#"hello: D
payload 42
payload 42
payload 42
payload 42
["ab", "", "cd"]
["ab", "", "cd"]
top: outer: payload 42
[]
err("payload 42")
"#;

fn run(wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("error_message.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn error_message_payload_survives_its_source_on_both_legs() {
    assert_eq!(run(false), EXPECTED, "native");
    assert_eq!(run(true), EXPECTED, "wasm");
}
