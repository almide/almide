// The route of a client request (#2828 audit): URL parsing (#2821), the
// request head and its injection checks (#2822, client half), proxies
// (#2819), the no-limits dial and size cap (#2825) and TLS trust (#2820).
// `include!`d by http_client_core.rs — the same splice discipline applies:
// no `use` lines (the includer's `std::io::{Read, Write}` and
// `std::net::TcpStream` are in scope), everything else fully qualified.

// ── The request URL (#2821) ──
//
// Split the way curl, Go's net/url and the WHATWG URL standard split an
// http(s) URL, and refuse what they refuse instead of guessing: the scheme
// is case-insensitive and must be http or https (a missing one is an error,
// never plain HTTP by default), the authority ends at the first `/`, `?` or
// `#`, userinfo is everything before its LAST `@`, an IPv6 host is bracketed,
// a port is 1..=65535 (an empty one means the default), an internationalised
// host goes out as punycode, the fragment is never sent, and bytes a request
// line cannot carry are percent-encoded. Every refusal names the URL and the
// part that is wrong, before anything is dialled.

/// A request URL as the wire needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct AlmideHttpUrl {
    pub https: bool,
    /// The host to dial and to name in TLS: a lowercased (punycoded) name,
    /// or an IP literal WITHOUT brackets.
    pub host: String,
    pub port: u16,
    /// The `Host` header: the host (an IPv6 literal bracketed) plus `:port`
    /// when the port is not the scheme's default.
    pub authority: String,
    /// The origin-form request target: the path (at least `/`) and the
    /// query, percent-encoded, without the fragment.
    pub target: String,
    /// The percent-decoded `user` and `password` of the userinfo, if any.
    pub userinfo: Option<(Vec<u8>, Vec<u8>)>,
}

impl AlmideHttpUrl {
    /// `host:port` with the port always present (CONNECT's authority-form).
    pub fn host_port(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn http_url_error(url: &str, why: &str) -> String {
    format!("invalid URL {:?}: {}", url, why)
}

/// Parse an http / https URL (#2821). `Err` names the URL and the problem.
pub fn http_parse_url(url: &str) -> Result<AlmideHttpUrl, String> {
    let bad = |why: String| http_url_error(url, &why);
    let is_scheme = |s: &str| {
        s.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic())
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
    };
    let (scheme, rest) = match url.find(':') {
        Some(i) if is_scheme(&url[..i]) && url[i + 1..].starts_with("//") => {
            (url[..i].to_ascii_lowercase(), &url[i + 3..])
        }
        Some(i) if url[..i].eq_ignore_ascii_case("http") || url[..i].eq_ignore_ascii_case("https") => {
            return Err(bad(format!("expected \"//\" after \"{}:\"", &url[..i])));
        }
        _ => return Err(bad("missing scheme (expected http:// or https://)".to_string())),
    };
    let https = match scheme.as_str() {
        "http" => false,
        "https" => true,
        other => return Err(bad(format!("unsupported scheme {:?} (only http:// and https:// are supported)", other))),
    };
    let default_port: u16 = if https { 443 } else { 80 };
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(auth_end);
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    let (host, port_text) = if let Some(v6) = hostport.strip_prefix('[') {
        let close = v6.find(']').ok_or_else(|| bad("unterminated IPv6 address (missing \"]\")".to_string()))?;
        let addr: std::net::Ipv6Addr =
            v6[..close].parse().map_err(|_| bad(format!("invalid IPv6 address {:?}", &v6[..close])))?;
        let after = &v6[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(bad(format!("unexpected {:?} after the IPv6 address", after))),
        };
        (addr.to_string(), port)
    } else {
        match hostport.split_once(':') {
            Some((_, p)) if p.contains(':') => {
                return Err(bad("an IPv6 address must be written in brackets, e.g. http://[::1]:8080/".to_string()));
            }
            Some((h, p)) => (http_normalize_host(h).map_err(bad)?, Some(p)),
            None => (http_normalize_host(hostport).map_err(bad)?, None),
        }
    };
    let port = match port_text {
        None | Some("") => default_port,
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.bytes().all(|b| b.is_ascii_digit()) => n,
            _ => return Err(bad(format!("invalid port {:?} (expected a number from 1 to 65535)", p))),
        },
    };
    let bracketed = if host.contains(':') { format!("[{}]", host) } else { host.clone() };
    let authority = if port == default_port { bracketed } else { format!("{}:{}", bracketed, port) };
    let userinfo = userinfo.filter(|u| !u.is_empty()).map(|u| {
        let (user, pass) = u.split_once(':').unwrap_or((u, ""));
        (http_percent_decode_bytes(user), http_percent_decode_bytes(pass))
    });
    let tail = tail.split('#').next().unwrap_or("");
    let (path, query) = match tail.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (tail, None),
    };
    let mut target = if path.is_empty() { "/".to_string() } else { http_percent_encode(path, false) };
    if let Some(q) = query {
        target.push('?');
        target.push_str(&http_percent_encode(q, true));
    }
    Ok(AlmideHttpUrl { https, host, port, authority, target, userinfo })
}

/// A registered host name, lowercased; a non-ASCII label becomes its
/// `xn--` punycode form (RFC 3492 — the IDNA mapping is the lowercasing only,
/// not the full UTS #46 table).
fn http_normalize_host(host: &str) -> Result<String, String> {
    if host.is_empty() {
        return Err("empty host".to_string());
    }
    let labels: Vec<&str> = host.split('.').collect();
    let mut out: Vec<String> = Vec::with_capacity(labels.len());
    for (i, label) in labels.iter().enumerate() {
        if label.is_empty() {
            if i + 1 == labels.len() && i > 0 {
                out.push(String::new()); // the trailing dot of a fully-qualified name
                continue;
            }
            return Err(format!("empty label in host {:?}", host));
        }
        let lower: Vec<char> = label.chars().flat_map(char::to_lowercase).collect();
        if let Some(c) = lower.iter().find(|c| c.is_ascii() && !(c.is_ascii_alphanumeric() || **c == '-' || **c == '_')) {
            return Err(format!("invalid character {:?} in host {:?}", c, host));
        }
        if lower.iter().all(char::is_ascii) {
            out.push(lower.into_iter().collect());
        } else {
            let encoded = http_punycode(&lower).ok_or_else(|| format!("host {:?} cannot be punycoded", host))?;
            out.push(format!("xn--{}", encoded));
        }
    }
    Ok(out.join("."))
}

/// RFC 3492 punycode of one label (without the `xn--` prefix).
fn http_punycode(input: &[char]) -> Option<String> {
    const BASE: u32 = 36;
    const TMIN: u32 = 1;
    const TMAX: u32 = 26;
    fn digit(d: u32) -> char {
        if d < 26 { (b'a' + d as u8) as char } else { (b'0' + (d - 26) as u8) as char }
    }
    fn adapt(delta: u32, points: u32, first: bool) -> u32 {
        let mut delta = if first { delta / 700 } else { delta / 2 };
        delta += delta / points;
        let mut k = 0;
        while delta > ((BASE - TMIN) * TMAX) / 2 {
            delta /= BASE - TMIN;
            k += BASE;
        }
        k + (BASE - TMIN + 1) * delta / (delta + 38)
    }
    let mut out: String = input.iter().filter(|c| c.is_ascii()).collect();
    let basic = out.len() as u32;
    if basic > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias, mut handled) = (128u32, 0u32, 72u32, basic);
    while (handled as usize) < input.len() {
        let m = input.iter().map(|&c| c as u32).filter(|&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(handled + 1)?)?;
        n = m;
        for &c in input {
            let c = c as u32;
            if c < n {
                delta = delta.checked_add(1)?;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias { TMIN } else if k >= bias + TMAX { TMAX } else { k - bias };
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, handled + 1, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1)?;
        n += 1;
    }
    Some(out)
}

/// Percent-encode what a request target cannot carry: controls, space,
/// non-ASCII and the WHATWG path (or query) percent-encode set. An existing
/// `%XX` is kept as it is.
fn http_percent_encode(s: &str, query: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let encode = b <= b' '
            || b >= 0x7f
            || matches!(b, b'"' | b'<' | b'>')
            || (!query && matches!(b, b'`' | b'{' | b'}'))
            || (query && b == b'\'');
        if encode {
            out.push_str(&format!("%{:02X}", b));
        } else {
            out.push(b as char);
        }
    }
    out
}

fn http_percent_decode_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if let (b'%', Some(h), Some(l)) =
            (b[i], b.get(i + 1).and_then(|&c| hex(c)), b.get(i + 2).and_then(|&c| hex(c)))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Standard base64 with padding (RFC 4648) — the Basic credential encoding.
fn http_base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn http_basic_credentials(user: &[u8], pass: &[u8]) -> String {
    let mut cred = user.to_vec();
    cred.push(b':');
    cred.extend_from_slice(pass);
    format!("Basic {}", http_base64(&cred))
}

// ── The request head (#2822, client half) ──
//
// A header name must be an RFC 9110 token and a value may not hold CR, LF,
// NUL or any other control but HTAB — what Go (httpguts), Node
// (`ERR_INVALID_CHAR`) and Python (`Invalid header value`) refuse. The
// method must be a token too. A refused request is an `Err` naming the part,
// returned before anything is dialled: never sanitised and sent.

fn http_is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

/// Refuse a method or header that would let the caller's text split the
/// request.
pub fn http_check_request(method: &str, headers: &[(String, String)]) -> Result<(), String> {
    if method.is_empty() || !method.bytes().all(http_is_tchar) {
        return Err(format!("invalid HTTP method {:?}: a method is a token (RFC 9110) — no spaces, controls or line breaks", method));
    }
    for (k, v) in headers {
        if k.is_empty() || !k.bytes().all(http_is_tchar) {
            return Err(format!(
                "invalid header name {:?}: a field name is a token (RFC 9110) — no spaces, colons, controls or line breaks",
                k
            ));
        }
        if v.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(format!(
                "invalid header value for {:?}: it contains CR, LF, NUL or another control character, which would split the request",
                k
            ));
        }
    }
    Ok(())
}

/// Everything a request needs before it is dialled: the URL, parsed, and the
/// method and headers, checked. `http.start` answers the same `Err` here,
/// synchronously.
pub fn http_prepare(method: &str, url: &str, headers: &[(String, String)]) -> Result<AlmideHttpUrl, String> {
    let u = http_parse_url(url)?;
    http_check_request(method, headers)?;
    Ok(u)
}

/// How the request reaches the origin once the connection is open: the
/// request-target to write.
#[derive(Debug, Clone)]
pub struct AlmideHttpRoute {
    pub target: String,
}

/// The request head + body: `Host` (with the port when it is not the
/// default), `Connection: close`, `Authorization: Basic` from the URL's
/// userinfo unless the caller sent an `Authorization`, a `Content-Length` whenever a body rides along (plus a
/// default JSON `Content-Type` unless the caller named one), then the
/// caller's headers in map order. A caller's `Host` replaces ours, as in
/// curl. ONE writer for every client shape, so the wire request cannot drift
/// between them. The headers must have passed `http_check_request`.
pub fn http_request_bytes(
    method: &str,
    u: &AlmideHttpUrl,
    route: &AlmideHttpRoute,
    body: &str,
    headers: &[(String, String)],
) -> Vec<u8> {
    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    let mut req = format!("{} {} HTTP/1.1\r\n", method, route.target);
    if !has("host") {
        req.push_str(&format!("Host: {}\r\n", u.authority));
    }
    req.push_str("Connection: close\r\n");
    if let (Some((user, pass)), false) = (&u.userinfo, has("authorization")) {
        req.push_str(&format!("Authorization: {}\r\n", http_basic_credentials(user, pass)));
    }
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
        if !has("content-type") {
            req.push_str("Content-Type: application/json\r\n");
        }
    }
    for (k, v) in headers.iter() {
        req.push_str(&format!("{}: {}\r\n", k, v));
    }
    req.push_str("\r\n");
    req.push_str(body);
    req.into_bytes()
}
