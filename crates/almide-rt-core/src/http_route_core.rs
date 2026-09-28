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
    /// The URL as the program passed it: the operand every error names.
    pub url: String,
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
    format!("{}{}: {}", HTTP_ERR_URL_HEAD, http_quote(url), why)
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
            let [head, tail] = HTTP_ERR_URL_SLASHES;
            return Err(bad(format!("{}{}{}", head, &url[..i], tail)));
        }
        _ => return Err(bad(HTTP_ERR_URL_NO_SCHEME.to_string())),
    };
    let https = match scheme.as_str() {
        "http" => false,
        "https" => true,
        other => {
            let [head, tail] = HTTP_ERR_URL_SCHEME;
            return Err(bad(format!("{}{}{}", head, other, tail)));
        }
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
            v6[..close].parse().map_err(|_| bad(format!("invalid IPv6 address {}", http_quote(&v6[..close]))))?;
        let after = &v6[close + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(bad(format!("unexpected {} after the IPv6 address", http_quote(after)))),
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
            _ => return Err(bad(format!("invalid port {} (expected a number from 1 to 65535)", http_quote(p)))),
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
    Ok(AlmideHttpUrl { https, host, port, authority, target, userinfo, url: url.to_string() })
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
            return Err(format!("empty label in host {}", http_quote(host)));
        }
        let lower: Vec<char> = label.chars().flat_map(char::to_lowercase).collect();
        if let Some(c) = lower.iter().find(|c| c.is_ascii() && !(c.is_ascii_alphanumeric() || **c == '-' || **c == '_')) {
            return Err(format!("invalid character {} in host {}", http_quote(&c.to_string()), http_quote(host)));
        }
        if lower.iter().all(char::is_ascii) {
            out.push(lower.into_iter().collect());
        } else {
            let encoded = http_punycode(&lower).ok_or_else(|| format!("host {} cannot be punycoded", http_quote(host)))?;
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
/// request, or a header the client manages (C-370): per header, the name,
/// then the value, then the name against `HTTP_FORBIDDEN_HEADERS` — the
/// order the p3 shim checks in, so the first refusal is the same text.
pub fn http_check_request(method: &str, headers: &[(String, String)]) -> Result<(), String> {
    if method.is_empty() || !method.bytes().all(http_is_tchar) {
        return Err(format!(
            "invalid HTTP method {}: a method is a token (RFC 9110) — no spaces, controls or line breaks",
            http_quote(method)
        ));
    }
    for (k, v) in headers {
        let refuse = |(head, tail): (&str, &str)| format!("{}{}{}", head, http_quote(k), tail);
        if k.is_empty() || !k.bytes().all(http_is_tchar) {
            return Err(refuse(HTTP_ERR_HEADER_NAME));
        }
        if v.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
            return Err(refuse(HTTP_ERR_HEADER_VALUE));
        }
        if HTTP_FORBIDDEN_HEADERS.iter().any(|f| f.eq_ignore_ascii_case(k)) {
            return Err(refuse(HTTP_ERR_HEADER_FORBIDDEN));
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
/// request-target to write (origin-form, or absolute-form through an HTTP
/// forward proxy) and the `Proxy-Authorization` that proxy wants.
#[derive(Debug, Clone)]
pub struct AlmideHttpRoute {
    pub target: String,
    pub proxy_auth: Option<String>,
}

/// The request head + body: `Host` (with the port when it is not the
/// default), `Connection: close`, `Authorization: Basic` from the URL's
/// userinfo unless the caller sent an `Authorization`, the proxy's
/// credentials, a `Content-Length` whenever a body rides along (plus a
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
    if let (Some(auth), false) = (&route.proxy_auth, has("proxy-authorization")) {
        req.push_str(&format!("Proxy-Authorization: {}\r\n", auth));
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

// ── Proxies (#2819) ──
//
// Read the way curl and reqwest read them. For an https:// URL the first of
// HTTPS_PROXY, https_proxy; for http:// the first of HTTP_PROXY, http_proxy
// (HTTP_PROXY is skipped when REQUEST_METHOD is set — under CGI a client's
// `Proxy:` header arrives as that variable, httpoxy / CVE-2016-5385, which Go
// and reqwest guard the same way); then ALL_PROXY, all_proxy for either.
// NO_PROXY / no_proxy: comma-separated; `*` bypasses everything; an IP or a
// CIDR block matches an address host; a name matches itself and its
// subdomains (a leading `.` or `*.` is ignored). No implicit exclusions (curl
// and reqwest proxy localhost too unless NO_PROXY says otherwise).
//
// A proxy URL without a scheme is http://. http:// proxies tunnel https with
// CONNECT and take http requests in absolute-form; socks5:// (local name
// resolution) and socks5h:// (the proxy resolves) are RFC 1928 with RFC 1929
// username/password. Userinfo in the proxy URL is the proxy's credential
// (Proxy-Authorization: Basic, or the SOCKS5 login). Any other proxy scheme
// is an error naming the variable, never a silent direct connection — the
// point of a proxy variable is often that the direct route is closed.

#[derive(Debug, Clone, PartialEq)]
enum AlmideHttpProxyKind {
    Http,
    Socks5 { remote_dns: bool },
}

#[derive(Debug, Clone)]
struct AlmideHttpProxy {
    kind: AlmideHttpProxyKind,
    host: String,
    port: u16,
    userinfo: Option<(Vec<u8>, Vec<u8>)>,
    /// `host:port (from VAR)` — for error messages; never the credentials.
    shown: String,
}

fn http_env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The proxy the environment names for `u`, if any.
fn http_proxy_for(u: &AlmideHttpUrl) -> Result<Option<AlmideHttpProxy>, String> {
    let cgi = std::env::var_os("REQUEST_METHOD").is_some();
    let names: [&str; 4] =
        if u.https { ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"] } else { ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] };
    let Some((var, raw)) = names
        .iter()
        .filter(|n| !(cgi && **n == "HTTP_PROXY"))
        .find_map(|n| http_env(n).map(|v| (*n, v)))
    else {
        return Ok(None);
    };
    if http_no_proxy_matches(&u.host) {
        return Ok(None);
    }
    let spec = if raw.contains("://") { raw } else { format!("http://{}", raw) };
    let (scheme, rest) = spec.split_once("://").unwrap_or(("http", &spec));
    let kind = match scheme.to_ascii_lowercase().as_str() {
        "http" => AlmideHttpProxyKind::Http,
        "socks5" => AlmideHttpProxyKind::Socks5 { remote_dns: false },
        "socks5h" => AlmideHttpProxyKind::Socks5 { remote_dns: true },
        other => {
            return Err(format!(
                "{} names a proxy with the scheme {:?}; only http://, socks5:// and socks5h:// proxies are supported",
                var, other
            ));
        }
    };
    // Parsed as an http URL for its authority; the error never repeats the
    // value, which may carry a password.
    let p = http_parse_url(&format!("http://{}", rest))
        .map_err(|e| format!("{} is not a valid proxy URL: {}", var, e.split_once(": ").map(|x| x.1).unwrap_or("")))?;
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("");
    let explicit_port = if hostport.starts_with('[') { hostport.contains("]:") } else { hostport.contains(':') };
    // A SOCKS proxy without a port is on 1080, as in curl.
    let port = if matches!(kind, AlmideHttpProxyKind::Socks5 { .. }) && !explicit_port { 1080 } else { p.port };
    let shown = if p.host.contains(':') { format!("[{}]:{} (from {})", p.host, port, var) } else { format!("{}:{} (from {})", p.host, port, var) };
    Ok(Some(AlmideHttpProxy { kind, host: p.host, port, userinfo: p.userinfo, shown }))
}

fn http_no_proxy_matches(host: &str) -> bool {
    let Some(list) = http_env("NO_PROXY").or_else(|| http_env("no_proxy")) else {
        return false;
    };
    let host = host.trim_end_matches('.');
    let ip: Option<std::net::IpAddr> = host.parse().ok();
    list.split(',').map(str::trim).filter(|e| !e.is_empty()).any(|entry| {
        if entry == "*" {
            return true;
        }
        let e = entry.to_ascii_lowercase();
        if let Some((net, bits)) = e.split_once('/') {
            let net = net.trim_start_matches('[').trim_end_matches(']');
            return match (ip, net.parse::<std::net::IpAddr>(), bits.parse::<u32>()) {
                (Some(ip), Ok(net), Ok(bits)) => http_cidr_contains(net, bits, ip),
                _ => false,
            };
        }
        // Drop a `:port` (and the brackets of an IPv6 entry).
        let bare = if let Some(v6) = e.strip_prefix('[') {
            v6.split(']').next().unwrap_or("").to_string()
        } else if e.matches(':').count() == 1 {
            e.split(':').next().unwrap_or("").to_string()
        } else {
            e.clone()
        };
        if let Ok(entry_ip) = bare.parse::<std::net::IpAddr>() {
            return ip == Some(entry_ip);
        }
        if ip.is_some() {
            return false; // a name entry never matches an address host
        }
        let name = bare.trim_start_matches('*').trim_start_matches('.').trim_end_matches('.');
        !name.is_empty() && (host == name || host.ends_with(&format!(".{}", name)))
    })
}

fn http_cidr_contains(net: std::net::IpAddr, bits: u32, ip: std::net::IpAddr) -> bool {
    match (net, ip) {
        (std::net::IpAddr::V4(n), std::net::IpAddr::V4(a)) if bits <= 32 => {
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            (u32::from(n) & mask) == (u32::from(a) & mask)
        }
        (std::net::IpAddr::V6(n), std::net::IpAddr::V6(a)) if bits <= 128 => {
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            (u128::from(n) & mask) == (u128::from(a) & mask)
        }
        _ => false,
    }
}

/// Open the connection a request to `u` travels on: straight to the origin,
/// or through the proxy the environment names (tunnelled for https and for
/// SOCKS). `dial` opens and configures one TCP connection and names its own
/// failures; the handshakes below read under the timeouts it set.
fn http_open_route(
    u: &AlmideHttpUrl,
    dial: &mut dyn FnMut(&str, u16) -> Result<TcpStream, String>,
) -> Result<(TcpStream, AlmideHttpRoute), String> {
    let direct = AlmideHttpRoute { target: u.target.clone(), proxy_auth: None };
    let Some(p) = http_proxy_for(u)? else {
        return Ok((dial(&u.host, u.port)?, direct));
    };
    let mut s = dial(&p.host, p.port).map_err(|e| format!("{} (proxy {})", e, p.shown))?;
    match p.kind {
        AlmideHttpProxyKind::Socks5 { remote_dns } => {
            http_socks5_connect(&mut s, u, &p, remote_dns)?;
            Ok((s, direct))
        }
        AlmideHttpProxyKind::Http if u.https => {
            http_proxy_tunnel(&mut s, u, &p)?;
            Ok((s, direct))
        }
        AlmideHttpProxyKind::Http => {
            let proxy_auth = p.userinfo.as_ref().map(|(user, pass)| http_basic_credentials(user, pass));
            Ok((s, AlmideHttpRoute { target: format!("http://{}{}", u.authority, u.target), proxy_auth }))
        }
    }
}

fn http_proxy_io(p: &AlmideHttpProxy, e: &std::io::Error) -> String {
    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) {
        format!("proxy {}: timed out waiting for the proxy", p.shown)
    } else {
        format!("proxy {}: {}", p.shown, e)
    }
}

/// `CONNECT host:port` through an HTTP proxy (RFC 9110 §9.3.6); any 2xx
/// opens the tunnel. The answer is read a byte at a time so nothing past its
/// blank line is consumed.
fn http_proxy_tunnel(s: &mut TcpStream, u: &AlmideHttpUrl, p: &AlmideHttpProxy) -> Result<(), String> {
    let hp = u.host_port();
    let mut req = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n", hp, hp);
    if let Some((user, pass)) = &p.userinfo {
        req.push_str(&format!("Proxy-Authorization: {}\r\n", http_basic_credentials(user, pass)));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).map_err(|e| http_proxy_io(p, &e))?;
    let mut head: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match s.read(&mut byte) {
            Ok(0) => return Err(format!("proxy {} closed the connection before answering CONNECT {}", p.shown, hp)),
            Ok(_) => head.push(byte[0]),
            Err(e) => return Err(http_proxy_io(p, &e)),
        }
        if head.len() > HTTP_MAX_HEAD_BYTES {
            return Err(format!("proxy {}: the answer to CONNECT is too large", p.shown));
        }
    }
    let text = String::from_utf8_lossy(&head);
    let line = text.lines().next().unwrap_or("").trim();
    let code: i64 = line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    if (200..300).contains(&code) {
        Ok(())
    } else {
        Err(format!("proxy {} refused CONNECT {}: {}", p.shown, hp, line))
    }
}

/// The SOCKS5 greeting, optional RFC 1929 login and CONNECT (RFC 1928).
fn http_socks5_connect(s: &mut TcpStream, u: &AlmideHttpUrl, p: &AlmideHttpProxy, remote_dns: bool) -> Result<(), String> {
    let fail = |why: &str| format!("proxy {}: SOCKS5 {}", p.shown, why);
    let io = |e: std::io::Error| http_proxy_io(p, &e);
    let greeting: &[u8] = if p.userinfo.is_some() { &[5, 2, 0, 2] } else { &[5, 1, 0] };
    s.write_all(greeting).map_err(io)?;
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply).map_err(io)?;
    match (reply[0], reply[1], &p.userinfo) {
        (5, 0, _) => {}
        (5, 2, Some((user, pass))) => {
            if user.len() > 255 || pass.len() > 255 {
                return Err(fail("credentials are longer than 255 bytes"));
            }
            let mut login = vec![1, user.len() as u8];
            login.extend_from_slice(user);
            login.push(pass.len() as u8);
            login.extend_from_slice(pass);
            s.write_all(&login).map_err(io)?;
            let mut ok = [0u8; 2];
            s.read_exact(&mut ok).map_err(io)?;
            if ok[1] != 0 {
                return Err(fail("login refused"));
            }
        }
        (5, 0xff, _) => return Err(fail("accepts none of the offered authentication methods")),
        _ => return Err(fail("gave an unexpected greeting")),
    }
    let mut req = vec![5, 1, 0];
    let ip: Option<std::net::IpAddr> = if remote_dns {
        u.host.parse().ok()
    } else {
        let addrs = std::net::ToSocketAddrs::to_socket_addrs(&(u.host.as_str(), u.port))
            .map_err(|_| http_error_text(HttpErrorClass::Dns, &u.url))?;
        let mut addrs: Vec<std::net::SocketAddr> = addrs.collect();
        addrs.sort_by_key(|a| a.is_ipv6());
        Some(addrs.first().ok_or_else(|| http_error_text(HttpErrorClass::Dns, &u.url))?.ip())
    };
    match ip {
        Some(std::net::IpAddr::V4(a)) => {
            req.push(1);
            req.extend_from_slice(&a.octets());
        }
        Some(std::net::IpAddr::V6(a)) => {
            req.push(4);
            req.extend_from_slice(&a.octets());
        }
        None => {
            if u.host.len() > 255 {
                return Err(fail("host name is longer than 255 bytes"));
            }
            req.push(3);
            req.push(u.host.len() as u8);
            req.extend_from_slice(u.host.as_bytes());
        }
    }
    req.extend_from_slice(&u.port.to_be_bytes());
    s.write_all(&req).map_err(io)?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).map_err(io)?;
    if head[1] != 0 {
        let why = match head[1] {
            1 => "general failure",
            2 => "connection not allowed by ruleset",
            3 => "network unreachable",
            4 => "host unreachable",
            5 => "connection refused",
            6 => "TTL expired",
            7 => "command not supported",
            8 => "address type not supported",
            _ => "unknown error",
        };
        return Err(fail(&format!("refused CONNECT {}: {}", u.host_port(), why)));
    }
    let rest = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut n = [0u8; 1];
            s.read_exact(&mut n).map_err(io)?;
            n[0] as usize + 2
        }
        _ => return Err(fail("answered with an unknown address type")),
    };
    let mut bound = vec![0u8; rest];
    s.read_exact(&mut bound).map_err(io)?;
    Ok(())
}

// ── Limits (#2825) ──
//
// The calls that take no limits (get / request / *_status / *_bytes /
// *_response / request_stream) bound the dial by the same
// ALMIDE_HTTP_TIMEOUT_SECS that bounds their reads (default 30 s, Go's
// Dialer default; 0 = none), and the buffered ones stop reading past
// ALMIDE_HTTP_MAX_RESPONSE_BYTES (default 1 GiB of response as received;
// 0 = no cap). A response head (every path, the call handle included) is
// capped at 1 MiB, Go's MaxResponseHeaderBytes. There is no default total
// deadline — none of curl, Go, Python, Node or reqwest sets one — `http.start`
// with `total_ms` is the way to ask for one.

/// The largest response head (status line + fields) any path accepts.
pub const HTTP_MAX_HEAD_BYTES: usize = 1 << 20;

/// The response size cap: 1 GiB unless ALMIDE_HTTP_MAX_RESPONSE_BYTES
/// overrides it; `0` means no cap.
pub fn client_max_response_bytes() -> Option<usize> {
    match std::env::var("ALMIDE_HTTP_MAX_RESPONSE_BYTES").ok().and_then(|v| v.trim().parse::<usize>().ok()) {
        Some(0) => None,
        Some(n) => Some(n),
        None => Some(1 << 30),
    }
}

/// A response head past `HTTP_MAX_HEAD_BYTES` is the protocol class: no
/// server that means to be read sends one. Called by the splice's streaming
/// client (runtime/rs/src/http.rs), which this crate does not compile.
#[allow(dead_code)]
fn http_head_too_large(url: &str) -> String {
    http_error_text(HttpErrorClass::Protocol, url)
}

/// Resolve and dial `host:port`, each address in turn, every attempt bounded
/// by `timeout()` (asked afresh per address, so a wall clock can clip it).
/// A name that resolves to nothing is `Resolve`; otherwise the last
/// address's error.
fn http_dial(
    host: &str,
    port: u16,
    timeout: &dyn Fn() -> Option<std::time::Duration>,
) -> Result<TcpStream, HttpDialError> {
    let addrs = std::net::ToSocketAddrs::to_socket_addrs(&(host, port)).map_err(|_| HttpDialError::Resolve)?;
    let mut last: Option<std::io::Error> = None;
    for addr in addrs {
        let dialed = match timeout() {
            Some(t) => TcpStream::connect_timeout(&addr, t),
            None => TcpStream::connect(addr),
        };
        match dialed {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(e),
        }
    }
    Err(last.map(HttpDialError::Connect).unwrap_or(HttpDialError::Resolve))
}

/// The dial of every call that takes no limits: the proxy route, a connect
/// timeout, and the read (and write) timeout `read_default_secs`, both
/// overridable by ALMIDE_HTTP_TIMEOUT_SECS. A failed dial is `u`'s dns,
/// timeout or connect text (a proxy's dial adds which proxy).
fn http_client_open(u: &AlmideHttpUrl, read_default_secs: u64) -> Result<(TcpStream, AlmideHttpRoute), String> {
    let connect_timeout = client_read_timeout(30);
    let io_timeout = client_read_timeout(read_default_secs);
    http_open_route(u, &mut |host, port| {
        let s = http_dial(host, port, &|| connect_timeout).map_err(|e| http_dial_error_text(&u.url, &e))?;
        s.set_read_timeout(io_timeout).ok();
        s.set_write_timeout(io_timeout).ok();
        Ok(s)
    })
}

// ── TLS trust (#2820) ──
//
// The roots are the bundled webpki set PLUS the platform's trust store, read
// by rustls-native-certs — the crate reqwest's `rustls-tls-native-roots`,
// hyper-rustls and ureq use: the macOS keychain, the Windows store, the
// OpenSSL bundle / directory on Linux (openssl-probe) — or, when
// SSL_CERT_FILE / SSL_CERT_DIR is set, the certificates those name instead
// of the platform store (curl, Python and Go read the same variables). Read
// once per process. A certificate or handshake failure is reported as the
// tls class, not as the write it used to surface through; a trust store the
// variables name but that does not load keeps its own `TLS error: …` text.

#[cfg(not(target_arch = "wasm32"))]
fn http_tls_config() -> Result<std::sync::Arc<rustls::ClientConfig>, String> {
    static CONFIG: std::sync::OnceLock<Result<std::sync::Arc<rustls::ClientConfig>, String>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let native = rustls_native_certs::load_native_certs();
            let named = ["SSL_CERT_FILE", "SSL_CERT_DIR"].into_iter().filter(|v| std::env::var_os(v).is_some()).collect::<Vec<_>>();
            if !named.is_empty() && native.certs.is_empty() {
                let why = native.errors.first().map(|e| e.to_string()).unwrap_or_else(|| "no certificates found".to_string());
                return Err(format!("TLS error: could not load the CA certificates {} names: {}", named.join(" / "), why));
            }
            roots.add_parsable_certificates(native.certs);
            Ok(std::sync::Arc::new(
                rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth(),
            ))
        })
        .clone()
}

/// A failed handshake: the timeout class when the server went quiet, the
/// tls class otherwise — a certificate the roots do not vouch for, a peer
/// that does not speak TLS. A stock p3 host reports every such failure as
/// one `TLS-protocol-error`, so the rustls reason is not part of the text.
#[cfg(not(target_arch = "wasm32"))]
fn http_tls_error(url: &str, e: &std::io::Error) -> String {
    if e.get_ref().is_none_or(|i| i.downcast_ref::<rustls::Error>().is_none())
        && matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
    {
        return http_error_text(HttpErrorClass::Timeout, url);
    }
    http_error_text(HttpErrorClass::Tls, url)
}
