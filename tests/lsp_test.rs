//! Automated LSP integration tests.
//! Spawns `almide lsp` as a subprocess, sends JSON-RPC over stdin/stdout,
//! and verifies responses.

use std::io::{Read, Write, BufRead, BufReader};
use std::process::{Command, Stdio};
use serde_json::{json, Value};

struct LspClient {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
}

impl LspClient {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start almide lsp");
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        let mut client = LspClient { child, reader };
        client.initialize();
        client
    }

    fn send(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(header.as_bytes()).unwrap();
        stdin.write_all(body.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn recv(&mut self) -> Value {
        // Read Content-Length header
        let mut header = String::new();
        loop {
            header.clear();
            self.reader.read_line(&mut header).unwrap();
            let trimmed = header.trim();
            if trimmed.is_empty() { break; }
            if trimmed.starts_with("Content-Length:") {
                let len: usize = trimmed.split(':').nth(1).unwrap().trim().parse().unwrap();
                // Read blank line after header
                let mut blank = String::new();
                self.reader.read_line(&mut blank).unwrap();
                // Read body
                let mut buf = vec![0u8; len];
                self.reader.read_exact(&mut buf).unwrap();
                return serde_json::from_slice(&buf).unwrap();
            }
        }
        panic!("no response received");
    }

    /// Read responses until we find one with the given id.
    fn recv_response(&mut self, id: i64) -> Value {
        for _ in 0..50 {
            let msg = self.recv();
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
                return msg;
            }
            // skip notifications (diagnostics etc.)
        }
        panic!("response id={} not found", id);
    }

    fn initialize(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": null,
                "capabilities": {},
                "rootUri": null
            }
        }));
        let resp = self.recv_response(0);
        assert!(resp.get("result").is_some(), "initialize should succeed");

        // Send initialized notification
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }));
    }

    fn open_file(&mut self, uri: &str, text: &str) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "almide",
                    "version": 1,
                    "text": text
                }
            }
        }));
        // Consume diagnostic notification
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn hover(&mut self, id: i64, uri: &str, line: u32, character: u32) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        }));
        self.recv_response(id)
    }

    fn definition(&mut self, id: i64, uri: &str, line: u32, character: u32) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        }));
        self.recv_response(id)
    }

    fn completion(&mut self, id: i64, uri: &str, line: u32, character: u32) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        }));
        self.recv_response(id)
    }

    fn document_symbols(&mut self, id: i64, uri: &str) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/documentSymbol",
            "params": {
                "textDocument": { "uri": uri }
            }
        }));
        self.recv_response(id)
    }

    fn shutdown(&mut self) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": 999,
            "method": "shutdown",
            "params": null
        }));
        let _ = self.recv_response(999);
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }));
        let _ = self.child.wait();
    }
}

const TEST_URI: &str = "file:///tmp/lsp_test.almd";

const TEST_SOURCE: &str = r#"import io

type Color = | Red | Green | Blue

fn greet(name: String) -> String = "Hello, " + name

fn double(x: Int) -> Int = x * 2

let greeting = "world"

effect fn main() -> Unit = {
  io.print(greet(greeting) + "\n")
}
"#;

fn hover_value(resp: &Value) -> String {
    resp["result"]["contents"]["value"].as_str().unwrap_or("").to_string()
}

#[test]
fn lsp_hover_keyword() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "fn" keyword at line 4, col 0
    let resp = c.hover(1, TEST_URI, 4, 0);
    assert!(hover_value(&resp).contains("Function declaration"), "hover on 'fn' keyword");
    c.shutdown();
}

#[test]
fn lsp_hover_function() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "greet" at line 4: fn greet(name: String) -> String = ...
    let resp = c.hover(1, TEST_URI, 4, 4);
    let val = hover_value(&resp);
    assert!(val.contains("fn greet"), "hover on fn greet: got {}", val);
    assert!(val.contains("String"), "hover shows return type");
    c.shutdown();
}

#[test]
fn lsp_hover_variant_constructor() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "Red" at line 2: type Color = | Red | Green | Blue
    // Find the position of "Red" — after "| "
    let red_col = TEST_SOURCE.lines().nth(2).unwrap().find("Red").unwrap() as u32;
    let resp = c.hover(1, TEST_URI, 2, red_col);
    let val = hover_value(&resp);
    assert!(val.contains("variant of Color"), "hover on Red: got {}", val);
    c.shutdown();
}

#[test]
fn lsp_hover_type_declaration() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "Color" at line 2
    let col = TEST_SOURCE.lines().nth(2).unwrap().find("Color").unwrap() as u32;
    let resp = c.hover(1, TEST_URI, 2, col);
    let val = hover_value(&resp);
    assert!(val.contains("| Red"), "hover on type Color shows variants: got {}", val);
    assert!(val.contains("| Blue"), "hover shows Blue variant");
    c.shutdown();
}

#[test]
fn lsp_hover_top_let() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "greeting" at line 8: let greeting = "world"
    let col = TEST_SOURCE.lines().nth(8).unwrap().find("greeting").unwrap() as u32;
    let resp = c.hover(1, TEST_URI, 8, col);
    let val = hover_value(&resp);
    assert!(val.contains("greeting") && val.contains("String"), "hover on let greeting: got {}", val);
    c.shutdown();
}

#[test]
fn lsp_hover_primitive_type() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // "Int" at line 6: fn double(x: Int) -> Int
    let col = TEST_SOURCE.lines().nth(6).unwrap().find("Int").unwrap() as u32;
    let resp = c.hover(1, TEST_URI, 6, col);
    let val = hover_value(&resp);
    assert!(val.contains("64-bit"), "hover on Int: got {}", val);
    c.shutdown();
}

#[test]
fn lsp_hover_stdlib_module_func() {
    let mut c = LspClient::start();
    // Source with string.to_upper
    let src = "let x = string.to_upper(\"hello\")\n";
    c.open_file(TEST_URI, src);
    // hover on "to_upper" — col after "string."
    let col = src.find("to_upper").unwrap() as u32;
    let resp = c.hover(1, TEST_URI, 0, col);
    let val = hover_value(&resp);
    assert!(val.contains("fn string.to_upper"), "hover on to_upper: got {}", val);
    c.shutdown();
}

#[test]
fn lsp_definition_fn() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    // Cmd+click on "greet" in main body (line 11)
    let col = TEST_SOURCE.lines().nth(11).unwrap().find("greet").unwrap() as u32;
    let resp = c.definition(1, TEST_URI, 11, col);
    let result = &resp["result"];
    assert!(!result.is_null(), "definition should return a location");
    let def_line = result["range"]["start"]["line"].as_u64().unwrap();
    assert_eq!(def_line, 4, "greet is declared on line 4");
    c.shutdown();
}

#[test]
fn lsp_definition_variant() {
    let mut c = LspClient::start();
    let src = "type Color = | Red | Green | Blue\nlet c = Red\n";
    c.open_file(TEST_URI, src);
    // Cmd+click on "Red" at line 1 col 8
    let col = src.lines().nth(1).unwrap().find("Red").unwrap() as u32;
    let resp = c.definition(1, TEST_URI, 1, col);
    let result = &resp["result"];
    assert!(!result.is_null(), "definition of variant should return location");
    let def_line = result["range"]["start"]["line"].as_u64().unwrap();
    assert_eq!(def_line, 0, "Color type is on line 0");
    c.shutdown();
}

#[test]
fn lsp_completion_module() {
    let mut c = LspClient::start();
    let src = "let x = string.\n";
    c.open_file(TEST_URI, src);
    let resp = c.completion(1, TEST_URI, 0, 15); // after "string."
    let items = resp["result"].as_array().unwrap();
    assert!(!items.is_empty(), "completion after string. should return items");
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(labels.contains(&"to_upper"), "should contain to_upper: {:?}", labels);
    assert!(labels.contains(&"len"), "should contain len: {:?}", labels);
    c.shutdown();
}

#[test]
fn lsp_completion_keyword() {
    let mut c = LspClient::start();
    let src = "ma\n";
    c.open_file(TEST_URI, src);
    let resp = c.completion(1, TEST_URI, 0, 2); // after "ma"
    let items = resp["result"].as_array().unwrap();
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(labels.contains(&"match"), "should suggest match: {:?}", labels);
    c.shutdown();
}

#[test]
fn lsp_document_symbols() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    let resp = c.document_symbols(1, TEST_URI);
    let symbols = resp["result"].as_array().unwrap();
    let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"greet"), "should contain greet: {:?}", names);
    assert!(names.contains(&"double"), "should contain double: {:?}", names);
    assert!(names.contains(&"Color"), "should contain Color: {:?}", names);
    assert!(names.contains(&"main"), "should contain main: {:?}", names);
    c.shutdown();
}

#[test]
fn lsp_diagnostics_type_error() {
    let mut c = LspClient::start();
    let src = "fn bad() -> Int = \"hello\"\n";
    c.open_file(TEST_URI, src);
    // Read diagnostic notification
    let msg = c.recv();
    let diags = &msg["params"]["diagnostics"];
    assert!(diags.is_array(), "should receive diagnostics");
    let arr = diags.as_array().unwrap();
    assert!(!arr.is_empty(), "should have at least one diagnostic");
    let codes: Vec<&str> = arr.iter().filter_map(|d| d["code"].as_str()).collect();
    assert!(codes.contains(&"E001"), "should contain E001: {:?}", codes);
    c.shutdown();
}
