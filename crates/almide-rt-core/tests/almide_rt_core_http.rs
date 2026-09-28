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

/// A one-connection peer: accept, read the request head, then run `reply`.
fn peer(reply: impl FnOnce(TcpStream) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://127.0.0.1:{}/path?q=1", l.local_addr().unwrap().port());
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
    let e = client::http_call_spawn("GET", "http://127.0.0.1:1/", "", vec![], -1, 0).err().unwrap();
    assert_eq!(e, "invalid limits: total_ms and idle_ms must be >= 0 (0 = no limit), got total_ms -1 idle_ms 0");
    let e = client::http_call_spawn("GET", "http://127.0.0.1:1/", "", vec![], 0, -5).err().unwrap();
    assert!(e.contains("got total_ms 0 idle_ms -5"), "{e}");
}

#[test]
fn a_refused_connection_is_a_connection_error() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let want = format!("could not connect to \"http://127.0.0.1:{port}/\" (connection refused or unreachable)");
    let sh = start(&format!("http://127.0.0.1:{port}/"), 0, 0);
    let e = client::http_call_wait(&sh).err().unwrap();
    assert_eq!(e, want);
    // The same with a wall clock set: the dial is bounded by it.
    let sh = start(&format!("http://127.0.0.1:{port}/"), 5_000, 0);
    let e = client::http_call_wait(&sh).err().unwrap();
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
    h.join().unwrap();
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
    h.join().unwrap();
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
    h.join().unwrap();
}

#[test]
fn a_body_framed_by_the_close_ends_with_the_close() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 200 OK\r\n\r\nuntil".to_vec(), b" close".to_vec()]);
    let sh = start(&url, 0, 0);
    assert_eq!(client::http_call_wait(&sh).expect("answered").2, "until close");
    h.join().unwrap();
}

#[test]
fn a_close_before_the_head_is_an_error() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 200 OK\r\nX-Partial".to_vec()]);
    let sh = start(&url, 0, 0);
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), format!("malformed or incomplete response from \"{url}\""));
    h.join().unwrap();
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
    h.join().unwrap();
}

#[test]
fn a_non_2xx_stream_is_refused_with_its_body_quoted() {
    let (url, h) = pieces_peer(vec![b"HTTP/1.1 404 Not Found\r\nContent-Length: 7\r\n\r\nmissing".to_vec()]);
    let sh = start(&url, 0, 0);
    let (text, outcome) = client::http_call_stream_step(&sh);
    assert_eq!(text, "");
    assert_eq!(outcome, Some(Err("HTTP 404: Not Found: missing".to_string())));
    h.join().unwrap();
}

// ── limits and cancel ──

#[test]
fn an_idle_peer_trips_idle_ms() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 0, 150);
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), "request timeout: idle_ms 150 exceeded");
    let _ = tx.send(());
    h.join().unwrap();
}

#[test]
fn a_silent_peer_trips_total_ms() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 200, 0);
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), "request timeout: total_ms 200 exceeded");
    // Both limits set: the wall clock is the shorter one and names itself.
    let _ = tx.send(());
    h.join().unwrap();
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 150, 5_000);
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), "request timeout: total_ms 150 exceeded");
    let _ = tx.send(());
    h.join().unwrap();
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
    h.join().unwrap();
}

#[test]
fn a_streaming_step_is_bounded_by_the_wall_clock() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 150, 0);
    let (text, outcome) = stream_all(&sh);
    assert_eq!(text, "");
    assert_eq!(outcome, Err("request timeout: total_ms 150 exceeded".to_string()));
    let _ = tx.send(());
    h.join().unwrap();
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
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), "request cancelled");
    assert_eq!(client::http_call_read_new(&sh), "");
    let _ = tx.send(());
    h.join().unwrap();
}

#[test]
fn cancel_before_the_head_arrives() {
    let (url, tx, h) = silent_peer();
    let sh = start(&url, 0, 0);
    thread::sleep(Duration::from_millis(50));
    sh.cancel();
    assert_eq!(client::http_call_wait(&sh).err().unwrap(), "request cancelled");
    assert_eq!(client::http_call_poll(&sh), Some(Err("request cancelled".to_string())));
    let _ = tx.send(());
    h.join().unwrap();
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
        let out = String::from_utf8(server::http_server_response_bytes(status, &[], "")).unwrap();
        assert_eq!(out, format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"));
    }
    let headers = vec![
        ("Content-Type".to_string(), "text/plain".to_string()),
        ("connection".to_string(), "keep-alive".to_string()),
        ("X-B".to_string(), "2".to_string()),
    ];
    let out = String::from_utf8(server::http_server_response_bytes(200, &headers, "héllo")).unwrap();
    assert_eq!(out, "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-B: 2\r\nConnection: close\r\nContent-Length: 6\r\n\r\nhéllo");
}

#[test]
fn head_204_and_304_carry_no_body() {
    let head = String::from_utf8(server::http_server_response_bytes_for(true, 200, &[], "hello")).unwrap();
    assert_eq!(head, "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 5\r\n\r\n");
    let no_content = String::from_utf8(server::http_server_response_bytes(204, &[], "x")).unwrap();
    assert_eq!(no_content, "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
    let not_modified = String::from_utf8(server::http_server_response_bytes(304, &[], "x")).unwrap();
    assert_eq!(not_modified, "HTTP/1.1 304 Not Modified\r\nConnection: close\r\nContent-Length: 1\r\n\r\n");
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
    let e = server::http_server_bind(99_999).err().unwrap();
    assert!(e.starts_with("bind failed: "), "{e}");
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
    let port = listener.local_addr().unwrap().port();
    let client = thread::spawn(move || {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(raw).unwrap();
        if half_close {
            s.shutdown(std::net::Shutdown::Write).unwrap();
        }
        let mut resp = Vec::new();
        let _ = s.read_to_end(&mut resp);
        // The core answered: a second connection proves it keeps serving.
        let mut next = TcpStream::connect(("127.0.0.1", port)).unwrap();
        next.write_all(b"GET /next HTTP/1.1\r\n\r\n").unwrap();
        let mut second = String::new();
        next.read_to_string(&mut second).unwrap();
        (String::from_utf8_lossy(&resp).into_owned(), second)
    });
    let (conn, req) = server::http_server_next_with(&listener, &limits).expect("no shutdown signal in this test");
    let mut out = String::new();
    if req.1 == "/next" {
        server::http_server_write(conn, 200, &[], "next").unwrap();
    } else {
        serve(conn, req);
        let (conn, req) = server::http_server_next_with(&listener, &limits).expect("second");
        assert_eq!(req.1, "/next");
        server::http_server_write(conn, 200, &[], "next").unwrap();
    }
    let (first, second) = client.join().unwrap();
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
        b"POST /x HTTP/1.1\r\nContent-Length: 999999999999999999999999\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&past_u64), "HTTP/1.1 413 Content Too Large");
}

#[test]
fn a_short_body_is_400_and_a_stalled_one_is_503() {
    let cut = exchange(
        b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc",
        true,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&cut), "HTTP/1.1 400 Bad Request");
    let limits = server::HttpServerLimits { max_body_bytes: 1 << 20, request_timeout_ms: 300 };
    let stalled = exchange(b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc", false, limits, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&stalled), "HTTP/1.1 503 Service Unavailable");
}

#[test]
fn a_response_after_the_request_timeout_is_503() {
    let limits = server::HttpServerLimits { max_body_bytes: 1 << 20, request_timeout_ms: 200 };
    let resp = exchange(b"GET /slow HTTP/1.1\r\n\r\n", false, limits, |conn, _| {
        thread::sleep(Duration::from_millis(400));
        server::http_server_write(conn, 200, &[], "late").unwrap();
    });
    assert_eq!(status_line(&resp), "HTTP/1.1 503 Service Unavailable");
    assert!(!resp.ends_with("late"), "{resp:?}");
}

#[test]
fn a_chunked_body_is_decoded_and_bounded() {
    let resp = exchange(
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n2;ext=1\r\nhe\r\n3\r\nllo\r\n0\r\nX-Trailer: t\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |conn, (method, _, body, _)| server::http_server_write(conn, 200, &[], &format!("{method} {body}")).unwrap(),
    );
    assert!(resp.ends_with("\r\n\r\nPOST hello"), "{resp:?}");
    let limits = server::HttpServerLimits { max_body_bytes: 4, request_timeout_ms: 30_000 };
    let over = exchange(
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
        false,
        limits,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&over), "HTTP/1.1 413 Content Too Large");
    let gzip = exchange(
        b"POST /x HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&gzip), "HTTP/1.1 501 Not Implemented");
    let both = exchange(
        b"POST /x HTTP/1.1\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        false,
        server::HTTP_SERVER_DEFAULT_LIMITS,
        |_, r| panic!("the handler saw {r:?}"),
    );
    assert_eq!(status_line(&both), "HTTP/1.1 400 Bad Request");
}

#[test]
fn header_lines_are_bounded_in_length_and_count() {
    static LONG: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let long = LONG.get_or_init(|| format!("GET /x HTTP/1.1\r\nX-Long: {}\r\n\r\n", "a".repeat(10_000)).into_bytes());
    let resp = exchange(long, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 431 Request Header Fields Too Large");
    static MANY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let many = MANY.get_or_init(|| format!("GET /x HTTP/1.1\r\n{}\r\n", "X-H: 1\r\n".repeat(101)).into_bytes());
    let resp = exchange(many, false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| panic!("the handler saw {r:?}"));
    assert_eq!(status_line(&resp), "HTTP/1.1 431 Request Header Fields Too Large");
}

#[test]
fn a_crlf_header_is_refused_with_500() {
    let resp = exchange(b"GET /x HTTP/1.1\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, _| {
        let evil = [("X-A".to_string(), "1\r\nSet-Cookie: evil=1".to_string())];
        server::http_server_write(conn, 200, &evil, "x").unwrap();
    });
    assert_eq!(status_line(&resp), "HTTP/1.1 500 Internal Server Error");
    assert!(!resp.contains("Set-Cookie"), "{resp:?}");
}

#[test]
fn the_server_skips_an_unparsable_request_and_answers_the_next() {
    let listener = server::http_server_bind(0).expect("bind");
    let port = listener.local_addr().unwrap().port();
    let client = thread::spawn(move || {
        // First connection: a request line with no target — dropped unanswered.
        let mut bad = TcpStream::connect(("127.0.0.1", port)).unwrap();
        bad.write_all(b"GARBAGE\r\n\r\n").unwrap();
        let mut rest = Vec::new();
        let _ = bad.read_to_end(&mut rest);
        assert!(rest.is_empty(), "an unparsable request gets no answer");
        // Second connection: a request with headers and a body.
        let mut ok = TcpStream::connect(("127.0.0.1", port)).unwrap();
        ok.write_all(b"POST /echo?x=1 HTTP/1.1\r\nHost: t\r\nno colon here\r\ncontent-length: 4\r\nX-K:  v \r\n\r\nping").unwrap();
        let mut resp = String::new();
        ok.read_to_string(&mut resp).unwrap();
        resp
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
    assert_eq!(client.join().unwrap(), "HTTP/1.1 404 Not Found\r\nX-R: 1\r\nConnection: close\r\nContent-Length: 4\r\n\r\nnope");
}

#[test]
fn a_request_without_a_body_reads_as_empty_and_a_garbled_length_is_400() {
    let resp = exchange(b"GET / HTTP/1.1\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |conn, (method, target, body, _)| {
        assert_eq!((method.as_str(), target.as_str(), body.as_str()), ("GET", "/", ""));
        server::http_server_write(conn, 200, &[], "").unwrap();
    });
    assert_eq!(resp, "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    let nan = exchange(b"GET / HTTP/1.1\r\nContent-Length: nan\r\n\r\n", false, server::HTTP_SERVER_DEFAULT_LIMITS, |_, r| {
        panic!("the handler saw {r:?}")
    });
    assert_eq!(status_line(&nan), "HTTP/1.1 400 Bad Request");
}
