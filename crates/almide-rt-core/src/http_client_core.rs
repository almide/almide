// The HTTP client core (#1715) — ONE definition of the String, status and
// bytes clients, shared verbatim between the splice template
// (runtime/rs/src/http.rs `include!`s this file; the embed resolver inlines
// it) and the embedded host (almide-wasm-run links it). Headers travel as
// `&[(String, String)]` — the AlmideMap-facing wrappers in http.rs convert.
//
// EVERY error text here is a cross-lane observable (C-328): the timeout
// wording names ALMIDE_HTTP_TIMEOUT_SECS (#1561), the tolerant read keeps a
// syntactically complete response on close-without-close_notify (#1592),
// and chunked completeness is judged by the same size-walk the decoder
// performs. Change a string here and every lane changes together — that is
// the point.

// Splice discipline: this text is hoisted into ONE flat module next to
// http.rs's remainder, and the assembler dedups only EXACT `use` lines —
// so the TLS types are written fully qualified (no rustls/Arc imports to
// collide or strand a cfg attribute), and the io/net imports here are the
// COMPLEMENT of http.rs's (which keeps BufRead/BufReader/TcpListener).
use std::io::{Read, Write};
use std::net::TcpStream;

pub fn parse_url(url: &str) -> Result<(bool, String, u16, String), String> {
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
        Some(i) => (&host_port[..i], host_port[i + 1..].parse::<u16>().unwrap_or(default_port)),
        None => (host_port, default_port),
    };
    Ok((is_https, host.to_string(), port, path.to_string()))
}

/// The client read timeout: `default_secs` unless `ALMIDE_HTTP_TIMEOUT_SECS`
/// overrides it; `0` means NO timeout (block until the server answers). A
/// local-LLM endpoint routinely needs 30-120 s before the first byte (#1561).
pub fn client_read_timeout(default_secs: u64) -> Option<std::time::Duration> {
    match std::env::var("ALMIDE_HTTP_TIMEOUT_SECS").ok().and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(0) => None,
        Some(s) => Some(std::time::Duration::from_secs(s)),
        None => Some(std::time::Duration::from_secs(default_secs)),
    }
}

/// A read error message the caller can ACT on: the timeout case names the
/// env var; everything else keeps the original detail.
pub fn read_error_msg(e: &std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
        "read timed out waiting for the server (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)"
            .to_string()
    } else {
        format!("read failed: {}", e)
    }
}

/// Read a full `Connection: close` HTTP response, tolerating a peer that
/// closes without TLS close_notify (#1592). A read error after a
/// SYNTACTICALLY COMPLETE response keeps the data; before completeness it
/// still propagates — a truncated body is never silently returned.
pub fn read_response_tolerant(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&buf[..n]),
            Err(e) => {
                if response_is_complete(&response) {
                    break;
                }
                return Err(read_error_msg(&e));
            }
        }
    }
    Ok(response)
}

/// Is this response whole by ITS OWN framing? Chunked completeness is judged
/// by the same size-walk the decoder performs, never a substring probe.
pub fn response_is_complete(resp: &[u8]) -> bool {
    let Some(idx) = resp.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&resp[..idx]).to_lowercase();
    let body = &resp[idx + 4..];
    if headers.contains("transfer-encoding: chunked") {
        return chunked_body_terminated(body);
    }
    if let Some(cl) = headers
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
    {
        return body.len() >= cl;
    }
    true
}

/// Walk the chunk sizes exactly as `decode_chunked_bytes` does and report
/// whether the terminal 0-chunk was reached.
pub fn chunked_body_terminated(body: &[u8]) -> bool {
    let mut pos = 0usize;
    loop {
        let Some(line_end) = body[pos..].windows(2).position(|w| w == b"\r\n") else {
            return false;
        };
        let size_str = String::from_utf8_lossy(&body[pos..pos + line_end]);
        let Ok(size) = usize::from_str_radix(size_str.trim(), 16) else {
            return false;
        };
        if size == 0 {
            return true;
        }
        pos += line_end + 2 + size;
        if pos > body.len() {
            return false;
        }
        if body[pos..].starts_with(b"\r\n") {
            pos += 2;
        }
    }
}

pub fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;
    while let Some(line_end) = remaining.find("\r\n") {
        let size = usize::from_str_radix(remaining[..line_end].trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let data_start = line_end + 2;
        if data_start + size <= remaining.len() {
            result.push_str(&remaining[data_start..data_start + size]);
            remaining = &remaining[data_start + size..];
            if remaining.starts_with("\r\n") {
                remaining = &remaining[2..];
            }
        } else {
            break;
        }
    }
    result
}

/// Byte-level chunked transfer-decoding (mirrors `decode_chunked` for `Vec<u8>`).
pub fn decode_chunked_bytes(body: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        let line_end = match body[pos..].windows(2).position(|w| w == b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };
        let size_str = String::from_utf8_lossy(&body[pos..line_end]);
        let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let data_start = line_end + 2;
        if data_start + size <= body.len() {
            result.extend_from_slice(&body[data_start..data_start + size]);
            pos = data_start + size;
            if pos + 2 <= body.len() && &body[pos..pos + 2] == b"\r\n" {
                pos += 2;
            }
        } else {
            break;
        }
    }
    result
}

#[cfg(not(target_arch = "wasm32"))]
pub fn make_tls_stream(
    host: &str,
    stream: TcpStream,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, String> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = std::sync::Arc::new(
        rustls::ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth(),
    );
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("invalid DNS name: {}", e))?;
    let conn = rustls::ClientConnection::new(config, server_name)
        .map_err(|e| format!("TLS error: {}", e))?;
    Ok(rustls::StreamOwned::new(conn, stream))
}

/// Perform an HTTP request/response exchange over any Read+Write stream.
pub fn http_exchange(
    stream: &mut (impl Read + Write),
    method: &str,
    host: &str,
    path: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<String, String> {
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

    let response = read_response_tolerant(stream)?;
    let text = String::from_utf8_lossy(&response).to_string();

    if let Some(idx) = text.find("\r\n\r\n") {
        let resp_body = &text[idx + 4..];
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

/// Like `http_exchange`, but also parses the status line and returns
/// `(status_code, body)`. A missing/unparseable status line yields code 0.
pub fn http_exchange_status(
    stream: &mut (impl Read + Write),
    method: &str,
    host: &str,
    path: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(i64, String), String> {
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

    let response = read_response_tolerant(stream)?;
    let text = String::from_utf8_lossy(&response).to_string();

    if let Some(idx) = text.find("\r\n\r\n") {
        let header_section = &text[..idx];
        let resp_body = &text[idx + 4..];
        let status_line = header_section.lines().next().unwrap_or("");
        let code: i64 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body_out = if header_section.to_lowercase().contains("transfer-encoding: chunked") {
            decode_chunked(resp_body)
        } else {
            resp_body.to_string()
        };
        Ok((code, body_out))
    } else {
        Ok((0, text))
    }
}

/// Like `http_exchange` but returns the raw response body bytes — binary
/// payloads are never run through `from_utf8_lossy`.
pub fn http_exchange_bytes(
    stream: &mut (impl Read + Write),
    method: &str,
    host: &str,
    path: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<Vec<u8>, String> {
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

    let response = read_response_tolerant(stream)?;

    if let Some(idx) = response.windows(4).position(|w| w == b"\r\n\r\n") {
        let header_section = String::from_utf8_lossy(&response[..idx]).to_lowercase();
        let resp_body = &response[idx + 4..];
        if header_section.contains("transfer-encoding: chunked") {
            Ok(decode_chunked_bytes(resp_body))
        } else {
            Ok(resp_body.to_vec())
        }
    } else {
        Ok(response)
    }
}

/// The String client: `Ok(body)` for any complete response, `Err` for
/// transport failures (connection / TLS / timeout).
pub fn request(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<String, String> {
    let (is_https, host, port, path) = parse_url(url)?;

    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    stream.set_read_timeout(client_read_timeout(30)).ok();

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

/// The status-preserving client: `(status_code, body)` for ANY complete
/// response — a 404 is `Ok((404, body))`, not an `Err`.
pub fn request_status(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(i64, String), String> {
    let (is_https, host, port, path) = parse_url(url)?;

    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    stream.set_read_timeout(client_read_timeout(30)).ok();

    if is_https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls_stream = make_tls_stream(&host, stream)?;
            http_exchange_status(&mut tls_stream, method, &host, &path, body, headers)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("HTTPS is not supported on WASM target".to_string())
        }
    } else {
        let mut stream = stream;
        http_exchange_status(&mut stream, method, &host, &path, body, headers)
    }
}

/// The binary client: the raw response body as `Vec<u8>`.
pub fn request_bytes(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<Vec<u8>, String> {
    let (is_https, host, port, path) = parse_url(url)?;

    let stream = TcpStream::connect(format!("{}:{}", host, port))
        .map_err(|e| format!("connection failed: {}", e))?;
    stream.set_read_timeout(client_read_timeout(30)).ok();

    if is_https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls_stream = make_tls_stream(&host, stream)?;
            http_exchange_bytes(&mut tls_stream, method, &host, &path, body, headers)
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err("HTTPS is not supported on WASM target".to_string())
        }
    } else {
        let mut stream = stream;
        http_exchange_bytes(&mut stream, method, &host, &path, body, headers)
    }
}
