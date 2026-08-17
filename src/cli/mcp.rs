//! `almide mcp` — a Model Context Protocol server over stdio.
//!
//! The compiler's answers reach an agent as typed calls with JSON results
//! instead of a shell command whose human-formatted output has to be re-parsed
//! by the model. That parsing step is where accuracy leaks, and this repo is
//! scored on one metric: how accurately an LLM writes and modifies Almide.
//!
//! Transport: newline-delimited JSON-RPC 2.0 on stdin/stdout (the MCP stdio
//! transport — no `Content-Length` framing; that is LSP, which `almide lsp`
//! speaks). Nothing but protocol messages may be written to stdout, so every
//! tool runs the compiler in a SUBPROCESS and captures its streams
//! ([`super::mcp_tools`]); the in-process `out()`/`err()` writers are never
//! used on this path.
//!
//! Implemented methods: `initialize`, `tools/list`, `tools/call`, `ping`.
//! Capabilities advertise `tools` only, so an unknown method is answered with
//! JSON-RPC `-32601` rather than a guess.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// MCP revision this server implements. A client that asks for a different one
/// gets its own version echoed back when we can serve it — the tool surface is
/// identical across the revisions that have `tools/*` — and this one otherwise.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Revisions whose `tools/*` shape this server satisfies as-is.
const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// What the model is told about this server up front. Short on purpose: the
/// per-tool descriptions carry the detail.
const INSTRUCTIONS: &str = "\
Almide compiler tools. Prefer these over shelling out to `almide`: they return \
structured results, so nothing has to be parsed out of rendered text.

- almide_check   type errors as {code, file, line, col, message, hint, try}
- almide_api     exact signatures of a module or of @stdlib/<module>
- almide_explain what a diagnostic code means and the sanctioned fix
- almide_test    run the .almd test blocks (compiles and executes)
- almide_fmt_check  which files are not formatted (never rewrites)

Write operations are deliberately absent: run `almide fmt` / `almide fix` in a \
shell when you intend to change files.";

/// Read newline-delimited JSON-RPC from stdin until EOF, answering each
/// request on stdout. Returns when the client closes the pipe.
pub fn run_mcp() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(response) = handle_message(text) {
            if writeln!(stdout, "{}", response).is_err() || stdout.flush().is_err() {
                return;
            }
        }
    }
}

/// One inbound message → an optional response line. Notifications (no `id`)
/// are answered with `None`, as JSON-RPC requires.
fn handle_message(text: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => return Some(error_response(Value::Null, -32700, &format!("parse error: {}", e))),
    };
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    // A notification carries no id and must never be answered — not even with
    // an error, or a client that sends `notifications/initialized` sees a
    // spurious failure.
    let id = id.filter(|v| !v.is_null())?;
    if method.is_empty() {
        return Some(error_response(id, -32600, "invalid request: no method"));
    }
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    Some(match dispatch(method, &params) {
        Ok(result) => success_response(id, result),
        Err(RpcError { code, message }) => error_response(id, code, &message),
    })
}

struct RpcError {
    code: i64,
    message: String,
}

fn dispatch(method: &str, params: &Value) -> Result<Value, RpcError> {
    match method {
        "initialize" => Ok(initialize_result(params)),
        "tools/list" => Ok(json!({ "tools": super::mcp_tools::catalog() })),
        "tools/call" => tools_call(params),
        "ping" => Ok(json!({})),
        other => Err(RpcError {
            code: -32601,
            message: format!("method not found: {}", other),
        }),
    }
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(|v| v.as_str());
    let version = match requested {
        Some(v) if SUPPORTED_VERSIONS.contains(&v) => v,
        _ => PROTOCOL_VERSION,
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "almide",
            "title": "Almide compiler",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

/// `tools/call`: a failing TOOL is a successful RPC carrying `isError: true`
/// (the agent is meant to read it and react); only a malformed request is a
/// JSON-RPC error.
fn tools_call(params: &Value) -> Result<Value, RpcError> {
    let name = params.get("name").and_then(|n| n.as_str()).ok_or(RpcError {
        code: -32602,
        message: "invalid params: `name` is required".to_string(),
    })?;
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    Ok(match super::mcp_tools::call(name, &args) {
        Ok(value) => text_result(&serde_json::to_string_pretty(&value).unwrap_or_default(), false),
        Err(message) => text_result(&message, true),
    })
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": is_error,
    })
}

fn success_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(text: &str) -> Value {
        serde_json::from_str(&handle_message(text).expect("a response")).unwrap()
    }

    #[test]
    fn initialize_echoes_a_supported_protocol_version() {
        let r = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#);
        assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(r["result"]["serverInfo"]["name"], "almide");
        assert!(r["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn initialize_falls_back_to_our_revision_for_an_unknown_one() {
        let r = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#);
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn notifications_get_no_response() {
        assert!(handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let r = call(r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#);
        assert_eq!(r["error"]["code"], -32601);
        assert_eq!(r["id"], 7);
    }

    #[test]
    fn malformed_json_is_a_parse_error_with_a_null_id() {
        let r = call("{not json");
        assert_eq!(r["error"]["code"], -32700);
        assert!(r["id"].is_null());
    }

    #[test]
    fn a_failing_tool_is_a_successful_rpc_flagged_as_an_error() {
        let r = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nope"}}"#);
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("unknown tool"));
    }

    #[test]
    fn tools_call_without_a_name_is_an_invalid_params_error() {
        let r = call(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{}}"#);
        assert_eq!(r["error"]["code"], -32602);
    }

    #[test]
    fn tools_list_returns_the_catalog() {
        let r = call(r#"{"jsonrpc":"2.0","id":4,"method":"tools/list"}"#);
        let tools = r["result"]["tools"].as_array().expect("tools array");
        assert!(tools.iter().any(|t| t["name"] == "almide_check"));
    }
}
