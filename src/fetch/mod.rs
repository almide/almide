//! The dependency fetch (ADR-0035 D2): an in-process git client.
//!
//! Two operations, both taking a typed [`GitSource`]:
//! - [`list_refs`]: the remote's branches and tags (git's `ls-refs`);
//! - [`fetch_commit`]: one commit, shallow, read into a [`PackageTree`]. The
//!   pack's objects are re-hashed ([`pack::read`]), so the tree is bound to
//!   the commit id, and the tree is built from git's own entry modes
//!   ([`objects::tree_of_commit`]), so a link or a submodule is refused
//!   before anything is written.
//!
//! An https source is fetched over [`http`] in this process. An ssh source
//! starts exactly one program, through [`ssh::launch`]. Nothing else on the
//! fetch path starts a process (`tests/fetch_spawn_gate_test.rs`).
//!
//! The wire protocol is git's protocol v2, with a v0 fallback for a server
//! that does not speak v2.

pub mod http;
pub mod objects;
pub mod pack;
pub mod pktline;
mod ssh;

use std::io::{Read, Write};

use crate::git_source::{CommitId, GitSource, Transport};
use crate::package_tree::{PackageFormatError, PackageTree};
use pack::ObjectFormat;
use pktline::{Pkt, PktReader};

/// Why a fetch failed.
#[derive(Debug)]
pub enum FetchError {
    /// The remote says the repository (or the commit) is not there.
    NotFound(String),
    /// The remote could not be reached or failed.
    Unreachable(String),
    /// The remote answered, but what it sent does not hold together.
    Remote(String),
    /// The content is not a valid package (D3).
    Format(PackageFormatError),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::NotFound(s) | FetchError::Unreachable(s) | FetchError::Remote(s) => f.write_str(s),
            FetchError::Format(e) => write!(f, "{e}"),
        }
    }
}

fn remote(s: impl Into<String>) -> FetchError {
    FetchError::Remote(s.into())
}

/// One ref the remote has.
#[derive(Debug, Clone)]
pub struct RemoteRef {
    /// The full name, `refs/heads/main`, `refs/tags/v1.0.0`, `HEAD`.
    pub name: String,
    /// The commit it names (a tag peeled to its commit), lowercase hex.
    pub commit: String,
}

/// What [`list_refs`] found.
#[derive(Debug, Clone, Default)]
pub struct Refs {
    pub refs: Vec<RemoteRef>,
}

impl Refs {
    fn find(&self, full: &str) -> Option<&str> {
        self.refs.iter().find(|r| r.name == full).map(|r| r.commit.as_str())
    }

    /// The commit of tag `name`, else of branch `name` (what `git clone
    /// --branch` accepts).
    pub fn tag_or_branch(&self, name: &str) -> Option<&str> {
        self.find(&format!("refs/tags/{name}")).or_else(|| self.find(&format!("refs/heads/{name}")))
    }

    /// The commit of branch `name`, else of tag `name`.
    pub fn branch_or_tag(&self, name: &str) -> Option<&str> {
        self.find(&format!("refs/heads/{name}")).or_else(|| self.find(&format!("refs/tags/{name}")))
    }

    /// The commit of the default branch.
    pub fn head(&self) -> Option<&str> {
        self.find("HEAD")
    }

    pub fn heads(&self) -> Vec<String> {
        self.refs.iter().filter_map(|r| r.name.strip_prefix("refs/heads/")).map(str::to_string).collect()
    }

    pub fn tags(&self) -> Vec<String> {
        self.refs.iter().filter_map(|r| r.name.strip_prefix("refs/tags/")).map(str::to_string).collect()
    }
}

/// An open conversation with one repository.
struct Session<'a> {
    src: &'a GitSource,
    dot_git: bool,
    ssh: Option<SshPipe>,
    v2: bool,
    caps: Vec<String>,
    format: ObjectFormat,
    /// v0: the refs the advertisement listed, `(name, oid)`, `^{}` lines
    /// included.
    v0_refs: Vec<(String, String)>,
}

struct SshPipe {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
}

impl Drop for SshPipe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn from_http(e: http::HttpError) -> FetchError {
    match e {
        http::HttpError::NotFound(s) => FetchError::NotFound(s),
        http::HttpError::Failed(s) => FetchError::Unreachable(s),
    }
}

fn hex_ok(s: &str, format: ObjectFormat) -> bool {
    s.len() == 2 * format.len() && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl<'a> Session<'a> {
    fn open(src: &'a GitSource) -> Result<Session<'a>, FetchError> {
        // The spelling as written first; without a written `.git`, the
        // `.git` spelling when the plain one is not found.
        let spellings: &[bool] = if src.dot_git { &[true] } else { &[false, true] };
        let mut last = None;
        for &dot_git in spellings {
            match Session::open_as(src, dot_git) {
                Ok(s) => return Ok(s),
                Err(FetchError::NotFound(e)) => last = Some(FetchError::NotFound(e)),
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| FetchError::NotFound(format!("{src}: not found"))))
    }

    fn open_as(src: &'a GitSource, dot_git: bool) -> Result<Session<'a>, FetchError> {
        let mut s = Session { src, dot_git, ssh: None, v2: false, caps: Vec::new(), format: ObjectFormat::Sha1, v0_refs: Vec::new() };
        let lines = match src.transport {
            Transport::Https => {
                let body = http::request(src, dot_git, "/info/refs?service=git-upload-pack", None, true).map_err(from_http)?;
                let mut r = PktReader::new(&body[..]);
                let mut first = r.lines_to_flush().map_err(remote)?;
                // A smart server starts with `# service=…` and a flush.
                if first.first().is_some_and(|l| l.starts_with(b"# service=")) {
                    first = r.lines_to_flush().map_err(remote)?;
                }
                first
            }
            Transport::Ssh => {
                let mut child = ssh::launch(src, dot_git).map_err(|e| FetchError::Unreachable(format!("cannot start ssh for {src}: {e}")))?;
                let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
                    return Err(FetchError::Unreachable("ssh started without pipes".to_string()));
                };
                let mut pipe = SshPipe { child, stdin, stdout };
                let lines = PktReader::new(&mut pipe.stdout).lines_to_flush();
                let lines = match lines {
                    Ok(l) => l,
                    Err(_) => {
                        let status = pipe.child.wait().ok();
                        return Err(FetchError::NotFound(format!(
                            "{src}: ssh ended without a git answer{}",
                            status.and_then(|s| s.code()).map(|c| format!(" (exit {c})")).unwrap_or_default()
                        )));
                    }
                };
                s.ssh = Some(pipe);
                lines
            }
        };
        if lines.first().is_some_and(|l| l == b"version 2") {
            s.v2 = true;
            s.caps = lines[1..].iter().map(|l| String::from_utf8_lossy(l).into_owned()).collect();
        } else {
            for (i, l) in lines.iter().enumerate() {
                let (main, caps) = match l.iter().position(|b| *b == 0) {
                    Some(z) => (&l[..z], Some(&l[z + 1..])),
                    None => (&l[..], None),
                };
                if i == 0 {
                    if let Some(c) = caps {
                        s.caps = String::from_utf8_lossy(c).split(' ').map(str::to_string).collect();
                    }
                }
                let text = String::from_utf8_lossy(main);
                if let Some((oid, name)) = text.split_once(' ') {
                    s.v0_refs.push((name.to_string(), oid.to_string()));
                }
            }
        }
        s.format = match s.caps.iter().find_map(|c| c.strip_prefix("object-format=")) {
            None | Some("sha1") => ObjectFormat::Sha1,
            Some("sha256") => ObjectFormat::Sha256,
            Some(other) => return Err(remote(format!("{src}: unknown object format `{}`", pktline::shown(other.as_bytes())))),
        };
        Ok(s)
    }

    fn cap(&self, name: &str) -> Option<&str> {
        self.caps.iter().find_map(|c| if c == name { Some("") } else { c.strip_prefix(name).and_then(|r| r.strip_prefix('=')) })
    }

    /// Send one request and return the response stream.
    fn call(&mut self, req: &[u8]) -> Result<Box<dyn Read + '_>, FetchError> {
        match &mut self.ssh {
            Some(pipe) => {
                pipe.stdin.write_all(req).and_then(|_| pipe.stdin.flush()).map_err(|e| FetchError::Unreachable(format!("{}: {e}", self.src)))?;
                Ok(Box::new(&mut pipe.stdout))
            }
            None => {
                let body = http::request(self.src, self.dot_git, "/git-upload-pack", Some(("application/x-git-upload-pack-request", req)), self.v2)
                    .map_err(from_http)?;
                Ok(Box::new(std::io::Cursor::new(body)))
            }
        }
    }

    fn v2_command(&self, cmd: &str) -> Vec<u8> {
        let mut req = Vec::new();
        pktline::line(&mut req, &format!("command={cmd}\n"));
        pktline::line(&mut req, &format!("agent=almide/{}\n", env!("CARGO_PKG_VERSION")));
        if self.format != ObjectFormat::Sha1 {
            pktline::line(&mut req, &format!("object-format={}\n", self.format.name()));
        }
        pktline::delim(&mut req);
        req
    }

    fn refs(&mut self) -> Result<Refs, FetchError> {
        let mut raw: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
        if self.v2 {
            let mut req = self.v2_command("ls-refs");
            for l in ["peel\n", "symrefs\n", "ref-prefix HEAD\n", "ref-prefix refs/heads/\n", "ref-prefix refs/tags/\n"] {
                pktline::line(&mut req, l);
            }
            pktline::flush(&mut req);
            let src = self.src.to_string();
            let mut r = PktReader::new(self.call(&req)?);
            for l in r.lines_to_flush().map_err(remote)? {
                let text = String::from_utf8_lossy(&l).into_owned();
                if let Some(msg) = text.strip_prefix("ERR ") {
                    return Err(remote(format!("{src}: {}", pktline::shown(msg.as_bytes()))));
                }
                let mut parts = text.split(' ');
                let (Some(oid), Some(name)) = (parts.next(), parts.next()) else { continue };
                let mut peeled = None;
                let mut symref = None;
                for p in parts {
                    if let Some(x) = p.strip_prefix("peeled:") {
                        peeled = Some(x.to_string());
                    } else if let Some(x) = p.strip_prefix("symref-target:") {
                        symref = Some(x.to_string());
                    }
                }
                raw.push((name.to_string(), oid.to_string(), peeled, symref));
            }
        } else {
            for (name, oid) in &self.v0_refs {
                if let Some(base) = name.strip_suffix("^{}") {
                    if let Some(entry) = raw.iter_mut().find(|e| e.0 == base) {
                        entry.2 = Some(oid.clone());
                    }
                } else {
                    raw.push((name.clone(), oid.clone(), None, None));
                }
            }
        }
        let mut refs = Refs::default();
        for (name, oid, peeled, _) in raw {
            let commit = peeled.unwrap_or(oid);
            // A ref is listed only when its name prints as written and its
            // id is a hash of this repository's format.
            let printable = name.chars().all(|c| !c.is_control() && !c.is_whitespace() && !crate::git_source::is_bidi_control(c));
            let wanted = name == "HEAD" || name.starts_with("refs/heads/") || name.starts_with("refs/tags/");
            if printable && wanted && hex_ok(&commit, self.format) {
                refs.refs.push(RemoteRef { name, commit });
            }
        }
        Ok(refs)
    }

    fn fetch(&mut self, commit: &CommitId) -> Result<PackageTree, FetchError> {
        if commit.as_str().len() != 2 * self.format.len() {
            return Err(remote(format!("{}: commit {commit} is not a {} id, the repository's object format", self.src, self.format.name())));
        }
        let src = self.src.to_string();
        let mut req;
        if self.v2 {
            req = self.v2_command("fetch");
            let shallow = self.cap("fetch").is_some_and(|f| f.split(' ').any(|x| x == "shallow"));
            pktline::line(&mut req, "ofs-delta\n");
            pktline::line(&mut req, "no-progress\n");
            if shallow {
                pktline::line(&mut req, "deepen 1\n");
            }
            pktline::line(&mut req, &format!("want {commit}\n"));
            pktline::line(&mut req, "done\n");
            pktline::flush(&mut req);
        } else {
            let band = if self.cap("side-band-64k").is_some() {
                "side-band-64k"
            } else if self.cap("side-band").is_some() {
                "side-band"
            } else {
                return Err(remote(format!("{src}: the server does not offer side-band")));
            };
            let shallow = self.cap("shallow").is_some();
            let mut caps = format!("{band} ofs-delta no-progress agent=almide/{}", env!("CARGO_PKG_VERSION"));
            if shallow {
                caps.push_str(" shallow");
            }
            if self.format != ObjectFormat::Sha1 {
                caps.push_str(&format!(" object-format={}", self.format.name()));
            }
            req = Vec::new();
            pktline::line(&mut req, &format!("want {commit} {caps}\n"));
            if shallow {
                pktline::line(&mut req, "deepen 1\n");
            }
            pktline::flush(&mut req);
            pktline::line(&mut req, "done\n");
        }
        let v2 = self.v2;
        let format = self.format;
        let mut r = PktReader::new(self.call(&req)?);
        let err_line = |d: &[u8]| d.strip_prefix(b"ERR ").map(|m| FetchError::NotFound(format!("{src}: the remote refused commit {commit}: {}", pktline::shown(m))));
        // Up to the pack: v2 sections before `packfile`, or v0 shallow
        // lines and the NAK.
        loop {
            match r.expect().map_err(remote)? {
                Pkt::Data(d) => {
                    if let Some(e) = err_line(&d) {
                        return Err(e);
                    }
                    let line = d.strip_suffix(b"\n").unwrap_or(&d);
                    if v2 && line == b"packfile" {
                        break;
                    }
                    if !v2 && (line == b"NAK" || line.starts_with(b"ACK ")) {
                        break;
                    }
                }
                Pkt::Flush | Pkt::Delim => {}
                Pkt::ResponseEnd => return Err(remote(format!("{src}: the response ended before the pack"))),
            }
        }
        let mut pack_bytes = Vec::new();
        loop {
            match r.next().map_err(remote)? {
                None | Some(Pkt::Flush) | Some(Pkt::ResponseEnd) => break,
                Some(Pkt::Delim) => return Err(remote(format!("{src}: a delimiter inside the pack"))),
                Some(Pkt::Data(d)) => match d.split_first() {
                    Some((1, data)) => {
                        pack_bytes.extend_from_slice(data);
                        if pack_bytes.len() > http::MAX_BODY {
                            return Err(remote(format!("{src}: a pack over {} bytes", http::MAX_BODY)));
                        }
                    }
                    Some((2, _)) => {}
                    Some((3, msg)) => return Err(remote(format!("{src}: the remote failed: {}", pktline::shown(msg)))),
                    _ => return Err(remote(format!("{src}: a malformed side-band packet"))),
                },
            }
        }
        let objs = pack::read(&pack_bytes, format).map_err(|e| remote(format!("{src}: {e}")))?;
        objects::tree_of_commit(&objs, commit.as_str()).map_err(|e| match e {
            objects::TreeError::Remote(s) => remote(format!("{src}: {s}")),
            objects::TreeError::Format(f) => FetchError::Format(f),
        })
    }
}

/// The branches and tags of `src` (and its default branch as `HEAD`).
pub fn list_refs(src: &GitSource) -> Result<Refs, FetchError> {
    Session::open(src)?.refs()
}

/// The package tree of `commit` in `src`, every object re-hashed, the tree
/// checked against the package format (D3).
pub fn fetch_commit(src: &GitSource, commit: &CommitId) -> Result<PackageTree, FetchError> {
    Session::open(src)?.fetch(commit)
}
