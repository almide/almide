//! A pure handler lambda that reaches an `HttpHandler` slot through a generic
//! container builds and runs on both legs (#4768).
//!
//! `let v: Secret[HttpHandler] = secret.new((req) => …)` and the forms that
//! carry it through `ok(..)`, `some(..)`, `result.unwrap_or(v, d)` and
//! `result.unwrap_or_else(v, (e) => …)` were accepted by `almide check`, but
//! the lambda kept its bare `HttpResponse` return: native failed with E0308
//! and the wasm lane with E082 `ty-mismatch`. The effect-slot sweep now
//! adapts a value at the effect slot a generic call's type variable names,
//! reading the variable from the call's own slot or from its other arguments,
//! and brings the arms of an `if` / `match` back to one adapted type.
//! Found by the stdlib re-inspection's parity campaign for `http.mount`.

use std::io::Write;
use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const PROGRAM: &str = r#"import env
import http
import secret

effect fn call(h: HttpHandler, target: String) -> String = {
  let res = h(http.new_request("GET", target, "", []))!
  "${http.status(res)} ${http.body(res)}"
}

effect fn main() -> Unit = {
  let v: Secret[HttpHandler] = secret.new((req) => http.response(201, "a:" + http.path(req)))
  println(call(secret.reveal(v), "/x")!)
  let v0: Result[Secret[HttpHandler], String] = ok(secret.new((req) => http.response(202, "b")))
  let h0 = secret.reveal(result.unwrap_or_else(v0, (e) => secret.new((req) => http.response(500, e))))
  println(call(h0, "/y")!)
  let e0: Result[Secret[HttpHandler], String] = err("fallback")
  let h1 = secret.reveal(result.unwrap_or_else(e0, (e) => secret.new((req) => http.response(203, e))))
  println(call(h1, "/z")!)
  let h2 = secret.reveal(result.unwrap_or(e0, secret.new((req) => http.response(204, "d"))))
  println(call(h2, "/w")!)
  let o: Option[Secret[HttpHandler]] = some(secret.new((req) => http.response(205, "o")))
  match o {
    some(s) => println(call(secret.reveal(s), "/o")!),
    none => println("none"),
  }
  let r = http.router([http.mount("/m", secret.reveal(v))])!
  println(call(r, "/m/q")!)
  let pick = list.len(env.args()) > 99
  let h3 = secret.reveal(if pick then result.unwrap_or_else(v0, (e) => secret.new((req) => http.response(500, e))) else secret.new((req) => http.response(206, "if")))
  println(call(h3, "/i")!)
  let h4 = secret.reveal(match e0 { ok(s) => s, err(m) => secret.new((req) => http.response(207, m)) })
  println(call(h4, "/m")!)
}
"#;

const EXPECTED: &str = "201 a:/x\n202 b\n203 fallback\n204 d\n205 o\n201 a:/q\n206 if\n207 fallback\n";

fn run(target: &str) {
    let dir = std::env::temp_dir().join(format!("almd_4768_{}_{}", target, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::File::create(&file).unwrap().write_all(PROGRAM.as_bytes()).unwrap();
    let out = Command::new(almide_bin())
        .args(["run", file.to_str().unwrap(), "--target", target])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "the {target} run failed:\n{stderr}");
    assert_eq!(stdout, EXPECTED, "{target} stdout");
}

#[test]
fn a_pure_handler_through_a_generic_container_runs_native() {
    run("native");
}

#[test]
fn a_pure_handler_through_a_generic_container_runs_wasm() {
    run("wasm");
}
