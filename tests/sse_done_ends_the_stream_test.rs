//! OpenAI's `data: [DONE]` ends the stream: nothing after it is applied.
//!
//! The native SSE helper (runtime/rs/src/sse.rs) set a `done` flag that only
//! the NEXT chunk checked, so an event that arrived after `[DONE]` in the
//! same chunk was still parsed — its text reached the callback and the
//! returned content. A loopback server sends the whole stream in one write.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

const STREAM: &str = "data: {\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"kept\"}}]}\n\n\
data: [DONE]\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" AFTER\"}}]}\n\n";

fn serve_once(listener: TcpListener) {
    let (mut sock, _) = listener.accept().expect("accept");
    let mut buf = [0u8; 8192];
    let mut req = Vec::new();
    // read the head and the (small) body
    loop {
        let n = sock.read(&mut buf).expect("read");
        req.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&req);
        if let Some(head_end) = text.find("\r\n\r\n") {
            let len = text[..head_end]
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                .unwrap_or(0);
            if req.len() >= head_end + 4 + len { break; }
        }
        if n == 0 { break; }
    }
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
    sock.write_all(head.as_bytes()).expect("head");
    sock.write_all(STREAM.as_bytes()).expect("stream");
}

#[test]
fn an_event_after_done_in_the_same_chunk_is_not_applied() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || serve_once(listener));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("done.almd");
    std::fs::write(&file, format!("import http\n\neffect fn main() -> Unit = {{\n  let r = http.openai_streaming_call(\"http://127.0.0.1:{port}\", \"k\", \"{{}}\", (d) => println(\"delta[${{d}}]\"))\n  match r {{\n    ok(s) => println(s),\n    err(e) => println(\"err ${{e}}\"),\n  }}\n}}\n")).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("run");
    server.join().expect("server");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("delta[kept]"), "{stdout}");
    assert!(!stdout.contains("AFTER"), "an event after [DONE] was applied:\n{stdout}");
    assert!(stdout.contains("\"content\":\"kept\""), "{stdout}");
}
