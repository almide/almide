//! The embedded host's http call table (ops 53..=59) and serve ops (70..=72)
//! driven directly, below the guest: the start-frame parser, the answer
//! encodings and the error answers, against loopback peers only. The
//! end-to-end fixtures that reach these through a guest are release-only
//! and network-bound, so without these the coverage job never runs them.

use crate::host_serve::{self, ServeState, OP_SERVE_BIND, OP_SERVE_NEXT, OP_SERVE_REPLY};
use crate::http_call_host::{by_id, open, HttpCalls};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

fn cell(s: &str) -> String {
    format!("{}\n{}", s.chars().count(), s)
}

fn start_frame(method: &str, body: &str, total: &str, idle: &str, headers: &[(&str, &str)]) -> Vec<u8> {
    let mut f = [method, body, total, idle].iter().map(|s| cell(s)).collect::<String>();
    for (k, v) in headers {
        f.push_str(&cell(k));
        f.push_str(&cell(v));
    }
    f.into_bytes()
}

fn status(ret: i64) -> i64 {
    ret >> 32
}

fn len(ret: i64) -> usize {
    (ret & 0xFFFF_FFFF) as usize
}

fn text(r: (i64, Vec<u8>)) -> (i64, String) {
    assert_eq!(len(r.0), r.1.len(), "the len half matches the payload");
    (status(r.0), String::from_utf8(r.1).unwrap())
}

/// Decode `u32 LE len + bytes` frames.
fn unframe(mut b: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let n = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
        out.push(String::from_utf8(b[4..4 + n].to_vec()).unwrap());
        b = &b[4 + n..];
    }
    out
}

/// One-connection peer that reads the request head, then writes `reply` in
/// the given pieces and closes.
fn peer(pieces: Vec<&'static [u8]>) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/", l.local_addr().unwrap().port());
    let h = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }
        for p in pieces {
            let _ = s.write_all(p);
            let _ = s.flush();
            thread::sleep(Duration::from_millis(30));
        }
    });
    (url, h)
}

#[test]
fn a_malformed_start_frame_is_answered_as_an_error() {
    let calls = Mutex::new(HttpCalls::default());
    let url = "http://127.0.0.1:1/";
    for (frame, want) in [
        (b"GET".to_vec(), "malformed http call frame (missing length)"),
        (b"x\nGET".to_vec(), "malformed http call frame (bad length)"),
        (b"9\nGET".to_vec(), "malformed http call frame (short cell)"),
        (start_frame("GET", "", "soon", "0", &[]), "malformed http call frame (bad limit)"),
        (start_frame("GET", "", "0", "never", &[]), "malformed http call frame (bad limit)"),
        // A header key with no value cell.
        ([start_frame("GET", "", "0", "0", &[]), cell("k").into_bytes()].concat(), "malformed http call frame (missing length)"),
    ] {
        assert_eq!(text(open(&calls, url, &frame)), (1, want.to_string()));
    }
    // A well-formed frame the core refuses: the core's own message.
    let (st, m) = text(open(&calls, url, &start_frame("GET", "", "-1", "0", &[])));
    assert_eq!(st, 1);
    assert!(m.starts_with("invalid limits:"), "{m}");
}

#[test]
fn unknown_ids_and_ops_are_errors_and_drop_is_idempotent() {
    let calls = Mutex::new(HttpCalls::default());
    assert_eq!(text(by_id(&calls, 54, 7)), (1, "unknown http call 7".to_string()));
    assert_eq!(by_id(&calls, 59, 7), (0, Vec::new()), "dropping an unknown id is a no-op");
    let (url, h) = peer(vec![b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"]);
    let (ret, _) = open(&calls, &url, &start_frame("GET", "", "0", "0", &[]));
    assert_eq!(status(ret), 0);
    let id = len(ret) as u32;
    assert_eq!(text(by_id(&calls, 99, id)), (1, "unknown http call op 99".to_string()));
    let _ = by_id(&calls, 55, id);
    assert_eq!(by_id(&calls, 59, id), (0, Vec::new()));
    assert_eq!(status(by_id(&calls, 54, id).0), 1, "gone after drop");
    h.join().unwrap();
}

#[test]
fn a_call_answers_state_wait_read_and_step() {
    let calls = Mutex::new(HttpCalls::default());
    let (url, h) = peer(vec![b"HTTP/1.1 200 OK\r\nX-A: 1\r\nX-A: 2\r\nContent-Length: 5\r\n\r\nh\xC3\xA9", b"ll"]);
    let frame = start_frame("POST", "payload", "5000", "0", &[("X-Req", "é")]);
    let (ret, _) = open(&calls, &url, &frame);
    assert_eq!(status(ret), 0);
    let id = len(ret) as u32;
    // Wait: [status, body, k1, v1, …] in wire order.
    let (st, payload) = by_id(&calls, 55, id);
    assert_eq!(status(st), 0);
    let items = unframe(&payload);
    assert_eq!(items[..2], ["200".to_string(), "héll".to_string()]);
    assert_eq!(items[2..].iter().filter(|s| *s == "X-A").count(), 2);
    assert_eq!(text(by_id(&calls, 54, id)), (0, "1".to_string()));
    assert_eq!(text(by_id(&calls, 56, id)), (0, "héll".to_string()));
    // The call is over: one step answers `o` with an empty cell.
    assert_eq!(text(by_id(&calls, 58, id)), (0, "o0\n".to_string()));
    assert_eq!(text(by_id(&calls, 57, id)), (0, String::new()));
    h.join().unwrap();
}

#[test]
fn a_running_call_steps_c_then_ends_x_on_cancel() {
    let calls = Mutex::new(HttpCalls::default());
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/", l.local_addr().unwrap().port());
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let h = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\npart");
        let _ = rx.recv_timeout(Duration::from_secs(10));
    });
    let (ret, _) = open(&calls, &url, &start_frame("GET", "", "0", "0", &[]));
    let id = len(ret) as u32;
    assert_eq!(text(by_id(&calls, 58, id)), (0, "c4\npart".to_string()));
    assert_eq!(text(by_id(&calls, 54, id)), (0, "0".to_string()));
    let _ = by_id(&calls, 57, id);
    assert_eq!(text(by_id(&calls, 58, id)), (0, "x0\nrequest cancelled".to_string()));
    assert_eq!(text(by_id(&calls, 55, id)), (1, "request cancelled".to_string()));
    let _ = tx.send(());
    h.join().unwrap();
}

#[test]
fn a_live_call_is_cancelled_when_the_table_drops() {
    let calls = Mutex::new(HttpCalls::default());
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/", l.local_addr().unwrap().port());
    let h = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        // The client's cancel shuts the connection: the read ends.
        let mut buf = Vec::new();
        let _ = s.read_to_end(&mut buf);
    });
    let (ret, _) = open(&calls, &url, &start_frame("GET", "", "0", "0", &[]));
    assert_eq!(status(ret), 0);
    thread::sleep(Duration::from_millis(50));
    drop(calls);
    h.join().unwrap();
}

// ── serve ops ──

fn frames(items: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    for s in items {
        b.extend_from_slice(&(s.len() as u32).to_le_bytes());
        b.extend_from_slice(s.as_bytes());
    }
    b
}

type Cells = (String, String, Vec<(String, String)>);

fn parse_cells(a: &str) -> Result<Cells, String> {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = a;
    while !rest.is_empty() {
        let nl = rest.find('\n').ok_or("bad cells")?;
        let n: usize = rest[..nl].parse().map_err(|_| "bad cells")?;
        let tail = &rest[nl + 1..];
        let end = tail.char_indices().nth(n).map(|(i, _)| i).unwrap_or(tail.len());
        parts.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    if parts.len() < 2 {
        return Err("bad cells".to_string());
    }
    let pairs = parts[2..].chunks(2).map(|c| (c[0].clone(), c.get(1).cloned().unwrap_or_default())).collect();
    Ok((parts[0].clone(), parts[1].clone(), pairs))
}

fn serve(st: &Mutex<ServeState>, op: i32, a: &str) -> (i64, Vec<u8>) {
    // No live stdout: the harness never arms shutdown signals.
    host_serve::dispatch(st, None, op, a, frames, parse_cells)
}

#[test]
fn serve_ops_refuse_out_of_order_calls_and_a_bad_port() {
    let st = Mutex::new(ServeState::default());
    assert_eq!(text(serve(&st, OP_SERVE_NEXT, "")), (1, "http.serve: no listener bound".to_string()));
    assert_eq!(text(serve(&st, OP_SERVE_REPLY, "")), (1, "http.serve: no request pending".to_string()));
    let (s, m) = text(serve(&st, OP_SERVE_BIND, "99999"));
    assert_eq!(s, 1);
    assert!(m.starts_with("bind failed: "), "{m}");
}

#[test]
fn serve_ops_bind_take_a_request_and_reply() {
    // Find a free port, then bind it through the op (it binds 0.0.0.0).
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let st = Mutex::new(ServeState::default());
    assert_eq!(serve(&st, OP_SERVE_BIND, &format!(" {port} ")), (0, Vec::new()));
    let client = thread::spawn(move || {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"PUT /x HTTP/1.1\r\nX-K: v\r\nContent-Length: 2\r\n\r\nhi").unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        // Second request: its reply carries unparsable cells.
        let mut s2 = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s2.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        resp
    });
    let (ret, payload) = serve(&st, OP_SERVE_NEXT, "");
    assert_eq!(status(ret), 0);
    assert_eq!(unframe(&payload), ["PUT", "/x", "hi", "X-K", "v", "Content-Length", "2"]);
    let reply = [cell("201"), cell("made"), cell("X-R"), cell("1")].concat();
    assert_eq!(serve(&st, OP_SERVE_REPLY, &reply), (0, Vec::new()));
    assert_eq!(client.join().unwrap(), "HTTP/1.1 201 Created\r\nX-R: 1\r\nContent-Length: 4\r\n\r\nmade");
    let (ret, _) = serve(&st, OP_SERVE_NEXT, "");
    assert_eq!(status(ret), 0);
    assert_eq!(text(serve(&st, OP_SERVE_REPLY, "nonsense")), (1, "bad cells".to_string()));
}
