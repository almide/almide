//! A closure VALUE handed through a stdlib call's value position comes back as
//! the same closure on wasm, not a fresh closure layer around it.
//!
//! Found by the stdlib inspection's complexity probe of `http.wrap`: `wrap(h,
//! [])` is `list.fold([], h, step)`, and the wasm leg's callback eta-expansion
//! rewrote EVERY fn-typed variable argument of a higher-order stdlib call into
//! `(a) => v(a)` — the fold's INIT included. Each pass through `wrap` then
//! returned one more layer, so a handler wrapped k times nested k frames when
//! invoked (and k drop frames when released): a stack overflow on wasm where
//! native, which hands the `Rc` straight through, prints the answer.
//!
//! Each program re-passes one closure a million times and then calls it; the
//! wasm leg must agree with native.
use std::process::Command;

const DEPTH: &str = "1000000";

/// The probe's shape: a self-hosted `http.wrap` with no middleware.
const HTTP_WRAP: &str = r#"import http

fn go(x: HttpHandler, i: Int) -> HttpHandler = if i <= 0 then x else go(http.wrap(x, []), i - 1)

effect fn main() -> Unit = {
  let x0: HttpHandler = (req) => http.response(200, "ok")
  let x: HttpHandler = go(x0, DEPTH)
  println(http.body(x(http.new_request("GET", "/", "", map.new()))!))
}
"#;

/// The probe's other shape: one identity middleware, so the fold's step runs.
const HTTP_WRAP_IDENTITY: &str = r#"import http

fn go(x: HttpHandler, i: Int) -> HttpHandler = if i <= 0 then x else go(http.wrap(x, [(next) => next]), i - 1)

effect fn main() -> Unit = {
  let x0: HttpHandler = (req) => http.response(200, "ok")
  let x: HttpHandler = go(x0, DEPTH)
  println(http.body(x(http.new_request("GET", "/", "", map.new()))!))
}
"#;

/// The same position in user code: a pure closure as `list.fold`'s init.
const FOLD_INIT: &str = r#"fn pass(x: (Int) -> Int, ms: List[Int]) -> (Int) -> Int = list.fold(ms, x, (a, b) => a)

fn go(x: (Int) -> Int, i: Int) -> (Int) -> Int = if i <= 0 then x else go(pass(x, []), i - 1)

effect fn main() -> Unit = {
  let x0: (Int) -> Int = (n) => n + 1
  let x = go(x0, DEPTH)
  println(int.to_string(x(41)))
}
"#;

/// An effect closure as `list.fold`'s init.
const FOLD_INIT_EFFECT: &str = r#"type H = effect (Int) -> Int

fn pass(x: H, ms: List[Int]) -> H = list.fold(ms, x, (a, b) => a)

fn go(x: H, i: Int) -> H = if i <= 0 then x else go(pass(x, []), i - 1)

effect fn main() -> Unit = {
  let x0: H = (n) => n + 1
  let x = go(x0, DEPTH)
  println(int.to_string(x(41)!))
}
"#;

fn run(src: &str, wasm: bool) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("pass_through.almd");
    std::fs::write(&file, src.replace("DEPTH", DEPTH)).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_closure_passed_through_a_value_position_is_not_layered_on_wasm() {
    for (name, src, want) in [
        ("http.wrap", HTTP_WRAP, "ok\n"),
        ("http.wrap identity middleware", HTTP_WRAP_IDENTITY, "ok\n"),
        ("fold init", FOLD_INIT, "42\n"),
        ("fold init (effect)", FOLD_INIT_EFFECT, "42\n"),
    ] {
        let native = run(src, false);
        assert!(native.0, "{name}: native failed: {}", native.2);
        assert_eq!(native.1, want, "{name}: native stdout");
        let wasm = run(src, true);
        assert!(
            wasm.0,
            "{name}: wasm failed (a closure layer per pass?): {}",
            wasm.2
        );
        assert_eq!(wasm.1, want, "{name}: wasm stdout");
    }
}
