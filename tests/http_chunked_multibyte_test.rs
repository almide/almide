//! #2536: a chunked response whose multibyte character straddles two chunks.
//!
//! The text clients used to lossily decode the WHOLE framed response before
//! removing the chunk framing, so the U+FFFD replacement changed the body's
//! length under the byte-counted chunk walk and slicing the `&str` panicked
//! ("end byte index N is not a char boundary"). The streaming client did
//! not panic but decoded each delivery on its own, so the split character
//! arrived as two U+FFFD pieces.
//!
//! One local server, one program that reads the same response through every
//! client shape: `request_response`, `request`, `request_status`,
//! `request_bytes` (the control that was already right) and
//! `request_stream` (each chunk flushed separately, so the split also lands
//! between two socket reads). Every body must be byte-equal to the payload.
use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

const BODY: &str = "<p>明日の天気</p>";

/// The chunked wire form of `BODY`, split one byte into `天`.
fn chunks() -> Vec<Vec<u8>> {
    let body = BODY.as_bytes();
    let split = BODY.find('天').unwrap() + 1;
    let mut out = Vec::new();
    for part in [&body[..split], &body[split..]] {
        let mut c = format!("{:x}\r\n", part.len()).into_bytes();
        c.extend_from_slice(part);
        c.extend_from_slice(b"\r\n");
        out.push(c);
    }
    out.push(b"0\r\n\r\n".to_vec());
    out
}

/// Build `main_body` as a client of a local server that answers each of its
/// `requests` connections with the split response, run it, and return its
/// stdout (panicking with the client's stderr if it failed).
fn run_client(main_body: &str, requests: usize) -> String {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("chunked.almd");
    let app = dir.path().join("chunked");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let main_body = main_body.replace("URL", &format!("http://127.0.0.1:{port}/"));
    std::fs::write(&source, format!("import http\neffect fn main() -> Unit = {{\n{main_body}\n}}\n")).unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into());
    let built = Command::new(bin).arg("build").arg(&source).arg("-o").arg(&app).output().expect("build client");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));

    // Start only after compilation succeeds; a regression cannot strand accept.
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        for _ in 0..requests {
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return; // the client died early; its stderr says why
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("accept: {e}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = std::io::BufReader::new(&mut socket);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let _ = socket.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            );
            for c in chunks() {
                let _ = socket.write_all(&c);
                let _ = socket.flush();
                // Separate TCP segments, so the split also falls between reads.
                std::thread::sleep(Duration::from_millis(30));
            }
        }
    });
    let output = Command::new(app).env("ALMIDE_HTTP_TIMEOUT_SECS", "5").output().unwrap();
    server.join().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "client failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

fn line(stdout: &str, tag: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(tag))
        .unwrap_or_else(|| panic!("no `{tag}` line in:\n{stdout}"))
        .to_string()
}

#[test]
fn a_character_split_across_chunks_reads_back_whole_in_every_text_client_shape() {
    let out = run_client(
        r#"  let resp = http.request_response("GET", "URL", "", [:])!
  println("response:" + http.body(resp))
  println("request:" + http.request("GET", "URL", "", [:])!)
  let (code, b) = http.request_status("GET", "URL", "", [:])!
  println("status:" + int.to_string(code) + ":" + b)
  let raw = http.request_bytes("GET", "URL", "", [:])!
  println("bytes:" + bytes.to_string(raw)!)"#,
        4,
    );
    assert_eq!(line(&out, "response:"), BODY);
    assert_eq!(line(&out, "request:"), BODY);
    assert_eq!(line(&out, "status:"), format!("200:{BODY}"));
    assert_eq!(line(&out, "bytes:"), BODY);
}

#[test]
fn a_character_split_across_chunks_streams_whole() {
    let out = run_client(
        r#"  var pieces: List[String] = []
  http.request_stream("GET", "URL", "", [:], (chunk: String) => list.push(pieces, chunk))!
  println("stream:" + string.join(pieces, ""))"#,
        1,
    );
    assert_eq!(line(&out, "stream:"), BODY);
}
