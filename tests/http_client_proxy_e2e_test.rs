//! #2819 / #2822 / #2824 end to end: a BUILT Almide program (the generated
//! project, its Cargo manifest with the trust-store crate, and the streaming
//! client that lives in runtime/rs/src/http.rs rather than the shared core)
//! run with `HTTP_PROXY` pointing at a local forward proxy.
//!
//! The proxy answers every request itself: a `100 Continue`, then a chunked
//! body whose chunk carries an extension, holding the request line it
//! received. So one run shows, per client shape (`get`, `request_stream`,
//! `start` + `wait`), that the request went to the proxy in absolute-form and
//! that the answer was read past the interim head and the extension. The
//! last probe puts a CR/LF in a header value on the streaming path and
//! expects the refusal, with nothing sent.
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn a_built_program_goes_through_http_proxy_on_every_client_shape() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("proxied.almd");
    let app = dir.path().join("proxied");
    std::fs::write(
        &source,
        r#"import http

effect fn main() -> Unit = {
  println("get:" + http.get("http://origin.invalid:8080/get?a=1")!)
  var pieces: List[String] = []
  http.request_stream("GET", "http://origin.invalid/stream", "", [:], (c: String) => list.push(pieces, c))!
  println("stream:" + string.join(pieces, ""))
  let call = http.start("GET", "http://origin.invalid/start", "", [:], { total_ms: 10000, idle_ms: 0 })!
  println("start:" + http.body(http.wait(call)!))
  var dropped: List[String] = []
  match http.request_stream("GET", "http://origin.invalid/x", "", ["X-A": "1\r\nX-B: 2"], (c: String) => list.push(dropped, c)) {
    ok(_) => println("inject:unexpected-ok"),
    err(e) => println("inject:${e}"),
  }
}
"#,
    )
    .unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into());
    let built = Command::new(bin).arg("build").arg(&source).arg("-o").arg(&app).output().expect("build");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let proxy = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(60);
        while stopped.try_recv().is_err() && Instant::now() < deadline {
            let mut s = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap_or(0);
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
            }
            let body = format!("via:{}", line.trim_end());
            let _ = s.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
            let _ = s.write_all(
                format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x};ext=1\r\n{body}\r\n0\r\n\r\n", body.len())
                    .as_bytes(),
            );
            seen.push(line.trim_end().to_string());
        }
        seen
    });
    let out = Command::new(&app)
        .env("HTTP_PROXY", format!("http://127.0.0.1:{port}"))
        .env_remove("http_proxy")
        .env_remove("ALL_PROXY")
        .env_remove("all_proxy")
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .env_remove("REQUEST_METHOD")
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "10")
        .output()
        .unwrap();
    stop.send(()).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr));
    let want = [
        "get:via:GET http://origin.invalid:8080/get?a=1 HTTP/1.1",
        "stream:via:GET http://origin.invalid/stream HTTP/1.1",
        "start:via:GET http://origin.invalid/start HTTP/1.1",
        "inject:invalid header value for \"X-A\": it contains CR, LF, NUL or another control character, which would split the request",
    ];
    assert_eq!(stdout.lines().collect::<Vec<_>>(), want, "stdout:\n{stdout}");
    // Three requests reached the proxy; the injected one never did.
    let seen = proxy.join().unwrap();
    assert_eq!(seen.len(), 3, "{seen:?}");
}
