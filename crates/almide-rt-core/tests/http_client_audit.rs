//! The HTTP client audit (#2828): the shared client core against the
//! behaviour of curl, Go, Python, Node and reqwest, one section per issue.
//!
//! - #2821 the URL: scheme, userinfo, IPv6, `?` without a path, ports,
//!   fragments, unencoded paths, IDN, and a `Host` that carries the port.
//! - #2822 (client half) CR/LF and other controls in header names / values
//!   and the method are refused before anything is sent.
//! - #2820 the trust store: `SSL_CERT_FILE` names a CA generated here, and a
//!   certificate failure reads as the tls class (ADR-0023 §4.2), not as a
//!   failed write.
//! - #2819 proxies: `HTTP_PROXY` (absolute-form), `HTTPS_PROXY` (CONNECT),
//!   `ALL_PROXY` (SOCKS5), `NO_PROXY`, and the `http.start` handle.
//! - #2824 chunk extensions and `100 Continue`.
//! - #2825 the connect timeout and the response size cap.
//!
//! Environment-dependent cases run in a CHILD process (this test binary
//! re-executed on `child_request` with the request in `HTTP_AUDIT_CHILD_SPEC`):
//! the proxy and trust variables are read from the process environment, and
//! setting them in-process would race the other tests (and needs `unsafe`,
//! which the workspace forbids). Every peer, proxy and TLS server is a thread
//! on loopback; nothing leaves the host except the connect-timeout probe,
//! which dials a non-routable address on purpose.

use almide_rt_core::http_client_core as client;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ── peers ──

/// Read one request head (through the blank line) off `s`.
fn read_head(s: &TcpStream) -> String {
    let mut r = BufReader::new(s.try_clone().expect("clone"));
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    head
}

/// A one-connection origin on `listener`: capture the request head, answer
/// `reply`, and send the head back over the channel.
fn origin_on(listener: TcpListener, reply: Vec<u8>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    listener.set_nonblocking(true).expect("a nonblocking accept loop");
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut s = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Err(_) => return,
            }
        };
        s.set_nonblocking(false).expect("a blocking accepted stream");
        s.set_read_timeout(Some(Duration::from_secs(5))).expect("a read timeout on the accepted stream");
        let head = read_head(&s);
        let _ = s.write_all(&reply);
        let _ = tx.send(head);
    });
    rx
}

/// An origin on 127.0.0.1: `(port, captured request head)`.
fn origin(reply: &[u8]) -> (u16, mpsc::Receiver<String>) {
    let (l, port) = loopback();
    (port, origin_on(l, reply.to_vec()))
}

const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";

/// A listener on an ephemeral 127.0.0.1 port, and that port.
fn loopback() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind a loopback listener");
    let port = l.local_addr().expect("a bound listener has an address").port();
    (l, port)
}

fn get(url: &str) -> Result<client::HttpTextResponse, String> {
    client::request_response("GET", url, "", &[])
}

/// `get` that must succeed; the failure names the URL and the error.
fn get_ok(url: &str) -> client::HttpTextResponse {
    get(url).unwrap_or_else(|e| panic!("GET {url}: {e}"))
}

fn response_with(url: &str, headers: &[(String, String)]) -> client::HttpTextResponse {
    client::request_response("GET", url, "", headers).unwrap_or_else(|e| panic!("GET {url}: {e}"))
}

fn bytes_ok(url: &str) -> Vec<u8> {
    client::request_bytes("GET", url, "", &[]).unwrap_or_else(|e| panic!("GET {url} as bytes: {e}"))
}

/// The same GET through the call handle (`http.start` + wait).
fn start_get(url: &str) -> Result<client::HttpTextResponse, String> {
    client::http_call_spawn("GET", url, "", vec![], 5000, 0).and_then(|sh| client::http_call_wait(&sh))
}

fn parsed(url: &str) -> client::AlmideHttpUrl {
    client::http_parse_url(url).unwrap_or_else(|e| panic!("{url} parses: {e}"))
}

/// The next head a peer captured, within 5 s.
fn recv(rx: &mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(5)).expect("the peer captured a request")
}

fn utf8(p: &std::path::Path) -> &str {
    p.to_str().expect("the temp dir path is UTF-8")
}

fn head_of(rx: &mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(10)).expect("the origin was reached")
}

fn request_line(head: &str) -> &str {
    head.lines().next().unwrap_or("")
}

/// A listener nobody should reach: `true` if something connected within
/// `wait`.
fn untouched_listener() -> (u16, impl FnOnce(Duration) -> bool) {
    let (l, port) = loopback();
    l.set_nonblocking(true).expect("a nonblocking accept probe");
    (port, move |wait: Duration| {
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if l.accept().is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    })
}

// ── child processes (environment-dependent cases) ──

/// Run `child_request` in a fresh process with `env` (and every proxy /
/// trust variable of the parent removed). `spec` = `kind|method|url[|Header: value]`.
fn in_child(spec: &str, env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(std::env::current_exe().expect("the test binary path"));
    cmd.args(["--exact", "child_request", "--nocapture", "--test-threads=1"]);
    for v in [
        "HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY",
        "no_proxy", "SSL_CERT_FILE", "SSL_CERT_DIR", "ALMIDE_HTTP_TIMEOUT_SECS", "ALMIDE_HTTP_MAX_RESPONSE_BYTES",
        "REQUEST_METHOD",
    ] {
        cmd.env_remove(v);
    }
    cmd.env("HTTP_AUDIT_CHILD_SPEC", spec);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run child");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    stdout
        .lines()
        .find_map(|l| l.split_once("RESULT:").map(|(_, r)| r))
        .map(str::to_string)
        .unwrap_or_else(|| panic!("child printed no RESULT line\nstdout:\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr)))
}

/// The child side: perform the request `HTTP_AUDIT_CHILD_SPEC` describes and
/// print `RESULT:ok <status> <body>` or `RESULT:err <message>`. A no-op in
/// an ordinary test run.
#[test]
fn child_request() {
    let Ok(spec) = std::env::var("HTTP_AUDIT_CHILD_SPEC") else { return };
    let parts: Vec<&str> = spec.splitn(4, '|').collect();
    let (kind, method, url) = (parts[0], parts[1], parts[2]);
    let headers: Vec<(String, String)> = parts
        .get(3)
        .and_then(|h| h.split_once(": "))
        .map(|(k, v)| vec![(k.to_string(), v.to_string())])
        .unwrap_or_default();
    let started = Instant::now();
    let result = match kind {
        "response" => client::request_response(method, url, "", &headers).map(|(c, _, b)| format!("{c} {b}")),
        "bytes" => client::request_bytes(method, url, "", &headers).map(|b| format!("0 {}", String::from_utf8_lossy(&b))),
        "start" => client::http_call_spawn(method, url, "", headers, 10_000, 0)
            .and_then(|sh| client::http_call_wait(&sh))
            .map(|(c, _, b)| format!("{c} {b}")),
        other => panic!("unknown child kind {other}"),
    };
    let secs = started.elapsed().as_secs_f64();
    match result {
        Ok(s) => println!("RESULT:ok {s}"),
        Err(e) => println!("RESULT:err {e} [after {secs:.1}s]"),
    }
}

// ── #2821: the URL ──

#[test]
fn the_host_header_carries_a_non_default_port() {
    let (port, rx) = origin(OK);
    let r = get_ok(&format!("http://127.0.0.1:{port}/echo"));
    assert_eq!(r.0, 200);
    let head = head_of(&rx);
    assert!(head.contains(&format!("\r\nHost: 127.0.0.1:{port}\r\n")), "{head}");
}

#[test]
fn userinfo_becomes_basic_auth_and_never_the_host() {
    let (port, rx) = origin(OK);
    let r = get_ok(&format!("http://user:p%40ss@127.0.0.1:{port}/echo"));
    assert_eq!(r.0, 200);
    let head = head_of(&rx);
    assert_eq!(request_line(&head), "GET /echo HTTP/1.1");
    // base64("user:p@ss")
    assert!(head.contains("\r\nAuthorization: Basic dXNlcjpwQHNz\r\n"), "{head}");
    assert!(!head.contains("user:"), "{head}");
}

#[test]
fn a_callers_authorization_header_wins_over_userinfo() {
    let (port, rx) = origin(OK);
    let hs = vec![("Authorization".to_string(), "Bearer t".to_string())];
    response_with(&format!("http://u:p@127.0.0.1:{port}/"), &hs);
    let head = head_of(&rx);
    assert!(head.contains("Authorization: Bearer t\r\n") && !head.contains("Basic"), "{head}");
}

#[test]
fn an_ipv6_literal_is_dialled_and_bracketed_in_host() {
    let Ok(l) = TcpListener::bind("[::1]:0") else {
        eprintln!("no IPv6 loopback on this host; skipped");
        return;
    };
    let port = l.local_addr().expect("a bound listener has an address").port();
    let rx = origin_on(l, OK.to_vec());
    let r = get_ok(&format!("http://[::1]:{port}/echo"));
    assert_eq!(r.0, 200);
    assert!(head_of(&rx).contains(&format!("\r\nHost: [::1]:{port}\r\n")));
}

#[test]
fn a_query_without_a_path_keeps_the_port() {
    let (port, rx) = origin(OK);
    let r = get_ok(&format!("http://127.0.0.1:{port}?x=1"));
    assert_eq!(r.0, 200);
    assert_eq!(request_line(&head_of(&rx)), "GET /?x=1 HTTP/1.1");
}

#[test]
fn the_fragment_is_never_sent_and_unsafe_bytes_are_percent_encoded() {
    let (port, rx) = origin(OK);
    get_ok(&format!("http://127.0.0.1:{port}/a b/café?q=a b#frag"));
    assert_eq!(request_line(&head_of(&rx)), "GET /a%20b/caf%C3%A9?q=a%20b HTTP/1.1");
}

#[test]
fn the_scheme_is_case_insensitive() {
    let (port, rx) = origin(OK);
    let r = get_ok(&format!("HTTP://127.0.0.1:{port}/echo"));
    assert_eq!(r.0, 200);
    assert_eq!(request_line(&head_of(&rx)), "GET /echo HTTP/1.1");
}

#[test]
fn bad_urls_are_refused_before_any_connection() {
    let (port, touched) = untouched_listener();
    let cases = [
        ("http://127.0.0.1:99999/echo".to_string(), "invalid port"),
        (format!("http://127.0.0.1:{port}x/echo"), "invalid port"),
        ("http://127.0.0.1:0/".to_string(), "invalid port"),
        (format!("ftp://127.0.0.1:{port}/"), "unsupported scheme \"ftp\""),
        (format!("127.0.0.1:{port}/echo"), "missing scheme"),
        (format!("localhost:{port}/echo"), "missing scheme"),
        ("http:///echo".to_string(), "empty host"),
        ("http://[::1/".to_string(), "IPv6"),
        (format!("http://::1:{port}/"), "IPv6"),
        ("http://exa mple.com/".to_string(), "invalid character"),
    ];
    for (url, want) in &cases {
        let e = get(url).expect_err(url);
        assert!(e.starts_with("invalid URL "), "{url}: {e}");
        assert!(e.contains(want), "{url}: want {want:?} in {e:?}");
        // The call handle refuses the same URL synchronously.
        let e2 = client::http_call_spawn("GET", url, "", vec![], 0, 0).err().expect("start refuses");
        assert_eq!(&e2, &e, "{url}");
    }
    assert!(!touched(Duration::from_millis(200)), "a refused URL still dialled");
}

#[test]
fn an_empty_port_means_the_default_one() {
    let u = parsed("https://example.com:/x");
    assert_eq!((u.port, u.authority.as_str(), u.target.as_str()), (443, "example.com", "/x"));
}

#[test]
fn an_idn_host_goes_out_as_punycode() {
    let u = parsed("https://Bücher.example/");
    assert_eq!(u.host, "xn--bcher-kva.example");
    assert_eq!(u.authority, "xn--bcher-kva.example");
    let u = parsed("http://例え.テスト:8080/");
    assert_eq!(u.authority, "xn--r8jz45g.xn--zckzah:8080");
}

// ── #2822 (client half): header injection ──

#[test]
fn a_crlf_in_a_header_value_is_refused_before_sending() {
    let (port, touched) = untouched_listener();
    let url = format!("http://127.0.0.1:{port}/echo");
    let bad_values = ["a\r\nX-Injected: yes", "a\nb", "a\rb", "a\0b"];
    for v in bad_values {
        let hs = vec![("X-Test".to_string(), v.to_string())];
        let e = client::request_response("GET", &url, "", &hs).expect_err(v);
        assert!(e.starts_with("invalid header value for \"X-Test\""), "{e}");
        let e2 = client::request_bytes("GET", &url, "", &hs).expect_err(v);
        assert_eq!(e2, e);
        let e3 = client::http_call_spawn("GET", &url, "", hs, 0, 0).err().expect("start refuses");
        assert_eq!(e3, e);
    }
    for k in ["X-Test\r\nX-Injected", "X Test", "X:Test", ""] {
        let hs = vec![(k.to_string(), "v".to_string())];
        let e = client::request_response("GET", &url, "", &hs).expect_err(k);
        assert!(e.starts_with("invalid header name"), "{e}");
    }
    for m in ["GET /x HTTP/1.1\r\nX: y\r\n\r\nGET", "GE T", ""] {
        let e = client::request_response(m, &url, "", &[]).expect_err(m);
        assert!(e.starts_with("invalid HTTP method"), "{e}");
    }
    assert!(!touched(Duration::from_millis(200)), "an injected request still dialled");
}

#[test]
fn a_header_the_client_manages_is_refused_by_name_in_any_case() {
    // C-370: the nine names a stock p3 host forbids are refused on native
    // and the embedded lane too, before anything is dialled.
    let (port, touched) = untouched_listener();
    let url = format!("http://127.0.0.1:{port}/echo");
    for k in ["Host", "connection", "Keep-Alive", "TRANSFER-ENCODING", "upgrade", "Proxy-Authorization", "http2-settings"] {
        let hs = vec![(k.to_string(), "x".to_string())];
        let e = client::request_response("GET", &url, "", &hs).expect_err(k);
        assert_eq!(e, format!("forbidden header name \"{k}\": the HTTP client manages this field"));
        let e2 = client::http_call_spawn("GET", &url, "", hs, 0, 0).err().expect("start refuses");
        assert_eq!(e2, e);
    }
    // The name is checked before the value, the value before the managed set.
    let hs = vec![("Host".to_string(), "a\nb".to_string())];
    let e = client::request_response("GET", &url, "", &hs).unwrap_err();
    assert!(e.starts_with("invalid header value for \"Host\""), "{e}");
    assert!(!touched(Duration::from_millis(200)), "a managed header still dialled");
}

#[test]
fn the_error_quote_escapes_what_rust_debug_escapes_in_ascii() {
    for s in ["plain", "q\"uote", "back\\slash", "t\tr\rn\n0\0", "\u{1b}[m", "del\u{7f}", "héllo"] {
        let want = if s.is_ascii() { format!("{s:?}") } else { format!("\"{s}\"") };
        assert_eq!(client::http_quote(s), want, "{s:?}");
    }
}

#[test]
fn ordinary_headers_still_go_out() {
    let (port, rx) = origin(OK);
    let hs = vec![("X-Test".to_string(), "a\tb: c; d=\"e\"".to_string())];
    response_with(&format!("http://127.0.0.1:{port}/"), &hs);
    assert!(head_of(&rx).contains("\r\nX-Test: a\tb: c; d=\"e\"\r\n"));
}

// ── #2824: chunk extensions and 1xx ──

#[test]
fn a_chunk_extension_does_not_empty_the_body() {
    let reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;name=v\r\nhello\r\n6 ; x\r\n world\r\n0\r\n\r\n";
    let (port, _rx) = origin(reply);
    let r = get_ok(&format!("http://127.0.0.1:{port}/"));
    assert_eq!((r.0, r.2.as_str()), (200, "hello world"));
    let (port, _rx) = origin(reply);
    assert_eq!(bytes_ok(&format!("http://127.0.0.1:{port}/")), b"hello world");
    let (port, _rx) = origin(reply);
    let r = start_get(&format!("http://127.0.0.1:{port}/")).expect("the started call is answered");
    assert_eq!((r.0, r.2.as_str()), (200, "hello world"));
}

#[test]
fn a_malformed_chunk_size_is_an_error_not_an_empty_body() {
    let reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nhello\r\n0\r\n\r\n";
    let (port, _rx) = origin(reply);
    let e = get(&format!("http://127.0.0.1:{port}/")).unwrap_err();
    assert_eq!(e, format!("malformed or incomplete response from \"http://127.0.0.1:{port}/\""));
    let (port, _rx) = origin(reply);
    let e = start_get(&format!("http://127.0.0.1:{port}/")).expect_err("the started call fails");
    assert_eq!(e, format!("malformed or incomplete response from \"http://127.0.0.1:{port}/\""));
}

#[test]
fn a_100_continue_is_skipped_for_the_final_response() {
    let reply = b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 103 Early Hints\r\nLink: </a>\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-Final: 1\r\n\r\nok";
    let (port, _rx) = origin(reply);
    let r = get_ok(&format!("http://127.0.0.1:{port}/"));
    assert_eq!((r.0, r.2.as_str()), (200, "ok"));
    assert!(r.1.iter().any(|(k, _)| k == "X-Final") && !r.1.iter().any(|(k, _)| k == "Link"));
    let (port, _rx) = origin(reply);
    assert_eq!(bytes_ok(&format!("http://127.0.0.1:{port}/")), b"ok");
    let (port, _rx) = origin(reply);
    let r = start_get(&format!("http://127.0.0.1:{port}/")).expect("the started call is answered");
    assert_eq!((r.0, r.2.as_str()), (200, "ok"));
}

// ── #2825: limits ──

#[test]
fn the_connect_timeout_follows_almide_http_timeout_secs() {
    // 10.255.255.1 is non-routable: the SYN goes unanswered, so only a
    // connect timeout ends the dial (reqwest's connect_timeout test uses the
    // same address). A network that answers "unreachable" at once proves
    // nothing either way, so that outcome is reported and not judged.
    let t = Instant::now();
    let out = in_child("response|GET|http://10.255.255.1:81/", &[("ALMIDE_HTTP_TIMEOUT_SECS", "2")]);
    let secs = t.elapsed().as_secs_f64();
    if out.starts_with("err") && !out.contains("timed out") && secs < 1.5 {
        eprintln!("the network refused 10.255.255.1 at once ({out}); the timeout is not observable here");
        return;
    }
    assert!(secs < 15.0, "the dial ran {secs:.1}s past a 2 s connect timeout: {out}");
    let want = "err timed out waiting for \"http://10.255.255.1:81/\" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)";
    assert!(out.starts_with(want), "{out}");
}

#[test]
fn the_response_size_is_capped() {
    let big = format!("HTTP/1.1 200 OK\r\nContent-Length: 5000\r\n\r\n{}", "x".repeat(5000));
    let (port, _rx) = origin(big.as_bytes());
    let out = in_child(&format!("response|GET|http://127.0.0.1:{port}/"), &[("ALMIDE_HTTP_MAX_RESPONSE_BYTES", "1000")]);
    let want = format!(
        "err response from \"http://127.0.0.1:{port}/\" is larger than 1000 bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)"
    );
    assert!(out.starts_with(&want), "{out}");
    // Under the cap it is answered whole.
    let (port, _rx) = origin(big.as_bytes());
    let out = in_child(&format!("bytes|GET|http://127.0.0.1:{port}/"), &[("ALMIDE_HTTP_MAX_RESPONSE_BYTES", "10000")]);
    assert_eq!(out, format!("ok 0 {}", "x".repeat(5000)));
}

// ── #2820: the trust store ──

/// A CA and a leaf for `names` signed by it: the CA's PEM and a server
/// config that presents the leaf.
fn tls_material(names: &[&str]) -> Result<(String, rustls::ServerConfig), Box<dyn std::error::Error>> {
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
    let ca_key = KeyPair::generate()?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new())?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name.push(rcgen::DnType::CommonName, "almide audit test CA");
    let ca = ca_params.self_signed(&ca_key)?;
    let leaf_key = KeyPair::generate()?;
    let leaf_params = CertificateParams::new(names.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
    let issuer = rcgen::Issuer::from_params(&ca_params, &ca_key);
    let leaf = leaf_params.signed_by(&leaf_key, &issuer)?;
    let chain = vec![leaf.der().clone()];
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into());
    let cfg = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(chain, key)?;
    Ok((ca.pem(), cfg))
}

/// Answer one TLS request on `tcp` with `ok`.
fn serve_tls_ok(cfg: std::sync::Arc<rustls::ServerConfig>, mut tcp: TcpStream) {
    tcp.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let Ok(mut conn) = rustls::ServerConnection::new(cfg) else { return };
    let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
    let mut buf = Vec::new();
    let mut b = [0u8; 1024];
    while !buf.ends_with(b"\r\n\r\n") {
        match tls.read(&mut b) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&b[..n]),
        }
    }
    if buf.ends_with(b"\r\n\r\n") {
        let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        tls.conn.send_close_notify();
        let _ = tls.flush();
    }
}

/// A CA and a leaf for `names` signed by it; the TLS server answers one
/// request with `ok` per connection. Returns (port, CA PEM path).
fn tls_origin(names: &[&str], connections: usize) -> (u16, std::path::PathBuf) {
    let (ca_pem, cfg) = tls_material(names).expect("generate the test CA, leaf and server config");
    // One directory per CA: SSL_CERT_DIR reads every file in it, and the
    // tests run in parallel.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("almide-audit-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the CA directory");
    let ca_path = dir.join("ca.pem");
    std::fs::write(&ca_path, ca_pem).expect("write the CA PEM");

    let cfg = std::sync::Arc::new(cfg);
    let (l, port) = loopback();
    thread::spawn(move || {
        for _ in 0..connections {
            let Ok((tcp, _)) = l.accept() else { return };
            serve_tls_ok(cfg.clone(), tcp);
        }
    });
    (port, ca_path)
}

#[test]
fn ssl_cert_file_is_trusted_and_a_certificate_failure_reads_as_tls() {
    let (port, ca) = tls_origin(&["localhost"], 3);
    let url = format!("https://localhost:{port}/");
    let out = in_child(&format!("response|GET|{url}"), &[("SSL_CERT_FILE", utf8(&ca))]);
    assert_eq!(out, "ok 200 ok");
    let out = in_child(&format!("start|GET|{url}"), &[("SSL_CERT_FILE", utf8(&ca))]);
    assert_eq!(out, "ok 200 ok");
    // Without it the test CA is unknown — and that is the tls class.
    let out = in_child(&format!("response|GET|{url}"), &[]);
    assert!(out.starts_with(&format!("err TLS handshake with \"{url}\" failed")), "{out}");
}

#[test]
fn ssl_cert_dir_is_trusted() {
    let (port, ca) = tls_origin(&["localhost"], 1);
    let dir = utf8(ca.parent().expect("the CA PEM sits in its own directory")).to_string();
    let out = in_child(&format!("response|GET|https://localhost:{port}/"), &[("SSL_CERT_DIR", &dir)]);
    assert_eq!(out, "ok 200 ok");
}

#[test]
fn an_unreadable_ssl_cert_file_is_named() {
    let (port, _ca) = tls_origin(&["localhost"], 1);
    let out = in_child(&format!("response|GET|https://localhost:{port}/"), &[("SSL_CERT_FILE", "/nonexistent/ca.pem")]);
    assert!(out.starts_with("err TLS error: ") && out.contains("SSL_CERT_FILE"), "{out}");
}

// ── #2819: proxies ──

/// Relay a CONNECT tunnel between `s` and 127.0.0.1:`dest` until both
/// directions close.
fn tunnel(s: TcpStream, dest: u16) -> std::io::Result<()> {
    let up = TcpStream::connect(("127.0.0.1", dest))?;
    let (mut a, mut b) = (s.try_clone()?, up.try_clone()?);
    let (mut c, mut d) = (s, up);
    let t = thread::spawn(move || {
        let _ = std::io::copy(&mut a, &mut b);
        let _ = b.shutdown(std::net::Shutdown::Write);
    });
    let _ = std::io::copy(&mut d, &mut c);
    let _ = c.shutdown(std::net::Shutdown::Write);
    let _ = t.join();
    Ok(())
}

/// A forward proxy that answers every request itself with its own request
/// line as the body, and a CONNECT proxy that tunnels to `tunnel_to`. Each
/// accepted request head is sent over the channel. Handles `n` connections.
fn proxy(n: usize, tunnel_to: Option<u16>) -> (u16, mpsc::Receiver<String>) {
    let (l, port) = loopback();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut s, _)) = l.accept() else { return };
            s.set_read_timeout(Some(Duration::from_secs(10))).ok();
            let head = read_head(&s);
            let line = request_line(&head).to_string();
            let _ = tx.send(head.clone());
            if line.starts_with("CONNECT ") {
                let Some(dest) = tunnel_to else {
                    let _ = s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
                    continue;
                };
                let _ = s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
                let _ = tunnel(s, dest);
            } else {
                let body = format!("via proxy: {line}");
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes());
            }
        }
    });
    (port, rx)
}

#[test]
fn http_proxy_gets_the_absolute_form_with_proxy_authorization() {
    let (pp, rx) = proxy(1, None);
    let proxy_url = format!("http://pu:pw@127.0.0.1:{pp}");
    let out = in_child("response|GET|http://origin.invalid:8080/x?y=1", &[("HTTP_PROXY", &proxy_url)]);
    assert_eq!(out, "ok 200 via proxy: GET http://origin.invalid:8080/x?y=1 HTTP/1.1");
    let head = recv(&rx);
    assert!(head.contains("\r\nHost: origin.invalid:8080\r\n"), "{head}");
    assert!(head.contains("\r\nProxy-Authorization: Basic cHU6cHc=\r\n"), "{head}");
}

#[test]
fn lowercase_http_proxy_and_all_proxy_are_read_and_start_uses_them() {
    let (pp, _rx) = proxy(3, None);
    let proxy_url = format!("127.0.0.1:{pp}"); // no scheme = http://, as curl and reqwest read it
    let out = in_child("response|GET|http://a.invalid/", &[("http_proxy", &proxy_url)]);
    assert_eq!(out, "ok 200 via proxy: GET http://a.invalid/ HTTP/1.1");
    let out = in_child("bytes|GET|http://b.invalid/", &[("ALL_PROXY", &proxy_url)]);
    assert_eq!(out, "ok 0 via proxy: GET http://b.invalid/ HTTP/1.1");
    let out = in_child("start|GET|http://c.invalid/", &[("HTTP_PROXY", &proxy_url)]);
    assert_eq!(out, "ok 200 via proxy: GET http://c.invalid/ HTTP/1.1");
}

#[test]
fn https_proxy_tunnels_with_connect_on_every_path() {
    let (tls_port, ca) = tls_origin(&["secure.invalid"], 2);
    let (pp, rx) = proxy(2, Some(tls_port));
    let env = [("HTTPS_PROXY", format!("http://127.0.0.1:{pp}")), ("SSL_CERT_FILE", utf8(&ca).to_string())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let out = in_child("response|GET|https://secure.invalid/", &env);
    assert_eq!(out, "ok 200 ok");
    assert_eq!(request_line(&recv(&rx)), "CONNECT secure.invalid:443 HTTP/1.1");
    let out = in_child("start|GET|https://secure.invalid/", &env);
    assert_eq!(out, "ok 200 ok");
    assert_eq!(request_line(&recv(&rx)), "CONNECT secure.invalid:443 HTTP/1.1");
}

#[test]
fn a_refused_connect_names_the_proxy_answer() {
    let (pp, _rx) = proxy(1, None);
    let out = in_child("response|GET|https://secure.invalid/", &[("HTTPS_PROXY", &format!("http://127.0.0.1:{pp}"))]);
    assert!(out.starts_with("err proxy 127.0.0.1:") && out.contains("refused CONNECT secure.invalid:443: HTTP/1.1 403 Forbidden"), "{out}");
}

#[test]
fn no_proxy_bypasses_by_suffix_ip_cidr_and_star() {
    for no_proxy in ["127.0.0.1", "localhost,127.0.0.0/8", "*", ".0.0.1", "example.com, 127.0.0.1"] {
        let (pp, rx) = proxy(1, None);
        let (port, _orx) = origin(OK);
        let out = in_child(
            &format!("response|GET|http://127.0.0.1:{port}/"),
            &[("HTTP_PROXY", &format!("http://127.0.0.1:{pp}")), ("NO_PROXY", no_proxy)],
        );
        if no_proxy == ".0.0.1" {
            // A suffix entry matches host NAMES, never the digits of an address.
            assert!(out.starts_with("ok 200 via proxy"), "NO_PROXY={no_proxy}: {out}");
            continue;
        }
        assert_eq!(out, "ok 200 ok", "NO_PROXY={no_proxy}");
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "NO_PROXY={no_proxy} still used the proxy");
    }
}

#[test]
fn no_proxy_matches_a_domain_and_its_subdomains_only() {
    for (host, bypass) in [("example.invalid", true), ("api.example.invalid", true), ("notexample.invalid", false)] {
        let (pp, _rx) = proxy(1, None);
        let out = in_child(
            &format!("response|GET|http://{host}:1/"),
            &[("HTTP_PROXY", &format!("http://127.0.0.1:{pp}")), ("no_proxy", "example.invalid")],
        );
        // Bypassed = dialled directly, which fails (`.invalid` never resolves).
        assert_eq!(out.starts_with("err cannot resolve the host of"), bypass, "{host}: {out}");
    }
}

#[test]
fn http_proxy_is_ignored_under_cgi_but_lowercase_is_not() {
    // httpoxy (CVE-2016-5385): a CGI request's `Proxy:` header arrives as
    // HTTP_PROXY. Go and reqwest skip the variable when REQUEST_METHOD is set.
    let (port, _orx) = origin(OK);
    let (pp, rx) = proxy(1, None);
    let out = in_child(
        &format!("response|GET|http://127.0.0.1:{port}/"),
        &[("HTTP_PROXY", &format!("http://127.0.0.1:{pp}")), ("REQUEST_METHOD", "GET")],
    );
    assert_eq!(out, "ok 200 ok");
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

/// `n` bytes off `s`.
fn read_n(s: &mut TcpStream, n: usize) -> std::io::Result<Vec<u8>> {
    let mut b = vec![0u8; n];
    s.read_exact(&mut b)?;
    Ok(b)
}

/// The RFC 1929 sub-negotiation: `user:pass@`.
fn socks5_credentials(s: &mut TcpStream) -> std::io::Result<String> {
    let v = read_n(s, 2)?;
    let u = read_n(s, v[1] as usize)?;
    let pl = read_n(s, 1)?;
    let p = read_n(s, pl[0] as usize)?;
    s.write_all(&[1, 0])?;
    Ok(format!("{}:{}@", String::from_utf8_lossy(&u), String::from_utf8_lossy(&p)))
}

/// The requested destination host (IPv4 or a name resolved at the proxy).
fn socks5_target(s: &mut TcpStream, atyp: u8) -> std::io::Result<String> {
    match atyp {
        1 => {
            let a = read_n(s, 4)?;
            Ok(std::net::Ipv4Addr::new(a[0], a[1], a[2], a[3]).to_string())
        }
        3 => {
            let n = read_n(s, 1)?;
            Ok(String::from_utf8_lossy(&read_n(s, n[0] as usize)?).into_owned())
        }
        _ => panic!("atyp"),
    }
}

/// One SOCKS5 session on `s`: negotiate, report `user:pass@host:port` on
/// `tx`, then relay to the origin on `dest`.
fn socks5_session(mut s: TcpStream, dest: u16, tx: mpsc::Sender<String>) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(10))).ok();
    let b = read_n(&mut s, 2)?;
    let methods = read_n(&mut s, b[1] as usize)?;
    let auth = methods.contains(&2);
    s.write_all(&[5, if auth { 2 } else { 0 }])?;
    let who = if auth { socks5_credentials(&mut s)? } else { String::new() };
    let req = read_n(&mut s, 4)?;
    let target = socks5_target(&mut s, req[3])?;
    let p = read_n(&mut s, 2)?;
    let _ = tx.send(format!("{who}{target}:{}", u16::from_be_bytes([p[0], p[1]])));
    s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])?;
    let up = TcpStream::connect(("127.0.0.1", dest))?;
    let (mut a, mut b2) = (s.try_clone()?, up.try_clone()?);
    let (mut c, mut d) = (s, up);
    let t = thread::spawn(move || {
        let _ = std::io::copy(&mut a, &mut b2);
    });
    let _ = std::io::copy(&mut d, &mut c);
    let _ = c.shutdown(std::net::Shutdown::Write);
    drop(t);
    Ok(())
}

/// A SOCKS5 proxy (RFC 1928, username/password per RFC 1929) that records
/// the requested destination and relays to the origin on `dest`.
fn socks5(dest: u16) -> (u16, mpsc::Receiver<String>) {
    let (l, port) = loopback();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok((s, _)) = l.accept() else { return };
        let _ = socks5_session(s, dest, tx);
    });
    (port, rx)
}

#[test]
fn all_proxy_socks5h_resolves_at_the_proxy() {
    let (port, orx) = origin(OK);
    let (sp, rx) = socks5(port);
    let out = in_child("response|GET|http://far.invalid:8080/p", &[("ALL_PROXY", &format!("socks5h://u:p@127.0.0.1:{sp}"))]);
    assert_eq!(out, "ok 200 ok");
    assert_eq!(recv(&rx), "u:p@far.invalid:8080");
    // Through SOCKS the origin sees an ordinary origin-form request.
    let head = head_of(&orx);
    assert_eq!(request_line(&head), "GET /p HTTP/1.1");
    assert!(head.contains("\r\nHost: far.invalid:8080\r\n"));
}

#[test]
fn an_unsupported_proxy_scheme_is_an_error_not_a_direct_connection() {
    let (port, touched) = untouched_listener();
    let out = in_child(
        &format!("response|GET|http://127.0.0.1:{port}/"),
        &[("HTTP_PROXY", "https://secret:pw@127.0.0.1:9")],
    );
    assert!(out.starts_with("err HTTP_PROXY names a proxy with the scheme \"https\""), "{out}");
    assert!(!out.contains("secret"), "the proxy credentials leaked into the error: {out}");
    assert!(!touched(Duration::from_millis(200)));
}
