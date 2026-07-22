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
pub struct Package {
    pub name: String,
    pub version: String,
    /// Minimum compiler version required (Cargo `rust-version` style).
    /// `None` = no check (backward compatible).
    pub almide_min: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub git: String,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub package: Package,
    pub dependencies: Vec<Dependency>,
    /// Allowed effect capabilities for this package (Security Layer 2).
    /// If empty, all capabilities are allowed (backwards compatible).
    /// e.g., ["IO", "Net", "Log"]
    pub permissions: Vec<String>,
    /// Native Rust crate dependencies added to generated Cargo.toml.
    /// e.g., [("wasmtime", "42.0.0")]
    pub native_deps: Vec<NativeDep>,
    /// Directory containing this project's almide.toml. almide.lock lives here,
    /// never in the invoking process's cwd.
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct NativeDep {
    pub name: String,
    pub spec: String,
}

/// Parse almide.toml (simple line-based, no toml crate)
/// `parse_toml`'s running accumulator — one field group per TOML section, so
/// the per-section line handlers below can each take just the fields they
/// touch by `&mut` reference (write-only from each handler's own
/// perspective; no handler reads a field another handler writes).
#[derive(Default)]
struct TomlAccum {
    name: String,
    version: String,
    almide_min: Option<String>,
    deps: Vec<Dependency>,
    permissions: Vec<String>,
    native_deps: Vec<NativeDep>,
}

/// `parse_toml`'s `[package]` section line handler. Extracted verbatim.
fn apply_package_line(line: &str, acc: &mut TomlAccum) {
    if let Some((key, val)) = parse_kv(line) {
        match key {
            "name" => acc.name = val,
            "version" => acc.version = val,
            "almide" => acc.almide_min = Some(val),
            _ => {}
        }
    }
}

/// `parse_toml`'s `[permissions]` section line handler. Extracted verbatim.
fn apply_permissions_line(line: &str, acc: &mut TomlAccum) {
    if let Some(("allow", val)) = parse_kv(line) {
        acc.permissions.extend(
            val.trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|s| !s.is_empty())
        );
    }
}

/// `parse_toml`'s `[native-deps]` section line handler. Extracted verbatim.
fn apply_native_deps_line(line: &str, acc: &mut TomlAccum) {
    if let Some((dep_name, spec)) = parse_kv(line) {
        acc.native_deps.push(NativeDep {
            name: dep_name.to_string(),
            spec,
        });
    }
}

pub fn parse_toml(path: &Path) -> Result<Project, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let mut acc = TomlAccum { version: "0.1.0".to_string(), ..TomlAccum::default() };
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
            } else if line == "[native-deps]" {
                "native-deps"
            } else {
                ""
            };
            continue;
        }
        match section {
            "package" => apply_package_line(line, &mut acc),
            "dependencies" => {
                if let Some(dep) = parse_dep_line(line) {
                    acc.deps.push(dep);
                }
            }
            "permissions" => apply_permissions_line(line, &mut acc),
            "native-deps" => apply_native_deps_line(line, &mut acc),
            _ => {}
        }
    }

    // Validate package name: must be a valid Almide identifier (no hyphens).
    // Like Go, the package name IS the import name. No implicit conversion.
    if acc.name.contains('-') {
        return Err(format!(
            "package name '{}' contains hyphens — use underscores instead\n  \
             hint: rename to '{}' in [package] name. The package name is the import name.",
            acc.name,
            acc.name.replace('-', "_"),
        ));
    }

    let root = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok(Project {
        package: Package { name: acc.name, version: acc.version, almide_min: acc.almide_min },
        dependencies: acc.deps,
        permissions: acc.permissions,
        native_deps: acc.native_deps,
        root,
    })
}

/// Verify the installed compiler satisfies the package's minimum version.
/// Returns `Err` with a human-readable message when the pin is violated.
/// `ALMIDE_SKIP_VERSION_CHECK=1` bypasses the check.
pub fn check_compiler_version(project: &Project) -> Result<(), String> {
    let skip = std::env::var("ALMIDE_SKIP_VERSION_CHECK").is_ok();
    check_compiler_version_with(project, skip)
}

/// Env-free core of [`check_compiler_version`]: the `ALMIDE_SKIP_VERSION_CHECK`
/// bypass arrives as the `skip` parameter so tests never touch process env.
/// (Process env is process-GLOBAL: parallel `cargo test` threads racing on
/// `set_var`/`remove_var` made any env-reading sibling test flaky — the
/// recurring `check_rejects_malformed_pin` CI failure.)
pub fn check_compiler_version_with(project: &Project, skip: bool) -> Result<(), String> {
    let Some(required) = project.package.almide_min.as_deref() else { return Ok(()); };
    if skip { return Ok(()); }
    let installed = env!("CARGO_PKG_VERSION");
    let req = semver::VersionReq::parse(&format!(">={}", required))
        .map_err(|e| format!(
            "invalid `almide` version pin '{}' in almide.toml [package]: {}",
            required, e
        ))?;
    let have = semver::Version::parse(installed)
        .map_err(|e| format!("internal: installed version '{}' unparseable: {}", installed, e))?;
    if req.matches(&have) { return Ok(()); }
    Err(format!(
        "package '{}' requires almide >= {}\n  installed version: {}\n  run 'almide self-update' to update, \
         or set ALMIDE_SKIP_VERSION_CHECK=1 to bypass",
        project.package.name, required, installed
    ))
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
    let mut path: Option<String> = None;

    for item in inner.split(',') {
        if let Some((k, v)) = parse_kv(item) {
            match k {
                "git" => git = v,
                "tag" => tag = Some(v),
                "branch" => branch = Some(v),
                "version" => version = Some(v),
                "path" => path = Some(v),
                _ => {}
            }
        }
    }

    if git.is_empty() && path.is_none() {
        return None;
    }

    Some(Dependency { name, git, tag, branch, version, path })
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
