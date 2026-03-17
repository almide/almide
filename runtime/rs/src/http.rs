// http extern — Rust native HTTP client/server (platform layer, no external crate)
// Uses std::net::TcpStream for client and TcpListener for server.

// HashMap already imported by prelude
use std::io::{Read, Write, BufRead, BufReader};
use std::net::{TcpStream, TcpListener};

// ── Response type ──

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
    let (host, port, path) = parse_url(url)?;

    let mut stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();

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

// ── Helpers ──

fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url.strip_prefix("http://").unwrap_or(url);
    let (host_port, path) = match url.find('/') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, "/"),
    };
    let (host, port) = match host_port.find(':') {
        Some(i) => (&host_port[..i], host_port[i+1..].parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };
    Ok((host.to_string(), port, path.to_string()))
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

