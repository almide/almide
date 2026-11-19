//! The https half of the fetch (ADR-0035 D2): HTTP/1.1 over rustls, in
//! process. The request target is built from a [`GitSource`] only.
//!
//! - Certificates: the platform's roots (`SSL_CERT_FILE` / `SSL_CERT_DIR`
//!   replace them, as for other tools), else the bundled Mozilla roots.
//! - A proxy: `HTTPS_PROXY` / `https_proxy` / `ALL_PROXY` / `all_proxy`
//!   (`http://host:port`), skipped for hosts `NO_PROXY` / `no_proxy` lists.
//!   The proxy only carries the TLS stream (`CONNECT`); the certificate is
//!   still checked against the source's host.
//! - No redirect is followed: a repository that moved is reported with its
//!   new location, so trust is decided on the URL the manifest names.
//! - No credentials are sent.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use crate::git_source::GitSource;

/// Bytes one response body may have: a pack of one package, with room for
/// git's own overhead.
pub const MAX_BODY: usize = 2 * crate::package_tree::MAX_PACKAGE_BYTES as usize;

const TIMEOUT: Duration = Duration::from_secs(60);

/// How an https request failed.
#[derive(Debug)]
pub enum HttpError {
    /// The server answered that the repository is not there for us
    /// (401, 403, 404, 410).
    NotFound(String),
    /// Anything else: no network, TLS, a server error, a malformed answer.
    Failed(String),
}

impl HttpError {
    pub fn text(&self) -> &str {
        match self {
            HttpError::NotFound(s) | HttpError::Failed(s) => s,
        }
    }
}

fn failed(s: impl Into<String>) -> HttpError {
    HttpError::Failed(s.into())
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, HttpError> {
    static CONFIG: std::sync::OnceLock<Result<Arc<rustls::ClientConfig>, String>> = std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            // The same roots as the stdlib http client
            // (almide-rt-core `http_tls_config`, #2820): the bundled webpki
            // set plus the platform store, or what SSL_CERT_FILE /
            // SSL_CERT_DIR name instead of the platform store.
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let native = rustls_native_certs::load_native_certs();
            let named = ["SSL_CERT_FILE", "SSL_CERT_DIR"].into_iter().filter(|v| std::env::var_os(v).is_some()).collect::<Vec<_>>();
            if !named.is_empty() && native.certs.is_empty() {
                let why = native.errors.first().map(|e| e.to_string()).unwrap_or_else(|| "no certificates found".to_string());
                return Err(format!("TLS error: could not load the CA certificates {} names: {}", named.join(" / "), why));
            }
            roots.add_parsable_certificates(native.certs);
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map(|b| Arc::new(b.with_root_certificates(roots).with_no_client_auth()))
                .map_err(|e| format!("TLS setup failed: {e}"))
        })
        .clone()
        .map_err(HttpError::Failed)
}

/// `host:port` of the proxy for `host`, from the environment.
fn proxy_for(host: &str) -> Option<String> {
    let var = |names: &[&str]| names.iter().find_map(|n| std::env::var(n).ok().filter(|v| !v.trim().is_empty()));
    let proxy = var(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"])?;
    if let Some(no) = var(&["NO_PROXY", "no_proxy"]) {
        let skip = no.split(',').map(str::trim).filter(|e| !e.is_empty()).any(|e| {
            let e = e.trim_start_matches('.').to_ascii_lowercase();
            e == "*" || host == e || host.ends_with(&format!(".{e}"))
        });
        if skip {
            return None;
        }
    }
    let rest = proxy.trim().strip_prefix("http://").unwrap_or(proxy.trim());
    let rest = rest.trim_end_matches('/');
    let hostport = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
    Some(if hostport.contains(':') { hostport.to_string() } else { format!("{hostport}:80") })
}

fn connect(addr: &str) -> Result<TcpStream, HttpError> {
    let addrs = addr.to_socket_addrs().map_err(|e| failed(format!("cannot resolve {addr}: {e}")))?;
    let mut last = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, TIMEOUT) {
            Ok(s) => {
                let _ = s.set_read_timeout(Some(TIMEOUT));
                let _ = s.set_write_timeout(Some(TIMEOUT));
                return Ok(s);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(failed(format!("cannot connect to {addr}: {}", last.map_or("no address".to_string(), |e| e.to_string()))))
}

/// One request to the repository `src`: `GET`/`POST` `<repo path><suffix>`.
/// Returns the body of a 200 answer.
pub fn request(src: &GitSource, dot_git: bool, suffix: &str, body: Option<(&str, &[u8])>, protocol_v2: bool) -> Result<Vec<u8>, HttpError> {
    let host = src.host.as_str();
    let port = src.port.unwrap_or(443);
    let authority = if port == 443 { host.to_string() } else { format!("{host}:{port}") };
    let mut tcp = match proxy_for(host) {
        Some(proxy) => {
            let mut s = connect(&proxy)?;
            write!(s, "CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\r\n").map_err(|e| failed(format!("proxy {proxy}: {e}")))?;
            let head = read_head(&mut s).map_err(|e| failed(format!("proxy {proxy}: {e}")))?;
            let status = status_of(&head).map_err(|e| failed(format!("proxy {proxy}: {e}")))?;
            if status != 200 {
                return Err(failed(format!("proxy {proxy} refused CONNECT {host}:{port} ({status})")));
            }
            s
        }
        None => connect(&format!("{host}:{port}"))?,
    };
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| failed(format!("{host}: {e}")))?;
    let conn = rustls::ClientConnection::new(tls_config()?, name).map_err(|e| failed(format!("TLS to {host}: {e}")))?;
    let mut tls = rustls::StreamOwned::new(conn, &mut tcp);
    let target = format!("{}{suffix}", src.remote_path(dot_git));
    let mut req = format!(
        "{} {target} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: git/2.0 (almide {})\r\nAccept: */*\r\nConnection: close\r\n",
        if body.is_some() { "POST" } else { "GET" },
        env!("CARGO_PKG_VERSION"),
    );
    if protocol_v2 {
        req.push_str("Git-Protocol: version=2\r\n");
    }
    if let Some((ctype, b)) = body {
        req.push_str(&format!("Content-Type: {ctype}\r\nContent-Length: {}\r\n", b.len()));
    }
    req.push_str("\r\n");
    let shown = format!("{}{suffix}", src.https_url(dot_git));
    let io = |e: std::io::Error| failed(format!("{shown}: {e}"));
    tls.write_all(req.as_bytes()).map_err(io)?;
    if let Some((_, b)) = body {
        tls.write_all(b).map_err(io)?;
    }
    tls.flush().map_err(io)?;
    let head = read_head(&mut tls).map_err(|e| failed(format!("{shown}: {e}")))?;
    let status = status_of(&head).map_err(|e| failed(format!("{shown}: {e}")))?;
    let header = |name: &str| {
        head.lines().skip(1).find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
    };
    match status {
        200 => {}
        301 | 302 | 303 | 307 | 308 => {
            let to = header("location").map(|l| crate::fetch::pktline::shown(l.as_bytes())).unwrap_or_default();
            return Err(HttpError::NotFound(format!(
                "{shown} answered with a redirect to `{to}`; a redirect is not followed\n  hint: if the repository moved, write its new URL in the manifest"
            )));
        }
        401 | 403 | 404 | 410 => return Err(HttpError::NotFound(format!("{shown}: the server answered {status} (no such repository, or it is private)"))),
        s => return Err(failed(format!("{shown}: the server answered {s}"))),
    }
    if header("content-encoding").is_some_and(|e| !e.eq_ignore_ascii_case("identity")) {
        return Err(failed(format!("{shown}: the server sent a compressed body, which was not asked for")));
    }
    let chunked = header("transfer-encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked"));
    let length = header("content-length").and_then(|l| l.parse::<usize>().ok());
    let body = if chunked {
        read_chunked(&mut tls)
    } else if let Some(n) = length {
        if n > MAX_BODY {
            return Err(failed(format!("{shown}: a body of {n} bytes (max {MAX_BODY})")));
        }
        let mut b = vec![0u8; n];
        tls.read_exact(&mut b).map(|_| b).map_err(|e| e.to_string())
    } else {
        read_to_end_capped(&mut tls)
    }
    .map_err(|e| failed(format!("{shown}: {e}")))?;
    if let Some(ct) = header("content-type") {
        let want = if suffix.contains("info/refs") { "application/x-git-upload-pack-advertisement" } else { "application/x-git-upload-pack-result" };
        if !ct.to_ascii_lowercase().starts_with(want) {
            return Err(HttpError::NotFound(format!("{shown}: not a git smart-HTTP answer (content type `{}`)", crate::fetch::pktline::shown(ct.as_bytes()))));
        }
    }
    Ok(body)
}

fn status_of(head: &str) -> Result<u16, String> {
    let line = head.lines().next().unwrap_or_default();
    let mut parts = line.split_whitespace();
    match (parts.next(), parts.next().and_then(|c| c.parse::<u16>().ok())) {
        (Some(v), Some(c)) if v.starts_with("HTTP/1.") => Ok(c),
        _ => Err(format!("a malformed status line `{}`", crate::fetch::pktline::shown(line.as_bytes()))),
    }
}

/// The status line and headers, up to the blank line (at most 64 KiB).
fn read_head(r: &mut impl Read) -> Result<String, String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match r.read(&mut byte) {
            Ok(0) => return Err("the connection closed before the response head ended".to_string()),
            Ok(_) => head.push(byte[0]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
        if head.len() > 64 * 1024 {
            return Err("a response head over 64 KiB".to_string());
        }
    }
    String::from_utf8(head).map_err(|_| "a response head that is not text".to_string())
}

fn read_line(r: &mut impl Read) -> Result<String, String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while !line.ends_with(b"\r\n") {
        if r.read(&mut byte).map_err(|e| e.to_string())? == 0 {
            return Err("the connection closed inside a chunk header".to_string());
        }
        line.push(byte[0]);
        if line.len() > 1024 {
            return Err("a chunk header over 1 KiB".to_string());
        }
    }
    Ok(String::from_utf8_lossy(&line[..line.len() - 2]).into_owned())
}

fn read_chunked(r: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line = read_line(r)?;
        let size_text = line.split(';').next().unwrap_or("").trim();
        let n = usize::from_str_radix(size_text, 16).map_err(|_| "a malformed chunk size".to_string())?;
        if n == 0 {
            // Trailers, up to the blank line.
            while !read_line(r)?.is_empty() {}
            return Ok(out);
        }
        if out.len() + n > MAX_BODY {
            return Err(format!("a body over {MAX_BODY} bytes"));
        }
        let start = out.len();
        out.resize(start + n, 0);
        r.read_exact(&mut out[start..]).map_err(|e| e.to_string())?;
        if !read_line(r)?.is_empty() {
            return Err("a chunk not followed by CRLF".to_string());
        }
    }
}

fn read_to_end_capped(r: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let n = r.take(MAX_BODY as u64 + 1).read_to_end(&mut out);
    match n {
        Ok(_) if out.len() > MAX_BODY => Err(format!("a body over {MAX_BODY} bytes")),
        Ok(_) => Ok(out),
        // A server that closes without TLS close_notify after a
        // Connection: close body: what arrived is the body.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(out),
        Err(e) => Err(e.to_string()),
    }
}
