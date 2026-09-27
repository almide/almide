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
// close after one response. Change it here and both lanes change together.

// Splice discipline (see http_client_core.rs): NO `use` lines — every std
// path is written fully qualified, so this text imports nothing that could
// collide with http.rs's or the client core's imports in the flat module.

/// A parsed request: (method, target, body, headers in wire order).
pub type HttpServerRequest = (String, String, String, Vec<(String, String)>);

/// Bind the listener `http.serve(port, _)` accepts on: every interface.
pub fn http_server_bind(port: i64) -> Result<std::net::TcpListener, String> {
    std::net::TcpListener::bind(format!("0.0.0.0:{}", port)).map_err(|e| format!("bind failed: {}", e))
}

/// The next request: accept, then parse. A failed accept or an unparsable
/// request drops that connection unanswered and waits for the next one.
/// `None` once a shutdown signal has arrived (ADR-0020 §5.6): the server
/// stops accepting, and a connection accepted after the signal — the
/// watcher's wake-up or a late client — is closed unanswered.
pub fn http_server_next(listener: &std::net::TcpListener) -> Option<(std::net::TcpStream, HttpServerRequest)> {
    loop {
        if http_server_stopping() {
            return None;
        }
        let accepted = listener.accept();
        if http_server_stopping() {
            return None;
        }
        let mut stream = match accepted {
            Ok((s, _)) => s,
            Err(_) => continue,
        };
        if let Ok(req) = http_server_read_request(&mut stream) {
            return Some((stream, req));
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

fn http_server_read_request(stream: &mut std::net::TcpStream) -> Result<HttpServerRequest, String> {
    let mut reader = std::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut first_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut first_line).map_err(|e| e.to_string())?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("invalid request".into());
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        std::io::BufRead::read_line(&mut reader, &mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(idx) = trimmed.find(':') {
            let key = trimmed[..idx].trim().to_string();
            let val = trimmed[idx + 1..].trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = val.parse().unwrap_or(0);
            }
            headers.push((key, val));
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        std::io::Read::read_exact(&mut reader, &mut body).ok();
    }

    Ok((method, path, String::from_utf8_lossy(&body).to_string(), headers))
}

/// The response bytes: status line (fixed reason table, `OK` outside it),
/// the response's headers in order, Content-Length, the body.
pub fn http_server_response_bytes(status: i64, headers: &[(String, String)], body: &str) -> Vec<u8> {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let mut out = format!("HTTP/1.1 {} {}\r\n", status, status_text);
    for (k, v) in headers {
        out.push_str(&format!("{}: {}\r\n", k, v));
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    out.push_str(body);
    out.into_bytes()
}

/// Write one response and close the connection (the stream drops here).
pub fn http_server_write(mut stream: std::net::TcpStream, status: i64, headers: &[(String, String)], body: &str) -> Result<(), String> {
    std::io::Write::write_all(&mut stream, &http_server_response_bytes(status, headers, body)).map_err(|e| e.to_string())
}
