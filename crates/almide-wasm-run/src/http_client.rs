//! The embedded host's HTTP client (#1710 increment 1) — a VERBATIM
//! transcription of the native runtime template's client core
//! (`runtime/rs/src/http.rs`; the only delta is `AlmideMap` headers →
//! `&[(String, String)]`, an identity re-spelling). The template is
//! splice-source, not a linkable crate, so the share is textual; the
//! equality that matters — error texts, timeout wording, the
//! close-without-close_notify tolerance, chunked framing — is pinned by
//! the native⇄embedded cross fixtures, which run BOTH copies on the same
//! probes. A drift here fails those fixtures, never ships silently.
//! Retire this copy when runtime/rs becomes a real crate.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

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
        Some(i) => (&host_port[..i], host_port[i + 1..].parse::<u16>().unwrap_or(default_port)),
        None => (host_port, default_port),
    };
    Ok((is_https, host.to_string(), port, path.to_string()))
}

fn client_read_timeout(default_secs: u64) -> Option<std::time::Duration> {
    match std::env::var("ALMIDE_HTTP_TIMEOUT_SECS").ok().and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(0) => None,
        Some(s) => Some(std::time::Duration::from_secs(s)),
        None => Some(std::time::Duration::from_secs(default_secs)),
    }
}

fn read_error_msg(e: &std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
        "read timed out waiting for the server (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)"
            .to_string()
    } else {
        format!("read failed: {}", e)
    }
}

fn read_response_tolerant(stream: &mut impl Read) -> Result<Vec<u8>, String> {
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

fn response_is_complete(resp: &[u8]) -> bool {
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

fn chunked_body_terminated(body: &[u8]) -> bool {
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

fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;
    loop {
        let line_end = match remaining.find("\r\n") {
            Some(i) => i,
            None => break,
        };
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

fn make_tls_stream(
    host: &str,
    stream: TcpStream,
) -> Result<StreamOwned<ClientConnection, TcpStream>, String> {
    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config =
        Arc::new(ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth());
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("invalid DNS name: {}", e))?;
    let conn = ClientConnection::new(config, server_name).map_err(|e| format!("TLS error: {}", e))?;
    Ok(StreamOwned::new(conn, stream))
}

fn http_exchange(
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

/// The String client: `Ok(body)` for any complete response, `Err` for
/// transport failures — the native `almide_http_request` verbatim.
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
        let mut tls_stream = make_tls_stream(&host, stream)?;
        http_exchange(&mut tls_stream, method, &host, &path, body, headers)
    } else {
        let mut stream = stream;
        http_exchange(&mut stream, method, &host, &path, body, headers)
    }
}
