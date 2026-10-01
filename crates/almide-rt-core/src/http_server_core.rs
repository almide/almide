// The HTTP server core (#2650) — ONE definition of how `http.serve` binds,
// reads a request and writes a response, shared verbatim between the splice
// template (runtime/rs/src/http.rs `include!`s this file; the embed resolver
// inlines it) and the embedded wasm host (almide-wasm-run links it and serves
// the guest's serve ops 70..=72 with these functions). Requests and responses
// travel as plain tuples — the AlmideHttp{Request,Response} wrappers live in
// http.rs, the guest's List[String] reps in stdlib/http_serve.almd.
//
// EVERY byte written here is a cross-lane observable (C-367): the status
// line and its reason table, the header order, the Content-Length line, the
// close after one response, the limits' answers (413 / 400 / 503, #2823).
// Change it here and both lanes change together.

// Splice discipline (see http_client_core.rs): NO `use` lines — every std
// path is written fully qualified, so this text imports nothing that could
// collide with http.rs's or the client core's imports in the flat module.

/// A parsed request: (method, target, body, headers in wire order).
pub type HttpServerRequest = (String, String, String, Vec<(String, String)>);

/// Bind the listener `http.serve(port, _)` accepts on: every interface
/// (`0.0.0.0`), as C-367 states. A loopback-only default would break every
/// deployed server that is reached from outside its host, so the default
/// stays; choosing the address is an explicit option that rides on
/// `serve_with_limits` (ADR-0020 §5.7, #2826).
pub fn http_server_bind(port: i64) -> Result<std::net::TcpListener, String> {
    std::net::TcpListener::bind(format!("0.0.0.0:{}", port)).map_err(|e| format!("bind failed: {}", e))
}

/// The server's limits (ADR-0020 §5.7). `http.serve` runs with
/// [`HTTP_SERVER_DEFAULT_LIMITS`]; `serve_with_limits` will pass its record's
/// values here.
#[derive(Clone, Copy, Debug)]
pub struct HttpServerLimits {
    /// A larger body (declared or chunked) is `413`, and the handler is not
    /// called (#2823).
    pub max_body_bytes: usize,
    /// From accept: a request not read by then is `503`; a handler that
    /// answers after it has its response replaced by `503`.
    pub request_timeout_ms: u64,
}

/// `http.serve`'s limits: 1 MiB of body, a 30 s request timeout (ADR-0020 §5.7).
pub const HTTP_SERVER_DEFAULT_LIMITS: HttpServerLimits = HttpServerLimits { max_body_bytes: 1 << 20, request_timeout_ms: HTTP_SERVER_DRAIN_MS };

/// The longest request line and header line (`414` / `431` beyond), and the
/// most header lines (`431` beyond).
const HTTP_SERVER_MAX_LINE: usize = 8 * 1024;
const HTTP_SERVER_MAX_HEADERS: usize = 100;

/// A connection whose request has been read: the response goes back on it.
/// It remembers what shapes that response — a HEAD gets no body (#2826) —
/// and when the request arrived, for the request timeout.
pub struct HttpServerConn {
    stream: std::net::TcpStream,
    head: bool,
    deadline: std::time::Instant,
}

/// Why a request was not handed to the handler: a connection closed or
/// silent before its first byte is dropped unanswered; anything else is
/// answered by the core with a status and the connection closes.
enum HttpServerReject {
    Drop,
    Status(i64),
}

/// The next request: accept, then parse. A failed accept or an unparsable
/// request line drops that connection unanswered and waits for the next one;
/// a request the limits refuse (#2823) is answered here — `413`, `400`,
/// `503`, … — and the handler never sees it.
/// `None` once a shutdown signal has arrived (ADR-0020 §5.6): the server
/// stops accepting, and a connection accepted after the signal — the
/// watcher's wake-up or a late client — is closed unanswered.
pub fn http_server_next(listener: &std::net::TcpListener) -> Option<(HttpServerConn, HttpServerRequest)> {
    http_server_next_with(listener, &HTTP_SERVER_DEFAULT_LIMITS)
}

/// [`http_server_next`] under explicit limits.
pub fn http_server_next_with(listener: &std::net::TcpListener, limits: &HttpServerLimits) -> Option<(HttpServerConn, HttpServerRequest)> {
    loop {
        if http_server_stopping() {
            return None;
        }
        let accepted = listener.accept();
        if http_server_stopping() {
            return None;
        }
        let stream = match accepted {
            Ok((s, _)) => s,
            Err(_) => continue,
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(limits.request_timeout_ms);
        let mut conn = HttpServerConn { stream, head: false, deadline };
        match http_server_read_request(&conn.stream, deadline, limits.max_body_bytes) {
            Ok(req) => {
                conn.head = req.0 == "HEAD";
                return Some((conn, req));
            }
            Err(HttpServerReject::Drop) => {}
            Err(HttpServerReject::Status(status)) => {
                let headers = [("Content-Type".to_string(), "text/plain".to_string())];
                let _ = http_server_send(conn, status, &headers, http_server_reason(status));
            }
        }
    }
}

// ── Shutdown (ADR-0020 §5.6, #2692, C-367) ──
//
// On the first SIGTERM or SIGINT (Ctrl-C / Ctrl-Break on Windows) a socket
// host stops accepting, lets the request in flight finish, flushes stdout and
// returns from `http.serve`. A second signal, or a drain that outlasts the
// request timeout, flushes and exits 1 — never a 128+signal code (C-350).
//
// This core holds the part both lanes share and that needs no `unsafe`: the
// signal count, the accept loop's stop test (above), and the WATCHER thread.
// Installing the handler is each host's own: the native runtime calls the C
// `signal` / `SetConsoleCtrlHandler` directly (runtime/rs/src/http.rs), the
// embedded host uses the `ctrlc` crate (almide-wasm-run/src/host_serve.rs).
// Either handler only calls `http_server_signal`, which is async-signal-safe
// (one atomic add).
//
// The serve loop is sequential today, so "in flight" is at most the one
// request the serving thread is handling; the watcher never touches it. When
// the worker pool lands (§5.5, #2665) the drain waits for every worker's
// request instead, and the forced path gets a process-global stdout buffer to
// flush from any thread.

/// Shutdown signals received since the server armed (see
/// [`http_server_watch`]).
pub static HTTP_SERVER_SIGNALS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The drain bound: `request_timeout_ms`'s default (ADR-0020 §5.7). A
/// request still in flight this long after the first signal is abandoned and
/// the run ends as a forced stop.
pub const HTTP_SERVER_DRAIN_MS: u64 = 30_000;

/// Count one shutdown signal; answers the count so far. The one thing a
/// signal handler does, so it stays async-signal-safe.
pub fn http_server_signal() -> usize {
    HTTP_SERVER_SIGNALS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

/// A first signal arrived: stop accepting and drain.
pub fn http_server_stopping() -> bool {
    HTTP_SERVER_SIGNALS.load(std::sync::atomic::Ordering::SeqCst) >= 1
}

/// A second signal arrived: skip the drain, flush and exit 1.
pub fn http_server_forced() -> bool {
    HTTP_SERVER_SIGNALS.load(std::sync::atomic::Ordering::SeqCst) >= 2
}

/// The watcher of one `http.serve` run; dropping it stops the thread.
pub struct HttpServerWatch {
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for HttpServerWatch {
    fn drop(&mut self) {
        self.done.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Arm shutdown for `listener`: reset the count and start the watcher. On the
/// first signal the watcher wakes the blocking `accept` with a loopback
/// connection (a blocking accept restarts after a signal), so the serve loop
/// sees [`http_server_stopping`]. On a second signal, or once the drain has
/// run [`HTTP_SERVER_DRAIN_MS`], it calls `force`, every tick until the
/// process ends: `force` flushes stdout and exits 1, or — natively, where the
/// buffer belongs to the serving thread — makes that thread do so.
pub fn http_server_watch(listener: &std::net::TcpListener, force: Box<dyn Fn() + Send>) -> HttpServerWatch {
    HTTP_SERVER_SIGNALS.store(0, std::sync::atomic::Ordering::SeqCst);
    let port = listener.local_addr().map(|a| a.port()).ok();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = done.clone();
    let thread = std::thread::Builder::new()
        .name("almide-serve-watch".into())
        .spawn(move || {
            let mut drain_from: Option<std::time::Instant> = None;
            while !seen.load(std::sync::atomic::Ordering::SeqCst) {
                if http_server_stopping() {
                    let from = *drain_from.get_or_insert_with(|| {
                        if let Some(p) = port {
                            let wake = std::net::SocketAddr::from(([127, 0, 0, 1], p));
                            let _ = std::net::TcpStream::connect_timeout(&wake, std::time::Duration::from_secs(1));
                        }
                        std::time::Instant::now()
                    });
                    if http_server_forced() || from.elapsed() >= std::time::Duration::from_millis(HTTP_SERVER_DRAIN_MS) {
                        force();
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        })
        .ok();
    HttpServerWatch { done, thread }
}

// ── Reading a request (#2823) ──
//
// Every read is bounded: by the request's deadline (the socket's read timeout
// is set to what is left before each read, so a client trickling bytes cannot
// stretch it), by the line length, the header count and the body limit. The
// body is allocated only up to the limit, never from the declared length.

fn http_server_timeout(deadline: std::time::Instant) -> Option<std::time::Duration> {
    let left = deadline.saturating_duration_since(std::time::Instant::now());
    (!left.is_zero()).then_some(left)
}

/// Buffered bytes, refilled under the deadline. `Ok(&[])` is the peer's EOF.
fn http_server_fill<'a>(
    reader: &'a mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
    started: bool,
) -> Result<&'a [u8], HttpServerReject> {
    // Nothing read yet: a silent or vanished connection is dropped. Past the
    // first byte, running out of time is the request timeout's 503.
    let late = if started { HttpServerReject::Status(503) } else { HttpServerReject::Drop };
    let Some(left) = http_server_timeout(deadline) else {
        return Err(late);
    };
    if reader.get_ref().set_read_timeout(Some(left)).is_err() {
        return Err(HttpServerReject::Drop);
    }
    match std::io::BufRead::fill_buf(reader) {
        Ok(b) => Ok(b),
        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Err(late),
        Err(_) => Err(HttpServerReject::Drop),
    }
}

/// One line without its line ending; `too_long` answers a line over the limit.
fn http_server_line(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
    started: bool,
    too_long: HttpServerReject,
) -> Result<String, HttpServerReject> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let buf = http_server_fill(reader, deadline, started || !line.is_empty())?;
        if buf.is_empty() {
            // EOF: before the first byte the peer just left; mid-request the
            // request is cut short.
            return Err(if started || !line.is_empty() { HttpServerReject::Status(400) } else { HttpServerReject::Drop });
        }
        let (take, done) = match buf.iter().position(|&b| b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        line.extend_from_slice(&buf[..take]);
        std::io::BufRead::consume(reader, take);
        if line.len() > HTTP_SERVER_MAX_LINE {
            return Err(too_long);
        }
        if done {
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            return Ok(String::from_utf8_lossy(&line).into_owned());
        }
    }
}

/// Exactly `n` bytes of body (`n` is already within the limit).
fn http_server_exact(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
    n: usize,
    out: &mut Vec<u8>,
) -> Result<(), HttpServerReject> {
    let mut left = n;
    while left > 0 {
        let buf = http_server_fill(reader, deadline, true)?;
        if buf.is_empty() {
            // Fewer bytes than Content-Length, then EOF: the handler never
            // sees a zero-filled tail (#2823).
            return Err(HttpServerReject::Status(400));
        }
        let take = buf.len().min(left);
        out.extend_from_slice(&buf[..take]);
        std::io::BufRead::consume(reader, take);
        left -= take;
    }
    Ok(())
}

/// A chunked request body (RFC 9112 §7.1), chunk extensions and trailers
/// skipped, bounded by `max_body` as it arrives.
fn http_server_chunked(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
    max_body: usize,
) -> Result<Vec<u8>, HttpServerReject> {
    let bad = || HttpServerReject::Status(400);
    let mut body = Vec::new();
    loop {
        let line = http_server_line(reader, deadline, true, bad())?;
        let size = line.split(';').next().unwrap_or_default().trim();
        if size.is_empty() || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let n = match usize::from_str_radix(size, 16) {
            Ok(n) => n,
            Err(_) => return Err(HttpServerReject::Status(413)),
        };
        if n == 0 {
            break;
        }
        if n > max_body.saturating_sub(body.len()) {
            return Err(HttpServerReject::Status(413));
        }
        http_server_exact(reader, deadline, n, &mut body)?;
        if !http_server_line(reader, deadline, true, bad())?.is_empty() {
            return Err(bad());
        }
    }
    // Trailer fields up to the empty line; their count shares the header cap.
    for _ in 0..=HTTP_SERVER_MAX_HEADERS {
        if http_server_line(reader, deadline, true, HttpServerReject::Status(431))?.is_empty() {
            return Ok(body);
        }
    }
    Err(HttpServerReject::Status(431))
}

/// The body framing a request head declares.
#[derive(Default)]
struct HttpServerFraming {
    content_length: Option<u64>,
    chunked: bool,
}

impl HttpServerFraming {
    /// Fold one header field into the framing: a garbled or conflicting
    /// length is 400, a coding other than `chunked` is 501.
    fn note(&mut self, key: &str, val: &str) -> Result<(), HttpServerReject> {
        if key.eq_ignore_ascii_case("content-length") {
            // Digits only; a length past u64 is too large, not garbage.
            if val.is_empty() || !val.bytes().all(|b| b.is_ascii_digit()) {
                return Err(HttpServerReject::Status(400));
            }
            let n = val.parse::<u64>().unwrap_or(u64::MAX);
            if self.content_length.is_some_and(|m| m != n) {
                return Err(HttpServerReject::Status(400));
            }
            self.content_length = Some(n);
        } else if key.eq_ignore_ascii_case("transfer-encoding") {
            // Only `chunked` is decoded; any other coding is 501.
            for coding in val.split(',').map(|c| c.trim()).filter(|c| !c.is_empty()) {
                if coding.eq_ignore_ascii_case("chunked") && !self.chunked {
                    self.chunked = true;
                } else {
                    return Err(HttpServerReject::Status(501));
                }
            }
        }
        Ok(())
    }
}

/// The header fields up to the empty line (a line without a colon is
/// skipped but counts toward the cap), and the framing they declare.
fn http_server_read_headers(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
) -> Result<(Vec<(String, String)>, HttpServerFraming), HttpServerReject> {
    let too_many = || HttpServerReject::Status(431);
    let mut headers = Vec::new();
    let mut framing = HttpServerFraming::default();
    let mut lines = 0usize;
    loop {
        let line = http_server_line(reader, deadline, true, too_many())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok((headers, framing));
        }
        lines += 1;
        if lines > HTTP_SERVER_MAX_HEADERS {
            return Err(too_many());
        }
        if let Some(idx) = trimmed.find(':') {
            let key = trimmed[..idx].trim().to_string();
            let val = trimmed[idx + 1..].trim().to_string();
            framing.note(&key, &val)?;
            headers.push((key, val));
        }
    }
}

/// The body the framing declares, bounded by `max_body`.
fn http_server_read_body(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    deadline: std::time::Instant,
    max_body: usize,
    framing: &HttpServerFraming,
) -> Result<Vec<u8>, HttpServerReject> {
    if framing.chunked {
        // Both framings at once is a smuggling shape (RFC 9112 §6.3): refused.
        if framing.content_length.is_some() {
            return Err(HttpServerReject::Status(400));
        }
        return http_server_chunked(reader, deadline, max_body);
    }
    let n = framing.content_length.unwrap_or(0);
    if n > max_body as u64 {
        return Err(HttpServerReject::Status(413));
    }
    let mut body = Vec::with_capacity(n as usize);
    http_server_exact(reader, deadline, n as usize, &mut body)?;
    Ok(body)
}

fn http_server_read_request(
    stream: &std::net::TcpStream,
    deadline: std::time::Instant,
    max_body: usize,
) -> Result<HttpServerRequest, HttpServerReject> {
    let clone = stream.try_clone().map_err(|_| HttpServerReject::Drop)?;
    let mut reader = std::io::BufReader::new(clone);
    let first_line = http_server_line(&mut reader, deadline, false, HttpServerReject::Status(414))?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(HttpServerReject::Drop);
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();
    let (headers, framing) = http_server_read_headers(&mut reader, deadline)?;
    let body = http_server_read_body(&mut reader, deadline, max_body, &framing)?;
    Ok((method, path, String::from_utf8_lossy(&body).to_string(), headers))
}

// ── Writing a response ──

/// The reason phrase: the IANA HTTP Status Code Registry (RFC 9110 §15 and
/// the codes registered since); a code outside it gets an empty reason, which
/// the status line allows (RFC 9112 §4). C-367 does not compare it.
pub fn http_server_reason(status: i64) -> &'static str {
    match HTTP_SERVER_REASONS.binary_search_by_key(&status, |&(code, _)| code) {
        Ok(i) => HTTP_SERVER_REASONS[i].1,
        Err(_) => "",
    }
}

/// Sorted by code: `http_server_reason` binary-searches it.
const HTTP_SERVER_REASONS: &[(i64, &str)] = &[
    (100, "Continue"),
    (101, "Switching Protocols"),
    (102, "Processing"),
    (103, "Early Hints"),
    (200, "OK"),
    (201, "Created"),
    (202, "Accepted"),
    (203, "Non-Authoritative Information"),
    (204, "No Content"),
    (205, "Reset Content"),
    (206, "Partial Content"),
    (207, "Multi-Status"),
    (208, "Already Reported"),
    (226, "IM Used"),
    (300, "Multiple Choices"),
    (301, "Moved Permanently"),
    (302, "Found"),
    (303, "See Other"),
    (304, "Not Modified"),
    (305, "Use Proxy"),
    (307, "Temporary Redirect"),
    (308, "Permanent Redirect"),
    (400, "Bad Request"),
    (401, "Unauthorized"),
    (402, "Payment Required"),
    (403, "Forbidden"),
    (404, "Not Found"),
    (405, "Method Not Allowed"),
    (406, "Not Acceptable"),
    (407, "Proxy Authentication Required"),
    (408, "Request Timeout"),
    (409, "Conflict"),
    (410, "Gone"),
    (411, "Length Required"),
    (412, "Precondition Failed"),
    (413, "Content Too Large"),
    (414, "URI Too Long"),
    (415, "Unsupported Media Type"),
    (416, "Range Not Satisfiable"),
    (417, "Expectation Failed"),
    (418, "I'm a teapot"),
    (421, "Misdirected Request"),
    (422, "Unprocessable Content"),
    (423, "Locked"),
    (424, "Failed Dependency"),
    (425, "Too Early"),
    (426, "Upgrade Required"),
    (428, "Precondition Required"),
    (429, "Too Many Requests"),
    (431, "Request Header Fields Too Large"),
    (451, "Unavailable For Legal Reasons"),
    (500, "Internal Server Error"),
    (501, "Not Implemented"),
    (502, "Bad Gateway"),
    (503, "Service Unavailable"),
    (504, "Gateway Timeout"),
    (505, "HTTP Version Not Supported"),
    (506, "Variant Also Negotiates"),
    (507, "Insufficient Storage"),
    (508, "Loop Detected"),
    (510, "Not Extended"),
    (511, "Network Authentication Required"),
];

/// A header the response may not carry as written (#2822): a name that is
/// not an RFC 9110 token, or a value holding CR, LF or NUL — either would
/// let the handler's text end the field and start another (response
/// splitting). `None` for a well-formed field.
pub fn http_server_bad_header(name: &str, value: &str) -> Option<String> {
    let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    if name.is_empty() || !name.bytes().all(tchar) {
        return Some(format!("response header name {:?} is not a token", name));
    }
    if value.bytes().any(|b| matches!(b, b'\r' | b'\n' | 0)) {
        return Some(format!("response header {} has a CR, LF or NUL in its value", name));
    }
    None
}

/// The response bytes for a request of any method but HEAD: see
/// [`http_server_response_bytes_for`].
pub fn http_server_response_bytes(status: i64, headers: &[(String, String)], body: &str) -> Vec<u8> {
    http_server_response_bytes_for(false, status, headers, body)
}

/// The response bytes: status line, the response's headers in order (a
/// handler's `Connection` field dropped — the core owns the connection),
/// `Connection: close` (one response per connection; keep-alive is #2665),
/// Content-Length, the body. A HEAD request, a 1xx, a 204 and a 304 get no
/// body (RFC 9110 §6.4.1); 1xx and 204 get no Content-Length either
/// (§8.6). The headers must already have passed [`http_server_bad_header`].
pub fn http_server_response_bytes_for(head: bool, status: i64, headers: &[(String, String)], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", status, http_server_reason(status));
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("connection") {
            continue;
        }
        out.push_str(&format!("{}: {}\r\n", k, v));
    }
    out.push_str("Connection: close\r\n");
    let informational = (100..200).contains(&status);
    if !(informational || status == 204) {
        out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    out.push_str("\r\n");
    if !(head || informational || status == 204 || status == 304) {
        out.push_str(body);
    }
    out.into_bytes()
}

/// Write the handler's response and close the connection. Two answers
/// replace it: a header that would split the response (#2822) is a `500`
/// plus one stderr line, and a response that arrives after the request
/// timeout (ADR-0020 §5.7) is a `503`.
pub fn http_server_write(conn: HttpServerConn, status: i64, headers: &[(String, String)], body: &str) -> Result<(), String> {
    let plain = [("Content-Type".to_string(), "text/plain".to_string())];
    if let Some(why) = headers.iter().find_map(|(k, v)| http_server_bad_header(k, v)) {
        eprintln!("http.serve: refused the response: {}", why.escape_debug());
        return http_server_send(conn, 500, &plain, &format!("Internal error: {}", why.escape_debug()));
    }
    if std::time::Instant::now() >= conn.deadline {
        return http_server_send(conn, 503, &plain, http_server_reason(503));
    }
    http_server_send(conn, status, headers, body)
}

/// Send, then close without resetting: the write side is shut first and
/// whatever the client still sends (a body the core refused, a pipelined
/// request) is read and dropped for a moment, so the close does not turn
/// into a TCP reset that could discard the response before the client read
/// it.
fn http_server_send(conn: HttpServerConn, status: i64, headers: &[(String, String)], body: &str) -> Result<(), String> {
    let mut stream = conn.stream;
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(HTTP_SERVER_DRAIN_MS)));
    let sent = std::io::Write::write_all(&mut stream, &http_server_response_bytes_for(conn.head, status, headers, body)).map_err(|e| e.to_string());
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let until = std::time::Instant::now() + std::time::Duration::from_millis(250);
    let mut sink = [0u8; 16 * 1024];
    let mut drained = 0usize;
    while drained < 1 << 20 {
        let Some(left) = http_server_timeout(until) else { break };
        if stream.set_read_timeout(Some(left)).is_err() {
            break;
        }
        match std::io::Read::read(&mut stream, &mut sink) {
            Ok(0) | Err(_) => break,
            Ok(n) => drained += n,
        }
    }
    sent
}
