//! #2063: Almide callbacks (including shared mutation) cross the streaming ABI.
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn stream_accepts_expression_block_and_bound_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("stream.almd");
    let app = dir.path().join("stream");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::write(&source, format!(r#"
import http
effect fn main() -> Unit = {{
  var chunks: List[String] = []
  http.request_stream("GET", "http://127.0.0.1:{port}/", "", [:], (chunk: String) => list.push(chunks, chunk))!
  http.request_stream("GET", "http://127.0.0.1:{port}/", "", [:], (chunk: String) => {{ list.push(chunks, chunk) }})!
  let receive = (chunk: String) => {{ list.push(chunks, chunk) }}
  http.request_stream("GET", "http://127.0.0.1:{port}/", "", [:], receive)!
  assert_eq(string.join(chunks, ""), "hellohellohello")
  println("stream-ok")
}}
"#)).unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into());
    let built = Command::new(bin)
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&app)
        .output()
        .expect("build streaming client");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );

    // Start only after compilation succeeds; a regression cannot strand accept.
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        for response in [
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhe\r\n3\r\nllo\r\n0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
        ] {
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "client did not connect");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("accept: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = std::io::BufReader::new(&mut socket);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0, "truncated request");
                if line == "\r\n" {
                    break;
                }
            }
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    let output = Command::new(app)
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "5")
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "stream-ok");
}
