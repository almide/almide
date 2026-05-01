// http extern — Rust native HTTP client/server (platform layer)
// Uses std::net::TcpStream for client and TcpListener for server.
// HTTPS via rustls (pure-Rust TLS).

// HashMap already imported by prelude
use std::io::{Read, Write, BufRead, BufReader};
use std::net::{TcpStream, TcpListener};

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use rustls::{ClientConfig, ClientConnection, StreamOwned, RootCertStore};

// ── Response type ──
pub type Response = AlmideHttpResponse;

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
    pub fn with_headers(status: i64, body: String, headers: HashMap<String, String>) -> Self {
        Self { status, body, headers: headers.into_iter().collect() }
    }
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

pub fn almide_rt_http_with_headers(status: i64, body: &str, headers: &HashMap<String, String>) -> AlmideHttpResponse {
    let mut resp = AlmideHttpResponse::new(status, body.to_string());
    for (k, v) in headers {
        resp.headers.retain(|(ek, _)| !ek.eq_ignore_ascii_case(k));
        resp.headers.push((k.clone(), v.clone()));
    }
    resp
}

pub fn almide_http_set_status(mut resp: AlmideHttpResponse, code: i64) -> AlmideHttpResponse {
    resp.status = code; resp
}

pub fn almide_http_get_body(resp: &AlmideHttpResponse) -> String {
    resp.body.clone()
}

pub fn almide_http_set_header(mut resp: AlmideHttpResponse, key: &str, value: &str) -> AlmideHttpResponse {
    resp.headers.retain(|(k, _)| k != key);
    resp.headers.push((key.to_string(), value.to_string()));
    resp
}

pub fn almide_http_get_header(resp: &AlmideHttpResponse, key: &str) -> Option<String> {
    resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
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

pub fn almide_http_query_params(req: &AlmideHttpRequest) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(q) = req.path.split('?').nth(1) {
        for pair in q.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(k.to_string(), v.to_string());
            }
        }
    }
    params
}

// ── HTTP Client ──

pub fn almide_http_get(url: &str) -> Result<String, String> {
    almide_http_request("GET", url, "", &HashMap::new())
}

pub fn almide_http_post(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("POST", url, body, &HashMap::new())
}

pub fn almide_http_put(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("PUT", url, body, &HashMap::new())
}

pub fn almide_http_patch(url: &str, body: &str) -> Result<String, String> {
    almide_http_request("PATCH", url, body, &HashMap::new())
}

pub fn almide_http_delete(url: &str) -> Result<String, String> {
    almide_http_request("DELETE", url, "", &HashMap::new())
}

pub fn almide_http_get_with_headers(url: &str, headers: &HashMap<String, String>) -> Result<String, String> {
    almide_http_request("GET", url, "", headers)
}

pub fn almide_http_request(method: &str, url: &str, body: &str, headers: &HashMap<String, String>) -> Result<String, String> {
    let (is_https, host, port, path) = parse_url(url)?;

    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();

    if is_https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls_stream = make_tls_stream(&host, stream)?;
            http_exchange(&mut tls_stream, method, &host, &path, body, headers)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("HTTPS is not supported on WASM target".to_string())
        }
    } else {
        let mut stream = stream;
        http_exchange(&mut stream, method, &host, &path, body, headers)
    }
}

/// Perform HTTP request/response exchange over any Read+Write stream.
fn http_exchange(stream: &mut (impl Read + Write), method: &str, host: &str, path: &str, body: &str, headers: &HashMap<String, String>) -> Result<String, String> {
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, host);
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            req.push_str("Content-Type: application/json\r\n");
        }
    }
    for (k, v) in headers { req.push_str(&format!("{}: {}\r\n", k, v)); }
    req.push_str("\r\n");
    req.push_str(body);

    stream.write_all(req.as_bytes()).map_err(|e| format!("write failed: {}", e))?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|e| format!("read failed: {}", e))?;
    let text = String::from_utf8_lossy(&response).to_string();

    // Split headers and body
    if let Some(idx) = text.find("\r\n\r\n") {
        let resp_body = &text[idx + 4..];
        // Handle chunked transfer encoding
        let header_section = &text[..idx];
        if header_section.to_lowercase().contains("transfer-encoding: chunked") {
            Ok(decode_chunked(resp_body))
        } else {
            Ok(resp_body.to_string())
        }
    } else {
        Ok(text)
    }
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
    headers: &HashMap<String, String>,
    mut on_chunk: impl FnMut(String),
) -> Result<(), String> {
    let (is_https, host, port, path) = parse_url(url)?;
    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    // Long read timeout — SSE responses can be quiet between events.
    stream.set_read_timeout(Some(std::time::Duration::from_secs(120))).ok();

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
    headers: &HashMap<String, String>,
    on_chunk: &mut F,
) -> Result<(), String> {
    let mut req = format!("{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", method, path, host);
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
        if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            req.push_str("Content-Type: application/json\r\n");
        }
    }
    for (k, v) in headers {
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
                    return Err(format!("read failed: {}", e));
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

// ── TLS ──

#[cfg(not(target_arch = "wasm32"))]
fn make_tls_stream(host: &str, stream: TcpStream) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    );
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("invalid DNS name: {}", e))?;
    let conn = ClientConnection::new(config, server_name).map_err(|e| format!("TLS error: {}", e))?;
    Ok(StreamOwned::new(conn, stream))
}

// ── HTTP Server ──

pub fn almide_http_serve(port: i64, handler: impl Fn(AlmideHttpRequest) -> Result<AlmideHttpResponse, String>) -> Result<(), String> {
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
    handler: impl Fn(AlmideHttpRequest) -> AlmideHttpResponse,
) -> Result<(), String> {
    almide_http_serve(port, move |req| Ok(handler(req)))
}

// ── Helpers ──

fn parse_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (is_https, url) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        (false, url)
    };
    let default_port: u16 = if is_https { 443 } else { 80 };
    let (host_port, path) = match url.find('/') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, "/"),
    };
    let (host, port) = match host_port.find(':') {
        Some(i) => (&host_port[..i], host_port[i+1..].parse::<u16>().unwrap_or(default_port)),
        None => (host_port, default_port),
    };
    Ok((is_https, host.to_string(), port, path.to_string()))
}

fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;
    loop {
        let line_end = match remaining.find("\r\n") { Some(i) => i, None => break };
        let size = usize::from_str_radix(remaining[..line_end].trim(), 16).unwrap_or(0);
        if size == 0 { break; }
        let data_start = line_end + 2;
        if data_start + size <= remaining.len() {
            result.push_str(&remaining[data_start..data_start + size]);
            remaining = &remaining[data_start + size..];
            if remaining.starts_with("\r\n") { remaining = &remaining[2..]; }
        } else { break; }
    }
    result
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
