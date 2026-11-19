//! Dependency fetching: version resolution, recursive deps, the lock, and
//! the cache of package trees (ADR-0035).
//!
//! - A git source is a typed [`GitSource`] (D1); trust, the fetch and every
//!   cache key are derived from it.
//! - The fetch itself is [`crate::fetch`]: in process for https, one typed
//!   ssh launcher for ssh (D2). This module starts no process.
//! - A fetched commit becomes a tree of regular files, written under
//!   `cache/trees/<sum>/` (D3), and the lock records its `sum` (D4).
//! - The sum is re-hashed on EVERY use, a cache hit included: a changed
//!   cache entry is refused (and fetched again when the network is allowed).
use std::path::{Path, PathBuf};

use crate::err;
use crate::fetch::{self, FetchError};
use crate::git_source::{CommitId, GitSource};
use crate::package_tree::{self, TreeSum};
use crate::project::{Dependency, FetchedDep, LockedDep, PkgId, Project, cache_dir, parse_lock_file, parse_toml, write_lock_file};

/// Fetch a dependency (or use the cache). Returns its package directory.
pub fn fetch_dep(dep: &Dependency) -> Result<PathBuf, String> {
    fetch_dep_with_lock(dep, None)
}

/// The ref a manifest entry names: its tag, else its branch, else `main`.
///
/// One definition on purpose. The lock LOOKUP and the lock WRITE must agree
/// on which ref a dependency asked for; while this expression was spelled
/// out in three places they could disagree, and a disagreement is not a slow
/// build — it is a lock entry describing a fetch that did not happen (#2522).
fn dep_ref_name(dep: &Dependency) -> &str {
    dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main")
}

/// The source of a git dependency (a path dependency has none).
fn source_of(dep: &Dependency) -> Result<&GitSource, String> {
    dep.git.as_ref().ok_or_else(|| format!("dependency `{}` has neither `git` nor `path`", dep.name))
}

/// One component of a cache path for one piece of text (a ref name),
/// encoded so that the encoding is injective and every result is a plain
/// directory name: `a-z`, `0-9`, `-`, `_` and a non-leading `.` stay as
/// written; every other byte (an uppercase letter included, so a
/// case-insensitive filesystem cannot fold two names together) becomes
/// `%XX`. An empty piece is `%`, which no other piece encodes to. A piece too
/// long for one directory name is split over several, each but the last
/// marked `%+` (a spelling no encoded piece can start with).
fn encode_cache_component(piece: &str, out: &mut PathBuf) {
    let mut enc = String::new();
    for (i, b) in piece.bytes().enumerate() {
        match b {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => enc.push(b as char),
            b'.' if i > 0 => enc.push('.'),
            _ => enc.push_str(&format!("%{:02X}", b)),
        }
    }
    if enc.is_empty() {
        enc.push('%');
    }
    const MAX: usize = 180;
    while enc.len() > MAX {
        // Never split inside a `%XX` escape.
        let mut cut = MAX;
        while enc.as_bytes()[cut - 1] == b'%' || enc.as_bytes()[cut - 2] == b'%' {
            cut -= 1;
        }
        out.push(format!("%+{}", &enc[..cut]));
        enc = enc[cut..].to_string();
    }
    out.push(enc);
}

// ── The cache (ADR-0035 D4) ────────────────────────────────────────────
//
//   cache/trees/<sum hex>/                      a package tree, keyed by its sum
//   cache/index/<sha256(source)>/commit/<id>    "h1:…": the tree of one commit
//   cache/index/<sha256(source)>/ref/<ref>      "<id>": a tag's commit, or a
//                                               floating ref's last resolution
//
// The tree directory is named by the sum, never by a lock or manifest
// string. The index only says where to look: what is found there is hashed
// again before it is used. Directories of the older layouts (`.sources/`,
// `<name>/<ref>`, …) are never read; `almide clean` removes them.

fn trees_dir() -> PathBuf {
    cache_dir().join("trees")
}

/// Where the tree with sum `sum` lives.
pub fn tree_dir(sum: &TreeSum) -> PathBuf {
    trees_dir().join(sum.hex())
}

/// The index directory of one source: the sha256 of its canonical form,
/// so the path is fixed-length and no spelling of a source can name
/// another's directory.
fn index_dir(src: &GitSource) -> PathBuf {
    cache_dir().join("index").join(package_tree::hex(&package_tree::sha256(src.key().as_bytes())))
}

fn index_commit_file(src: &GitSource, commit: &CommitId) -> PathBuf {
    index_dir(src).join("commit").join(commit.as_str())
}

fn index_ref_file(src: &GitSource, ref_name: &str) -> PathBuf {
    let mut p = index_dir(src).join("ref");
    encode_cache_component(ref_name, &mut p);
    p
}

fn read_index_sum(src: &GitSource, commit: &CommitId) -> Option<TreeSum> {
    let text = std::fs::read_to_string(index_commit_file(src, commit)).ok()?;
    TreeSum::parse(text.trim()).ok()
}

fn read_index_ref(src: &GitSource, ref_name: &str) -> Option<CommitId> {
    let text = std::fs::read_to_string(index_ref_file(src, ref_name)).ok()?;
    CommitId::parse(text.trim()).ok()
}

/// Best effort: write `text` to `path` through a sibling temp file.
fn write_index(path: &Path, text: &str) {
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(tmp) = cache_sibling(path, "tmp") else { return };
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The package directory a dependency names inside its tree: the tree
/// itself, or its `subdir` (#3381) — refused unless it is a directory inside
/// the tree that holds an `almide.toml` whose `[package] name` is the
/// dependency's key. Without `subdir`, a tree whose root is no package but
/// whose subdirectories are says which `subdir` to write.
fn package_dir_in(dep: &Dependency, checkout: &Path) -> Result<PathBuf, String> {
    let at = dep_ref_name(dep);
    let shown = dep.git.as_ref().map(|g| g.to_string()).unwrap_or_default();
    let Some(subdir) = dep.subdir.as_deref() else {
        if !checkout.join("almide.toml").exists() && !checkout.join("src").is_dir() && !has_almd_file(checkout) {
            let found = packages_in(checkout);
            if !found.is_empty() {
                return Err(format!(
                    "{}the root of {} at {at} is not a package (no almide.toml) — dependency `{}` needs a `subdir`\n  \
                     hint: this repository holds the packages {}; add `subdir = \"…\"` naming one, e.g. `subdir = \"{}\"`",
                    dep.location_prefix(),
                    shown,
                    dep.name,
                    found.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", "),
                    found.iter().find(|p| p.rsplit('/').next() == Some(dep.name.as_str())).unwrap_or(&found[0]),
                ));
            }
        }
        return Ok(checkout.to_path_buf());
    };
    let found_hint = || {
        let found = packages_in(checkout);
        match found.is_empty() {
            true => "  hint: no directory in this repository holds an almide.toml — check the `git` url and the ref".to_string(),
            false => format!(
                "  hint: the packages in this repository are {} — point `subdir` at one",
                found.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", ")
            ),
        }
    };
    let dir = checkout.join(subdir);
    if !dir.is_dir() {
        return Err(format!(
            "{}subdir `{subdir}` of dependency `{}` does not exist in {} at {at}\n{}",
            dep.location_prefix(),
            dep.name,
            shown,
            found_hint()
        ));
    }
    let inside = match (std::fs::canonicalize(&dir), std::fs::canonicalize(checkout)) {
        (Ok(d), Ok(c)) => d.starts_with(&c),
        _ => false,
    };
    if !inside {
        return Err(format!(
            "{}subdir `{subdir}` of dependency `{}` resolves outside the repository (through a symlink)\n  \
             hint: point `subdir` at the package directory itself",
            dep.location_prefix(),
            dep.name
        ));
    }
    let manifest = dir.join("almide.toml");
    if !manifest.exists() {
        return Err(format!(
            "{}subdir `{subdir}` of dependency `{}` has no almide.toml in {} at {at} — `subdir` must name a package directory\n{}",
            dep.location_prefix(),
            dep.name,
            shown,
            found_hint()
        ));
    }
    match crate::project::manifest_package_name(&manifest) {
        Some(found) if found == dep.name => Ok(dir),
        found => {
            let found = found.unwrap_or_default();
            Err(format!(
                "{}dependency `{}` names subdir `{subdir}`, whose package is `{found}`\n  \
                 hint: the dependency key is the import name and must be the package's name — \
                 rename the key to `{found}`, or point `subdir` at the package `{}`",
                dep.location_prefix(),
                dep.name,
                dep.name
            ))
        }
    }
}

/// Whether `dir` directly holds a `.almd` file.
fn has_almd_file(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "almd")))
}

/// The package directories of a tree, relative to it: every directory up to
/// two levels down (`pkg`, `pkgs/pkg`) holding an `almide.toml`, sorted. Dot
/// directories are skipped. For hints only.
fn packages_in(checkout: &Path) -> Vec<String> {
    fn subdirs(dir: &Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }
    let rel = |p: &Path| p.strip_prefix(checkout).unwrap_or(p).to_string_lossy().replace('\\', "/");
    let mut found = Vec::new();
    for d in subdirs(checkout) {
        if d.join("almide.toml").exists() {
            found.push(rel(&d));
        }
        for dd in subdirs(&d) {
            if dd.join("almide.toml").exists() {
                found.push(rel(&dd));
            }
        }
    }
    found
}

thread_local! {
    /// Set while [`resolve_cached_deps`] runs: no fetch and no lock write.
    static OFFLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn offline() -> bool {
    // `ALMIDE_DEPS_CACHE_ONLY` is the same mode for a whole process tree:
    // `almide mcp` sets it on every tool child, whose own children (the
    // `survive` legs, a test worker) inherit it (#4179).
    OFFLINE.with(|o| o.get()) || almide_base::env::flag("ALMIDE_DEPS_CACHE_ONLY")
}

/// Restores the previous offline state when dropped.
struct OfflineGuard(bool);

impl Drop for OfflineGuard {
    fn drop(&mut self) {
        OFFLINE.with(|o| o.set(self.0));
    }
}

/// The project's dependencies, resolved from what is already on disk: path
/// dependencies and trees already in the cache. It never fetches, and never
/// writes `almide.lock`. A dependency that is not cached yet is an error
/// naming the command that fetches it.
///
/// For tools that look at a project without being asked to build it — the
/// LSP opening a file, `almide ide` — so that an untrusted project cannot
/// choose which hosts this machine contacts, or write into its own tree,
/// before anything is run (#4128).
pub fn resolve_cached_deps(project: &Project) -> Result<Vec<FetchedDep>, String> {
    let _restore = OfflineGuard(OFFLINE.with(|o| o.replace(true)));
    fetch_all_deps(project)
}

/// A cross-process exclusive lock on one cache entry (#4529): the file
/// `.<leaf>.lock` beside the entry `dir`, held with `flock(LOCK_EX)` /
/// `LockFileEx` (fs2, as `BuildDirLock` does for build dirs). Each acquire
/// opens its own file handle, so threads of one process exclude each other
/// as well as separate `almide` processes sharing `~/.almide`. The OS drops
/// the lock when the holder exits, so a killed fetch never wedges the next.
///
/// The lock file is never deleted: unlinking it while another caller waits
/// on it would let a third caller lock a new file of the same name.
struct CacheEntryLock {
    #[cfg(any(unix, windows))]
    _file: std::fs::File,
}

impl CacheEntryLock {
    fn acquire(dir: &Path) -> Result<CacheEntryLock, String> {
        let parent = dir.parent().ok_or_else(|| "cache dir has no parent".to_string())?;
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create cache dir: {}", e))?;
        #[cfg(any(unix, windows))]
        {
            use fs2::FileExt;
            let leaf = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let lock_path = parent.join(format!(".{leaf}.lock"));
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| format!("Failed to open cache lock {}: {}", lock_path.display(), e))?;
            file.lock_exclusive().map_err(|e| format!("Failed to lock {}: {}", lock_path.display(), e))?;
            Ok(CacheEntryLock { _file: file })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(CacheEntryLock {})
        }
    }
}

static CACHE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A fresh sibling name for `dir`: `.<leaf>.<what>-<pid>-<seq>`.
fn cache_sibling(dir: &Path, what: &str) -> Result<PathBuf, String> {
    let parent = dir.parent().ok_or_else(|| "cache dir has no parent".to_string())?;
    let leaf = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let seq = CACHE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(parent.join(format!(".{}.{}-{}-{}", leaf, what, std::process::id(), seq)))
}

/// Remove cache entry `dir` under its [`CacheEntryLock`] (#4529): renamed
/// aside first, so no fetch holding the lock ever sees it half-deleted, and
/// the path is either the whole old entry or absent.
fn remove_cache_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let _lock = CacheEntryLock::acquire(dir)?;
    let aside = cache_sibling(dir, "old")?;
    match std::fs::rename(dir, &aside) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&aside);
            Ok(())
        }
        Err(_) if !dir.exists() => Ok(()),
        Err(e) => Err(format!("Failed to remove stale cache entry {}: {}", dir.display(), e)),
    }
}

/// Populate cache dir `dir` atomically: run `fill` against a fresh sibling
/// temp dir, then rename it into place. Callers may race — `almide test`
/// compiles test files on parallel threads and every thread resolves deps,
/// and two `almide` processes can share `~/.almide` — so `dir` exists only
/// when it is complete.
///
/// The check → fill → rename runs under the entry's [`CacheEntryLock`]
/// (#4529), so ONE caller fetches and the others wait for it and reuse its
/// tree. A caller whose fill failed still succeeds when the entry is there
/// after all (another process without the lock filled it).
fn populate_cache_dir(dir: &Path, fill: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    if dir.exists() {
        return Ok(());
    }
    if offline() {
        return Err(format!("dependency not fetched yet ({}); run `almide build` in the project to fetch it", dir.display()));
    }
    let _lock = CacheEntryLock::acquire(dir)?;
    if dir.exists() {
        // Another caller filled it while this one waited for the lock.
        return Ok(());
    }
    let tmp = cache_sibling(dir, "tmp")?;
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = fill(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        if dir.exists() {
            return Ok(());
        }
        return Err(e);
    }
    match std::fs::rename(&tmp, dir) {
        Ok(()) => Ok(()),
        Err(_) if dir.exists() => {
            // Another fill finished first; ours is redundant.
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(format!("Failed to move fetched dependency into cache: {}", e))
        }
    }
}

/// The text of a fetch failure for a dependency.
fn fetch_failure(name: &str, e: &FetchError) -> String {
    format!("Failed to fetch {name}: {e}")
}

/// A package tree in the cache, checked: its directory, the commit it is,
/// and its sum.
struct Tree {
    dir: PathBuf,
    commit: CommitId,
    sum: TreeSum,
}

/// The tree of `commit` of `src`, from the cache or fetched, with its sum
/// checked NOW (D4): `want` is the lock's sum when there is one.
///
/// - A cache entry is re-hashed on every use. One that does not hash to its
///   name (a file changed, added or removed, a link planted) is refused: in
///   cache-only mode that is an error, otherwise the entry is removed and
///   fetched again.
/// - A fetched tree whose sum is not the lock's is refused outright: the
///   remote served other content for the locked commit.
/// - One fetch per `(source, commit)` at a time (#4529): callers racing on
///   one commit wait for the first one's fetch under the commit's index
///   lock and then find its tree.
fn use_tree(name: &str, src: &GitSource, commit: &CommitId, want: Option<TreeSum>) -> Result<Tree, String> {
    if let Some(tree) = cached_tree(name, src, commit, want)? {
        return Ok(tree);
    }
    if offline() {
        return Err(format!(
            "dependency `{name}` ({src} at {}) is not fetched yet; run `almide build` in the project to fetch it",
            commit.short()
        ));
    }
    let _fetching = CacheEntryLock::acquire(&index_commit_file(src, commit))?;
    if let Some(tree) = cached_tree(name, src, commit, want)? {
        return Ok(tree);
    }
    err(&format!("Fetching {name} from {src} ({})", commit.short()));
    let tree = fetch::fetch_commit(src, commit).map_err(|e| fetch_failure(name, &e))?;
    let sum = tree.sum();
    if let Some(w) = want {
        if w != sum {
            return Err(format!(
                "error: dependency `{name}`: {src} at {} hashes to {sum}, but almide.lock records {w}\n  \
                 note: the remote served other content for the locked commit; nothing was written\n  \
                 hint: if the change is expected, run `almide update {name}` and review the almide.lock diff",
                commit.short()
            ));
        }
    }
    let dir = tree_dir(&sum);
    populate_cache_dir(&dir, |tmp| tree.write_to(tmp))?;
    write_index(&index_commit_file(src, commit), &format!("{sum}\n"));
    Ok(Tree { dir, commit: commit.clone(), sum })
}

/// The cached tree of `commit` (by the lock's sum, else the index's), when
/// it is there and hashes to its sum; `Ok(None)` when it is not cached. A
/// changed entry is an error in cache-only mode, and otherwise removed (so
/// the caller fetches it again) with a warning.
fn cached_tree(name: &str, src: &GitSource, commit: &CommitId, want: Option<TreeSum>) -> Result<Option<Tree>, String> {
    let Some(sum) = want.or_else(|| read_index_sum(src, commit)) else { return Ok(None) };
    let dir = tree_dir(&sum);
    if !dir.exists() {
        return Ok(None);
    }
    let found = package_tree::sum_on_disk(&dir);
    if found.as_ref() == Ok(&sum) {
        return Ok(Some(Tree { dir, commit: commit.clone(), sum }));
    }
    let why = match found {
        Ok(other) => format!("its files hash to {other}"),
        Err(e) => e.to_string(),
    };
    let refusal = format!(
        "the cached tree of dependency `{name}` ({src} at {}) does not match its sum {sum}: {why}\n  \
         note: the cache entry {} was changed after it was fetched",
        commit.short(),
        dir.display()
    );
    if offline() {
        return Err(format!("error: {refusal}\n  hint: run `almide build` in the project to fetch it again"));
    }
    err(&format!("warning: {refusal}; fetching it again"));
    remove_cache_dir(&dir)?;
    Ok(None)
}

/// Fetch a dependency, optionally pinned to a locked commit (and sum).
pub fn fetch_dep_with_lock(dep: &Dependency, locked: Option<&CommitId>) -> Result<PathBuf, String> {
    let pin = locked.map(|c| (c.clone(), None));
    fetch_dep_in_chain(dep, pin, std::slice::from_ref(&dep.name)).map(|f| f.dir)
}

/// What [`fetch_dep_in_chain`] produced: the package directory, and the
/// tree it came from when that may be written to `almide.lock`.
struct Fetched {
    dir: PathBuf,
    /// `None` for a path dependency, and when a floating ref could not be
    /// resolved on its remote and the cached tree was used instead: the
    /// lock must not turn that fallback into a pin (#4527), so the next
    /// build that reaches the remote resolves the ref again.
    lockable: Option<(CommitId, TreeSum)>,
}

/// [`fetch_dep_with_lock`], told the chain of dependency names from the root
/// that requires `dep` (for the error a missing ref gets, #4528).
///
/// With a lock pin, the pinned commit is used, and its sum when the lock
/// has one. Without one, a tag is resolved by its name (once: a tag never
/// moves, so its commit is remembered in the index). A FLOATING ref — a
/// branch, or no ref at all (the remote's default branch) — is resolved on
/// its remote every time (#4527). Cache-only mode ([`offline`]) reads what
/// the index remembers and never asks the remote.
fn fetch_dep_in_chain(dep: &Dependency, locked: Option<(CommitId, Option<TreeSum>)>, chain: &[String]) -> Result<Fetched, String> {
    // Path dependency: use local directory directly, no fetch needed.
    if let Some(ref path) = dep.path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(Fetched { dir: p, lockable: None });
        }
        return Err(format!("path dependency '{}' not found: {}", dep.name, path));
    }
    let src = source_of(dep)?;
    let ref_name = dep_ref_name(dep);
    let mut lockable = true;
    let tree = match (locked, dep.tag.is_some() || offline()) {
        (Some((commit, sum)), _) => use_tree(&dep.name, src, &commit, sum)?,
        (None, true) => {
            let commit = match read_index_ref(src, ref_name) {
                Some(c) => c,
                None if offline() => {
                    return Err(format!(
                        "dependency `{}` ({src} {ref_name}) is not fetched yet; run `almide build` in the project to fetch it",
                        dep.name
                    ))
                }
                None => {
                    let refs = remote_refs_once(src).map_err(|e| fetch_failure(&dep.name, &e))?;
                    let found = match dep.tag {
                        Some(_) => refs.tag_or_branch(ref_name),
                        None => refs.branch_or_tag(ref_name),
                    };
                    let Some(c) = found.and_then(|c| CommitId::parse(c).ok()) else {
                        return Err(missing_ref_error(dep, ref_name, chain, &RemoteRefs::from(&refs)));
                    };
                    write_index(&index_ref_file(src, ref_name), &format!("{c}\n"));
                    c
                }
            };
            use_tree(&dep.name, src, &commit, None)?
        }
        (None, false) => {
            let branch = dep.branch.as_deref();
            match remote_head_once(src, branch) {
                Ok(Some(commit)) => {
                    let tree = use_tree(&dep.name, src, &commit, None)?;
                    // Which commit the ref was last seen at, for the
                    // unreachable-remote fallback below.
                    write_index(&index_ref_file(src, ref_name), &format!("{commit}\n"));
                    tree
                }
                Ok(None) => {
                    let refs = remote_refs(src).unwrap_or_default();
                    return Err(missing_ref_error(dep, ref_name, chain, &refs));
                }
                Err(why) => {
                    lockable = false;
                    stale_cache_fallback(dep, src, chain, &why)?
                }
            }
        }
    };
    let lockable = lockable.then(|| (tree.commit.clone(), tree.sum));
    Ok(Fetched { dir: package_dir_in(dep, &tree.dir)?, lockable })
}

/// A floating ref whose remote could not be asked (#4527): the cached tree
/// that ref was last resolved to, with a warning naming its commit — the
/// same answer a cache-only resolve gives, said out loud — or, when nothing
/// is cached, an error naming the dependency and the way out.
fn stale_cache_fallback(dep: &Dependency, src: &GitSource, chain: &[String], why: &str) -> Result<Tree, String> {
    let ref_name = dep_ref_name(dep);
    let via = chain_text(chain);
    let cached = read_index_ref(src, ref_name).filter(|c| read_index_sum(src, c).is_some_and(|s| tree_dir(&s).is_dir()));
    match cached {
        Some(commit) => {
            let _offline = OfflineGuard(OFFLINE.with(|o| o.replace(true)));
            let tree = use_tree(&dep.name, src, &commit, None)?;
            err(&format!(
                "warning: could not reach {src} to resolve {ref_name} of dependency `{}`{via}; \
                 using the cached tree at {} (it may be behind the remote), and not locking it\n  \
                 fetch: {why}\n  \
                 hint: once the remote is reachable, the next build resolves {ref_name} again",
                dep.name,
                commit.short()
            ));
            Ok(tree)
        }
        None => Err(format!(
            "{}error: could not reach {src} to resolve {ref_name} of dependency `{}`{via}, and it is not in the cache\n  \
             fetch: {why}\n  \
             hint: connect to the network and run the command again, or pin the dependency with an `almide.lock` entry or a `tag`",
            dep.location_prefix(),
            dep.name
        )),
    }
}

/// ` (required by root → a → b)` for a chain of more than the dependency
/// itself, else nothing.
fn chain_text(chain: &[String]) -> String {
    if chain.len() > 1 { format!(" (required by {})", chain.join(" → ")) } else { String::new() }
}

/// The branch and tag names a remote has.
#[derive(Debug, Default)]
pub struct RemoteRefs {
    pub heads: Vec<String>,
    pub tags: Vec<String>,
}

impl From<&fetch::Refs> for RemoteRefs {
    fn from(r: &fetch::Refs) -> RemoteRefs {
        RemoteRefs { heads: r.heads(), tags: r.tags() }
    }
}

impl RemoteRefs {
    /// Whether `name` is a tag or a branch of the remote.
    pub fn has(&self, name: &str) -> bool {
        self.tags.iter().chain(self.heads.iter()).any(|r| r == name)
    }

    /// The newest `n` tags, newest first (version order, `v` ignored; a tag
    /// that is no version sorts below every version, by name).
    pub fn newest_tags(&self, n: usize) -> Vec<String> {
        let key = |t: &String| semver::Version::parse(t.trim_start_matches('v')).ok();
        let mut tags = self.tags.clone();
        tags.sort_by(|a, b| key(b).cmp(&key(a)).then_with(|| b.cmp(a)));
        tags.truncate(n);
        tags
    }
}

/// The remote's refs, asked once per process per source. `almide test`
/// resolves the dependencies on every worker thread; the memo keeps that to
/// one listing per source and makes every thread build the same commit even
/// if a branch moves mid-run (#4527).
fn remote_refs_once(src: &GitSource) -> Result<fetch::Refs, FetchError> {
    type Memo = std::collections::HashMap<String, Result<fetch::Refs, String>>;
    static MEMO: std::sync::OnceLock<std::sync::Mutex<Memo>> = std::sync::OnceLock::new();
    let mut memo = MEMO.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    let key = src.key();
    if let Some(hit) = memo.get(&key) {
        return hit.clone().map_err(FetchError::Unreachable);
    }
    let answer = fetch::list_refs(src);
    memo.insert(key, answer.as_ref().map(Clone::clone).map_err(|e| e.to_string()));
    answer
}

/// The remote's branch and tag names.
pub fn remote_refs(src: &GitSource) -> Result<RemoteRefs, String> {
    remote_refs_once(src).map(|r| RemoteRefs::from(&r)).map_err(|e| e.to_string())
}

/// The error for a dependency ref its remote does not have (#4528): which
/// dependency, which ref, the chain that requires it, and the newest tags
/// the remote does have.
pub fn missing_ref_error(dep: &Dependency, ref_name: &str, chain: &[String], refs: &RemoteRefs) -> String {
    let shown = dep.git.as_ref().map(|g| g.to_string()).unwrap_or_default();
    let kind = match (&dep.tag, &dep.branch) {
        (Some(_), _) => "tag",
        (None, Some(_)) => "branch",
        (None, None) => "its default branch",
    };
    let asked = match kind {
        "its default branch" => "its default branch".to_string(),
        k => format!("{k} {ref_name}"),
    };
    let tags = refs.newest_tags(5);
    let hint = match tags.is_empty() {
        true => format!("hint: {shown} has no tags; check the ref in the manifest, or push it first"),
        false => format!(
            "hint: the newest tags are {}{} — name one of them, or push {ref_name} first",
            tags.join(", "),
            if refs.tags.len() > tags.len() { ", …" } else { "" }
        ),
    };
    format!("{}error: dependency `{}`{} asks for {asked}, which {shown} does not have\n  {hint}", dep.location_prefix(), dep.name, chain_text(chain))
}

/// The current commit of a floating ref on its remote — `branch`, or the
/// default branch (`HEAD`) — from the per-process listing. `Ok(None)`: the
/// remote answered and has no such ref.
fn remote_head_once(src: &GitSource, branch: Option<&str>) -> Result<Option<CommitId>, String> {
    let refs = remote_refs_once(src).map_err(|e| e.to_string())?;
    let found = match branch {
        Some(b) => refs.refs.iter().find(|r| r.name == format!("refs/heads/{b}")).map(|r| r.commit.as_str()),
        None => refs.head(),
    };
    Ok(found.and_then(|c| CommitId::parse(c).ok()))
}

/// One fetch the walk made: the request as the lock would name it, the
/// `PkgId` MVS files it under, and the package directory it produced.
///
/// `update_lock_file` needs all three. The request identifies WHICH direct
/// dependency this was (the lookup key the lock is matched on); the `PkgId`
/// and the directory say which fetch MVS finally SELECTED for that package,
/// and the selected fetch is what the lock records (#2532).
#[derive(Debug, Clone)]
pub struct Resolution {
    pub lock: LockedDep,
    pub pkg_id: PkgId,
    pub source_dir: PathBuf,
}

/// Update almide.lock after fetching all dependencies.
///
/// The lock records the RESOLVED result, not the request: for each direct
/// dependency, the `(git, ref, commit, sum)` of the fetch Minimal Version
/// Selection chose for its package — the tree that was actually built
/// (#2532). When nothing raises the dependency that is its own request.
/// When a transitive requirement raises it (the manifest asks `foo@v1.0.0`,
/// a dependency asks `foo@v1.1.0`, v1.1.0 is built), the entry names v1.1.0
/// and v1.1.0's commit. The request lives only in `almide.toml`.
///
/// `resolved` is every fetch the walk made, in walk order; `fetched` is the
/// selection. The direct dependency's own fetch is found by its identity —
/// name, source and ref, the key the lock lookup also uses (#2522). The
/// selected fetch is then the one whose directory MVS kept for that
/// `PkgId`, so every entry is one fetch's own record (#2529).
pub fn update_lock_file(project_root: &Path, deps: &[Dependency], resolved: &[Resolution], fetched: &[FetchedDep]) -> Result<(), String> {
    let locked = resolved_lock_entries(deps, resolved, fetched);
    if !locked.is_empty() {
        write_lock_file(&project_root.join("almide.lock"), &locked)?;
    }
    Ok(())
}

/// The entries [`update_lock_file`] writes: one per direct git dependency,
/// the `(git, ref, commit, sum)` MVS selected for it.
fn resolved_lock_entries(deps: &[Dependency], resolved: &[Resolution], fetched: &[FetchedDep]) -> Vec<LockedDep> {
    let mut locked = Vec::new();
    for dep in deps {
        let Some(src) = &dep.git else { continue };
        let ref_name = dep_ref_name(dep);
        let Some(request) = resolved
            .iter()
            .find(|r| r.lock.name == dep.name && r.lock.git == *src && r.lock.ref_name == ref_name && r.lock.subdir == dep.subdir)
        else {
            // Nothing was resolved for it — a lock entry invented for it
            // here would be a false record.
            continue;
        };
        let selected = fetched
            .iter()
            .find(|f| f.pkg_id == request.pkg_id)
            .and_then(|f| resolved.iter().find(|r| r.pkg_id == f.pkg_id && r.source_dir == f.source_dir));
        // `None` means MVS selected a package with no git identity (a path
        // dependency raised this one): what was built is not a commit, so
        // there is nothing true to pin.
        if let Some(selected) = selected {
            locked.push(LockedDep { name: dep.name.clone(), ..selected.lock.clone() });
        }
    }
    locked
}

fn resolve_dep_version(dep: &Dependency) -> String {
    if let Some(ref ver) = dep.version {
        let cleaned = ver.trim_start_matches(|c: char| !c.is_ascii_digit());
        let parts: Vec<&str> = cleaned.split('.').collect();
        match parts.len() {
            1 => format!("{}.0.0", parts[0]),
            2 => format!("{}.{}.0", parts[0], parts[1]),
            _ => cleaned.to_string(),
        }
    } else if let Some(ref tag) = dep.tag {
        let v = tag.trim_start_matches('v');
        let parts: Vec<&str> = v.split('.').collect();
        match parts.len() {
            1 => format!("{}.0.0", parts[0]),
            2 => format!("{}.{}.0", parts[0], parts[1]),
            _ => v.to_string(),
        }
    } else {
        "0.0.0".to_string()
    }
}

/// Fetch all dependencies recursively and return FetchedDep list.
/// Same-name deps with same major version are unified; different majors coexist.
/// If almide.lock exists, uses locked commit hashes and sums for reproducibility.
pub fn fetch_all_deps(project: &Project) -> Result<Vec<FetchedDep>, String> {
    let lock_path = project.root.join("almide.lock");
    // A corrupt lock is an ERROR, never an empty set: swallowing it here used
    // to silently re-resolve every dependency from the network, defeating the
    // reproducibility the lock exists for (#1465).
    let locked = if lock_path.exists() { parse_lock_file(&lock_path)? } else { Vec::new() };

    let locked_mode = locked_mode();
    if locked_mode {
        refuse_unlocked_direct_deps(project, &locked)?;
    }

    let mut walk = Walk {
        root: &project.package.name,
        locked: &locked,
        trusted: &project.trust_owners,
        fetched: Vec::new(),
        visited: std::collections::HashSet::new(),
        resolved: Vec::new(),
    };
    walk.deps(&project.dependencies, &[])?;
    let Walk { fetched, resolved, .. } = walk;
    refuse_changed_sources(project, &locked, &resolved)?;

    if !project.dependencies.is_empty() {
        if locked_mode {
            let would_write = resolved_lock_entries(&project.dependencies, &resolved, &fetched);
            if !would_write.is_empty() && would_write != locked {
                return Err("error: --locked: almide.lock does not match what the dependencies resolve to, and --locked writes no lock\n  \
                     hint: run the command without --locked to update almide.lock, review the diff, and commit it"
                    .to_string());
            }
        } else if !offline() {
            // Update lock file if it doesn't exist or deps changed. A
            // cache-only resolve (`resolve_cached_deps`) never writes it (#4128).
            let _ = update_lock_file(&project.root, &project.dependencies, &resolved, &fetched);
        }
    }

    Ok(fetched)
}

static LOCKED_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `--locked` (#4093): every fetch must be satisfied by `almide.lock`, and
/// the lock is never written. Set once from the command line, before
/// anything fetches.
pub fn set_locked_mode(on: bool) {
    LOCKED_MODE.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn locked_mode() -> bool {
    LOCKED_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// `--locked`, before any network access: every direct git dependency has a
/// lock entry for its name, source and ref, and every entry has a `sum`
/// (ADR-0035 D4: a lock written before the tree hash pins a commit only).
fn refuse_unlocked_direct_deps(project: &Project, locked: &[LockedDep]) -> Result<(), String> {
    let entry = |d: &Dependency| locked.iter().find(|l| l.name == d.name && Some(&l.git) == d.git.as_ref() && l.ref_name == dep_ref_name(d));
    let missing: Vec<String> = project
        .dependencies
        .iter()
        .filter(|d| d.path.is_none() && entry(d).is_none())
        .map(|d| format!("`{}` ({} {})", d.name, d.git.as_ref().map(|g| g.to_string()).unwrap_or_default(), dep_ref_name(d)))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "error: --locked: not in almide.lock: {}\n  \
             note: --locked fetches nothing the lock does not name, and writes no lock\n  \
             hint: run the command once without --locked to lock it, review the almide.lock diff, and commit it",
            missing.join(", ")
        ));
    }
    let unsummed: Vec<String> = locked.iter().filter(|l| l.sum.is_none()).map(|l| format!("`{}`", l.name)).collect();
    if !unsummed.is_empty() {
        return Err(format!(
            "error: --locked: almide.lock has no `sum` for {}\n  \
             note: a lock written before tree sums pins a commit only; --locked needs the content hash too\n  \
             hint: run the command once without --locked to add the sums, review the almide.lock diff, and commit it",
            unsummed.join(", ")
        ));
    }
    Ok(())
}

/// A direct dependency whose lock entry names another source than the
/// manifest does (#4093): the manifest's `git` changed under a lock. It used
/// to be re-resolved silently from the new source; now the change has to be
/// asked for with `almide update <name>`.
///
/// Judged after the walk, so a lock entry that names another source because
/// MVS selected a same-named package raised by a transitive requirement
/// (#2532) — a source the walk did fetch — is not mistaken for a change.
fn refuse_changed_sources(project: &Project, locked: &[LockedDep], resolved: &[Resolution]) -> Result<(), String> {
    for dep in project.dependencies.iter() {
        let Some(src) = &dep.git else { continue };
        let Some(entry) = locked.iter().find(|l| l.name == dep.name) else { continue };
        if entry.git == *src || resolved.iter().any(|r| r.lock.git == entry.git) {
            continue;
        }
        return Err(format!(
            "error: `{name}` is locked to {old} but almide.toml now says {new}\n  \
             hint: if the change is intended, run `almide update {name}` to re-lock it from the new source",
            name = dep.name,
            old = entry.git,
            new = src,
        ));
    }
    Ok(())
}

/// After fetching a dependency at `path`, read its own `almide.toml` (if
/// any) for its declared package name, src dir, and transitive deps —
/// falling back to the requesting `Dependency`'s own name when there's no
/// manifest. A manifest that does not read is an error naming the
/// dependency: skipping it would drop its requirements silently, and a
/// requirement in a refused form (ADR-0035 D1) would vanish instead of
/// being reported.
fn resolve_fetched_dep_manifest(path: &Path, fallback_name: &str, chain: &[String]) -> Result<(String, PathBuf, Vec<Dependency>), String> {
    let src_dir = if path.join("src").is_dir() { path.join("src") } else { path.to_path_buf() };
    let dep_toml = path.join("almide.toml");
    if !dep_toml.exists() {
        return Ok((fallback_name.to_string(), src_dir, vec![]));
    }
    match parse_toml(&dep_toml) {
        Ok(dep_project) => Ok((dep_project.package.name, src_dir, dep_project.dependencies)),
        Err(e) => Err(format!("error: the manifest of dependency `{fallback_name}`{} is refused:\n{e}", chain_text(chain))),
    }
}

/// MVS ordering: parse as semver, treating an unparseable version as the
/// minimum (branch deps already resolve to "0.0.0" upstream).
fn semver_of(v: &str) -> semver::Version {
    semver::Version::parse(v).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
}

/// The dependency walk's state: the lock, the root's `[trust] owners`, and
/// what the walk has fetched so far.
///
/// Selection is Minimal Version Selection, the rule
/// `docs/specs/package-system.md` documents: per `PkgId` (name + major) the
/// MAXIMUM requested version wins, and the walk visits every distinct
/// requested version's own requirement list (#1458).
struct Walk<'a> {
    /// The root package's name: the first link of the chain a fetch error
    /// names (#4528).
    root: &'a str,
    locked: &'a [LockedDep],
    /// The ROOT manifest's `[trust] owners` (#4093). A dependency's own
    /// `[trust]` is never consulted.
    trusted: &'a [String],
    fetched: Vec<FetchedDep>,
    visited: std::collections::HashSet<String>,
    /// What each dependency RESOLVED to, recorded as the walk fetches it
    /// (#2529).
    resolved: Vec<Resolution>,
}

impl Walk<'_> {
    /// `deps`, required through `chain` (the dependency names from the root).
    fn deps(&mut self, deps: &[Dependency], chain: &[String]) -> Result<(), String> {
        for dep in deps {
            let mut chain = chain.to_vec();
            chain.push(dep.name.clone());
            // #4093: the owner is checked before anything is fetched, for a
            // direct, transitive or locked dependency alike (a lock is not a
            // grant). The source was read once, into a typed value, by the
            // manifest reader (ADR-0035 D1).
            if let Some(src) = &dep.git {
                if !crate::trust::is_trusted(self.trusted, src) {
                    return Err(crate::trust::fetch_refusal(&chain, src));
                }
            }
            fetch_one_dep_recursive(dep, self, &chain)?;
        }
        Ok(())
    }
}

fn fetch_one_dep_recursive(dep: &Dependency, walk: &mut Walk, chain: &[String]) -> Result<(), String> {
    let locked = walk.locked;
    let (fetched, visited) = (&walk.fetched, &mut walk.visited);
    let version_str = resolve_dep_version(dep);
    let pkg_id = PkgId::from_version_str(&dep.name, &version_str);

    // A package is a source AND a directory in it (#3381): two subdirs of
    // one repository are two packages, each walked for its own requirements.
    let visit_key = match (&dep.path, &dep.git) {
        (Some(p), _) => format!("path:{}@{}", p, version_str),
        (None, Some(g)) => format!("{}#{}@{}", g, dep.subdir.as_deref().unwrap_or(""), version_str),
        (None, None) => format!("?{}@{}", dep.name, version_str),
    };
    if visited.contains(&visit_key) {
        return Ok(());
    }
    visited.insert(visit_key);
    // ADR-0030: the dependency graph a manifest can ask for is bounded
    // (`dependency_count`, `dependency_depth`), before anything is fetched.
    {
        use almide_base::budget::{self, BudgetId};
        let (count, depth) = (visited.len() as u64, chain.len() as u64);
        budget::observe(BudgetId::DependencyCount, count);
        budget::observe(BudgetId::DependencyDepth, depth);
        if count > budget::limit(BudgetId::DependencyCount) {
            return Err(format!(
                "error: too many dependencies ({count}, max {}): the dependency graph reached `{}` through {}\n  \
                 hint: a real project needs far fewer; a manifest that asks for more is refused before it is fetched \
                 (`--budget dependency_count=N` on the command line raises the budget)",
                budget::limit(BudgetId::DependencyCount),
                dep.name,
                chain.join(" → "),
            ));
        }
        if depth > budget::limit(BudgetId::DependencyDepth) {
            return Err(format!(
                "error: dependency chain too deep ({depth}, max {}): {}\n  \
                 hint: a manifest that nests dependencies this deep is refused before it is fetched \
                 (`--budget dependency_depth=N` on the command line raises the budget)",
                budget::limit(BudgetId::DependencyDepth),
                chain.join(" → "),
            ));
        }
    }

    // Detect different major versions of the same package (diamond with version split)
    if let Some(existing) = fetched.iter().find(|f| f.pkg_id.name == pkg_id.name && f.pkg_id.major != pkg_id.major) {
        err(&format!("warning: package '{}' required at two different major versions", pkg_id.name));
        err(&format!("  → {} (already loaded)", existing.pkg_id));
        err(&format!("  → {} (newly required)", pkg_id));
        err(&format!("  Both versions will coexist. Types from v{} and v{} are incompatible.", existing.pkg_id.major, pkg_id.major));
    }

    // Use the locked commit only when the lock entry describes the SAME
    // source at the SAME ref the manifest asks for (#2522). An entry whose
    // `ref` is absent is a mismatch too. The match is on the SOURCE (git,
    // ref), not on the manifest key: the lock records what MVS selected
    // (#2532), so an entry raised by a transitive requirement names the ref
    // that requirement asked for.
    let want_ref = dep_ref_name(dep);
    let pin = dep.git.as_ref().and_then(|g| {
        locked.iter().find(|l| l.git == *g && l.ref_name == want_ref).map(|l| (l.commit.clone(), l.sum))
    });
    let shown: Vec<String> = std::iter::once(walk.root.to_string()).chain(chain.iter().cloned()).collect();
    let Fetched { dir: path, lockable } = fetch_dep_in_chain(dep, pin, &shown)?;
    let (fetched, resolved) = (&mut walk.fetched, &mut walk.resolved);

    let (module_name, source_dir, transitive_deps) = resolve_fetched_dep_manifest(&path, &dep.name, &shown)?;

    let actual_pkg_id = PkgId::from_version_str(&module_name, &version_str);
    // Record what THIS dependency resolved to, here, where the manifest entry
    // and the tree it produced are both in hand (#2529).
    if let (Some(git), Some((commit, sum))) = (&dep.git, lockable) {
        resolved.push(Resolution {
            lock: LockedDep {
                name: dep.name.clone(),
                git: git.clone(),
                ref_name: want_ref.to_string(),
                commit,
                sum: Some(sum),
                subdir: dep.subdir.clone(),
            },
            pkg_id: actual_pkg_id.clone(),
            source_dir: source_dir.clone(),
        });
    }
    // #4834: a package is its origin, not the name it declares for itself.
    // Two origins that declare one name (at any major) would be one import
    // name for two packages: MVS would put one in the other's place, and a
    // grant by that name would follow the swap. Refused, naming both.
    let origin = crate::project::PackageOrigin::of(dep);
    let required_by = shown.join(" → ");
    if let Some(other) = fetched.iter().find(|f| f.pkg_id.name == actual_pkg_id.name && f.origin != origin) {
        return Err(format!(
            "error: two sources declare the package `{name}`\n  \
             {} (required by {})\n  \
             {origin} (required by {required_by})\n  \
             note: `import {name}` can name only one of them; almide never picks one over the other\n  \
             hint: depend on `{name}` from one source; if both are needed, one of them must be published under another package name",
            other.origin,
            other.required_by,
            name = actual_pkg_id.name,
        ));
    }
    match fetched.iter_mut().find(|f| f.pkg_id == actual_pkg_id) {
        Some(existing) => {
            // MVS within one origin: a later, higher request replaces the
            // selection in place — position (and therefore lock-file
            // alignment) is preserved.
            if semver_of(&version_str) > semver_of(&existing.version) {
                existing.version = version_str;
                existing.source_dir = source_dir;
                existing.package_dir = path;
                existing.source = dep.git.clone();
            }
        }
        None => fetched.push(FetchedDep {
            pkg_id: actual_pkg_id,
            origin,
            required_by,
            source: dep.git.clone(),
            version: version_str,
            source_dir,
            package_dir: path,
        }),
    }

    if !transitive_deps.is_empty() {
        walk.deps(&transitive_deps, chain)?;
    }
    Ok(())
}

/// Resolve a short package specifier to a full git URL and optional tag.
///
/// A one-part name is a package under github.com/almide, looked up in the
/// embedded package snapshot (`crate::trust::almide_repo_for`, #4093)
/// because a package's name and its repository's name differ: `bindgen` is
/// `github.com/almide/almide-bindgen`. A name the snapshot does not list
/// resolves to `github.com/almide/<name>`. `almide/<repo>` names the package
/// the snapshot gives that repository.
pub fn resolve_package_spec(spec: &str) -> (String, String, Option<String>) {
    let (path, tag) = if let Some(pos) = spec.rfind('@') { (&spec[..pos], Some(spec[pos + 1..].to_string())) } else { (spec, None) };

    let parts: Vec<&str> = path.split('/').collect();
    let (git_url, name) = match parts.len() {
        1 => {
            let (name, repo) = crate::trust::almide_repo_for(parts[0]);
            (format!("https://github.com/almide/{}", repo), name.to_string())
        }
        2 if parts[0] == "almide" => {
            let (name, repo) = crate::trust::almide_repo_for(parts[1]);
            (format!("https://github.com/almide/{}", repo), name.to_string())
        }
        2 => (format!("https://github.com/{}/{}", parts[0], parts[1]), parts[1].to_string()),
        _ if parts[0].contains('.') => {
            let name = parts.last().expect("split always yields ≥1 element").to_string();
            (format!("https://{}", path), name)
        }
        _ => (format!("https://github.com/{}", path), parts.last().expect("split always yields ≥1 element").to_string()),
    };

    (name, git_url, tag)
}

/// Resolve the `almide add` arguments to `(name, git_url, tag)`. `--tag`
/// applies to both spellings; a `pkg@tag` suffix in the spec is the fallback
/// when `--tag` is absent.
///
/// With `--subdir` (#3381) the spec names the REPOSITORY, so the package
/// name defaults to the subdir's last component (`--subdir pkgs/ceangal` →
/// `ceangal`) rather than the repository's name. With `--git` the first
/// argument is the name, as before.
pub fn resolve_add_target(pkg: String, git: Option<String>, tag: Option<String>, subdir: Option<&str>) -> (String, String, Option<String>) {
    if let Some(git_url) = git {
        return (pkg, git_url, tag);
    }
    let (name, git_url, spec_tag) = resolve_package_spec(&pkg);
    let name = subdir.and_then(|s| s.rsplit('/').next()).map(str::to_string).unwrap_or(name);
    (name, git_url, tag.or(spec_tag))
}

/// Add a dependency to almide.toml. `subdir` must already be normalized
/// (`crate::project::normalize_subdir`); `git` is written in its canonical
/// form.
pub fn add_dep_to_toml(name: &str, git: &GitSource, tag: Option<&str>, subdir: Option<&str>) -> Result<(), String> {
    // Package names must be valid Almide identifiers (no hyphens).
    // The package name IS the import name — no implicit conversion.
    if name.contains('-') {
        return Err(format!(
            "package name '{}' contains hyphens — use underscores instead\n  \
             hint: use '{}' as the package name (rename in the dependency's almide.toml)",
            name,
            name.replace('-', "_"),
        ));
    }

    let toml_path = Path::new("almide.toml");
    if !toml_path.exists() {
        return Err("almide.toml not found. Run 'almide init' first.".into());
    }

    let mut content = crate::project::read_project_file(toml_path)?;

    let tag_part = tag.map(|t| format!(", tag = \"{t}\"")).unwrap_or_default();
    let subdir_part = subdir.map(|s| format!(", subdir = \"{s}\"")).unwrap_or_default();
    let dep_line = format!("{} = {{ git = \"{}\"{}{} }}", name, git, tag_part, subdir_part);

    if content.contains("[dependencies]") {
        content = content.replacen("[dependencies]", &format!("[dependencies]\n{}", dep_line), 1);
    } else {
        content.push_str(&format!("\n[dependencies]\n{}\n", dep_line));
    }

    std::fs::write(toml_path, content).map_err(|e| format!("Failed to write almide.toml: {}", e))?;

    Ok(())
}

/// `almide update [dep]` (#1131): advance a LOCKED git dependency to its
/// ref's current remote head.
///
/// The lock is intentionally sticky — `fetch_all_deps` reuses its commit so
/// builds reproduce, and `almide add` on an existing dep re-writes the same
/// pin. This is the path FORWARD — it rewrites only the named entries,
/// leaving every other pin byte-identical. The new head is fetched, so its
/// entry carries the new tree's `sum`.
pub fn update_locked_deps(project: &Project, only: Option<&str>) -> Result<Vec<(String, String, String)>, String> {
    let lock_path = project.root.join("almide.lock");
    let mut locked = if lock_path.exists() { parse_lock_file(&lock_path)? } else { Vec::new() };
    let targets: Vec<&Dependency> = match only {
        Some(name) => {
            let found: Vec<&Dependency> = project.dependencies.iter().filter(|d| d.name == name).collect();
            if found.is_empty() {
                return Err(format!("Dependency '{}' not found in almide.toml", name));
            }
            found
        }
        None => project.dependencies.iter().collect(),
    };
    let mut changed = Vec::new();
    let mut relock = Vec::new();
    for dep in targets {
        let ref_name = dep_ref_name(dep);
        let Some(src) = &dep.git else { continue };
        // #4093: `update` reaches the network too, so it checks the owner
        // first, like every fetch.
        if !crate::trust::is_trusted(&project.trust_owners, src) {
            return Err(crate::trust::fetch_refusal(std::slice::from_ref(&dep.name), src));
        }
        // The manifest's source changed under the lock: drop the old pin and
        // re-lock from the new source below, tag or not.
        if let Some(pos) = locked.iter().position(|l| l.name == dep.name && l.git != *src) {
            let old = locked.remove(pos);
            relock.push((dep.name.clone(), old.commit.to_string()));
            continue;
        }
        // A tag pins by definition — advancing it would silently change what
        // the manifest asked for. Only floating refs (branches, the default
        // `main`) move.
        if dep.tag.is_some() {
            err(&format!("{} is pinned to tag {} — not updated", dep.name, ref_name));
            continue;
        }
        let head = match remote_head_once(src, dep.branch.as_deref()) {
            Ok(Some(head)) => head,
            Ok(None) => {
                let chain = [project.package.name.clone(), dep.name.clone()];
                return Err(missing_ref_error(dep, ref_name, &chain, &remote_refs(src).unwrap_or_default()));
            }
            Err(e) => return Err(format!("listing the refs of {} failed: {}", src, e)),
        };
        let before = locked.iter().find(|l| l.name == dep.name).map(|l| l.commit.clone());
        if before.as_ref() == Some(&head) {
            err(&format!("{} already at {} ({})", dep.name, head.short(), ref_name));
            continue;
        }
        let tree = use_tree(&dep.name, src, &head, None)?;
        write_index(&index_ref_file(src, ref_name), &format!("{head}\n"));
        match locked.iter_mut().find(|l| l.name == dep.name) {
            Some(entry) => {
                entry.git = src.clone();
                entry.ref_name = ref_name.to_string();
                entry.commit = head.clone();
                entry.sum = Some(tree.sum);
                entry.subdir = dep.subdir.clone();
            }
            None => locked.push(LockedDep {
                name: dep.name.clone(),
                git: src.clone(),
                ref_name: ref_name.to_string(),
                commit: head.clone(),
                sum: Some(tree.sum),
                subdir: dep.subdir.clone(),
            }),
        }
        changed.push((dep.name.clone(), before.map(|c| c.to_string()).unwrap_or_default(), head.to_string()));
    }
    if !changed.is_empty() || !relock.is_empty() {
        write_lock_file(&lock_path, &locked)?;
    }
    if !relock.is_empty() {
        // Resolve the new sources and write their pins, as a build would.
        fetch_all_deps(project)?;
        let fresh = parse_lock_file(&lock_path)?;
        for (name, before) in relock {
            let after = fresh.iter().find(|l| l.name == name).map(|l| l.commit.to_string()).unwrap_or_default();
            changed.push((name, before, after));
        }
    }
    Ok(changed)
}

/// Why [`probe_source`] could not list a repository.
#[derive(Debug)]
pub enum ProbeFailure {
    /// The repository does not exist (or is private to someone else): the
    /// host said so.
    NotFound(String),
    /// Anything else — no network, a host that did not answer.
    Unreachable(String),
}

/// Does the repository at `src` exist, and which refs does it have? One
/// listing, no fetch. The listing lets `almide add` refuse a tag the
/// repository does not have (#4528).
pub fn probe_source(src: &GitSource) -> Result<RemoteRefs, ProbeFailure> {
    match remote_refs_once(src) {
        Ok(r) => Ok(RemoteRefs::from(&r)),
        Err(FetchError::NotFound(s)) => Err(ProbeFailure::NotFound(s)),
        Err(e) => Err(ProbeFailure::Unreachable(e.to_string())),
    }
}

/// The remote's current commit for `ref_name` (`HEAD`, a branch or a tag),
/// from one listing, without fetching.
pub fn git_remote_head(src: &GitSource, ref_name: &str) -> Result<CommitId, String> {
    let refs = remote_refs_once(src).map_err(|e| format!("listing the refs of {src} failed: {e}"))?;
    let found = if ref_name == "HEAD" { refs.head() } else { refs.branch_or_tag(ref_name) };
    found.and_then(|c| CommitId::parse(c).ok()).ok_or_else(|| format!("no ref '{}' at {}", ref_name, src))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, git: &str, tag: Option<&str>) -> Dependency {
        Dependency {
            name: name.into(),
            git: Some(GitSource::parse(git).unwrap()),
            tag: tag.map(str::to_string),
            branch: None,
            version: None,
            path: None,
            subdir: None,
            declared_at: None,
        }
    }

    /// ADR-0035 D4: the index of a source is keyed by a hash of its
    /// canonical form, so the spellings of one source share it and no two
    /// sources do; the tree directory is named by the sum alone.
    #[test]
    fn the_index_is_keyed_by_the_typed_source_and_the_tree_by_its_sum() {
        let idx = |s: &str| index_dir(&GitSource::parse(s).unwrap());
        assert_eq!(idx("https://github.com/one/fizz"), idx("https://github.com/one/fizz.git"));
        assert_eq!(idx("https://GitHub.com/one/fizz"), idx("https://github.com:443/one/fizz"));
        let distinct = [
            "https://github.com/one/fizz",
            "https://github.com/two/fizz",
            "https://github.com/One/fizz",
            "https://github.com/one/fizz/x",
            "ssh://git@github.com/one/fizz",
            "https://github.com:8443/one/fizz",
        ];
        for (i, a) in distinct.iter().enumerate() {
            for b in &distinct[i + 1..] {
                assert_ne!(idx(a), idx(b), "{a} and {b} share an index");
            }
        }
        let sum = TreeSum::of(std::iter::once(("a", package_tree::sha256(b"x"))));
        assert_eq!(tree_dir(&sum), cache_dir().join("trees").join(sum.hex()));
        assert_eq!(sum.hex().len(), 64);
    }

    #[test]
    fn add_tag_flag_is_kept_for_short_specs() {
        let (name, git, tag) = resolve_add_target("almide/svg".into(), None, Some("v0.1.0".into()), None);
        assert_eq!(name, "svg");
        assert_eq!(git, "https://github.com/almide/svg");
        assert_eq!(tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn add_tag_flag_wins_over_spec_suffix_and_suffix_is_fallback() {
        let (_, _, tag) = resolve_add_target("svg@v0.2.0".into(), None, Some("v0.1.0".into()), None);
        assert_eq!(tag.as_deref(), Some("v0.1.0"));
        let (_, _, tag) = resolve_add_target("svg@v0.2.0".into(), None, None, None);
        assert_eq!(tag.as_deref(), Some("v0.2.0"));
    }

    /// #3381: with `--subdir` the spec names the repository; the package is
    /// named after the subdir's last component.
    #[test]
    fn add_subdir_names_the_package_after_the_subdir() {
        let (name, git, tag) =
            resolve_add_target("almide-graphics/ceangal2@v0.1.0".into(), None, None, Some("pkgs/ceangal"));
        assert_eq!(name, "ceangal");
        assert_eq!(git, "https://github.com/almide-graphics/ceangal2");
        assert_eq!(tag.as_deref(), Some("v0.1.0"));
        // `--git` keeps the first argument as the name.
        let (name, _, _) = resolve_add_target("cg".into(), Some("https://example.org/r/cg".into()), None, Some("ceangal"));
        assert_eq!(name, "cg");
    }

    /// #4128: a cache-only resolve of a git dependency that is not cached
    /// fails without cloning and without writing almide.lock; path
    /// dependencies still resolve.
    #[test]
    fn resolve_cached_deps_never_fetches_or_writes_the_lock() {
        let root = std::env::temp_dir().join(format!("almide-offline-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("app");
        let local = root.join("local");
        std::fs::create_dir_all(app.join("src")).unwrap();
        std::fs::create_dir_all(local.join("src")).unwrap();
        std::fs::write(local.join("almide.toml"), "[package]\nname = \"local\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(local.join("src/mod.almd"), "fn f() -> Int = 1\n").unwrap();
        // A git source no cache entry exists for.
        let git = format!("https://github.com/almide/no-such-repo-{}", std::process::id());
        std::fs::write(
            app.join("almide.toml"),
            format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlocal = {{ path = \"{}\" }}\nremote = {{ git = \"{git}\", tag = \"v0.1.0\" }}\n", local.display()),
        )
        .unwrap();
        let project = parse_toml(&app.join("almide.toml")).unwrap();

        let err = resolve_cached_deps(&project).unwrap_err();
        assert!(err.contains("not fetched yet"), "{err}");
        assert!(!app.join("almide.lock").exists(), "a cache-only resolve wrote almide.lock");
        // The guard is restored: offline mode does not leak past the call.
        assert!(!offline());

        // Path-only projects still resolve offline.
        std::fs::write(
            app.join("almide.toml"),
            format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlocal = {{ path = \"{}\" }}\n", local.display()),
        )
        .unwrap();
        let project = parse_toml(&app.join("almide.toml")).unwrap();
        let deps = resolve_cached_deps(&project).unwrap();
        assert_eq!(deps.len(), 1);
        assert!(!app.join("almide.lock").exists(), "a cache-only resolve wrote almide.lock");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #3381: the checks the fetch makes on a `subdir` — a package
    /// directory whose manifest names this dependency, else a refusal that
    /// lists the packages the repository does hold.
    #[test]
    fn package_dir_in_checks_the_subdir_against_the_checkout() {
        let root = std::env::temp_dir().join(format!("almide-subdir-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pkg = |dir: &str, name: &str| {
            let d = root.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("almide.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")).unwrap();
        };
        pkg("ceangal", "ceangal");
        pkg("snaidhm", "snaidhm");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        let with = |name: &str, sub: Option<&str>| Dependency {
            subdir: sub.map(str::to_string),
            declared_at: Some("almide.toml:7".into()),
            ..dep(name, "https://github.com/almide/repo", Some("v0.1.0"))
        };

        assert_eq!(package_dir_in(&with("ceangal", Some("ceangal")), &root).unwrap(), root.join("ceangal"));

        let missing = package_dir_in(&with("ceangal", Some("nope")), &root).unwrap_err();
        assert!(missing.starts_with("almide.toml:7: subdir `nope`"), "{missing}");
        assert!(missing.contains("does not exist") && missing.contains("`ceangal`, `snaidhm`"), "{missing}");

        let no_manifest = package_dir_in(&with("docs", Some("docs")), &root).unwrap_err();
        assert!(no_manifest.contains("has no almide.toml"), "{no_manifest}");

        let mismatch = package_dir_in(&with("ceangal", Some("snaidhm")), &root).unwrap_err();
        assert!(mismatch.contains("whose package is `snaidhm`") && mismatch.contains("rename the key to `snaidhm`"), "{mismatch}");

        // No subdir, and the root is no package: say which subdir to write.
        let root_only = package_dir_in(&with("ceangal", None), &root).unwrap_err();
        assert!(root_only.contains("needs a `subdir`") && root_only.contains("`subdir = \"ceangal\"`"), "{root_only}");

        #[cfg(unix)]
        {
            let outside = root.with_extension("outside");
            let _ = std::fs::remove_dir_all(&outside);
            std::fs::create_dir_all(&outside).unwrap();
            std::fs::write(outside.join("almide.toml"), "[package]\nname = \"esc\"\n").unwrap();
            std::os::unix::fs::symlink(&outside, root.join("esc")).unwrap();
            let escaped = package_dir_in(&with("esc", Some("esc")), &root).unwrap_err();
            assert!(escaped.contains("resolves outside the repository"), "{escaped}");
            let _ = std::fs::remove_dir_all(&outside);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn populate_cache_dir_survives_concurrent_callers() {
        let root = std::env::temp_dir().join(format!("almide-populate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("dep").join("abcdef123456");
        let handles: Vec<_> = (0..8).map(|i| {
            let dir = dir.clone();
            std::thread::spawn(move || {
                populate_cache_dir(&dir, |tmp| {
                    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    std::fs::write(tmp.join("marker"), format!("{}", i)).map_err(|e| e.to_string())
                })
            })
        }).collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        // Exactly one complete copy is in place and no temp dirs are left behind.
        assert!(dir.join("marker").is_file());
        let leftovers: Vec<_> = std::fs::read_dir(dir.parent().unwrap()).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != "abcdef123456" && n != ".abcdef123456.lock")
            .collect();
        assert!(leftovers.is_empty(), "leftover temp dirs: {:?}", leftovers);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn populate_cache_dir_leaves_nothing_when_clone_fails() {
        let root = std::env::temp_dir().join(format!("almide-populate-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("dep").join("deadbeef0000");
        let r = populate_cache_dir(&dir, |tmp| {
            std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
            Err("boom".to_string())
        });
        assert_eq!(r, Err("boom".to_string()));
        assert!(!dir.exists());
        // Only the entry's lock file is left (it is never deleted, see
        // `CacheEntryLock`); no temp dir.
        let left: Vec<String> = std::fs::read_dir(dir.parent().unwrap()).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec![".deadbeef0000.lock".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #4529: callers racing on one cache entry run ONE clone; the others
    /// wait for it under the entry's lock and reuse its checkout. Without
    /// the lock every caller cloned, which is what invited the remote to
    /// turn some of them away.
    #[test]
    fn concurrent_callers_of_one_entry_clone_once() {
        let root = std::env::temp_dir().join(format!("almide-populate-once-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("dep").join("0123456789ab");
        let clones = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let handles: Vec<_> = (0..8).map(|_| {
            let (dir, clones) = (dir.clone(), clones.clone());
            std::thread::spawn(move || {
                populate_cache_dir(&dir, |tmp| {
                    clones.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    std::fs::write(tmp.join("marker"), "x").map_err(|e| e.to_string())
                })
            })
        }).collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        assert!(dir.join("marker").is_file());
        assert_eq!(clones.load(std::sync::atomic::Ordering::SeqCst), 1, "every racing caller cloned");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #4529: a caller whose clone failed, but which finds the entry filled
    /// by then (by a process that does not take the lock), succeeds instead
    /// of failing the whole command.
    #[test]
    fn a_failed_clone_succeeds_when_the_entry_was_filled_meanwhile() {
        let root = std::env::temp_dir().join(format!("almide-populate-filled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("dep").join("feedface0000");
        let r = populate_cache_dir(&dir, |tmp| {
            // A sibling (no lock: an older almide) renames its finished
            // clone into place while ours is failing.
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("marker"), "sibling").map_err(|e| e.to_string())?;
            std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
            Err("early EOF".to_string())
        });
        assert_eq!(r, Ok(()));
        assert!(dir.join("marker").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// #4529: `update`'s removal of a ref-keyed checkout waits for the
    /// entry's lock, so it cannot delete a checkout a fetch is populating.
    #[test]
    fn removing_an_entry_waits_for_its_lock() {
        let root = std::env::temp_dir().join(format!("almide-remove-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("dep").join("main");
        std::fs::create_dir_all(&dir).unwrap();
        let held = CacheEntryLock::acquire(&dir).unwrap();
        let remover = {
            let dir = dir.clone();
            std::thread::spawn(move || remove_cache_dir(&dir))
        };
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(dir.exists(), "the checkout was removed while its lock was held");
        drop(held);
        remover.join().unwrap().unwrap();
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remote_refs_order_tags_newest_first() {
        let refs = RemoteRefs {
            heads: vec!["main".into()],
            tags: vec!["v0.1.0".into(), "v0.10.0".into(), "v0.2.0".into(), "nightly".into()],
        };
        assert!(refs.has("main") && refs.has("v0.2.0") && !refs.has("v9.9.9"));
        assert_eq!(refs.newest_tags(3), vec!["v0.10.0", "v0.2.0", "v0.1.0"]);
    }
}
