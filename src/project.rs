/// Project configuration (almide.toml) and dependency management.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone)]
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

    // Parse inline table: { git = "...", tag = "..." }
    if !rest.starts_with('{') {
        return None;
    }
    let inner = rest.trim_start_matches('{').trim_end_matches('}').trim();
    let mut git = String::new();
    let mut tag: Option<String> = None;
    let mut branch: Option<String> = None;

    for item in inner.split(',') {
        if let Some((k, v)) = parse_kv(item) {
            match k {
                "git" => git = v,
                "tag" => tag = Some(v),
                "branch" => branch = Some(v),
                _ => {}
            }
        }
    }

    if git.is_empty() {
        return None;
    }

    Some(Dependency { name, git, tag, branch })
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
        // Clean up failed clone
        let _ = std::fs::remove_dir_all(&dep_dir);
        return Err(format!("Failed to fetch {}: {}", dep.name, stderr));
    }

    Ok(dep_dir)
}

/// Fetch all dependencies and return their source paths.
/// Uses the dependency's almide.toml `name` as the module name (not the repo name).
pub fn fetch_all_deps(project: &Project) -> Result<HashMap<String, PathBuf>, String> {
    let mut paths = HashMap::new();
    for dep in &project.dependencies {
        let path = fetch_dep(dep)?;

        // Read the dependency's almide.toml to get its package name
        let module_name = {
            let dep_toml = path.join("almide.toml");
            if dep_toml.exists() {
                parse_toml(&dep_toml)
                    .map(|p| p.package.name)
                    .unwrap_or_else(|_| dep.name.clone())
            } else {
                dep.name.clone()
            }
        };

        // Look for src/ directory first, then root
        let src_dir = path.join("src");
        if src_dir.is_dir() {
            paths.insert(module_name, src_dir);
        } else {
            paths.insert(module_name, path);
        }
    }
    Ok(paths)
}

/// Resolve a short package specifier to a full git URL and optional tag.
///
/// Rules:
///   "github.com/org/repo"       → https://github.com/org/repo
///   "gitlab.com/org/repo"       → https://gitlab.com/org/repo
///   "org/repo"                  → https://github.com/org/repo
///   "name"                      → https://github.com/almide/name
///
/// @version suffix is split into tag:
///   "fizzbuzz@v0.1.0"           → (url, Some("v0.1.0"))
pub fn resolve_package_spec(spec: &str) -> (String, String, Option<String>) {
    // Split @version
    let (path, tag) = if let Some(pos) = spec.rfind('@') {
        (&spec[..pos], Some(spec[pos + 1..].to_string()))
    } else {
        (spec, None)
    };

    let parts: Vec<&str> = path.split('/').collect();
    let (git_url, name) = match parts.len() {
        1 => {
            // "fizzbuzz" → github.com/almide/fizzbuzz
            (format!("https://github.com/almide/{}", parts[0]), parts[0].to_string())
        }
        2 => {
            // "org/repo" → github.com/org/repo
            (format!("https://github.com/{}/{}", parts[0], parts[1]), parts[1].to_string())
        }
        _ if parts[0].contains('.') => {
            // "github.com/org/repo" or "gitlab.com/org/repo"
            let name = parts.last().unwrap().to_string();
            (format!("https://{}", path), name)
        }
        _ => {
            (format!("https://github.com/{}", path), parts.last().unwrap().to_string())
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
        // Add after [dependencies] section
        content = content.replacen("[dependencies]", &format!("[dependencies]\n{}", dep_line), 1);
    } else {
        // Add new section
        content.push_str(&format!("\n[dependencies]\n{}\n", dep_line));
    }

    std::fs::write(toml_path, content)
        .map_err(|e| format!("Failed to write almide.toml: {}", e))?;

    eprintln!("Added {} to almide.toml", name);
    Ok(())
}
