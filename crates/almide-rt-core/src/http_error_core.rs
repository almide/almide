// The client's error classes and their ONE renderer (ADR-0023 §4.2, C-328,
// C-370). `include!`d by http_client_core.rs — the splice discipline
// applies: no `use` lines, everything fully qualified.
//
// Every client failure the program can observe is put in a class before it
// is written, and the text of a class is built from the pieces below and
// nothing else: the URL (or header name) the program passed, quoted by
// `http_quote`, and, for too-large, the limit. No text carries an OS errno
// or a library's wording, so the same failure reads the same on native,
// the embedded lane and the stock p3 component — whose shim
// (crates/almide-wasm-run/src/wasi_p3_http_err.rs) emits its texts FROM
// these constants and twins `http_quote` byte for byte. What is not
// classified (a proxy's answer, a trust store that does not load) keeps its
// own text, which native and the embedded lane share by running this code.
//
//   class          text
//   invalid-url    invalid URL "<url>": <reason>
//   refused-header invalid header name "<name>": … / invalid header value
//                  for "<name>": … / forbidden header name "<name>": …
//   dns            cannot resolve the host of "<url>"
//   connect        could not connect to "<url>" (connection refused or unreachable)
//   timeout        timed out waiting for "<url>" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)
//   tls            TLS handshake with "<url>" failed
//   too-large      response from "<url>" is larger than <n> bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)
//   protocol       malformed or incomplete response from "<url>"

/// A transport failure's class. The request-side classes (invalid-url,
/// refused-header) are rendered where they are found, from the pieces below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpErrorClass {
    Dns,
    Connect,
    Timeout,
    Tls,
    Protocol,
}

/// `(head, tail)` of a transport class: the text is `head`, the quoted URL,
/// `tail`.
pub fn http_error_parts(class: HttpErrorClass) -> (&'static str, &'static str) {
    match class {
        HttpErrorClass::Dns => ("cannot resolve the host of ", ""),
        HttpErrorClass::Connect => ("could not connect to ", " (connection refused or unreachable)"),
        HttpErrorClass::Timeout => ("timed out waiting for ", " (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)"),
        HttpErrorClass::Tls => ("TLS handshake with ", " failed"),
        HttpErrorClass::Protocol => ("malformed or incomplete response from ", ""),
    }
}

/// too-large: head, the quoted URL, middle, the limit in decimal, tail.
pub const HTTP_ERR_TOO_LARGE: [&str; 3] =
    ["response from ", " is larger than ", " bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)"];

/// invalid-url: `HTTP_ERR_URL_HEAD`, the quoted URL, `": "`, the reason.
pub const HTTP_ERR_URL_HEAD: &str = "invalid URL ";
/// The scheme reasons — the three the p3 shim finds itself.
pub const HTTP_ERR_URL_NO_SCHEME: &str = "missing scheme (expected http:// or https://)";
/// `expected "//" after "<scheme>:"` — the scheme as written.
pub const HTTP_ERR_URL_SLASHES: [&str; 2] = ["expected \"//\" after \"", ":\""];
/// `unsupported scheme "<scheme>" (…)` — the scheme lowercased (a scheme is
/// letters, digits, `+`, `-` and `.`, so quoting it adds only the quotes).
pub const HTTP_ERR_URL_SCHEME: [&str; 2] = ["unsupported scheme \"", "\" (only http:// and https:// are supported)"];

/// refused-header: `(head, tail)` around the quoted field name.
pub const HTTP_ERR_HEADER_NAME: (&str, &str) =
    ("invalid header name ", ": a field name is a token (RFC 9110) — no spaces, colons, controls or line breaks");
pub const HTTP_ERR_HEADER_VALUE: (&str, &str) = (
    "invalid header value for ",
    ": it contains CR, LF, NUL or another control character, which would split the request",
);
pub const HTTP_ERR_HEADER_FORBIDDEN: (&str, &str) = ("forbidden header name ", ": the HTTP client manages this field");

/// The fields the client (or the host under it) manages, refused by name,
/// ASCII case-insensitively, on every lane (C-370): wasmtime-wasi-http's
/// DEFAULT_FORBIDDEN_HEADERS, which a stock p3 host refuses anyway.
pub const HTTP_FORBIDDEN_HEADERS: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "transfer-encoding",
    "upgrade",
    "host",
    "http2-settings",
];

/// The p3 shim's text for a host error it has no class for:
/// head, the quoted URL, middle, the wasi:http case name, tail.
pub const HTTP_ERR_UNCLASSIFIED: [&str; 3] = ["http request to ", " failed (", ")"];

/// Quote `s` in double quotes: `"` and `\` are escaped, TAB, CR, LF and NUL
/// read `\t` `\r` `\n` `\0`, every other ASCII control (and DEL) reads
/// `\u{<hex>}` in lowercase hex, and every other byte is copied. Equal to
/// Rust's `{:?}` on ASCII text; the p3 shim implements the same bytes.
pub fn http_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The text of a transport failure of `class` on a request to `url`.
pub fn http_error_text(class: HttpErrorClass, url: &str) -> String {
    let (head, tail) = http_error_parts(class);
    format!("{}{}{}", head, http_quote(url), tail)
}

/// The text of a response past the `max`-byte limit.
pub fn http_too_large_text(url: &str, max: usize) -> String {
    let [head, mid, tail] = HTTP_ERR_TOO_LARGE;
    format!("{}{}{}{}{}", head, http_quote(url), mid, max, tail)
}

/// An io error on an open connection: a timeout, or a broken exchange.
pub fn http_io_error_text(url: &str, e: &std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
        http_error_text(HttpErrorClass::Timeout, url)
    } else {
        http_error_text(HttpErrorClass::Protocol, url)
    }
}

/// Why a dial failed: the name did not resolve, or no address answered.
#[derive(Debug)]
pub enum HttpDialError {
    Resolve,
    Connect(std::io::Error),
}

/// The text of a failed dial to the origin of `url`.
pub fn http_dial_error_text(url: &str, e: &HttpDialError) -> String {
    match e {
        HttpDialError::Resolve => http_error_text(HttpErrorClass::Dns, url),
        HttpDialError::Connect(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
            http_error_text(HttpErrorClass::Timeout, url)
        }
        HttpDialError::Connect(_) => http_error_text(HttpErrorClass::Connect, url),
    }
}
