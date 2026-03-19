/// Project configuration (almide.toml) and dependency management.

use std::path::{Path, PathBuf};

/// Package identity for diamond dependency resolution.
/// Two packages with the same (name, major) are considered the same package
/// and will be unified to a single version. Different majors coexist.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PkgId {
    pub name: String,
    pub major: u64,
}

impl PkgId {
    pub fn from_version(name: &str, version: &semver::Version) -> Self {
        PkgId {
            name: name.to_string(),
            major: if version.major == 0 { version.minor } else { version.major },
        }
    }

    pub fn from_version_str(name: &str, ver_str: &str) -> Self {
        if let Ok(v) = semver::Version::parse(ver_str) {
            Self::from_version(name, &v)
        } else {
            PkgId { name: name.to_string(), major: 0 }
        }
    }

    /// Module name used in generated Rust code: "json_v2"
    pub fn mod_name(&self) -> String {
        format!("{}_v{}", self.name, self.major)
    }

}

impl std::fmt::Display for PkgId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} v{}.x", self.name, self.major)
    }
}

#[derive(Debug, Clone)]
pub struct FetchedDep {
    pub pkg_id: PkgId,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub edition: String,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub git: String,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub package: Package,
    pub dependencies: Vec<Dependency>,
    /// Allowed effect capabilities for this package (Security Layer 2).
    /// If empty, all capabilities are allowed (backwards compatible).
    /// e.g., ["IO", "Net", "Log"]
    pub permissions: Vec<String>,
}

/// Parse almide.toml (simple line-based, no toml crate)
pub fn parse_toml(path: &Path) -> Result<Project, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let mut name = String::new();
    let mut version = "0.1.0".to_string();
    let mut edition = "2026".to_string();
    let mut deps: Vec<Dependency> = Vec::new();
    let mut permissions: Vec<String> = Vec::new();
    let mut section = "";

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = if line == "[package]" {
                "package"
            } else if line == "[dependencies]" {
                "dependencies"
            } else if line == "[permissions]" {
                "permissions"
            } else {
                ""
            };
            continue;
        }
        match section {
            "package" => {
                if let Some((key, val)) = parse_kv(line) {
                    match key {
                        "name" => name = val,
                        "version" => version = val,
                        "edition" => edition = val,
                        _ => {}
                    }
                }
            }
            "dependencies" => {
                if let Some(dep) = parse_dep_line(line) {
                    deps.push(dep);
                }
            }
            "permissions" => {
                if let Some((key, val)) = parse_kv(line) {
                    if key == "allow" {
                        // Parse array: allow = ["IO", "Net"]
                        let val = val.trim_matches(|c| c == '[' || c == ']');
                        for item in val.split(',') {
                            let item = item.trim().trim_matches('"').trim_matches('\'');
                            if !item.is_empty() {
                                permissions.push(item.to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Project {
        package: Package { name, version, edition },
        dependencies: deps,
        permissions,
    })
}

fn parse_kv(line: &str) -> Option<(&str, String)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim();
    let val = parts.next()?.trim().trim_matches('"').to_string();
    Some((key, val))
}

/// Parse: name = { git = "url", tag = "v0.1.0" }
fn parse_dep_line(line: &str) -> Option<Dependency> {
    let mut parts = line.splitn(2, '=');
    let name = parts.next()?.trim().to_string();
    let rest = parts.next()?.trim();

    if !rest.starts_with('{') {
        return None;
    }
    let inner = rest.trim_start_matches('{').trim_end_matches('}').trim();
    let mut git = String::new();
    let mut tag: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut version: Option<String> = None;

    for item in inner.split(',') {
        if let Some((k, v)) = parse_kv(item) {
            match k {
                "git" => git = v,
                "tag" => tag = Some(v),
                "branch" => branch = Some(v),
                "version" => version = Some(v),
                _ => {}
            }
        }
    }

    if git.is_empty() {
        return None;
    }

    Some(Dependency { name, git, tag, branch, version })
}

/// Cache directory for dependencies
pub fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".almide").join("cache")
}

/// A locked dependency entry for almide.lock
#[derive(Debug, Clone)]
pub struct LockedDep {
    pub name: String,
    pub git: String,
    pub ref_name: String,
    pub commit: String,
}

/// Parse almide.lock (simple line-based: name = { git = "...", ref = "...", commit = "..." })
pub fn parse_lock_file(path: &Path) -> Result<Vec<LockedDep>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut locked = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if let Some(dep) = parse_lock_line(line) {
            locked.push(dep);
        }
    }
    Ok(locked)
}

fn parse_lock_line(line: &str) -> Option<LockedDep> {
    let mut parts = line.splitn(2, '=');
    let name = parts.next()?.trim().to_string();
    let rest = parts.next()?.trim();
    if !rest.starts_with('{') { return None; }
    let inner = rest.trim_start_matches('{').trim_end_matches('}').trim();
    let mut git = String::new();
    let mut ref_name = String::new();
    let mut commit = String::new();
    for item in inner.split(',') {
        if let Some((k, v)) = parse_kv(item) {
            match k {
                "git" => git = v,
                "ref" => ref_name = v,
                "commit" => commit = v,
                _ => {}
            }
        }
    }
    if git.is_empty() || commit.is_empty() { return None; }
    Some(LockedDep { name, git, ref_name, commit })
}

/// Write almide.lock
pub fn write_lock_file(path: &Path, locked: &[LockedDep]) -> Result<(), String> {
    let mut content = String::from("# almide.lock — auto-generated, do not edit\n\n");
    for dep in locked {
        content.push_str(&format!(
            "{} = {{ git = \"{}\", ref = \"{}\", commit = \"{}\" }}\n",
            dep.name, dep.git, dep.ref_name, dep.commit
        ));
    }
    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}
