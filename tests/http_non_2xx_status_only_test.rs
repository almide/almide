//! A non-2xx answer is the response, not an err, on every leg, and its status
//! is the code alone — no reason phrase (owner ruling 2026-10-17). wasi:http
//! hands the guest a status code and no reason phrase, so a leg on it cannot
//! spell one. The streaming clients that refused a non-2xx as
//! `HTTP <status>: <body>` are gone (R-http-01): the call handle and `send`
//! answer `ok` with the status and the body.
//!
//! The server answers every request `404` with a reason phrase no table
//! would produce (`Gone Fishing`), so a leg that still renders the line's
//! reason, or one that renders a table's, is visible. The handle and `send`
//! run on native (`almide run`) and on the embedded lane
//! (`almide run --target wasm`), and both print the expected bytes.
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

fn program(port: u16, call: &str) -> String {
    format!(
        r#"import http

fn said(tag: String, r: Result[HttpResponse, String]) -> Unit = match r {{
  ok(resp) => println("${{tag}} ok ${{http.status(resp)}} ${{http.body(resp)}}"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

effect fn main() -> Unit = {{
  let url = "http://127.0.0.1:{port}/x"
  {call}
}}
"#
    )
}

/// The handle (op 53) and the framed client (op 65) are served by different
/// wasm hosts, so each runs as its own program: one artifact naming both is
/// refused at check time (E081).
const CALLS: &[(&str, &str)] = &[
    ("said(\"handle\", http.wait(http.start(http.new_request(\"GET\", url, \"\", []), { total_ms: 20000, idle_ms: 0 })!))", "handle ok 404 gone\n"),
    ("said(\"send\", http.send(http.new_request(\"GET\", url, \"\", [])))", "send ok 404 gone\n"),
];

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
fn a_non_2xx_call_answers_the_status_and_body_without_a_reason_phrase_on_every_leg() {
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    for (i, (call, expected)) in CALLS.iter().enumerate() {
        let file = dir.path().join(format!("non_2xx_{i}.almd"));
        std::fs::write(&file, program(port, call)).expect("write");
        let native = run(&file, false);
        assert_eq!((native.0.as_str(), native.2), (*expected, Some(0)), "native: {}", native.1);
        let embedded = run(&file, true);
        assert_eq!((embedded.0.as_str(), embedded.2), (*expected, Some(0)), "the embedded lane: {}", embedded.1);
    }
}
