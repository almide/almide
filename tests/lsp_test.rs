//! Automated LSP integration tests.
//! Spawns `almide lsp` as a subprocess, sends JSON-RPC over stdin/stdout,
//! and verifies responses.

use std::io::{Read, Write, BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use serde_json::{json, Value};

/// How long any single server response may take before the test fails.
/// Reads used to block UNBOUNDED on the server pipe, so a non-responding
/// server was an infinite test hang — 4h20m on the Windows leg before the
/// 6h job default would have killed it (#1008). A deadline turns any future
/// hang into a red naming the wait, in about a minute.
const RECV_DEADLINE: Duration = Duration::from_secs(60);

struct LspClient {
    child: std::process::Child,
    /// Parsed server messages, delivered by the reader thread — recv'ing
    /// through a channel is what makes the deadline possible.
    rx: mpsc::Receiver<Value>,
    /// The server's captured stderr (trace lines, drop notices) — dumped
    /// into the deadline panic so a hang names its cause (#1008).
    server_log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// The `capabilities` object from the initialize response.
    capabilities: Value,
}

/// The reader half, on its own thread: blocks on the pipe, parses framed
/// JSON-RPC messages, forwards them. When the child dies (or is killed by
/// `Drop`) the pipe EOFs and the thread exits.
fn reader_loop(stdout: std::process::ChildStdout, tx: mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut header = String::new();
        let mut len: Option<usize> = None;
        loop {
            header.clear();
            if reader.read_line(&mut header).is_err() || header.is_empty() {
                return; // EOF / broken pipe — server gone
            }
            let trimmed = header.trim();
            if trimmed.is_empty() {
                if len.is_some() { break; } // end of headers
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
                len = rest.trim().parse().ok();
            }
        }
        let Some(len) = len else { return };
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).is_err() { return; }
        let Ok(msg) = serde_json::from_slice::<Value>(&buf) else { return };
        if tx.send(msg).is_err() { return; } // client dropped
    }
}

impl Drop for LspClient {
    /// Kill the server on ANY exit path (panic included): a deadline panic
    /// must not leave an orphan `almide lsp` holding the pipe open.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl LspClient {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("lsp")
            .env("ALMIDE_LSP_TRACE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to start almide lsp");
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || reader_loop(stdout, tx));
        let server_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = server_log.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                let Ok(line) = line else { return };
                log.lock().unwrap().push(line);
            }
        });
        let mut client = LspClient { child, rx, server_log, capabilities: Value::Null };
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
        match self.rx.recv_timeout(RECV_DEADLINE) {
            Ok(msg) => msg,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!(
                "no server message within {RECV_DEADLINE:?} — the server is hung \
                 (#1008); an unbounded read here previously turned this into a \
                 4h+ CI hang.\nserver stderr (tail):\n{}",
                self.server_log_tail()
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!(
                    "server pipe closed without a response (server died).\nserver stderr (tail):\n{}",
                    self.server_log_tail()
                )
            }
        }
    }

    /// The last lines of the server's captured stderr, for hang forensics.
    fn server_log_tail(&self) -> String {
        let log = self.server_log.lock().unwrap();
        let start = log.len().saturating_sub(40);
        log[start..].join("\n")
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
        self.capabilities = resp["result"]["capabilities"].clone();

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

    fn did_change(&mut self, uri: &str, text: &str) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": text }]
            }
        }));
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn signature_help(&mut self, id: i64, uri: &str, line: u32, character: u32) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }
        }));
        self.recv_response(id)
    }

    fn code_action(&mut self, id: i64, uri: &str, diagnostics: Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri },
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "context": { "diagnostics": diagnostics }
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

/// UTF-16 code-unit column of `target`'s first occurrence in `line` — the
/// encoding the server declares and every request position must use.
fn utf16_col(line: &str, target: &str) -> u32 {
    let byte = line.find(target).unwrap();
    line[..byte].chars().map(|c| c.len_utf16() as u32).sum()
}

#[test]
fn lsp_capabilities_declare_utf16_and_no_rename() {
    let mut c = LspClient::start();
    assert_eq!(
        c.capabilities["positionEncoding"].as_str(),
        Some("utf-16"),
        "server must declare the encoding it honors: {}",
        c.capabilities
    );
    assert!(
        c.capabilities.get("renameProvider").is_none(),
        "unscoped textual rename must not be advertised: {}",
        c.capabilities
    );
    c.shutdown();
}

#[test]
fn lsp_hover_after_nonascii_text() {
    let mut c = LspClient::start();
    // Japanese before the hover target: the UTF-16 column differs from the
    // byte column, which the old byte-sliced positions either missed or
    // panicked on.
    let src = "let greeting = \"world\"\nlet msg = \"こんにちは、\" + greeting\n";
    c.open_file(TEST_URI, src);
    let line1 = src.lines().nth(1).unwrap();
    let resp = c.hover(1, TEST_URI, 1, utf16_col(line1, "greeting"));
    let val = hover_value(&resp);
    assert!(val.contains("greeting") && val.contains("String"), "hover after Japanese text: got {}", val);
    c.shutdown();
}

#[test]
fn lsp_position_inside_multibyte_no_panic() {
    let mut c = LspClient::start();
    let src = "let msg = \"こんにちは\"\nlet x = 1\n";
    c.open_file(TEST_URI, src);
    // A UTF-16 column landing inside the Japanese literal: the old code
    // sliced the line at that value as a BYTE offset — mid-char — and the
    // server died of a char-boundary panic.
    let resp = c.hover(1, TEST_URI, 0, 13);
    assert!(resp.get("id").is_some(), "server must answer, not die");
    // And a column far past EOL must clamp, not panic.
    let resp = c.hover(2, TEST_URI, 0, 10_000);
    assert!(resp.get("id").is_some(), "past-EOL hover must answer");
    // The server is still alive and correct afterwards.
    let line1 = src.lines().nth(1).unwrap();
    let resp = c.hover(3, TEST_URI, 1, utf16_col(line1, "x"));
    assert!(hover_value(&resp).contains("Int"), "server still serves after odd positions");
    c.shutdown();
}

#[test]
fn lsp_signature_help_past_eol_no_panic() {
    let mut c = LspClient::start();
    let src = "let s = string.len(\"日本語\")\n";
    c.open_file(TEST_URI, src);
    // The old code sliced `&line[..pos.character]` with no bound check —
    // any client cursor past EOL (or inside the multibyte literal) killed
    // the server.
    let resp = c.signature_help(1, TEST_URI, 0, 10_000);
    assert!(resp.get("id").is_some(), "past-EOL signatureHelp must answer");
    let resp = c.signature_help(2, TEST_URI, 0, utf16_col(src, "\")") + 1);
    assert!(resp.get("id").is_some(), "signatureHelp near multibyte text must answer");
    c.shutdown();
}

#[test]
fn lsp_document_symbols_carry_real_uri() {
    let mut c = LspClient::start();
    c.open_file(TEST_URI, TEST_SOURCE);
    let resp = c.document_symbols(1, TEST_URI);
    let symbols = resp["result"].as_array().unwrap();
    assert!(!symbols.is_empty());
    for s in symbols {
        assert_eq!(
            s["location"]["uri"].as_str(),
            Some(TEST_URI),
            "documentSymbol must carry the document's own URI, not a fabricated one: {}",
            s
        );
    }
    c.shutdown();
}

#[test]
fn lsp_didchange_never_fetches_or_writes_lock() {
    // A project with a dependency manifest: didChange must not shell out to
    // git or write almide.lock — the old server did both on every keystroke.
    let dir = std::env::temp_dir().join(format!("almide_lsp_didchange_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"lsp_didchange_probe\"\n\n[dependencies]\nnonexistent_pkg = { git = \"https://invalid.invalid/nowhere\", branch = \"main\" }\n",
    ).unwrap();
    let file = dir.join("main.almd");
    std::fs::write(&file, "let x = 1\n").unwrap();
    let uri = format!("file://{}", file.display());

    let mut c = LspClient::start();
    // didChange without a prior didOpen: the cache has no entry for this
    // project, and the no-fetch path must resolve against no deps.
    c.did_change(&uri, "let x = 2\n");
    let resp = c.hover(1, &uri, 0, 4);
    // PROBE ONLY (#1008): dump the exchange even on success so the Windows
    // leg's log carries the full server-side trace either way.
    eprintln!("[probe] uri sent: {uri}");
    eprintln!("[probe] hover response: {resp}");
    eprintln!("[probe] server stderr:\n{}", c.server_log_tail());
    assert!(resp.get("id").is_some(), "server must answer after didChange");
    assert!(
        !dir.join("almide.lock").exists(),
        "didChange must never write almide.lock"
    );
    c.shutdown();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn lsp_try_fix_reaches_client_as_code_action() {
    let mut c = LspClient::start();
    // E013 with a close-match suggestion emits a machine-applicable
    // try_replace fix ("p.name") — it must survive into the published
    // diagnostic's `data` and come back as a quickfix edit.
    let src = "type Person = { name: String, age: Int }\nfn get(p: Person) -> String = p.nam\n";
    c.open_file(TEST_URI, src);
    let msg = c.recv();
    let diags = msg["params"]["diagnostics"].clone();
    let e013 = diags.as_array().unwrap().iter()
        .find(|d| d["code"].as_str() == Some("E013"))
        .unwrap_or_else(|| panic!("expected an E013 diagnostic, got {}", diags));
    assert!(e013.get("data").is_some(), "E013's fix-it must ride in `data`: {}", e013);

    let resp = c.code_action(1, TEST_URI, json!([e013]));
    let actions = resp["result"].as_array().unwrap();
    let fix = actions.iter().find(|a| a["title"].as_str().map_or(false, |t| t.contains("p.name")))
        .unwrap_or_else(|| panic!("expected a quickfix applying `p.name`, got {}", resp["result"]));
    let edits = &fix["edit"]["changes"][TEST_URI];
    assert_eq!(edits[0]["newText"].as_str(), Some("p.name"), "quickfix edit text: {}", fix);
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
