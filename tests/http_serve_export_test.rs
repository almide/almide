//! The stock serve export's driver (#2659, C-375). The server fixture
//! `spec/serve_cross/http_serve_export.almd` never exits, so no generic
//! runner executes it: this driver builds it with `almide build --target
//! wasm` (a `wasi:http/handler@0.3.0` component), serves the artifact with
//! `wasmtime serve` — no `-S` / `-W` flag, only the address the host owns —
//! and runs it natively with `almide run`, then replays ONE request script
//! against both and compares what C-375 promises under C-367's HTTP-semantic
//! comparison: each response's status code, header set (names ASCII
//! case-insensitive; `date`, `connection`, `keep-alive`, `transfer-encoding`
//! and `content-length` left out) and de-framed body — not the reason phrase.
//!
//! On the artifact alone it then checks native's limits (a body over 1 MiB
//! answers 413 and a request line over 8 KiB answers 414, the handler not
//! called; the 431 limits are compared against native in their own test),
//! a reused instance's heap staying flat over many requests (#3444),
//! concurrent requests (one handler in flight per instance; the
//! host serves the rest with more instances), and asks `/trap` last: the
//! export host answers its own 500 and discards the instance, where native
//! aborts the process.
//!
//! The execution half needs a wasmtime at the p3 floor (46); below it, or
//! without wasmtime, it skips — unless ALMIDE_EXPECT_TOOLS claims the tools.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/serve_cross/http_serve_export.almd")
}

/// A wasmtime that runs p3 components by default (the pin policy's floor,
/// `proofs/wasi-pin-policy.toml` [runtime].floor = 46).
fn wasmtime_serves_p3() -> bool {
    let major = Command::new("wasmtime")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().nth(1).map(str::to_string))
        .and_then(|v| v.split('.').next().and_then(|m| m.parse::<u32>().ok()))
        .unwrap_or(0);
    if major < 46 {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: no wasmtime at the p3 floor (46)");
        eprintln!("skipping the serve export's execution: no wasmtime at the p3 floor (46)");
        return false;
    }
    true
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port").local_addr().expect("addr").port()
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-serve-export-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Build the fixture for stock wasm: the artifact is the handler export.
fn build_export(dir: &Path) -> PathBuf {
    let out = dir.join("app.wasm");
    let r = Command::new(almide_bin())
        .arg("build")
        .arg(fixture())
        .args(["--target", "wasm", "-o"])
        .arg(&out)
        .output()
        .expect("almide build runs");
    assert!(r.status.success(), "almide build --target wasm failed:\n{}", String::from_utf8_lossy(&r.stderr));
    out
}

/// A server in its own process group, its output to a file.
fn spawn(mut c: Command, log: &Path) -> Child {
    let file = std::fs::File::create(log).expect("log file");
    c.process_group(0)
        .stdin(Stdio::null())
        .stdout(file.try_clone().expect("log handle"))
        .stderr(file);
    c.spawn().expect("server starts")
}

fn kill_group(child: &mut Child) {
    let _ = Command::new("kill").args(["-9", "--", &format!("-{}", child.id())]).status();
    let _ = child.kill();
    let _ = child.wait();
}

/// `wasmtime serve` on the artifact, with `flags` before it: (port, server).
fn serve_artifact(dir: &Path, wasm: &Path, flags: &[String]) -> (u16, Child) {
    let port = free_port();
    let mut wc = Command::new("wasmtime");
    wc.arg("serve").arg("--addr").arg(format!("127.0.0.1:{port}")).args(flags).arg(wasm);
    (port, spawn(wc, &dir.join("wasmtime.log")))
}

/// The fixture run natively: (port, server).
fn serve_native(dir: &Path) -> (u16, Child) {
    let port = free_port();
    let mut nc = Command::new(almide_bin());
    nc.arg("run").arg(fixture()).arg("--").arg(port.to_string());
    (port, spawn(nc, &dir.join("native.log")))
}

/// One request on a fresh connection, read to the server's close. Every
/// request asks to close (`wasmtime serve` keeps a connection alive
/// otherwise). Connection attempts retry while the server starts: the
/// native leg compiles first.
fn ask(port: u16, raw: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut conn = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => break c,
            Err(e) => {
                assert!(Instant::now() < deadline, "server on {port} never accepted: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    };
    conn.set_read_timeout(Some(Duration::from_secs(60))).expect("timeout");
    // A server that refuses mid-body (413) may close before the client is
    // done writing: what it answered is still on the socket.
    let _ = conn.write_all(raw);
    let mut out = Vec::new();
    let _ = conn.read_to_end(&mut out);
    out
}

/// A request with `Connection: close` added to its head.
fn request(method: &str, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut head = format!("{method} {target} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body.is_empty() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    let mut raw = head.into_bytes();
    raw.extend_from_slice(body);
    raw
}

/// A response as C-367's HTTP-semantic comparison observes it.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    status: u16,
    headers: BTreeMap<String, Vec<String>>,
    body: Vec<u8>,
}

const HOST_FIELDS: &[&str] = &["date", "connection", "keep-alive", "transfer-encoding", "content-length"];

fn observe(raw: &[u8]) -> Result<Observed, String> {
    // A `100 Continue` interim head comes before the response proper.
    let mut raw = raw;
    while raw.starts_with(b"HTTP/1.1 100") {
        let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("an interim head without its end")?;
        raw = &raw[cut + 4..];
    }
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no end of the response head")?;
    let head = std::str::from_utf8(&raw[..cut]).map_err(|e| format!("the head is not UTF-8: {e}"))?;
    let rest = &raw[cut + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| format!("no status code in {status_line:?}"))?;
    let mut headers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut length, mut chunked) = (None, false);
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| format!("a header line without a colon: {line:?}"))?;
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches([' ', '\t']).to_string();
        match name.as_str() {
            "content-length" => length = value.parse::<usize>().ok(),
            "transfer-encoding" => chunked |= value.to_ascii_lowercase().contains("chunked"),
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

fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("a chunk size line without CRLF")?;
        let size = std::str::from_utf8(&rest[..eol]).map_err(|e| format!("chunk size: {e}"))?;
        let n = usize::from_str_radix(size.split(';').next().unwrap_or_default().trim(), 16)
            .map_err(|e| format!("chunk size {size:?}: {e}"))?;
        rest = &rest[eol + 2..];
        if n == 0 {
            return Ok(body);
        }
        body.extend_from_slice(rest.get(..n).ok_or("a chunk shorter than its size")?);
        rest = rest.get(n..).and_then(|r| r.strip_prefix(b"\r\n")).ok_or("a chunk without its CRLF")?;
    }
}

/// The script both legs answer: GET, POST with a UTF-8 body and a header, a
/// header read that misses, a status outside the reason table, a handler
/// err (500), a 404 with a query, another method, and a body that is not
/// UTF-8 (decoded with replacement on both legs).
fn script() -> Vec<Vec<u8>> {
    vec![
        request("GET", "/hello", &[], b""),
        request("POST", "/echo", &[("X-Token", "abc")], "日本語 ok".as_bytes()),
        request("GET", "/echo", &[], b""),
        request("GET", "/teapot", &[], b""),
        request("GET", "/fail", &[], b""),
        request("GET", "/missing?x=1&y=%E7%8C%AB", &[], b""),
        request("DELETE", "/hello", &[], b""),
        request("PUT", "/echo", &[], b"a\xffb\xe3\x81c\xf0\x9f\x98\x80"),
    ]
}

#[test]
fn the_stock_serve_export_answers_what_native_answers_under_wasmtime_serve() {
    let dir = scratch("replay");
    // The emission half: the artifact is the handler export and imports no
    // filesystem (`wasmtime serve` links none without a flag).
    let wasm = build_export(&dir);
    let bytes = std::fs::read(&wasm).expect("read the artifact");
    let has = |s: &[u8]| bytes.windows(s.len()).any(|w| w == s);
    assert!(has(b"wasi:http/handler@0.3.0"), "the artifact exports no wasi:http/handler@0.3.0");
    assert!(!has(b"wasi:filesystem"), "the serve export imports wasi:filesystem");
    assert!(!has(b"wasi:cli/run"), "the serve export still carries wasi:cli/run");
    if !wasmtime_serves_p3() {
        return;
    }

    let (wport, mut served) = serve_artifact(&dir, &wasm, &[]);
    let (nport, mut native) = serve_native(&dir);

    let script = script();
    let answers: Vec<(Vec<u8>, Vec<u8>)> = script.iter().map(|r| (ask(nport, r), ask(wport, r))).collect();
    // The artifact alone: native's limits, concurrency, the trap.
    let big = vec![b'a'; (1 << 20) + 1];
    let over_body = ask(wport, &request("POST", "/echo", &[], &big));
    let at_limit = ask(wport, &request("POST", "/echo", &[], &big[..1 << 20]));
    let long = format!("/{}", "x".repeat(8200));
    let over_line = ask(wport, &request("GET", &long, &[], b""));
    let under_line = ask(wport, &request("GET", &long[..8100], &[], b""));
    let concurrent: Vec<Vec<u8>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..16).map(|_| s.spawn(|| ask(wport, &request("GET", "/hello", &[], b"")))).collect();
        hs.into_iter().map(|h| h.join().expect("a concurrent request")).collect()
    });
    let trapped = ask(wport, &request("GET", "/trap", &[], b""));
    let after_trap = ask(wport, &request("GET", "/hello", &[], b""));
    kill_group(&mut served);
    kill_group(&mut native);
    let log = std::fs::read_to_string(dir.join("wasmtime.log")).unwrap_or_default();

    // The script's own shape, so an empty answer on both legs cannot pass.
    let first = observe(&answers[0].0).expect("native answered /hello");
    assert_eq!((first.status, first.body.as_slice()), (200, b"hello from almide".as_slice()));
    for (i, (n, w)) in answers.iter().enumerate() {
        let (on, ow) = (observe(n), observe(w));
        assert!(
            on.is_ok() && on == ow,
            "request {i} answered differently:\nnative: {on:?}\nexport: {ow:?}\nnative raw: {:?}\nexport raw: {:?}\nwasmtime log:\n{log}",
            String::from_utf8_lossy(n),
            String::from_utf8_lossy(w)
        );
    }
    let refused = |raw: &[u8], status: u16, reason: &str| {
        let o = observe(raw).unwrap_or_else(|e| panic!("{e}: {:?}\n{log}", String::from_utf8_lossy(raw)));
        assert_eq!((o.status, o.body.as_slice()), (status, reason.as_bytes()), "{log}");
        assert_eq!(o.headers.get("content-type").map(Vec::as_slice), Some(["text/plain".to_string()].as_slice()));
    };
    refused(&over_body, 413, "Content Too Large");
    refused(&over_line, 414, "URI Too Long");
    let ok_status = |raw: &[u8]| observe(raw).map(|o| o.status).unwrap_or(0);
    assert_eq!(ok_status(&at_limit), 201, "a body of exactly 1 MiB is admitted");
    assert_eq!(ok_status(&under_line), 404, "a request line under 8 KiB reaches the handler");
    for c in &concurrent {
        let o = observe(c).unwrap_or_else(|e| panic!("{e}\n{log}"));
        assert_eq!((o.status, o.body.as_slice()), (200, b"hello from almide".as_slice()), "{log}");
    }
    assert_eq!(ok_status(&trapped), 500, "a trap answers the host's 500");
    assert_eq!(ok_status(&after_trap), 200, "the host serves on with a fresh instance");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The 431 limits (native's server core: more than 100 header fields, or a
/// header line over 8 KiB with its CRLF), each on both sides of the
/// boundary, against native under C-367's comparison. The fixture's
/// request adds `Host` and `Connection`, so 98 more fields is 100 and 99 is
/// 101; `X-Big: ` plus the value plus CRLF is the line.
///
/// One answer is the host's, not the export's: `wasmtime serve` refuses the
/// 101st field itself (hyper's own limit of 100) before the handler is
/// called, with an empty body and no `Content-Type`, where native answers
/// the reason as text/plain. The status and the boundary agree; that case
/// asserts both and the host's answer as it is, so a host that starts
/// passing such a request through is noticed here.
#[test]
fn the_431_limits_answer_at_natives_boundaries_under_wasmtime_serve() {
    let dir = scratch("h431");
    let wasm = build_export(&dir);
    if !wasmtime_serves_p3() {
        return;
    }
    let (wport, mut served) = serve_artifact(&dir, &wasm, &[]);
    let (nport, mut native) = serve_native(&dir);
    let fields = |n: usize| (0..n).map(|k| (format!("X-H{k}"), "v".to_string())).collect::<Vec<_>>();
    let big = |line: usize| vec![("X-Big".to_string(), "a".repeat(line - "X-Big: \r\n".len()))];
    let cases = [
        ("100 fields", fields(98), 200),
        ("101 fields", fields(99), 431),
        ("an 8192-byte header line", big(8192), 200),
        ("an 8193-byte header line", big(8193), 431),
    ];
    let answers: Vec<(Vec<u8>, Vec<u8>)> = cases
        .iter()
        .map(|(_, h, _)| {
            let h: Vec<(&str, &str)> = h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let raw = request("GET", "/hello", &h, b"");
            (ask(nport, &raw), ask(wport, &raw))
        })
        .collect();
    kill_group(&mut served);
    kill_group(&mut native);
    let log = std::fs::read_to_string(dir.join("wasmtime.log")).unwrap_or_default();
    for ((name, _, status), (n, w)) in cases.iter().zip(&answers) {
        let (on, ow) = (observe(n), observe(w));
        let on = on.unwrap_or_else(|e| panic!("{name}: native: {e}: {:?}", String::from_utf8_lossy(n)));
        assert_eq!(on.status, *status, "{name}: native answered {on:?}");
        if *name == "101 fields" {
            let ow = ow.unwrap_or_else(|e| panic!("{name}: export: {e}: {:?}\n{log}", String::from_utf8_lossy(w)));
            assert_eq!(on.body, b"Request Header Fields Too Large", "{name}: native's reason");
            assert_eq!((ow.status, ow.body.as_slice(), ow.headers.is_empty()), (431, b"".as_slice(), true), "{name}: the host's answer changed: {ow:?}\n{log}");
            continue;
        }
        assert!(
            Ok(&on) == ow.as_ref(),
            "{name} answered differently:\nnative: {on:?}\nexport: {ow:?}\nexport raw: {:?}\nwasmtime log:\n{log}",
            String::from_utf8_lossy(w)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// One reused instance answers many requests in constant heap (#3444): each
/// request runs in an arena the export resets once it is answered. The
/// artifact is served under a linear-memory ceiling a single request fits
/// in with room to spare (it needs 512 KiB; the ceiling is 2 MiB), and 200
/// POSTs of a 4 KiB body go to one instance (`--max-instance-reuse-count`,
/// when the host has it; wasmtime's default for a p3 component is 128). A
/// shim that keeps its per-request buffers (the header list, the 64 KiB body
/// buffer, the frames, the reply) and the guest's per-request blocks
/// outgrows the ceiling within about twenty requests: the guest's allocation
/// fails, the host answers 500 and starts a fresh instance, which fails the
/// same way later.
#[test]
fn a_reused_instance_answers_many_requests_in_constant_heap() {
    const REQUESTS: usize = 200;
    let dir = scratch("arena");
    let wasm = build_export(&dir);
    if !wasmtime_serves_p3() {
        return;
    }
    let mut flags = vec!["-W".to_string(), format!("max-memory-size={}", 2 << 20)];
    let help = Command::new("wasmtime").args(["serve", "--help"]).output().map(|o| o.stdout).unwrap_or_default();
    if String::from_utf8_lossy(&help).contains("--max-instance-reuse-count") {
        flags.extend(["--max-instance-reuse-count".to_string(), REQUESTS.to_string()]);
    }
    let (wport, mut served) = serve_artifact(&dir, &wasm, &flags);
    let body: Vec<u8> = (0..4096).map(|k| b'a' + (k % 26) as u8).collect();
    let mut want = b"POST ".to_vec();
    want.extend_from_slice(&body);
    let failed: Vec<(usize, Result<Observed, String>)> = (0..REQUESTS)
        .map(|k| (k, observe(&ask(wport, &request("POST", "/echo", &[], &body)))))
        .filter(|(_, o)| !matches!(o, Ok(o) if o.status == 201 && o.body == want))
        .collect();
    kill_group(&mut served);
    let log = std::fs::read_to_string(dir.join("wasmtime.log")).unwrap_or_default();
    assert!(
        failed.is_empty(),
        "{} of {REQUESTS} requests failed under a 2 MiB memory ceiling, the first: {:?}\nwasmtime log:\n{log}",
        failed.len(),
        failed.first().map(|(k, o)| (k, o.as_ref().map(|o| o.status)))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The shape rule: an http.serve program whose `main` does anything but
/// serve is refused at check time with E081 and the rule as the reason, and
/// a serve export that reaches the filesystem is E081 with the service
/// world's reason.
#[test]
fn a_server_that_is_not_serve_shaped_or_reads_files_is_refused_for_stock_wasm() {
    let dir = scratch("shape");
    let cases = [
        (
            "unshaped.almd",
            "import http\n\neffect fn app(req: HttpRequest) -> HttpResponse = http.response(200, \"x\")\n\neffect fn main() -> Unit = {\n  println(\"ready\")\n  http.serve(8080, app)!\n}\n",
            "main` is not run",
        ),
        (
            "files.almd",
            "import fs\nimport http\n\neffect fn app(req: HttpRequest) -> HttpResponse = http.response(200, fs.read_text(\"x\")!)\n\neffect fn main() -> Unit = http.serve(8080, app)!\n",
            "wasi:filesystem",
        ),
    ];
    for (name, src, reason) in cases {
        let path = dir.join(name);
        std::fs::write(&path, src).expect("write the program");
        let r = Command::new(almide_bin())
            .arg("build")
            .arg(&path)
            .args(["--target", "wasm", "-o"])
            .arg(dir.join("out.wasm"))
            .output()
            .expect("almide build runs");
        let err = String::from_utf8_lossy(&r.stderr);
        assert!(!r.status.success(), "{name} built for stock wasm:\n{err}");
        assert!(err.contains("error[E081]") && err.contains(reason), "{name}: expected E081 naming {reason:?}:\n{err}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
