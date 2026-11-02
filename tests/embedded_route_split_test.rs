//! `almide run --target wasm` refuses a program no single wasm route serves:
//! one naming an op only the WASI 0.3 component serves (the full-response
//! http family, op 65) together with one the embedded lane serves on its
//! core-module host only (the call handle, whose streamed reads and cancel
//! need the core host's call core) or that the component does not serve
//! (the subprocess family). A program the component serves whole — op 65
//! beside the tcp or udp family — runs as the component instead
//! (tests/net_tcp_p3_cross_test.rs, tests/net_udp_p3_cross_test.rs).
//!
//! It ran such a program on the core-module route, whose host has no op 65
//! (ADR-0023 decision 1), so every full-response call answered
//! `err("unknown fs op 65")` where native answered the response. Found by the
//! ADR-0027 sandbox campaign (`xtarget-fuzz run --focus http.patch_response`).
use std::process::Command;

const PROBE: &str = r#"import http

effect fn main() -> Unit = {
  let r: Result[HttpResponse, String] = http.get_response("http://127.0.0.1:9/x")
  println("${result.is_ok(r)}")
  let c = http.start(http.new_request("GET", "http://127.0.0.1:9/y", "", []), { total_ms: 0, idle_ms: 0 })!
  println("${result.is_ok(http.wait(c))}")
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
    assert!(stderr.contains("http call handle"), "names the op only the core host serves: {stderr}");
}
