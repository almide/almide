/// Dependency fetching: git clone, version resolution, recursive deps.
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::project::{Dependency, FetchedDep, Project, LockedDep, PkgId, cache_dir, parse_lock_file, parse_toml, write_lock_file};
use crate::err;

/// Get the current HEAD commit hash in a git repo
fn git_head_hash(repo_dir: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C").arg(repo_dir)
        .arg("rev-parse").arg("HEAD")
        .output()
        .map_err(|e| format!("Failed to run git rev-parse: {}", e))?;
    if !output.status.success() {
        return Err("Failed to get git HEAD hash".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Fetch a dependency (clone or use cached). Returns (path, commit_hash).
pub fn fetch_dep(dep: &Dependency) -> Result<PathBuf, String> {
    fetch_dep_with_lock(dep, None)
}

/// The ref a manifest entry names: its tag, else its branch, else `main`.
///
/// One definition on purpose. The cache path, the lock LOOKUP and the lock
/// WRITE must agree on which ref a dependency asked for; while this
/// expression was spelled out in three places they could disagree, and a
/// disagreement is not a slow build — it is a lock entry describing a fetch
/// that did not happen (#2522).
fn dep_ref_name(dep: &Dependency) -> &str {
    dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main")
}

/// One component of a cache path for one piece of a source URL, encoded so
/// that the encoding is injective and every result is a plain directory
/// name: `a-z`, `0-9`, `-`, `_` and a non-leading `.` stay as written; every
/// other byte (an uppercase letter included, so a case-insensitive
/// filesystem cannot fold two sources together) becomes `%XX`. An empty
/// piece is `%`, which no other piece encodes to. A piece too long for one
/// directory name is split over several, each but the last marked `%+`
/// (a spelling no encoded piece can start with).
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

/// Where the checkouts of one SOURCE live (#4093):
/// `~/.almide/cache/.sources/<scheme>/<host>/<owner>/<repo>/@/`, with the
/// per-ref and per-commit directories underneath. Every piece of the URL is
/// one or more directory levels ([`encode_cache_component`]), and `@`
/// (which no encoded piece can be) closes the URL, so the path is an
/// injective function of the URL as written: two sources never share a
/// directory, whatever their package names.
///
/// The key is the source alone, never the package name: a package named
/// `svg` fetched from `github.com/evil/svg` cannot answer for
/// `github.com/almide/svg`. The layout before this one hashed the URL into
/// 64 bits (FNV-1a, #2523), which separated honest sources but is not
/// collision resistant: a URL chosen to collide with a trusted one would have
/// shared its directory. Directories of the older layouts
/// (`<name>/<ref>`, `<name>/.src-<hash>/`, `.repos/.src-<hash>/`) are never
/// read again; they are inert until `almide clean`.
///
/// A `subdir` dependency (#3381) is one package of a repository that may
/// hold several, so it shares its repository's checkout and reads its own
/// directory inside it, as before.
pub fn dep_cache_root(dep: &Dependency) -> PathBuf {
    let mut dir = cache_dir().join(".sources");
    let (scheme, rest) = dep.git.split_once("://").unwrap_or(("", dep.git.as_str()));
    encode_cache_component(scheme, &mut dir);
    for piece in rest.split('/') {
        encode_cache_component(piece, &mut dir);
    }
    dir.join("@")
}

/// The package directory a dependency names inside its checkout: the
/// checkout itself, or its `subdir` (#3381) — refused unless it is a
/// directory inside the checkout (no symlink out of it) that holds an
/// `almide.toml` whose `[package] name` is the dependency's key. Without
/// `subdir`, a checkout whose root is no package but whose subdirectories
/// are says which `subdir` to write.
fn package_dir_in(dep: &Dependency, checkout: &Path) -> Result<PathBuf, String> {
    let at = dep_ref_name(dep);
    let Some(subdir) = dep.subdir.as_deref() else {
        if !checkout.join("almide.toml").exists()
            && !checkout.join("src").is_dir()
            && !has_almd_file(checkout)
        {
            let found = packages_in(checkout);
            if !found.is_empty() {
                return Err(format!(
                    "{}the root of {} at {at} is not a package (no almide.toml) — dependency `{}` needs a `subdir`\n  \
                     hint: this repository holds the packages {}; add `subdir = \"…\"` naming one, e.g. `subdir = \"{}\"`",
                    dep.location_prefix(), dep.git, dep.name,
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
            dep.location_prefix(), dep.name, dep.git, found_hint()
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
            dep.location_prefix(), dep.name
        ));
    }
    let manifest = dir.join("almide.toml");
    if !manifest.exists() {
        return Err(format!(
            "{}subdir `{subdir}` of dependency `{}` has no almide.toml in {} at {at} — `subdir` must name a package directory\n{}",
            dep.location_prefix(), dep.name, dep.git, found_hint()
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
                dep.location_prefix(), dep.name, dep.name
            ))
        }
    }
}

/// Whether `dir` directly holds a `.almd` file.
fn has_almd_file(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "almd"))
    })
}

/// The package directories of a checkout, relative to it: every directory
/// up to two levels down (`pkg`, `pkgs/pkg`) holding an `almide.toml`,
/// sorted. Dot directories (`.git`) are skipped. For hints only.
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
    /// Set while [`resolve_cached_deps`] runs: no clone and no lock write.
    static OFFLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn offline() -> bool {
    OFFLINE.with(|o| o.get())
}

/// Restores the previous offline state when dropped.
struct OfflineGuard(bool);

impl Drop for OfflineGuard {
    fn drop(&mut self) {
        OFFLINE.with(|o| o.set(self.0));
    }
}

/// The project's dependencies, resolved from what is already on disk: path
/// dependencies and checkouts already in the cache. It never runs `git` to
/// fetch, and never writes `almide.lock`. A dependency that is not cached
/// yet is an error naming the command that fetches it.
///
/// For tools that look at a project without being asked to build it — the
/// LSP opening a file, `almide ide` — so that an untrusted project cannot
/// choose which hosts this machine contacts, or write into its own tree,
/// before anything is run (#4128).
pub fn resolve_cached_deps(project: &Project) -> Result<Vec<FetchedDep>, String> {
    let _restore = OfflineGuard(OFFLINE.with(|o| o.replace(true)));
    fetch_all_deps(project)
}

/// Populate cache dir `dir` atomically: run `clone` against a fresh sibling
/// temp dir, then rename it into place. Callers may race — `almide test`
/// compiles test files on parallel threads and every thread resolves deps,
/// and two `almide` processes can share `~/.almide` — so the old
/// "`dir.exists()` then clone into `dir`" let several clones write into the
/// same directory (`fatal: cannot copy ... File exists`) and left a
/// half-populated dir that later runs took for a finished cache. With the
/// rename, `dir` exists only when it is complete; the first rename wins and
/// the losers discard their own copy.
fn populate_cache_dir(dir: &Path, clone: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    if dir.exists() {
        return Ok(());
    }
    if offline() {
        return Err(format!(
            "dependency not fetched yet ({}); run `almide build` in the project to fetch it",
            dir.display()
        ));
    }
    let parent = dir.parent().ok_or_else(|| "cache dir has no parent".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;
    let leaf = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!(".{}.tmp-{}-{}", leaf, std::process::id(), seq));
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = clone(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    match std::fs::rename(&tmp, dir) {
        Ok(()) => Ok(()),
        Err(_) if dir.exists() => {
            // Another clone finished first; ours is redundant.
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(format!("Failed to move fetched dependency into cache: {}", e))
        }
    }
}

/// `fetch_dep_with_lock`'s locked-commit path: clone into a commit-keyed
/// cache dir and checkout the exact commit.
fn fetch_dep_at_commit(dep: &Dependency, commit: &str) -> Result<PathBuf, String> {
    let dir = dep_cache_root(dep).join(&commit[..12.min(commit.len())]);
    if dir.exists() {
        return Ok(dir);
    }
    // The checkout this run (or an earlier one) made at the dependency's own
    // ref, when it sits at exactly the locked commit: the lock is written right
    // after that fetch, so a second resolution in the same command (check's
    // whole-program pass) would otherwise clone the same commit again.
    if let Some(ref_name) = dep.tag.as_deref().or(dep.branch.as_deref()) {
        let at_ref = dep_cache_root(dep).join(ref_name);
        if at_ref.exists() && git_head_hash(&at_ref).ok().as_deref() == Some(commit) {
            return Ok(at_ref);
        }
    }
    err(&format!("Fetching {} from {} (locked: {})", dep.name, dep.git, &commit[..8.min(commit.len())]));
    populate_cache_dir(&dir, |tmp| {
        let output = Command::new("git")
            .arg("clone").arg(&dep.git).arg(tmp)
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            return Err(format!("Failed to fetch {}: {}", dep.name, String::from_utf8_lossy(&output.stderr)));
        }
        let checkout = Command::new("git")
            .arg("-C").arg(tmp)
            .arg("checkout").arg(commit)
            .output()
            .map_err(|e| format!("Failed to checkout commit: {}", e))?;
        if !checkout.status.success() {
            return Err(format!("Failed to checkout {} at {}: {}", dep.name, commit, String::from_utf8_lossy(&checkout.stderr)));
        }
        Ok(())
    })?;
    Ok(dir)
}

/// `fetch_dep_with_lock`'s ref-based path (tag/branch, no lock pin): clone
/// into a ref-keyed cache dir.
fn fetch_dep_at_ref(dep: &Dependency, ref_name: &str) -> Result<PathBuf, String> {
    let dep_dir = dep_cache_root(dep).join(ref_name);

    if dep_dir.exists() {
        return Ok(dep_dir);
    }

    err(&format!("Fetching {} from {} ({})", dep.name, dep.git, ref_name));

    populate_cache_dir(&dep_dir, |tmp| {
        let mut cmd = Command::new("git");
        cmd.arg("clone")
            .arg("--depth").arg("1")
            .arg(&dep.git)
            .arg(tmp);

        if let Some(ref tag) = dep.tag {
            cmd.arg("--branch").arg(tag);
        } else if let Some(ref branch) = dep.branch {
            cmd.arg("--branch").arg(branch);
        }

        let output = cmd.output()
            .map_err(|e| format!("Failed to run git: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to fetch {}: {}", dep.name, stderr));
        }
        Ok(())
    })?;

    Ok(dep_dir)
}

/// Fetch a dependency, optionally pinned to a locked commit hash.
pub fn fetch_dep_with_lock(dep: &Dependency, locked_commit: Option<&str>) -> Result<PathBuf, String> {
    // Path dependency: use local directory directly, no fetch needed.
    if let Some(ref path) = dep.path {
        let p = PathBuf::from(path);
        if p.exists() { return Ok(p); }
        return Err(format!("path dependency '{}' not found: {}", dep.name, path));
    }

    // If locked to a specific commit, use commit-based cache dir
    let checkout = match locked_commit {
        Some(commit) => fetch_dep_at_commit(dep, commit)?,
        None => fetch_dep_at_ref(dep, dep_ref_name(dep))?,
    };
    package_dir_in(dep, &checkout)
}

/// One fetch the walk made: the request as the lock would name it, the
/// `PkgId` MVS files it under, and the checkout it produced.
///
/// `update_lock_file` needs all three. The request identifies WHICH direct
/// dependency this was (the lookup key the lock is matched on); the `PkgId`
/// and the checkout say which fetch MVS finally SELECTED for that package, and
/// the selected fetch is what the lock records (#2532).
#[derive(Debug, Clone)]
pub struct Resolution {
    pub lock: LockedDep,
    pub pkg_id: PkgId,
    pub source_dir: PathBuf,
}

/// Update almide.lock after fetching all dependencies.
///
/// The lock records the RESOLVED result, not the request: for each direct
/// dependency, the `(git, ref, commit)` of the fetch Minimal Version Selection
/// chose for its package — the checkout that was actually built (#2532). When
/// nothing raises the dependency that is its own request. When a transitive
/// requirement raises it (the manifest asks `foo@v1.0.0`, a dependency asks
/// `foo@v1.1.0`, v1.1.0 is built), the entry names v1.1.0 and v1.1.0's commit.
/// The request lives only in `almide.toml`. This is the Cargo.lock reading:
/// the lock answers "what was built", which is what readers ask it.
///
/// `resolved` is every fetch the walk made, in walk order; `fetched` is the
/// selection. The direct dependency's own fetch is found by its identity —
/// name, source and ref, the key the lock lookup also uses (#2522). It used
/// to be `deps.iter().zip(resolved.iter())`, but `deps` is the direct list
/// while the walk is the flattened graph, so the indexes only coincided when
/// nothing had transitive dependencies (#2529). The selected fetch is then the
/// one whose checkout MVS kept for that `PkgId` — matched on the checkout
/// itself, so every entry is one fetch's own triple, never a ref from one
/// fetch beside another's commit.
pub fn update_lock_file(
    project_root: &Path,
    deps: &[Dependency],
    resolved: &[Resolution],
    fetched: &[FetchedDep],
) -> Result<(), String> {
    let locked = resolved_lock_entries(deps, resolved, fetched);
    if !locked.is_empty() {
        write_lock_file(&project_root.join("almide.lock"), &locked)?;
    }
    Ok(())
}

/// The entries [`update_lock_file`] writes: one per direct git dependency,
/// the `(git, ref, commit)` MVS selected for it.
fn resolved_lock_entries(deps: &[Dependency], resolved: &[Resolution], fetched: &[FetchedDep]) -> Vec<LockedDep> {
    let mut locked = Vec::new();
    for dep in deps {
        let ref_name = dep_ref_name(dep);
        let Some(request) = resolved.iter().find(|r| {
            r.lock.name == dep.name
                && r.lock.git == dep.git
                && r.lock.ref_name == ref_name
                && r.lock.subdir == dep.subdir
        }) else {
            // Nothing was resolved for it (a path dependency, or a checkout
            // with no git metadata) — a lock entry invented for it here
            // would be a false record.
            continue;
        };
        let selected = fetched
            .iter()
            .find(|f| f.pkg_id == request.pkg_id)
            .and_then(|f| resolved.iter().find(|r| r.pkg_id == f.pkg_id && r.source_dir == f.source_dir));
        // `None` means MVS selected a checkout with no git identity (a path
        // dependency raised this one): what was built is not a commit, so
        // there is nothing true to pin.
        if let Some(selected) = selected {
            locked.push(LockedDep {
                name: dep.name.clone(),
                git: selected.lock.git.clone(),
                ref_name: selected.lock.ref_name.clone(),
                commit: selected.lock.commit.clone(),
                subdir: selected.lock.subdir.clone(),
            });
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
/// If almide.lock exists, uses locked commit hashes for reproducibility.
pub fn fetch_all_deps(project: &Project) -> Result<Vec<FetchedDep>, String> {
    let lock_path = project.root.join("almide.lock");
    // A corrupt lock is an ERROR, never an empty set: swallowing it here used
    // to silently re-resolve every dependency from the network, defeating the
    // reproducibility the lock exists for (#1465).
    let locked = if lock_path.exists() {
        parse_lock_file(&lock_path)?
    } else {
        Vec::new()
    };

    let locked_mode = locked_mode();
    if locked_mode {
        refuse_unlocked_direct_deps(project, &locked)?;
    }

    let mut walk = Walk {
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
/// lock entry for its name, source and ref.
fn refuse_unlocked_direct_deps(project: &Project, locked: &[LockedDep]) -> Result<(), String> {
    let missing: Vec<String> = project
        .dependencies
        .iter()
        .filter(|d| d.path.is_none())
        .filter(|d| !locked.iter().any(|l| l.name == d.name && l.git == d.git && l.ref_name == dep_ref_name(d)))
        .map(|d| format!("`{}` ({} {})", d.name, d.git, dep_ref_name(d)))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "error: --locked: not in almide.lock: {}\n  \
         note: --locked fetches nothing the lock does not name, and writes no lock\n  \
         hint: run the command once without --locked to lock it, review the almide.lock diff, and commit it",
        missing.join(", ")
    ))
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
    for dep in project.dependencies.iter().filter(|d| d.path.is_none()) {
        let Some(entry) = locked.iter().find(|l| l.name == dep.name) else { continue };
        if entry.git == dep.git || resolved.iter().any(|r| r.lock.git == entry.git) {
            continue;
        }
        return Err(format!(
            "error: `{name}` is locked to {old} but almide.toml now says {new}\n  \
             hint: if the change is intended, run `almide update {name}` to re-lock it from the new source",
            name = dep.name,
            old = entry.git,
            new = dep.git,
        ));
    }
    Ok(())
}


/// After fetching a dependency at `path`, read its own `almide.toml` (if
/// any) for its declared package name, src dir, and transitive deps —
/// falling back to the requesting `Dependency`'s own name when there's no
/// manifest or it fails to parse. Extracted verbatim from
/// `fetch_deps_recursive`'s per-dep body (the 3 branches computed the same
/// `src_dir` expression independently; hoisted out here, same result).
fn resolve_fetched_dep_manifest(path: &Path, fallback_name: &str) -> (String, PathBuf, Vec<Dependency>) {
    let src_dir = if path.join("src").is_dir() { path.join("src") } else { path.to_path_buf() };
    let dep_toml = path.join("almide.toml");
    if !dep_toml.exists() {
        return (fallback_name.to_string(), src_dir, vec![]);
    }
    match parse_toml(&dep_toml) {
        Ok(dep_project) => (dep_project.package.name, src_dir, dep_project.dependencies),
        Err(_) => (fallback_name.to_string(), src_dir, vec![]),
    }
}

/// MVS ordering: parse as semver, treating an unparseable version as the
/// minimum (branch deps already resolve to "0.0.0" upstream).
fn semver_of(v: &str) -> semver::Version {
    semver::Version::parse(v).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
}

/// `fetch_deps_recursive`'s per-dependency body: version/pkg-id resolution,
/// diamond-dependency major-version-clash warning, dedup via `visited`,
/// fetch (with lock pin if any), resolve the dependency's own manifest,
/// record it, then recurse into its own transitive deps.
///
/// Selection is Minimal Version Selection, the rule
/// `docs/specs/package-system.md` documents: per `PkgId` (name + major) the
/// MAXIMUM requested version wins, and the walk visits every distinct
/// requested version's own requirement list (reachability is over requested
/// versions, not selected ones — Go's algorithm). The previous body
/// short-circuited on the first `PkgId` sighting, so the resolved graph
/// depended on declaration order (#1458).
/// The dependency walk's state: the lock, the root's `[trust] owners`, and
/// what the walk has fetched so far.
struct Walk<'a> {
    locked: &'a [LockedDep],
    /// The ROOT manifest's `[trust] owners` (#4093). A dependency's own
    /// `[trust]` is never consulted.
    trusted: &'a [String],
    fetched: Vec<FetchedDep>,
    visited: std::collections::HashSet<String>,
    /// What each dependency RESOLVED to, recorded as the walk fetches it.
    /// `fetched` cannot answer this: it is the flattened graph, its entries
    /// carry the package's own declared name rather than the manifest key,
    /// and nothing in them names a source or a ref (#2529).
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
            // grant).
            if dep.path.is_none() {
                match crate::trust::source_owner(&dep.git) {
                    Err(e) => return Err(format!("error: dependency `{}`: {e}", dep.name)),
                    Ok(Some(owner)) if !crate::trust::is_trusted(self.trusted, &owner) => {
                        return Err(crate::trust::fetch_refusal(&chain, &dep.git, &owner));
                    }
                    Ok(_) => {}
                }
            }
            fetch_one_dep_recursive(dep, self, &chain)?;
        }
        Ok(())
    }
}

fn fetch_one_dep_recursive(dep: &Dependency, walk: &mut Walk, chain: &[String]) -> Result<(), String> {
    let locked = walk.locked;
    let (fetched, visited, resolved) = (&mut walk.fetched, &mut walk.visited, &mut walk.resolved);
    let version_str = resolve_dep_version(dep);
    let pkg_id = PkgId::from_version_str(&dep.name, &version_str);

    // A package is a source AND a directory in it (#3381): two subdirs of
    // one repository are two packages, each walked for its own requirements.
    let visit_key = if let Some(ref p) = dep.path {
        format!("path:{}@{}", p, version_str)
    } else {
        format!("{}#{}@{}", dep.git, dep.subdir.as_deref().unwrap_or(""), version_str)
    };
    if visited.contains(&visit_key) {
        return Ok(());
    }
    visited.insert(visit_key);

    // Detect different major versions of the same package (diamond with version split)
    if let Some(existing) = fetched.iter().find(|f| f.pkg_id.name == pkg_id.name && f.pkg_id.major != pkg_id.major) {
        err(&format!("warning: package '{}' required at two different major versions", pkg_id.name));
        err(&format!("  → {} (already loaded)", existing.pkg_id));
        err(&format!("  → {} (newly required)", pkg_id));
        err(&format!("  Both versions will coexist. Types from v{} and v{} are incompatible.", existing.pkg_id.major, pkg_id.major));
    }

    // Use the locked commit only when the lock entry describes the SAME
    // source at the SAME ref the manifest asks for. Matching on the name
    // alone reused the old commit after a tag bump, and `update_lock_file`
    // then stamped the manifest's NEW ref beside it: an entry asserting a
    // (git, ref, commit) triple that was never true together, while the build
    // quietly compiled the old tag (#2522). A manifest that no longer matches
    // its pin is a request to resolve the ref again — and the lock is
    // rewritten from what was actually fetched, so it stays a true record.
    //
    // An entry whose `ref` is absent (only a hand-written lock: the writer
    // has always emitted one) is a mismatch too. Reusing its commit would
    // relabel it with the manifest's ref on the way out — the same false
    // record by another route.
    //
    // The match is on the SOURCE (git, ref), not on the manifest key. The
    // lock records what MVS selected (#2532), so an entry raised by a
    // transitive requirement names the ref that requirement asked for, and
    // that request is spelled in ANOTHER package's manifest, under whatever
    // key that manifest chose. The pin is a fact about a repository at a ref;
    // who asked for it does not change which commit it is. The direct
    // request below the raise (`foo@v1.0.0` when `v1.1.0` is locked) simply
    // finds no pin and resolves its ref, exactly as a transitive dependency
    // always has — it is walked for its requirements, not built.
    let want_ref = dep_ref_name(dep);
    let locked_commit = locked.iter()
        .find(|l| l.git == dep.git && l.ref_name == want_ref)
        .map(|l| l.commit.as_str());
    let path = fetch_dep_with_lock(dep, locked_commit)?;

    // Record what THIS dependency resolved to, here, where the manifest entry
    // and the checkout it produced are both in hand. Pairing them later — from
    // the flattened graph, by position or by a reconstructed key — is what
    // #2529 was. A dependency with no git metadata (a path dependency) records
    // nothing and is simply absent from the lock, as it was before.
    let git_commit = git_head_hash(&path).ok();

    let (module_name, source_dir, transitive_deps) = resolve_fetched_dep_manifest(&path, &dep.name);

    let actual_pkg_id = PkgId::from_version_str(&module_name, &version_str);
    if let Some(commit) = git_commit {
        resolved.push(Resolution {
            lock: LockedDep {
                name: dep.name.clone(),
                git: dep.git.clone(),
                ref_name: want_ref.to_string(),
                commit,
                subdir: dep.subdir.clone(),
            },
            pkg_id: actual_pkg_id.clone(),
            source_dir: source_dir.clone(),
        });
    }
    match fetched.iter_mut().find(|f| f.pkg_id == actual_pkg_id) {
        Some(existing) => {
            // MVS: a later, higher request replaces the selection in place —
            // position (and therefore lock-file alignment) is preserved.
            if semver_of(&version_str) > semver_of(&existing.version) {
                existing.version = version_str;
                existing.source_dir = source_dir;
                existing.package_dir = path;
            }
        }
        None => fetched.push(FetchedDep {
            pkg_id: actual_pkg_id,
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
    let (path, tag) = if let Some(pos) = spec.rfind('@') {
        (&spec[..pos], Some(spec[pos + 1..].to_string()))
    } else {
        (spec, None)
    };

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
        2 => {
            (format!("https://github.com/{}/{}", parts[0], parts[1]), parts[1].to_string())
        }
        _ if parts[0].contains('.') => {
            let name = parts.last().expect("split always yields ≥1 element").to_string();
            (format!("https://{}", path), name)
        }
        _ => {
            (format!("https://github.com/{}", path), parts.last().expect("split always yields ≥1 element").to_string())
        }
    };

    (name, git_url, tag)
}

/// Resolve the `almide add` arguments to `(name, git_url, tag)`. `--tag`
/// applies to both spellings; a `pkg@tag` suffix in the spec is the fallback
/// when `--tag` is absent. (Previously the spec's tag silently replaced
/// `--tag`, so `almide add almide/svg --tag v0.1.0` pinned to `main`.)
///
/// With `--subdir` (#3381) the spec names the REPOSITORY, so the package
/// name defaults to the subdir's last component (`--subdir pkgs/ceangal` →
/// `ceangal`) rather than the repository's name. With `--git` the first
/// argument is the name, as before.
pub fn resolve_add_target(
    pkg: String,
    git: Option<String>,
    tag: Option<String>,
    subdir: Option<&str>,
) -> (String, String, Option<String>) {
    if let Some(git_url) = git {
        return (pkg, git_url, tag);
    }
    let (name, git_url, spec_tag) = resolve_package_spec(&pkg);
    let name = subdir
        .and_then(|s| s.rsplit('/').next())
        .map(str::to_string)
        .unwrap_or(name);
    (name, git_url, tag.or(spec_tag))
}

/// Add a dependency to almide.toml. `subdir` must already be normalized
/// (`crate::project::normalize_subdir`).
pub fn add_dep_to_toml(name: &str, git: &str, tag: Option<&str>, subdir: Option<&str>) -> Result<(), String> {
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

    let mut content = std::fs::read_to_string(toml_path)
        .map_err(|e| format!("Failed to read almide.toml: {}", e))?;

    let tag_part = tag.map(|t| format!(", tag = \"{t}\"")).unwrap_or_default();
    let subdir_part = subdir.map(|s| format!(", subdir = \"{s}\"")).unwrap_or_default();
    let dep_line = format!("{} = {{ git = \"{}\"{}{} }}", name, git, tag_part, subdir_part);

    if content.contains("[dependencies]") {
        content = content.replacen("[dependencies]", &format!("[dependencies]\n{}", dep_line), 1);
    } else {
        content.push_str(&format!("\n[dependencies]\n{}\n", dep_line));
    }

    std::fs::write(toml_path, content)
        .map_err(|e| format!("Failed to write almide.toml: {}", e))?;

    Ok(())
}

/// `almide update [dep]` (#1131): advance a LOCKED git dependency to its
/// ref's current remote head.
///
/// The lock is intentionally sticky — `fetch_all_deps` reuses its commit so
/// builds reproduce, and `almide add` on an existing dep re-writes the same
/// pin. That left no sanctioned way FORWARD: `almide clean` clears the cache
/// (not the lock), and the lock's own header says not to edit it. This is
/// that path — it rewrites only the named entries, leaving every other pin
/// byte-identical.
pub fn update_locked_deps(project: &Project, only: Option<&str>) -> Result<Vec<(String, String, String)>, String> {
    let lock_path = project.root.join("almide.lock");
    let mut locked = if lock_path.exists() {
        parse_lock_file(&lock_path)?
    } else {
        Vec::new()
    };
    let targets: Vec<&Dependency> = match only {
        Some(name) => {
            let found: Vec<&Dependency> =
                project.dependencies.iter().filter(|d| d.name == name).collect();
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
        if dep.path.is_some() {
            continue;
        }
        // #4093: `update` reaches the network too, so it checks the owner
        // first, like every fetch.
        match crate::trust::source_owner(&dep.git) {
            Err(e) => return Err(format!("error: dependency `{}`: {e}", dep.name)),
            Ok(Some(owner)) if !crate::trust::is_trusted(&project.trust_owners, &owner) => {
                return Err(crate::trust::fetch_refusal(std::slice::from_ref(&dep.name), &dep.git, &owner));
            }
            Ok(_) => {}
        }
        // The manifest's source changed under the lock: drop the old pin and
        // re-lock from the new source below, tag or not (the tag rule keeps a
        // pin from MOVING, and a pin to another repository is no pin of this
        // one).
        if let Some(pos) = locked.iter().position(|l| l.name == dep.name && l.git != dep.git) {
            let old = locked.remove(pos);
            relock.push((dep.name.clone(), old.commit));
            continue;
        }
        // A tag pins by definition — advancing it would silently change what
        // the manifest asked for. Only floating refs (branches, the default
        // `main`) move.
        if dep.tag.is_some() {
            err(&format!("{} is pinned to tag {} — not updated", dep.name, ref_name));
            continue;
        }
        let head = git_remote_head(&dep.git, ref_name)?;
        let before = locked.iter().find(|l| l.name == dep.name).map(|l| l.commit.clone());
        if before.as_deref() == Some(head.as_str()) {
            err(&format!("{} already at {} ({})", dep.name, &head[..head.len().min(12)], ref_name));
            continue;
        }
        // Drop the stale cache dir so the next fetch re-clones at the new
        // head — under THIS dependency's source key, so advancing one project's
        // branch cannot evict a same-named package fetched from elsewhere.
        let cached = dep_cache_root(dep).join(ref_name);
        let _ = std::fs::remove_dir_all(&cached);
        match locked.iter_mut().find(|l| l.name == dep.name) {
            Some(entry) => {
                entry.git = dep.git.clone();
                entry.ref_name = ref_name.to_string();
                entry.commit = head.clone();
                entry.subdir = dep.subdir.clone();
            }
            None => locked.push(LockedDep {
                name: dep.name.clone(),
                git: dep.git.clone(),
                ref_name: ref_name.to_string(),
                commit: head.clone(),
                subdir: dep.subdir.clone(),
            }),
        }
        changed.push((dep.name.clone(), before.unwrap_or_default(), head));
    }
    if !changed.is_empty() || !relock.is_empty() {
        write_lock_file(&lock_path, &locked)?;
    }
    if !relock.is_empty() {
        // Resolve the new sources and write their pins, as a build would.
        fetch_all_deps(project)?;
        let fresh = parse_lock_file(&lock_path)?;
        for (name, before) in relock {
            let after = fresh.iter().find(|l| l.name == name).map(|l| l.commit.clone()).unwrap_or_default();
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

/// Does the repository at `url` exist? One `git ls-remote`, no clone, with
/// git's terminal prompts off: GitHub answers a missing repository with an
/// authentication request, which an interactive git would turn into a
/// password prompt (#4093).
pub fn probe_source(url: &str) -> Result<(), ProbeFailure> {
    let out = Command::new("git")
        .args(["ls-remote", "--heads", "--tags", url])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| ProbeFailure::Unreachable(format!("failed to run git: {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let lower = stderr.to_ascii_lowercase();
    let not_found = [
        "repository not found",
        "not found",
        "could not read username",
        "does not appear to be a git repository",
        "does not exist",
        "no such file or directory",
    ];
    if not_found.iter().any(|p| lower.contains(p)) {
        Err(ProbeFailure::NotFound(stderr))
    } else {
        Err(ProbeFailure::Unreachable(stderr))
    }
}

/// The remote's current commit for `ref_name` — read WITHOUT cloning
/// (`git ls-remote`), so `update` costs one network round-trip per dep.
pub fn git_remote_head(git_url: &str, ref_name: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["ls-remote", git_url, ref_name])
        .output()
        .map_err(|e| format!("git ls-remote failed: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-remote {} {} failed: {}",
            git_url, ref_name, String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let hash = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or_default()
        .to_string();
    if hash.is_empty() {
        return Err(format!("no ref '{}' at {}", ref_name, git_url));
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, git: &str, tag: Option<&str>) -> Dependency {
        Dependency {
            name: name.into(),
            git: git.into(),
            tag: tag.map(str::to_string),
            branch: None,
            version: None,
            path: None,
            subdir: None,
            declared_at: None,
        }
    }

    /// The cache path is the source URL itself, piece by piece (#4093), so
    /// it is part of the on-disk format: pinned literally.
    #[test]
    fn the_cache_root_spells_the_source() {
        let root = dep_cache_root(&dep("svg", "https://github.com/almide/svg", None));
        assert_eq!(root, cache_dir().join(".sources/https/github.com/almide/svg/@"));
        let upper = dep_cache_root(&dep("svg", "https://github.com/Almide/svg.git", None));
        assert_eq!(upper, cache_dir().join(".sources/https/github.com/%41lmide/svg.git/@"));
    }

    /// #2523, #4093: a package NAME cannot put two repositories in one
    /// directory, and no two spellings of a URL alias.
    #[test]
    fn two_sources_sharing_a_name_get_separate_cache_roots() {
        let root = |git: &str| dep_cache_root(&dep("fizz", git, Some("v0.2.2")));
        let urls = [
            "https://github.com/one/fizz",
            "https://github.com/two/fizz",
            "https://github.com/one/fizz.git",
            "https://github.com/One/fizz",
            "https://github.com/one/fizz/@",
            "https://github.com/one/fizz/",
            "https://github.com/one/.fizz",
            "https://github.com/one/%2Efizz",
            "file:///tmp/one/fizz",
            "/tmp/one/fizz",
        ];
        for (i, a) in urls.iter().enumerate() {
            for b in &urls[i + 1..] {
                assert_ne!(root(a), root(b), "{a} and {b} share a cache root");
            }
        }
        // The package name is not part of it.
        assert_eq!(dep_cache_root(&dep("a", urls[0], None)), dep_cache_root(&dep("b", urls[0], None)));
    }

    /// A long piece is split over several directory levels and stays
    /// distinct from the same text with a `/` in it.
    #[test]
    fn a_long_url_piece_splits_without_aliasing() {
        let long = "x".repeat(400);
        let a = dep_cache_root(&dep("x", &format!("https://h.example/o/{long}"), None));
        let b = dep_cache_root(&dep("x", &format!("https://h.example/o/{}/{}", &long[..180], &long[180..]), None));
        assert_ne!(a, b);
        assert!(a.components().all(|c| c.as_os_str().len() <= 200));
    }

    /// #3381: two packages of one repository share ONE clone root (one fetch
    /// per ref), whatever their names.
    #[test]
    fn subdir_packages_of_one_repository_share_a_clone_root() {
        let with = |name: &str, sub: &str| Dependency {
            subdir: Some(sub.into()),
            ..dep(name, "https://github.com/almide-graphics/ceangal2", Some("v0.1.0"))
        };
        let a = dep_cache_root(&with("ceangal", "ceangal"));
        let b = dep_cache_root(&with("snaidhm", "snaidhm"));
        assert_eq!(a, b, "two subdirs of one repository must share the clone");
        let plain = dep_cache_root(&dep("ceangal", "https://github.com/almide-graphics/ceangal2", Some("v0.1.0")));
        assert_eq!(a, plain, "the source alone keys the checkout (#4093)");
        // Another repository still gets its own root.
        let other = dep_cache_root(&Dependency {
            subdir: Some("ceangal".into()),
            ..dep("ceangal", "https://github.com/someone/else", Some("v0.1.0"))
        });
        assert_ne!(a, other);
        // `@` closes the URL: no encoded URL piece can spell it.
        assert_eq!(a.file_name().unwrap(), "@");
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
        let (name, _, _) = resolve_add_target("cg".into(), Some("file:///r".into()), None, Some("ceangal"));
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
        // A git source no cache entry exists for: a never-created local path.
        let git = format!("file://{}", root.join("no-such-repo").display());
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
            ..dep(name, "file:///repo", Some("v0.1.0"))
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
            .filter(|n| n != "abcdef123456")
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
        assert!(std::fs::read_dir(dir.parent().unwrap()).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
