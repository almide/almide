// http extern — Rust native HTTP client/server (platform layer)
// Uses std::net::TcpStream for client and TcpListener for server.
// HTTPS via rustls (pure-Rust TLS).
// SSE streaming: almide_rt_sse_openai_chat, almide_rt_sse_anthropic_messages (in sse.rs)

// HashMap already imported by prelude
// Read/Write/TcpStream come from the inlined client core (#1715); this
// file imports only its own remainder (server + SSE parsing).
use std::io::{BufRead, BufReader};
use std::net::TcpListener;

// ── HTTP response/request types ──
// The user-facing `HttpRequest` / `HttpResponse` nominals are RUNTIME-BACKED
// (stdlib_info::RUNTIME_BACKED_TYPES): the emitter spells them under the
// reserved `Almide*` names below (#1821), so a user's own `type HttpRequest`
// never meets a runtime item of the same spelling in the flat-spliced module.
// No bare alias may exist here — it collided (E0428) with exactly that user
// type whenever http.rs was inlined.

#[derive(Clone, Debug, PartialEq)]
pub struct AlmideHttpResponse {
    pub status: i64,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl AlmideHttpResponse {
    pub fn new(status: i64, body: String) -> Self {
        Self { status, body, headers: vec![("Content-Type".into(), "text/plain".into())] }
    }
    pub fn json(status: i64, body: String) -> Self {
        Self { status, body, headers: vec![("Content-Type".into(), "application/json".into())] }
    }
    /// EXACTLY the caller's headers — no seeded `Content-Type` (#1352). Entries
    /// are upserted in map order, so the list carries at most one entry per
    /// case-insensitive field name. Delegates to the same helper the
    /// `almide_rt_http_with_headers` intrinsic uses, so the struct API and the
    /// intrinsic cannot drift apart again — they used to disagree, this one
    /// REPLACING the header list while the intrinsic seeded-then-upserted.
    pub fn with_headers(status: i64, body: String, headers: AlmideMap<String, String>) -> Self {
        Self::from_headers(status, body, &headers)
    }
    fn from_headers(status: i64, body: String, headers: &AlmideMap<String, String>) -> Self {
        let mut resp = Self { status, body, headers: Vec::new() };
        for (k, v) in headers.iter() {
            upsert_header(&mut resp.headers, k, v);
        }
        resp
    }
}

/// Case-insensitive header upsert — RFC 9110 §5.1: field names are
/// case-insensitive, and ASCII-only (a field name is a `token`), which is
/// exactly `eq_ignore_ascii_case`. Replaces the first matching field's VALUE
/// IN PLACE (the name keeps the spelling it was first stored under, and the
/// entry does not move), and appends when the field is absent. Shared by
/// `set_header` and `with_headers` so a response never carries two entries for
/// one field name and set-then-get always round-trips: before #1352
/// `set_header` matched case-SENSITIVELY, so setting `content-type` on a
/// response holding `Content-Type` appended a SECOND entry that the
/// case-insensitive `get_header` then shadowed — the write vanished.
fn upsert_header(headers: &mut Vec<(String, String)>, key: &str, value: &str) {
    for slot in headers.iter_mut() {
        if slot.0.eq_ignore_ascii_case(key) {
            slot.1 = value.to_string();
            return;
        }
    }
    headers.push((key.to_string(), value.to_string()));
}

// ── Response builders ──

pub fn almide_http_redirect(url: &str, code: i64) -> AlmideHttpResponse {
    AlmideHttpResponse { status: code, body: String::new(), headers: vec![("Location".into(), url.to_string())] }
}

pub fn almide_rt_http_not_found(body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::new(404, body.to_string())
}

pub fn almide_rt_http_redirect(url: &str) -> AlmideHttpResponse {
    almide_http_redirect(url, 302)
}

pub fn almide_rt_http_response(status: i64, body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::new(status, body.to_string())
}

pub fn almide_rt_http_json(status: i64, body: &str) -> AlmideHttpResponse {
    AlmideHttpResponse::json(status, body.to_string())
}

pub fn almide_rt_http_with_headers(status: i64, body: &str, headers: &AlmideMap<String, String>) -> AlmideHttpResponse {
    AlmideHttpResponse::from_headers(status, body.to_string(), headers)
}

pub fn almide_http_set_status(mut resp: AlmideHttpResponse, code: i64) -> AlmideHttpResponse {
    resp.status = code; resp
}

pub fn almide_http_get_body(resp: &AlmideHttpResponse) -> String {
    resp.body.clone()
}

pub fn almide_http_set_header(mut resp: AlmideHttpResponse, key: &str, value: &str) -> AlmideHttpResponse {
    upsert_header(&mut resp.headers, key, value);
    resp
}

pub fn almide_http_get_header(resp: &AlmideHttpResponse, key: &str) -> Option<String> {
    resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
}

/// The status code of a response — the read twin of `almide_http_set_status`
/// (#1791; the Almide surface had no status GETTER before the response family).
pub fn almide_http_status_code(resp: &AlmideHttpResponse) -> i64 {
    resp.status
}

/// EVERY value of one field, in wire order — the accessor that keeps a
/// repeated field (`Set-Cookie`) whole where `get_header` answers the first.
pub fn almide_http_header_values(resp: &AlmideHttpResponse, key: &str) -> Vec<String> {
    resp.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone()).collect()
}

/// All headers as a map keyed by the LOWERCASED field name (RFC 9110 §5.1 —
/// field names are case-insensitive ASCII tokens). A repeated field keeps its
/// FIRST value — the same rule `get_header` / `req_header` apply — so a
/// `Set-Cookie` list is read through `header_values`, never through this map.
pub fn almide_http_headers(resp: &AlmideHttpResponse) -> AlmideMap<String, String> {
    let mut out = AlmideMap::new();
    for (k, v) in resp.headers.iter() {
        let name = k.to_ascii_lowercase();
        if !out.contains_key(&name) {
            out.insert(name, v.clone());
        }
    }
    out
}

pub fn almide_http_set_cookie(mut resp: AlmideHttpResponse, name: &str, value: &str) -> AlmideHttpResponse {
    resp.headers.push(("Set-Cookie".into(), format!("{}={}", name, value)));
    resp
}

// ── Request accessors ──

#[derive(Clone, Debug)]
pub struct AlmideHttpRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

pub fn almide_http_req_method(req: &AlmideHttpRequest) -> String { req.method.clone() }
pub fn almide_http_req_path(req: &AlmideHttpRequest) -> String { req.path.clone() }
pub fn almide_http_req_body(req: &AlmideHttpRequest) -> String { req.body.clone() }

pub fn almide_http_req_header(req: &AlmideHttpRequest, key: &str) -> Option<String> {
    req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
}

pub fn almide_http_query_params(req: &AlmideHttpRequest) -> AlmideMap<String, String> {
    let mut params = AlmideMap::new();
    if let Some(q) = req.path.split('?').nth(1) {
        for pair in q.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(percent_decode(k), percent_decode(v));
            }
        }
    }
    params
}

/// Percent-decode a URL component for manual use (stdlib `http.url_decode`).
/// Same rules as the query-param decoder: `+` → space, `%XX` → raw byte.
pub fn almide_http_url_decode(s: &str) -> String {
    percent_decode(s)
}


pub fn almide_http_get(url: &str) -> Result<String, String> {
    almide_http_request("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_post(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("POST", url, body, &AlmideMap::new())
}

pub fn almide_http_put(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("PUT", url, body, &AlmideMap::new())
}

pub fn almide_http_patch(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("PATCH", url, body, &AlmideMap::new())
}

pub fn almide_http_delete(url: &str) -> Result<String, String> {
    almide_http_request("DELETE", url, "", &AlmideMap::new())
}

pub fn almide_http_get_with_headers(url: &str, headers: &AlmideMap<String, String>) -> Result<String, String> {
    almide_http_request("GET", url, "", headers)
}

// ── HTTP Client ──
//
// The client core (parse/timeout/framing/TLS/exchange, all three result
// shapes) lives in crates/almide-rt-core/src/http_client_core.rs and is
// inlined here at embed time (#1715) — the SAME text the embedded host
// links, so the C-328 equality holds by shared code. The wrappers below
// only adapt the AlmideMap header surface to the core's pair slice.
include!("../../../crates/almide-rt-core/src/http_client_core.rs");

/// Headers cross into the shared core as a plain pair slice.
fn header_pairs(headers: &AlmideMap<String, String>) -> Vec<(String, String)> {
    headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn almide_http_request(method: &str, url: &str, body: &str, headers: &AlmideMap<String, String>) -> Result<String, String> {
    request(method, url, body, &header_pairs(headers))
}

// ── Status-preserving client ──
//
// Mirrors the String client but returns `(status_code, body)` for ANY
// complete response — a 404 is `Ok((404, body))`, not an `Err`. `Err` is
// reserved for transport failures (connection / TLS / timeout). The String
// client above collapses the status into the body-or-error distinction, so a
// caller that needs to branch on the numeric status (e.g. 404 vs 200) uses
// these instead.

pub fn almide_http_get_status(url: &str) -> Result<(i64, String), String> {
    almide_http_request_status("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_request_status(method: &str, url: &str, body: &str, headers: &AlmideMap<String, String>) -> Result<(i64, String), String> {
    request_status(method, url, body, &header_pairs(headers))
}

// ── Full-response client (#1791) ──
//
// The twin of every verb-shaped String client, answering the WHOLE response
// as an `AlmideHttpResponse` — status, every header line in wire order
// (repeats kept), body — for ANY complete response; `Err` is transport-only.
// Redirects are never followed, so a 3xx arrives with its `Location`. The
// String and status clients above are projections of `request_response` in
// the shared core, so the three shapes cannot drift apart.

pub fn almide_http_get_response(url: &str) -> Result<AlmideHttpResponse, String> {
    almide_http_request_response("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_post_response(url: &str, body: &str) -> Result<AlmideHttpResponse, String> {
    almide_http_request_response("POST", url, body, &AlmideMap::new())
}

pub fn almide_http_put_response(url: &str, body: &str) -> Result<AlmideHttpResponse, String> {
    almide_http_request_response("PUT", url, body, &AlmideMap::new())
}

pub fn almide_http_patch_response(url: &str, body: &str) -> Result<AlmideHttpResponse, String> {
    almide_http_request_response("PATCH", url, body, &AlmideMap::new())
}

pub fn almide_http_delete_response(url: &str) -> Result<AlmideHttpResponse, String> {
    almide_http_request_response("DELETE", url, "", &AlmideMap::new())
}

pub fn almide_http_request_response(method: &str, url: &str, body: &str, headers: &AlmideMap<String, String>) -> Result<AlmideHttpResponse, String> {
    request_response(method, url, body, &header_pairs(headers))
        .map(|(status, headers, body)| AlmideHttpResponse { status, body, headers })
}

// ── Binary client ──
//
// Mirrors the String client but returns the raw response body as `Vec<u8>`
// (Almide `Bytes`) with NO UTF-8 conversion, so binary payloads (images,
// audio — e.g. TTS mp3/wav) survive byte-identical.

pub fn almide_http_get_bytes(url: &str) -> Result<Vec<u8>, String> {
    almide_http_request_bytes("GET", url, "", &AlmideMap::new())
}

pub fn almide_http_request_bytes(method: &str, url: &str, body: &str, headers: &AlmideMap<String, String>) -> Result<Vec<u8>, String> {
    request_bytes(method, url, body, &header_pairs(headers))
}



// ── Streaming request ──
//
// Like almide_http_request but delivers the response body to a callback
// in chunks as they arrive on the wire. Designed for Server-Sent Events
// (text/event-stream) where a single HTTP response carries many small
// "data: ..." records over time. Handles both `Transfer-Encoding: chunked`
// (the common SSE shape) and plain bodies.
//
// The callback receives raw UTF-8 substrings of the body — it is the
// caller's job to do SSE line splitting / parsing / event assembly.

pub fn almide_http_request_stream(
    method: &str,
    url: &str,
    body: &str,
    headers: &AlmideMap<String, String>,
    mut on_chunk: impl FnMut(String),
) -> Result<(), String> {
    let (is_https, host, port, path) = parse_url(url)?;
    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    // Long read timeout — SSE responses can be quiet between events.
    stream.set_read_timeout(client_read_timeout(120)).ok();

    let mut wrap = |s: &str| on_chunk(s.to_string());
    if is_https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls = make_tls_stream(&host, stream)?;
            http_exchange_stream(&mut tls, method, &host, &path, body, headers, &mut wrap)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("HTTPS streaming not supported on WASM target".to_string())
        }
    } else {
        let mut s = stream;
        http_exchange_stream(&mut s, method, &host, &path, body, headers, &mut wrap)
    }
}

fn http_exchange_stream<S: Read + Write, F: FnMut(&str)>(
    stream: &mut S,
    method: &str,
    host: &str,
    path: &str,
    body: &str,
    headers: &AlmideMap<String, String>,
    on_chunk: &mut F,
) -> Result<(), String> {
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, host);
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            req.push_str("Content-Type: application/json\r\n");
        }
    }
    for (k, v) in headers.iter() {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).map_err(|e| format!("write failed: {}", e))?;

    let mut buf = vec![0u8; 8192];
    let mut acc: Vec<u8> = Vec::new();
    let mut headers_done = false;
    let mut chunked = false;
    let mut chunk_remaining: usize = 0;
    let mut awaiting_size = true;
    let mut error_status: Option<String> = None;

    loop {
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                if acc.is_empty() && !headers_done {
                    return Err(read_error_msg(&e));
                }
                break;
            }
        };
        if n == 0 {
            break;
        }
        acc.extend_from_slice(&buf[..n]);

        if !headers_done {
            if let Some(idx) = acc.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_section = String::from_utf8_lossy(&acc[..idx]).to_string();
                let status_line = header_section.lines().next().unwrap_or("");
                let code: i64 = status_line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                if !(200..300).contains(&code) {
                    error_status = Some(format!(
                        "HTTP {}: {}",
                        code,
                        status_line.splitn(3, ' ').nth(2).unwrap_or("")
                    ));
                }
                chunked = header_section
                    .to_lowercase()
                    .contains("transfer-encoding: chunked");
                acc.drain(..idx + 4);
                headers_done = true;
            } else {
                continue;
            }
        }

        if let Some(ref msg) = error_status {
            // Drain remaining body for error message context.
            let body_text = String::from_utf8_lossy(&acc).to_string();
            return Err(format!("{}: {}", msg, body_text.chars().take(500).collect::<String>()));
        }

        if chunked {
            'outer: loop {
                if awaiting_size {
                    // Look for \r\n that terminates the size line.
                    let mut nl = None;
                    for i in 0..acc.len().saturating_sub(1) {
                        if acc[i] == b'\r' && acc[i + 1] == b'\n' {
                            nl = Some(i);
                            break;
                        }
                    }
                    let nl = match nl {
                        Some(i) => i,
                        None => break 'outer, // need more bytes
                    };
                    let size_line = String::from_utf8_lossy(&acc[..nl]);
                    let size_str = size_line.split(';').next().unwrap_or("").trim();
                    let size = usize::from_str_radix(size_str, 16).unwrap_or(0);
                    acc.drain(..nl + 2);
                    if size == 0 {
                        return Ok(());
                    }
                    chunk_remaining = size;
                    awaiting_size = false;
                }
                let take = chunk_remaining.min(acc.len());
                if take > 0 {
                    let drained: Vec<u8> = acc.drain(..take).collect();
                    let s = String::from_utf8_lossy(&drained);
                    on_chunk(&s);
                    chunk_remaining -= take;
                }
                if chunk_remaining == 0 {
                    if acc.len() < 2 {
                        // Need the trailing CRLF; wait for more bytes.
                        break 'outer;
                    }
                    if acc.starts_with(b"\r\n") {
                        acc.drain(..2);
                    }
                    awaiting_size = true;
                } else {
                    break 'outer;
                }
            }
        } else {
            // Plain body — surface as-is.
            if !acc.is_empty() {
                let drained: Vec<u8> = acc.drain(..).collect();
                let s = String::from_utf8_lossy(&drained);
                on_chunk(&s);
            }
        }
    }

    if !headers_done {
        return Err("connection closed before headers received".to_string());
    }
    Ok(())
}


// ── HTTP Server ──

pub fn almide_http_serve(port: i64, handler: std::rc::Rc<dyn Fn(AlmideHttpRequest) -> Result<AlmideHttpResponse, String>>) -> Result<(), String> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .map_err(|e| format!("bind failed: {}", e))?;

    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let req = match parse_request(&mut stream) { Ok(r) => r, Err(_) => continue };
        let resp = match handler(req) {
            Ok(r) => r,
            Err(e) => AlmideHttpResponse::new(500, format!("Internal error: {}", e)),
        };
        let _ = write_response(&mut stream, &resp);
    }
    Ok(())
}

// Handler-as-closure wrapper for `@intrinsic` migration of `http.serve`.
// The Almide side passes a `(Request) -> Response` closure; this wrapper
// composes it with `Ok(...)` so the inner `almide_http_serve` keeps its
// `Result<Response, String>` contract (future error-in-handler support).
pub fn almide_rt_http_serve(
    port: i64,
    handler: std::rc::Rc<dyn Fn(AlmideHttpRequest) -> Result<AlmideHttpResponse, String>>,
) -> Result<(), String> {
    // #1055: the handler slot is effect-typed, so the checker already hands
    // us the carrier shape — a handler `Err` becomes the listener loop's 500.
    almide_http_serve(port, handler)
}

// ── Helpers ──





/// Percent-decode an `application/x-www-form-urlencoded` component: `+` → space,
/// `%XX` → the raw byte 0xXX. Bytes are gathered first then interpreted as UTF-8
/// (lossy) so multi-byte sequences like `%E7%8C%AB` (猫) round-trip. Malformed or
/// truncated `%` escapes pass through verbatim.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_request(stream: &mut TcpStream) -> Result<AlmideHttpRequest, String> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).map_err(|e| e.to_string())?;
    let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
    if parts.len() < 2 { return Err("invalid request".into()); }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() { break; }
        if let Some(idx) = trimmed.find(':') {
            let key = trimmed[..idx].trim().to_string();
            let val = trimmed[idx+1..].trim().to_string();
            if key.eq_ignore_ascii_case("content-length") {
                content_length = val.parse().unwrap_or(0);
            }
            headers.push((key, val));
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 { reader.read_exact(&mut body).ok(); }

    Ok(AlmideHttpRequest { method, path, body: String::from_utf8_lossy(&body).to_string(), headers })
}

fn write_response(stream: &mut TcpStream, resp: &AlmideHttpResponse) -> Result<(), String> {
    let status_text = match resp.status {
        200 => "OK", 201 => "Created", 204 => "No Content",
        301 => "Moved Permanently", 302 => "Found", 304 => "Not Modified",
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden",
        404 => "Not Found", 405 => "Method Not Allowed",
        500 => "Internal Server Error", _ => "OK",
    };
    let mut out = format!("HTTP/1.1 {} {}\r\n", resp.status, status_text);
    for (k, v) in &resp.headers { out.push_str(&format!("{}: {}\r\n", k, v)); }
    out.push_str(&format!("Content-Length: {}\r\n\r\n", resp.body.len()));
    out.push_str(&resp.body);
    stream.write_all(out.as_bytes()).map_err(|e| e.to_string())
}

