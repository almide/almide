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
pub fn http_server_next(listener: &std::net::TcpListener) -> (std::net::TcpStream, HttpServerRequest) {
    loop {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(_) => continue,
        };
        if let Ok(req) = http_server_read_request(&mut stream) {
            return (stream, req);
        }
    }
}

fn http_server_read_request(stream: &mut std::net::TcpStream) -> Result<HttpServerRequest, String> {
    let mut reader = std::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut first_line = String::new();
    std::io::BufRead::read_line(&mut reader, &mut first_line).map_err(|e| e.to_string())?;
    let parts: Vec<&str> = first_line.trim().split_whitespace().collect();
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
