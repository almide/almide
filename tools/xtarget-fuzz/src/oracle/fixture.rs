//! The network a run sees (ADR-0027 D5): TCP echo, sink and source servers and a small
//! HTTP/1.1 server on 127.0.0.1, started once per harness process and handed
//! to every run as `XF_TCP_PORT` / `XF_SINK_PORT` / `XF_SOURCE_PORT` / `XF_HTTP`
//! (the same on every leg), and a Unix-domain sink (`XF_UNIX_SINK`, #3620): a
//! listener at a short absolute path (`sun_path` holds at most 104 bytes on
//! macOS) that accepts every connection and drains it, so a `unix.write_fds`
//! of any size never backs up. No run reaches any other host.
//!
//! HTTP routes (every answer has a Content-Length unless chunked):
//! - `/echo` — 200, body `<METHOD> <path> <request body>`
//! - `/status/<code>` — that status, body `s<code>`
//! - `/bytes/<n>` — 200, n bytes `i % 251`
//! - `/chunked/<n>` — 200, chunked, n chunks `c<i>\n`, i zero-padded to 7 digits so
//!   the body grows exactly with n (an unpadded index adds log n bytes per chunk)
//! - `/sse/<n>` — 200, `text/event-stream`, n OpenAI-shaped delta events then
//!   `data: [DONE]`
//! - anything else — 404, body `missing`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::OnceLock;

/// The env every run gets: `(name, value)`.
pub fn env() -> &'static [(String, String)] {
    static ENV: OnceLock<Vec<(String, String)>> = OnceLock::new();
    ENV.get_or_init(|| {
        let tcp = spawn_server(echo);
        let sink = spawn_server(sink);
        let source = spawn_server(source);
        let http = spawn_server(serve_http);
        vec![
            ("XF_TCP_PORT".into(), tcp.to_string()),
            ("XF_SINK_PORT".into(), sink.to_string()),
            ("XF_SOURCE_PORT".into(), source.to_string()),
            ("XF_HTTP".into(), format!("127.0.0.1:{http}")),
            ("XF_UNIX_SINK".into(), spawn_unix_sink()),
        ]
    })
}

fn spawn_server(handle: fn(TcpStream)) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("fixture listen");
    let port = l.local_addr().expect("fixture addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || handle(s));
        }
    });
    port
}

/// The Unix-domain sink's path, listening once this returns. One per
/// harness process (the pid names it); a leftover from an earlier process
/// with the same pid is removed first.
#[cfg(unix)]
fn spawn_unix_sink() -> String {
    let path = format!("/tmp/xf-{}-sink.sock", std::process::id());
    let _ = std::fs::remove_file(&path);
    let l = std::os::unix::net::UnixListener::bind(&path).expect("unix sink listen");
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s = s;
                let mut buf = [0u8; 65536];
                while matches!(s.read(&mut buf), Ok(n) if n > 0) {}
            });
        }
    });
    path
}

#[cfg(not(unix))]
fn spawn_unix_sink() -> String {
    String::new()
}

fn echo(mut s: TcpStream) {
    let mut buf = [0u8; 65536];
    while let Ok(n) = s.read(&mut buf) {
        if n == 0 || s.write_all(&buf[..n]).is_err() {
            return;
        }
    }
}

/// Reads and drops everything: a writer's probe never backs up.
fn sink(mut s: TcpStream) {
    let mut buf = [0u8; 65536];
    while matches!(s.read(&mut buf), Ok(n) if n > 0) {}
}

/// Writes `a` until the peer closes: a reader's probe never waits.
fn source(mut s: TcpStream) {
    let buf = [b'a'; 65536];
    while s.write_all(&buf).is_ok() {}
}

fn serve_http(s: TcpStream) {
    let mut r = BufReader::new(match s.try_clone() {
        Ok(c) => c,
        Err(_) => return,
    });
    let mut w = s;
    // Keep-alive: answer requests until the client closes.
    while let Some((method, path, body, close)) = read_request(&mut r) {
        if respond(&mut w, &method, &path, &body).is_err() || close {
            return;
        }
    }
}

/// One request: method, path, body, and whether the client asked to close.
fn read_request(r: &mut impl BufRead) -> Option<(String, String, Vec<u8>, bool)> {
    let mut line = String::new();
    if r.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut len = 0usize;
    let mut close = false;
    loop {
        let mut h = String::new();
        r.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
            close |= k.eq_ignore_ascii_case("connection") && v.trim().eq_ignore_ascii_case("close");
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some((method, path, body, close))
}

fn respond(w: &mut impl Write, method: &str, path: &str, body: &[u8]) -> std::io::Result<()> {
    let num = |p: &str| path.strip_prefix(p).and_then(|n| n.parse::<usize>().ok());
    if path == "/echo" {
        let mut out = format!("{method} {path} ").into_bytes();
        out.extend_from_slice(body);
        return whole(w, 200, "text/plain", &out);
    }
    if let Some(code) = num("/status/") {
        return whole(w, code as u16, "text/plain", format!("s{code}").as_bytes());
    }
    if let Some(n) = num("/bytes/") {
        return whole(
            w,
            200,
            "application/octet-stream",
            &(0..n).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
        );
    }
    if let Some(n) = num("/chunked/") {
        return chunked(w, "text/plain", (0..n).map(|i| format!("c{i:07}\n")));
    }
    // OpenAI-compatible base URL `/sse/<n>`: `<base>/chat/completions`.
    let path = path.strip_suffix("/chat/completions").unwrap_or(path);
    let num = |p: &str| path.strip_prefix(p).and_then(|n| n.parse::<usize>().ok());
    if let Some(n) = num("/sse/") {
        let events = (0..n)
            .map(|i| format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"t{i} \"}}}}]}}\n\n"))
            .chain(std::iter::once("data: [DONE]\n\n".to_string()));
        return chunked(w, "text/event-stream", events);
    }
    whole(w, 404, "text/plain", b"missing")
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

fn whole(w: &mut impl Write, code: u16, ty: &str, body: &[u8]) -> std::io::Result<()> {
    write!(
        w,
        "HTTP/1.1 {code} {}\r\nContent-Type: {ty}\r\nContent-Length: {}\r\n\r\n",
        reason(code),
        body.len()
    )?;
    w.write_all(body)?;
    w.flush()
}

/// The whole chunked response in ONE write: `w` is the bare socket, and a
/// write per chunk made the arrival pattern (how many reads, how many
/// deliveries) the scheduler's — the inspection then measured the server's
/// pacing, not the client's work.
fn chunked(
    w: &mut impl Write,
    ty: &str,
    parts: impl Iterator<Item = String>,
) -> std::io::Result<()> {
    let mut out =
        format!("HTTP/1.1 200 OK\r\nContent-Type: {ty}\r\nTransfer-Encoding: chunked\r\n\r\n")
            .into_bytes();
    for p in parts {
        out.extend_from_slice(format!("{:x}\r\n{p}\r\n", p.len()).as_bytes());
    }
    out.extend_from_slice(b"0\r\n\r\n");
    w.write_all(&out)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixtures_echo_and_answer_their_routes() {
        let env = env();
        let port: u16 = env
            .iter()
            .find(|(k, _)| k == "XF_TCP_PORT")
            .unwrap()
            .1
            .parse()
            .unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"ping").unwrap();
        let mut got = [0u8; 4];
        s.read_exact(&mut got).unwrap();
        assert_eq!(&got, b"ping");
        let http = &env.iter().find(|(k, _)| k == "XF_HTTP").unwrap().1;
        let mut h = TcpStream::connect(http.as_str()).unwrap();
        h.write_all(
            b"POST /echo HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi",
        )
        .unwrap();
        let mut resp = String::new();
        let _ = h.read_to_string(&mut resp);
        assert!(resp.starts_with("HTTP/1.1 200 OK"), "{resp}");
        assert!(resp.ends_with("POST /echo hi"), "{resp}");
    }
}
