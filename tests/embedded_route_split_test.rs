//! `almide run --target wasm` refuses a program no single wasm route serves:
//! one naming an op only the WASI 0.3 component serves (the full-response
//! http family, op 65) together with one the component does not serve (the
//! net core, the call handle, the subprocess family).
//!
//! It ran such a program on the core-module route, whose host has no op 65
//! (ADR-0023 decision 1), so every full-response call answered
//! `err("unknown fs op 65")` where native answered the response. Found by the
//! ADR-0027 sandbox campaign (`xtarget-fuzz run --focus http.patch_response`).
use std::process::Command;

const PROBE: &str = r#"import http
import net

effect fn main() -> Unit = {
  let r: Result[HttpResponse, String] = http.get_response("http://127.0.0.1:9/x")
  println("${result.is_ok(r)}")
  let c: Result[Int, String] = net.tcp_connect("127.0.0.1", 9)
  println("${result.is_ok(c)}")
}
"#;

#[test]
fn a_program_split_across_the_two_wasm_routes_is_refused_by_name() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("split.almd");
    std::fs::write(&file, PROBE).expect("write");
    let o = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(["--target", "wasm"]).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "ran: stdout {:?}", String::from_utf8_lossy(&o.stdout));
    assert!(o.stdout.is_empty(), "printed before refusing: {:?}", String::from_utf8_lossy(&o.stdout));
    assert!(stderr.contains("error[E081]"), "{stderr}");
    assert!(stderr.contains("*_response"), "names the component-only op: {stderr}");
    assert!(stderr.contains("net"), "names the op the component does not serve: {stderr}");
}
