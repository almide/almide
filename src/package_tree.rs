//! ADR-0035 D3 / D4: package content is a tree of regular files, and the
//! lock holds a hash of that tree.
//!
//! - A fetched package becomes a [`PackageTree`] — `(RelPath, bytes)` pairs —
//!   before anything touches the filesystem. Its builder refuses a link, a
//!   submodule, a name outside the file-name grammar, two paths equal under
//!   case folding, and anything over the [budgets](MAX_FILE_BYTES). The writer
//!   then creates only directories and regular files, under a fresh root.
//! - [`package_files`] is the ONE enumeration of a package on disk. The tree
//!   hash, the fetch validation, the module resolver (`resolve::source_entries`)
//!   and the native / media grant scans all go through [`dir_entries`], which
//!   never follows a link: inside a dependency's cache entry a link is
//!   tampering and is refused; in a local package it is skipped.
//! - [`TreeSum`] is Go's `h1:` dirhash shape: sha256 over the sorted lines
//!   `sha256(file) + "  " + RelPath + "\n"`, base64.
//!
//! The budgets are fixed by this module (ADR-0030 form: counted, never a
//! time, and nothing in a source, manifest, lock or environment raises them).
//! They were set from a measurement of every almide-org repository, every
//! tag and HEAD (2026-11-19): the largest library is 359 files at depth 5
//! and 3.4 MB.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Bytes of one file.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Bytes of every file of one package together.
pub const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
/// Regular files of one package.
pub const MAX_FILES: usize = 10_000;
/// Segments of one path (`src/a/b.almd` is 3).
pub const MAX_DEPTH: usize = 32;

/// A package-format refusal, naming the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFormatError {
    pub entry: String,
    pub why: String,
}

impl std::fmt::Display for PackageFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "package format: `{}` {}", self.entry.escape_debug(), self.why)
    }
}

fn refuse<T>(entry: &str, why: impl Into<String>) -> Result<T, PackageFormatError> {
    Err(PackageFormatError { entry: entry.to_string(), why: why.into() })
}

/// One file or directory name of a package: `[A-Za-z0-9._-]{1,100}`, not
/// starting with `-`, not `.` / `..`, not a Windows reserved device name, and
/// not `.git` in any case (git's own metadata never travels as content).
pub fn file_segment(name: &[u8]) -> Result<&str, String> {
    let ok = !name.is_empty()
        && name.len() <= 100
        && name[0] != b'-'
        && name.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !name.iter().all(|b| *b == b'.');
    let text = std::str::from_utf8(name).ok().filter(|_| ok).ok_or_else(|| {
        "has a name outside the package file-name grammar (letters, digits, `.`, `_`, `-`; not starting with `-`; at most 100)"
            .to_string()
    })?;
    if text.eq_ignore_ascii_case(".git") {
        return Err("is git metadata, which a package cannot hold".to_string());
    }
    let stem = text.split('.').next().unwrap_or(text).to_ascii_lowercase();
    if crate::git_source::is_windows_reserved(&stem) {
        return Err("is a reserved device name on Windows".to_string());
    }
    Ok(text)
}

/// The running budget and collision check of one package, shared by the
/// fetch-side builder and the on-disk verifier so both refuse the same trees.
#[derive(Default)]
struct FormatCheck {
    files: usize,
    bytes: u64,
    /// Case-folded path → (as written, is a directory).
    folded: HashMap<String, (String, bool)>,
}

impl FormatCheck {
    /// Admit the regular file `segs` (already grammar-checked) of `size` bytes.
    fn admit(&mut self, segs: &[&str], size: u64) -> Result<String, PackageFormatError> {
        let path = segs.join("/");
        if segs.len() > MAX_DEPTH {
            return refuse(&path, format!("is {} levels deep (max {MAX_DEPTH})", segs.len()));
        }
        if size > MAX_FILE_BYTES {
            return refuse(&path, format!("is {size} bytes (max {MAX_FILE_BYTES} per file)"));
        }
        self.files += 1;
        if self.files > MAX_FILES {
            return refuse(&path, format!("is one file more than a package may hold (max {MAX_FILES})"));
        }
        self.bytes += size;
        if self.bytes > MAX_PACKAGE_BYTES {
            return refuse(&path, format!("takes the package past {MAX_PACKAGE_BYTES} bytes"));
        }
        for i in 1..=segs.len() {
            let written = segs[..i].join("/");
            let is_dir = i < segs.len();
            match self.folded.get(&written.to_ascii_lowercase()) {
                Some((other, other_dir)) if *other != written || (*other_dir != is_dir) => {
                    return refuse(&path, format!("collides with `{other}` when case is folded (or as a file and a directory)"));
                }
                Some(_) if !is_dir => return refuse(&path, "appears twice"),
                Some(_) => {}
                None => {
                    self.folded.insert(written.to_ascii_lowercase(), (written, is_dir));
                }
            }
        }
        Ok(path)
    }
}

/// A fetched package before it is written: regular files only.
#[derive(Default)]
pub struct PackageTree {
    check: FormatCheck,
    files: BTreeMap<String, Vec<u8>>,
}

/// What a fetched tree entry is, as the source (a git tree) names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Regular,
    Symlink,
    Submodule,
    Other,
}

impl PackageTree {
    pub fn new() -> PackageTree {
        PackageTree::default()
    }

    /// Add one entry at `segs` (raw names from the source). Anything but a
    /// regular file is refused, as is a name outside the grammar.
    pub fn add(&mut self, segs: &[&[u8]], kind: EntryKind, bytes: Vec<u8>) -> Result<(), PackageFormatError> {
        let shown = segs.iter().map(|s| String::from_utf8_lossy(s).into_owned()).collect::<Vec<_>>().join("/");
        match kind {
            EntryKind::Regular => {}
            EntryKind::Symlink => return refuse(&shown, "is a symbolic link; a package holds regular files only"),
            EntryKind::Submodule => return refuse(&shown, "is a submodule (gitlink); a package holds regular files only"),
            EntryKind::Other => return refuse(&shown, "is not a regular file"),
        }
        let mut names = Vec::with_capacity(segs.len());
        for s in segs {
            names.push(file_segment(s).or_else(|why| refuse(&shown, why))?);
        }
        let path = self.check.admit(&names, bytes.len() as u64)?;
        self.files.insert(path, bytes);
        Ok(())
    }

    /// Check a directory's own name before its entries are read, so a
    /// refused directory is named rather than its first file.
    pub fn check_dir(segs: &[&[u8]]) -> Result<(), PackageFormatError> {
        let shown = segs.iter().map(|s| String::from_utf8_lossy(s).into_owned()).collect::<Vec<_>>().join("/");
        if segs.len() >= MAX_DEPTH {
            return refuse(&shown, format!("is {} levels deep (max {MAX_DEPTH})", segs.len() + 1));
        }
        for s in segs {
            file_segment(s).or_else(|why| refuse(&shown, why))?;
        }
        Ok(())
    }

    pub fn sum(&self) -> TreeSum {
        TreeSum::of(self.files.iter().map(|(p, b)| (p.as_str(), sha256(b))))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Write the tree under `root`, which must not exist yet: directories and
    /// regular files only, each created new, so nothing is written through
    /// an existing entry.
    pub fn write_to(&self, root: &Path) -> Result<(), String> {
        use std::io::Write;
        std::fs::create_dir(root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
        for (path, bytes) in &self.files {
            let mut at = root.to_path_buf();
            let segs: Vec<&str> = path.split('/').collect();
            for dir in &segs[..segs.len() - 1] {
                at.push(dir);
                match std::fs::create_dir(&at) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && std::fs::symlink_metadata(&at).is_ok_and(|m| m.is_dir()) => {}
                    Err(e) => return Err(format!("cannot create {}: {e}", at.display())),
                }
            }
            at.push(segs[segs.len() - 1]);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&at)
                .map_err(|e| format!("cannot write {}: {e}", at.display()))?;
            f.write_all(bytes).map_err(|e| format!("cannot write {}: {e}", at.display()))?;
        }
        Ok(())
    }
}

/// What [`dir_entries`] does with a link (or anything not a file or a
/// directory) it meets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Links {
    /// A dependency's cache entry: a link there was not written by the
    /// fetch, so it is tampering and the walk fails.
    Refuse,
    /// A local package (`path =`, the project itself): the link is not
    /// followed and not listed.
    Skip,
}

/// The entries of one directory, each in name order: `(files, dirs)`. Never
/// follows a link. THE primitive every package reader walks with.
pub fn dir_entries(dir: &Path, links: Links) -> Result<(Vec<PathBuf>, Vec<PathBuf>), PackageFormatError> {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let (Ok(kind), p) = (e.file_type(), e.path()) else { continue };
        if kind.is_dir() {
            dirs.push(p);
        } else if kind.is_file() {
            files.push(p);
        } else if links == Links::Refuse {
            return refuse(
                &p.display().to_string(),
                "is a link or special file inside a dependency's cache entry; the fetch never writes one, so the entry was changed",
            );
        }
    }
    files.sort();
    dirs.sort();
    Ok((files, dirs))
}

/// Every regular file of the package at `root`, as `(RelPath, path)` in
/// RelPath order. The one enumeration (D3).
pub fn package_files(root: &Path, links: Links) -> Result<Vec<(String, PathBuf)>, PackageFormatError> {
    fn walk(dir: &Path, rel: &str, links: Links, out: &mut Vec<(String, PathBuf)>) -> Result<(), PackageFormatError> {
        let (files, dirs) = dir_entries(dir, links)?;
        let join = |p: &Path| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if rel.is_empty() { name } else { format!("{rel}/{name}") }
        };
        for f in files {
            out.push((join(&f), f));
        }
        for d in dirs {
            let r = join(&d);
            walk(&d, &r, links, out)?;
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, "", links, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// The tree hash of a dependency's cache entry, re-read from disk under the
/// same format rules and budgets as the fetch: a link, an extra file, a name
/// outside the grammar or an over-budget file fails here, before anything
/// reads the package.
pub fn sum_on_disk(root: &Path) -> Result<TreeSum, PackageFormatError> {
    use std::io::Read;
    let mut check = FormatCheck::default();
    let mut lines = Vec::new();
    for (rel, path) in package_files(root, Links::Refuse)? {
        let segs: Vec<&str> = rel.split('/').collect();
        for s in &segs {
            file_segment(s.as_bytes()).or_else(|why| refuse(&rel, why))?;
        }
        let size = std::fs::symlink_metadata(&path).map(|m| m.len()).unwrap_or(0);
        check.admit(&segs, size)?;
        let mut bytes = Vec::with_capacity(size as usize);
        std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
            .or_else(|e| refuse(&rel, format!("cannot be read: {e}")))?;
        if bytes.len() as u64 != size {
            return refuse(&rel, "changed while it was read");
        }
        lines.push((rel, sha256(&bytes)));
    }
    Ok(TreeSum::of(lines.iter().map(|(p, h)| (p.as_str(), *h))))
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let d = sha2::Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

/// `h1:<base64 sha256>` of a package tree (D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TreeSum([u8; 32]);

impl TreeSum {
    /// From `(RelPath, sha256(file))` pairs in any order. As Go's dirhash
    /// `Hash1`, the lines are in file-name order (byte order of the RelPath).
    pub fn of<'a>(files: impl Iterator<Item = (&'a str, [u8; 32])>) -> TreeSum {
        let mut files: Vec<(&str, [u8; 32])> = files.collect();
        files.sort_by(|a, b| a.0.cmp(b.0));
        let lines: String = files.iter().map(|(p, h)| format!("{}  {p}\n", hex(h))).collect();
        TreeSum(sha256(lines.as_bytes()))
    }

    /// `h1:` followed by the 44-character base64 of 32 bytes; nothing else.
    pub fn parse(s: &str) -> Result<TreeSum, String> {
        let bad = || format!("`{}` is not a tree sum (`h1:` and 44 base64 characters)", s.escape_debug());
        let b64 = s.strip_prefix("h1:").ok_or_else(bad)?;
        let bytes = base64_decode(b64).ok_or_else(bad)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| bad())?;
        // Only the canonical spelling: re-encoding must give the input back.
        let sum = TreeSum(arr);
        if sum.to_string() != s {
            return Err(bad());
        }
        Ok(sum)
    }

    /// Lowercase hex of the digest: the cache directory name.
    pub fn hex(&self) -> String {
        hex(&self.0)
    }
}

impl std::fmt::Display for TreeSum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "h1:{}", base64_encode(&self.0))
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::new();
    for chunk in s.as_bytes().chunks(4) {
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            let v = if *c == b'=' && i >= 4 - pad { 0 } else { B64.iter().position(|x| x == c)? as u32 };
            n = n << 6 | v;
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad.min(2)]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(entries: &[(&str, EntryKind, usize)]) -> Result<PackageTree, PackageFormatError> {
        let mut t = PackageTree::new();
        for (p, k, n) in entries {
            let segs: Vec<&[u8]> = p.split('/').map(str::as_bytes).collect();
            t.add(&segs, *k, vec![b'x'; *n])?;
        }
        Ok(t)
    }

    #[test]
    fn a_tree_of_regular_files_is_accepted_with_dot_files() {
        let t = tree(&[
            ("almide.toml", EntryKind::Regular, 10),
            (".gitignore", EntryKind::Regular, 3),
            (".github/workflows/ci.yml", EntryKind::Regular, 3),
            ("src/mod.almd", EntryKind::Regular, 30),
        ])
        .unwrap();
        assert_eq!(t.len(), 4);
    }

    #[test]
    fn every_listed_form_is_refused_naming_the_entry() {
        let cases: &[(&[(&str, EntryKind, usize)], &str)] = &[
            (&[("src/ext", EntryKind::Symlink, 1)], "symbolic link"),
            (&[("vendor/x", EntryKind::Submodule, 0)], "submodule"),
            (&[("../x", EntryKind::Regular, 1)], "grammar"),
            (&[("src/./x", EntryKind::Regular, 1)], "grammar"),
            (&[("/etc/passwd", EntryKind::Regular, 1)], "grammar"),
            (&[("src/-rf", EntryKind::Regular, 1)], "grammar"),
            (&[("src/a\u{202E}b", EntryKind::Regular, 1)], "grammar"),
            (&[("src/a b", EntryKind::Regular, 1)], "grammar"),
            (&[("src/CON", EntryKind::Regular, 1)], "reserved"),
            (&[(".git/config", EntryKind::Regular, 1)], "git metadata"),
            (&[("README", EntryKind::Regular, 1), ("readme", EntryKind::Regular, 1)], "case"),
            (&[("Src/a", EntryKind::Regular, 1), ("src/b", EntryKind::Regular, 1)], "case"),
            (&[("big", EntryKind::Regular, MAX_FILE_BYTES as usize + 1)], "per file"),
        ];
        for (entries, needle) in cases {
            let e = tree(entries).err().unwrap_or_else(|| panic!("{entries:?} accepted"));
            assert!(e.to_string().contains(needle), "{entries:?}: {e}");
        }
        let deep = vec!["d"; MAX_DEPTH + 1].join("/");
        assert!(tree(&[(&deep, EntryKind::Regular, 1)]).is_err());
        let ok_deep = vec!["d"; MAX_DEPTH].join("/");
        assert!(tree(&[(&ok_deep, EntryKind::Regular, 1)]).is_ok());
    }

    #[test]
    fn the_file_and_byte_budgets_count() {
        let mut t = PackageTree::new();
        for i in 0..MAX_FILES {
            let name = format!("f{i}");
            t.add(&[name.as_bytes()], EntryKind::Regular, Vec::new()).unwrap();
        }
        assert!(t.add(&[b"one-more"], EntryKind::Regular, Vec::new()).unwrap_err().to_string().contains("max 10000"));
        let mut t = PackageTree::new();
        for i in 0..4 {
            let name = format!("f{i}");
            t.add(&[name.as_bytes()], EntryKind::Regular, vec![0; MAX_FILE_BYTES as usize]).unwrap();
        }
        assert!(t.add(&[b"f4"], EntryKind::Regular, vec![0; 1]).is_err());
    }

    #[test]
    fn the_sum_is_go_dirhash_shaped_and_round_trips() {
        let t = tree(&[("b", EntryKind::Regular, 1), ("a/c", EntryKind::Regular, 2)]).unwrap();
        let lines = format!("{}  a/c\n{}  b\n", hex(&sha256(b"xx")), hex(&sha256(b"x")));
        assert_eq!(t.sum(), TreeSum(sha256(lines.as_bytes())));
        let s = t.sum().to_string();
        assert!(s.starts_with("h1:") && s.len() == 47, "{s}");
        assert_eq!(TreeSum::parse(&s).unwrap(), t.sum());
        for bad in ["", "h1:", "sha256:abc", &s[..46], &format!("{s}="), &s.replace("h1:", "H1:"), "h1:../../../../etc/passwd/xxxxxxxxxxxxxxxxxxxxxxxxx"] {
            assert!(TreeSum::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_decode("Zm9vYg==").unwrap(), b"foob");
    }

    #[test]
    fn the_disk_sum_matches_the_written_tree_and_refuses_a_link() {
        let root = std::env::temp_dir().join(format!("almide-pkgtree-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let t = tree(&[("almide.toml", EntryKind::Regular, 5), ("src/mod.almd", EntryKind::Regular, 7)]).unwrap();
        t.write_to(&root).unwrap();
        assert_eq!(sum_on_disk(&root).unwrap(), t.sum());
        std::fs::write(root.join("src/mod.almd"), b"tampered").unwrap();
        assert_ne!(sum_on_disk(&root).unwrap(), t.sum());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", root.join("src/ext")).unwrap();
            assert!(sum_on_disk(&root).unwrap_err().to_string().contains("link"));
            let (files, dirs) = dir_entries(&root.join("src"), Links::Skip).unwrap();
            assert_eq!((files.len(), dirs.len()), (1, 0));
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
