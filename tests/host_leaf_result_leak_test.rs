//! A self-hosted stdlib body that calls a bodyless host-op leaf (`= _`,
//! intercepted by name in the wasm emitter: `__http_framed_*`, `__proc_*`,
//! `__http_call_*`, `__http_serve_*`) left blocks live on the embedded wasm
//! lane (#3623):
//! - the leaf's answer is built fresh at rc 1, but the ownership classifier
//!   counted the call as borrowed, so returning it took a +1 and the Result
//!   block and its payload were never freed (2 blocks per call);
//! - a temporary argument handed straight to the leaf (`__http_frame(...)`)
//!   was parked for release by a scope the intercept never opened (1 more).
//!
//! Every probe makes one call, matches the answer and prints the error, then
//! asserts that nothing is live at exit.
use std::process::Command;

const URL: &str = "http://127.0.0.1:1/x";

fn live_line(name: &str, src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("almide run");
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "{name}: {err}");
    err.lines()
        .find(|l| l.contains("__ALMD_WASM_ALLOC"))
        .unwrap_or_else(|| panic!("{name}: no alloc count line in {err}"))
        .to_string()
}

fn http_probe(pre: &str, call: &str) -> String {
    format!(
        "import http\neffect fn main() -> Unit = {{\n{pre}  match {call} {{\n    ok(_) => println(\"?\"),\n    err(e) => println(e),\n  }}\n}}\n"
    )
}

#[test]
fn self_hosted_host_leaf_calls_leave_no_block_live_on_wasm() {
    let probes: Vec<(&str, String)> = vec![
        ("request_inline_map", http_probe("", &format!("http.request(\"GET\", \"{URL}\", \"\", map.new())"))),
        ("request_let_map", http_probe("  let h: Map[String, String] = map.new()\n", &format!("http.request(\"GET\", \"{URL}\", \"\", h)"))),
        ("request_status", http_probe("", &format!("http.request_status(\"GET\", \"{URL}\", \"\", map.new())"))),
        ("request_bytes", http_probe("", &format!("http.request_bytes(\"GET\", \"{URL}\", \"\", map.new())"))),
        ("get_status", http_probe("", &format!("http.get_status(\"{URL}\")"))),
        ("get_bytes", http_probe("", &format!("http.get_bytes(\"{URL}\")"))),
        (
            "process_exec",
            "import process\neffect fn main() -> Unit = {\n  match process.exec(\"/nonexistent-cmd\", []) {\n    ok(_) => println(\"?\"),\n    err(e) => println(e),\n  }\n}\n"
                .to_string(),
        ),
        (
            "process_exec_args",
            "import process\neffect fn main() -> Unit = {\n  match process.exec(\"/nonexistent-cmd\", [\"a\" + \"b\", \"c\"]) {\n    ok(_) => println(\"?\"),\n    err(e) => println(e),\n  }\n}\n"
                .to_string(),
        ),
    ];
    let leaks: Vec<String> = probes
        .iter()
        .map(|(name, src)| (name, live_line(name, src)))
        .filter(|(_, line)| !line.contains(" live=0 "))
        .map(|(name, line)| format!("{name}: {line}"))
        .collect();
    assert!(
        leaks.is_empty(),
        "blocks left live at exit:\n{}",
        leaks.join("\n")
    );
}
