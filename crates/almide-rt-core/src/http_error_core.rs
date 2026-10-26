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
//   status         HTTP <status>: <the body's first 500 chars> (the streaming calls)

/// The head of a non-2xx refusal (`request_stream` and its streaming twins):
/// `HTTP <status>`, which the caller follows with `: ` and the body. No reason
/// phrase (owner ruling 2026-10-17): wasi:http hands a guest the status code
/// and none, so a leg on it could not spell one, and C-367 keeps the phrase
/// out of its comparison for the same cause.
pub fn http_status_refusal(status: i64) -> String {
    format!("HTTP {status}")
}

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

/// The fields that frame the request body, which the client writes from the
/// body it sends: a caller's copy (ASCII case-insensitive) is DROPPED on
/// every lane, never sent and never refused — the Fetch standard's forbidden
/// request header, and Go's net/http, which ignores a `Content-Length` in
/// `Request.Header`. Sent as given, a length that disagrees with the body is
/// a malformed request natively and `HTTP-request-body-size` under wasi:http.
/// (`transfer-encoding` frames too, but is refused above, C-370.)
pub const HTTP_FRAMING_HEADERS: [&str; 1] = ["content-length"];

/// Whether `name` is one of `HTTP_FRAMING_HEADERS`.
pub fn http_is_framing_header(name: &str) -> bool {
    HTTP_FRAMING_HEADERS.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// The p3 shim's text for a host error it has no class for:
/// head, the quoted URL, middle, the wasi:http case name, tail.
pub const HTTP_ERR_UNCLASSIFIED: [&str; 3] = ["http request to ", " failed (", ")"];

/// Quote `s` in double quotes: `"` and `\` are escaped, and every character
/// C-418 escapes (C0, DEL, C1 and the bidi controls, #4126) is written by
/// that one rule (`almide_rt_errtext_escape`): TAB, CR, LF and NUL read `\t`
/// `\r` `\n` `\0`, the rest `\u{<lowercase hex>}`. Every other character is
/// copied. On ASCII text this is Rust's `{:?}`, and the p3 shim's
/// `$http_quote` writes the same bytes for every text that reaches it (a URL
/// that passed the request checks is ASCII).
pub fn http_quote(s: &str) -> String {
    format!("\"{}\"", almide_rt_errtext_escape(&s.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// What replaces the userinfo password wherever an error quotes a URL
/// (C-328, #4205) — Go net/http's `stripPassword` token.
pub const HTTP_URL_PASSWORD_MASK: &str = "***";

/// The byte span `[from, to)` of `url`'s userinfo password, read on the raw
/// text the program passed, the way `http_parse_url` splits it: the
/// authority starts past a `//` standing at the first `/`, `?` or `#` (at
/// the start of the text when none is a `//`) and ends at the next `/`, `?`
/// or `#`; the userinfo is
/// what precedes the authority's LAST `@`; the password is what follows the
/// userinfo's FIRST `:` — possibly empty. `None` when the userinfo has no
/// `:` (or there is no `@`). The query and fragment are never inside it.
/// The p3 shim's `$http_quote` computes the same span (wasi_p3_http_err.rs).
pub fn http_url_password_span(url: &str) -> Option<(usize, usize)> {
    let start = match url.find(['/', '?', '#']) {
        Some(s) if url[s..].starts_with("//") => s + 2,
        _ => 0,
    };
    let end = url[start..].find(['/', '?', '#']).map_or(url.len(), |i| start + i);
    let at = start + url[start..end].rfind('@')?;
    let colon = start + url[start..at].find(':')?;
    Some((colon + 1, at))
}

/// `url` with its userinfo password (if any) replaced by
/// `HTTP_URL_PASSWORD_MASK`; every other byte is kept — the username,
/// scheme, host, port, path, query and fragment.
pub fn http_redact_url(url: &str) -> String {
    match http_url_password_span(url) {
        Some((from, to)) => format!("{}{}{}", &url[..from], HTTP_URL_PASSWORD_MASK, &url[to..]),
        None => url.to_string(),
    }
}

/// Quote a URL in an error text: `http_quote` of `http_redact_url`. Every
/// error that names the URL the program passed quotes it through here.
pub fn http_quote_url(url: &str) -> String {
    http_quote(&http_redact_url(url))
}

/// The text of a transport failure of `class` on a request to `url`.
pub fn http_error_text(class: HttpErrorClass, url: &str) -> String {
    let (head, tail) = http_error_parts(class);
    format!("{}{}{}", head, http_quote_url(url), tail)
}

/// The text of a response past the `max`-byte limit.
pub fn http_too_large_text(url: &str, max: usize) -> String {
    let [head, mid, tail] = HTTP_ERR_TOO_LARGE;
    format!("{}{}{}{}{}", head, http_quote_url(url), mid, max, tail)
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
