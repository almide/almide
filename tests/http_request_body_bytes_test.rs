//! `http.set_body_bytes` on a request sends the bytes it was given (#4744).
//! The request body used to cross to the runtime as a `String`, so every
//! byte that was not UTF-8 left as U+FFFD: 256 bytes went out as 512.
//!
//! The server is a loopback echo that answers what it received: the length,
//! the body as hex, and the `X-After` header — the header cell that follows
//! the body cell in the wasm legs' request frame, so a body cell cut at the
//! wrong byte shows up as a wrong header as well as a wrong body. Every lane
//! must answer the same bytes: native, the embedded wasm lane (`http.send`
//! and the `http.start` call handle) and the stock p3 component.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Read one request: `(body, X-After value)`.
fn read_request(s: &mut TcpStream) -> (Vec<u8>, String) {
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
    let field = |name: &str| -> Option<String> {
        head.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    };
    let len = field("content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[head_end..(head_end + len).min(buf.len())].to_vec();
    (body, field("x-after").unwrap_or_default())
}

/// Answer `len=<n> after=<X-After> hex=<body as hex>` until the test ends.
fn serve() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
                let (body, after) = read_request(&mut s);
                let payload = format!("len={} after={after} hex={}", body.len(), hex(&body));
                let _ = s.write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len())
                        .as_bytes(),
                );
            });
        }
    });
    port
}

/// Every byte value in order, every byte value starting on a UTF-8
/// continuation byte (the body cell's one irregular length), a valid UTF-8
/// body given as bytes and a text body — the last two the negative controls
/// that already went out exact.
fn send_program(port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    format!(
        r#"import http

effect fn put(tag: String, req: HttpRequest) -> Unit = match http.send(req) {{
  ok(r) => println("${{tag}}: ${{http.body(r)}}"),
  err(e) => println("${{tag}} err: ${{e}}"),
}}

effect fn main() -> Unit = {{
  let all = bytes.from_list(list.range(0, 256))
  let high_first = bytes.from_list(list.range(128, 256) + list.range(0, 128))
  let bare = http.new_request("PUT", "{base}/x", "", [("Content-Type", "application/octet-stream"), ("X-After", "z")])
  put("all", http.set_body_bytes(bare, all))!
  put("high-first", http.set_body_bytes(bare, high_first))!
  put("utf8-bytes", http.set_body_bytes(bare, bytes.from_string("λ€😀")))!
  put("text", http.set_body(http.set_body_bytes(bare, all), "plain"))!
  let held = http.set_body_bytes(bare, all)
  println("held: ${{bytes.len(http.body_bytes(held))}} ${{http.body_bytes(held) == all}}")
}}
"#
    )
}

/// The call handle (`http.start`) sends the same bytes. Its own program: the
/// embedded lane serves the call handle and the framed client on different
/// hosts (E081).
fn call_program(port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    format!(
        r#"import http

effect fn call(tag: String, req: HttpRequest) -> Unit = {{
  let c = http.start(req, {{ total_ms: 0, idle_ms: 0 }})!
  match http.wait(c) {{
    ok(r) => println("${{tag}}: ${{http.body(r)}}"),
    err(e) => println("${{tag}} err: ${{e}}"),
  }}
}}

effect fn main() -> Unit = {{
  let bare = http.new_request("PUT", "{base}/c", "", [("X-After", "call")])
  call("all", http.set_body_bytes(bare, bytes.from_list(list.range(0, 256))))!
  call("high-first", http.set_body_bytes(bare, bytes.from_list(list.range(128, 256) + list.range(0, 128))))!
  call("text", http.new_request("PUT", "{base}/c", "λλ", [("X-After", "call")]))!
}}
"#
    )
}

fn send_expected() -> String {
    let all: Vec<u8> = (0..=255u8).collect();
    let high_first: Vec<u8> = (128..=255u8).chain(0..128u8).collect();
    let utf8 = "λ€😀".as_bytes();
    format!(
        "all: len=256 after=z hex={}\nhigh-first: len=256 after=z hex={}\nutf8-bytes: len={} after=z hex={}\ntext: len=5 after=z hex={}\nheld: 256 true\n",
        hex(&all),
        hex(&high_first),
        utf8.len(),
        hex(utf8),
        hex(b"plain"),
    )
}

fn call_expected() -> String {
    let all: Vec<u8> = (0..=255u8).collect();
    let high_first: Vec<u8> = (128..=255u8).chain(0..128u8).collect();
    format!(
        "all: len=256 after=call hex={}\nhigh-first: len=256 after=call hex={}\ntext: len=4 after=call hex={}\n",
        hex(&all),
        hex(&high_first),
        hex("λλ".as_bytes()),
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

fn run(file: &Path, wasm: bool) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    observe(c)
}

/// Both programs on native and the embedded lane, every answer pinned.
fn assert_lanes(name: &str, src: String, want: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("write");
    let native = run(&file, false);
    assert_eq!((native.0.as_str(), native.2), (want, Some(0)), "{name} native: {}", native.1);
    let embedded = run(&file, true);
    assert_eq!((embedded.0.as_str(), embedded.2), (want, Some(0)), "{name} embedded: {}", embedded.1);
}

#[test]
fn send_carries_every_byte_on_native_and_the_embedded_lane() {
    assert_lanes("http_body_bytes_send", send_program(serve()), &send_expected());
}

#[test]
fn start_carries_every_byte_on_native_and_the_embedded_lane() {
    assert_lanes("http_body_bytes_call", call_program(serve()), &call_expected());
}

/// wasmtime runs p3 components from 46 on (the pin policy's floor).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
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
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor", v.trim());
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

#[test]
fn send_carries_every_byte_on_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_body_bytes.almd");
    std::fs::write(&file, send_program(port)).expect("write");
    let wasm = dir.path().join("http_body_bytes.wasm");
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", file.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", wasm.to_str().expect("utf8")])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("almide build");
    assert!(built.status.success(), "p3 build:\n{}", String::from_utf8_lossy(&built.stderr));
    let mut c = Command::new("wasmtime");
    c.args(["run", "-S", "http=y"]).arg(&wasm);
    let p3 = observe(c);
    let want = send_expected();
    assert_eq!((p3.0.as_str(), p3.2), (want.as_str(), Some(0)), "p3: {}", p3.1);
}
