//! #4203: a runtime abort inside an `http.serve` handler fails only that
//! request. The server answers it with the fixed C-409 500, writes one stderr
//! line naming the request and the abort, and keeps serving.
//!
//! The handler indexes a 3-element list by the request body's length, so a
//! 1-byte body is served and a 5-byte body reaches an out-of-bounds index.
//! Before the fix the second request got an empty reply and every later
//! client got "connection refused".
//!
//! Native leg only; tests/http_serve_cross_test.rs runs the same requests
//! on native and the embedded wasm lane and compares them byte for byte.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SERVER: &str = r#"import http
effect fn main() -> Unit =
  http.serve(PORT, (req) => {
    let xs = [10, 20, 30]
    let i = string.len(http.body(req))
    http.response(200, "val=" + int.to_string(xs[i]))
  })
"#;

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(port: u16, body: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    write!(s, "POST / HTTP/1.1\r\nHost: a\r\nContent-Length: {}\r\n\r\n{}", body.len(), body)?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

/// Wait until the server answers a served request (it compiles first).
fn wait_up(port: u16) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(240) {
        if let Ok(r) = request(port, "X")
            && r.starts_with("HTTP/1.1 200") {
                return;
            }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("the server never came up on {port}");
}

#[test]
fn an_abort_in_the_handler_fails_only_its_request() {
    let dir = tempfile::tempdir().unwrap();
    let port = 20000 + (std::process::id() % 20000) as u16;
    std::fs::write(dir.path().join("s.almd"), SERVER.replace("PORT", &port.to_string())).unwrap();
    let stderr_path = dir.path().join("stderr.txt");
    let child = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "s.almd"])
        .current_dir(dir.path())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr_path).unwrap())
        .spawn()
        .expect("spawn almide run");
    let mut server = Server(child);
    wait_up(port);

    // Control: an in-bounds request is served.
    let ok = request(port, "X").expect("served request");
    assert!(ok.starts_with("HTTP/1.1 200") && ok.ends_with("val=20"), "{ok}");

    // The out-of-bounds request: the fixed 500, not an empty reply.
    let bad = request(port, "XXXXX").expect("the aborting request is answered");
    assert!(bad.starts_with("HTTP/1.1 500"), "{bad}");
    assert!(bad.contains("Internal Server Error (request "), "the C-409 body: {bad}");
    assert!(!bad.contains("out of bounds"), "the abort text never reaches the client: {bad}");

    // The server is still up and serving.
    let after = request(port, "X").expect("the server survives the abort");
    assert!(after.starts_with("HTTP/1.1 200") && after.ends_with("val=20"), "{after}");
    assert!(server.0.try_wait().unwrap().is_none(), "the server process is still running");

    drop(server);
    let stderr = std::fs::read_to_string(&stderr_path).unwrap();
    assert!(
        stderr.lines().any(|l| l.starts_with("http.serve: request ") && l.ends_with("handler aborted: \"index out of bounds\"")),
        "one stderr line names the request and the abort:\n{stderr}"
    );
    assert!(!stderr.contains("panicked"), "no Rust panic banner:\n{stderr}");
}
