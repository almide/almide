//! A streamed body that ends before its framing says it is complete is an
//! `err`, not a silent `ok` (owner decision 2026-10-17, by cross-language
//! comparison: Go's io.ErrUnexpectedEOF, Python requests' ChunkedEncodingError,
//! curl's exit 18, reqwest's body-stream error and fetch's stream error all
//! report a truncated body). A body read through the call handle
//! (`http.start`, then `read_new` until `poll` answers) answers it on native
//! and on the embedded lane (`almide run --target wasm`) with C-328's
//! protocol class, `malformed or incomplete response from "<url>"`, the text
//! the buffered client already gives a truncated response. What arrived
//! before the cut is still read first.
//!
//! The loopback server answers:
//!   /length    `Content-Length: 20`, 5 bytes, then closes;
//!   /chunked   one chunk, then closes without the terminal `0` chunk;
//!   /whole     a complete `Content-Length` body (the control: still `ok`).
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn read_head(s: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match s.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    String::from_utf8_lossy(&buf).lines().next().unwrap_or("").to_string()
}

fn answer(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let line = read_head(&mut s);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let reply: &[u8] = match path.as_str() {
        "/length" => b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\nhello",
        "/chunked" => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n",
        _ => b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
    };
    let _ = s.write_all(reply);
    let _ = s.flush();
    // Let the delivered bytes arrive as their own read before the close.
    std::thread::sleep(Duration::from_millis(150));
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
        r#"import env
import http

effect fn piece(tag: String, c: HttpCall) -> Unit = {{
  let p = http.read_new(c)
  if p == "" then () else println(tag + " chunk[" + p + "]")
}}

effect fn drain(tag: String, c: HttpCall) -> Result[Unit, String] = match http.poll(c) {{
  none => {{
    piece(tag, c)!
    env.sleep_ms(5)
    drain(tag, c)
  }},
  some(r) => {{
    piece(tag, c)!
    match r {{
      ok(_) => ok(()),
      err(e) => err(e),
    }}
  }},
}}

fn said(tag: String, r: Result[Unit, String]) -> Unit = match r {{
  ok(_) => println("${{tag}} ok"),
  err(e) => println("${{tag}} err ${{e}}"),
}}

effect fn main() -> Unit = {{
  for p in ["length", "chunked", "whole"] {{
    let url = "http://127.0.0.1:{port}/" + p
    let c = http.start(http.new_request("GET", url, "", []), {{ total_ms: 20000, idle_ms: 0 }})!
    said("handle " + p, drain("handle " + p, c))
  }}
}}
"#
    )
}

fn expected(port: u16) -> String {
    let mut out = String::new();
    for p in ["length", "chunked", "whole"] {
        out.push_str(&format!("handle {p} chunk[hello]\n"));
        if p == "whole" {
            out.push_str(&format!("handle {p} ok\n"));
        } else {
            out.push_str(&format!("handle {p} err malformed or incomplete response from \"http://127.0.0.1:{port}/{p}\"\n"));
        }
    }
    out
}

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
fn a_streamed_body_cut_short_of_its_framing_is_an_err_on_native_and_the_embedded_lane() {
    let port = serve();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("truncated.almd");
    std::fs::write(&file, program(port)).expect("write");
    let want = expected(port);
    let native = run(&file, false);
    assert_eq!((native.0.as_str(), native.2), (want.as_str(), Some(0)), "native: {}", native.1);
    let embedded = run(&file, true);
    assert_eq!((embedded.0.as_str(), embedded.2), (want.as_str(), Some(0)), "the embedded lane: {}", embedded.1);
}
