//! ADR-0035 D1: a dependency source is read ONCE into a typed [`GitSource`].
//!
//! Every audited hole of the string era (#4810 option injection, #4811 trust
//! bypass, #4812 a lock commit used as a path) sat where one string was read
//! by two readers that disagreed. Here there is one reader. Trust
//! ([`TrustPrefix::covers`]), the fetch URL ([`GitSource::https_url`]), the
//! ssh argv (`fetch::ssh`) and every cache key are serialized from the typed
//! fields only, and no field can hold a value another reader could take for
//! an option, a second host or a path:
//!
//! ```text
//! GitSource = { transport: Https | Ssh, user: Segment?, host: Host, port: 1..65535?, path: [Segment] (1 or more) }
//! Host      = lowercase ASCII DNS name, 1..253 bytes, labels [a-z0-9-], not starting or ending with "-"
//! Segment   = [A-Za-z0-9_] [A-Za-z0-9._-]{0,99}   -- never starts with "-" or ".", never "." or ".."
//! ```
//!
//! Kept: any host, any path depth, a port, `https://`, `ssh://` and the
//! scp-like `user@host:path` (normalized into the same value), a plain ssh
//! user, and a `.git` suffix (normalized away from the identity). Refused,
//! each with its own diagnostic: a component starting with `-`, a query or a
//! fragment, userinfo on https or a password anywhere, any other scheme,
//! control / bidi / whitespace characters raw or percent-encoded, an empty or
//! dot-only segment, and a Windows reserved name.

use std::fmt;

/// How a source is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Transport {
    Https,
    Ssh,
}

/// A lowercase ASCII DNS name (see the module grammar). Cannot start with `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Host(String);

/// One path component: `[A-Za-z0-9_][A-Za-z0-9._-]{0,99}`, not a Windows
/// reserved name. Cannot start with `-` or `.`, so it is never an option and
/// never `.` / `..`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Segment(String);

/// A dependency source on a git host. Equality and hashing are on the
/// identity: a written `.git` suffix is not part of it.
#[derive(Debug, Clone)]
pub struct GitSource {
    pub transport: Transport,
    pub user: Option<Segment>,
    pub host: Host,
    /// `None` for the transport's default port (443 / 22).
    pub port: Option<u16>,
    pub path: Vec<Segment>,
    /// The last segment was written with a `.git` suffix. Not identity: the
    /// fetch asks for that spelling first, since a plain git server may only
    /// serve it.
    pub dot_git: bool,
}

/// A commit id as a lock holds it: 40 or 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommitId(String);

/// A `[trust] owners` entry: a host and a whole-segment path prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrustPrefix {
    pub host: Host,
    pub path: Vec<Segment>,
}

/// Windows reserved device names, refused as a segment (alone or before an
/// extension, any case): a package checked out on Windows could not hold it.
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
    "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// `stem` (lowercased, before any `.`) is a Windows reserved device name.
pub fn is_windows_reserved(stem: &str) -> bool {
    WINDOWS_RESERVED.contains(&stem)
}

/// A bidirectional-text control: it reorders how a terminal shows the text
/// around it, so a URL holding one can display as another URL.
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// A character no source may hold, raw or percent-encoded.
fn is_forbidden_char(c: char) -> bool {
    c.is_control() || c.is_whitespace() || is_bidi_control(c)
}

impl Host {
    pub fn parse(s: &str) -> Result<Host, String> {
        if s.starts_with('-') {
            return Err(format!("its host `{}` begins with `-`, which git and ssh read as an option", s.escape_debug()));
        }
        let lower = s.to_ascii_lowercase();
        let label_ok = |l: &str| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        if lower.is_empty() || lower.len() > 253 || !lower.split('.').all(label_ok) {
            return Err(format!(
                "its host `{}` is not a DNS name (letters, digits and `-` in dot-separated labels)",
                s.escape_debug()
            ));
        }
        Ok(Host(lower))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Segment {
    pub fn parse(s: &str) -> Result<Segment, String> {
        if s.is_empty() {
            return Err("it has an empty path segment (`//` or a trailing `/`)".to_string());
        }
        if s.starts_with('-') {
            return Err(format!("its path segment `{}` begins with `-`, which git and ssh read as an option", s.escape_debug()));
        }
        if s.bytes().all(|b| b == b'.') {
            return Err(format!("it has a `{s}` path segment, so the repository it names cannot be read from its text"));
        }
        let first_ok = s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
        let rest_ok = s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !first_ok || !rest_ok || s.len() > 100 {
            return Err(format!(
                "its path segment `{}` is not a plain name (letters, digits, `_`, `.` and `-`, starting with a letter, digit or `_`, at most 100)",
                s.escape_debug()
            ));
        }
        let stem = s.split('.').next().unwrap_or(s).to_ascii_lowercase();
        if is_windows_reserved(&stem) {
            return Err(format!("its path segment `{s}` is a reserved device name on Windows"));
        }
        Ok(Segment(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl CommitId {
    /// 40 (sha1) or 64 (sha256) lowercase hex digits; nothing else.
    pub fn parse(s: &str) -> Result<CommitId, String> {
        if (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            Ok(CommitId(s.to_string()))
        } else {
            Err(format!("`{}` is not a hex commit hash (40 or 64 lowercase hex digits)", s.escape_debug()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first 12 digits, for messages.
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a written source was refused, and the canonical form to write
/// instead when the source has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    pub written: String,
    pub why: String,
    pub canonical: Option<String>,
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the git source `{}` is refused: {}", self.written.escape_debug(), self.why)?;
        match &self.canonical {
            Some(c) => write!(f, "\n  hint: write it as `{c}`"),
            None => write!(f, "\n  hint: write the source as https://host/owner/repo or ssh://host/owner/repo; a local package is a `path =` dependency"),
        }
    }
}

/// Decode `%XX` escapes (bytes), leaving anything malformed as written.
fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

impl GitSource {
    /// Read a source as written in a manifest, a lock, `almide add` or
    /// `almide tool install`. The only reader of a source string.
    pub fn parse(written: &str) -> Result<GitSource, SourceError> {
        let refuse = |why: String| SourceError { written: written.to_string(), why, canonical: None };
        if written.is_empty() {
            return Err(refuse("it is empty".to_string()));
        }
        if written.starts_with('-') {
            return Err(refuse("it begins with `-`, which git reads as an option".to_string()));
        }
        if let Some(c) = written.chars().find(|c| is_forbidden_char(*c)) {
            return Err(refuse(format!("it holds {}", char_kind(c))));
        }
        if written.contains('%') {
            let decoded = String::from_utf8_lossy(&percent_decode(written)).into_owned();
            return Err(refuse(match decoded.chars().find(|c| is_forbidden_char(*c)) {
                Some(c) => format!("it holds a percent-encoded {}", char_kind(c)),
                None => "it holds a percent escape, so the repository it names cannot be read from its text".to_string(),
            }));
        }
        if written.contains(['?', '#']) {
            return Err(refuse("it holds a `?` or `#` (a query or fragment), so the host git contacts cannot be read from its text".to_string()));
        }
        if written.contains('\\') {
            return Err(refuse("it holds a `\\`".to_string()));
        }
        // `ext::…`, `fd::…`: a git transport helper, which runs a command.
        if let Some((scheme, _)) = written.split_once("::") {
            if !scheme.contains(['/', '@', ':']) {
                return Err(refuse(format!("`{scheme}::` is a git transport helper, which runs a command; only https:// and ssh:// sources are fetched")));
            }
        }
        let (transport, authority, path, scp) = if let Some((scheme, rest)) = written.split_once("://") {
            let transport = match scheme.to_ascii_lowercase().as_str() {
                "https" => Transport::Https,
                "ssh" => Transport::Ssh,
                other => {
                    return Err(refuse(format!(
                        "its scheme `{}://` is not fetched; only https:// and ssh:// are (a local package is a `path =` dependency)",
                        other.escape_debug()
                    )))
                }
            };
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            (transport, authority, path, false)
        } else {
            match written.split_once(':') {
                // scp-like `user@host:owner/repo`: the part before the first
                // `:` holds no `/`.
                Some((before, path)) if !before.is_empty() && !before.contains('/') => (Transport::Ssh, before, path, true),
                _ => {
                    return Err(refuse(
                        "it is not a URL (https://host/owner/repo, ssh://host/owner/repo or user@host:owner/repo)".to_string(),
                    ))
                }
            }
        };
        let (userinfo, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, authority),
        };
        let mut canonical_fix: Option<&'static str> = None;
        let user = match userinfo {
            Some(u) if u.contains(':') => {
                return Err(refuse("it holds a password or token in its user part; credentials never go in a source".to_string()))
            }
            Some(_) if transport == Transport::Https => {
                canonical_fix = Some("https userinfo");
                None
            }
            Some(u) => Some(Segment::parse(u).map_err(|e| refuse(e.replace("path segment", "user")))?),
            None => None,
        };
        let (host_text, port) = if scp {
            (hostport, None)
        } else {
            match hostport.split_once(':') {
                Some((h, p)) => {
                    let port = p
                        .parse::<u16>()
                        .ok()
                        .filter(|n| *n > 0 && p.bytes().all(|b| b.is_ascii_digit()))
                        .ok_or_else(|| refuse(format!("its port `{}` is not a number in 1..65535", p.escape_debug())))?;
                    (h, Some(port))
                }
                None => (hostport, None),
            }
        };
        if host_text.starts_with('[') {
            return Err(refuse("its host is an IP literal; a source names a DNS host".to_string()));
        }
        let host = Host::parse(host_text).map_err(&refuse)?;
        let default_port = match transport {
            Transport::Https => 443,
            Transport::Ssh => 22,
        };
        let port = port.filter(|p| *p != default_port);
        let path_text = path.strip_prefix('/').filter(|_| scp).unwrap_or(path);
        let mut pieces: Vec<&str> = path_text.split('/').collect();
        if pieces.len() > 1 && pieces.last() == Some(&"") && !pieces[..pieces.len() - 1].contains(&"") {
            canonical_fix = canonical_fix.or(Some("trailing slash"));
            pieces.pop();
        }
        let mut dot_git = false;
        let mut segments = Vec::with_capacity(pieces.len());
        let last = pieces.len().saturating_sub(1);
        for (i, piece) in pieces.iter().enumerate() {
            let piece = if i == last {
                match piece.strip_suffix(".git") {
                    Some(stem) if !stem.is_empty() => {
                        dot_git = true;
                        stem
                    }
                    _ => piece,
                }
            } else {
                piece
            };
            segments.push(Segment::parse(piece).map_err(&refuse)?);
        }
        if segments.is_empty() {
            return Err(refuse("it names no repository path".to_string()));
        }
        let source = GitSource { transport, user, host, port, path: segments, dot_git };
        match canonical_fix {
            Some("https userinfo") => Err(SourceError {
                written: written.to_string(),
                why: "it has a user part; an https source carries no user or credentials".to_string(),
                canonical: Some(source.to_string()),
            }),
            Some(_) => Err(SourceError {
                written: written.to_string(),
                why: "it ends with `/`".to_string(),
                canonical: Some(source.to_string()),
            }),
            None => Ok(source),
        }
    }

    /// The https URL of the repository (identity spelling, or with `.git`).
    pub fn https_url(&self, with_dot_git: bool) -> String {
        let mut s = format!("https://{}", self.host.as_str());
        if let Some(p) = self.port {
            s.push_str(&format!(":{p}"));
        }
        for seg in &self.path {
            s.push('/');
            s.push_str(seg.as_str());
        }
        if with_dot_git {
            s.push_str(".git");
        }
        s
    }

    /// The repository path as the remote `git-upload-pack` is told it:
    /// `/seg/seg[.git]`. Every segment is a [`Segment`], so the result holds
    /// no quote, space or shell metacharacter.
    pub fn remote_path(&self, with_dot_git: bool) -> String {
        let mut s = String::new();
        for seg in &self.path {
            s.push('/');
            s.push_str(seg.as_str());
        }
        if with_dot_git {
            s.push_str(".git");
        }
        s
    }

    /// The `host/seg/…` identity: the cache and index key, and what trust is
    /// shown as. The transport and port are part of it too (two ports are
    /// two servers), so they are spelled in.
    pub fn key(&self) -> String {
        self.to_string()
    }

    /// `host/first-segment`: what `almide trust` grants by default.
    pub fn owner(&self) -> TrustPrefix {
        TrustPrefix { host: self.host.clone(), path: vec![self.path[0].clone()] }
    }
}

impl PartialEq for GitSource {
    fn eq(&self, o: &GitSource) -> bool {
        self.transport == o.transport && self.user == o.user && self.host == o.host && self.port == o.port && self.path == o.path
    }
}

impl Eq for GitSource {}

impl std::hash::Hash for GitSource {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        (self.transport, &self.user, &self.host, self.port, &self.path).hash(h)
    }
}

/// The canonical form: `https://host[:port]/a/b` or `ssh://[user@]host[:port]/a/b`.
impl fmt::Display for GitSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.transport {
            Transport::Https => f.write_str("https://")?,
            Transport::Ssh => f.write_str("ssh://")?,
        }
        if let Some(u) = &self.user {
            write!(f, "{}@", u.as_str())?;
        }
        f.write_str(self.host.as_str())?;
        if let Some(p) = self.port {
            write!(f, ":{p}")?;
        }
        for seg in &self.path {
            write!(f, "/{}", seg.as_str())?;
        }
        Ok(())
    }
}

impl TrustPrefix {
    /// `host/seg[/seg…]`, each part in the [`GitSource`] grammar. A leading
    /// `https://` and a trailing `/` are tolerated (a pasted URL).
    pub fn parse(entry: &str) -> Result<TrustPrefix, String> {
        let refuse = |why: String| {
            format!(
                "[trust].owners entry `{}` is not `host/owner`: {why}\n  hint: write the host and the owner, like \"github.com/{}\"",
                entry.escape_debug(),
                entry.trim_end_matches('/').rsplit('/').next().unwrap_or(entry)
            )
        };
        if let Some(c) = entry.chars().find(|c| is_forbidden_char(*c)) {
            return Err(refuse(format!("it holds {}", char_kind(c))));
        }
        let t = entry.strip_prefix("https://").unwrap_or(entry);
        let t = t.strip_suffix('/').unwrap_or(t);
        let mut parts = t.split('/');
        let host_text = parts.next().unwrap_or("");
        if !host_text.contains('.') {
            return Err(refuse("its host has no `.`".to_string()));
        }
        let host = Host::parse(host_text).map_err(&refuse)?;
        let path = parts.map(Segment::parse).collect::<Result<Vec<_>, _>>().map_err(&refuse)?;
        if path.is_empty() {
            return Err(refuse("it names no owner".to_string()));
        }
        Ok(TrustPrefix { host, path })
    }

    /// Does this grant cover `source`: the same host, and the grant's path
    /// is a whole-segment prefix of the source's path. Segments compare
    /// without case, as the forges this is used with resolve them.
    pub fn covers(&self, source: &GitSource) -> bool {
        self.host == source.host
            && self.path.len() <= source.path.len()
            && self.path.iter().zip(&source.path).all(|(a, b)| a.as_str().eq_ignore_ascii_case(b.as_str()))
    }
}

impl fmt::Display for TrustPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.host.as_str())?;
        for seg in &self.path {
            write!(f, "/{}", seg.as_str())?;
        }
        Ok(())
    }
}

fn char_kind(c: char) -> String {
    if is_bidi_control(c) {
        format!("a bidirectional-text control (U+{:04X}), which can make it display as another URL", c as u32)
    } else if c.is_control() {
        format!("a control character (U+{:04X})", c as u32)
    } else {
        format!("whitespace (U+{:04X})", c as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> GitSource {
        GitSource::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    fn refused(s: &str) -> SourceError {
        GitSource::parse(s).err().unwrap_or_else(|| panic!("{s} was accepted"))
    }

    #[test]
    fn kept_forms_read_into_one_value() {
        let a = ok("https://github.com/almide/json");
        assert_eq!(a, ok("https://github.com/almide/json.git"));
        assert_eq!(a, ok("HTTPS://GitHub.com/almide/json"));
        assert_eq!(a, ok("https://github.com:443/almide/json"));
        assert_eq!(a.to_string(), "https://github.com/almide/json");
        let s = ok("git@github.com:almide/json.git");
        assert_eq!(s, ok("ssh://git@github.com/almide/json"));
        assert_eq!(s, ok("ssh://git@github.com:22/almide/json"));
        assert_eq!(s.to_string(), "ssh://git@github.com/almide/json");
        assert!(s.dot_git);
        let deep = ok("https://gitlab.example.org:8443/group/sub/team/repo");
        assert_eq!(deep.port, Some(8443));
        assert_eq!(deep.path.len(), 4);
        assert_eq!(deep.https_url(false), "https://gitlab.example.org:8443/group/sub/team/repo");
        assert_eq!(deep.remote_path(true), "/group/sub/team/repo.git");
    }

    #[test]
    fn the_audited_and_listed_forms_are_refused() {
        for s in [
            "--upload-pack=touch /tmp/x",
            "-x",
            "https://-evil.com/a/b",
            "ssh://-oProxyCommand=x/a/b",
            "ssh://-l@host/a/b",
            "https://github.com/-x/repo",
            "git@github.com:-x/repo",
            "https://evil.invalid?@github.com/almide/json",
            "https://evil.invalid#@github.com/almide/json",
            "https://github.com/almide/json?x=1",
            "https://user:pw@github.com/almide/json",
            "ssh://git:token@github.com/almide/json",
            "git://github.com/almide/json",
            "http://github.com/almide/json",
            "file:///srv/git/x",
            "ext::sh -c touch% /tmp/x",
            "ext::evil",
            "/srv/git/x",
            "https://github.com/almide/js\u{202E}on",
            "https://github.com/almide/%E2%80%AEjson",
            "https://github.com/almide/js%0Aon",
            "https://github.com/almide/js on",
            "https://github.com/almide/json\n",
            "https://github.com/almide//json",
            "https://github.com/almide/./json",
            "https://github.com/almide/../evil/x",
            "https://github.com/almide/...",
            "https://github.com/almide/con",
            "https://github.com/almide/NUL.txt",
            "https://[::1]/a/b",
            "https://github.com:0/a/b",
            "https://github.com:99999/a/b",
            "https://github.com",
            "https://github.com/almide/.hidden",
            "https://git_hub.com/a/b",
        ] {
            refused(s);
        }
        assert!(refused("--upload-pack=x").why.contains("begins with `-`"));
        assert!(refused("https://evil.invalid?@github.com/almide/json").why.contains("holds a `?` or `#`"));
    }

    #[test]
    fn a_fixable_spelling_names_its_canonical_form() {
        let e = refused("https://alice@github.com/almide/json");
        assert_eq!(e.canonical.as_deref(), Some("https://github.com/almide/json"));
        let e = refused("https://github.com/almide/json/");
        assert_eq!(e.canonical.as_deref(), Some("https://github.com/almide/json"));
        assert!(e.to_string().contains("write it as `https://github.com/almide/json`"));
    }

    #[test]
    fn trust_is_host_and_whole_segment_prefix() {
        let p = TrustPrefix::parse("github.com/almide").unwrap();
        assert!(p.covers(&ok("https://github.com/almide/json")));
        assert!(p.covers(&ok("git@github.com:Almide/json")));
        assert!(!p.covers(&ok("https://github.com/almide-evil/json")));
        assert!(!p.covers(&ok("https://github.com.evil.io/almide/json")));
        assert!(!p.covers(&ok("https://gitlab.com/almide/json")));
        let sub = TrustPrefix::parse("gitlab.com/group/sub").unwrap();
        assert!(sub.covers(&ok("https://gitlab.com/group/sub/repo")));
        assert!(!sub.covers(&ok("https://gitlab.com/group/other/repo")));
        assert!(!sub.covers(&ok("https://gitlab.com/group/subway")));
        assert!(TrustPrefix::parse("almide").is_err());
        assert!(TrustPrefix::parse("github.com/").is_err());
        assert!(TrustPrefix::parse("github.com/-x").is_err());
        assert!(TrustPrefix::parse("github.com/a/../b").is_err());
        assert_eq!(TrustPrefix::parse("https://GitHub.com/Bob/").unwrap().to_string(), "github.com/Bob");
    }

    #[test]
    fn commit_ids_are_40_or_64_lowercase_hex() {
        assert!(CommitId::parse(&"a".repeat(40)).is_ok());
        assert!(CommitId::parse(&"0".repeat(64)).is_ok());
        for bad in ["", "abc", &"A".repeat(40), &"a".repeat(41), "/home/x", "--upload-pack=x", &"g".repeat(40)] {
            assert!(CommitId::parse(bad).is_err(), "{bad}");
        }
    }
}
