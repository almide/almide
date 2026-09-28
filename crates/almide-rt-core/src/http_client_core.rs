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

// URL, request head, proxies, dial limits and TLS trust: one include, so
// this file stays under the 1000-line limit.
include!("http_route_core.rs");

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
/// still propagates — a truncated body is never silently returned. More than
/// `max` bytes is an error naming the cap (#2825).
pub fn read_response_tolerant(stream: &mut impl Read, max: Option<usize>) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                response.extend_from_slice(&buf[..n]);
                if let Some(max) = max.filter(|m| response.len() > *m) {
                    return Err(format!(
                        "response too large: more than {} bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)",
                        max
                    ));
                }
            }
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

/// The status code of a head's status line (0 when it has none).
fn http_status_of(head: &[u8]) -> i64 {
    let end = head.windows(2).position(|w| w == b"\r\n").unwrap_or(head.len());
    String::from_utf8_lossy(&head[..end]).split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// Is this status an interim (1xx) answer the final one follows? `101
/// Switching Protocols` is final — the connection is no longer HTTP after it.
fn http_is_interim(status: i64) -> bool {
    (100..200).contains(&status) && status != 101
}

/// The FINAL response head in `resp`, skipping any complete 1xx interim
/// heads before it (`100 Continue`, `103 Early Hints` — RFC 9110 §15.2, what
/// curl, Go and Python do; #2824): `(start, end)` with `end` at its blank
/// line. `None` while no final head is whole.
pub fn http_final_head(resp: &[u8]) -> Option<(usize, usize)> {
    let mut start = 0;
    loop {
        let end = start + resp[start..].windows(4).position(|w| w == b"\r\n\r\n")?;
        if http_is_interim(http_status_of(&resp[start..end])) {
            start = end + 4;
            continue;
        }
        return Some((start, end));
    }
}

/// The field lines of a head (after its status line): wire order, repeats
/// kept, names in their wire spelling.
fn http_head_fields(head: &str) -> Vec<(String, String)> {
    head.lines()
        .skip(1)
        .filter_map(|line| {
            let (k, v) = line.split_once(':')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

fn http_fields_chunked(fields: &[(String, String)]) -> bool {
    fields.iter().any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.to_ascii_lowercase().contains("chunked"))
}

/// Is this response whole by ITS OWN framing? Chunked completeness is judged
/// by the same size-walk the decoder performs, never a substring probe.
pub fn response_is_complete(resp: &[u8]) -> bool {
    let Some((start, idx)) = http_final_head(resp) else {
        return false;
    };
    let fields = http_head_fields(&String::from_utf8_lossy(&resp[start..idx]));
    let body = &resp[idx + 4..];
    if http_fields_chunked(&fields) {
        return chunked_body_terminated(body);
    }
    if let Some(cl) = fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
    {
        return body.len() >= cl;
    }
    true
}

/// The size of a chunk from its size line (RFC 9112 §7.1): the hex digits
/// before any `;` chunk extension, which is ignored (#2824). Anything else is
/// an error — never a silent 0 that would end the body early.
pub fn http_chunk_size(line: &[u8]) -> Result<usize, String> {
    let text = String::from_utf8_lossy(line);
    let size = text.split(';').next().unwrap_or("").trim_matches([' ', '\t']);
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("malformed chunked body: bad chunk-size line {:?}", text));
    }
    usize::from_str_radix(size, 16).map_err(|_| format!("malformed chunked body: chunk size {:?} is too large", size))
}

/// Walk the chunk sizes exactly as `decode_chunked_bytes` does and report
/// whether the terminal 0-chunk was reached.
pub fn chunked_body_terminated(body: &[u8]) -> bool {
    let mut pos = 0usize;
    loop {
        let Some(line_end) = body[pos..].windows(2).position(|w| w == b"\r\n") else {
            return false;
        };
        let Ok(size) = http_chunk_size(&body[pos..pos + line_end]) else {
            return false;
        };
        if size == 0 {
            return true;
        }
        pos = match (pos + line_end + 2).checked_add(size) {
            Some(p) if p <= body.len() => p,
            _ => return false,
        };
        if body[pos..].starts_with(b"\r\n") {
            pos += 2;
        }
    }
}

/// Chunked transfer-decoding. It runs on BYTES, before any text decoding:
/// chunk sizes count bytes, and a multibyte character may straddle two
/// chunks (#2536), so lossily decoding the framed body first would change
/// its length under the size walk. A malformed size line is an error.
pub fn decode_chunked_bytes(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        let line_end = match body[pos..].windows(2).position(|w| w == b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };
        let size = http_chunk_size(&body[pos..line_end])?;
        if size == 0 {
            break;
        }
        let data_start = line_end + 2;
        match data_start.checked_add(size) {
            Some(data_end) if data_end <= body.len() => {
                result.extend_from_slice(&body[data_start..data_end]);
                pos = data_end;
                if pos + 2 <= body.len() && &body[pos..pos + 2] == b"\r\n" {
                    pos += 2;
                }
            }
            _ => break,
        }
    }
    Ok(result)
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

/// A parsed text response: `(status_code, headers, body)`. The header list
/// is EVERY field line in wire order — names keep their wire spelling, and a
/// repeated field (`Set-Cookie`) keeps every occurrence (#1791); the
/// accessors do the case-insensitive matching. A missing / unparseable
/// status line yields code 0 and an empty header list (the whole text is
/// then the body — the same tolerance the body-only client always had).
pub type HttpTextResponse = (i64, Vec<(String, String)>, String);

/// A parsed response with its body still bytes: `(status_code, headers,
/// transfer-decoded body)`.
pub type HttpRawResponse = (i64, Vec<(String, String)>, Vec<u8>);

/// Split a whole response into status, fields and the transfer-decoded body
/// bytes, past any 1xx interim heads. The framing is removed on BYTES; only
/// a text caller then decodes the body (#2536). The head is ASCII by
/// protocol, so reading it lossily is harmless.
pub fn http_parse_response(response: &[u8]) -> Result<HttpRawResponse, String> {
    let Some((start, idx)) = http_final_head(response) else {
        return Ok((0, Vec::new(), response.to_vec()));
    };
    let head = String::from_utf8_lossy(&response[start..idx]);
    let code = http_status_of(&response[start..idx]);
    let fields = http_head_fields(&head);
    let body = &response[idx + 4..];
    let body = if http_fields_chunked(&fields) { decode_chunked_bytes(body)? } else { body.to_vec() };
    Ok((code, fields, body))
}

/// Write the prepared request and read the whole response.
pub fn http_exchange_raw(stream: &mut (impl Read + Write), request: &[u8]) -> Result<Vec<u8>, String> {
    stream.write_all(request).map_err(|e| format!("write failed: {}", e))?;
    read_response_tolerant(stream, client_max_response_bytes())
}

/// The one buffered client every shape projects: prepare (URL, method,
/// headers — refused before any dial), open (proxy, timeouts), TLS, exchange,
/// parse.
fn http_request_raw(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<HttpRawResponse, String> {
    let u = http_prepare(method, url, headers)?;
    let (stream, route) = http_client_open(&u, 30)?;
    let request = http_request_bytes(method, &u, &route, body, headers);
    let response = if u.https {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut tls = make_tls_stream(&u.host, stream)?;
            http_exchange_raw(&mut tls, &request)?
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = stream;
            return Err("HTTPS is not supported on WASM target".to_string());
        }
    } else {
        let mut stream = stream;
        http_exchange_raw(&mut stream, &request)?
    };
    http_parse_response(&response)
}

/// The full-response client (#1791): `(status_code, headers, body)` for ANY
/// complete response — a 404 or a 3xx is `Ok`, with its `Location` in the
/// header list. Redirects are NEVER followed: the 3xx and its `Location` are
/// the answer, so the final URL is always the URL the caller passed. `Err`
/// is a refused request (URL, method, header) or a transport failure
/// (connection / proxy / TLS / timeout / size cap / framing).
pub fn request_response(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<HttpTextResponse, String> {
    http_request_raw(method, url, body, headers).map(|(c, h, b)| (c, h, String::from_utf8_lossy(&b).into_owned()))
}

/// The String client: `Ok(body)` for any complete response, `Err` for
/// transport failures (connection / TLS / timeout) — the body projection of
/// `request_response`.
pub fn request(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<String, String> {
    request_response(method, url, body, headers).map(|(_, _, b)| b)
}

/// The status-preserving client: `(status_code, body)` for ANY complete
/// response — a 404 is `Ok((404, body))`, not an `Err` — the status
/// projection of `request_response`.
pub fn request_status(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<(i64, String), String> {
    request_response(method, url, body, headers).map(|(c, _, b)| (c, b))
}

/// The binary client: the raw response body as `Vec<u8>` — binary payloads
/// are never run through `from_utf8_lossy`.
pub fn request_bytes(
    method: &str,
    url: &str,
    body: &str,
    headers: &[(String, String)],
) -> Result<Vec<u8>, String> {
    http_request_raw(method, url, body, headers).map(|(_, _, b)| b)
}

// The call-handle core (#2631 / #2633) rides the same include chain: inlined
// into http.rs's flat module at embed time, compiled into this module for the
// embedded host.
include!("http_call_core.rs");
