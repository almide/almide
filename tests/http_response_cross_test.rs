//! The full-response http client (`get_response` … `request_response`,
//! #1791) on the stock WASI 0.3 component (ADR-0023 step 4): every verb must
//! answer what native answers through the response accessors — the status,
//! `get_header`, every value of a repeated field in order (`header_values`),
//! the lowercased header set (`headers`) and the body — for a 200, a 404 and
//! an unfollowed 302, and the same err text for a closed port. That is C-328's
//! p3 comparison: the header SET, lowercased, since wasi:http fields need not
//! keep the wire spelling or the order across names. The set leaves out the
//! fields the client manages (C-370's nine): a stock wasi:http host withholds
//! them from a response from wasmtime 49 on, so every lane drops them, and the
//! `/ok` reply carries `Keep-Alive` beside `Connection` to show it.
//!
//! The server is a loopback listener with canned replies, so no network is
//! needed. The embedded lane gets the family at ADR-0023 step 7.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Read one request: the head, then a Content-Length body.
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
    let len = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
        .unwrap_or(0);
    while buf.len() < head_end + len {
        let n = s.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
    let first = head.lines().next().unwrap_or("").to_string();
    let x_echo = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().starts_with("x-echo:").then(|| l[7..].trim().to_string()))
        .unwrap_or_default();
    (first, body, x_echo)
}

fn reply(first: &str, body: &str, x_echo: &str) -> Vec<u8> {
    let path = first.split_whitespace().nth(1).unwrap_or("/");
    let method = first.split_whitespace().next().unwrap_or("");
    let (status, extra, payload) = match path {
        "/ok" => ("200 OK", "X-A: 1\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nX-Multi: v\r\nKeep-Alive: timeout=5\r\n", "héllo\nworld".to_string()),
        "/missing" => ("404 Not Found", "X-Why: gone\r\n", "nope".to_string()),
        "/redir" => ("302 Found", "Location: /ok\r\n", String::new()),
        _ => ("200 OK", "", format!("{method}|{body}|{x_echo}")),
    };
    format!("HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len()).into_bytes()
}

/// Serve canned replies on a loopback port until the test ends.
fn serve() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
                let (first, body, x_echo) = read_request(&mut s);
                let _ = s.write_all(&reply(&first, &body, &x_echo));
            });
        }
    });
    port
}

/// A port nothing listens on.
fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

fn program(port: u16, dead: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    format!(
        r#"import http

fn show(tag: String, r: Result[http.HttpResponse, String]) -> String = match r {{
  ok(resp) => {{
    let cookies = http.header_values(resp, "SET-COOKIE") |> list.join(",")
    let a = http.get_header(resp, "x-a") ?? "-"
    let loc = http.get_header(resp, "location") ?? "-"
    let keys = map.keys(http.headers(resp)) |> list.sort |> list.join(",")
    "${{tag}} ${{int.to_string(http.status_code(resp))}} a=${{a}} loc=${{loc}} cookies=${{cookies}} keys=${{keys}} body=[${{http.body(resp)}}]"
  }},
  err(e) => "${{tag}} err: ${{e}}",
}}

effect fn main() -> Unit = {{
  println(show("get", http.get_response("{base}/ok")))
  println(show("missing", http.get_response("{base}/missing")))
  println(show("redir", http.get_response("{base}/redir")))
  println(show("post", http.post_response("{base}/echo", "p-body")))
  println(show("put", http.put_response("{base}/echo", "u-body")))
  println(show("patch", http.patch_response("{base}/echo", "")))
  println(show("delete", http.delete_response("{base}/echo")))
  println(show("request", http.request_response("OPTIONS", "{base}/echo", "r-body", map.from_list([("X-Echo", "λ v")]))))
  println(show("closed", http.get_response("http://127.0.0.1:{dead}/ok")))
  // The plain client's empty PATCH (op 46): the same request framing as
  // patch_response's — no content, not a chunked `0\r\n\r\n`.
  match http.patch("{base}/echo", "") {{
    ok(b) => println("plain-patch [${{b}}]"),
    err(e) => println("plain-patch err: ${{e}}"),
  }}
}}
"#
    )
}

fn observe(mut c: Command) -> (String, String, Option<i32>) {
    let out = c.stdin(Stdio::null()).output().expect("spawn");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.code())
}

fn native(file: &Path) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    observe(c)
}

/// Build `file` as a WASI 0.3 component and run it on stock wasmtime.
fn p3(file: &Path, scratch: &Path) -> (String, String, Option<i32>) {
    let wasm = scratch.join("http_response.wasm");
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", file.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", wasm.to_str().expect("utf8")])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("almide build");
    assert!(built.status.success(), "p3 build:\n{}", String::from_utf8_lossy(&built.stderr));
    let mut c = Command::new("wasmtime");
    c.args(["run", "-S", "http=y"]).arg(&wasm);
    observe(c)
}

/// wasmtime runs p3 components from 46 on (the pin policy's floor).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v.split_whitespace().nth(1).and_then(|x| x.split('.').next()).and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
    if major < 46 {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor", v.trim());
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

#[test]
fn the_full_response_family_reads_the_same_on_native_and_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let port = serve();
    let dead = closed_port();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_response.almd");
    std::fs::write(&file, program(port, dead)).expect("write");
    let native = native(&file);
    assert_eq!(native.2, Some(0), "native failed: {}", native.1);
    // The native answer is the reference; pin its shape so a native change
    // cannot pass by moving both legs.
    assert!(native.0.contains("get 200 a=1 loc=- cookies=a=1,b=2 keys=content-length,set-cookie,x-a,x-multi "), "{}", native.0);
    assert!(native.0.contains("redir 302 a=- loc=/ok "), "{}", native.0);
    assert!(native.0.contains("request 200 ") && native.0.contains("body=[OPTIONS|r-body|λ v]"), "{}", native.0);
    assert!(native.0.contains("closed err: "), "{}", native.0);
    assert!(native.0.contains("patch 200 a=- loc=- cookies= keys=content-length body=[PATCH||]"), "{}", native.0);
    assert!(native.0.contains("plain-patch [PATCH||]"), "{}", native.0);
    let p3 = p3(&file, dir.path());
    assert_eq!((&p3.0, p3.2), (&native.0, native.2), "the p3 component diverged: {}", p3.1);
}
