//! The driver for a program that serves (`http.serve`): it never returns on
//! its own, so no rung can run it to its exit. The harness is its client
//! instead (ADR-0027 D5 turned around: the fixture is the program, the
//! harness is the peer). One session:
//!
//! 1. start the program at a port the harness chose (socket hosts read it
//!    from [`PORT_ENV`]; `wasmtime serve` is given `--addr`);
//! 2. send the session's fixed request sequence, one request per
//!    connection, each read to the server's close. The first request is the
//!    readiness proof: its connect retries while the program compiles and
//!    binds, and once it is answered `http.serve` has armed its signal
//!    handling (the #2692 shutdown fixture's rule);
//! 3. stop the server with one SIGTERM. A socket host drains, flushes and
//!    returns from `http.serve` (ADR-0020 §5.6), so its exit is part of
//!    what is observed; the stock host's exit is the host's own.
//!
//! The observable is the client-side transcript: one line per response in
//! C-367's terms — status code, the header set (names compared
//! case-insensitively, the order kept within one name) and the de-framed
//! body. The fields a host manages (`date`, `connection`, `keep-alive`,
//! `transfer-encoding`, `content-length`) and the reason phrase are not
//! values C-367 promises; each is printed as a verdict instead (ADR-0027
//! D3): `date` is absent or an IMF-fixdate inside the request's window,
//! the framing fields agree with the bytes received, and the reason phrase
//! is in RFC 9112's grammar. A failed verdict prints what failed, so the
//! comparison stays byte equality and a broken host field is never equal
//! to a sound one.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The env var a served probe reads its port from.
pub const PORT_ENV: &str = "XF_SERVE_PORT";

/// The header line a served probe starts with: `// @xf-serve <plan>`.
pub const MARK: &str = "// @xf-serve ";

/// One request of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Req {
    pub method: &'static str,
    pub target: String,
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: Vec<u8>,
}

impl Req {
    fn new(method: &'static str, target: &str, headers: &[(&'static str, &'static str)], body: &[u8]) -> Req {
        Req { method, target: target.to_string(), headers: headers.to_vec(), body: body.to_vec() }
    }

    /// The request's bytes: HTTP/1.1 with `Connection: close` (`wasmtime
    /// serve` keeps a connection alive otherwise) and a `Content-Length`
    /// for a body.
    pub fn wire(&self) -> Vec<u8> {
        let mut head = format!("{} {} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n", self.method, self.target);
        for (k, v) in &self.headers {
            let _ = write!(head, "{k}: {v}\r\n");
        }
        if !self.body.is_empty() {
            let _ = write!(head, "Content-Length: {}\r\n", self.body.len());
        }
        head.push_str("\r\n");
        let mut raw = head.into_bytes();
        raw.extend_from_slice(&self.body);
        raw
    }
}

/// What a session sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// [`script`]: the request set every generated handler answers.
    Script,
    /// The readiness request, then `k` POSTs, each with an `n`-byte body:
    /// the complexity probe's size is the body a request carries.
    Sized { k: u64, n: u64 },
    /// [`script`], then `k` sized POSTs: the alloc probe.
    ScriptThenSized { k: u64, n: u64 },
}

impl Plan {
    pub fn header(&self) -> String {
        match self {
            Plan::Script => format!("{MARK}script\n"),
            Plan::Sized { k, n } => format!("{MARK}sized k={k} n={n}\n"),
            Plan::ScriptThenSized { k, n } => format!("{MARK}script+sized k={k} n={n}\n"),
        }
    }

    pub fn requests(&self) -> Vec<Req> {
        match self {
            Plan::Script => script(),
            Plan::Sized { k, n } => {
                let mut v = vec![ready()];
                v.extend((0..*k).map(|i| sized(i, *n)));
                v
            }
            Plan::ScriptThenSized { k, n } => {
                let mut v = script();
                v.extend((0..*k).map(|i| sized(i, *n)));
                v
            }
        }
    }
}

/// The plan a served probe's header line names, or None for a program that
/// is not a served probe.
pub fn plan_of(src: &str) -> Option<Plan> {
    let line = src.lines().find_map(|l| l.strip_prefix(MARK))?;
    let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
    let field = |name: &str| rest.split_whitespace().find_map(|w| w.strip_prefix(&format!("{name}="))?.parse::<u64>().ok());
    match kind.trim() {
        "script" => Some(Plan::Script),
        "sized" => Some(Plan::Sized { k: field("k")?, n: field("n")? }),
        "script+sized" => Some(Plan::ScriptThenSized { k: field("k")?, n: field("n")? }),
        _ => None,
    }
}

/// The readiness request (and the first line of every sized session).
fn ready() -> Req {
    Req::new("GET", "/", &[], b"")
}

/// The `i`th sized request: a POST whose body is `n` bytes. The bytes vary
/// with the position so no layer can share one buffer between requests.
fn sized(i: u64, n: u64) -> Req {
    let body: Vec<u8> = (0..n).map(|j| b'a' + ((i + j) % 26) as u8).collect();
    Req::new("POST", &format!("/s/{i}"), &[("content-type", "text/plain")], &body)
}

/// The fixed request set: what C-367's replay and C-375's export scripts
/// send, in one sequence — GET, a percent-encoded query (a pair without
/// `=`, a value holding `=`, `+`), POST with a UTF-8 body and a header the
/// handler may read, PUT with a body that is not UTF-8, DELETE, a JSON
/// body, two headers of one name, and an empty-path target with a query.
pub fn script() -> Vec<Req> {
    vec![
        ready(),
        Req::new("GET", "/hello?x=1&y=%E7%8C%AB&c=x+y&nokey&d=1=2", &[], b""),
        Req::new("POST", "/echo", &[("X-Token", "abc"), ("Content-Type", "text/plain; charset=utf-8")], "日本語 ok".as_bytes()),
        Req::new("PUT", "/a/b", &[], b"a\xffb\xe3\x81c\xf0\x9f\x98\x80"),
        Req::new("DELETE", "/users/42", &[("x-a", "1")], b""),
        Req::new("POST", "/j", &[("content-type", "application/json")], b"[1, \"a\xc3\xa9\", {\"k\": null}]"),
        Req::new("GET", "/p/1?a=1", &[("X-B", "one"), ("X-B", "two")], b""),
        Req::new("GET", "/missing", &[], b""),
    ]
}

/// How the program under test is hosted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// Native or the embedded lane: binds the port it reads from
    /// [`PORT_ENV`], drains on SIGTERM and returns from `http.serve`.
    Socket,
    /// `wasmtime serve` on the export component: the port is its `--addr`;
    /// its exit on SIGTERM is the host's.
    Stock,
}

/// A finished session.
#[derive(Debug)]
pub struct Session {
    /// One line per response ([`transcript_line`]), then, for a socket
    /// host, `exit <code>`.
    pub transcript: String,
    /// The process's own output and status.
    pub output: Output,
    /// Wall time from spawn to exit.
    pub wall: Duration,
}

/// How long a server may take to accept its first connection (the embedded
/// lane compiles first), and to exit after the SIGTERM.
pub const READY_CAP: Duration = Duration::from_secs(180);
pub const STOP_CAP: Duration = Duration::from_secs(30);

/// A port nothing listens on now, below every ephemeral range (Linux
/// 32768.., macOS 49152..): the server binds it only after it starts (the
/// embedded lane compiles first), and meanwhile the readiness connects of
/// this and every parallel session take ephemeral ports, which an
/// ephemeral pick could collide with. Ports are handed out in turn from an
/// offset per process. A race with another process is still a bind
/// failure, which [`drive_fresh`] retries on a new port.
pub fn free_port() -> u16 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let base = std::process::id().wrapping_mul(7919) % 10_000;
    for _ in 0..10_000 {
        let port = (20_000 + (base + NEXT.fetch_add(1, Ordering::Relaxed)) % 10_000) as u16;
        if TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return port;
        }
    }
    TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(0)
}

/// [`drive`] with a fresh port per attempt: `make(port)` builds the command.
/// A server that exits before it answered with `bind failed` lost a port
/// race; that attempt is repeated (up to 3 times), never reported.
pub fn drive_fresh(make: &mut dyn FnMut(u16) -> Command, host: Host, plan: &Plan, wrapped: bool) -> Result<Session, String> {
    let mut last = String::new();
    for _ in 0..3 {
        let port = free_port();
        match drive(&mut make(port), port, host, plan, wrapped) {
            Err(e) if e.contains("bind failed") => last = e,
            r => return r,
        }
    }
    Err(last)
}

/// [`drive_fresh`] for a socket host, whose port is only its env: the one
/// command is spawned again on a new port after a lost race.
pub fn drive_socket(cmd: &mut Command, plan: &Plan, wrapped: bool) -> Result<Session, String> {
    let mut last = String::new();
    for _ in 0..3 {
        let port = free_port();
        match drive(cmd, port, Host::Socket, plan, wrapped) {
            Err(e) if e.contains("bind failed") => last = e,
            r => return r,
        }
    }
    Err(last)
}

/// Where a failed session's message turns from what happened (compared
/// across legs) to the leg's own stderr (not compared: a panic's text is
/// the leg's).
pub const DETAIL: &str = " \u{2014} ";

/// A session's outcome as the legs compare it: the transcript, or the
/// failure up to its stderr detail.
pub fn leg_view(r: &Result<Session, String>) -> String {
    match r {
        Ok(s) => s.transcript.clone(),
        Err(e) => format!("FAILED {}\n", e.split(DETAIL).next().unwrap_or(e)),
    }
}

/// One session (see the module doc). `wrapped`: the spawned process is a
/// launcher whose only child is the server (`perf stat`): the SIGTERM goes
/// to that child, so the launcher sees the server exit and reports.
pub fn drive(cmd: &mut Command, port: u16, host: Host, plan: &Plan, wrapped: bool) -> Result<Session, String> {
    if host == Host::Socket {
        cmd.env(PORT_ENV, port.to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let start = Instant::now();
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("spawn: {e}"))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_t = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let collect = |child: &mut Child, status: std::process::ExitStatus, out_t: std::thread::JoinHandle<Vec<u8>>, err_t: std::thread::JoinHandle<Vec<u8>>| {
        let _ = child;
        Output { status, stdout: out_t.join().unwrap_or_default(), stderr: err_t.join().unwrap_or_default() }
    };

    let mut transcript = String::new();
    for (i, req) in plan.requests().iter().enumerate() {
        let deadline = Instant::now() + if i == 0 { READY_CAP } else { STOP_CAP };
        let t0 = SystemTime::now();
        match ask(port, &req.wire(), deadline, &mut child) {
            Ok(raw) => {
                let t1 = SystemTime::now();
                transcript.push_str(&transcript_line(i, req, &raw, (t0, t1)));
            }
            Err(Asked::Exited(status)) => {
                let out = collect(&mut child, status, out_t, err_t);
                return Err(format!("the server exited before answering request {i} ({} {}): exit {}{DETAIL}{}", req.method, req.target, exit_text(&out.status), first_line(&out.stderr)));
            }
            Err(Asked::Timeout(what)) => {
                kill_group(&mut child);
                let out = collect(&mut child, std::process::ExitStatus::default(), out_t, err_t);
                return Err(format!("request {i} ({} {}): {what}{DETAIL}{}", req.method, req.target, first_line(&out.stderr)));
            }
        }
    }

    // Stop: one SIGTERM to the server.
    let target = if wrapped { child_of(child.id()).unwrap_or(child.id()) } else { child.id() };
    signal(target, Signal::Term);
    let stop_deadline = Instant::now() + STOP_CAP;
    let status = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break Some(s);
        }
        if Instant::now() >= stop_deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let Some(status) = status else {
        kill_group(&mut child);
        let _ = (out_t.join(), err_t.join());
        return Err(format!("still serving {}s after SIGTERM: http.serve did not return (ADR-0020 §5.6)", STOP_CAP.as_secs()));
    };
    let wall = start.elapsed();
    let output = collect(&mut child, status, out_t, err_t);
    if host == Host::Socket {
        let _ = writeln!(transcript, "exit {}", exit_text(&output.status));
    }
    Ok(Session { transcript, output, wall })
}

fn exit_text(s: &std::process::ExitStatus) -> String {
    match s.code() {
        Some(c) => c.to_string(),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                format!("signal {}", s.signal().unwrap_or(0))
            }
            #[cfg(not(unix))]
            "killed".to_string()
        }
    }
}

fn first_line(b: &[u8]) -> String {
    let t = String::from_utf8_lossy(b);
    let mut lines = t.lines().filter(|l| !l.trim().is_empty());
    let first = lines.clone().next().unwrap_or("").to_string();
    lines.find(|l| l.contains("rror")).map(str::to_string).unwrap_or(first).chars().take(200).collect()
}

enum Asked {
    Exited(std::process::ExitStatus),
    Timeout(String),
}

/// One request on a fresh connection, read to the server's close. The
/// connect retries until `deadline` while the server is still starting; a
/// server that exits meanwhile is reported, not waited out.
fn ask(port: u16, raw: &[u8], deadline: Instant, child: &mut Child) -> Result<Vec<u8>, Asked> {
    let mut conn = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => break c,
            Err(e) => {
                if let Ok(Some(status)) = child.try_wait() {
                    return Err(Asked::Exited(status));
                }
                if Instant::now() >= deadline {
                    return Err(Asked::Timeout(format!("the server never accepted on {port}: {e}")));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_secs(1));
    let _ = conn.set_read_timeout(Some(left));
    let _ = conn.set_nodelay(true);
    // A server that answers before the body is all sent (413) may close
    // first: what it answered is still on the socket.
    let _ = conn.write_all(raw);
    let mut out = Vec::new();
    match conn.read_to_end(&mut out) {
        Ok(_) => Ok(out),
        // A reset after a complete answer is still that answer.
        Err(_) if !out.is_empty() => Ok(out),
        Err(e) => {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(Asked::Exited(status));
            }
            Err(Asked::Timeout(format!("no answer: {e}")))
        }
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Term,
    Kill,
}

fn signal(pid: u32, s: Signal) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, match s {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        });
    }
}

/// The spawned process's child (a launcher's server), polled briefly: a
/// launcher that execs nothing has none.
fn child_of(pid: u32) -> Option<u32> {
    for _ in 0..100 {
        let out = Command::new("pgrep").args(["-P", &pid.to_string()]).output().ok()?;
        if let Some(p) = String::from_utf8_lossy(&out.stdout).lines().next().and_then(|l| l.trim().parse().ok()) {
            return Some(p);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// SIGKILL the session's whole process group, then reap the child.
fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    signal(child.id(), Signal::Kill);
    let _ = child.wait();
}

/// A response as C-367 observes it.
#[derive(Debug, PartialEq, Eq)]
pub struct Observed {
    pub version: String,
    pub status: i128,
    /// The status line's code as written.
    status_token: String,
    /// Lowercased name → values in wire order: the order between different
    /// names is dropped, the order within one name kept.
    pub headers: BTreeMap<String, Vec<String>>,
    pub body: Vec<u8>,
    /// The host-managed parts, judged as verdicts ([`Observed::verdicts`]).
    reason: String,
    date: Vec<String>,
    content_length: Vec<String>,
    transfer_encoding: Vec<String>,
    connection: Vec<String>,
    /// The bytes after the head, before de-framing.
    raw_body_len: usize,
    /// The first header line outside the field grammar, if any.
    bad_field: Option<String>,
}

/// Parse one HTTP/1.1 response read to the connection's close.
pub fn observe(raw: &[u8]) -> Result<Observed, String> {
    let mut raw = raw;
    // A `100 Continue` interim head comes before the response proper.
    // A `100` with nothing after it was the final answer, and is read as one.
    while raw.starts_with(b"HTTP/1.1 100 ") {
        let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("an interim head without its end")?;
        if cut + 4 == raw.len() {
            break;
        }
        raw = &raw[cut + 4..];
    }
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("no end of the response head")?;
    let head = std::str::from_utf8(&raw[..cut]).map_err(|e| format!("the head is not UTF-8: {e}"))?;
    let rest = &raw[cut + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default().to_string();
    let status_token = parts.next().unwrap_or_default().to_string();
    // Any integer is read as a value: what a host wrote is compared, and
    // whether it is a status code at all is the `status` verdict.
    let status = status_token.parse::<i128>().map_err(|_| format!("no status code in {status_line:?}"))?;
    let reason = parts.next().unwrap_or_default().to_string();
    let mut o = Observed {
        version,
        status,
        status_token,
        headers: BTreeMap::new(),
        body: Vec::new(),
        reason,
        date: Vec::new(),
        content_length: Vec::new(),
        transfer_encoding: Vec::new(),
        connection: Vec::new(),
        raw_body_len: rest.len(),
        bad_field: None,
    };
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| format!("a header line without a colon: {line:?}"))?;
        // RFC 9110 §5.1 / §5.5: a field name is a token, a field value holds
        // no control byte but HTAB.
        if o.bad_field.is_none() && (!is_token(name) || value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)) {
            o.bad_field = Some(format!("the field {:?}: {:?} is not a token name with a control-free value", name, value));
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches([' ', '\t']).to_string();
        match name.as_str() {
            "date" => o.date.push(value),
            "content-length" => o.content_length.push(value),
            "transfer-encoding" => o.transfer_encoding.push(value),
            "connection" | "keep-alive" => o.connection.push(value),
            _ => o.headers.entry(name).or_default().push(value),
        }
    }
    let chunked = o.transfer_encoding.iter().any(|v| v.to_ascii_lowercase().contains("chunked"));
    // RFC 9110 §6.4.1: a 1xx, 204 or 304 has no content whatever its
    // framing fields say (a 304's Content-Length describes the selected
    // representation, §8.6); the bytes after its head are judged by the
    // framing verdict.
    o.body = if o.bodiless() {
        Vec::new()
    } else if chunked {
        dechunk(rest)?
    } else if let Some(n) = o.content_length.first().and_then(|v| v.parse::<usize>().ok()) {
        // A length the bytes do not meet is the framing verdict's to report.
        rest[..n.min(rest.len())].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(o)
}

fn dechunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n").ok_or("a chunk size line without CRLF")?;
        let size = std::str::from_utf8(&rest[..eol]).map_err(|e| format!("chunk size: {e}"))?;
        let n = usize::from_str_radix(size.split(';').next().unwrap_or_default().trim(), 16).map_err(|e| format!("chunk size {size:?}: {e}"))?;
        rest = &rest[eol + 2..];
        if n == 0 {
            return Ok(body);
        }
        body.extend_from_slice(rest.get(..n).ok_or("a chunk shorter than its size")?);
        rest = rest.get(n..).and_then(|r| r.strip_prefix(b"\r\n")).ok_or("a chunk without its CRLF")?;
    }
}

impl Observed {
    /// A status whose response has no content (RFC 9110 §6.4.1).
    fn bodiless(&self) -> bool {
        self.status == 204 || self.status == 304 || (100..200).contains(&self.status)
    }

    /// The verdicts on the host-managed parts (ADR-0027 D3), each `ok` or
    /// what failed. `window`: the system clock just before the request was
    /// sent and just after its answer was read.
    pub fn verdicts(&self, window: (SystemTime, SystemTime)) -> Vec<(&'static str, Result<(), String>)> {
        let secs = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let (lo, hi) = (secs(window.0) - 1, secs(window.1) + 1);
        let date = match self.date.as_slice() {
            [] => Ok(()),
            [d] => match imf_fixdate(d) {
                Some(t) if (lo..=hi).contains(&t) => Ok(()),
                Some(t) => Err(format!("date {d:?} is outside the request's window ({t} not in {lo}..={hi})")),
                None => Err(format!("date {d:?} is not an IMF-fixdate")),
            },
            many => Err(format!("{} date fields", many.len())),
        };
        let chunked = self.transfer_encoding.iter().any(|v| v.eq_ignore_ascii_case("chunked"));
        let framing = match (self.content_length.as_slice(), self.transfer_encoding.as_slice()) {
            // §6.4.1 / §8.6: nothing follows a bodiless head; a 304 may carry
            // one Content-Length, a 1xx or 204 none, and none carries a
            // Transfer-Encoding.
            (cl, te) if self.bodiless() => match (self.raw_body_len, cl, te) {
                (0, [], []) => Ok(()),
                (0, [n], []) if self.status == 304 && n.parse::<usize>().is_ok() => Ok(()),
                (0, cl, te) => Err(format!("a {} with content-length {cl:?} and transfer-encoding {te:?}", self.status)),
                (extra, _, _) => Err(format!("{extra} bytes followed the head of a {}", self.status)),
            },
            ([], []) => Ok(()),
            ([], _) if chunked && self.transfer_encoding.len() == 1 => Ok(()),
            ([], te) => Err(format!("transfer-encoding {te:?} is not chunked")),
            ([cl], []) => match cl.parse::<usize>() {
                Ok(n) if n == self.body.len() && n == self.raw_body_len => Ok(()),
                Ok(n) => Err(format!("content-length {n} but {} bytes followed the head", self.raw_body_len)),
                Err(_) => Err(format!("content-length {cl:?} is not a length")),
            },
            (cl, te) => Err(format!("content-length {cl:?} with transfer-encoding {te:?}")),
        };
        let connection = if self.connection.iter().all(|v| v.split(',').all(|t| is_token(t.trim()) || t.trim().contains('='))) {
            Ok(())
        } else {
            Err(format!("connection fields {:?} are not token lists", self.connection))
        };
        // RFC 9112 §4: reason-phrase = 1*( HTAB / SP / VCHAR / obs-text ),
        // and it may be empty.
        let reason = if self.reason.bytes().all(|b| b == b'\t' || b == b' ' || (0x21..=0x7e).contains(&b) || b >= 0x80) {
            Ok(())
        } else {
            Err(format!("reason phrase {:?} has a control byte", self.reason))
        };
        // RFC 9112 §4: status-code = 3DIGIT; RFC 9110 §15: values outside
        // 100..=599 are invalid, and a 1xx is interim, never the final answer
        // (the client reads on for one that never comes).
        let status = if self.status_token.len() == 3 && self.status_token.bytes().all(|b| b.is_ascii_digit()) && (200..=599).contains(&self.status) {
            Ok(())
        } else {
            Err(format!("the final status {:?} is not a 3-digit code in 200..=599", self.status_token))
        };
        let fields = self.bad_field.clone().map_or(Ok(()), Err);
        vec![("status", status), ("fields", fields), ("date", date), ("framing", framing), ("connection", connection), ("reason", reason)]
    }
}

fn is_token(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// Seconds since the epoch of an IMF-fixdate (`Sun, 06 Nov 1994 08:49:37
/// GMT`, RFC 9110 §5.6.7), or None for any other form.
pub fn imf_fixdate(s: &str) -> Option<i64> {
    let (wd, rest) = s.split_once(", ")?;
    if !["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(&wd) {
        return None;
    }
    let f: Vec<&str> = rest.split(' ').collect();
    let [day, mon, year, time, "GMT"] = f.as_slice() else { return None };
    let day: i64 = (day.len() == 2).then(|| day.parse().ok()).flatten()?;
    let mon = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|m| m == mon)? as i64 + 1;
    let year: i64 = (year.len() == 4).then(|| year.parse().ok()).flatten()?;
    let t: Vec<i64> = time.split(':').map(|p| if p.len() == 2 { p.parse().ok() } else { None }).collect::<Option<_>>()?;
    let [h, m, sec] = t.as_slice() else { return None };
    if !(1..=31).contains(&day) || *h > 23 || *m > 59 || *sec > 60 {
        return None;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if mon <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mon + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    // The weekday must match the date.
    let wday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    (wday == wd).then_some(days * 86400 + h * 3600 + m * 60 + sec)
}

/// Bytes as text: printable ASCII as is, everything else `\xNN`.
pub fn escape(b: &[u8]) -> String {
    let mut s = String::new();
    for &c in b {
        match c {
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(c as char),
            _ => {
                let _ = write!(s, "\\x{c:02x}");
            }
        }
    }
    s
}

/// One response's transcript line: what C-367 compares as values, then the
/// verdicts on what it leaves to the host. A body longer than 256 bytes is
/// shown as its length and FNV-1a hash.
pub fn transcript_line(i: usize, req: &Req, raw: &[u8], window: (SystemTime, SystemTime)) -> String {
    let what = format!("#{i} {} {}", req.method, escape(req.target.as_bytes()));
    let o = match observe(raw) {
        Ok(o) => o,
        Err(e) => return format!("{what} -> UNPARSEABLE ({e}): {}\n", escape(&raw[..raw.len().min(200)])),
    };
    let headers: Vec<String> = o.headers.iter().map(|(k, vs)| format!("{}: {}", escape(k.as_bytes()), vs.iter().map(|v| escape(v.as_bytes())).collect::<Vec<_>>().join(" | "))).collect();
    let body = if o.body.len() > 256 {
        format!("<{} bytes fnv1a {:016x}>", o.body.len(), o.body.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3)))
    } else {
        escape(&o.body)
    };
    let verdicts: Vec<String> = o.verdicts(window).into_iter().map(|(k, v)| match v {
        Ok(()) => format!("{k} ok"),
        Err(e) => format!("{k} FAILED: {e}"),
    }).collect();
    format!("{what} -> {} {} [{}] body={body} ({})\n", escape(o.version.as_bytes()), o.status, headers.join("; "), verdicts.join(", "))
}

/// The response lines of a transcript, without a socket host's `exit` line:
/// what a stock host is compared on.
pub fn responses(transcript: &str) -> String {
    transcript.lines().filter(|l| l.starts_with('#')).map(|l| format!("{l}\n")).collect()
}

/// Every verdict in the transcript held.
pub fn verdicts_hold(transcript: &str) -> bool {
    !transcript.contains(" FAILED: ") && !transcript.contains("-> UNPARSEABLE")
}

/// Where two transcripts first differ, as a signature for de-duplicating
/// findings: the request and the part (`status`, `headers`, `body`,
/// `verdicts`, `exit`, or `length`) that differs.
pub fn first_difference(a: &str, b: &str) -> Option<String> {
    let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    for (x, y) in la.iter().zip(&lb) {
        if x == y {
            continue;
        }
        let req = x.split(" -> ").next().unwrap_or(x).split(' ').take(2).collect::<Vec<_>>().join(" ");
        if x.starts_with("exit") || y.starts_with("exit") {
            return Some("exit".into());
        }
        let part = |l: &str| -> [String; 4] {
            let (_, r) = l.split_once(" -> ").unwrap_or(("", l));
            let status = r.split(" [").next().unwrap_or("").to_string();
            let headers = r.split_once(" [").and_then(|(_, h)| h.split_once("] body=")).map(|(h, _)| h.to_string()).unwrap_or_default();
            let (body, verdicts) = r.split_once("] body=").map(|(_, b)| b.rsplit_once(" (").map(|(b, v)| (b.to_string(), v.to_string())).unwrap_or((b.to_string(), String::new()))).unwrap_or_default();
            [status, headers, body, verdicts]
        };
        let (px, py) = (part(x), part(y));
        let which = ["status", "headers", "body", "verdicts"].iter().zip(px.iter().zip(py.iter())).find(|(_, (p, q))| p != q).map(|(w, _)| *w).unwrap_or("line");
        return Some(format!("{req} {which}"));
    }
    (la.len() != lb.len()).then(|| "length".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_round_trips_through_its_header_line() {
        for p in [Plan::Script, Plan::Sized { k: 16, n: 4096 }, Plan::ScriptThenSized { k: 3, n: 7 }] {
            let src = format!("{}import http\n", p.header());
            assert_eq!(plan_of(&src), Some(p));
        }
        assert_eq!(plan_of("import http\n"), None);
        assert_eq!(plan_of("// @xf-serve sized k=1\n"), None, "a sized plan names both k and n");
    }

    #[test]
    fn a_sized_session_is_the_readiness_request_then_k_bodies_of_n_bytes() {
        let reqs = Plan::Sized { k: 3, n: 5 }.requests();
        assert_eq!(reqs.len(), 4);
        assert_eq!(reqs[0], ready());
        assert!(reqs[1..].iter().all(|r| r.method == "POST" && r.body.len() == 5));
        assert_ne!(reqs[1].body, reqs[2].body, "each request carries its own bytes");
        let wire = String::from_utf8(reqs[1].wire()).unwrap();
        assert!(wire.starts_with("POST /s/0 HTTP/1.1\r\nHost: t\r\nConnection: close\r\n"), "{wire}");
        assert!(wire.ends_with("Content-Length: 5\r\n\r\nabcde"), "{wire}");
        assert!(!String::from_utf8(ready().wire()).unwrap().contains("Content-Length"), "an empty body sends no length");
    }

    fn window() -> (SystemTime, SystemTime) {
        // 2026-10-17T20:38:29Z ± 0.
        let t = UNIX_EPOCH + Duration::from_secs(1_792_269_509);
        (t, t)
    }

    #[test]
    fn imf_fixdate_parses_the_rfc_example_and_refuses_other_forms() {
        assert_eq!(imf_fixdate("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(imf_fixdate("Sat, 17 Oct 2026 20:38:29 GMT"), Some(1_792_269_509));
        assert_eq!(imf_fixdate("Mon, 06 Nov 1994 08:49:37 GMT"), None, "the weekday must match the date");
        assert_eq!(imf_fixdate("Sunday, 06-Nov-94 08:49:37 GMT"), None, "the obsolete RFC 850 form");
        assert_eq!(imf_fixdate("Sun Nov  6 08:49:37 1994"), None, "the asctime form");
        assert_eq!(imf_fixdate("Sun, 06 Nov 1994 08:49:37 UTC"), None);
    }

    #[test]
    fn the_header_set_drops_host_fields_and_case_and_keeps_order_within_a_name() {
        let raw = b"HTTP/1.1 418 I'm a teapot\r\nX-B: 1\r\nx-a: v\r\nx-b: 2\r\ndate: Sat, 17 Oct 2026 20:38:29 GMT\r\ncontent-length: 2\r\n\r\nok";
        let o = observe(raw).unwrap();
        assert_eq!(o.status, 418);
        assert_eq!(o.headers.get("x-b"), Some(&vec!["1".to_string(), "2".to_string()]));
        assert_eq!(o.headers.keys().collect::<Vec<_>>(), ["x-a", "x-b"]);
        assert_eq!(o.body, b"ok");
        let req = ready();
        let line = transcript_line(0, &req, raw, window());
        assert_eq!(line, "#0 GET / -> HTTP/1.1 418 [x-a: v; x-b: 1 | 2] body=ok (status ok, fields ok, date ok, framing ok, connection ok, reason ok)\n");
        // The same answer from a host that writes no date, frames by chunks
        // and has its own reason phrase is the same line.
        let other = b"HTTP/1.1 418 OK\r\nx-a: v\r\nTransfer-Encoding: chunked\r\nX-B: 1\r\nX-B: 2\r\nConnection: close\r\n\r\n1\r\no\r\n1\r\nk\r\n0\r\n\r\n";
        assert_eq!(transcript_line(0, &req, other, window()), line);
    }

    #[test]
    fn a_host_field_that_breaks_its_verdict_is_never_equal_to_a_sound_one() {
        let req = ready();
        let sound = transcript_line(0, &req, b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok", window());
        for broken in [
            &b"HTTP/1.1 200 OK\r\ndate: Sun, 06 Nov 1994 08:49:37 GMT\r\ncontent-length: 2\r\n\r\nok"[..],
            b"HTTP/1.1 200 OK\r\ndate: yesterday\r\ncontent-length: 2\r\n\r\nok",
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nokEXTRA",
            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\ntransfer-encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
            b"HTTP/1.1 200 O\x01K\r\ncontent-length: 2\r\n\r\nok",
            b"HTTP/1.1 200 OK\r\nx-a: a\x01b\r\ncontent-length: 2\r\n\r\nok",
            b"HTTP/1.1 200 OK\r\nx a: v\r\ncontent-length: 2\r\n\r\nok",
            b"HTTP/1.1 40 OK\r\ncontent-length: 2\r\n\r\nok",
        ] {
            let line = transcript_line(0, &req, broken, window());
            assert_ne!(line, sound);
            assert!(!verdicts_hold(&line), "{line}");
        }
        assert!(verdicts_hold(&sound));
    }

    #[test]
    fn a_final_status_outside_200_to_599_or_not_three_digits_fails_its_verdict() {
        let req = ready();
        let line = |code: &str| transcript_line(0, &req, format!("HTTP/1.1 {code} \r\ncontent-length: 0\r\n\r\n").as_bytes(), window());
        for ok in ["200", "418", "599", "299"] {
            assert!(verdicts_hold(&line(ok)), "{}", line(ok));
        }
        for bad in ["40", "127", "103", "1000", "-1", "-9223372036854775808", "2147483647", "600"] {
            let l = line(bad);
            assert!(!verdicts_hold(&l) && l.contains(&format!("status FAILED: the final status \"{bad}\"")), "{l}");
        }
        // A `100` alone is the final answer, not an interim head to skip.
        assert!(line("100").contains("-> HTTP/1.1 100 [] body="), "{}", line("100"));
        // An interim 100 before the final answer is skipped.
        let both = transcript_line(0, &req, b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok", window());
        assert!(both.contains("-> HTTP/1.1 200 [] body=ok (status ok"), "{both}");
    }

    #[test]
    fn a_bodiless_status_has_no_content_whatever_its_framing_fields_say() {
        let req = ready();
        let l = |raw: &[u8]| transcript_line(0, &req, raw, window());
        // A 304 may name the representation's length; nothing follows it.
        assert!(verdicts_hold(&l(b"HTTP/1.1 304 Not Modified\r\nContent-Length: 2\r\n\r\n")));
        assert!(verdicts_hold(&l(b"HTTP/1.1 204 No Content\r\n\r\n")));
        assert_eq!(l(b"HTTP/1.1 304 Not Modified\r\nContent-Length: 2\r\n\r\n"), l(b"HTTP/1.1 304 X\r\n\r\n"));
        for bad in [&b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n"[..], b"HTTP/1.1 304 N\r\n\r\nok", b"HTTP/1.1 204 N\r\ntransfer-encoding: chunked\r\n\r\n"] {
            assert!(l(bad).contains("framing FAILED"), "{}", l(bad));
        }
    }

    #[test]
    fn the_first_difference_names_the_request_and_the_part() {
        let req = ready();
        let a = transcript_line(0, &req, b"HTTP/1.1 200 OK\r\nx: 1\r\ncontent-length: 2\r\n\r\nok", window());
        let b = transcript_line(0, &req, b"HTTP/1.1 200 OK\r\nx: 2\r\ncontent-length: 2\r\n\r\nok", window());
        let c = transcript_line(0, &req, b"HTTP/1.1 500 OK\r\nx: 1\r\ncontent-length: 2\r\n\r\nok", window());
        let d = transcript_line(0, &req, b"HTTP/1.1 200 OK\r\nx: 1\r\ncontent-length: 2\r\n\r\nno", window());
        assert_eq!(first_difference(&a, &a), None);
        assert_eq!(first_difference(&a, &b).as_deref(), Some("#0 GET headers"));
        assert_eq!(first_difference(&a, &c).as_deref(), Some("#0 GET status"));
        assert_eq!(first_difference(&a, &d).as_deref(), Some("#0 GET body"));
        assert_eq!(first_difference(&format!("{a}exit 0\n"), &format!("{a}exit 1\n")).as_deref(), Some("exit"));
        assert_eq!(first_difference(&a, &format!("{a}exit 0\n")).as_deref(), Some("length"));
        assert_eq!(responses(&format!("{a}exit 0\n")), a);
    }

    /// The driver against a stand-in server: a `sh` loop is not portable
    /// enough, so the stand-in is this test binary's own thread serving the
    /// port, and the "server process" is `sleep`, stopped by the SIGTERM.
    /// Covers: the readiness retry (the listener binds late), one connection
    /// per request read to close, the transcript, and the stop.
    #[test]
    fn a_session_retries_until_ready_sends_each_request_and_stops_the_server() {
        // The listener binds late, as a server that compiles first does.
        let port = free_port();
        assert!((20_000..30_000).contains(&port), "{port}: below the ephemeral ranges");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen2 = seen.clone();
        let n = Plan::Script.requests().len();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let l = TcpListener::bind(("127.0.0.1", port)).unwrap();
            for _ in 0..n {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read the head (and whatever body came with it).
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let k = s.read(&mut chunk).unwrap();
                    if k == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..k]);
                }
                let line = String::from_utf8_lossy(&buf).lines().next().unwrap_or("").to_string();
                seen2.lock().unwrap().push(line.clone());
                let body = line.split(' ').nth(1).unwrap_or("").to_string();
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        let mut cmd = Command::new("sleep");
        cmd.arg("600");
        let s = drive(&mut cmd, port, Host::Socket, &Plan::Script, false).expect("session");
        let sent = seen.lock().unwrap().clone();
        assert_eq!(sent.len(), n);
        assert_eq!(sent[0], "GET / HTTP/1.1");
        let lines: Vec<&str> = s.transcript.lines().collect();
        assert_eq!(lines.len(), n + 1);
        assert_eq!(lines[0], "#0 GET / -> HTTP/1.1 200 [] body=/ (status ok, fields ok, date ok, framing ok, connection ok, reason ok)");
        // `sleep` dies of the SIGTERM: its exit is the signal, which a
        // served program's would not be (a socket host returns from serve).
        assert_eq!(*lines.last().unwrap(), "exit signal 15");
    }

    /// Under a launcher (`perf stat -- server`) the SIGTERM goes to the
    /// launcher's child, and the launcher exits on its own when it ends: a
    /// `sh` that runs `sleep` as a child and then `exit 7` stands in for it.
    #[test]
    fn a_wrapped_session_signals_the_launchers_child_and_the_launcher_reports() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
        });
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 600; exit 7"]);
        let s = drive(&mut cmd, port, Host::Socket, &Plan::Sized { k: 0, n: 0 }, true).expect("session");
        assert!(s.transcript.ends_with("exit 7\n"), "{}", s.transcript);
    }

    #[test]
    fn a_server_that_exits_before_answering_is_reported_with_its_exit() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo 'Error: bind failed: in use' >&2; exit 1"]);
        let e = drive(&mut cmd, free_port(), Host::Socket, &Plan::Script, false).unwrap_err();
        assert!(e.contains("exited before answering request 0") && e.contains("exit 1") && e.contains("bind failed"), "{e}");
        assert_eq!(leg_view(&Err(e)), "FAILED the server exited before answering request 0 (GET /): exit 1\n");
    }
}
