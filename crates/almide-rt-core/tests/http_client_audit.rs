//! The HTTP client audit (#2828): the shared client core against the
//! behaviour of curl, Go, Python, Node and reqwest, one section per issue.
//!
//! - #2821 the URL: scheme, userinfo, IPv6, `?` without a path, ports,
//!   fragments, unencoded paths, IDN, and a `Host` that carries the port.
//! - #2822 (client half) CR/LF and other controls in header names / values
//!   and the method are refused before anything is sent.
//! - #2820 the trust store: `SSL_CERT_FILE` names a CA generated here, and a
//!   certificate failure reads as `TLS error: …`, not `write failed: …`.
//! - #2819 proxies: `HTTP_PROXY` (absolute-form), `HTTPS_PROXY` (CONNECT),
//!   `ALL_PROXY` (SOCKS5), `NO_PROXY`, and the `http.start` handle.
//! - #2824 chunk extensions and `100 Continue`.
//! - #2825 the connect timeout and the response size cap.
//!
//! Environment-dependent cases run in a CHILD process (this test binary
//! re-executed on `child_request` with the request in `ALMIDE_AUDIT_CHILD`):
//! the proxy and trust variables are read from the process environment, and
//! setting them in-process would race the other tests (and needs `unsafe`,
//! which the workspace forbids). Every peer, proxy and TLS server is a thread
//! on loopback; nothing leaves the host except the connect-timeout probe,
//! which dials a non-routable address on purpose.

use almide_rt_core::http_client_core as client;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ── peers ──

/// Read one request head (through the blank line) off `s`.
fn read_head(s: &TcpStream) -> String {
    let mut r = BufReader::new(s.try_clone().expect("clone"));
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    head
}

/// A one-connection origin on `listener`: capture the request head, answer
/// `reply`, and send the head back over the channel.
fn origin_on(listener: TcpListener, reply: Vec<u8>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut s = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Err(_) => return,
            }
        };
        s.set_nonblocking(false).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let head = read_head(&s);
        let _ = s.write_all(&reply);
        let _ = tx.send(head);
    });
    rx
}

/// An origin on 127.0.0.1: `(port, captured request head)`.
fn origin(reply: &[u8]) -> (u16, mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (port, origin_on(l, reply.to_vec()))
}

const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";

fn get(url: &str) -> Result<client::HttpTextResponse, String> {
    client::request_response("GET", url, "", &[])
}

fn head_of(rx: &mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(10)).expect("the origin was reached")
}

fn request_line(head: &str) -> &str {
    head.lines().next().unwrap_or("")
}

/// A listener nobody should reach: `true` if something connected within
/// `wait`.
fn untouched_listener() -> (u16, impl FnOnce(Duration) -> bool) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    (port, move |wait: Duration| {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if l.accept().is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    })
}

// ── #2821: the URL ──

#[test]
fn the_host_header_carries_a_non_default_port() {
    let (port, rx) = origin(OK);
    let r = get(&format!("http://127.0.0.1:{port}/echo")).unwrap();
    assert_eq!(r.0, 200);
    let head = head_of(&rx);
    assert!(head.contains(&format!("\r\nHost: 127.0.0.1:{port}\r\n")), "{head}");
}

#[test]
fn userinfo_becomes_basic_auth_and_never_the_host() {
    let (port, rx) = origin(OK);
    let r = get(&format!("http://user:p%40ss@127.0.0.1:{port}/echo")).unwrap();
    assert_eq!(r.0, 200);
    let head = head_of(&rx);
    assert_eq!(request_line(&head), "GET /echo HTTP/1.1");
    // base64("user:p@ss")
    assert!(head.contains("\r\nAuthorization: Basic dXNlcjpwQHNz\r\n"), "{head}");
    assert!(!head.contains("user:"), "{head}");
}

#[test]
fn a_callers_authorization_header_wins_over_userinfo() {
    let (port, rx) = origin(OK);
    let hs = vec![("Authorization".to_string(), "Bearer t".to_string())];
    client::request_response("GET", &format!("http://u:p@127.0.0.1:{port}/"), "", &hs).unwrap();
    let head = head_of(&rx);
    assert!(head.contains("Authorization: Bearer t\r\n") && !head.contains("Basic"), "{head}");
}

#[test]
fn an_ipv6_literal_is_dialled_and_bracketed_in_host() {
    let Ok(l) = TcpListener::bind("[::1]:0") else {
        eprintln!("no IPv6 loopback on this host; skipped");
        return;
    };
    let port = l.local_addr().unwrap().port();
    let rx = origin_on(l, OK.to_vec());
    let r = get(&format!("http://[::1]:{port}/echo")).unwrap();
    assert_eq!(r.0, 200);
    assert!(head_of(&rx).contains(&format!("\r\nHost: [::1]:{port}\r\n")));
}

#[test]
fn a_query_without_a_path_keeps_the_port() {
    let (port, rx) = origin(OK);
    let r = get(&format!("http://127.0.0.1:{port}?x=1")).unwrap();
    assert_eq!(r.0, 200);
    assert_eq!(request_line(&head_of(&rx)), "GET /?x=1 HTTP/1.1");
}

#[test]
fn the_fragment_is_never_sent_and_unsafe_bytes_are_percent_encoded() {
    let (port, rx) = origin(OK);
    get(&format!("http://127.0.0.1:{port}/a b/café?q=a b#frag")).unwrap();
    assert_eq!(request_line(&head_of(&rx)), "GET /a%20b/caf%C3%A9?q=a%20b HTTP/1.1");
}

#[test]
fn the_scheme_is_case_insensitive() {
    let (port, rx) = origin(OK);
    let r = get(&format!("HTTP://127.0.0.1:{port}/echo")).unwrap();
    assert_eq!(r.0, 200);
    assert_eq!(request_line(&head_of(&rx)), "GET /echo HTTP/1.1");
}

#[test]
fn bad_urls_are_refused_before_any_connection() {
    let (port, touched) = untouched_listener();
    let cases = [
        ("http://127.0.0.1:99999/echo".to_string(), "invalid port"),
        (format!("http://127.0.0.1:{port}x/echo"), "invalid port"),
        ("http://127.0.0.1:0/".to_string(), "invalid port"),
        (format!("ftp://127.0.0.1:{port}/"), "unsupported scheme \"ftp\""),
        (format!("127.0.0.1:{port}/echo"), "missing scheme"),
        (format!("localhost:{port}/echo"), "missing scheme"),
        ("http:///echo".to_string(), "empty host"),
        ("http://[::1/".to_string(), "IPv6"),
        (format!("http://::1:{port}/"), "IPv6"),
        ("http://exa mple.com/".to_string(), "invalid character"),
    ];
    for (url, want) in &cases {
        let e = get(url).expect_err(url);
        assert!(e.starts_with("invalid URL "), "{url}: {e}");
        assert!(e.contains(want), "{url}: want {want:?} in {e:?}");
        // The call handle refuses the same URL synchronously.
        let e2 = client::http_call_spawn("GET", url, "", vec![], 0, 0).err().expect("start refuses");
        assert_eq!(&e2, &e, "{url}");
    }
    assert!(!touched(Duration::from_millis(200)), "a refused URL still dialled");
}

#[test]
fn an_empty_port_means_the_default_one() {
    let u = client::http_parse_url("https://example.com:/x").unwrap();
    assert_eq!((u.port, u.authority.as_str(), u.target.as_str()), (443, "example.com", "/x"));
}

#[test]
fn an_idn_host_goes_out_as_punycode() {
    let u = client::http_parse_url("https://Bücher.example/").unwrap();
    assert_eq!(u.host, "xn--bcher-kva.example");
    assert_eq!(u.authority, "xn--bcher-kva.example");
    let u = client::http_parse_url("http://例え.テスト:8080/").unwrap();
    assert_eq!(u.authority, "xn--r8jz45g.xn--zckzah:8080");
}

// ── #2824: chunk extensions and 1xx ──

#[test]
fn a_chunk_extension_does_not_empty_the_body() {
    let reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;name=v\r\nhello\r\n6 ; x\r\n world\r\n0\r\n\r\n";
    let (port, _rx) = origin(reply);
    let r = get(&format!("http://127.0.0.1:{port}/")).unwrap();
    assert_eq!((r.0, r.2.as_str()), (200, "hello world"));
    let (port, _rx) = origin(reply);
    assert_eq!(client::request_bytes("GET", &format!("http://127.0.0.1:{port}/"), "", &[]).unwrap(), b"hello world");
    let (port, _rx) = origin(reply);
    let sh = client::http_call_spawn("GET", &format!("http://127.0.0.1:{port}/"), "", vec![], 5000, 0).unwrap();
    let r = client::http_call_wait(&sh).unwrap();
    assert_eq!((r.0, r.2.as_str()), (200, "hello world"));
}

#[test]
fn a_malformed_chunk_size_is_an_error_not_an_empty_body() {
    let reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nhello\r\n0\r\n\r\n";
    let (port, _rx) = origin(reply);
    let e = get(&format!("http://127.0.0.1:{port}/")).unwrap_err();
    assert!(e.contains("malformed chunked body"), "{e}");
    let (port, _rx) = origin(reply);
    let sh = client::http_call_spawn("GET", &format!("http://127.0.0.1:{port}/"), "", vec![], 5000, 0).unwrap();
    let e = client::http_call_wait(&sh).unwrap_err();
    assert!(e.contains("malformed chunked body"), "{e}");
}

#[test]
fn a_100_continue_is_skipped_for_the_final_response() {
    let reply = b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Early Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-Final: 1\r\n\r\nok";
    let (port, _rx) = origin(reply);
    let r = get(&format!("http://127.0.0.1:{port}/")).unwrap();
    assert_eq!((r.0, r.2.as_str()), (200, "ok"));
    assert!(r.1.iter().any(|(k, _)| k == "X-Final") && !r.1.iter().any(|(k, _)| k == "Link"));
    let (port, _rx) = origin(reply);
    assert_eq!(client::request_bytes("GET", &format!("http://127.0.0.1:{port}/"), "", &[]).unwrap(), b"ok");
    let (port, _rx) = origin(reply);
    let sh = client::http_call_spawn("GET", &format!("http://127.0.0.1:{port}/"), "", vec![], 5000, 0).unwrap();
    let r = client::http_call_wait(&sh).unwrap();
    assert_eq!((r.0, r.2.as_str()), (200, "ok"));
}
