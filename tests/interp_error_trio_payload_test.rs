//! The reference interpreter runs the self-hosted `error` bodies (native has
//! its runtime intrinsic, wasm lowers the trio directly). Those bodies read
//! the Result as a scalar `Result[Int, String]` laid out len-as-tag, which
//! the interpreter's values are not: `error.message(ok("café"))` read the Ok
//! String's length as an Err tag and returned "café", and `error.context`
//! rebuilt an Ok Bool / Float payload as an Int (`ok(0)`). Both backends
//! printed the right value, so the 3-way gate cast a wrong third vote.
//!
//! Found by the counted stdlib inspection's parity campaign (`error.context`
//! seed 1791780367, `error.message` seed 1791780313). The interpreter must
//! print what native prints.
use std::process::Command;

const SOURCE: &str = r#"fn main() -> Unit = {
  let b: Result[Bool, String] = ok(false)
  let f: Result[Float, String] = ok(-0.0)
  let s: Result[String, String] = ok("café")
  let e: Result[String, String] = err("boom")
  println("${error.context(b, "ctx")}")
  println("${error.context(f, "ctx")}")
  println("${error.context(s, "ctx")}")
  println("${error.context(e, "ctx")}")
  println("[${error.message(s)}]")
  println("[${error.message(b)}]")
  println("[${error.message(e)}]")
  println("[${error.message(error.context(error.context(e, "a"), "b"))}]")
  println(error.chain("outer", "inner"))
  let v0: Result[Bool, String] = ok(bytes.is_empty((if true then bytes.from_list([7, 1, 100]) else bytes.from_list([1]))))
  println("${error.context(error.context(v0, "hello"), "")}")
  let acc0: Result[String, String] = ok(string.to_lower("café"))
  println("[${error.message(acc0)}]")
}
"#;

fn native() -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("error_trio.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_interpreter_prints_what_native_prints_for_the_error_trio() {
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", SOURCE).expect("front failed");
    let out = almide_interp::Interpreter::new(&ir).run_main();
    assert_eq!(out.status, almide_interp::RunStatus::Ok, "stderr=<{}>", out.stderr);
    let want = native();
    assert_eq!(
        want,
        "ok(false)\nok(-0)\nok(\"café\")\nerr(\"ctx: boom\")\n[]\n[]\n[boom]\n[b: a: boom]\nouter\ncaused by: inner\nok(false)\n[]\n",
        "native"
    );
    assert_eq!(out.stdout, want, "interpreter");
}
