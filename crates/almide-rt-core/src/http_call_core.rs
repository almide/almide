// The HTTP call-handle core (#2631 native, #2633 wasm) — ONE definition of
// the in-flight call (`http.start` / `poll` / `read_new` / `wait` / `cancel`
// and the `_with_limits` streaming step), shared the way the client core is:
// http_client_core.rs `include!`s this file, so the splice template inlines it
// into runtime/rs/src/http.rs's flat module and the embedded wasm host
// (almide-wasm-run) links the same text through the crate. The C-366
// observables — the limit and cancel error texts, the close-on-end, the
// UTF-8-safe `read_new` split — are equal on both lanes by shared code.
//
// Headers travel as `&[(String, String)]` and a finished response as the
// client core's `HttpTextResponse`; the AlmideMap / AlmideHttpResponse
// wrappers stay in http.rs. No `use` lines: the includer's
// `std::io::{Read, Write}` and `std::net::TcpStream` are in scope.

/// The length of an incomplete UTF-8 sequence at the END of `b` (0..=3):
/// the bytes from the last lead byte on, when that lead byte announces more
/// bytes than follow it. Splitting just before a lead byte never changes
/// what `from_utf8_lossy` produces, so holding these back and decoding them
/// with the next read gives the same text as decoding the whole body.
pub fn http_stream_incomplete_utf8_tail(b: &[u8]) -> usize {
    for back in 1..=b.len().min(4) {
        let byte = b[b.len() - back];
        if byte & 0xC0 == 0x80 {
            continue; // continuation byte — keep looking for its lead
        }
        let want = match byte {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        return if want > back { back } else { 0 };
    }
    0
}

// ── Call handle (#2631) ──
//
// `http.start` returns at once with an `HttpCall`: a worker thread dials,
// writes the request and reads the response into the shared state below, and
// the caller looks at it without blocking (`poll`, `read_new`), blocks for the
// end (`wait`), or closes the connection (`cancel`). Two limits ride along,
// both in milliseconds and both 0 = no limit:
//   total_ms  a WALL CLOCK from `start`: dial, write, first byte and body all
//             count. The worker's socket timeouts are clipped to it, and every
//             `poll` / `read_new` / `wait` checks it too, so it fires even while
//             the worker is stuck in a name lookup.
//   idle_ms   the longest gap between two reads that bring bytes (the wait for
//             the first byte counts) — the socket read timeout.
// ALMIDE_HTTP_TIMEOUT_SECS is NOT read here: it is the default of the calls
// that take no limits. A finished call (answered, failed, timed out or
// cancelled) has its socket shut down, so the server sees the close at once.
// Dropping the last copy of the handle cancels it.

pub struct AlmideHttpCallHead {
    status: i64,
    reason: String,
    headers: Vec<(String, String)>,
}

pub struct AlmideHttpCallState {
    head: Option<AlmideHttpCallHead>,
    /// The transfer-decoded body received so far.
    body: Vec<u8>,
    /// How much of `body` `read_new` has handed out.
    delivered: usize,
    /// `Some` once the call is over: `Ok` = a complete response.
    outcome: Option<Result<(), String>>,
    /// A duplicate of the connection, kept only to shut it down.
    socket: Option<TcpStream>,
}

pub struct AlmideHttpCallShared {
    state: std::sync::Mutex<AlmideHttpCallState>,
    cv: std::sync::Condvar,
    deadline: Option<std::time::Instant>,
    total_ms: i64,
    idle_ms: i64,
}

fn http_call_total_msg(total_ms: i64) -> String {
    format!("request timeout: total_ms {} exceeded", total_ms)
}

fn http_call_idle_msg(idle_ms: i64) -> String {
    format!("request timeout: idle_ms {} exceeded", idle_ms)
}

const HTTP_CALL_CANCELLED: &str = "request cancelled";

impl AlmideHttpCallShared {
    fn lock(&self) -> std::sync::MutexGuard<'_, AlmideHttpCallState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// End the call with `outcome` unless it already ended, and close the
    /// connection either way.
    fn finish(&self, outcome: Result<(), String>) {
        let mut st = self.lock();
        if st.outcome.is_none() {
            st.outcome = Some(outcome);
        }
        if let Some(s) = st.socket.take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        drop(st);
        self.cv.notify_all();
    }

    /// `cancel`: a running call ends as `err("request cancelled")` and the
    /// body bytes nobody has read yet are dropped, so nothing more arrives. A
    /// call that already ended is left as it is.
    pub fn cancel(&self) {
        let mut st = self.lock();
        if st.outcome.is_some() {
            return;
        }
        st.outcome = Some(Err(HTTP_CALL_CANCELLED.to_string()));
        st.delivered = st.body.len();
        if let Some(s) = st.socket.take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        drop(st);
        self.cv.notify_all();
    }

    fn past_deadline(&self) -> bool {
        self.deadline.is_some_and(|d| std::time::Instant::now() >= d)
    }

    /// The wall clock, checked from the caller's side.
    fn check_deadline(&self) {
        if self.past_deadline() {
            self.finish(Err(http_call_total_msg(self.total_ms)));
        }
    }

    /// Block until `ready` holds or the call ends, never past the deadline.
    fn wait_until(&self, ready: impl Fn(&AlmideHttpCallState) -> bool) -> std::sync::MutexGuard<'_, AlmideHttpCallState> {
        let mut st = self.lock();
        loop {
            if st.outcome.is_some() || ready(&st) {
                return st;
            }
            match self.deadline {
                Some(d) => {
                    let now = std::time::Instant::now();
                    if now >= d {
                        drop(st);
                        self.finish(Err(http_call_total_msg(self.total_ms)));
                        st = self.lock();
                        continue;
                    }
                    st = self.cv.wait_timeout(st, d - now).unwrap_or_else(|p| p.into_inner()).0;
                }
                None => {
                    st = self.cv.wait(st).unwrap_or_else(|p| p.into_inner());
                }
            }
        }
    }
}

pub fn http_call_take_new(st: &mut AlmideHttpCallState) -> String {
    let fresh = &st.body[st.delivered..];
    let keep = if st.outcome.is_some() { 0 } else { http_stream_incomplete_utf8_tail(fresh) };
    let ready = fresh.len() - keep;
    let text = String::from_utf8_lossy(&fresh[..ready]).into_owned();
    st.delivered += ready;
    text
}

fn http_call_result(st: &AlmideHttpCallState, outcome: &Result<(), String>) -> Result<HttpTextResponse, String> {
    match (outcome, &st.head) {
        (Err(e), _) => Err(e.clone()),
        (Ok(()), Some(h)) => Ok((h.status, h.headers.clone(), String::from_utf8_lossy(&st.body).into_owned())),
        (Ok(()), None) => Err("connection closed before headers received".to_string()),
    }
}

/// How the body is framed on the wire.
enum HttpCallFraming {
    Chunked { remaining: usize, awaiting_size: bool },
    Length(usize),
    UntilClose,
}

/// Move the decoded body bytes out of `raw` into `out`; `true` once the body
/// is complete by its own framing.
fn http_call_decode(f: &mut HttpCallFraming, raw: &mut Vec<u8>, out: &mut Vec<u8>) -> bool {
    match f {
        HttpCallFraming::Length(left) => {
            let take = (*left).min(raw.len());
            out.extend(raw.drain(..take));
            *left -= take;
            *left == 0
        }
        HttpCallFraming::UntilClose => {
            out.append(raw);
            false
        }
        HttpCallFraming::Chunked { remaining, awaiting_size } => loop {
            if !*awaiting_size && *remaining == 0 {
                // The CRLF that closes a chunk's data.
                if raw.len() < 2 {
                    return false;
                }
                if raw.starts_with(b"\r\n") {
                    raw.drain(..2);
                }
                *awaiting_size = true;
            }
            if *awaiting_size {
                let Some(nl) = raw.windows(2).position(|w| w == b"\r\n") else {
                    return false;
                };
                let size_line = String::from_utf8_lossy(&raw[..nl]).into_owned();
                let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
                raw.drain(..nl + 2);
                if size == 0 {
                    return true;
                }
                *remaining = size;
                *awaiting_size = false;
            }
            let take = (*remaining).min(raw.len());
            out.extend(raw.drain(..take));
            *remaining -= take;
            if *remaining > 0 {
                return false;
            }
        },
    }
}

/// Split the head off `raw` once it is whole: status, reason, header lines
/// (wire order, repeats kept — the `request_response` rule) and framing.
fn http_call_parse_head(raw: &mut Vec<u8>) -> Option<(AlmideHttpCallHead, HttpCallFraming)> {
    let idx = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let section = String::from_utf8_lossy(&raw[..idx]).into_owned();
    raw.drain(..idx + 4);
    let mut lines = section.lines();
    let status_line = lines.next().unwrap_or("");
    let status: i64 = status_line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let reason = status_line.splitn(3, ' ').nth(2).unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| {
            let (k, v) = line.split_once(':')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.to_ascii_lowercase().contains("chunked"));
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok());
    let framing = match (chunked, length) {
        (true, _) => HttpCallFraming::Chunked { remaining: 0, awaiting_size: true },
        (false, Some(n)) => HttpCallFraming::Length(n),
        (false, None) => HttpCallFraming::UntilClose,
    };
    Some((AlmideHttpCallHead { status, reason, headers }, framing))
}

/// The socket timeout for the next blocking step: the idle limit (reads
/// only) clipped to what is left of the wall clock. `Err` once the wall clock
/// ran out.
fn http_call_step_timeout(sh: &AlmideHttpCallShared, idle: bool) -> Result<Option<std::time::Duration>, String> {
    let left = match sh.deadline {
        Some(d) => {
            let now = std::time::Instant::now();
            if now >= d {
                return Err(http_call_total_msg(sh.total_ms));
            }
            Some(d - now)
        }
        None => None,
    };
    let idle = (idle && sh.idle_ms > 0).then(|| std::time::Duration::from_millis(sh.idle_ms as u64));
    let t = match (left, idle) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    Ok(t.map(|d| d.max(std::time::Duration::from_millis(1))))
}

/// Name the limit an io error ran into; a non-timeout error keeps its detail.
fn http_call_io_error(sh: &AlmideHttpCallShared, e: &std::io::Error, what: &str) -> String {
    if sh.past_deadline() {
        return http_call_total_msg(sh.total_ms);
    }
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) && sh.idle_ms > 0 && what == "read" {
        return http_call_idle_msg(sh.idle_ms);
    }
    format!("{} failed: {}", what, e)
}

fn http_call_run(
    sh: &AlmideHttpCallShared,
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(), String> {
    let (is_https, host, port, path) = parse_url(url)?;
    let addrs: Vec<std::net::SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
        .map_err(|e| http_call_io_error(sh, &e, "connection"))?
        .collect();
    let mut last_err: Option<std::io::Error> = None;
    let mut stream: Option<TcpStream> = None;
    for addr in addrs {
        let dialed = match http_call_step_timeout(sh, false)? {
            Some(t) => TcpStream::connect_timeout(&addr, t),
            None => TcpStream::connect(addr),
        };
        match dialed {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let stream = match (stream, last_err) {
        (Some(s), _) => s,
        (None, Some(e)) => return Err(http_call_io_error(sh, &e, "connection")),
        (None, None) => return Err(format!("connection failed: no address for {}", host)),
    };
    // The control copy sets the timeouts and is what `cancel` shuts down.
    let ctl = stream.try_clone().map_err(|e| format!("connection failed: {}", e))?;
    {
        let mut st = sh.lock();
        if st.outcome.is_some() {
            let _ = ctl.shutdown(std::net::Shutdown::Both);
            return Err(HTTP_CALL_CANCELLED.to_string());
        }
        st.socket = Some(ctl.try_clone().map_err(|e| format!("connection failed: {}", e))?);
    }
    if is_https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls = make_tls_stream(&host, stream)?;
            http_call_pump(sh, &mut tls, &ctl, method, &host, &path, body, headers)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("HTTPS is not supported on WASM target".to_string())
        }
    } else {
        let mut s = stream;
        http_call_pump(sh, &mut s, &ctl, method, &host, &path, body, headers)
    }
}

#[allow(clippy::too_many_arguments)]
fn http_call_pump<S: Read + Write>(
    sh: &AlmideHttpCallShared,
    s: &mut S,
    ctl: &TcpStream,
    method: &str,
    host: &str,
    path: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(), String> {
    ctl.set_write_timeout(http_call_step_timeout(sh, false)?).ok();
    if let Err(e) = http_write_request(s, method, host, path, body, headers) {
        return Err(if sh.past_deadline() { http_call_total_msg(sh.total_ms) } else { e });
    }
    let mut raw: Vec<u8> = Vec::new();
    let mut framing: Option<HttpCallFraming> = None;
    let mut buf = vec![0u8; 8192];
    loop {
        ctl.set_read_timeout(http_call_step_timeout(sh, true)?).ok();
        let n = match s.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                if sh.lock().outcome.is_some() {
                    return Err(HTTP_CALL_CANCELLED.to_string());
                }
                // A peer that closes without TLS close_notify after a body
                // framed by the close itself has answered completely (#1592).
                if e.kind() == std::io::ErrorKind::UnexpectedEof && matches!(framing, Some(HttpCallFraming::UntilClose)) {
                    return Ok(());
                }
                return Err(http_call_io_error(sh, &e, "read"));
            }
        };
        if n == 0 {
            return match framing {
                None => Err("connection closed before headers received".to_string()),
                Some(_) => Ok(()),
            };
        }
        raw.extend_from_slice(&buf[..n]);
        if framing.is_none() {
            let Some((head, f)) = http_call_parse_head(&mut raw) else {
                continue;
            };
            let mut st = sh.lock();
            if st.outcome.is_some() {
                return Err(HTTP_CALL_CANCELLED.to_string());
            }
            st.head = Some(head);
            framing = Some(f);
        }
        let mut decoded = Vec::new();
        let complete = http_call_decode(framing.as_mut().expect("framing is set"), &mut raw, &mut decoded);
        {
            let mut st = sh.lock();
            if st.outcome.is_some() {
                return Err(HTTP_CALL_CANCELLED.to_string());
            }
            st.body.extend_from_slice(&decoded);
        }
        sh.cv.notify_all();
        if complete {
            return Ok(());
        }
    }
}


/// Validate the limits and the url, then start the worker: the shared state
/// the handle wraps. `start`'s whole synchronous half — both lanes answer the
/// same `invalid limits: ...` / url error before any thread exists.
pub fn http_call_spawn(
    method: &str,
    url: &str,
    body: &str,
    headers: Vec<(String, String)>,
    total_ms: i64,
    idle_ms: i64,
) -> Result<std::sync::Arc<AlmideHttpCallShared>, String> {
    if total_ms < 0 || idle_ms < 0 {
        return Err(format!(
            "invalid limits: total_ms and idle_ms must be >= 0 (0 = no limit), got total_ms {} idle_ms {}",
            total_ms, idle_ms
        ));
    }
    parse_url(url)?;
    let shared = std::sync::Arc::new(AlmideHttpCallShared {
        state: std::sync::Mutex::new(AlmideHttpCallState {
            head: None,
            body: Vec::new(),
            delivered: 0,
            outcome: None,
            socket: None,
        }),
        cv: std::sync::Condvar::new(),
        deadline: (total_ms > 0)
            .then(|| std::time::Instant::now() + std::time::Duration::from_millis(total_ms as u64)),
        total_ms,
        idle_ms,
    });
    let worker = shared.clone();
    let (method, url, body) = (method.to_string(), url.to_string(), body.to_string());
    std::thread::Builder::new()
        .name("almide-http-call".to_string())
        .spawn(move || {
            let outcome = http_call_run(&worker, &method, &url, &body, &headers);
            worker.finish(outcome);
        })
        .map_err(|e| format!("could not start the request: {}", e))?;
    Ok(shared)
}

/// `poll`: never blocks — `None` while the call runs, the result once it ended.
pub fn http_call_poll(sh: &AlmideHttpCallShared) -> Option<Result<HttpTextResponse, String>> {
    sh.check_deadline();
    let st = sh.lock();
    st.outcome.as_ref().map(|o| http_call_result(&st, o))
}

/// `wait`: block until the call ends (bounded by its limits) and answer it —
/// ANY complete response is `Ok`, as with `request_response`.
pub fn http_call_wait(sh: &AlmideHttpCallShared) -> Result<HttpTextResponse, String> {
    let st = sh.wait_until(|_| false);
    let o = st.outcome.as_ref().expect("wait_until returns an ended call");
    http_call_result(&st, o)
}

/// `read_new`: the body text since the previous read; never blocks.
pub fn http_call_read_new(sh: &AlmideHttpCallShared) -> String {
    sh.check_deadline();
    let mut st = sh.lock();
    http_call_take_new(&mut st)
}

/// One turn of the streaming client on the handle (`request_stream_with_limits`
/// and the SSE twins): block until bytes arrive or the call ends, then answer
/// the text to deliver and — once the call is over — how it ended. A non-2xx
/// status is refused the way `request_stream` refuses it (after the whole
/// body, its first 500 chars quoted), with nothing delivered.
pub fn http_call_stream_step(sh: &AlmideHttpCallShared) -> (String, Option<Result<(), String>>) {
    let mut st = sh.wait_until(|st| st.head.is_some() && st.body.len() > st.delivered);
    let refused = st
        .head
        .as_ref()
        .filter(|h| !(200..300).contains(&h.status))
        .map(|h| format!("HTTP {}: {}", h.status, h.reason));
    if let Some(prefix) = refused {
        drop(st);
        let st = sh.wait_until(|_| false);
        let text = String::from_utf8_lossy(&st.body).into_owned();
        return (String::new(), Some(Err(format!("{}: {}", prefix, text.chars().take(500).collect::<String>()))));
    }
    let piece = http_call_take_new(&mut st);
    (piece, st.outcome.clone())
}
