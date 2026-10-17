//! A non-2xx refusal reads `HTTP <status>: <body>` on every leg — no reason
//! phrase (owner ruling 2026-10-17). wasi:http hands the guest a status code
//! and no reason phrase, so a leg on it cannot spell one; the reason phrase
//! is already outside C-367's comparison for the same cause. Native used to
//! answer `HTTP 404: Not Found: gone` from the status line.
//!
//! The server answers every request `404` with a reason phrase no table
//! would produce (`Gone Fishing`), so a leg that still renders the line's
//! reason, or one that renders a table's, is visible. All four streaming
//! calls — the two `_with_limits` twins and the plain `request_stream` /
//! `openai_streaming_call` — run on native (`almide run`) and on the embedded
//! lane (`almide run --target wasm`), and both print the expected bytes.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Read one request (head plus its Content-Length body) and answer 404.
fn answer(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => break buf.len(),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_ascii_lowercase();
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < head_end + len {
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let _ = s.write_all(b"HTTP/1.1 404 Gone Fishing\r\nContent-Length: 4\r\nConnection: close\r\n\r\ngone");
}

fn serve() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || answer(s));
        }
    });
    port
}

fn program(port: u16) -> String {
    format!(
        r#"import http

fn said(tag: String, r: Result[Unit, String]) -> Unit = match r {{
  ok(_) => println("${{tag}} ok"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

fn said_s(tag: String, r: Result[String, String]) -> Unit = match r {{
  ok(j) => println("${{tag}} ok ${{j}}"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

effect fn main() -> Unit = {{
  let url = "http://127.0.0.1:{port}/x"
  let base = "http://127.0.0.1:{port}/v1"
  said("stream_limits", http.request_stream_with_limits("GET", url, "", map.new(), {{ total_ms: 20000, idle_ms: 0 }}, (c) => println("chunk ${{c}}")))
  said_s("openai_limits", http.openai_streaming_call_with_limits(base, "k", "{{}}", {{ total_ms: 20000, idle_ms: 0 }}, (d) => println("delta ${{d}}")))
  said_s("openai", http.openai_streaming_call(base, "k", "{{}}", (d) => println("delta ${{d}}")))
  said("stream", http.request_stream("GET", url, "", map.new(), (c) => println("chunk ${{c}}")))
}}
"#
    )
}

const EXPECTED: &str = "stream_limits err HTTP 404: gone\nopenai_limits err HTTP 404: gone\n\
openai err HTTP 404: gone\nstream err HTTP 404: gone\n";

fn run(file: &Path, wasm: bool) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.stdin(Stdio::null()).output().expect("spawn");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.code())
}

#[test]
fn a_non_2xx_streaming_call_answers_the_status_without_a_reason_phrase_on_every_leg() {
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("non_2xx.almd");
    std::fs::write(&file, program(port)).expect("write");
    let native = run(&file, false);
    assert_eq!((native.0.as_str(), native.2), (EXPECTED, Some(0)), "native: {}", native.1);
    let embedded = run(&file, true);
    assert_eq!((embedded.0.as_str(), embedded.2), (EXPECTED, Some(0)), "the embedded lane: {}", embedded.1);
}
