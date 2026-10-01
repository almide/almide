// The proxy half of a client request's route (#2819): environment
// selection, NO_PROXY, the HTTP CONNECT tunnel and SOCKS5. `include!`d by
// http_route_core.rs — the same splice discipline applies: no `use` lines,
// everything else fully qualified.

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
    let io = |e: std::io::Error| http_proxy_io(p, &e);
    http_socks5_greet(s, p)?;
    let req = http_socks5_request(u, p, remote_dns)?;
    s.write_all(&req).map_err(io)?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).map_err(io)?;
    if head[1] != 0 {
        let why = http_socks5_reply_reason(head[1]);
        return Err(http_socks5_fail(p, &format!("refused CONNECT {}: {}", u.host_port(), why)));
    }
    let rest = match head[3] {
        1 => 4 + 2,
        4 => 16 + 2,
        3 => {
            let mut n = [0u8; 1];
            s.read_exact(&mut n).map_err(io)?;
            n[0] as usize + 2
        }
        _ => return Err(http_socks5_fail(p, "answered with an unknown address type")),
    };
    let mut bound = vec![0u8; rest];
    s.read_exact(&mut bound).map_err(io)?;
    Ok(())
}

fn http_socks5_fail(p: &AlmideHttpProxy, why: &str) -> String {
    format!("proxy {}: SOCKS5 {}", p.shown, why)
}

/// The method negotiation, and the RFC 1929 login when the proxy asks for it.
fn http_socks5_greet(s: &mut TcpStream, p: &AlmideHttpProxy) -> Result<(), String> {
    let fail = |why: &str| http_socks5_fail(p, why);
    let io = |e: std::io::Error| http_proxy_io(p, &e);
    let greeting: &[u8] = if p.userinfo.is_some() { &[5, 2, 0, 2] } else { &[5, 1, 0] };
    s.write_all(greeting).map_err(io)?;
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply).map_err(io)?;
    match (reply[0], reply[1], &p.userinfo) {
        (5, 0, _) => Ok(()),
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
            Ok(())
        }
        (5, 0xff, _) => Err(fail("accepts none of the offered authentication methods")),
        _ => Err(fail("gave an unexpected greeting")),
    }
}

/// The CONNECT request for `u`: an address resolved here, or the host name
/// for the proxy to resolve (`remote_dns`, socks5h).
fn http_socks5_request(u: &AlmideHttpUrl, p: &AlmideHttpProxy, remote_dns: bool) -> Result<Vec<u8>, String> {
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
                return Err(http_socks5_fail(p, "host name is longer than 255 bytes"));
            }
            req.push(3);
            req.push(u.host.len() as u8);
            req.extend_from_slice(u.host.as_bytes());
        }
    }
    req.extend_from_slice(&u.port.to_be_bytes());
    Ok(req)
}

/// The RFC 1928 §6 reply code in words.
fn http_socks5_reply_reason(code: u8) -> &'static str {
    match code {
        1 => "general failure",
        2 => "connection not allowed by ruleset",
        3 => "network unreachable",
        4 => "host unreachable",
        5 => "connection refused",
        6 => "TTL expired",
        7 => "command not supported",
        8 => "address type not supported",
        _ => "unknown error",
    }
}
