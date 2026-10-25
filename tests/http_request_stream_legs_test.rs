//! A streamed body read through the call handle (`http.start`, then
//! `read_new` until `poll` answers — R-http-01 retired the callback clients)
//! on native and on the embedded lane (`almide run --target wasm`): the same
//! piece sequence, the same outcome, the same transport error texts.
//!
//! The loopback server paces its writes (80 ms between pieces), so each piece
//! reaches the client as its own arrival and the delivery sequence is the
//! server's, not the scheduler's:
//!   /chunked   `Transfer-Encoding: chunked`, three chunks, the second ending
//!              inside the two-byte `é` — the split character is delivered
//!              whole, with the next chunk;
//!   /length    a `Content-Length` body in two writes;
//!   /silent    a head-less socket that never answers, for the call's
//!              `idle_ms` limit, whose text is the handle's limit error on
//!              both legs.
//! A closed port is C-328's connect class.
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

fn paced(s: &mut TcpStream, pieces: &[&[u8]]) {
    for p in pieces {
        let _ = s.write_all(p);
        let _ = s.flush();
        std::thread::sleep(Duration::from_millis(80));
    }
}

fn answer(mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let line = read_head(&mut s);
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    match path.as_str() {
        "/chunked" => paced(
            &mut s,
            &[
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                b"4\r\none \r\n",
                b"3\r\ntw\xC3\r\n",
                b"8\r\n\xA9o three\r\n",
                b"0\r\n\r\n",
            ],
        ),
        "/length" => paced(&mut s, &[b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello", b" world"]),
        "/silent" => std::thread::sleep(Duration::from_secs(5)),
        _ => {
            let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    }
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

fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    l.local_addr().expect("addr").port()
}

fn program(port: u16, dead: u16) -> String {
    format!(
        r#"import env
import http

effect fn said(tag: String, c: HttpCall) -> Unit = {{
  let p = http.read_new(c)
  if p == "" then () else println("${{tag}} chunk[${{p}}]")
}}

effect fn drain(tag: String, c: HttpCall) -> Result[Unit, String] = match http.poll(c) {{
  none => {{
    said(tag, c)!
    env.sleep_ms(5)
    drain(tag, c)
  }},
  some(r) => {{
    said(tag, c)!
    match r {{
      ok(_) => ok(()),
      err(e) => err(e),
    }}
  }},
}}

effect fn show(tag: String, url: String) -> Unit = {{
  let outcome = match http.start(http.new_request("GET", url, "", []), {{ total_ms: 0, idle_ms: 1000 }}) {{
    ok(c) => drain(tag, c),
    err(e) => err(e),
  }}
  match outcome {{
    ok(_) => println("${{tag}} ok"),
    err(e) => println("${{tag}} err ${{e}}"),
  }}
}}

effect fn main() -> Unit = {{
  show("chunked", "http://127.0.0.1:{port}/chunked")!
  show("length", "http://127.0.0.1:{port}/length")!
  show("silent", "http://127.0.0.1:{port}/silent")!
  show("closed", "http://127.0.0.1:{dead}/x")!
  println("done")
}}
"#
    )
}

fn expected(_port: u16, dead: u16) -> String {
    format!(
        "chunked chunk[one ]\nchunked chunk[tw]\nchunked chunk[\u{e9}o three]\nchunked ok\n\
length chunk[hello]\nlength chunk[ world]\nlength ok\n\
silent err request timeout: idle_ms 1000 exceeded\n\
closed err could not connect to \"http://127.0.0.1:{dead}/x\" (connection refused or unreachable)\n\
done\n"
    )
}

fn run(file: &Path, wasm: bool) -> (String, String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(file).env("ALMIDE_HTTP_TIMEOUT_SECS", "1");
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.stdin(Stdio::null()).output().expect("spawn");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned(), out.status.code())
}

#[test]
fn a_streamed_body_read_through_the_handle_is_the_same_on_native_and_the_embedded_lane() {
    let port = serve();
    let dead = closed_port();
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("handle_stream.almd");
    std::fs::write(&file, program(port, dead)).expect("write");
    let want = expected(port, dead);
    let native = run(&file, false);
    assert_eq!((native.0.as_str(), native.2), (want.as_str(), Some(0)), "native: {}", native.1);
    let embedded = run(&file, true);
    assert_eq!((embedded.0.as_str(), embedded.2), (want.as_str(), Some(0)), "the embedded lane: {}", embedded.1);
}
