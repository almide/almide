//! The native ⇄ EMBEDDED-lane server net (#2650, C-367). The server fixture
//! `spec/serve_cross/http_serve_replay.almd` never exits, so no generic
//! runner executes it: this driver starts it with `almide run` natively and
//! with `almide run --target wasm` (the embedded almide.* host), waits for
//! its "ready" line on stderr, replays ONE request script against each and
//! compares what C-367 promises (ADR-0020 §7.1, #2700), not the raw bytes:
//!
//! - each response's status code, header set and de-framed body. The header
//!   set is the (name, value) pairs with names ASCII-case-insensitive, fields
//!   of different names unordered and fields of one name in their relative
//!   order (RFC 9110 §5.3); the host-managed fields `date`, `connection`,
//!   `keep-alive`, `transfer-encoding` and `content-length` are left out, and
//!   the reason phrase is not compared (hyper writes `418 I'm a teapot` where
//!   the native core writes `418 OK`, measured by the #2659 prototype);
//! - the stderr transcripts as multisets of lines: the order of lines across
//!   requests is not promised.
//!
//! Two more properties ride along:
//!
//! - one instance per run: `/boot` answers a random draw main made once and
//!   the handler captured — two requests of one run must see the same value
//!   (a per-request instance would draw again);
//! - the bind failure: on an occupied port both legs abort with the same
//!   stderr line and exit code.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/serve_cross/http_serve_replay.almd")
}

/// The request script: GET, POST with a UTF-8 body and a header, a header
/// read that misses, a percent-encoded query (a pair without `=`, a value
/// holding `=`, `+`), a status outside the reason table plus a lowercase
/// header name, a redirect, a 404, a handler err (500), another method, and
/// a clock read in main and in the handler (#2703: a serving program walled
/// its clock reads on the embedded lane).
const SCRIPT: &[&[u8]] = &[
    b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n",
    "POST /echo HTTP/1.1\r\nHost: t\r\nX-Token: abc\r\nContent-Length: 12\r\n\r\n日本語 ok".as_bytes(),
    b"GET /echo HTTP/1.1\r\n\r\n",
    b"GET /query?b=2&a=%E7%8C%AB&c=x+y&nokey&d=1=2 HTTP/1.1\r\n\r\n",
    b"GET /teapot HTTP/1.1\r\n\r\n",
    b"GET /redirect HTTP/1.1\r\n\r\n",
    b"GET /missing HTTP/1.1\r\n\r\n",
    b"GET /fail HTTP/1.1\r\n\r\n",
    b"DELETE /hello HTTP/1.1\r\n\r\n",
    b"GET /clock HTTP/1.1\r\n\r\n",
];

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port").local_addr().expect("addr").port()
}

fn start(wasm: bool, port: u16) -> Child {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg(fixture());
    if wasm {
        c.args(["--target", "wasm"]);
    }
    c.arg("--").arg(port.to_string());
    // Own process group: the kill below must take the whole tree (the wasm
    // lane and a Windows-style launcher are not one process with the program).
    c.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.spawn().expect("almide runs")
}

fn kill_group(child: &mut Child) {
    // `-9 -- -<pgid>`, the one spelling both kills agree on (measured in
    // ubuntu:24.04): procps `kill -9 -<pgid>` exits 0 and kills NOTHING, and
    // `-s KILL -- -<pgid>` is a usage error there — either left the server
    // running and the wait below hung the CI job until its timeout.
    let group = Command::new("kill").args(["-9", "--", &format!("-{}", child.id())]).status();
    assert!(group.as_ref().is_ok_and(|s| s.success()), "could not kill the server's process group: {group:?}");
    let _ = child.kill();
    let _ = child.wait();
}

/// One request on a fresh connection; the answer is everything up to the
/// server's close. Connection attempts retry while the server binds (its
/// "ready" line is printed just before `http.serve`).
fn ask(port: u16, raw: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut conn = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => break c,
            Err(e) => {
                assert!(Instant::now() < deadline, "server on {port} never accepted: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    conn.set_read_timeout(Some(Duration::from_secs(60))).expect("timeout");
    conn.write_all(raw).expect("send request");
    let mut out = Vec::new();
    conn.read_to_end(&mut out).expect("read response to close");
    out
}

/// A response as C-367 observes it. Two responses answer the same exactly
/// when their `Observed` values are equal.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    status: u16,
    /// Lowercased name → values in wire order. The map drops the order
    /// between different names; the Vec keeps the order within one name.
    headers: BTreeMap<String, Vec<String>>,
    body: Vec<u8>,
}

/// The fields a host manages: the framing and the connection's lifetime.
const HOST_FIELDS: &[&str] = &["date", "connection", "keep-alive", "transfer-encoding", "content-length"];

/// Parses one HTTP/1.1 response read to the connection's close.
fn observe(raw: &[u8]) -> Result<Observed, String> {
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no end of the response head")?;
    let head = std::str::from_utf8(&raw[..cut]).map_err(|e| format!("the head is not UTF-8: {e}"))?;
    let rest = &raw[cut + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    // `HTTP/1.1 <status> <reason>`: the reason phrase is the host's.
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/") {
        return Err(format!("not a status line: {status_line:?}"));
    }
    let status = parts
        .next()
        .and_then(|c| (c.len() == 3).then_some(c).and_then(|c| c.parse::<u16>().ok()))
        .ok_or_else(|| format!("no status code in {status_line:?}"))?;
    let mut headers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| format!("a header line without a colon: {line:?}"))?;
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches([' ', '\t']).to_string();
        match name.as_str() {
            "content-length" => length = Some(value.parse::<usize>().map_err(|e| format!("content-length {value:?}: {e}"))?),
            "transfer-encoding" => chunked |= value.to_ascii_lowercase().split(',').any(|c| c.trim() == "chunked"),
            _ => {}
        }
        if !HOST_FIELDS.contains(&name.as_str()) {
            headers.entry(name).or_default().push(value);
        }
    }
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = length {
        rest.get(..n).ok_or_else(|| format!("content-length {n} but {} body bytes", rest.len()))?.to_vec()
    } else {
        rest.to_vec()
    };
    Ok(Observed { status, headers, body })
}

/// Decodes a chunked body (RFC 9112 §7.1); trailers are ignored.
fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("a chunk size line without CRLF")?;
        let line = std::str::from_utf8(&rest[..eol]).map_err(|e| format!("chunk size: {e}"))?;
        let size = line.split(';').next().unwrap_or_default().trim();
        let n = usize::from_str_radix(size, 16).map_err(|e| format!("chunk size {size:?}: {e}"))?;
        rest = &rest[eol + 2..];
        if n == 0 {
            return Ok(body);
        }
        body.extend_from_slice(rest.get(..n).ok_or("a chunk shorter than its size")?);
        rest = rest.get(n..).and_then(|r| r.strip_prefix(b"\r\n")).ok_or("a chunk without its CRLF")?;
    }
}

/// C-367's response comparison: `Ok` when the two answer the same.
fn same_answer(a: &[u8], b: &[u8]) -> Result<(), String> {
    let (a, b) = (observe(a)?, observe(b)?);
    if a == b {
        Ok(())
    } else {
        Err(format!("{a:?}\n  vs\n{b:?}"))
    }
}

/// The stderr transcript as a multiset of lines.
fn line_multiset(s: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = s.lines().collect();
    lines.sort_unstable();
    lines
}

struct Run {
    responses: Vec<Vec<u8>>,
    boots: Vec<Vec<u8>>,
    stderr: String,
}

fn replay(wasm: bool) -> Run {
    let port = free_port();
    let mut child = start(wasm, port);
    let mut err = BufReader::new(child.stderr.take().expect("stderr piped"));
    let mut first = String::new();
    err.read_line(&mut first).expect("read the ready line");
    if first != "ready\n" {
        let mut rest = String::new();
        let _ = err.read_to_string(&mut rest);
        kill_group(&mut child);
        panic!("wasm={wasm}: expected the ready line, got {first:?}{rest}");
    }
    let responses: Vec<Vec<u8>> = SCRIPT.iter().map(|r| ask(port, r)).collect();
    let boots: Vec<Vec<u8>> = (0..2)
        .map(|_| {
            let r = ask(port, b"GET /boot HTTP/1.1\r\n\r\n");
            let cut = r.windows(4).position(|w| w == b"\r\n\r\n").expect("a response head") + 4;
            r[cut..].to_vec()
        })
        .collect();
    kill_group(&mut child);
    let mut stderr = first;
    let _ = err.read_to_string(&mut stderr);
    Run { responses, boots, stderr }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn http_serve_answers_the_same_status_headers_and_body_on_native_and_the_embedded_lane() {
    let native = replay(false);
    let wasm = replay(true);
    // The script's own shape, so an empty answer on both legs cannot pass.
    assert!(native.responses[0].ends_with(b"\r\n\r\nhello"), "{:?}", String::from_utf8_lossy(&native.responses[0]));
    assert!(
        native.responses[7].starts_with(b"HTTP/1.1 500 Internal Server Error\r\n")
            && native.responses[7].ends_with(b"Internal error: denied /fail"),
        "{:?}",
        String::from_utf8_lossy(&native.responses[7])
    );
    assert!(native.responses[9].ends_with(b"\r\n\r\ntrue true"), "{:?}", String::from_utf8_lossy(&native.responses[9]));
    assert_eq!(native.responses.len(), wasm.responses.len());
    for (i, (n, w)) in native.responses.iter().zip(&wasm.responses).enumerate() {
        if let Err(diff) = same_answer(n, w) {
            panic!(
                "request {i} ({:?}) answered differently:\n{diff}\nnative: {:?}\nwasm:   {:?}",
                String::from_utf8_lossy(&SCRIPT[i][..SCRIPT[i].len().min(40)]),
                String::from_utf8_lossy(n),
                String::from_utf8_lossy(w)
            );
        }
    }
    // Every request left its line: an empty transcript on both legs cannot pass.
    assert_eq!(native.stderr.lines().filter(|l| l.starts_with("GET ") || l.starts_with("POST ") || l.starts_with("DELETE ")).count(), SCRIPT.len() + 2, "{:?}", native.stderr);
    assert_eq!(line_multiset(&native.stderr), line_multiset(&wasm.stderr), "the stderr transcripts hold different lines");
    for run in [&native, &wasm] {
        assert_eq!(run.boots[0], run.boots[1], "one run answered two different draws: not one instance");
        assert!(!run.boots[0].is_empty());
    }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_bind_failure_aborts_identically_on_native_and_the_embedded_lane() {
    let held = TcpListener::bind("0.0.0.0:0").expect("hold a port");
    let port = held.local_addr().expect("addr").port();
    let run = |wasm: bool| {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(fixture());
        if wasm {
            c.args(["--target", "wasm"]);
        }
        c.arg("--").arg(port.to_string());
        c.stdin(Stdio::null()).output().expect("almide runs")
    };
    let native = run(false);
    let wasm = run(true);
    let native_err = String::from_utf8_lossy(&native.stderr);
    assert!(native_err.starts_with("ready\nError: bind failed: "), "{native_err:?}");
    assert_eq!(native.status.code(), Some(1));
    assert_eq!(native.status.code(), wasm.status.code());
    assert_eq!(native_err, String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(native.stdout, wasm.stdout);

    // NOT in tail position: `http.serve` is typed never-err, so the `!` is a
    // no-op and the native runtime's returned Err used to vanish here — the
    // program printed "after" and exited 0 with no server. Both legs abort.
    let dir = std::env::temp_dir().join(format!("almide-serve-bind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let prog = dir.join("nontail.almd");
    std::fs::write(
        &prog,
        format!(
            "import http\n\neffect fn main() -> Unit = {{\n  println(\"before\")\n  http.serve({port}, (req) => http.response(200, \"x\"))!\n  eprintln(\"after\")\n}}\n"
        ),
    )
    .expect("write the program");
    let run = |wasm: bool| {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(&prog);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        c.stdin(Stdio::null()).output().expect("almide runs")
    };
    for out in [run(false), run(true)] {
        assert_eq!(out.status.code(), Some(1), "{:?}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("Error: bind failed: "),
            "{:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    drop(held);
}

/// The comparator itself, on constructed responses: it must reject every
/// change C-367 observes and accept every change it leaves to the host.
#[test]
fn the_response_comparator_rejects_what_c367_observes_and_accepts_what_it_does_not() {
    let base = b"HTTP/1.1 418 OK\r\nX-Kind: teapot\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello";
    assert!(same_answer(base, base).is_ok());
    let rejected: &[(&str, &[u8])] = &[
        ("a changed status", b"HTTP/1.1 419 OK\r\nX-Kind: teapot\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello"),
        ("a changed header value", b"HTTP/1.1 418 OK\r\nX-Kind: kettle\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello"),
        ("a reordered same-name header", b"HTTP/1.1 418 OK\r\nX-Kind: teapot\r\nSet-Cookie: b=2\r\nSet-Cookie: a=1\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello"),
        ("a missing header", b"HTTP/1.1 418 OK\r\nX-Kind: teapot\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Length: 5\r\n\r\nhello"),
        ("a changed body", b"HTTP/1.1 418 OK\r\nX-Kind: teapot\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhellO"),
    ];
    for (what, other) in rejected {
        assert!(same_answer(base, other).is_err(), "{what} was accepted");
        assert!(same_answer(other, base).is_err(), "{what} was accepted (reversed)");
    }
    let accepted: &[(&str, &[u8])] = &[
        ("a reordered header with a different name", b"HTTP/1.1 418 OK\r\nContent-Type: text/plain\r\nSet-Cookie: a=1\r\nX-Kind: teapot\r\nSet-Cookie: b=2\r\nContent-Length: 5\r\n\r\nhello"),
        ("a different reason phrase", b"HTTP/1.1 418 I'm a teapot\r\nX-Kind: teapot\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nContent-Length: 5\r\n\r\nhello"),
        ("a different case in a header name", b"HTTP/1.1 418 OK\r\nx-kind: teapot\r\nset-cookie: a=1\r\nSET-COOKIE: b=2\r\ncontent-type: text/plain\r\ncontent-length: 5\r\n\r\nhello"),
        ("host-managed fields and chunked framing", b"HTTP/1.1 418 OK\r\nDate: Mon, 28 Sep 2026 00:00:00 GMT\r\nX-Kind: teapot\r\nConnection: close\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nhel\r\n2\r\nlo\r\n0\r\n\r\n"),
    ];
    for (what, other) in accepted {
        assert!(same_answer(base, other).is_ok(), "{what} was rejected: {:?}", same_answer(base, other));
        assert!(same_answer(other, base).is_ok(), "{what} was rejected (reversed)");
    }
    // The stderr multiset: order across requests is free, the lines are not.
    assert_eq!(line_multiset("ready\nGET /a\nGET /b\n"), line_multiset("ready\nGET /b\nGET /a\n"));
    assert_ne!(line_multiset("ready\nGET /a\nGET /a\n"), line_multiset("ready\nGET /a\n"));
}

// ── Shutdown (ADR-0020 §5.6, #2692, C-367) ──
//
// `spec/serve_cross/http_serve_shutdown.almd` runs with stdout redirected to
// a FILE, where stdout is 64 KiB-buffered: before #2692 a SIGTERM lost every
// line. Natively the two tests below run the BUILT binary; the launcher
// tests after them run the same fixture through `almide run` (#2809).

fn shutdown_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/serve_cross/http_serve_shutdown.almd")
}

fn scratch(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-serve-shutdown-{}-{what}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The native binary of the shutdown fixture, built once per test process.
fn shutdown_binary() -> PathBuf {
    static BIN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BIN.get_or_init(|| {
        let bin = scratch("bin").join("http_serve_shutdown");
        let out = Command::new(almide_bin())
            .arg("build")
            .arg(shutdown_fixture())
            .arg("-o")
            .arg(&bin)
            .stdin(Stdio::null())
            .output()
            .expect("almide build runs");
        assert!(out.status.success(), "almide build failed: {}", String::from_utf8_lossy(&out.stderr));
        bin
    })
    .clone()
}

/// A shutdown run: the server, its stdout file, its port.
struct Served {
    child: Child,
    out: PathBuf,
    port: u16,
}

/// Start the fixture with stdout to a file, wait for its ready line, and
/// answer one request — which proves `http.serve` armed its signal handling,
/// so the signals below never meet the default disposition.
fn serve_to_file(wasm: bool, what: &str) -> Served {
    let c = if wasm {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(shutdown_fixture()).args(["--target", "wasm", "--"]);
        c
    } else {
        Command::new(shutdown_binary())
    };
    serve_cmd_to_file(c, wasm, what)
}

/// [`serve_to_file`] natively through the launcher: `almide run <fixture>`.
fn launch_to_file(what: &str) -> Served {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg(shutdown_fixture()).arg("--");
    serve_cmd_to_file(c, false, what)
}

fn serve_cmd_to_file(mut c: Command, wasm: bool, what: &str) -> Served {
    let port = free_port();
    let out = scratch(what).join(if wasm { "wasm.out" } else { "native.out" });
    c.arg(port.to_string());
    c.process_group(0)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&out).expect("stdout file"))
        .stderr(Stdio::piped());
    let mut child = c.spawn().expect("the server starts");
    let mut err = BufReader::new(child.stderr.take().expect("stderr piped"));
    let mut first = String::new();
    err.read_line(&mut first).expect("read the ready line");
    if first != "ready\n" {
        kill_group(&mut child);
        panic!("wasm={wasm}: expected the ready line, got {first:?}");
    }
    let hello = ask(port, b"GET /hello HTTP/1.1\r\n\r\n");
    assert!(hello.ends_with(b"\r\n\r\nok /hello"), "wasm={wasm}: {:?}", String::from_utf8_lossy(&hello));
    Served { child, out, port }
}

fn sigterm(child: &Child) {
    let st = Command::new("kill").args(["-TERM", &child.id().to_string()]).status();
    assert!(st.as_ref().is_ok_and(|s| s.success()), "could not signal the server: {st:?}");
}

/// Send `raw` on a fresh connection from a thread; the answer is whatever
/// arrived before the close (empty when the server exits without answering).
fn ask_in_background(port: u16, raw: &'static [u8]) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Ok(mut conn) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = conn.set_read_timeout(Some(Duration::from_secs(90)));
            let _ = conn.write_all(raw);
            let _ = conn.read_to_end(&mut out);
        }
        out
    })
}

/// Wait for the server to exit on its own; a server still running after the
/// deadline is killed and the test fails (it did not stop on the signal).
fn exit_code_within(served: &mut Served, secs: u64) -> Option<i32> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(status) = served.child.try_wait().expect("poll the server") {
            return status.code();
        }
        if Instant::now() >= deadline {
            kill_group(&mut served.child);
            panic!("the server was still running {secs} s after the signal");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_sigterm_drains_the_request_in_flight_flushes_stdout_and_returns_from_serve_on_native_and_the_embedded_lane() {
    for wasm in [false, true] {
        let mut served = serve_to_file(wasm, "drain");
        // `/nap` sleeps 1 s in the handler; the signal lands inside it.
        let nap = ask_in_background(served.port, b"GET /nap HTTP/1.1\r\n\r\n");
        std::thread::sleep(Duration::from_millis(300));
        sigterm(&served.child);
        let code = exit_code_within(&mut served, 20);
        let answer = nap.join().expect("the client thread");
        let out = std::fs::read_to_string(&served.out).expect("read the stdout file");
        assert!(answer.ends_with(b"\r\n\r\nok /nap"), "wasm={wasm}: the request in flight was not answered: {:?}", String::from_utf8_lossy(&answer));
        // Every line, in order, and "stopped": `http.serve` returned and main went on.
        assert_eq!(out, "listening\nhit /hello\nhit /nap\nstopped\n", "wasm={wasm}");
        assert_eq!(code, Some(0), "wasm={wasm}: main's exit code");
        // The listener is gone: nothing accepts on the port any more.
        assert!(TcpStream::connect(("127.0.0.1", served.port)).is_err(), "wasm={wasm}: still accepting");
    }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_second_sigterm_during_the_drain_flushes_stdout_and_exits_1_on_native_and_the_embedded_lane() {
    for wasm in [false, true] {
        let mut served = serve_to_file(wasm, "force");
        // `/stall` sleeps 60 s: only the second signal ends the run.
        let stall = ask_in_background(served.port, b"GET /stall HTTP/1.1\r\n\r\n");
        std::thread::sleep(Duration::from_millis(300));
        sigterm(&served.child);
        std::thread::sleep(Duration::from_millis(300));
        sigterm(&served.child);
        let code = exit_code_within(&mut served, 20);
        let answer = stall.join().expect("the client thread");
        let out = std::fs::read_to_string(&served.out).expect("read the stdout file");
        assert_eq!(code, Some(1), "wasm={wasm}: a forced stop exits 1, never 128+signal (C-350)");
        // Every line written before the stop — including the one the stalled
        // handler printed — and no "stopped": `http.serve` did not return.
        assert_eq!(out, "listening\nhit /hello\nhit /stall\n", "wasm={wasm}");
        assert!(answer.is_empty(), "wasm={wasm}: the stalled request was answered: {:?}", String::from_utf8_lossy(&answer));
    }
}

// ── The launcher (#2809) ──
//
// `almide run` used to spawn the program and wait: a SIGTERM to the launcher's
// pid alone killed the launcher and left the server running, undrained. The
// launcher now execs the program (Unix), so the pid a supervisor holds IS the
// program's. Both runs below start in a process group of their own.

fn signal(target: &str, sig: &str) -> bool {
    Command::new("kill").args([sig, "--", target]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Some process of the group `pgid` still exists (a zombie is reaped by now).
fn group_alive(pgid: u32) -> bool {
    signal(&format!("-{pgid}"), "-0")
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_sigterm_to_the_almide_run_pid_alone_drains_the_program_and_leaves_no_process_behind() {
    let mut served = launch_to_file("launcher-term");
    let pgid = served.child.id();
    // The control: the orphan probe below sees a live group.
    assert!(group_alive(pgid), "the orphan probe does not see the running server's group");
    let nap = ask_in_background(served.port, b"GET /nap HTTP/1.1\r\n\r\n");
    std::thread::sleep(Duration::from_millis(300));
    // The launcher's pid ONLY, as a supervisor holding it sends it.
    assert!(signal(&pgid.to_string(), "-TERM"), "could not signal the launcher");
    let code = exit_code_within(&mut served, 20);
    let answer = nap.join().expect("the client thread");
    let out = std::fs::read_to_string(&served.out).expect("read the stdout file");
    assert!(answer.ends_with(b"\r\n\r\nok /nap"), "the request in flight was not answered: {:?}", String::from_utf8_lossy(&answer));
    // The PROGRAM drained: its stdout is whole, "stopped" included.
    assert_eq!(out, "listening\nhit /hello\nhit /nap\nstopped\n");
    // The status `almide run` exits with is the program's: main returned, 0.
    assert_eq!(code, Some(0), "almide run's exit code");
    assert!(!group_alive(pgid), "a process of the run outlived `almide run` (orphaned program)");
    assert!(TcpStream::connect(("127.0.0.1", served.port)).is_err(), "still accepting");
}

/// A terminal's Ctrl-C signals the whole foreground group. The program must
/// see it ONCE: a launcher that also forwarded it would deliver a second
/// signal, which is the forced stop (exit 1, no "stopped").
#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_sigint_to_the_almide_run_group_drains_once_and_is_not_a_forced_stop() {
    let mut served = launch_to_file("launcher-int");
    let pgid = served.child.id();
    let nap = ask_in_background(served.port, b"GET /nap HTTP/1.1\r\n\r\n");
    std::thread::sleep(Duration::from_millis(300));
    assert!(signal(&format!("-{pgid}"), "-INT"), "could not signal the run's group");
    let code = exit_code_within(&mut served, 20);
    let answer = nap.join().expect("the client thread");
    let out = std::fs::read_to_string(&served.out).expect("read the stdout file");
    assert!(answer.ends_with(b"\r\n\r\nok /nap"), "the request in flight was not answered: {:?}", String::from_utf8_lossy(&answer));
    assert_eq!(out, "listening\nhit /hello\nhit /nap\nstopped\n");
    assert_eq!(code, Some(0), "almide run's exit code");
    assert!(!group_alive(pgid), "a process of the run outlived `almide run`");
}
