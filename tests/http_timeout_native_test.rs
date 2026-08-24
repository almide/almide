//! #1561: the native HTTP client's read timeout is CONFIGURABLE via
//! ALMIDE_HTTP_TIMEOUT_SECS (0 = no timeout — block until the server
//! answers). The old hardcoded 30 s killed any call to a slow endpoint
//! (a local-LLM server routinely needs 30-120 s before the first byte)
//! with the unactionable `read failed: Resource temporarily unavailable
//! (os error 35)`; the timeout case now names the env var.
//!
//! Pins BOTH directions fast (no 30 s wait in CI): a 3 s in-test server
//! against a 1 s timeout must fail with the actionable message, and the
//! same server under `0` (blocking) must succeed. One compiled binary,
//! two runs. NATIVE-only by nature (std::net client).

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

/// A one-shot slow HTTP server: accept, drain the request head, sleep,
/// answer 200. Returns the bound port; serves `n` connections then exits.
fn spawn_slow_server(sleep_ms: u64, n: usize) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut sock, _)) = listener.accept() else { return };
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf);
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            let body = b"{\"ok\":true}";
            let _ = write!(
                sock,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(body);
        }
    });
    port
}

#[test]
fn http_read_timeout_is_env_configurable() {
    let dir = std::env::temp_dir().join(format!("almd_http_to_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"httpto\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [\"Net\", \"IO\"]\n",
    )
    .unwrap();
    let port = spawn_slow_server(3000, 2);
    std::fs::write(
        dir.join("src/main.almd"),
        format!(
            "import http\n\n\
             effect fn main() -> Unit = {{\n  \
               var hs = map.new()\n  \
               hs = map.set(hs, \"Content-Type\", \"application/json\")\n  \
               match http.request(\"POST\", \"http://127.0.0.1:{port}/x\", \"{{}}\", hs) {{\n    \
                 ok(body) => println(\"ok: \" + string.take(body, 30)),\n    \
                 err(e) => println(\"err: \" + e),\n  }}\n}}\n"
        ),
    )
    .unwrap();
    let app = dir.join("app");
    let out = Command::new(almide_bin())
        .args(["build", dir.join("src/main.almd").to_str().unwrap(), "-o", app.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide build");
    assert!(
        out.status.success(),
        "almide build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 1 s timeout vs a 3 s server: must fail FAST with the actionable message.
    let out = Command::new(&app)
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "1")
        .output()
        .expect("failed to run app (timeout leg)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("read timed out waiting for the server")
            && stdout.contains("ALMIDE_HTTP_TIMEOUT_SECS"),
        "timeout leg must surface the actionable message, got:\n{stdout}"
    );

    // 0 = blocking: the same slow server must succeed.
    let out = Command::new(&app)
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "0")
        .output()
        .expect("failed to run app (blocking leg)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("ok:"),
        "blocking leg must succeed against the slow server, got:\n{stdout}"
    );

    std::fs::remove_dir_all(&dir).ok();
}
