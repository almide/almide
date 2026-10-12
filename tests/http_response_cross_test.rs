//! The full-response http client (`get_response` … `request_response`,
//! #1791) on the embedded wasm host (`almide run --target wasm`, #3620): every
//! verb must answer the same HttpResponse native answers — the status, every
//! header line in wire order with repeats kept (read through `get_header`,
//! `header_values` and `headers`), and the body — for a 200, a 404 and an
//! unfollowed 302, and the same err text for a closed port.
//!
//! The server is a loopback listener with canned replies, so no network is
//! needed. The p3 component leg does not serve the family yet (#1710).

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
        "/ok" => ("200 OK", "X-A: 1\r\nSet-Cookie: a=1\r\nset-cookie: b=2\r\nX-Multi: v\r\n", "héllo\nworld".to_string()),
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
}}
"#
    )
}

fn run(file: &Path, wasm: bool) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.stdin(Stdio::null()).output().expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.code())
}

#[test]
fn the_full_response_family_reads_the_same_on_native_and_the_embedded_host() {
    let port = serve();
    let dead = closed_port();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("http_response.almd");
    std::fs::write(&file, program(port, dead)).expect("write");
    let native = run(&file, false);
    assert_eq!(native.2, Some(0), "native failed: {}", native.1);
    // The native answer is the reference; pin its shape so a native change
    // cannot pass by moving both legs.
    assert!(native.0.contains("get 200 a=1 loc=- cookies=a=1,b=2 "), "{}", native.0);
    assert!(native.0.contains("redir 302 a=- loc=/ok "), "{}", native.0);
    assert!(native.0.contains("request 200 ") && native.0.contains("body=[OPTIONS|r-body|λ v]"), "{}", native.0);
    assert!(native.0.contains("closed err: "), "{}", native.0);
    let embedded = run(&file, true);
    assert_eq!((&embedded.0, embedded.2), (&native.0, native.2), "embedded diverged: {}", embedded.1);
}
