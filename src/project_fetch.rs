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
    let cache = cache_dir();
    let dir = cache.join(&dep.name).join(&commit[..12.min(commit.len())]);
    if dir.exists() {
        return Ok(dir);
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
    let cache = cache_dir();
    let dep_dir = cache.join(&dep.name).join(ref_name);

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
    if let Some(commit) = locked_commit {
        return fetch_dep_at_commit(dep, commit);
    }

    let ref_name = dep.tag.as_deref()
        .or(dep.branch.as_deref())
        .unwrap_or("main");
    fetch_dep_at_ref(dep, ref_name)
}

/// Update almide.lock after fetching all dependencies.
pub fn update_lock_file(project_root: &Path, deps: &[Dependency], fetched: &[FetchedDep]) -> Result<(), String> {
    let lock_path = project_root.join("almide.lock");
    let lock_path = lock_path.as_path();
    let mut locked = Vec::new();
    for (dep, fd) in deps.iter().zip(fetched.iter()) {
        let ref_name = dep.tag.as_deref()
            .or(dep.branch.as_deref())
            .unwrap_or("main");
        let commit = git_head_hash(&fd.source_dir)
            .or_else(|_| git_head_hash(fd.source_dir.parent().unwrap_or(&fd.source_dir)))
            .unwrap_or_default();
        if !commit.is_empty() {
            locked.push(LockedDep {
                name: dep.name.clone(),
                git: dep.git.clone(),
                ref_name: ref_name.to_string(),
                commit,
            });
        }
    }
    if !locked.is_empty() {
        write_lock_file(lock_path, &locked)?;
    }
    Ok(())
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
    let locked = if lock_path.exists() {
        parse_lock_file(&lock_path).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut fetched: Vec<FetchedDep> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    fetch_deps_recursive(&project.dependencies, &locked, &mut fetched, &mut visited)?;

    // Update lock file if it doesn't exist or deps changed
    if !project.dependencies.is_empty() {
        let _ = update_lock_file(&project.root, &project.dependencies, &fetched);
    }

    Ok(fetched)
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

/// `fetch_deps_recursive`'s per-dependency body: version/pkg-id resolution,
/// diamond-dependency major-version-clash warning, dedup via `visited`,
/// fetch (with lock pin if any), resolve the dependency's own manifest,
/// record it, then recurse into its own transitive deps. Extracted
/// verbatim.
fn fetch_one_dep_recursive(
    dep: &Dependency,
    locked: &[LockedDep],
    fetched: &mut Vec<FetchedDep>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    let version_str = resolve_dep_version(dep);
    let pkg_id = PkgId::from_version_str(&dep.name, &version_str);

    if fetched.iter().any(|f| f.pkg_id == pkg_id) {
        return Ok(());
    }

    // Detect different major versions of the same package (diamond with version split)
    if let Some(existing) = fetched.iter().find(|f| f.pkg_id.name == pkg_id.name && f.pkg_id.major != pkg_id.major) {
        err(&format!("warning: package '{}' required at two different major versions", pkg_id.name));
        err(&format!("  → {} (already loaded)", existing.pkg_id));
        err(&format!("  → {} (newly required)", pkg_id));
        err(&format!("  Both versions will coexist. Types from v{} and v{} are incompatible.", existing.pkg_id.major, pkg_id.major));
    }

    let visit_key = if let Some(ref p) = dep.path {
        format!("path:{}@{}", p, version_str)
    } else {
        format!("{}@{}", dep.git, version_str)
    };
    if visited.contains(&visit_key) {
        return Ok(());
    }
    visited.insert(visit_key);

    // Use locked commit if available
    let locked_commit = locked.iter()
        .find(|l| l.name == dep.name)
        .map(|l| l.commit.as_str());
    let path = fetch_dep_with_lock(dep, locked_commit)?;

    let (module_name, source_dir, transitive_deps) = resolve_fetched_dep_manifest(&path, &dep.name);

    let actual_pkg_id = PkgId::from_version_str(&module_name, &version_str);
    fetched.push(FetchedDep {
        pkg_id: actual_pkg_id,
        source_dir,
    });

    if !transitive_deps.is_empty() {
        fetch_deps_recursive(&transitive_deps, locked, fetched, visited)?;
    }
    Ok(())
}

fn fetch_deps_recursive(
    deps: &[Dependency],
    locked: &[LockedDep],
    fetched: &mut Vec<FetchedDep>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    for dep in deps {
        fetch_one_dep_recursive(dep, locked, fetched, visited)?;
    }
    Ok(())
}

/// Resolve a short package specifier to a full git URL and optional tag.
pub fn resolve_package_spec(spec: &str) -> (String, String, Option<String>) {
    let (path, tag) = if let Some(pos) = spec.rfind('@') {
        (&spec[..pos], Some(spec[pos + 1..].to_string()))
    } else {
        (spec, None)
    };

    let parts: Vec<&str> = path.split('/').collect();
    let (git_url, name) = match parts.len() {
        1 => {
            (format!("https://github.com/almide/{}", parts[0]), parts[0].to_string())
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
pub fn resolve_add_target(pkg: String, git: Option<String>, tag: Option<String>) -> (String, String, Option<String>) {
    if let Some(git_url) = git {
        return (pkg, git_url, tag);
    }
    let (name, git_url, spec_tag) = resolve_package_spec(&pkg);
    (name, git_url, tag.or(spec_tag))
}

/// Add a dependency to almide.toml
pub fn add_dep_to_toml(name: &str, git: &str, tag: Option<&str>) -> Result<(), String> {
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

    let dep_line = if let Some(tag) = tag {
        format!("{} = {{ git = \"{}\", tag = \"{}\" }}", name, git, tag)
    } else {
        format!("{} = {{ git = \"{}\" }}", name, git)
    };

    if content.contains("[dependencies]") {
        content = content.replacen("[dependencies]", &format!("[dependencies]\n{}", dep_line), 1);
    } else {
        content.push_str(&format!("\n[dependencies]\n{}\n", dep_line));
    }

    std::fs::write(toml_path, content)
        .map_err(|e| format!("Failed to write almide.toml: {}", e))?;

    err(&format!("Added {} to almide.toml", name));
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
    for dep in targets {
        let ref_name = dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main");
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
        // Drop the stale cache dir so the next fetch re-clones at the new head.
        let cached = cache_dir().join(&dep.name).join(ref_name);
        let _ = std::fs::remove_dir_all(&cached);
        match locked.iter_mut().find(|l| l.name == dep.name) {
            Some(entry) => {
                entry.git = dep.git.clone();
                entry.ref_name = ref_name.to_string();
                entry.commit = head.clone();
            }
            None => locked.push(LockedDep {
                name: dep.name.clone(),
                git: dep.git.clone(),
                ref_name: ref_name.to_string(),
                commit: head.clone(),
            }),
        }
        changed.push((dep.name.clone(), before.unwrap_or_default(), head));
    }
    if !changed.is_empty() {
        write_lock_file(&lock_path, &locked)?;
    }
    Ok(changed)
}

/// The remote's current commit for `ref_name` — read WITHOUT cloning
/// (`git ls-remote`), so `update` costs one network round-trip per dep.
fn git_remote_head(git_url: &str, ref_name: &str) -> Result<String, String> {
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

    #[test]
    fn add_tag_flag_is_kept_for_short_specs() {
        let (name, git, tag) = resolve_add_target("almide/svg".into(), None, Some("v0.1.0".into()));
        assert_eq!(name, "svg");
        assert_eq!(git, "https://github.com/almide/svg");
        assert_eq!(tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn add_tag_flag_wins_over_spec_suffix_and_suffix_is_fallback() {
        let (_, _, tag) = resolve_add_target("svg@v0.2.0".into(), None, Some("v0.1.0".into()));
        assert_eq!(tag.as_deref(), Some("v0.1.0"));
        let (_, _, tag) = resolve_add_target("svg@v0.2.0".into(), None, None);
        assert_eq!(tag.as_deref(), Some("v0.2.0"));
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
