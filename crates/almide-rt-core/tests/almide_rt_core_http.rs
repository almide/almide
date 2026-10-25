//! The shared http cores (#2631 call handle, #2650 server) driven through
//! their public surface against loopback peers, so the code both lanes run
//! is exercised by `cargo test` and measured by proofs/coverage.sh — the
//! embedded-lane fixtures that drive it end to end run release-only and
//! against the network, which the coverage job does not do.
//!
//! Every peer here is a thread on 127.0.0.1 with an ephemeral port; nothing
//! leaves the host. Timing-sensitive cases use limits far below the peer's
//! hold time, so the verdict never races the peer.

use almide_rt_core::http_client_core as client;
use almide_rt_core::http_server_core as server;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// ── peers ──

/// The ephemeral port a listener was bound to.
fn port_of(l: &TcpListener) -> u16 {
    l.local_addr().expect("a bound listener has an address").port()
}

/// Join a peer thread; a peer that panicked fails the test.
fn join<T>(h: thread::JoinHandle<T>) -> T {
    h.join().expect("the peer thread finished")
}

/// The error a call ends with; an answered call fails the test.
fn wait_err(sh: &client::AlmideHttpCallShared) -> String {
    client::http_call_wait(sh).expect_err("the call fails")
}

/// The server core's response bytes as text (they are ASCII + UTF-8 bodies).
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("the response bytes are UTF-8")
}

/// A one-connection peer: accept, read the request head, then run `reply`.
fn peer(reply: impl FnOnce(TcpStream) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://127.0.0.1:{}/path?q=1", port_of(&l));
    let h = thread::spawn(move || {
        let (s, _) = l.accept().expect("accept");
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }
        reply(s);
    });
    (url, h)
}

/// A peer that answers `bytes` in the given pieces, pausing between them so
/// the client sees them as separate reads, then closes.
fn pieces_peer(pieces: Vec<Vec<u8>>) -> (String, thread::JoinHandle<()>) {
    peer(move |mut s| {
        for p in pieces {
            let _ = s.write_all(&p);
            let _ = s.flush();
            thread::sleep(Duration::from_millis(30));
        }
    })
}

/// A peer that holds the connection open without answering until `release`
/// fires (or the client goes away).
fn silent_peer() -> (String, mpsc::Sender<()>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<()>();
    let (url, h) = peer(move |_s| {
        let _ = rx.recv_timeout(Duration::from_secs(10));
    });
    (url, tx, h)
}

fn start(url: &str, total_ms: i64, idle_ms: i64) -> std::sync::Arc<client::AlmideHttpCallShared> {
    let headers = vec![("X-Test".to_string(), "1".to_string())];
    client::http_call_spawn("GET", url, "", headers, total_ms, idle_ms).expect("call starts")
}

/// Drain a call through the streaming step until it ends.
fn stream_all(sh: &client::AlmideHttpCallShared) -> (String, Result<(), String>) {
    let mut text = String::new();
    loop {
        let (piece, ended) = client::http_call_stream_step(sh);
        text.push_str(&piece);
        if let Some(o) = ended {
            return (text, o);
        }
    }
}

// ── the incomplete-UTF-8 tail ──

#[test]
fn utf8_tail_holds_back_only_an_unfinished_sequence() {
    assert_eq!(client::http_stream_incomplete_utf8_tail(b""), 0);
    assert_eq!(client::http_stream_incomplete_utf8_tail(b"abc"), 0);
    let e_acute = "é".as_bytes(); // 2 bytes
    let euro = "€".as_bytes(); // 3 bytes
    let clef = "𝄞".as_bytes(); // 4 bytes
    assert_eq!(client::http_stream_incomplete_utf8_tail(e_acute), 0);
    assert_eq!(client::http_stream_incomplete_utf8_tail(&e_acute[..1]), 1);
    assert_eq!(client::http_stream_incomplete_utf8_tail(euro), 0);
    assert_eq!(client::http_stream_incomplete_utf8_tail(&euro[..2]), 2);
    assert_eq!(client::http_stream_incomplete_utf8_tail(clef), 0);
    assert_eq!(client::http_stream_incomplete_utf8_tail(&clef[..3]), 3);
    assert_eq!(client::http_stream_incomplete_utf8_tail(&clef[..1]), 1);
    // Four continuation bytes and no lead in reach: nothing to hold back.
    assert_eq!(client::http_stream_incomplete_utf8_tail(&[0x80, 0x80, 0x80, 0x80]), 0);
    // A stray byte that is no lead (0xF8..) is complete on its own.
    assert_eq!(client::http_stream_incomplete_utf8_tail(&[b'a', 0xF8]), 0);
}

// ── start: the synchronous half ──

#[test]
fn start_refuses_negative_limits_before_any_thread() {
    let e = client::http_call_spawn("GET", "http://127.0.0.1:1/", "", vec![], -1, 0).err().expect("start refuses");
    assert_eq!(e, "invalid limits: total_ms and idle_ms must be >= 0 (0 = no limit), got total_ms -1 idle_ms 0");
    let e = client::http_call_spawn("GET", "http://127.0.0.1:1/", "", vec![], 0, -5).err().expect("start refuses");
    assert!(e.contains("got total_ms 0 idle_ms -5"), "{e}");
}

#[test]
fn a_refused_connection_is_a_connection_error() {
    let port = port_of(&TcpListener::bind("127.0.0.1:0").expect("bind loopback"));
    let want = format!("could not connect to \"http://127.0.0.1:{port}/\" (connection refused or unreachable)");
    let sh = start(&format!("http://127.0.0.1:{port}/"), 0, 0);
    let e = wait_err(&sh);
    assert_eq!(e, want);
    // The same with a wall clock set: the dial is bounded by it.
    let sh = start(&format!("http://127.0.0.1:{port}/"), 5_000, 0);
    let e = wait_err(&sh);
    assert_eq!(e, want);
}

// ── framings ──

#[test]
fn a_content_length_response_is_answered_whole() {
    let (url, h) = pieces_peer(vec![
        b"HTTP/1.1 201 Created\r\nX-A: 1\r\nX-A: 2\r\nbad header line\r\nContent-Length: 11\r\n\r\nhello".to_vec(),
        b" world".to_vec(),
    ]);
    let sh = start(&url, 0, 0);
    let (status, headers, body) = client::http_call_wait(&sh).expect("answered");
    assert_eq!(status, 201);
    assert_eq!(body, "hello world");
    let a: Vec<&str> = headers.iter().filter(|(k, _)| k == "X-A").map(|(_, v)| v.as_str()).collect();
    assert_eq!(a, ["1", "2"], "wire order, repeats kept");
    // An ended call polls as its result, and read_new hands the body out once.
    assert!(matches!(client::http_call_poll(&sh), Some(Ok((201, _, _)))));
    assert_eq!(client::http_call_read_new(&sh), "hello world");
    assert_eq!(client::http_call_read_new(&sh), "");
    // Cancelling an ended call leaves it as it is.
    sh.cancel();
    assert!(client::http_call_wait(&sh).is_ok());
    join(h);
}

#[test]
fn a_chunked_response_decodes_across_split_reads() {
    let (url, h) = pieces_peer(vec![
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: Chunked\r\n\r\n5;ext=1\r\nhel".to_vec(),
        b"lo\r".to_vec(),
        b"\n6\r\n world\r\n".to_vec(),
        b"0\r\n\r\n".to_vec(),
    ]);
    let sh = start(&url, 0, 0);
    let (status, _, body) = client::http_call_wait(&sh).expect("answered");
    assert_eq!((status, body.as_str()), (200, "hello world"));
    join(h);
}

#[test]
fn a_chunk_size_line_split_mid_line_waits_for_the_rest() {
    let (url, h) = pieces_peer(vec![
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".to_vec(),
        b"3".to_vec(),
        b"\r\nabc\r\n0\r\n\r\n".to_vec(),
    ]);
    let sh = start(&url, 0, 0);
    assert_eq!(client::http_call_wait(&sh).expect("answered").2, "abc");
    join(h);
}

/// C-408 (#4137): the buffered client refuses a response that ended before
/// its framing did, with the call-handle client's text, and both shapes agree
/// on every case; a whole response in the same shapes is still answered.
#[test]
fn the_buffered_client_refuses_an_incomplete_response_as_the_call_client_does() {
    let cases: [&[u8]; 5] = [
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhel",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n",
        b"just some text with no head",
    ];
    for raw in cases {
        let (url, h) = pieces_peer(vec![raw.to_vec()]);
        let got = client::request("GET", &url, "", &[]);
        join(h);
        assert_eq!(got, Err(format!("malformed or incomplete response from \"{url}\"")), "{:?}", String::from_utf8_lossy(raw));
        let (url, h) = pieces_peer(vec![raw.to_vec()]);
        let sh = start(&url, 0, 0);
        assert!(client::http_call_wait(&sh).is_err(), "the call client answered {:?}", String::from_utf8_lossy(raw));
        join(h);
    }
    for (raw, want) in [
        (&b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabcdef"[..], "abc"),
        (&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"[..], "hello"),
        (&b"HTTP/1.1 200 OK\r\n\r\nuntil close"[..], "until close"),
        (&b"HTTP/1.1 204 No Content\r\nContent-Length: 9\r\n\r\n"[..], ""),
    ] {
        let (url, h) = pieces_peer(vec![raw.to_vec()]);
        assert_eq!(client::request("GET", &url, "", &[]).as_deref(), Ok(want));
        join(h);
    }
}

/// C-405 (#4112, RFC 9112 §2.2 / §3 / §3.2): a malformed request line, a
/// non-origin target, a bare LF or CR, and a missing or repeated Host are 400
/// before the handler runs; an absolute-form target reaches it as origin-form.
#[test]
fn a_malformed_request_line_or_host_is_400() {
    for head in [
        &b"GET  /x HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"GET /x HTTP/1.1 extra\r\nHost: a\r\n\r\n"[..],
        &b"GET\t/x HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"G(T /x HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"GET /x HTTP/9.9\r\nHost: a\r\n\r\n"[..],
        &b"GET /x\r\nHost: a\r\n\r\n"[..],
        &b"GET /a\x00b HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"OPTIONS * HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"CONNECT 127.0.0.1:22 HTTP/1.1\r\nHost: a\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost : a\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\nHost: a\n\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\rX: 1\r\n\r\n"[..],
    ] {
        let resp = exchange(head, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
        assert_eq!(status_line(&resp), "HTTP/1.1 400 Bad Request", "{:?}", String::from_utf8_lossy(head));
    }
    for (head, want) in [
        (&b"GET http://evil.example/p?q=1 HTTP/1.1\r\nHost: a\r\n\r\n"[..], "/p?q=1"),
        (&b"GET HTTPS://evil.example HTTP/1.1\r\nHost: a\r\n\r\n"[..], "/"),
        (&b"GET http://evil.example?x HTTP/1.1\r\nHost: a\r\n\r\n"[..], "/?x"),
        (&b"GET /x HTTP/1.0\r\n\r\n"[..], "/x"),
    ] {
        let resp = exchange(head, false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, (_, path, _, _)| {
            server::http_server_write(conn, 200, &[], &path).expect("written")
        });
        assert!(resp.ends_with(&format!("\r\n\r\n{want}")), "{resp:?}");
    }
}

/// C-407 (#4134): every response carries `X-Content-Type-Options: nosniff`
/// after the handler's fields, unless the handler set that field.
#[test]
fn the_response_carries_nosniff_unless_the_handler_set_it() {
    let plain = String::from_utf8(server::http_server_response_bytes(200, &[("X-A".into(), "1".into())], "x")).expect("utf8");
    assert_eq!(plain, "HTTP/1.1 200 OK\r\nX-A: 1\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 1\r\n\r\nx");
    let own = String::from_utf8(server::http_server_response_bytes(200, &[("x-content-type-options".into(), "other".into())], "")).expect("utf8");
    assert_eq!(own, "HTTP/1.1 200 OK\r\nx-content-type-options: other\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
}

/// C-402 (#4116): the client's URL splitter applies url.parse's rules where
/// a browser could read the URL as pointing somewhere else.
#[test]
fn the_client_url_splitter_follows_url_parse_where_a_browser_could_disagree() {
    for bad in [
        "http://allowed.example\\@127.0.0.1:9/x",
        "http://a.example/x\u{1}y",
        "http://127.1/",
        "http://0x7f.0.0.1/",
        "http://2130706433/",
        "http://0177.0.0.1/",
        "http://256.0.0.1/",
        "http://1.2.3.4.5/",
    ] {
        assert!(client::http_parse_url(bad).is_err(), "{bad:?} was accepted");
    }
    let u = client::http_parse_url(" \thttp://Good.example/a/%2e%2e/b/./c?q=/../x\n").expect("parses");
    assert_eq!((u.host.as_str(), u.target.as_str()), ("good.example", "/b/c?q=/../x"));
    let u = client::http_parse_url("http://127.0.0.1:8/a/..").expect("parses");
    assert_eq!((u.host.as_str(), u.target.as_str()), ("127.0.0.1", "/"));
}

/// C-408: a response head that is not whole within HTTP_MAX_HEAD_BYTES is
/// refused on the buffered path, as the call path refuses it.
#[test]
fn the_buffered_client_caps_the_response_head() {
    let mut head = b"HTTP/1.1 200 OK\r\n".to_vec();
    while head.len() <= (1 << 20) + 64 {
        head.extend_from_slice(b"x: y\r\n");
    }
    head.extend_from_slice(b"Content-Length: 0\r\n\r\n");
    let (url, h) = pieces_peer(vec![head]);
    let got = client::request("GET", &url, "", &[]);
    join(h);
    assert_eq!(got, Err(format!("malformed or incomplete response from \"{url}\"")));
}

#[test]
fn a_body_framed_by_the_close_ends_with_the_close() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 200 OK\r\n\r\nuntil".to_vec(), b" close".to_vec()]);
    let sh = start(&url, 0, 0);
    assert_eq!(client::http_call_wait(&sh).expect("answered").2, "until close");
    join(h);
}

#[test]
fn a_close_before_the_head_is_an_error() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 200 OK\r\nX-Partial".to_vec()]);
    let sh = start(&url, 0, 0);
    assert_eq!(wait_err(&sh), format!("malformed or incomplete response from \"{url}\""));
    join(h);
}

// ── interrupted reads ──
//
// A read the kernel interrupts (EINTR, `ErrorKind::Interrupted`) moved no
// bytes and is to be retried — never the end of the response. Linux returns
// EINTR from a blocking `recv` on a socket with SO_RCVTIMEO whenever a
// signal reaches the reading thread, SA_RESTART or not (signal(7)), and every
// client read here runs under a timeout. A program that spawned a process
// while a call was in flight got `malformed or incomplete response` natively
// while the embedded lane, its reads on another process, answered.

/// A stream that serves `pieces`, each preceded by one `Interrupted` read.
struct Interrupting {
    pieces: std::collections::VecDeque<Vec<u8>>,
    interrupt_next: bool,
}

impl Interrupting {
    fn new(pieces: &[&[u8]]) -> Self {
        Interrupting { pieces: pieces.iter().map(|p| p.to_vec()).collect(), interrupt_next: true }
    }
}

impl Read for Interrupting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.interrupt_next && !self.pieces.is_empty() {
            self.interrupt_next = false;
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
        }
        self.interrupt_next = true;
        let Some(mut p) = self.pieces.pop_front() else { return Ok(0) };
        let n = p.len().min(buf.len());
        buf[..n].copy_from_slice(&p[..n]);
        if n < p.len() {
            self.pieces.push_front(p.split_off(n));
        }
        Ok(n)
    }
}

impl Write for Interrupting {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const INTERRUPTED_PIECES: [&[u8]; 3] = [b"HTTP/1.1 404 Not Found\r\nContent-Le", b"ngth: 4\r\n\r\ns4", b"04"];

#[test]
fn an_interrupted_read_is_retried_by_the_buffered_client() {
    let url = "http://127.0.0.1:1/status/404";
    let got = client::read_response_tolerant(&mut Interrupting::new(&INTERRUPTED_PIECES), url, None).expect("answered");
    assert_eq!(got, INTERRUPTED_PIECES.concat());
    let raw = client::http_exchange_raw(&mut Interrupting::new(&INTERRUPTED_PIECES), url, b"GET / HTTP/1.1\r\nHost: a\r\n\r\n").expect("answered");
    assert_eq!(raw, INTERRUPTED_PIECES.concat());
}

/// The env var that turns `signalled_call_child` into the scenario below.
#[cfg(target_os = "linux")]
const SIGNALLED_CALL_URL: &str = "ALMIDE_TEST_SIGNALLED_CALL_URL";

/// The call handle end to end, the way the finding arose: a program spawns a
/// command that does not exist while a call with `idle_ms` waits for its
/// response. The child exits while the spawning thread still blocks every
/// signal (posix_spawn), so its SIGCHLD goes to a thread that does not — the
/// call's worker, in a process that has only those two threads. That is the
/// shape of a compiled program, so the scenario runs in a fresh copy of this
/// test binary (one test, on the main thread); the peer stays here.
#[cfg(target_os = "linux")]
#[test]
fn a_signal_during_a_timed_read_does_not_end_the_call() {
    for _ in 0..20 {
        let (url, h) = peer(|mut s| {
            thread::sleep(Duration::from_millis(300));
            let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\n\r\ns404");
        });
        let out = std::process::Command::new(std::env::current_exe().expect("the test binary"))
            .args(["--exact", "signalled_call_child", "--test-threads=1", "--nocapture"])
            .env(SIGNALLED_CALL_URL, &url)
            .output()
            .expect("the child runs");
        let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{text}");
        assert!(text.contains("answered 404 s404"), "{text}");
        join(h);
    }
}

/// The child half of the test above; a no-op unless its env var is set.
#[cfg(target_os = "linux")]
#[test]
fn signalled_call_child() {
    let Ok(url) = std::env::var(SIGNALLED_CALL_URL) else { return };
    let sh = start(&url, 0, 30_000);
    while client::http_call_poll(&sh).is_none() {
        let _ = std::process::Command::new("/no/such/command").current_dir(".").output();
    }
    let (status, _, body) = client::http_call_wait(&sh).expect("answered");
    println!("answered {status} {body}");
}

// ── streaming ──

#[test]
fn the_stream_step_splits_only_between_characters() {
    let body = "añb€c";
    let bytes = body.as_bytes();
    // Split inside `ñ` and inside `€`.
    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len());
    let mut first = head.into_bytes();
    first.extend_from_slice(&bytes[..2]);
    let (url, h) = pieces_peer(vec![first, bytes[2..5].to_vec(), bytes[5..].to_vec()]);
    let sh = start(&url, 0, 0);
    let (text, outcome) = stream_all(&sh);
    assert_eq!(text, body);
    assert_eq!(outcome, Ok(()));
    join(h);
}

#[test]
fn a_non_2xx_stream_is_refused_with_its_body_quoted() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 404 Not Found\r\nContent-Length: 7\r\n\r\nmissing".to_vec()]);
    let sh = start(&url, 0, 0);
    let (text, outcome) = client::http_call_stream_step(&sh);
    assert_eq!(text, "");
    assert_eq!(outcome, Some(Err("HTTP 404: missing".to_string())));
    join(h);
}

// ── limits and cancel ──

#[test]
fn an_idle_peer_trips_idle_ms() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 0, 150);
    assert_eq!(wait_err(&sh), "request timeout: idle_ms 150 exceeded");
    let _ = tx.send(());
    join(h);
}

#[test]
fn a_silent_peer_trips_total_ms() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 200, 0);
    assert_eq!(wait_err(&sh), "request timeout: total_ms 200 exceeded");
    // Both limits set: the wall clock is the shorter one and names itself.
    let _ = tx.send(());
    join(h);
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 150, 5_000);
    assert_eq!(wait_err(&sh), "request timeout: total_ms 150 exceeded");
    let _ = tx.send(());
    join(h);
}

#[test]
fn the_wall_clock_is_checked_from_the_callers_side() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 100, 0);
    thread::sleep(Duration::from_millis(250));
    // poll and read_new check the deadline themselves.
    assert_eq!(client::http_call_read_new(&sh), "");
    assert_eq!(client::http_call_poll(&sh), Some(Err("request timeout: total_ms 100 exceeded".to_string())));
    let _ = tx.send(());
    join(h);
}

#[test]
fn a_streaming_step_is_bounded_by_the_wall_clock() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 150, 0);
    let (text, outcome) = stream_all(&sh);
    assert_eq!(text, "");
    assert_eq!(outcome, Err("request timeout: total_ms 150 exceeded".to_string()));
    let _ = tx.send(());
    join(h);
}

#[test]
fn cancel_ends_a_running_call_and_drops_unread_bytes() {
    let (tx, rx) = mpsc::channel::<()>();
    let (url, h) = peer(move |mut s| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial");
        let _ = s.flush();
        let _ = rx.recv_timeout(Duration::from_secs(10));
    });
    let sh = start(&url, 0, 0);
    // Wait until the partial body is in, without consuming it.
    let mut seen = false;
    for _ in 0..200 {
        if client::http_call_poll(&sh).is_none() {
            thread::sleep(Duration::from_millis(10));
        }
        let (piece, ended) = client::http_call_stream_step(&sh);
        assert!(ended.is_none());
        if !piece.is_empty() {
            assert_eq!(piece, "partial");
            seen = true;
            break;
        }
    }
    assert!(seen, "the partial body arrived");
    assert!(client::http_call_poll(&sh).is_none(), "still running");
    sh.cancel();
    assert_eq!(wait_err(&sh), "request cancelled");
    assert_eq!(client::http_call_read_new(&sh), "");
    let _ = tx.send(());
    join(h);
}

#[test]
fn cancel_before_the_head_arrives() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 0, 0);
    thread::sleep(Duration::from_millis(50));
    sh.cancel();
    assert_eq!(wait_err(&sh), "request cancelled");
    assert_eq!(client::http_call_poll(&sh), Some(Err("request cancelled".to_string())));
    let _ = tx.send(());
    join(h);
}

// ── the server core ──

#[test]
fn the_response_bytes_carry_the_registered_reason_and_close_the_connection() {
    let table = [
        (200, "OK"),
        (201, "Created"),
        (301, "Moved Permanently"),
        (400, "Bad Request"),
        (404, "Not Found"),
        (413, "Content Too Large"),
        (418, "I'm a teapot"),
        (429, "Too Many Requests"),
        (500, "Internal Server Error"),
        (503, "Service Unavailable"),
        (599, ""),
    ];
    for (status, reason) in table {
        let out = text(server::http_server_response_bytes(status, &[], ""));
        assert_eq!(out, format!("HTTP/1.1 {status} {reason}\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"));
    }
    let headers = vec![
        ("Content-Type".to_string(), "text/plain".to_string()),
        ("connection".to_string(), "keep-alive".to_string()),
        ("X-B".to_string(), "2".to_string()),
    ];
    let out = text(server::http_server_response_bytes(200, &headers, "héllo"));
    assert_eq!(out, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-B: 2\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 6\r\n\r\nhéllo");
}

#[test]
fn head_204_and_304_carry_no_body() {
    let head = text(server::http_server_response_bytes_for(true, 200, &[], "hello"));
    assert_eq!(head, "HTTP/1.1 200 OK\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 5\r\n\r\n");
    let no_content = text(server::http_server_response_bytes(204, &[], "x"));
    assert_eq!(no_content, "HTTP/1.1 204 No Content\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n");
    let not_modified = text(server::http_server_response_bytes(304, &[], "x"));
    assert_eq!(not_modified, "HTTP/1.1 304 Not Modified\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 1\r\n\r\n");
}

#[test]
fn a_header_that_would_split_the_response_is_named() {
    assert_eq!(server::http_server_bad_header("X-A", "1"), None);
    assert_eq!(server::http_server_bad_header("X-A", "a b\t\"c\""), None);
    assert!(server::http_server_bad_header("X-A", "1\r\nSet-Cookie: evil=1").is_some());
    assert!(server::http_server_bad_header("X-A", "1\nx").is_some());
    assert!(server::http_server_bad_header("X-A", "1\0").is_some());
    assert!(server::http_server_bad_header("X-A\r\nSet-Cookie", "1").is_some());
    assert!(server::http_server_bad_header("X A", "1").is_some());
    assert!(server::http_server_bad_header("", "1").is_some());
}

#[test]
fn a_bind_failure_names_itself() {
    let e = server::http_server_bind(99_999).expect_err("port 99999 is refused");
    assert!(e.starts_with("bind failed: "), "{e}");
}

/// A connection to the server on `port` that has sent `raw`.
fn connect_send(port: u16, raw: &[u8]) -> std::io::Result<TcpStream> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.write_all(raw)?;
    Ok(s)
}

/// Send `raw` on a fresh connection and read the answer to the close.
fn fetch(port: u16, raw: &[u8]) -> std::io::Result<String> {
    let mut s = connect_send(port, raw)?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

/// One raw exchange against `http_server_next_with`: the client sends `raw`
/// (then half-closes when `half_close`), the server either answers the
/// request itself or hands it over, and `serve` gets what was handed over.
fn exchange(
    raw: &'static [u8],
    half_close: bool,
    limits: server::HttpServerLimits,
    serve: impl FnOnce(server::HttpServerConn, server::HttpServerRequest),
) -> String {
    let listener = server::http_server_bind(0).expect("bind");
    let port = port_of(&listener);
    let client = thread::spawn(move || {
        let mut s = connect_send(port, raw).expect("the client reaches the server");
        if half_close {
            s.shutdown(std::net::Shutdown::Write).expect("half-close the request");
        }
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        // The core answered: a second connection proves it keeps serving.
        let second = fetch(port, b"GET /next HTTP/1.1\r\nHost: a\r\n\r\n").expect("the second request is answered");
        (String::from_utf8_lossy(&resp).into_owned(), second)
    });
    let (conn, req) = server::http_server_next_with(&listener, &limits).expect("no shutdown signal in this test");
    let mut out = String::new();
    if req.1 == "/next" {
        server::http_server_write(conn, 200, &[], "next").expect("written");
    } else {
        serve(conn, req);
        let (conn, req) = server::http_server_next_with(&listener, &limits).expect("second");
        assert_eq!(req.1, "/next");
        server::http_server_write(conn, 200, &[], "next").expect("written");
    }
    let (first, second) = join(client);
    assert!(second.ends_with("\r\n\r\nnext"), "the server stopped answering: {second:?}");
    out.push_str(&first);
    out
}

fn status_line(resp: &str) -> &str {
    resp.split("\r\n").next().unwrap_or_default()
}

#[test]
fn a_huge_content_length_is_413_without_allocating_it() {
    let resp = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 99999999999\r\n\r\nabc",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&resp), "HTTP/1.1 413 Content Too Large");
    let past_u64 = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 999999999999999999999999\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&past_u64), "HTTP/1.1 413 Content Too Large");
}

#[test]
fn a_short_body_is_400_and_a_stalled_one_is_503() {
    let cut = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 10\r\n\r\nabc",
        true,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&cut), "HTTP/1.1 400 Bad Request");
    let limits = server::HttpServerLimits { max_body_bytes: 1 << 20, request_timeout_ms: 300, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let stalled = exchange(b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 10\r\n\r\nabc", false, limits, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&stalled), "HTTP/1.1 503 Service Unavailable");
}

#[test]
fn a_response_after_the_request_timeout_is_503() {
    let limits = server::HttpServerLimits { max_body_bytes: 1 << 20, request_timeout_ms: 200, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let resp = exchange(b"GET /slow HTTP/1.1\r\nHost: a\r\n\r\n", false, limits, |conn, _| {
        thread::sleep(Duration::from_millis(400));
        server::http_server_write(conn, 200, &[], "late").expect("written");
    });
    assert_eq!(status_line(&resp), "HTTP/1.1 503 Service Unavailable");
    assert!(!resp.ends_with("late"), "{resp:?}");
}

#[test]
fn a_chunked_body_is_decoded_and_bounded() {
    let resp = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n2;ext=1\r\nhe\r\n3\r\nllo\r\n0\r\nX-Trailer: t\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |conn, (method, _, body, _)| server::http_server_write(conn, 200, &[], &format!("{method} {body}")).expect("written"),
    );
    assert!(resp.ends_with("\r\n\r\nPOST hello"), "{resp:?}");
    let limits = server::HttpServerLimits { max_body_bytes: 4, request_timeout_ms: 30_000, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let over = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
        false,
        limits,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&over), "HTTP/1.1 413 Content Too Large");
    let gzip = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&gzip), "HTTP/1.1 501 Not Implemented");
    let both = exchange(
        b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&both), "HTTP/1.1 400 Bad Request");
}

/// RFC 9112 §5.1 / §5.2 (C-405, #4112): whitespace before a header colon, a
/// line without a colon and an obs-fold line are each 400 before the handler
/// runs; a well-formed head with optional whitespace around the value is read.
#[test]
fn malformed_header_field_lines_are_400() {
    for head in [
        &b"POST /x HTTP/1.1\r\nHost: a\r\nTransfer-Encoding : chunked\r\n\r\n0\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nno colon here\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nX-A: 1\r\n folded: 2\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nX-A: 1\r\n\tfolded\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nX-A: 1\r\n   \r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\n: empty-name\r\n\r\n"[..],
        &b"GET /x HTTP/1.1\r\nHost: a\r\nX A: 1\r\n\r\n"[..],
    ] {
        let resp = exchange(head, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
        assert_eq!(status_line(&resp), "HTTP/1.1 400 Bad Request", "{:?}", String::from_utf8_lossy(head));
    }
    let ok = exchange(
        b"GET /x HTTP/1.1\r\nHost: a\r\nX-A:\t 1 \t\r\nX-B:2\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |conn, (_, _, _, headers)| server::http_server_write(conn, 200, &[], &format!("{headers:?}")).expect("written"),
    );
    assert!(ok.ends_with(r#"[("Host", "a"), ("X-A", "1"), ("X-B", "2")]"#), "{ok:?}");
}

#[test]
fn header_lines_are_bounded_in_length_and_count() {
    static LONG: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let long = LONG.get_or_init(|| format!("GET /x HTTP/1.1\r\nHost: a\r\nX-Long: {}\r\n\r\n", "a".repeat(10_000)).into_bytes());
    let resp = exchange(long, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 431 Request Header Fields Too Large");
    static MANY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let many = MANY.get_or_init(|| format!("GET /x HTTP/1.1\r\nHost: a\r\n{}\r\n", "X-H: 1\r\n".repeat(101)).into_bytes());
    let resp = exchange(many, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 431 Request Header Fields Too Large");
}

#[test]
fn a_crlf_header_is_refused_with_500() {
    let resp = exchange(b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, _| {
        let evil = [("X-A".to_string(), "1\r\nSet-Cookie: evil=1".to_string())];
        server::http_server_write(conn, 200, &evil, "x").expect("written");
    });
    assert_eq!(status_line(&resp), "HTTP/1.1 500 Internal Server Error");
    assert!(!resp.contains("Set-Cookie"), "{resp:?}");
}

/// Every control character in a value is refused, HTAB alone allowed
/// (RFC 9110 §5.5); the refusal names the field, and a name that is not a
/// token is quoted with its control characters escaped.
#[test]
fn a_control_character_in_a_value_is_refused_and_a_bad_name_is_quoted() {
    assert_eq!(server::http_server_bad_header("X-A", "a\tb"), None);
    assert_eq!(server::http_server_bad_header("X-A", "café"), None);
    for v in ["a\u{1}b", "a\u{7f}", "\u{1b}[31m", "1\r\nx", "1\0"] {
        assert_eq!(server::http_server_bad_header("X-A", v).as_deref(), Some("response header X-A has a control character in its value"), "{v:?}");
    }
    assert_eq!(server::http_server_bad_header("a b", "v").as_deref(), Some("response header name \"a b\" is not a token"));
    assert_eq!(server::http_server_bad_header("é\"\\\u{1}\u{85}", "v").as_deref(), Some("response header name \"é\\\"\\\\\\u{1}\\u{85}\" is not a token"));
    assert_eq!(server::http_server_bad_header("", "v").as_deref(), Some("response header name \"\" is not a token"));
}

/// A final status is a 3-digit code in 200..=599 (RFC 9110 §15): anything
/// else, a 1xx included, is refused.
#[test]
fn a_status_outside_200_to_599_is_refused() {
    for s in [200, 204, 299, 304, 418, 599] {
        assert_eq!(server::http_server_bad_status(s), None, "{s}");
    }
    for s in [0, 42, 100, 103, 199, 600, 1000, -1, i64::MIN, i64::MAX] {
        assert_eq!(server::http_server_bad_status(s), Some(format!("response status {s} is not a final status code")), "{s}");
    }
}

/// The server owns the framing: a handler's Content-Length,
/// Transfer-Encoding and Keep-Alive are dropped, as its Connection is.
#[test]
fn the_handlers_framing_fields_are_dropped() {
    let headers = [
        ("Content-Length".to_string(), "99".to_string()),
        ("transfer-encoding".to_string(), "chunked".to_string()),
        ("Keep-Alive".to_string(), "timeout=5".to_string()),
        ("Connection".to_string(), "keep-alive".to_string()),
        ("X-B".to_string(), "2".to_string()),
    ];
    let out = text(server::http_server_response_bytes(200, &headers, "hello"));
    assert_eq!(out, "HTTP/1.1 200 OK\r\nX-B: 2\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 5\r\n\r\nhello");
}

#[test]
fn a_bad_status_or_control_value_is_answered_500_with_the_reason() {
    let resp = exchange(b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, _| {
        server::http_server_write(conn, 103, &[], "x").expect("written");
    });
    assert_eq!(status_line(&resp), "HTTP/1.1 500 Internal Server Error");
    assert!(resp.ends_with("\r\n\r\nInternal Server Error"), "{resp:?}");
    let resp = exchange(b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, _| {
        server::http_server_write(conn, 200, &[("a b".to_string(), "v".to_string())], "x").expect("written");
    });
    assert!(resp.ends_with("\r\n\r\nInternal Server Error"), "{resp:?}");
}

#[test]
fn the_server_skips_an_unparsable_request_and_answers_the_next() {
    let listener = server::http_server_bind(0).expect("bind");
    let port = port_of(&listener);
    let client = thread::spawn(move || {
        // First connection: a request line with no target — a 400 the core
        // answers itself (C-405, RFC 9112 §3).
        let mut bad = connect_send(port, b"GARBAGE\r\n\r\n").expect("the client reaches the server");
        let mut rest = Vec::new();
        let _ = bad.read_to_end(&mut rest);
        assert!(rest.starts_with(b"HTTP/1.1 400 Bad Request\r\n"), "{:?}", String::from_utf8_lossy(&rest));
        // Second connection: a request with headers and a body.
        fetch(port, b"POST /echo?x=1 HTTP/1.1\r\nHost: t\r\ncontent-length: 4\r\nX-K:  v \r\n\r\nping")
            .expect("the second connection is answered")
    });
    let (stream, (method, target, body, headers)) = server::http_server_next(&listener).expect("no shutdown signal in this test");
    assert_eq!((method.as_str(), target.as_str(), body.as_str()), ("POST", "/echo?x=1", "ping"));
    assert_eq!(
        headers,
        vec![
            ("Host".to_string(), "t".to_string()),
            ("content-length".to_string(), "4".to_string()),
            ("X-K".to_string(), "v".to_string()),
        ]
    );
    server::http_server_write(stream, 404, &[("X-R".to_string(), "1".to_string())], "nope").expect("written");
    assert_eq!(join(client), "HTTP/1.1 404 Not Found\r\nX-R: 1\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 4\r\n\r\nnope");
}

#[test]
fn a_request_without_a_body_reads_as_empty_and_a_garbled_length_is_400() {
    let resp = exchange(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, (method, target, body, _)| {
        assert_eq!((method.as_str(), target.as_str(), body.as_str()), ("GET", "/", ""));
        server::http_server_write(conn, 200, &[], "").expect("written");
    });
    assert_eq!(resp, "HTTP/1.1 200 OK\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    let nan = exchange(b"GET / HTTP/1.1\r\nHost: a\r\nContent-Length: nan\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| {
        panic!("the handler saw {r:?}")
    });
    assert_eq!(status_line(&nan), "HTTP/1.1 400 Bad Request");
}

// ── slow clients (#4021) ──

/// Serve `listener` through the pool on a thread of its own, answering each
/// request with its target. The thread is left running: a test never sends
/// the shutdown signal, which is process-global.
fn pool_serving(listener: TcpListener, limits: server::HttpServerLimits) {
    thread::spawn(move || {
        let pool = server::http_server_pool(listener, limits);
        while let Some((conn, req)) = pool.next() {
            let _ = server::http_server_write(conn, 200, &[], &req.1);
        }
    });
}

/// A client that sends `head` one byte every `gap`, then waits for the
/// server's answer: answers it and how long after connecting it arrived.
fn trickle(port: u16, head: &'static [u8], gap: Duration) -> thread::JoinHandle<(String, Duration)> {
    thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("the slow client connects");
        for b in head {
            // The server may close the connection first (the deadline): stop.
            if s.write_all(&[*b]).is_err() {
                break;
            }
            thread::sleep(gap);
            // Once the server answered, stop writing: a late byte would turn
            // its close into a reset that can discard the answer.
            s.set_nonblocking(true).expect("nonblocking");
            let answered = !matches!(s.peek(&mut [0u8; 1]), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock);
            s.set_nonblocking(false).expect("blocking");
            if answered {
                break;
            }
        }
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        (String::from_utf8_lossy(&out).into_owned(), started.elapsed())
    })
}

#[test]
fn a_client_trickling_its_head_does_not_hold_up_the_next_client() {
    let listener = server::http_server_bind(0).expect("bind");
    let port = port_of(&listener);
    pool_serving(listener, server::HTTP_SERVER_DEFAULT_LIMITS);
    // 40 bytes at 50 ms: two seconds of head, well under the 10 s deadline.
    let slow = trickle(port, b"GET /slow HTTP/1.1\r\nHost: a\r\nX-A: 1\r\n\r\n", Duration::from_millis(50));
    thread::sleep(Duration::from_millis(200));
    let started = std::time::Instant::now();
    let fast = fetch(port, b"GET /fast HTTP/1.1\r\nHost: a\r\n\r\n").expect("the second client is answered");
    let took = started.elapsed();
    assert!(fast.ends_with("\r\n\r\n/fast"), "{fast:?}");
    assert!(took < Duration::from_secs(1), "the second client waited {took:?} behind the slow one");
    // The slow client is answered too, once its head is whole.
    let (resp, _) = join(slow);
    assert!(resp.ends_with("\r\n\r\n/slow"), "{resp:?}");
}

#[test]
fn a_head_not_whole_by_the_header_deadline_is_408_and_a_silent_client_is_closed_unanswered() {
    let limits = server::HttpServerLimits { header_timeout_ms: 300, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let listener = server::http_server_bind(0).expect("bind");
    let port = port_of(&listener);
    pool_serving(listener, limits);
    // One byte every 50 ms for six seconds: the deadline cuts it at 300 ms.
    let slow = trickle(port, &[b'G'; 120], Duration::from_millis(50));
    let silent = thread::spawn(move || {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("the silent client connects");
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        out
    });
    let (resp, took) = join(slow);
    assert_eq!(status_line(&resp), "HTTP/1.1 408 Request Timeout", "{resp:?}");
    assert!(resp.contains("\r\nConnection: close\r\n"), "{resp:?}");
    assert!(took >= Duration::from_millis(300) && took < Duration::from_secs(3), "the 408 came after {took:?}");
    assert!(join(silent).is_empty(), "a client that sent nothing gets no answer");
    // The sequential reader applies the same deadline.
    let limits = server::HttpServerLimits { header_timeout_ms: 200, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let resp = exchange(b"GET /x HTTP/1.1\r\nHost: a\r\n", false, limits, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 408 Request Timeout");
    // A stalled body is still the request timeout's 503, not a 408.
    let limits = server::HttpServerLimits { header_timeout_ms: 200, request_timeout_ms: 400, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let resp = exchange(b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 10\r\n\r\nabc", false, limits, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 503 Service Unavailable");
}

#[test]
fn at_max_in_flight_the_pool_stops_accepting_until_a_connection_closes() {
    let limits = server::HttpServerLimits { header_timeout_ms: 500, max_in_flight: 2, ..server::HTTP_SERVER_DEFAULT_LIMITS };
    let listener = server::http_server_bind(0).expect("bind");
    let port = port_of(&listener);
    pool_serving(listener, limits);
    // Two silent connections take both places until the header deadline
    // closes them.
    let held: Vec<TcpStream> = (0..2).map(|_| TcpStream::connect(("127.0.0.1", port)).expect("connect")).collect();
    thread::sleep(Duration::from_millis(100));
    let started = std::time::Instant::now();
    let third = fetch(port, b"GET /third HTTP/1.1\r\nHost: a\r\n\r\n").expect("the third client is answered");
    let took = started.elapsed();
    assert!(third.ends_with("\r\n\r\n/third"), "{third:?}");
    assert!(took >= Duration::from_millis(300), "the third connection was read at {took:?}, past the bound of 2");
    drop(held);
}
