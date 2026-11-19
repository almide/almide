//! A git host for the dependency tests, with no network (ADR-0035).
//!
//! The fetch is an in-process smart-HTTP client that only speaks https, so a
//! test cannot point it at a directory. Instead this file runs, inside the
//! test process, what a real deployment would put between almide and a
//! forge:
//!
//! - an HTTP proxy that accepts `CONNECT host:443` for ANY host, and
//! - behind it, a TLS endpoint that presents a certificate for that host,
//!   signed by a throwaway CA made for this host instance, and answers
//!   git's smart-HTTP requests by running `git upload-pack --stateless-rpc`
//!   on the repository registered for the URL.
//!
//! The almide child is pointed at it only through the standard variables
//! every https client honours — `HTTPS_PROXY` and `SSL_CERT_FILE` — so the
//! manifests and locks name real `https://github.com/<owner>/<repo>` URLs,
//! which is what trust and the lock read, and nothing in almide knows it is
//! under test. Spawning `git` here is the TEST serving a repository; the
//! fetch path itself starts no process (tests/fetch_spawn_gate_test.rs).

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shared {
    /// `host/path` (no `.git`) → the repository directory to serve.
    repos: Mutex<HashMap<String, PathBuf>>,
    /// Requests served (any URL), for "fetched nothing" assertions.
    requests: AtomicUsize,
    /// Answer as a server that only speaks protocol v0.
    v0_only: std::sync::atomic::AtomicBool,
    ca_params: rcgen::CertificateParams,
    ca_key: rcgen::KeyPair,
    leaves: Mutex<HashMap<String, Arc<rustls::ServerConfig>>>,
}

pub struct GitHost {
    shared: Arc<Shared>,
    port: u16,
    pub ca_path: PathBuf,
    dir: PathBuf,
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// One host for a whole test binary. Tests that share it keep their
/// repositories apart by URL ([`GitHost::url_of`]).
pub fn shared() -> &'static GitHost {
    static HOST: std::sync::OnceLock<GitHost> = std::sync::OnceLock::new();
    HOST.get_or_init(|| GitHost::start(&std::env::temp_dir().join(format!("almide-git-host-{}", std::process::id()))))
}

/// `git <args>` in `dir`, with a fixed identity; panics on failure.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn key_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.trim_end_matches('/');
    rest.strip_suffix(".git").unwrap_or(rest).to_ascii_lowercase()
}

impl GitHost {
    /// A new host; its CA lives under `dir`.
    pub fn start(dir: &Path) -> GitHost {
        use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair};
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = dir.join(format!(".git-host-{n}"));
        std::fs::create_dir_all(&dir).expect("mkdir git host");
        let ca_key = KeyPair::generate().expect("CA key");
        let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.distinguished_name.push(rcgen::DnType::CommonName, "almide test git host CA");
        let ca = ca_params.self_signed(&ca_key).expect("CA cert");
        let ca_path = dir.join("ca.pem");
        std::fs::write(&ca_path, ca.pem()).expect("write CA");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let shared = Arc::new(Shared {
            repos: Mutex::new(HashMap::new()),
            requests: AtomicUsize::new(0),
            v0_only: std::sync::atomic::AtomicBool::new(false),
            ca_params,
            ca_key,
            leaves: Mutex::new(HashMap::new()),
        });
        let s = shared.clone();
        std::thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let s = s.clone();
                std::thread::spawn(move || serve(s, tcp));
            }
        });
        GitHost { shared, port, ca_path, dir }
    }

    /// Serve `repo` (a bare or working repository) at `url`
    /// (`https://host/owner/repo`, with or without `.git`).
    pub fn serve(&self, url: &str, repo: &Path) {
        self.shared.repos.lock().unwrap().insert(key_of(url), repo.to_path_buf());
    }

    /// Serve the repository at `dir` under a URL of its own, under the
    /// default-trusted owner: `https://github.com/almide/<dir name>-<hash of
    /// the path>`. The same `dir` always gets the same URL.
    pub fn url_of(&self, dir: &Path) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        dir.hash(&mut h);
        let name: String = dir
            .file_name()
            .map(|n| n.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect())
            .unwrap_or_else(|| "repo".to_string());
        let url = format!("https://github.com/almide/{}-{:08x}", name.trim_start_matches(['-', '_']), h.finish() as u32);
        self.serve(&url, dir);
        url
    }

    /// Stop serving `url`: it answers 404 from now on.
    pub fn unserve(&self, url: &str) {
        self.shared.repos.lock().unwrap().remove(&key_of(url));
    }

    /// From now on, ignore `Git-Protocol: version=2` (an older server).
    pub fn speak_v0_only(&self) {
        self.shared.v0_only.store(true, Ordering::SeqCst);
    }

    /// Requests answered so far.
    pub fn requests(&self) -> usize {
        self.shared.requests.load(Ordering::SeqCst)
    }

    /// Point `cmd` (an almide child) at this host: the proxy and the CA.
    pub fn env<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
        cmd.env("HTTPS_PROXY", format!("http://127.0.0.1:{}", self.port))
            .env_remove("https_proxy")
            .env_remove("ALL_PROXY")
            .env_remove("all_proxy")
            // A native build's cargo honours the same variable: crates.io
            // is not this host's to answer.
            .env("NO_PROXY", "crates.io")
            .env("no_proxy", "crates.io")
            .env("SSL_CERT_FILE", &self.ca_path)
            .env_remove("SSL_CERT_DIR")
            .env("GIT_TERMINAL_PROMPT", "0")
    }

    /// A working repository at `dir` holding `files`, committed and tagged
    /// with each of `tags`, served at `url`. Returns the HEAD commit.
    pub fn publish(&self, url: &str, dir: &Path, files: &[(&str, &str)], tags: &[&str]) -> String {
        for (path, body) in files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).expect("mkdir");
            std::fs::write(p, body).expect("write");
        }
        if !dir.join(".git").exists() {
            git(dir, &["init", "-q"]);
        }
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-qm", "publish", "--allow-empty"]);
        for t in tags {
            git(dir, &["tag", "-f", t]);
        }
        self.serve(url, dir);
        git(dir, &["rev-parse", "HEAD"])
    }
}

impl Drop for GitHost {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn leaf_for(s: &Shared, host: &str) -> Option<Arc<rustls::ServerConfig>> {
    let mut leaves = s.leaves.lock().unwrap();
    if let Some(c) = leaves.get(host) {
        return Some(c.clone());
    }
    let key = rcgen::KeyPair::generate().ok()?;
    let params = rcgen::CertificateParams::new(vec![host.to_string()]).ok()?;
    let issuer = rcgen::Issuer::from_params(&s.ca_params, &s.ca_key);
    let leaf = params.signed_by(&key, &issuer).ok()?;
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .ok()?
        .with_no_client_auth()
        .with_single_cert(vec![leaf.der().clone()], rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()))
        .ok()?;
    let cfg = Arc::new(cfg);
    leaves.insert(host.to_string(), cfg.clone());
    Some(cfg)
}

fn read_head(r: &mut impl Read) -> Option<String> {
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if r.read(&mut b).ok()? == 0 {
            return None;
        }
        head.push(b[0]);
    }
    String::from_utf8(head).ok()
}

fn serve(s: Arc<Shared>, mut tcp: TcpStream) {
    tcp.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let Some(head) = read_head(&mut tcp) else { return };
    let line = head.lines().next().unwrap_or_default().to_string();
    let mut parts = line.split_whitespace();
    let (Some("CONNECT"), Some(target)) = (parts.next(), parts.next()) else {
        let _ = tcp.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    let host = target.rsplit_once(':').map_or(target, |(h, _)| h).to_string();
    let _ = tcp.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
    let Some(cfg) = leaf_for(&s, &host) else { return };
    let Ok(mut conn) = rustls::ServerConnection::new(cfg) else { return };
    let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
    let Some(req) = read_head(&mut tls) else { return };
    s.requests.fetch_add(1, Ordering::SeqCst);
    let req_line = req.lines().next().unwrap_or_default().to_string();
    let header = |name: &str| {
        req.lines().skip(1).find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
    };
    let mut rp = req_line.split_whitespace();
    let (method, path) = (rp.next().unwrap_or(""), rp.next().unwrap_or(""));
    let body = match header("content-length").and_then(|l| l.parse::<usize>().ok()) {
        Some(n) => {
            let mut b = vec![0u8; n];
            if tls.read_exact(&mut b).is_err() {
                return;
            }
            b
        }
        None => Vec::new(),
    };
    let v2 = header("git-protocol").is_some_and(|p| p.contains("version=2")) && !s.v0_only.load(Ordering::SeqCst);
    let (repo_path, advert) = if let Some(p) = path.strip_suffix("/info/refs?service=git-upload-pack") {
        (p, true)
    } else if let Some(p) = path.strip_suffix("/git-upload-pack") {
        (p, false)
    } else {
        respond(&mut tls, 404, "text/plain", b"no such endpoint");
        return;
    };
    let key = key_of(&format!("{host}{repo_path}"));
    let Some(dir) = s.repos.lock().unwrap().get(&key).cloned().filter(|d| d.exists()) else {
        respond(&mut tls, 404, "text/plain", b"no such repository");
        return;
    };
    let mut cmd = Command::new("git");
    cmd.arg("upload-pack").arg("--stateless-rpc");
    if advert {
        cmd.arg("--advertise-refs");
    }
    cmd.arg(&dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    if v2 {
        cmd.env("GIT_PROTOCOL", "version=2");
    }
    let Ok(mut child) = cmd.spawn() else { return };
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&body);
    });
    let out = child.wait_with_output().map(|o| o.stdout).unwrap_or_default();
    let _ = writer.join();
    let _ = method;
    if advert {
        let mut b = Vec::new();
        if !v2 {
            b.extend_from_slice(b"001e# service=git-upload-pack\n0000");
        }
        b.extend_from_slice(&out);
        respond(&mut tls, 200, "application/x-git-upload-pack-advertisement", &b);
    } else {
        respond(&mut tls, 200, "application/x-git-upload-pack-result", &out);
    }
}

fn respond(tls: &mut rustls::Stream<'_, rustls::ServerConnection, TcpStream>, status: u16, ctype: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = tls.write_all(head.as_bytes());
    let _ = tls.write_all(body);
    tls.conn.send_close_notify();
    let _ = tls.flush();
}
