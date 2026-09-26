//! #2631 / #2633: the HTTP call handle — per-call `total_ms` / `idle_ms`,
//! `cancel`, a `poll` that never blocks, `read_new`, and the `_with_limits`
//! streaming twins — against a loopback server this test runs, on BOTH legs:
//! the native binary and `almide run --target wasm` (the embedded host, which
//! serves the handle with the same call core, crates/almide-rt-core). The
//! server records, per connection, when it accepted and when it saw the
//! client close the connection (a write that fails, or a read that answers
//! end-of-stream while it waits), so every "the connection is closed" and
//! every "the limit fired at ~N ms" claim below is measured on the server's
//! side, the same way for both legs (C-366). The handle program prints no
//! timings, so its stdout must be byte-identical across the legs.
//!
//! The issue's numbers (a 40 s first byte, a 5 s deadline, a cancel after 3 s)
//! are scaled down through the limits: a 2 s first byte, 1 s deadlines, a
//! cancel after about 1 s. The server's paths:
//!   /trickle?<tag>          200, `text/event-stream`, no length, one
//!                           `data: <i>` event every 100 ms until the write fails
//!   /slow?<tag>             2 s of silence (watching for the close), then a
//!                           complete 5-byte answer
//!   /missing?<tag>          a complete 404 with a repeated header
//!   /utf8?<tag>             `caf` + the first byte of `é`, 400 ms later the
//!                           rest and `!`, then the close (no length)
//!   /v1/chat/completions    an OpenAI-shaped SSE trickle (POST)

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Conn {
    accepted: Instant,
    closed: Option<Instant>,
}

struct Server {
    port: u16,
    conns: Arc<Mutex<HashMap<String, Conn>>>,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let conns: Arc<Mutex<HashMap<String, Conn>>> = Arc::default();
        let log = conns.clone();
        std::thread::spawn(move || {
            for sock in listener.incoming() {
                let Ok(sock) = sock else { continue };
                let log = log.clone();
                std::thread::spawn(move || serve(sock, log));
            }
        });
        Server { port, conns }
    }

    /// The record of the connection whose request target was `target`.
    fn conn(&self, target: &str) -> Conn {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(c) = self.conns.lock().unwrap().get(target) {
                if c.closed.is_some() || Instant::now() >= deadline {
                    return c.clone();
                }
            } else if Instant::now() >= deadline {
                panic!("the server never saw a request for {target}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// How long after accepting `target` the server saw the client close it.
    fn closed_after(&self, target: &str) -> Duration {
        let c = self.conn(target);
        c.closed.unwrap_or_else(|| panic!("the server never saw {target} closed")).duration_since(c.accepted)
    }
}

fn read_head(sock: &mut TcpStream) -> Option<(String, String, Vec<u8>)> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    let end = loop {
        let n = sock.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
    };
    let head = String::from_utf8_lossy(&raw[..end]).into_owned();
    let mut first = head.lines().next().unwrap_or("").split_whitespace();
    let method = first.next().unwrap_or("").to_string();
    let target = first.next().unwrap_or("").to_string();
    let len: usize = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = raw[end + 4..].to_vec();
    while body.len() < len {
        let n = sock.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    Some((method, target, body))
}

/// Wait up to `total` for the client to close the connection: `true` (and
/// at once) when a read answers end-of-stream or a reset.
fn watch_close(sock: &mut TcpStream, total: Duration) -> bool {
    let until = Instant::now() + total;
    let mut buf = [0u8; 64];
    sock.set_read_timeout(Some(Duration::from_millis(20))).ok();
    while Instant::now() < until {
        match sock.read(&mut buf) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return true,
        }
    }
    false
}

fn serve(mut sock: TcpStream, log: Arc<Mutex<HashMap<String, Conn>>>) {
    let accepted = Instant::now();
    let Some((_method, target, _body)) = read_head(&mut sock) else { return };
    log.lock().unwrap().insert(target.clone(), Conn { accepted, closed: None });
    let path = target.split('?').next().unwrap_or("").to_string();
    let trickle = |sock: &mut TcpStream, head: &str, event: &dyn Fn(usize) -> String| -> bool {
        if sock.write_all(head.as_bytes()).is_err() {
            return true;
        }
        // At most 60 s, so a client that never closes cannot pin the thread.
        for i in 0..600 {
            if sock.write_all(event(i).as_bytes()).and_then(|_| sock.flush()).is_err() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    };
    let closed = match path.as_str() {
        "/trickle" => trickle(
            &mut sock,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n",
            &|i| format!("data: {i}\n\n"),
        ),
        "/v1/chat/completions" => trickle(
            &mut sock,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n",
            &|i| format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"t{i} \"}}}}]}}\n\n"),
        ),
        "/slow" => {
            if watch_close(&mut sock, Duration::from_secs(2)) {
                true
            } else {
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
                false
            }
        }
        "/missing" => {
            let _ = sock.write_all(
                b"HTTP/1.1 404 Not Found\r\nX-Kind: a\r\nX-Kind: b\r\nContent-Length: 4\r\n\r\ngone",
            );
            false
        }
        "/utf8" => {
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\n\r\ncaf\xC3").and_then(|_| sock.flush());
            std::thread::sleep(Duration::from_millis(400));
            let _ = sock.write_all(b"\xA9!");
            false
        }
        _ => false,
    };
    if closed && let Some(c) = log.lock().unwrap().get_mut(&target) {
        c.closed = Some(Instant::now());
    }
}

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Leg {
    Native,
    Wasm,
}

/// A program ready to run on one leg: the native binary, or the source the
/// embedded host runs through `almide run --target wasm`.
struct Prepared {
    leg: Leg,
    path: std::path::PathBuf,
}

fn prepare(dir: &std::path::Path, name: &str, src: &str, leg: Leg) -> Prepared {
    let source = dir.join(format!("{name}.almd"));
    std::fs::write(&source, src).unwrap();
    match leg {
        Leg::Native => {
            let app = dir.join(name);
            let built = Command::new(almide()).arg("build").arg(&source).arg("-o").arg(&app).output().unwrap();
            assert!(built.status.success(), "build {name}:\n{}", String::from_utf8_lossy(&built.stderr));
            Prepared { leg, path: app }
        }
        Leg::Wasm => Prepared { leg, path: source },
    }
}

fn command(p: &Prepared) -> Command {
    match p.leg {
        Leg::Native => Command::new(&p.path),
        Leg::Wasm => {
            let mut c = Command::new(almide());
            c.arg("run").arg(&p.path).arg("--target").arg("wasm");
            c
        }
    }
}

fn run(p: &Prepared, env: &[(&str, &str)]) -> Output {
    let mut cmd = command(p);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

/// `label: rest` lines of the program's stdout.
fn lines(out: &Output) -> HashMap<String, String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once(": ").map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// `<ms> <text>`: the elapsed milliseconds and the rest.
fn timed(v: &str) -> (u64, String) {
    let (ms, rest) = v.split_once(' ').unwrap_or((v, ""));
    (ms.parse().unwrap_or_else(|_| panic!("not a timed line: {v}")), rest.to_string())
}

const HANDLE_PROGRAM: &str = r#"
import http
import env

effect fn base() -> String = "http://127.0.0.1:" + (env.get("PORT") ?? "0")

fn show(r: Result[HttpResponse, String]) -> String = match r {
  ok(resp) => int.to_string(http.status_code(resp)) + " " + http.body(resp),
  err(e) => e,
}

fn polled(p: Result[HttpResponse, String]?) -> String = match p {
  some(r) => show(r),
  none => "pending",
}

fn first_event(s: String) -> String = string.split(s, "\n\n") |> list.first ?? ""

effect fn drain(c: HttpCall, rounds: Int, acc: String) -> String = {
  let acc2 = acc + http.read_new(c)
  if rounds == 0 then acc2
  else {
    env.sleep_ms(100)
    drain(c, rounds - 1, acc2)!
  }
}

// Poll without blocking, 100 ms apart, counting the turns the loop got.
effect fn spin(c: HttpCall, turns: Int, got: String) -> (Int, String) = match http.poll(c) {
  some(_) => (turns, got),
  none => if turns >= 10 then (turns, got)
  else {
    let got2 = got + http.read_new(c)
    env.sleep_ms(100)
    spin(c, turns + 1, got2)!
  },
}

effect fn start_and_drop(url: String) -> Unit = {
  let c = http.start("GET", url, "", [:], { total_ms: 0, idle_ms: 0 })!
  env.sleep_ms(300)
  println("dropping: " + (if string.len(http.read_new(c)) > 0 then "read" else "nothing"))
}

effect fn main() -> Unit = {
  let b = base()!
  // 1. total_ms is a wall clock: a stream that talks every 100 ms ends at it.
  let c1 = http.start("GET", b + "/trickle?total", "", [:], { total_ms: 1000, idle_ms: 0 })!
  let r1: Result[HttpResponse, String] = http.wait(c1)
  println("total: " + show(r1))
  println("total-body: " + first_event(http.read_new(c1)))

  // 2. poll never blocks; cancel after ~1 s closes the stream for good.
  let c2 = http.start("GET", b + "/trickle?cancel", "", [:], { total_ms: 0, idle_ms: 0 })!
  println("cancel-before: " + polled(http.poll(c2)))
  let (turns, got) = spin(c2, 0, "")!
  http.cancel(c2)
  println("cancel: turns=" + int.to_string(turns) + " delivered=" + (if list.len(string.split(got, "\n\n")) > 3 then "several" else "few"))
  println("cancel-first: " + first_event(got))
  println("after-cancel: [" + drain(c2, 5, "")! + "]")
  println("after-cancel-poll: " + polled(http.poll(c2)))
  http.cancel(c2)
  println("cancel-twice: " + polled(http.poll(c2)))

  // 3. per-call limits beat a slow first byte; the process-wide default
  //    (ALMIDE_HTTP_TIMEOUT_SECS=1 here) does not apply to a call with limits.
  let r3: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?roomy", "", [:], { total_ms: 4000, idle_ms: 4000 })!)
  println("slow-roomy: " + show(r3))
  let r4: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?total", "", [:], { total_ms: 1000, idle_ms: 0 })!)
  println("slow-total: " + show(r4))
  let r5: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?idle", "", [:], { total_ms: 0, idle_ms: 500 })!)
  println("slow-idle: " + show(r5))

  // 4. the whole response record, any status: the request_response answer.
  let r7: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/missing?record", "", [:], { total_ms: 2000, idle_ms: 0 })!)
  println("record: " + show(r7) + " " + (match r7 { ok(resp) => string.join(http.header_values(resp, "x-kind"), ","), err(_) => "-" }))

  // 5. read_new holds a split character back until it is whole.
  let c8 = http.start("GET", b + "/utf8?split", "", [:], { total_ms: 5000, idle_ms: 0 })!
  env.sleep_ms(200)
  let early = http.read_new(c8)
  let r8: Result[HttpResponse, String] = http.wait(c8)
  println("utf8: " + early + "|" + http.read_new(c8) + "|" + show(r8))

  // 6. the streaming twin ends at its limit, with the chunks delivered; a
  //    non-2xx is refused the request_stream way; a complete body is ok.
  var chunks: List[String] = []
  let r9: Result[Unit, String] = http.request_stream_with_limits("GET", b + "/trickle?stream", "", [:], { total_ms: 700, idle_ms: 0 }, (chunk: String) => list.push(chunks, chunk))
  println("stream: " + (match r9 { ok(_) => "ok", err(e) => e }))
  println("stream-first: " + first_event(string.join(chunks, "")))
  let r10: Result[Unit, String] = http.request_stream_with_limits("GET", b + "/missing?stream", "", [:], { total_ms: 2000, idle_ms: 0 }, (_) => ())
  println("stream-refused: " + (match r10 { ok(_) => "ok", err(e) => e }))
  var whole: List[String] = []
  let r11: Result[Unit, String] = http.request_stream_with_limits("GET", b + "/slow?stream", "", [:], { total_ms: 4000, idle_ms: 0 }, (chunk: String) => list.push(whole, chunk))
  println("stream-whole: " + (match r11 { ok(_) => "ok", err(e) => e }) + " " + string.join(whole, ""))

  // 7. dropping the last copy of the handle cancels the call.
  start_and_drop(b + "/trickle?drop")!
  env.sleep_ms(1500)
  println("dropped: done")

  // 8. limits are validated; a transport failure is the call's err.
  println("invalid: " + (match http.start("GET", b + "/missing?never", "", [:], { total_ms: -1, idle_ms: 0 }) { ok(_) => "ok", err(e) => e }))
  println("refused: " + show(http.wait(http.start("GET", "http://127.0.0.1:1/", "", [:], { total_ms: 0, idle_ms: 0 })!)))
}
"#;

/// The server-side and textual assertions every leg must meet.
fn check_handle_leg(leg: Leg, out: &Output, server: &Server) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "[{leg:?}] {stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let l = lines(out);
    let get = |k: &str| l.get(k).unwrap_or_else(|| panic!("[{leg:?}] no `{k}:` line in\n{stdout}")).clone();
    let within = |target: &str, lo: u64, hi: u64| {
        let ms = server.closed_after(target).as_millis() as u64;
        assert!((lo..hi).contains(&ms), "[{leg:?}] the server saw {target} closed {ms} ms after accept, want [{lo}, {hi})");
    };

    // 1. the wall clock fires at ~1 s, says which limit it was, and closes
    assert_eq!(get("total"), "request timeout: total_ms 1000 exceeded", "[{leg:?}]");
    assert_eq!(get("total-body"), "data: 0", "[{leg:?}] bytes that arrived before the limit stay readable");
    within("/trickle?total", 900, 2500);

    // 2. poll did not block (the loop turned every 100 ms while the stream
    //    ran), and after cancel nothing more arrived
    assert_eq!(get("cancel-before"), "pending", "[{leg:?}]");
    assert_eq!(get("cancel"), "turns=10 delivered=several", "[{leg:?}] the poll loop turned every 100 ms");
    assert_eq!(get("cancel-first"), "data: 0", "[{leg:?}]");
    assert_eq!(get("after-cancel"), "[]", "[{leg:?}] nothing arrives after cancel");
    assert_eq!(get("after-cancel-poll"), "request cancelled", "[{leg:?}]");
    assert_eq!(get("cancel-twice"), "request cancelled", "[{leg:?}] cancel is idempotent");
    within("/trickle?cancel", 800, 3500);

    // 3. the slow first byte: roomy limits succeed although the process-wide
    //    default (1 s here) would have failed; a tight total or idle fires,
    //    naming its limit, and closes the connection at it
    assert_eq!(get("slow-roomy"), "200 hello", "[{leg:?}]");
    assert!(server.conn("/slow?roomy").closed.is_none(), "[{leg:?}] the roomy call waited for its answer");
    assert_eq!(get("slow-total"), "request timeout: total_ms 1000 exceeded", "[{leg:?}]");
    within("/slow?total", 900, 1900);
    assert_eq!(get("slow-idle"), "request timeout: idle_ms 500 exceeded", "[{leg:?}]");
    within("/slow?idle", 400, 1900);

    // 4. wait answers the full record; a 404 is ok
    assert_eq!(get("record"), "404 gone a,b", "[{leg:?}]");

    // 5. a character split across arrivals is held back until whole
    assert_eq!(get("utf8"), "caf|é!|200 café!", "[{leg:?}]");

    // 6. the streaming twin
    assert_eq!(get("stream"), "request timeout: total_ms 700 exceeded", "[{leg:?}]");
    assert_eq!(get("stream-first"), "data: 0", "[{leg:?}]");
    within("/trickle?stream", 600, 2500);
    assert_eq!(get("stream-refused"), "HTTP 404: Not Found: gone", "[{leg:?}]");
    assert_eq!(get("stream-whole"), "ok hello", "[{leg:?}]");

    // 7. drop = cancel: the server saw the close long before the program's
    //    1.5 s sleep after the drop ended
    assert_eq!(get("dropping"), "read", "[{leg:?}]");
    assert_eq!(get("dropped"), "done", "[{leg:?}]");
    within("/trickle?drop", 0, 1400);

    // 8.
    assert!(get("invalid").starts_with("invalid limits: "), "[{leg:?}] {}", get("invalid"));
    assert!(get("refused").starts_with("connection failed: "), "[{leg:?}] {}", get("refused"));
    stdout
}

#[test]
fn call_handle_limits_cancel_poll_and_the_streaming_twins() {
    let dir = tempfile::tempdir().unwrap();
    let mut seen: Vec<(Leg, String)> = Vec::new();
    for leg in [Leg::Native, Leg::Wasm] {
        // A fresh server per leg: its connection log is keyed by target.
        let server = Server::start();
        let app = prepare(dir.path(), &format!("handle_{leg:?}"), HANDLE_PROGRAM, leg);
        // ALMIDE_HTTP_TIMEOUT_SECS=1 scales the 30 s default of the calls that
        // take no limits down to 1 s — the calls that DO take limits ignore it.
        let port = server.port.to_string();
        let out = run(&app, &[("PORT", &port), ("ALMIDE_HTTP_TIMEOUT_SECS", "1")]);
        seen.push((leg, check_handle_leg(leg, &out, &server)));
    }
    // The program prints no timings: the two legs answer byte-identically.
    assert_eq!(seen[0].1, seen[1].1, "native vs --target wasm stdout differ");
}

/// What only the native leg serves: the SSE twin (its accumulator has no wasm
/// body — `openai_streaming_call` itself is native-only) and the one-shot
/// full-response client that shows the process-wide default still applies to
/// a call WITHOUT limits.
const NATIVE_ONLY_PROGRAM: &str = r#"
import http
import env

effect fn main() -> Unit = {
  let b = "http://127.0.0.1:" + (env.get("PORT") ?? "0")
  let t6 = env.millis()
  let r6: Result[HttpResponse, String] = http.request_response("GET", b + "/slow?default", "", [:])
  println("slow-default: " + int.to_string(env.millis() - t6) + " " + (match r6 { ok(_) => "ok", err(e) => e }))
  var deltas: List[String] = []
  let t9 = env.millis()
  let r9: Result[String, String] = http.openai_streaming_call_with_limits(b + "/v1", "k", "{}", { total_ms: 700, idle_ms: 0 }, (d: String) => list.push(deltas, d))
  println("openai: " + int.to_string(env.millis() - t9) + " " + (match r9 { ok(_) => "ok", err(e) => e }))
  println("openai-first: " + (deltas |> list.first ?? ""))
}
"#;

#[test]
fn native_only_default_timeout_and_the_sse_twin() {
    let server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let app = prepare(dir.path(), "native_only", NATIVE_ONLY_PROGRAM, Leg::Native);
    let port = server.port.to_string();
    let out = run(&app, &[("PORT", &port), ("ALMIDE_HTTP_TIMEOUT_SECS", "1")]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let l = lines(&out);
    let get = |k: &str| l.get(k).unwrap_or_else(|| panic!("no `{k}:` line in\n{stdout}")).clone();
    let (_, rest) = timed(&get("slow-default"));
    assert!(rest.starts_with("read timed out waiting for the server"), "{rest}");
    let (ms, rest) = timed(&get("openai"));
    assert_eq!(rest, "request timeout: total_ms 700 exceeded");
    assert!((600..2500).contains(&ms), "{ms}");
    assert_eq!(get("openai-first"), "t0 ");
    assert!(server.conn("/v1/chat/completions").closed.is_some());
}

const OLD_STREAM_PROGRAM: &str = r#"
import http
import env

effect fn classic(url: String) -> String = match http.request_stream("GET", url, "", [:], (_) => ()) {
  ok(_) => "ok",
  err(e) => e,
}

effect fn limited(url: String) -> String = match http.wait(http.start("GET", url, "", [:], { total_ms: 1000, idle_ms: 0 })!) {
  ok(_) => "ok",
  err(e) => e,
}

effect fn main() -> Unit = {
  let url = "http://127.0.0.1:" + (env.get("PORT") ?? "0") + "/trickle?" + (env.get("TAG") ?? "")
  println(if env.get("LIMITED") == some("1") then limited(url)! else classic(url)!)
}
"#;

/// A/B: the classic `request_stream` never returns on a stream that talks
/// every 100 ms (its only bound is the between-bytes read timeout), while
/// `start` → `wait` with `total_ms` returns at the limit.
#[test]
fn a_trickle_outlives_the_classic_stream_but_not_total_ms() {
    let server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let app = prepare(dir.path(), "old_stream", OLD_STREAM_PROGRAM, Leg::Native);
    let port = server.port.to_string();

    let mut child = command(&app)
        .env("PORT", &port)
        .env("TAG", "classic")
        .env("ALMIDE_HTTP_TIMEOUT_SECS", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let still_running = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert!(still_running, "request_stream returned on a live trickle — the A leg no longer shows the hang");

    let started = Instant::now();
    let out = run(&app, &[("PORT", &port), ("TAG", "limited"), ("LIMITED", "1"), ("ALMIDE_HTTP_TIMEOUT_SECS", "1")]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "request timeout: total_ms 1000 exceeded");
    assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    assert!(server.conn("/trickle?limited").closed.is_some());
}

/// The E081 verdict names what is and is not served: the stock artifact
/// (`almide check --target wasm`, the build route) still refuses the handle
/// — stock WASI has no host for it — while the run route admits it, and the
/// SSE twin stays refused on every wasm leg.
#[test]
fn the_stock_route_and_the_sse_twin_keep_their_e081() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("stock.almd");
    std::fs::write(
        &src,
        "import http\n\neffect fn main() -> Unit = {\n  let c = http.start(\"GET\", \"http://127.0.0.1:1/\", \"\", [:], { total_ms: 0, idle_ms: 0 })!\n  http.cancel(c)\n}\n",
    )
    .unwrap();
    let checked = Command::new(almide()).arg("check").arg(&src).arg("--target").arg("wasm").output().unwrap();
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(!checked.status.success() && stderr.contains("error[E081]: `http.start`"), "{stderr}");
    let sse = dir.path().join("sse.almd");
    std::fs::write(
        &sse,
        "import http\n\neffect fn main() -> Unit = {\n  let r = http.openai_streaming_call_with_limits(\"http://127.0.0.1:1\", \"k\", \"{}\", { total_ms: 1, idle_ms: 0 }, (_) => ())\n  println(match r { ok(_) => \"ok\", err(e) => e })\n}\n",
    )
    .unwrap();
    let ran = Command::new(almide()).arg("run").arg(&sse).arg("--target").arg("wasm").output().unwrap();
    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(!ran.status.success() && stderr.contains("error[E081]: `http.openai_streaming_call_with_limits`"), "{stderr}");
}
