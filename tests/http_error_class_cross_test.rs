//! The http client's error classes on three lanes (ADR-0023 step 2, C-328,
//! C-370): native (`almide run`), the embedded wasm host (`almide run
//! --target wasm`) and the stock WASI 0.3 component (`wasmtime run
//! -S http=y`). Every failure a program can trigger without the network —
//! a bad URL, a refused header, a closed port, a server that never answers,
//! a TLS handshake against a server that does not speak TLS, a response
//! past the size limit — must give the same stdout, stderr and exit code on
//! all three. The texts come from one table in almide-rt-core
//! (http_error_core.rs), which native and the embedded host render from and
//! the p3 shim lays out as data; this test is what holds the three to it.
//!
//! The runtime floor is the pin policy's (wasmtime 46); below it the p3 leg
//! skips, and under ALMIDE_EXPECT_TOOLS (CI) that is a failure.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().expect("utf8 path").to_string();
    }
    "almide".to_string()
}

/// The installed wasmtime runs p3 components (the pin policy's floor, 46).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v
        .split_whitespace()
        .nth(1)
        .and_then(|ver| ver.split('.').next())
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    if major < 46 {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor (46)",
            v.trim()
        );
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

/// What a leg observed: stdout, stderr, exit code.
type Seen = (String, String, Option<i32>);

/// Run `cmd` to completion under a 120 s watchdog.
fn observe(mut cmd: Command, what: &str) -> Seen {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect(what);
    let deadline = Instant::now() + Duration::from_secs(120);
    while child.try_wait().expect("try_wait").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("{what}: still running after 120 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect(what);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

/// The three legs of `src`, each with `env` set: `[(leg, seen)]`.
fn three_legs(src: &Path, env: &[(&str, &str)], scratch: &Path) -> Vec<(&'static str, Seen)> {
    let run = |extra: &[&str]| {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(src).args(extra);
        for (k, v) in env {
            c.env(k, v);
        }
        c
    };
    let mut legs = vec![("native", observe(run(&[]), "almide run")), ("embedded", observe(run(&["--target", "wasm"]), "almide run --target wasm"))];
    let wasm: PathBuf = scratch.join(format!("{}.wasm", src.file_stem().expect("stem").to_string_lossy()));
    let built = Command::new(almide_bin())
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", wasm.to_str().expect("utf8")])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("almide build");
    assert!(built.status.success(), "p3 build of {}:\n{}", src.display(), String::from_utf8_lossy(&built.stderr));
    let mut c = Command::new("wasmtime");
    c.args(["run", "-S", "http=y"]);
    for (k, v) in env {
        c.arg("--env").arg(format!("{k}={v}"));
    }
    c.arg(&wasm);
    legs.push(("p3", observe(c, "wasmtime run")));
    legs
}

/// Every leg saw what native saw; native's stdout is returned.
fn assert_same(name: &str, legs: &[(&'static str, Seen)]) -> String {
    let (_, native) = &legs[0];
    for (leg, seen) in &legs[1..] {
        assert_eq!(seen, native, "{name}: the {leg} leg differs from native");
    }
    assert!(!native.0.contains("os error"), "{name}: an OS errno reached the text:\n{}", native.0);
    native.0.clone()
}

#[test]
fn the_spec_fixtures_read_the_same_on_native_embedded_and_p3() {
    if !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross");
    for name in [
        "http_error_classes.almd",
        "http_header_refusal.almd",
        "http_url_redaction.almd",
        "http_url_redaction_dial.almd",
        "http_error_quote_c1_bidi.almd",
    ] {
        let out = assert_same(name, &three_legs(&dir.join(name), &[], scratch.path()));
        assert!(!out.contains("unexpected ok"), "{name}:\n{out}");
        assert_no_password(name, &out);
    }
}

/// C-328 (#4205): the redaction fixtures spell every userinfo password
/// `s3cret`; no error text may show it.
fn assert_no_password(name: &str, out: &str) {
    assert!(!out.contains("s3cret"), "{name}: a URL's password reached an error text:\n{out}");
}

/// A listener that accepts and never answers: the stalled server.
fn stalled() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        let mut held: Vec<TcpStream> = Vec::new();
        for s in l.incoming().flatten() {
            held.push(s);
        }
    });
    port
}

/// A listener that reads a request head and answers `reply`, then closes.
fn answering(reply: Vec<u8>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let reply = reply.clone();
            std::thread::spawn(move || {
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let _ = s.write_all(&reply);
            });
        }
    });
    port
}

#[test]
fn the_server_classes_read_the_same_on_native_embedded_and_p3() {
    if !wasmtime_runs_p3() {
        return;
    }
    let stall = stalled();
    let body = "x".repeat(100_000);
    let big = answering(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes());
    let plain = answering(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
    let scratch = tempfile::tempdir().expect("tempdir");
    let src = scratch.path().join("server_classes.almd");
    std::fs::write(
        &src,
        format!(
            r#"import http

fn show(tag: String, r: Result[String, String]) -> Unit = {{
  match r {{
    ok(b) => println("${{tag}}: ok ${{string.len(b)}}"),
    err(e) => println("${{tag}}: ${{e}}"),
  }}
}}

effect fn main() -> Unit = {{
  show("timeout", http.get("http://127.0.0.1:{stall}/slow"))
  show("too-large", http.get("http://127.0.0.1:{big}/big"))
  show("too-large-framed", http.request("GET", "http://127.0.0.1:{big}/big", "", [("X-Ok", "1")]))
  show("tls", http.get("https://127.0.0.1:{plain}/"))
}}
"#
        ),
    )
    .expect("write program");
    let env = [("ALMIDE_HTTP_TIMEOUT_SECS", "1"), ("ALMIDE_HTTP_MAX_RESPONSE_BYTES", "1000")];
    let out = assert_same("server classes", &three_legs(&src, &env, scratch.path()));
    let want = format!(
        "timeout: timed out waiting for \"http://127.0.0.1:{stall}/slow\" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)\n\
         too-large: response from \"http://127.0.0.1:{big}/big\" is larger than 1000 bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)\n\
         too-large-framed: response from \"http://127.0.0.1:{big}/big\" is larger than 1000 bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)\n\
         tls: TLS handshake with \"https://127.0.0.1:{plain}/\" failed\n"
    );
    assert_eq!(out, want);
}

/// A listener that answers each request with the `Host` and every
/// `Authorization` field it received (`|`-joined), and the target.
fn auth_echo() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let field = |name: &str| -> Vec<String> {
                    head.lines()
                        .filter_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
                        .collect()
                };
                let target = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let body = format!("host={} auth={} target={target}", field("host").join("|"), field("authorization").join("|"));
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            });
        }
    });
    port
}

#[test]
fn a_url_userinfo_is_sent_as_basic_credentials_on_native_embedded_and_p3() {
    // C-328 (#4205): the userinfo never reaches the authority a lane dials;
    // rt-core sends it as `Authorization: Basic` (percent-decoded, `user:`
    // without a `:`) unless the caller sent one, and the p3 shim sends the
    // same field value — so the host, the credentials and the target agree.
    if !wasmtime_runs_p3() {
        return;
    }
    let port = auth_echo();
    let scratch = tempfile::tempdir().expect("tempdir");
    let src = scratch.path().join("userinfo.almd");
    std::fs::write(
        &src,
        format!(
            r#"import http

fn show(tag: String, r: Result[String, String]) -> Unit = {{
  match r {{
    ok(b) => println("${{tag}}: ${{b}}"),
    err(e) => println("${{tag}}: err ${{e}}"),
  }}
}}

effect fn main() -> Unit = {{
  show("password", http.get("http://alice:s3cret@127.0.0.1:{port}/p?token=abc"))
  show("user-only", http.get("http://alice@127.0.0.1:{port}/p"))
  show("empty-password", http.get("http://alice:@127.0.0.1:{port}/p"))
  show("empty-userinfo", http.get("http://@127.0.0.1:{port}/p"))
  show("encoded", http.get("http://al%40ce:p%3Ass%zz:x@127.0.0.1:{port}/p"))
  show("long", http.get("http://abcdefghijklmnopqrstuvwxyz:0123456789@127.0.0.1:{port}/"))
  show("caller-auth", http.request("GET", "http://alice:s3cret@127.0.0.1:{port}/p", "", [("authorization", "Bearer t")]))
  show("framed", http.request("GET", "http://bob:pw@127.0.0.1:{port}/p", "", [("X-Ok", "1")]))
}}
"#
        ),
    )
    .expect("write program");
    let out = assert_same("userinfo", &three_legs(&src, &[], scratch.path()));
    let h = format!("127.0.0.1:{port}");
    let want = format!(
        "password: host={h} auth=Basic YWxpY2U6czNjcmV0 target=/p?token=abc\n\
         user-only: host={h} auth=Basic YWxpY2U6 target=/p\n\
         empty-password: host={h} auth=Basic YWxpY2U6 target=/p\n\
         empty-userinfo: host={h} auth= target=/p\n\
         encoded: host={h} auth=Basic YWxAY2U6cDpzcyV6ejp4 target=/p\n\
         long: host={h} auth=Basic YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo6MDEyMzQ1Njc4OQ== target=/\n\
         caller-auth: host={h} auth=Bearer t target=/p\n\
         framed: host={h} auth=Basic Ym9iOnB3 target=/p\n"
    );
    assert_eq!(out, want);
}
