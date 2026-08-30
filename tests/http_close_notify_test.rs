//! #1592: `http.request` against a server that closes WITHOUT a clean
//! shutdown (Google front-ends drop the TLS session with no close_notify;
//! rustls surfaces `UnexpectedEof`) must return the response when it is
//! syntactically COMPLETE — curl's and every browser's behavior — and must
//! still error on a TRUNCATED body (a partial payload is never silently
//! returned). The dirty tail is simulated std-only: the server sends its
//! response and then HOLDS the socket open, so the client's next read
//! errors (a 1 s ALMIDE_HTTP_TIMEOUT_SECS) instead of seeing a clean EOF —
//! the same read-error-after-the-body path the missing close_notify
//! reaches over TLS. One in-test server per direction, the
//! http_timeout_native_test pattern. NATIVE-only by nature.

use std::io::{Read, Write as _};
use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// One-shot server: answer with `Content-Length: {promised}` but only
/// `sent` body bytes, then HOLD the socket open (never a clean close) so
/// the client's next read ERRORS at its 1 s timeout.
fn spawn_abrupt_server(promised: usize, sent: usize) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else { return };
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf);
        let body: String = "x".repeat(sent);
        let _ = write!(
            sock,
            "HTTP/1.1 200 OK\r\nContent-Length: {promised}\r\nConnection: close\r\n\r\n{body}"
        );
        let _ = sock.flush();
        // Hold well past the client's timeout, then let the socket drop.
        std::thread::sleep(std::time::Duration::from_secs(8));
    });
    port
}

fn run_client(port: u16) -> (i32, String) {
    let dir = std::env::temp_dir().join("almide-http-close-notify");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = format!(
        r#"import http

effect fn main() -> Unit = {{
  match http.request("GET", "http://127.0.0.1:{port}/", "", map.new()) {{
    ok(resp) => println("OK ${{int.to_string(string.len(resp))}}"),
    err(e) => println("ERR ${{e}}"),
  }}
}}
"#
    );
    let file = dir.join(format!("probe_{port}.almd"));
    std::fs::write(&file, src).expect("write");
    let out = Command::new(almide_bin())
        .args(["run", file.to_str().unwrap()])
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "1")
        .output()
        .expect("spawn almide");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn complete_body_survives_an_abrupt_close() {
    if !tools_available() {
        return;
    }
    let port = spawn_abrupt_server(5, 5);
    let (code, out) = run_client(port);
    assert_eq!(code, 0, "client crashed:\n{out}");
    assert!(
        out.contains("OK 5"),
        "a COMPLETE response was discarded on the abrupt close (#1592):\n{out}"
    );
}

#[test]
fn truncated_body_still_errors() {
    if !tools_available() {
        return;
    }
    let port = spawn_abrupt_server(50, 3);
    let (code, out) = run_client(port);
    assert_eq!(code, 0, "client crashed:\n{out}");
    assert!(
        out.contains("ERR "),
        "a TRUNCATED body (3 of 50 promised bytes) was silently returned:\n{out}"
    );
}
