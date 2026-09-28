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
