/// Project configuration (almide.toml) and dependency management.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::process::Command;

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

    /// The import name users write: "json"
    pub fn import_name(&self) -> &str {
        &self.name
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
    pub version: String,
    pub source_dir: PathBuf,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub name: String,
    pub version: String,
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
}

/// Parse almide.toml (simple line-based, no toml crate)
pub fn parse_toml(path: &Path) -> Result<Project, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let mut name = String::new();
    let mut version = "0.1.0".to_string();
    let mut deps: Vec<Dependency> = Vec::new();
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
                        _ => {}
                    }
                }
            }
            "dependencies" => {
                if let Some(dep) = parse_dep_line(line) {
                    deps.push(dep);
                }
            }
            _ => {}
        }
    }

    Ok(Project {
        package: Package { name, version },
        dependencies: deps,
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

/// Fetch a dependency (clone or use cached)
pub fn fetch_dep(dep: &Dependency) -> Result<PathBuf, String> {
    let cache = cache_dir();
    let ref_name = dep.tag.as_deref()
        .or(dep.branch.as_deref())
        .unwrap_or("main");
    let dep_dir = cache.join(&dep.name).join(ref_name);

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
pub fn fetch_all_deps(project: &Project) -> Result<Vec<FetchedDep>, String> {
    let mut fetched: Vec<FetchedDep> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    fetch_deps_recursive(&project.dependencies, &mut fetched, &mut visited)?;
    Ok(fetched)
}

/// Legacy API: returns HashMap<String, PathBuf> for backward compatibility.
pub fn fetch_all_deps_flat(project: &Project) -> Result<HashMap<String, PathBuf>, String> {
    let fetched = fetch_all_deps(project)?;
    let mut paths = HashMap::new();
    for dep in fetched {
        paths.insert(dep.pkg_id.name.clone(), dep.source_dir);
    }
    Ok(paths)
}

fn fetch_deps_recursive(
    deps: &[Dependency],
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

        let path = fetch_dep(dep)?;

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
            version: version_str,
            source_dir,
        });

        if !transitive_deps.is_empty() {
            fetch_deps_recursive(&transitive_deps, fetched, visited)?;
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
