//! `almide mcp` end-to-end: drive the REAL binary over stdio and assert the
//! shapes an agent depends on.
//!
//! Everything here spawns `CARGO_BIN_EXE_almide`, so a protocol regression, a
//! stray `println!` on the protocol stream, or a tool that stops returning
//! structured data fails the suite rather than silently degrading an agent's
//! feedback into text it has to guess at.
//!
//! `almide_test` is deliberately NOT exercised here: it compiles and executes
//! the tests through cargo, which belongs in `almide test`, not in a unit-suite
//! gate. Its wiring (argv, parsing, counting) is covered by the unit tests in
//! `src/cli/mcp_tools.rs`.

use std::io::Write;
use std::process::{Command, Stdio};
use serde_json::{json, Value};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Send every request as one line, close stdin, collect the response lines.
///
/// EOF is the server's exit condition, so this cannot hang on a missing
/// response the way a framed, long-lived client can.
fn mcp_session(requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(almide())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide mcp");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for r in requests {
            writeln!(stdin, "{}", r).expect("write request");
        }
    }
    let out = child.wait_with_output().expect("wait for almide mcp");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| {
                panic!("non-JSON line on the MCP stdout stream ({}): {:?}", e, l)
            })
        })
        .collect()
}

fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize",
           "params":{"protocolVersion":"2025-06-18","capabilities":{},
                     "clientInfo":{"name":"mcp_test","version":"1"}}})
}

fn call(id: i64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
           "params":{"name":name,"arguments":arguments}})
}

/// The tool's payload: `content[0].text` is a JSON document by construction —
/// that IS the structured result, and parsing it here is the same step a client
/// takes.
fn payload(response: &Value) -> Value {
    assert_eq!(response["result"]["isError"], false, "tool reported an error: {}", response);
    let text = response["result"]["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str(text).expect("tool payload is JSON")
}

fn write_temp(dir: &std::path::Path, name: &str, source: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path.to_string_lossy().to_string()
}

#[test]
fn initialize_reports_tools_capability_and_server_identity() {
    let responses = mcp_session(&[init()]);
    let r = &responses[0];
    assert_eq!(r["jsonrpc"], "2.0");
    assert_eq!(r["result"]["serverInfo"]["name"], "almide");
    assert_eq!(r["result"]["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    assert!(r["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn tools_list_is_the_five_named_tools_and_only_test_declares_a_side_effect() {
    let responses = mcp_session(&[init(), json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]);
    let tools = responses[1]["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        vec!["almide_check", "almide_test", "almide_api", "almide_explain", "almide_fmt_check"]
    );
    for t in tools {
        assert!(t["inputSchema"]["properties"].is_object(), "{}", t["name"]);
        let read_only = t["annotations"]["readOnlyHint"].as_bool().unwrap_or(false);
        assert_eq!(read_only, t["name"] != "almide_test", "{}", t["name"]);
    }
}

#[test]
fn check_returns_structured_diagnostics_with_a_fix_snippet() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp(
        dir.path(),
        "bad.almd",
        "fn add(a: Int, b: Int) -> Int = a + b\n\nfn main() -> Unit = {\n  print(add(1, 2))\n}\n",
    );
    let responses = mcp_session(&[init(), call(2, "almide_check", json!({"file": file}))]);
    let out = payload(&responses[1]);
    assert_eq!(out["ok"], false);
    assert!(out["errors"].as_u64().unwrap() >= 1, "{}", out);
    let d = &out["diagnostics"][0];
    // `print` is not a function in Almide; the diagnostic must arrive as
    // fields — code, position, and an applicable suggestion — not as prose.
    assert_eq!(d["code"], "E002");
    assert_eq!(d["level"], "error");
    assert_eq!(d["line"], 4);
    assert_eq!(d["try"], "println");
    assert!(d["try_replace"]["line"].is_number(), "{}", d);
}

#[test]
fn check_on_a_clean_file_is_ok_with_no_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp(dir.path(), "ok.almd", "fn add(a: Int, b: Int) -> Int = a + b\n");
    let responses = mcp_session(&[init(), call(2, "almide_check", json!({"file": file}))]);
    let out = payload(&responses[1]);
    assert_eq!(out["ok"], true, "{}", out);
    assert_eq!(out["errors"], 0);
    assert_eq!(out["diagnostics"].as_array().unwrap().len(), 0);
}

/// A file where no top-level declaration parses used to leave `check --json`
/// with nothing on stdout and a human-formatted error on stderr — the most
/// common LLM mistake was the one class the structured flag did not cover.
#[test]
fn json_check_reports_a_total_parse_failure_as_json() {
    let dir = tempfile::tempdir().unwrap();
    // Missing `=` before the body: the whole file fails to parse.
    let file = write_temp(dir.path(), "syntax.almd", "fn greet(n: String) -> String {\n  n\n}\n");
    let out = Command::new(almide())
        .args(["check", &file, "--json"])
        .output()
        .expect("almide check --json");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "no JSON emitted; stderr: {}", String::from_utf8_lossy(&out.stderr));
    let d: Value = serde_json::from_str(lines[0]).expect("diagnostic is JSON");
    assert_eq!(d["level"], "error");
    assert_eq!(d["line"], 1);
    assert!(d["message"].as_str().unwrap().contains("Missing '='"), "{}", d);
    // Unchanged gate: a file that did not parse still exits 1, so a harness
    // that reads the exit code does not silently flip to success.
    assert_eq!(out.status.code(), Some(1));

    // Same file through MCP: structured, and honestly marked not-ok.
    let responses = mcp_session(&[init(), call(2, "almide_check", json!({"file": file}))]);
    let payload = payload(&responses[1]);
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["diagnostics"][0]["line"], 1);
}

#[test]
fn api_lists_stdlib_signatures() {
    let responses = mcp_session(&[
        init(),
        call(2, "almide_api", json!({"target": "@stdlib/string", "filter": "trim"})),
    ]);
    let out = payload(&responses[1]);
    assert_eq!(out["outline"]["module"], "string");
    assert_eq!(out["outline"]["source"], "stdlib");
    let fns = out["outline"]["functions"].as_array().expect("functions");
    let names: Vec<&str> = fns.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"trim"), "{:?}", names);
    let trim = fns.iter().find(|f| f["name"] == "trim").unwrap();
    assert_eq!(trim["ret"], "String");
    assert_eq!(trim["params"][0]["ty"], "String");
}

#[test]
fn api_on_a_broken_file_is_a_tool_error_that_says_what_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_temp(dir.path(), "broken.almd", "fn f() -> Int = \"nope\"\n");
    let responses = mcp_session(&[init(), call(2, "almide_api", json!({"target": file}))]);
    assert_eq!(responses[1]["result"]["isError"], true);
    let text = responses[1]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("almide_check"), "{}", text);
}

#[test]
fn explain_returns_the_reference_page() {
    let responses = mcp_session(&[init(), call(2, "almide_explain", json!({"code": "e001"}))]);
    let out = payload(&responses[1]);
    assert_eq!(out["code"], "E001");
    assert!(out["doc"].as_str().unwrap().contains("E001"), "{}", out["doc"]);
}

#[test]
fn fmt_check_json_reports_drift_and_keeps_the_gate_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let messy = write_temp(dir.path(), "messy.almd", "fn f(a: Int) -> Int =     a  +  1\n");
    let tidy = write_temp(dir.path(), "tidy.almd", "fn f(a: Int) -> Int = a + 1\n");

    let out = Command::new(almide()).args(["fmt", "--check", "--json", &messy]).output().unwrap();
    let report: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["checked"], 1);
    assert_eq!(report["unformatted"].as_array().unwrap().len(), 1);
    assert_eq!(out.status.code(), Some(1), "--json keeps --check's gate semantics");
    // The gate must not have rewritten the file it was only asked to inspect.
    assert_eq!(std::fs::read_to_string(&messy).unwrap(), "fn f(a: Int) -> Int =     a  +  1\n");

    let out = Command::new(almide()).args(["fmt", "--check", "--json", &tidy]).output().unwrap();
    let report: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(out.status.code(), Some(0));

    // Same answer through MCP.
    let responses = mcp_session(&[init(), call(2, "almide_fmt_check", json!({"paths": [messy]}))]);
    let out = payload(&responses[1]);
    assert_eq!(out["report"]["ok"], false);
    assert_eq!(out["exit_code"], 1);
}

#[test]
fn protocol_errors_are_distinguished_from_tool_errors() {
    let responses = mcp_session(&[
        init(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}),
        call(3, "almide_deploy", json!({})),
        json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
    ]);
    // The notification produced no response, so ids run 1, 2, 3, 4 over four
    // response lines.
    assert_eq!(responses.len(), 4, "{:?}", responses);
    assert_eq!(responses[1]["error"]["code"], -32601, "unadvertised method");
    assert_eq!(responses[2]["result"]["isError"], true, "unknown tool is a tool error");
    assert_eq!(responses[3]["result"], json!({}), "ping");
}
