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
    /// The requested version this entry currently holds — MVS keeps the
    /// maximum requested version per `PkgId` (#1458).
    pub version: String,
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

/// `parse_toml`'s `[section]` header detection. Extracted verbatim (the
/// original if/else-if chain nested inside the section-header `if`, which
/// pushed that branch past the max-depth threshold).
fn detect_section(line: &str) -> &'static str {
    match line {
        "[package]" => "package",
        "[dependencies]" => "dependencies",
        "[permissions]" => "permissions",
        "[native-deps]" => "native-deps",
        _ => "",
    }
}

/// `parse_toml`'s per-line dispatch within the current `[section]`.
/// Extracted verbatim.
fn apply_toml_line(section: &str, line: &str, acc: &mut TomlAccum) {
    match section {
        "package" => apply_package_line(line, acc),
        "dependencies" => {
            if let Some(dep) = parse_dep_line(line) {
                acc.deps.push(dep);
            }
        }
        "permissions" => apply_permissions_line(line, acc),
        "native-deps" => apply_native_deps_line(line, acc),
        _ => {}
    }
}

/// Validate that a package name is a valid Almide identifier (no hyphens).
/// Like Go, the package name IS the import name — no implicit conversion.
/// Extracted verbatim.
fn validate_package_name(name: &str) -> Result<(), String> {
    if name.contains('-') {
        return Err(format!(
            "package name '{}' contains hyphens — use underscores instead\n  \
             hint: rename to '{}' in [package] name. The package name is the import name.",
            name,
            name.replace('-', "_"),
        ));
    }
    Ok(())
}

/// The project root is the directory containing `almide.toml` — `.` when
/// that directory is empty (a bare relative filename). Extracted verbatim.
fn project_root_from_toml_path(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// The bare or quoted key a `key = value` line assigns, or `None` for any
/// other line (blank, comment, header, or the continuation of a multi-line
/// value). Only keys spelled the way TOML spells a key are answered, so a
/// continuation line like `"a=b",` inside a multi-line array is not mistaken
/// for an assignment.
fn assigned_key(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    let bare = |k: &str| {
        !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    if bare(key) {
        return Some(key);
    }
    let quoted = key
        .strip_prefix('"')
        .and_then(|k| k.strip_suffix('"'))
        .or_else(|| key.strip_prefix('\'').and_then(|k| k.strip_suffix('\'')))?;
    (!quoted.contains(['"', '\''])).then_some(quoted)
}

/// A key (or a table header) written twice where TOML allows it once.
struct DuplicateKey {
    /// The table the key is in: `dependencies`, `package`, … or `""` for the
    /// top level (the lock file's shape). For a repeated header, the table
    /// itself.
    table: String,
    /// The key, or `None` when the table header itself is repeated.
    key: Option<String>,
    first_line: usize,
    second_line: usize,
}

/// The first key assigned twice in one table of `content`, or the first
/// table header written twice (#2583). TOML forbids both; the manifest reader
/// below is line-based rather than the `toml` crate, so it has to enforce it
/// itself — without this it silently accepted the second line and kept BOTH
/// dependencies, which the lock writer then recorded twice. `tables` limits
/// the scan to the tables a reader actually reads (`None` = every table).
fn find_duplicate_key(content: &str, tables: Option<&[&str]>) -> Option<DuplicateKey> {
    use std::collections::HashMap;
    let mut section = String::new();
    let mut headers: HashMap<String, usize> = HashMap::new();
    let mut seen: HashMap<(String, String), usize> = HashMap::new();
    for (idx, raw) in content.lines().enumerate() {
        let lineno = idx + 1;
        let line = raw.trim();
        if line.starts_with('[') && !line.starts_with("[[") && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            let read = tables.is_none_or(|t| t.contains(&section.as_str()));
            if read {
                if let Some(&first_line) = headers.get(&section) {
                    return Some(DuplicateKey {
                        table: section,
                        key: None,
                        first_line,
                        second_line: lineno,
                    });
                }
                headers.insert(section.clone(), lineno);
            }
            continue;
        }
        if tables.is_some_and(|t| !t.contains(&section.as_str())) {
            continue;
        }
        let Some(key) = assigned_key(line) else { continue };
        let slot = (section.clone(), key.to_string());
        if let Some(&first_line) = seen.get(&slot) {
            return Some(DuplicateKey {
                table: section,
                key: Some(key.to_string()),
                first_line,
                second_line: lineno,
            });
        }
        seen.insert(slot, lineno);
    }
    None
}

/// The tables `parse_toml` reads — a duplicate in any of them is refused.
const MANIFEST_TABLES: &[&str] = &["package", "dependencies", "permissions", "native-deps"];

/// Refuse an `almide.toml` that writes a key twice in a table the manifest
/// reader reads, naming both lines and the fix (#2583).
pub fn check_manifest_duplicates(path: &Path, content: &str) -> Result<(), String> {
    let Some(dup) = find_duplicate_key(content, Some(MANIFEST_TABLES)) else {
        return Ok(());
    };
    let at = format!("{}:{}", path.display(), dup.second_line);
    Err(match dup.key {
        Some(key) => {
            let what = if dup.table == "dependencies" {
                format!("dependency `{key}` is declared twice in [dependencies]")
            } else {
                format!("key `{key}` is declared twice in [{}]", dup.table)
            };
            format!(
                "{at}: {what} (first at line {first})\n  \
                 hint: keep one of lines {first} and {second} and delete the other — \
                 TOML allows a key only once per table",
                first = dup.first_line,
                second = dup.second_line,
            )
        }
        None => format!(
            "{at}: table [{table}] is declared twice (first at line {first})\n  \
             hint: move the entries under line {second} into the [{table}] table at line {first} \
             and delete the second header — TOML allows a table only once",
            table = dup.table,
            first = dup.first_line,
            second = dup.second_line,
        ),
    })
}

pub fn parse_toml(path: &Path) -> Result<Project, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    check_manifest_duplicates(path, &content)?;

    let mut acc = TomlAccum { version: "0.1.0".to_string(), ..TomlAccum::default() };
    let mut section = "";

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = detect_section(line);
            continue;
        }
        apply_toml_line(section, line, &mut acc);
    }

    validate_package_name(&acc.name)?;

    let root = project_root_from_toml_path(path);
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
    let skip = almide_base::env::flag("ALMIDE_SKIP_VERSION_CHECK");
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

/// Parse almide.lock. The write format below is valid TOML (one inline table
/// per dependency), so the reader is the `toml` crate rather than a hand-rolled
/// line splitter: a ref containing `,` `}` or `"` round-trips instead of being
/// mangled, and a corrupted entry is an ERROR naming the dependency instead of
/// a line that silently vanishes from the lock set and gets re-resolved from
/// the network (#1465). The one-entry-per-line shape — the property that keeps
/// lock merges conflict-friendly — is untouched; only the reader changed.
pub fn parse_lock_file(path: &Path) -> Result<Vec<LockedDep>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    // A lock written by a compiler that accepted a duplicated dependency in
    // almide.toml holds the same entry twice (#2583). The `toml` crate refuses
    // that as a bare "duplicate key"; say which lines and what to do instead.
    if let Some(dup) = find_duplicate_key(&content, None) {
        let what = match &dup.key {
            Some(key) => format!("lock entry `{key}` appears twice"),
            None => format!("table [{}] appears twice", dup.table),
        };
        return Err(format!(
            "{}:{}: {what} (first at line {first}) — almide.lock is generated, and an older \
             compiler wrote this from a dependency declared twice in almide.toml\n  \
             hint: make sure almide.toml declares the dependency once, then delete one of lines \
             {first} and {second} of {lock} (keep the one whose `git` matches almide.toml), or delete \
             {lock} and the next `almide check` / `almide run` rewrites it",
            path.display(),
            dup.second_line,
            first = dup.first_line,
            second = dup.second_line,
            lock = path.display(),
        ));
    }
    let table: toml::Table = content
        .parse()
        .map_err(|e| format!("{} is not valid TOML: {}", path.display(), e))?;
    let mut locked = Vec::new();
    for (name, val) in table {
        let entry = val.as_table().ok_or_else(|| {
            format!("{}: lock entry '{}' is not a table", path.display(), name)
        })?;
        let required = |key: &str| -> Result<String, String> {
            entry.get(key).and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: lock entry '{}' is missing string field '{}'",
                    path.display(),
                    name,
                    key
                )
            })
        };
        let git = required("git")?;
        let commit = required("commit")?;
        let ref_name =
            entry.get("ref").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        locked.push(LockedDep { name, git, ref_name, commit });
    }
    Ok(locked)
}

/// One TOML-escaped quoted string, so a `"` or `\` in a ref/url survives the
/// round trip — the writer half of #1465.
fn toml_quoted(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Write almide.lock
pub fn write_lock_file(path: &Path, locked: &[LockedDep]) -> Result<(), String> {
    let mut content = String::from("# almide.lock — auto-generated, do not edit\n\n");
    // One entry per name, whatever the caller hands in: a name written twice
    // makes the lock unreadable to every later run (#2583). The first entry
    // wins — the manifest's own first declaration.
    let mut written = std::collections::HashSet::new();
    for dep in locked.iter().filter(|d| written.insert(d.name.as_str())) {
        content.push_str(&format!(
            "{} = {{ git = {}, ref = {}, commit = {} }}\n",
            dep.name,
            toml_quoted(&dep.git),
            toml_quoted(&dep.ref_name),
            toml_quoted(&dep.commit)
        ));
    }
    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}
