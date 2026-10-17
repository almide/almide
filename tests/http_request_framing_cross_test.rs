// @contract: C-385
//! The client owns the request body's framing, on every lane: a caller's
//! `Content-Length` header (any case) is dropped and the client writes the
//! length of the body it sends — the Fetch standard's forbidden request
//! header and Go's net/http (`Request.Header`'s Content-Length is ignored;
//! the length comes from the body). Before, native wrote the caller's value
//! beside its own (two disagreeing lengths on the wire, a malformed request)
//! while wasi:http enforced the declared length against the body and the
//! embedded lane answered `err(... (HTTP-request-body-size))`.
//!
//! The server is a loopback echo that reports how the request was framed:
//! `ok` when every Content-Length it received equals the body length and no
//! Transfer-Encoding arrived (or there is no length and no body), else
//! `MISMATCH` with the values. A `content-length: 0` on an empty body is
//! framing-equivalent to none (the p3 shim sends it for an empty PATCH).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Read one request and describe its framing: `(request line, framing, body)`.
fn read_request(s: &mut TcpStream) -> (String, String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break buf.len();
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let field = |name: &str| -> Vec<String> {
        head.lines()
            .filter_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| v.trim().to_string())
            })
            .collect()
    };
    let cls = field("content-length");
    let tes = field("transfer-encoding");
    // The FIRST length frames the read, as a lenient server would.
    let len = cls
        .first()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = &buf[head_end..(head_end + len).min(buf.len())];
    let consistent = tes.is_empty()
        && cls
            .iter()
            .all(|v| v.parse::<usize>().ok() == Some(body.len()));
    let framing = if consistent {
        "ok".to_string()
    } else {
        format!("MISMATCH cl={cls:?} te={tes:?}")
    };
    let first = head.lines().next().unwrap_or("").to_string();
    (first, framing, String::from_utf8_lossy(body).into_owned())
}

/// Echo `<method>|<framing>|<body>` until the test ends; `/status/404` answers 404.
fn serve() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
                let (first, framing, body) = read_request(&mut s);
                let method = first.split_whitespace().next().unwrap_or("");
                let path = first.split_whitespace().nth(1).unwrap_or("/");
                let status = if path == "/status/404" {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let payload = format!("{method}|{framing}|{body}");
                let _ = s.write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len())
                        .as_bytes(),
                );
            });
        }
    });
    port
}

/// The framed family (request / request_status / request_bytes /
/// request_response), served on native, the embedded lane and stock p3.
fn framed_program(port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    format!(
        r#"import http

fn show(tag: String, r: Result[String, String]) -> Unit = match r {{
  ok(b) => println("${{tag}}: ${{b}}"),
  err(e) => println("${{tag}} err: ${{e}}"),
}}

effect fn main() -> Unit = {{
  // The inspection's case: http.response's headers carry a length of "ok".
  let r: Result[String, String] = http.request("GET", "{base}/status/404", "日本語", http.headers(http.response(200, "ok")))
  show("response-headers", r)
  show("short", http.request("POST", "{base}/e", "日本語", ["Content-Length": "2"]))
  show("long", http.request_status("PUT", "{base}/e", "abc", ["content-length": "99"]) |> result.map(((c, b)) => "${{int.to_string(c)}} ${{b}}"))
  show("equal", http.request_bytes("POST", "{base}/e", "xyz", ["CONTENT-LENGTH": "3"]) |> result.map((b) => bytes.to_string_lossy(b)))
  show("empty", http.request_response("PATCH", "{base}/e", "", ["Content-Length": "5"]) |> result.map((r) => http.body(r)))
  show("get", http.request("GET", "{base}/e", "", ["content-length": "7", "X-K": "v"]))
}}
"#
    )
}

/// The streaming members, served on native and the embedded lane.
fn stream_program(port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    format!(
        r#"import http

effect fn main() -> Unit = {{
  var got = ""
  let a: Result[Unit, String] = http.request_stream("POST", "{base}/e", "body", ["Content-Length": "1"], (c) => {{ got = got + c }})
  match a {{
    ok(_) => println("stream: ${{got}}"),
    err(e) => println("stream err: ${{e}}"),
  }}
  var got2 = ""
  let limits = {{ total_ms: 0, idle_ms: 0 }}
  let b: Result[Unit, String] = http.request_stream_with_limits("PUT", "{base}/e", "λλ", ["content-length": "40"], limits, (c) => {{ got2 = got2 + c }})
  match b {{
    ok(_) => println("limited: ${{got2}}"),
    err(e) => println("limited err: ${{e}}"),
  }}
}}
"#
    )
}

fn observe(mut c: Command) -> (String, String, Option<i32>) {
    let out = c.stdin(Stdio::null()).output().expect("spawn");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

fn native(file: &Path) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    observe(c)
}

fn embedded(file: &Path) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file).args(["--target", "wasm"]);
    observe(c)
}

/// Build `file` as a WASI 0.3 component and run it on stock wasmtime.
fn p3(file: &Path, scratch: &Path) -> (String, String, Option<i32>) {
    let wasm = scratch.join("http_framing.wasm");
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args([
            "build",
            file.to_str().expect("utf8"),
            "--target",
            "wasm",
            "--component",
            "-o",
            wasm.to_str().expect("utf8"),
        ])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("almide build");
    assert!(
        built.status.success(),
        "p3 build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let mut c = Command::new("wasmtime");
    c.args(["run", "-S", "http=y"]).arg(&wasm);
    observe(c)
}

/// wasmtime runs p3 components from 46 on (the pin policy's floor).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH"
        );
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v
        .split_whitespace()
        .nth(1)
        .and_then(|x| x.split('.').next())
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    if major < 46 {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor",
            v.trim()
        );
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

const FRAMED_EXPECTED: &str = "response-headers: GET|ok|日本語
short: POST|ok|日本語
long: 200 PUT|ok|abc
equal: POST|ok|xyz
empty: PATCH|ok|
get: GET|ok|
";

#[test]
fn a_caller_content_length_is_dropped_on_native_and_the_embedded_lane() {
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_framing.almd");
    std::fs::write(&file, framed_program(port)).expect("write");
    let native = native(&file);
    let embedded = embedded(&file);
    // Native is pinned to the client-framed answer, so a fix that moved
    // both legs to the malformed request cannot pass.
    assert_eq!(
        (native.0.as_str(), native.2),
        (FRAMED_EXPECTED, Some(0)),
        "native: {}\nembedded: {:?}",
        native.1,
        embedded
    );
    assert_eq!(
        (&embedded.0, embedded.2),
        (&native.0, native.2),
        "the embedded lane diverged: {}",
        embedded.1
    );
}

#[test]
fn a_caller_content_length_is_dropped_on_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_framing.almd");
    std::fs::write(&file, framed_program(port)).expect("write");
    let p3 = p3(&file, dir.path());
    assert_eq!(
        (p3.0.as_str(), p3.2),
        (FRAMED_EXPECTED, Some(0)),
        "the p3 component diverged: {}",
        p3.1
    );
}

#[test]
fn a_caller_content_length_is_dropped_by_the_streaming_client() {
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_framing_stream.almd");
    std::fs::write(&file, stream_program(port)).expect("write");
    let expected = "stream: POST|ok|body\nlimited: PUT|ok|λλ\n";
    let native = native(&file);
    let embedded = embedded(&file);
    assert_eq!(
        (native.0.as_str(), native.2),
        (expected, Some(0)),
        "native: {}\nembedded: {:?}",
        native.1,
        embedded
    );
    assert_eq!(
        (&embedded.0, embedded.2),
        (&native.0, native.2),
        "the embedded lane diverged: {}",
        embedded.1
    );
}
