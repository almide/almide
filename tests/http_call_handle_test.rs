//! #2631: the HTTP call handle — per-call `total_ms` / `idle_ms`, `cancel`,
//! a `poll` that never blocks, `read_new`, and the `_with_limits` streaming
//! twins — against a loopback server this test runs. The server records, per
//! connection, when it accepted and when its next write FAILED: that failure
//! is the server seeing the client close the connection, so every "the
//! connection is closed" claim below is measured on the server's side, not
//! inferred from the client's answer (C-366).
//!
//! The issue's numbers (a 40 s first byte, a 5 s deadline, a cancel after 3 s)
//! are scaled down through the limits: a 2 s first byte, 1 s deadlines, a
//! cancel after about 1 s. The server's paths:
//!   /trickle?<tag>          200, `text/event-stream`, no length, one
//!                           `data: <i>` event every 100 ms until the write fails
//!   /slow?<tag>             2 s of silence, then a complete 5-byte answer
//!   /missing?<tag>          a complete 404 with a repeated header
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

fn serve(mut sock: TcpStream, log: Arc<Mutex<HashMap<String, Conn>>>) {
    let accepted = Instant::now();
    let Some((_method, target, _body)) = read_head(&mut sock) else { return };
    log.lock().unwrap().insert(target.clone(), Conn { accepted, closed: None });
    let path = target.split('?').next().unwrap_or("").to_string();
    let trickle = |sock: &mut TcpStream, head: &str, event: &dyn Fn(usize) -> String| {
        if sock.write_all(head.as_bytes()).is_err() {
            return 0;
        }
        // At most 60 s, so a client that never closes cannot pin the thread.
        for i in 0..600 {
            if sock.write_all(event(i).as_bytes()).and_then(|_| sock.flush()).is_err() {
                return i;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        600
    };
    match path.as_str() {
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
            std::thread::sleep(Duration::from_secs(2));
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            0
        }
        "/missing" => {
            let _ = sock.write_all(
                b"HTTP/1.1 404 Not Found\r\nX-Kind: a\r\nX-Kind: b\r\nContent-Length: 4\r\n\r\ngone",
            );
            0
        }
        _ => 0,
    };
    let closed = matches!(path.as_str(), "/trickle" | "/v1/chat/completions").then(Instant::now);
    if let Some(c) = log.lock().unwrap().get_mut(&target) {
        c.closed = closed;
    }
}

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").into())
}

/// Build `src` into a native binary in `dir`; the binary's path.
fn build(dir: &std::path::Path, name: &str, src: &str) -> std::path::PathBuf {
    let source = dir.join(format!("{name}.almd"));
    let app = dir.join(name);
    std::fs::write(&source, src).unwrap();
    let built = Command::new(almide()).arg("build").arg(&source).arg("-o").arg(&app).output().unwrap();
    assert!(built.status.success(), "build {name}:\n{}", String::from_utf8_lossy(&built.stderr));
    app
}

fn run(app: &std::path::Path, env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(app);
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
  println("dropping: " + int.to_string(string.len(http.read_new(c))))
}

effect fn main() -> Unit = {
  let b = base()!
  // 1. total_ms is a wall clock: a stream that talks every 100 ms ends at it.
  let t1 = env.millis()
  let c1 = http.start("GET", b + "/trickle?total", "", [:], { total_ms: 1000, idle_ms: 0 })!
  let r1: Result[HttpResponse, String] = http.wait(c1)
  println("total: " + int.to_string(env.millis() - t1) + " " + show(r1))
  println("total-body: " + http.read_new(c1))

  // 2. poll never blocks; cancel after ~1 s closes the stream for good.
  let t2 = env.millis()
  let c2 = http.start("GET", b + "/trickle?cancel", "", [:], { total_ms: 0, idle_ms: 0 })!
  let (turns, got) = spin(c2, 0, "")!
  http.cancel(c2)
  println("cancel: " + int.to_string(env.millis() - t2) + " turns=" + int.to_string(turns) + " events=" + int.to_string(list.len(string.split(got, "\n\n")) - 1))
  println("cancel-first: " + (string.split(got, "\n\n") |> list.first ?? ""))
  println("after-cancel: [" + drain(c2, 5, "")! + "]")
  println("after-cancel-poll: " + (match http.poll(c2) { some(r) => show(r), none => "pending" }))
  http.cancel(c2)
  println("cancel-twice: " + (match http.poll(c2) { some(r) => show(r), none => "pending" }))

  // 3. per-call limits beat a slow first byte; the process-wide default does
  //    not apply to a call that carries limits.
  let t3 = env.millis()
  let r3: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?roomy", "", [:], { total_ms: 4000, idle_ms: 4000 })!)
  println("slow-roomy: " + int.to_string(env.millis() - t3) + " " + show(r3))
  let t4 = env.millis()
  let r4: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?total", "", [:], { total_ms: 1000, idle_ms: 0 })!)
  println("slow-total: " + int.to_string(env.millis() - t4) + " " + show(r4))
  let t5 = env.millis()
  let r5: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/slow?idle", "", [:], { total_ms: 0, idle_ms: 500 })!)
  println("slow-idle: " + int.to_string(env.millis() - t5) + " " + show(r5))
  let t6 = env.millis()
  let r6: Result[HttpResponse, String] = http.request_response("GET", b + "/slow?default", "", [:])
  println("slow-default: " + int.to_string(env.millis() - t6) + " " + show(r6))

  // 4. the whole response record, any status: the request_response answer.
  let r7: Result[HttpResponse, String] = http.wait(http.start("GET", b + "/missing?record", "", [:], { total_ms: 2000, idle_ms: 0 })!)
  println("record: " + show(r7) + " " + (match r7 { ok(resp) => string.join(http.header_values(resp, "x-kind"), ","), err(_) => "-" }))

  // 5. the streaming twins end at their limit, with the chunks delivered.
  var chunks: List[String] = []
  let t8 = env.millis()
  let r8: Result[Unit, String] = http.request_stream_with_limits("GET", b + "/trickle?stream", "", [:], { total_ms: 700, idle_ms: 0 }, (chunk: String) => list.push(chunks, chunk))
  println("stream: " + int.to_string(env.millis() - t8) + " " + (match r8 { ok(_) => "ok", err(e) => e }))
  println("stream-first: " + (string.split(string.join(chunks, ""), "\n\n") |> list.first ?? ""))
  var deltas: List[String] = []
  let t9 = env.millis()
  let r9: Result[String, String] = http.openai_streaming_call_with_limits(b + "/v1", "k", "{}", { total_ms: 700, idle_ms: 0 }, (d: String) => list.push(deltas, d))
  println("openai: " + int.to_string(env.millis() - t9) + " " + (match r9 { ok(_) => "ok", err(e) => e }))
  println("openai-first: " + (deltas |> list.first ?? ""))

  // 6. dropping the last copy of the handle cancels the call.
  start_and_drop(b + "/trickle?drop")!
  env.sleep_ms(1500)
  println("dropped: done")

  // 7. limits are validated.
  println("invalid: " + (match http.start("GET", b + "/missing?never", "", [:], { total_ms: -1, idle_ms: 0 }) { ok(_) => "ok", err(e) => e }))
}
"#;

#[test]
fn call_handle_limits_cancel_poll_and_the_streaming_twins() {
    let server = Server::start();
    let dir = tempfile::tempdir().unwrap();
    let app = build(dir.path(), "handle", HANDLE_PROGRAM);
    // ALMIDE_HTTP_TIMEOUT_SECS=1 scales the 30 s default of the calls that take
    // no limits down to 1 s — and the calls that DO take limits must ignore it.
    let port = server.port.to_string();
    let out = run(&app, &[("PORT", &port), ("ALMIDE_HTTP_TIMEOUT_SECS", "1")]);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    let l = lines(&out);
    let get = |k: &str| l.get(k).unwrap_or_else(|| panic!("no `{k}:` line in\n{stdout}")).clone();

    // 1. the wall clock fires at ~1 s and says which limit it was
    let (ms, rest) = timed(&get("total"));
    assert_eq!(rest, "request timeout: total_ms 1000 exceeded", "{stdout}");
    assert!((900..3000).contains(&ms), "total_ms 1000 fired after {ms} ms");
    assert!(get("total-body").starts_with("data: 0"), "bytes that arrived before the limit stay readable: {stdout}");
    let c = server.conn("/trickle?total");
    let closed = c.closed.expect("the server saw the close").duration_since(c.accepted);
    assert!(closed < Duration::from_millis(2500), "server saw the close {closed:?} after accept");

    // 2. poll did not block (the loop turned ~10 times while the stream ran),
    //    and after cancel nothing more arrived
    let cancel = get("cancel");
    let (ms, rest) = timed(&cancel);
    assert!((800..3000).contains(&ms), "{cancel}");
    assert!(rest.starts_with("turns=10 "), "the poll loop turned every 100 ms: {cancel}");
    let events: usize = rest.rsplit('=').next().unwrap().parse().unwrap();
    assert!(events >= 3, "read_new delivered the stream while it ran: {cancel}");
    assert_eq!(get("cancel-first"), "data: 0");
    assert_eq!(get("after-cancel"), "[]", "nothing arrives after cancel");
    assert_eq!(get("after-cancel-poll"), "request cancelled");
    assert_eq!(get("cancel-twice"), "request cancelled", "cancel is idempotent");
    let c = server.conn("/trickle?cancel");
    let closed = c.closed.expect("the server saw the close").duration_since(c.accepted);
    assert!(
        (Duration::from_millis(800)..Duration::from_millis(3500)).contains(&closed),
        "the server saw the close {closed:?} after accept (cancel ran at ~1 s)"
    );

    // 3. the slow first byte: roomy limits succeed although the process-wide
    //    default (1 s here) would have failed; a tight total or idle fires,
    //    naming its limit; the unlimited call keeps the process-wide default
    let (ms, rest) = timed(&get("slow-roomy"));
    assert_eq!(rest, "200 hello");
    assert!(ms >= 1900, "{ms}");
    let (ms, rest) = timed(&get("slow-total"));
    assert_eq!(rest, "request timeout: total_ms 1000 exceeded");
    assert!((900..1900).contains(&ms), "{ms}");
    let (ms, rest) = timed(&get("slow-idle"));
    assert_eq!(rest, "request timeout: idle_ms 500 exceeded");
    assert!((400..1900).contains(&ms), "{ms}");
    let (_, rest) = timed(&get("slow-default"));
    assert!(rest.starts_with("read timed out waiting for the server"), "{rest}");

    // 4. wait answers the full record; a 404 is ok
    assert_eq!(get("record"), "404 gone a,b");

    // 5. the streaming twins
    let (ms, rest) = timed(&get("stream"));
    assert_eq!(rest, "request timeout: total_ms 700 exceeded");
    assert!((600..2500).contains(&ms), "{ms}");
    assert_eq!(get("stream-first"), "data: 0");
    assert!(server.conn("/trickle?stream").closed.is_some());
    let (ms, rest) = timed(&get("openai"));
    assert_eq!(rest, "request timeout: total_ms 700 exceeded");
    assert!((600..2500).contains(&ms), "{ms}");
    assert_eq!(get("openai-first"), "t0 ");

    // 6. drop = cancel: the server saw the close long before the program's
    //    1.5 s sleep after the drop ended
    assert_eq!(get("dropped"), "done");
    let c = server.conn("/trickle?drop");
    let closed = c.closed.expect("the server saw the close").duration_since(c.accepted);
    assert!(closed < Duration::from_millis(1400), "dropping the handle closed the connection only after {closed:?}");

    // 7.
    assert!(get("invalid").starts_with("invalid limits: "), "{}", get("invalid"));
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
    let app = build(dir.path(), "old_stream", OLD_STREAM_PROGRAM);
    let port = server.port.to_string();

    let mut child = Command::new(&app)
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
