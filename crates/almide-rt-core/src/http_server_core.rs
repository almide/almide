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
// close after one response, the limits' answers (413 / 400 / 503, #2823;
// 408 for a request head not whole by the header deadline, #4021, C-389).
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
    /// From accept: a client that has sent part of its request head (the
    /// request line and the header fields) but not all of it by then is
    /// `408` (#4021). Separate from the body's read and from the handler's
    /// own time, which stay under `request_timeout_ms`. A client that sent
    /// nothing at all by then is closed unanswered (a browser's speculative
    /// pre-connect must not log a `408`).
    pub header_timeout_ms: u64,
    /// Connections accepted and not yet closed, at most (ADR-0020 §5.7
    /// `max_in_flight`). When that many are in flight the server stops
    /// accepting and new connections wait in the kernel's backlog.
    pub max_in_flight: usize,
}

/// `http.serve`'s request-head deadline: 10 s from accept (#4021).
pub const HTTP_SERVER_HEADER_TIMEOUT_MS: u64 = 10_000;

/// `http.serve`'s connections in flight at once (ADR-0020 §5.7).
pub const HTTP_SERVER_MAX_IN_FLIGHT: usize = 64;

/// `http.serve`'s limits: 1 MiB of body, a 10 s request-head deadline, a 30 s
/// request timeout, 64 connections in flight (ADR-0020 §5.7, #4021).
pub const HTTP_SERVER_DEFAULT_LIMITS: HttpServerLimits = HttpServerLimits {
    max_body_bytes: 1 << 20,
    request_timeout_ms: HTTP_SERVER_DRAIN_MS,
    header_timeout_ms: HTTP_SERVER_HEADER_TIMEOUT_MS,
    max_in_flight: HTTP_SERVER_MAX_IN_FLIGHT,
};

/// The longest request line and header line (`414` / `431` beyond), and the
/// most header lines (`431` beyond).
const HTTP_SERVER_MAX_LINE: usize = 8 * 1024;
const HTTP_SERVER_MAX_HEADERS: usize = 100;

/// A connection whose request has been read: the response goes back on it.
/// It remembers what shapes that response — a HEAD gets no body (#2826) —
/// and when the request arrived, for the request timeout.
pub struct HttpServerConn {
    out: HttpServerOut,
    head: bool,
    deadline: std::time::Instant,
}

/// Where a response's bytes go: straight onto the socket (the sequential
/// [`http_server_next_with`]), or to the pool worker that read the request
/// and that writes the bytes, so a client slow to read its answer holds
/// that worker, never the thread running the handler (#4021).
enum HttpServerOut {
    Stream(std::net::TcpStream),
    Worker(std::sync::mpsc::SyncSender<Vec<u8>>, std::sync::Arc<HttpServerGate>),
}

/// The time a read may take and the answer when it runs out: `408` for the
/// request head under the header deadline, `503` under the request timeout.
#[derive(Clone, Copy)]
struct HttpServerClock {
    at: std::time::Instant,
    late: i64,
}

impl HttpServerClock {
    /// The head's clock and the body's, both counted from `accepted`.
    fn for_request(accepted: std::time::Instant, limits: &HttpServerLimits) -> (HttpServerClock, HttpServerClock) {
        let total = accepted + std::time::Duration::from_millis(limits.request_timeout_ms);
        let head = accepted + std::time::Duration::from_millis(limits.header_timeout_ms);
        let body = HttpServerClock { at: total, late: 503 };
        // A request timeout shorter than the header deadline governs the head too.
        let head = if head < total { HttpServerClock { at: head, late: 408 } } else { body };
        (head, body)
    }
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
        let accepted = std::time::Instant::now();
        match http_server_read_request(&stream, accepted, limits) {
            Ok((req, deadline)) => {
                let head = req.0 == "HEAD";
                return Some((HttpServerConn { out: HttpServerOut::Stream(stream), head, deadline }, req));
            }
            Err(HttpServerReject::Drop) => {}
            Err(HttpServerReject::Status(status)) => {
                let _ = http_server_put(stream, &http_server_refusal_bytes(status));
            }
        }
    }
}

/// The bytes of the core's own refusal: `<status>` with its reason as a
/// `text/plain` body.
fn http_server_refusal_bytes(status: i64) -> Vec<u8> {
    let headers = [("Content-Type".to_string(), "text/plain".to_string())];
    http_server_response_bytes_for(false, status, &headers, http_server_reason(status))
}

// ── The connection pool (#4021, ADR-0020 §5.5) ──
//
// One slow client must not stall every other one (slowloris). The pool's
// ACCEPT thread admits up to `max_in_flight` connections; each gets a worker
// thread that reads its request (under the header deadline, then the request
// timeout), hands the parsed request to the serving thread, waits for the
// response's bytes and writes them. A client trickling its head, or slow to
// read its answer, holds one worker — never the thread that runs the handler,
// and never the accept loop. At `max_in_flight` the accept thread stops
// accepting, so new connections wait in the kernel's backlog: the thread
// count is bounded, never one per connection attempt.
//
// The HANDLER still runs on ONE thread, the one that called `serve`, one
// request at a time, in the order requests arrive whole. Natively the app is
// an `Rc<dyn Fn>` built on main's thread, and stdout is a per-thread buffer;
// running handlers on the workers needs the per-worker app factory and the
// process-global stdout of ADR-0020 §5.5 (#2665). The app is already
// instance-closed (E095) and cannot reach a `var` (E008), so that step
// changes timing only. The embedded wasm lane is one instance by design
// (C-367): it serves through this same pool, so its slow-client behaviour is
// native's.

/// The pool's shared counts: connections in flight (accepted, not yet
/// closed) and responses handed to a worker and not yet written.
struct HttpServerGate {
    counts: std::sync::Mutex<(usize, usize)>,
    changed: std::sync::Condvar,
}

impl HttpServerGate {
    fn update(&self, f: impl FnOnce(&mut (usize, usize))) {
        if let Ok(mut c) = self.counts.lock() {
            f(&mut c);
        }
        self.changed.notify_all();
    }
}

/// One connection's place in the pool, released when its worker ends.
struct HttpServerSlot(std::sync::Arc<HttpServerGate>);

impl Drop for HttpServerSlot {
    fn drop(&mut self) {
        self.0.update(|c| c.0 -= 1);
    }
}

/// A request the pool read whole, or the accept thread's stop.
type HttpServerArrival = Option<(HttpServerConn, HttpServerRequest)>;

/// A running pool: [`HttpServerPool::next`] answers its requests in the order
/// they arrive whole. Dropping it waits until every response handed to a
/// worker has been written (each write is bounded by its own timeout).
pub struct HttpServerPool {
    ready: std::sync::mpsc::Receiver<HttpServerArrival>,
    gate: std::sync::Arc<HttpServerGate>,
}

/// Start serving `listener` under `limits` (see the section comment).
pub fn http_server_pool(listener: std::net::TcpListener, limits: HttpServerLimits) -> HttpServerPool {
    let gate = std::sync::Arc::new(HttpServerGate { counts: std::sync::Mutex::new((0, 0)), changed: std::sync::Condvar::new() });
    let (tx, ready) = std::sync::mpsc::channel::<HttpServerArrival>();
    let shared = gate.clone();
    // No accept thread (the OS refused one): its sender is gone with it, so
    // `next` answers `None` at once.
    let _ = std::thread::Builder::new()
        .name("almide-serve-accept".into())
        .spawn(move || http_server_accept_loop(listener, limits, shared, tx));
    HttpServerPool { ready, gate }
}

impl HttpServerPool {
    /// The next request read whole; `None` once a shutdown signal arrived
    /// (ADR-0020 §5.6). A request read but not yet handed over when the
    /// signal arrived is closed unanswered, as an unaccepted connection is.
    pub fn next(&self) -> Option<(HttpServerConn, HttpServerRequest)> {
        loop {
            if http_server_stopping() {
                return None;
            }
            match self.ready.recv_timeout(std::time::Duration::from_millis(25)) {
                Ok(Some(arrival)) if !http_server_stopping() => return Some(arrival),
                Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

impl Drop for HttpServerPool {
    fn drop(&mut self) {
        let Ok(mut c) = self.gate.counts.lock() else { return };
        while c.1 > 0 {
            c = match self.gate.changed.wait(c) {
                Ok(c) => c,
                Err(_) => return,
            };
        }
    }
}

fn http_server_accept_loop(
    listener: std::net::TcpListener,
    limits: HttpServerLimits,
    gate: std::sync::Arc<HttpServerGate>,
    tx: std::sync::mpsc::Sender<HttpServerArrival>,
) {
    let max = limits.max_in_flight.max(1);
    loop {
        // A free slot first: at `max_in_flight` nothing is accepted, and the
        // kernel's backlog holds the next connections.
        if let Ok(mut c) = gate.counts.lock() {
            while c.0 >= max && !http_server_stopping() {
                c = match gate.changed.wait_timeout(c, std::time::Duration::from_millis(25)) {
                    Ok((c, _)) => c,
                    Err(_) => break,
                };
            }
        }
        if http_server_stopping() {
            break;
        }
        let accepted = listener.accept();
        if http_server_stopping() {
            break;
        }
        let stream = match accepted {
            Ok((s, _)) => s,
            Err(_) => {
                // Out of descriptors and the like: do not spin.
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
        };
        let at = std::time::Instant::now();
        gate.update(|c| c.0 += 1);
        let slot = HttpServerSlot(gate.clone());
        let worker_tx = tx.clone();
        let worker_gate = gate.clone();
        // A worker that cannot start drops its slot and its stream: that
        // connection closes unanswered.
        let _ = std::thread::Builder::new()
            .name("almide-serve-conn".into())
            .spawn(move || http_server_worker(stream, at, limits, slot, worker_gate, worker_tx));
    }
    // The listener closes here: connections not yet accepted are refused.
    drop(listener);
    let _ = tx.send(None);
}

/// One connection: read its request, hand it over, write its answer.
fn http_server_worker(
    stream: std::net::TcpStream,
    accepted: std::time::Instant,
    limits: HttpServerLimits,
    _slot: HttpServerSlot,
    gate: std::sync::Arc<HttpServerGate>,
    tx: std::sync::mpsc::Sender<HttpServerArrival>,
) {
    match http_server_read_request(&stream, accepted, &limits) {
        Ok((req, deadline)) => {
            let (reply, answer) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
            let head = req.0 == "HEAD";
            let conn = HttpServerConn { out: HttpServerOut::Worker(reply, gate.clone()), head, deadline };
            if tx.send(Some((conn, req))).is_err() {
                return;
            }
            drop(tx);
            // No bytes: the server stopped before handling it, or dropped
            // the connection — it closes unanswered.
            if let Ok(bytes) = answer.recv() {
                let _ = http_server_put(stream, &bytes);
                gate.update(|c| c.1 -= 1);
            }
        }
        Err(HttpServerReject::Drop) => {}
        Err(HttpServerReject::Status(status)) => {
            let _ = http_server_put(stream, &http_server_refusal_bytes(status));
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
// The handler runs on one thread, so "in flight" is at most the one request
// the serving thread is handling, plus the responses the pool's workers are
// still writing (dropping the pool waits for those, #4021); the watcher never
// touches them. When handlers run on the workers (§5.5, #2665) the drain
// waits for every worker's request instead, and the forced path gets a
// process-global stdout buffer to flush from any thread.

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
    clock: HttpServerClock,
    started: bool,
) -> Result<&'a [u8], HttpServerReject> {
    // Nothing read yet: a silent or vanished connection is dropped. Past the
    // first byte, running out of time is the clock's answer: the header
    // deadline's 408 while the head is read, the request timeout's 503 after.
    let late = if started { HttpServerReject::Status(clock.late) } else { HttpServerReject::Drop };
    let Some(left) = http_server_timeout(clock.at) else {
        return Err(late);
    };
    if reader.get_ref().set_read_timeout(Some(left)).is_err() {
        return Err(HttpServerReject::Drop);
    }
    // An interrupted read moved no bytes: read again (EINTR under
    // SO_RCVTIMEO — the client core's `http_read_retrying` says why). The
    // retry is a separate call so the borrow of the buffer ends with it.
    while let Err(e) = std::io::BufRead::fill_buf(reader) {
        if e.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
            late
        } else {
            HttpServerReject::Drop
        });
    }
    std::io::BufRead::fill_buf(reader).map_err(|_| HttpServerReject::Drop)
}

/// One line without its line ending; `too_long` answers a line over the limit.
fn http_server_line(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    clock: HttpServerClock,
    started: bool,
    too_long: HttpServerReject,
) -> Result<String, HttpServerReject> {
    let mut line: Vec<u8> = Vec::new();
    loop {
        let buf = http_server_fill(reader, clock, started || !line.is_empty())?;
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
            // RFC 9112 §2.2 (C-405): a line ends in CRLF, and a CR anywhere
            // else is refused — a bare LF or a bare CR is read differently by
            // a proxy that accepts it.
            if !line.ends_with(b"\r\n") {
                return Err(HttpServerReject::Status(400));
            }
            line.truncate(line.len() - 2);
            if line.contains(&b'\r') {
                return Err(HttpServerReject::Status(400));
            }
            return Ok(String::from_utf8_lossy(&line).into_owned());
        }
    }
}

/// Exactly `n` bytes of body (`n` is already within the limit).
fn http_server_exact(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    clock: HttpServerClock,
    n: usize,
    out: &mut Vec<u8>,
) -> Result<(), HttpServerReject> {
    let mut left = n;
    while left > 0 {
        let buf = http_server_fill(reader, clock, true)?;
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
    clock: HttpServerClock,
    max_body: usize,
) -> Result<Vec<u8>, HttpServerReject> {
    let bad = || HttpServerReject::Status(400);
    let mut body = Vec::new();
    loop {
        let line = http_server_line(reader, clock, true, bad())?;
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
        http_server_exact(reader, clock, n, &mut body)?;
        if !http_server_line(reader, clock, true, bad())?.is_empty() {
            return Err(bad());
        }
    }
    // Trailer fields up to the empty line; their count shares the header cap.
    for _ in 0..=HTTP_SERVER_MAX_HEADERS {
        if http_server_line(reader, clock, true, HttpServerReject::Status(431))?.is_empty() {
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

/// The header fields up to the empty line, read by RFC 9112 (ALS-R5, C-405,
/// #4112): a line starting with space or tab (obs-fold, §5.2), a line without
/// a colon, and a field name that is not a token — whitespace before the
/// colon included (§5.1) — are each `400`. The core used to trim the name,
/// skip a colon-less line and read a folded line as a new field, so a proxy in
/// front and the program could disagree about what a request carried. The
/// value is trimmed of optional whitespace (SP / HTAB) on both sides.
fn http_server_read_headers(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    clock: HttpServerClock,
) -> Result<(Vec<(String, String)>, HttpServerFraming), HttpServerReject> {
    let too_many = || HttpServerReject::Status(431);
    let mut headers = Vec::new();
    let mut framing = HttpServerFraming::default();
    let mut lines = 0usize;
    loop {
        let line = http_server_line(reader, clock, true, too_many())?;
        if line.is_empty() {
            return Ok((headers, framing));
        }
        lines += 1;
        if lines > HTTP_SERVER_MAX_HEADERS {
            return Err(too_many());
        }
        if line.starts_with([' ', '\t']) {
            return Err(HttpServerReject::Status(400));
        }
        let Some(idx) = line.find(':') else {
            return Err(HttpServerReject::Status(400));
        };
        let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
        let name = &line[..idx];
        if name.is_empty() || !name.bytes().all(tchar) {
            return Err(HttpServerReject::Status(400));
        }
        let key = name.to_string();
        let val = line[idx + 1..].trim_matches([' ', '\t']).to_string();
        framing.note(&key, &val)?;
        headers.push((key, val));
    }
}

/// The body the framing declares, bounded by `max_body`.
fn http_server_read_body(
    reader: &mut std::io::BufReader<std::net::TcpStream>,
    clock: HttpServerClock,
    max_body: usize,
    framing: &HttpServerFraming,
) -> Result<Vec<u8>, HttpServerReject> {
    if framing.chunked {
        // Both framings at once is a smuggling shape (RFC 9112 §6.3): refused.
        if framing.content_length.is_some() {
            return Err(HttpServerReject::Status(400));
        }
        return http_server_chunked(reader, clock, max_body);
    }
    let n = framing.content_length.unwrap_or(0);
    if n > max_body as u64 {
        return Err(HttpServerReject::Status(413));
    }
    let mut body = Vec::with_capacity(n as usize);
    http_server_exact(reader, clock, n as usize, &mut body)?;
    Ok(body)
}

/// The request line by RFC 9112 §3 (C-405, #4112): exactly three parts
/// separated by single spaces — a token method, a target, and `HTTP/1.1` or
/// `HTTP/1.0`. The target is origin-form (`/…`, no control character or
/// DEL), or absolute-form `http://` / `https://` mapped to its origin-form
/// path and query; asterisk-form and authority-form are refused. Answers
/// (method, origin-form target, is HTTP/1.1), or None for a `400`.
pub fn http_server_request_line(line: &str) -> Option<(String, String, bool)> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() != 3 {
        return None;
    }
    let (method, target, version) = (parts[0], parts[1], parts[2]);
    let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    if method.is_empty() || !method.bytes().all(tchar) {
        return None;
    }
    let http11 = match version {
        "HTTP/1.1" => true,
        "HTTP/1.0" => false,
        _ => return None,
    };
    if target.is_empty() || target.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return None;
    }
    let origin = if target.starts_with('/') {
        target.to_string()
    } else {
        let lower = target.to_ascii_lowercase();
        let rest = if lower.starts_with("http://") {
            &target[7..]
        } else if lower.starts_with("https://") {
            &target[8..]
        } else {
            return None;
        };
        let cut = rest.find(['/', '?']).unwrap_or(rest.len());
        if cut == 0 {
            return None;
        }
        let tail = &rest[cut..];
        if tail.starts_with('/') {
            tail.to_string()
        } else {
            format!("/{tail}")
        }
    };
    Some((method.to_string(), origin, http11))
}

/// The request on `stream`, accepted at `accepted`: its head under the header
/// deadline, its body under the request timeout. Answers the request and the
/// instant its response is due by.
fn http_server_read_request(
    stream: &std::net::TcpStream,
    accepted: std::time::Instant,
    limits: &HttpServerLimits,
) -> Result<(HttpServerRequest, std::time::Instant), HttpServerReject> {
    let (head, body_clock) = HttpServerClock::for_request(accepted, limits);
    let clone = stream.try_clone().map_err(|_| HttpServerReject::Drop)?;
    let mut reader = std::io::BufReader::new(clone);
    let first_line = http_server_line(&mut reader, head, false, HttpServerReject::Status(414))?;
    if first_line.is_empty() {
        return Err(HttpServerReject::Drop);
    }
    let (method, path, http11) = http_server_request_line(&first_line).ok_or(HttpServerReject::Status(400))?;
    let (headers, framing) = http_server_read_headers(&mut reader, head)?;
    // RFC 9112 §3.2: an HTTP/1.1 request carries exactly one Host field, and
    // no request carries two.
    let hosts = headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("host")).count();
    if hosts > 1 || (http11 && hosts == 0) {
        return Err(HttpServerReject::Status(400));
    }
    let body = http_server_read_body(&mut reader, body_clock, limits.max_body_bytes, &framing)?;
    Ok(((method, path, String::from_utf8_lossy(&body).to_string(), headers), body_clock.at))
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

/// A header the response may not carry as written (#2822, C-367): a name
/// that is not an RFC 9110 token, or a value holding a control character
/// other than HTAB (§5.5) — CR or LF would let the handler's text end the
/// field and start another (response splitting). `None` for a well-formed
/// field. The wasm legs apply the same rule, with the same text, in
/// `stdlib/http_serve.almd` (`__hs_refusal`).
pub fn http_server_bad_header(name: &str, value: &str) -> Option<String> {
    let tchar = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
    if name.is_empty() || !name.bytes().all(tchar) {
        return Some(format!("response header name {} is not a token", http_server_quote(name)));
    }
    if value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return Some(format!("response header {} has a control character in its value", name));
    }
    None
}

/// A status the response may not carry (C-367): a final status is a
/// 3-digit code in 200..=599 (RFC 9110 §15; a 1xx is interim, and a client
/// would wait on for the final answer).
pub fn http_server_bad_status(status: i64) -> Option<String> {
    (!(200..=599).contains(&status)).then(|| format!("response status {status} is not a final status code"))
}

/// `s` in double quotes for a refusal text: `\` and `"` escaped, and every
/// control character (U+0000..=U+001F, U+007F..=U+009F) as `\u{hex}`, so
/// the text is one printable line on every leg.
// A handler's err never reaches the client (C-409, #4150, CWE-209): it can
// carry paths, operands, errno text and whatever the program interpolated.
// The client gets a fixed body naming the request's id; the full error goes
// to stderr under the same id, quoted by `http_server_quote` so input in it
// cannot forge a log line or emit a terminal escape. TODO(#4126): share that
// issue's escaping function once it lands. The id is computed from the request
// itself — 8 lowercase hex digits of the 32-bit FNV-1a of `method SP target LF
// body`, `hash.fnv1a32`'s function — so every lane names the same request the
// same way (a per-loop counter cannot: a stock `wasmtime serve` instance sees
// one request). stdlib/http_serve.almd writes the same texts on the wasm legs.
/// The request id the handler-error texts carry.
pub fn http_server_request_id(method: &str, target: &str, body: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in method.bytes().chain(std::iter::once(b' ')).chain(target.bytes()).chain(std::iter::once(b'\n')).chain(body.bytes()) {
        h ^= u32::from(b);
        h = h.wrapping_mul(16_777_619);
    }
    format!("{h:08x}")
}

/// The 500 body for a handler err of the request `id` names.
pub fn http_server_handler_error_body(id: &str) -> String {
    format!("Internal Server Error (request {id})")
}

/// The stderr line for that err.
pub fn http_server_handler_error_line(id: &str, err: &str) -> String {
    format!("http.serve: request {id}: handler error: {}", http_server_quote(err))
}

/// The stderr line for a runtime ABORT inside the handler (#4203): the request
/// is answered with the same fixed 500 as an err, and the server keeps
/// serving. `abort` is the text the abort would have printed after `Error: `
/// (or after `PANIC: ` for a `panic(msg)`, which keeps that prefix).
pub fn http_server_handler_abort_line(id: &str, abort: &str) -> String {
    format!("http.serve: request {id}: handler aborted: {}", http_server_quote(abort))
}

pub fn http_server_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            // C-418's set (#4126): the C0 and C1 controls, DEL and the bidi
            // controls, every one as `\u{hex}` — a bidi control in a handler's
            // err no longer reorders the stderr line.
            c if almide_rt_errtext_is_escaped(c) => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The fields the server owns (the framing and the connection's lifetime):
/// a handler's are dropped from its response.
pub const HTTP_SERVER_OWNED_FIELDS: &[&str] = &["connection", "content-length", "transfer-encoding", "keep-alive"];

/// The response bytes for a request of any method but HEAD: see
/// [`http_server_response_bytes_for`].
pub fn http_server_response_bytes(status: i64, headers: &[(String, String)], body: &str) -> Vec<u8> {
    http_server_response_bytes_for(false, status, headers, body)
}

/// The response bytes: status line, the response's headers in order (a
/// handler's `Connection` field dropped — the core owns the connection),
/// `X-Content-Type-Options: nosniff` unless the response has that field,
/// `Connection: close` (one response per connection; keep-alive is #2665),
/// Content-Length, the body. A HEAD request, a 1xx, a 204 and a 304 get no
/// body (RFC 9110 §6.4.1); 1xx and 204 get no Content-Length either
/// (§8.6). The headers must already have passed [`http_server_bad_header`].
pub fn http_server_response_bytes_for(head: bool, status: i64, headers: &[(String, String)], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {} {}\r\n", status, http_server_reason(status));
    for (k, v) in headers {
        if HTTP_SERVER_OWNED_FIELDS.iter().any(|f| k.eq_ignore_ascii_case(f)) {
            continue;
        }
        out.push_str(&format!("{}: {}\r\n", k, v));
    }
    // A secure default (C-407, #4134): every response says
    // `X-Content-Type-Options: nosniff` unless the handler set the field.
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("x-content-type-options")) {
        out.push_str("X-Content-Type-Options: nosniff\r\n");
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
    if let Some(why) = headers.iter().find_map(|(k, v)| http_server_bad_header(k, v)).or_else(|| http_server_bad_status(status)) {
        eprintln!("http.serve: refused the response: {why}");
        return http_server_send(conn, 500, &plain, "Internal Server Error");
    }
    if std::time::Instant::now() >= conn.deadline {
        return http_server_send(conn, 503, &plain, http_server_reason(503));
    }
    http_server_send(conn, status, headers, body)
}

/// Send the response: on the socket directly, or through the pool worker
/// that holds the connection (it writes while the serving thread goes on).
fn http_server_send(conn: HttpServerConn, status: i64, headers: &[(String, String)], body: &str) -> Result<(), String> {
    let bytes = http_server_response_bytes_for(conn.head, status, headers, body);
    match conn.out {
        HttpServerOut::Stream(stream) => http_server_put(stream, &bytes),
        HttpServerOut::Worker(reply, gate) => {
            // Counted before it is handed over, so a pool dropped right after
            // still waits for this write.
            gate.update(|c| c.1 += 1);
            reply.send(bytes).map_err(|_| {
                gate.update(|c| c.1 -= 1);
                "the connection's worker is gone".to_string()
            })
        }
    }
}

/// Write `bytes`, then close without resetting: the write side is shut first
/// and whatever the client still sends (a body the core refused, a pipelined
/// request) is read and dropped for a moment, so the close does not turn
/// into a TCP reset that could discard the response before the client read
/// it.
fn http_server_put(mut stream: std::net::TcpStream, bytes: &[u8]) -> Result<(), String> {
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(HTTP_SERVER_DRAIN_MS)));
    let sent = std::io::Write::write_all(&mut stream, bytes).map_err(|e| e.to_string());
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
