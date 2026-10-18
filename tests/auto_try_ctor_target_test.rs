//! In an effect fn, a Result-typed call handed to `some(…)`, `ok(…)` or
//! `err(…)` keeps its Result when the constructor's own type says the payload
//! IS a Result. The auto-`?` pass decided list elements from the list's type
//! but hard-coded "unwrap" for these three, so `some(int.parse("x"))` (an
//! `Option[Result[Int, String]]` the checker accepts) lowered as
//! `Some(int.parse("x")?)`: native failed to compile (E0308) and wasm walled.
//! Found by the inspection campaign (error.message, seed 1791814733).
use std::process::Command;

const PROGRAM: &str = r#"effect fn main() -> Unit = {
  let o: Option[Result[Int, String]] = some(int.parse("x"))
  let p = some(int.parse("12"))
  let r: Result[Result[Int, String], String] = ok(int.parse("7"))
  let e: Result[Int, Result[Int, String]] = err(int.parse("y"))
  let show = (v: Result[Int, String]) => match v { ok(n) => "ok ${n}", err(m) => "err ${m}" }
  match o { some(v) => println(show(v)), none => println("none") }
  match p { some(v) => println(show(v)), none => println("none") }
  match r { ok(v) => println(show(v)), err(m) => println("outer ${m}") }
  match e { ok(n) => println("${n}"), err(v) => println(show(v)) }
}
"#;

fn run(file: &std::path::Path, wasm: bool) -> (Option<i32>, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_result_payload_of_some_ok_err_keeps_its_result_in_an_effect_fn() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("ctor.almd");
    std::fs::write(&file, PROGRAM).expect("source");
    let native = run(&file, false);
    assert_eq!(native.0, Some(0), "native: {}", native.2);
    let lines: Vec<&str> = native.1.lines().collect();
    assert_eq!(lines.len(), 4, "{}", native.1);
    assert!(lines[0].starts_with("err "), "some(int.parse(\"x\")) holds the err: {}", native.1);
    assert_eq!(&lines[1..3], ["ok 12", "ok 7"], "{}", native.1);
    assert!(lines[3].starts_with("err "), "err(int.parse(\"y\")) holds the err: {}", native.1);
    let wasm = run(&file, true);
    assert_eq!((wasm.0, &wasm.1), (native.0, &native.1), "wasm: {}", wasm.2);
}
