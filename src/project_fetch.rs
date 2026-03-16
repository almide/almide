/// Dependency fetching: git clone, version resolution, recursive deps.
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::project::{Dependency, FetchedDep, Project, LockedDep, PkgId, cache_dir, parse_lock_file, parse_toml, write_lock_file};

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

/// Fetch a dependency, optionally pinned to a locked commit hash.
pub fn fetch_dep_with_lock(dep: &Dependency, locked_commit: Option<&str>) -> Result<PathBuf, String> {
    let cache = cache_dir();
    let ref_name = dep.tag.as_deref()
        .or(dep.branch.as_deref())
        .unwrap_or("main");

    // If locked to a specific commit, use commit-based cache dir
    let dep_dir = if let Some(commit) = locked_commit {
        let dir = cache.join(&dep.name).join(&commit[..12.min(commit.len())]);
        if dir.exists() {
            return Ok(dir);
        }
        // Clone and checkout exact commit
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        eprintln!("Fetching {} from {} (locked: {})", dep.name, dep.git, &commit[..8.min(commit.len())]);
        let output = Command::new("git")
            .arg("clone").arg(&dep.git).arg(&dir)
            .output()
            .map_err(|e| format!("Failed to run git: {}", e))?;
        if !output.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("Failed to fetch {}: {}", dep.name, String::from_utf8_lossy(&output.stderr)));
        }
        let checkout = Command::new("git")
            .arg("-C").arg(&dir)
            .arg("checkout").arg(commit)
            .output()
            .map_err(|e| format!("Failed to checkout commit: {}", e))?;
        if !checkout.status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("Failed to checkout {} at {}: {}", dep.name, commit, String::from_utf8_lossy(&checkout.stderr)));
        }
        return Ok(dir);
    } else {
        cache.join(&dep.name).join(ref_name)
    };

    if dep_dir.exists() {
        return Ok(dep_dir);
    }

    std::fs::create_dir_all(&dep_dir)
        .map_err(|e| format!("Failed to create cache dir: {}", e))?;

    eprintln!("Fetching {} from {} ({})", dep.name, dep.git, ref_name);

    let mut cmd = Command::new("git");
    cmd.arg("clone")
        .arg("--depth").arg("1")
        .arg(&dep.git)
        .arg(&dep_dir);

    if let Some(ref tag) = dep.tag {
        cmd.arg("--branch").arg(tag);
    } else if let Some(ref branch) = dep.branch {
        cmd.arg("--branch").arg(branch);
    }

    let output = cmd.output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_dir_all(&dep_dir);
        return Err(format!("Failed to fetch {}: {}", dep.name, stderr));
    }

    Ok(dep_dir)
}

/// Update almide.lock after fetching all dependencies.
pub fn update_lock_file(deps: &[Dependency], fetched: &[FetchedDep]) -> Result<(), String> {
    let lock_path = Path::new("almide.lock");
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
    let lock_path = Path::new("almide.lock");
    let locked = if lock_path.exists() {
        parse_lock_file(lock_path).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut fetched: Vec<FetchedDep> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    fetch_deps_recursive(&project.dependencies, &locked, &mut fetched, &mut visited)?;

    // Update lock file if it doesn't exist or deps changed
    if !project.dependencies.is_empty() {
        let _ = update_lock_file(&project.dependencies, &fetched);
    }

    Ok(fetched)
}

fn fetch_deps_recursive(
    deps: &[Dependency],
    locked: &[LockedDep],
    fetched: &mut Vec<FetchedDep>,
    visited: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    for dep in deps {
        let version_str = resolve_dep_version(dep);
        let pkg_id = PkgId::from_version_str(&dep.name, &version_str);

        if fetched.iter().any(|f| f.pkg_id == pkg_id) {
            continue;
        }

        let visit_key = format!("{}@{}", dep.git, version_str);
        if visited.contains(&visit_key) {
            continue;
        }
        visited.insert(visit_key);

        // Use locked commit if available
        let locked_commit = locked.iter()
            .find(|l| l.name == dep.name)
            .map(|l| l.commit.as_str());
        let path = fetch_dep_with_lock(dep, locked_commit)?;

        let dep_toml = path.join("almide.toml");
        let (module_name, source_dir, transitive_deps) = if dep_toml.exists() {
            if let Ok(dep_project) = parse_toml(&dep_toml) {
                let name = dep_project.package.name;
                let src_dir = if path.join("src").is_dir() { path.join("src") } else { path.clone() };
                (name, src_dir, dep_project.dependencies)
            } else {
                let src_dir = if path.join("src").is_dir() { path.join("src") } else { path.clone() };
                (dep.name.clone(), src_dir, vec![])
            }
        } else {
            let src_dir = if path.join("src").is_dir() { path.join("src") } else { path.clone() };
            (dep.name.clone(), src_dir, vec![])
        };

        let actual_pkg_id = PkgId::from_version_str(&module_name, &version_str);
        fetched.push(FetchedDep {
            pkg_id: actual_pkg_id,
            source_dir,
        });

        if !transitive_deps.is_empty() {
            fetch_deps_recursive(&transitive_deps, locked, fetched, visited)?;
        }
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

/// Add a dependency to almide.toml
pub fn add_dep_to_toml(name: &str, git: &str, tag: Option<&str>) -> Result<(), String> {
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

    eprintln!("Added {} to almide.toml", name);
    Ok(())
}
