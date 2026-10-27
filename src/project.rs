/// Project configuration (almide.toml) and dependency management.

use std::path::{Path, PathBuf};

use almide_base::diagnostic::Diagnostic;

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
    /// The checkout itself (the directory holding its `almide.toml` and
    /// `native/`). `source_dir` is its `src/` when it has one. The native
    /// build reads this package's `native/` from HERE, so the Rust modules
    /// and the `.almd` sources come from one checkout (#3094).
    pub package_dir: PathBuf,
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
    /// `subdir = "…"` (#3381): the package directory inside the git
    /// repository, relative to its root, normalized (`/`-separated, no `.`
    /// or `..` components, no trailing `/`). `None` = the repository root
    /// is the package. Only a git dependency has one.
    pub subdir: Option<String>,
    /// Where the manifest declares this dependency (`path:line`), for the
    /// errors only the fetch can find (a missing `subdir`, a package name
    /// that is not this key). `None` when it was not read from a manifest.
    pub declared_at: Option<String>,
}

impl Dependency {
    /// `<declared_at>: ` — the prefix a fetch-time error about this
    /// dependency starts with, or nothing when it has no manifest line.
    pub fn location_prefix(&self) -> String {
        self.declared_at.as_deref().map(|at| format!("{at}: ")).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub package: Package,
    pub dependencies: Vec<Dependency>,
    /// `[permissions] allow = [...]`: the effect capabilities this package
    /// may use (Security Layer 2), e.g. `["IO", "Net"]`. Each name is an
    /// `Effect` category (`allowed_effects`); any other name is refused
    /// (#3247). `None` (no `allow` key) leaves capabilities unchecked;
    /// `Some([])` (an explicit empty list) allows none, so every capability
    /// use is refused (#4101). An empty list used to mean "allow all".
    pub permissions: Option<Vec<String>>,
    /// `[permissions] proc = [...]` (#2589, ADR-0025): the commands the
    /// subprocess family may start. `None` (no key) = any command.
    pub proc_allow: Option<Vec<String>>,
    /// `[permissions] fs = [...]` (#4022): the directories the program may
    /// touch, anchored to [`Project::root`] when relative. `None` (no key) =
    /// every path, as before; `[]` = none.
    pub fs_allow: Option<Vec<String>>,
    /// `[permissions] native = [...]` (#4093): the dependency packages whose
    /// native code (`native/`, `[native-deps]`, `@extern(rust|c)` fns) the
    /// build may compile. A dependency that brings native code and is not
    /// named here is refused; the root package's own native code needs no
    /// grant. See `crate::native_grant`.
    pub native_grants: Vec<String>,
    /// `[trust] owners = [...]` (#4093): the `host/owner`s this project
    /// fetches git dependencies from beyond `github.com/almide`, which is
    /// trusted by default. Only the root manifest's list is read. See
    /// `crate::trust`.
    pub trust_owners: Vec<String>,
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
    /// The platform key of `[target.<key>.native-deps]` (#3350) — a
    /// `cfg(...)` expression or a target triple, as written and validated —
    /// or `None` for `[native-deps]`, which every target gets.
    pub target: Option<String>,
}

/// `almide.toml` as the `toml` crate reads it (#3253). The manifest used to
/// be read one line at a time, so any value spread over several lines was
/// lost: `allow = [` with its names on the lines below read as `allow = []`,
/// which means "every capability". Each array spelling TOML has (multi-line,
/// a trailing comma, comments between items) and a multi-line inline table
/// now read as TOML says.
///
/// Two views of one parse: `values` for what the keys hold, `spans` (the
/// crate's spanned document) for what a `toml::Table` drops — the line an
/// `allow` name is written on, and the order the dependency tables write
/// their entries in (a TOML table has no order of its own).
struct Manifest<'i> {
    values: toml::Table,
    spans: toml::de::DeTable<'i>,
}

type SpannedValue<'i> = toml::Spanned<toml::de::DeValue<'i>>;

/// The 1-based line holding byte `offset` of `content`.
fn line_of(content: &str, offset: usize) -> usize {
    content.as_bytes()[..offset.min(content.len())].iter().filter(|&&b| b == b'\n').count() + 1
}

/// The byte offset each line of `content` starts at, so N lookups cost one
/// scan plus N binary searches. `line_of` per warning rescanned the file from
/// its start, and a manifest of N unknown tables took O(N²) (#4194).
struct LineStarts(Vec<usize>);

impl LineStarts {
    fn new(content: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(content.bytes().enumerate().filter(|&(_, b)| b == b'\n').map(|(i, _)| i + 1));
        LineStarts(starts)
    }

    /// Equal to [`line_of`] at every offset.
    fn line(&self, offset: usize) -> usize {
        self.0.partition_point(|&start| start <= offset)
    }
}

/// The most bytes read from `almide.toml` or `almide.lock` (#4194): 1 MiB.
/// A real manifest is a few hundred bytes to a few KB, and a lock line is
/// about 200 bytes, so 1 MiB holds a lock of several thousand dependencies —
/// far past any real project — while a file linked to `/dev/zero` or a
/// generated multi-gigabyte one stops here instead of exhausting memory.
/// The default of the `manifest_bytes` compile-cost budget (ADR-0030), which
/// is what [`read_project_file_bytes`] reads; `--budget manifest_bytes=N`
/// moves it, nothing in a project does.
pub const PROJECT_FILE_MAX_BYTES: u64 = 1 << 20;

fn project_file_max_bytes() -> u64 {
    almide_base::budget::limit(almide_base::budget::BudgetId::ManifestBytes)
}

/// The bytes of `almide.toml` / `almide.lock` at `path`, refused unless it is
/// a regular file (a symlink to one is followed) of at most
/// [`PROJECT_FILE_MAX_BYTES`]. A repository can commit either as a symlink to
/// `/dev/zero`, a FIFO or a device; reading one unbounded took the machine's
/// memory (#4194). The length is checked before reading and the read itself
/// is bounded, so a file that grows in between is refused as well.
pub fn read_project_file_bytes(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let failed = |e: std::io::Error| format!("Failed to read {}: {}", path.display(), e);
    let max = project_file_max_bytes();
    let too_big = || {
        format!(
            "{} is larger than {} bytes, which no manifest or lock file is\n  hint: replace it with the real file",
            path.display(),
            max
        )
    };
    let meta = std::fs::metadata(path).map_err(failed)?;
    if !meta.is_file() {
        return Err(format!(
            "{} is not a regular file — it is read only when it is one (a symlink to a regular file is followed)\n  hint: replace it with the real file",
            path.display()
        ));
    }
    if meta.len() > max {
        return Err(too_big());
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    std::fs::File::open(path).map_err(failed)?.take(max.saturating_add(1)).read_to_end(&mut bytes).map_err(failed)?;
    almide_base::budget::observe(almide_base::budget::BudgetId::ManifestBytes, bytes.len() as u64);
    if bytes.len() as u64 > max {
        return Err(too_big());
    }
    Ok(bytes)
}

/// [`read_project_file_bytes`] as UTF-8 text.
pub fn read_project_file(path: &Path) -> Result<String, String> {
    String::from_utf8(read_project_file_bytes(path)?)
        .map_err(|e| format!("Failed to read {}: stream did not contain valid UTF-8 ({e})", path.display()))
}

/// Manifest text as it may be printed: every control and bidi character
/// escaped (the one escaper for printed user data, `almide_base::err_text`). A key or value
/// from a cloned repository is never written to a terminal raw (#4126).
fn shown(s: &str) -> String {
    almide_base::err_text::almide_rt_errtext_escape(s)
}

/// [`shown`] line by line: for a multi-line message (a TOML error quoting
/// its source line) whose own line breaks are layout.
fn shown_lines(s: &str) -> String {
    s.split('\n').map(shown).collect::<Vec<_>>().join("\n")
}

/// Refuse a manifest whose keys, or whose strings that name a package or a
/// dependency, hold a control or bidi character (#4126): every one of them is
/// printed by some command (`Built …`, `Fetching … from …`), and none can be
/// written meaningfully with one. Other string values (a `description`)
/// may hold a newline or a tab. Every other such character is refused.
fn refuse_control_text(path: &Path, content: &str, table: &toml::de::DeTable<'_>, trail: &[&str]) -> Result<(), String> {
    let bad = |s: &str, strict: bool| {
        s.chars().any(|c| almide_base::err_text::almide_rt_errtext_is_escaped(c) && (strict || !matches!(c, '\n' | '\t' | '\r')))
    };
    for (key, value) in table {
        let k: &str = key.get_ref().as_ref();
        let refuse = |at: usize, what: &str| {
            Err(located(
                path,
                content,
                at,
                &format!("{what} `{}` holds a control or bidi character\n  hint: delete it; almide.toml prints these strings", shown(k)),
            ))
        };
        if bad(k, true) {
            return refuse(key.span().start, "key");
        }
        let mut here: Vec<&str> = trail.to_vec();
        here.push(k);
        let strict = here.first() == Some(&"dependencies")
            || (here.first() == Some(&"package") && matches!(k, "name" | "version" | "almide"));
        match value.get_ref() {
            toml::de::DeValue::String(v) if bad(v, strict) => return refuse(value.span().start, "the value of"),
            toml::de::DeValue::Table(t) => refuse_control_text(path, content, t, &here)?,
            toml::de::DeValue::Array(items) => {
                for item in items {
                    match item.get_ref() {
                        toml::de::DeValue::String(v) if bad(v, strict) => return refuse(item.span().start, "an entry of"),
                        toml::de::DeValue::Table(t) => refuse_control_text(path, content, t, &here)?,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `path:line: message`, the line found from a byte offset.
fn located(path: &Path, content: &str, offset: usize, message: &str) -> String {
    format!("{}:{}: {message}", path.display(), line_of(content, offset))
}

/// Parse `content` as the manifest, or say where it is not TOML.
fn read_manifest<'i>(path: &Path, content: &'i str) -> Result<Manifest<'i>, String> {
    let not_toml = |e: toml::de::Error| {
        let message = format!(
            "{}\n  hint: almide.toml is read as TOML — fix this line; every command reads the manifest",
            shown(e.message().trim())
        );
        match e.span() {
            Some(s) => located(path, content, s.start, &message),
            None => format!("{}: {message}", path.display()),
        }
    };
    let values: toml::Table = toml::from_str(content).map_err(not_toml)?;
    let spans = toml::de::DeTable::parse(content).map_err(not_toml)?.into_inner();
    refuse_control_text(path, content, &spans, &[])?;
    Ok(Manifest { values, spans })
}

/// The spanned value under `key` in a spanned table.
fn spanned_entry<'a, 'i>(table: &'a toml::de::DeTable<'i>, key: &str) -> Option<&'a SpannedValue<'i>> {
    table.iter().find(|(k, _)| k.get_ref().as_ref() == key).map(|(_, v)| v)
}

/// A string value as written; any other value as its TOML text (an inline
/// table stays a table, so a `[native-deps]` spec reaches Cargo.toml intact).
fn value_text(v: &toml::Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

impl Manifest<'_> {
    /// `[table]`'s entries in the order the file writes them.
    fn entries_in_file_order(&self, table: &str) -> Vec<(String, toml::Value)> {
        debug_assert!(TOP_LEVEL_KEYS.contains(&table), "[{table}] is read but not in TOP_LEVEL_KEYS");
        let Some(values) = self.values.get(table).and_then(toml::Value::as_table) else { return Vec::new() };
        let mut keys: Vec<(usize, String)> = spanned_entry(&self.spans, table)
            .and_then(|t| t.get_ref().as_table())
            .map(|t| t.iter().map(|(k, _)| (k.span().start, k.get_ref().to_string())).collect())
            .unwrap_or_default();
        keys.sort();
        keys.into_iter().filter_map(|(_, k)| values.get(&k).map(|v| (k, v.clone()))).collect()
    }

    /// Every `[target.<key>.native-deps]` entry (#3350), tables in file order
    /// and entries in file order within each. A `<key>` Cargo would refuse,
    /// a `[target.<key>]` table holding anything but `native-deps`, or a
    /// `native-deps` that is not a table is refused on the line that writes it.
    fn target_native_deps(&self, path: &Path, content: &str) -> Result<Vec<NativeDep>, String> {
        let Some(target) = spanned_entry(&self.spans, "target") else { return Ok(Vec::new()) };
        let Some(platforms) = target.get_ref().as_table() else {
            return Err(located(path, content, target.span().start, "[target] must be a table of `[target.'cfg(...)'.native-deps]` tables"));
        };
        let mut keyed: Vec<_> = platforms.iter().collect();
        keyed.sort_by_key(|(k, _)| k.span().start);
        let mut out = Vec::new();
        for (key, platform) in keyed {
            let key_text = key.get_ref().to_string();
            let at = key.span().start;
            crate::cargo_cfg::validate_target_key(&key_text).map_err(|e| {
                located(path, content, at, &format!(
                    "invalid platform `{key_text}` in [target.'{key_text}'.native-deps]: {e}\n  \
                     hint: write `cfg(...)` as Cargo does, e.g. [target.'cfg(target_os = \"android\")'.native-deps], or a target triple"
                ))
            })?;
            let Some(tables) = platform.get_ref().as_table() else {
                return Err(located(path, content, at, &format!("[target.'{key_text}'] must be a table holding `native-deps`")));
            };
            if let Some((sub, _)) = tables.iter().find(|(k, _)| !TARGET_PLATFORM_KEYS.contains(&k.get_ref().as_ref())) {
                return Err(located(path, content, sub.span().start, &format!(
                    "unknown table `{}` in [target.'{key_text}'] — only `native-deps` can be target-specific\n  \
                     hint: write [target.'{key_text}'.native-deps]",
                    sub.get_ref()
                )));
            }
            let Some(deps) = spanned_entry(tables, "native-deps") else { continue };
            let Some(deps) = deps.get_ref().as_table() else {
                return Err(located(path, content, deps.span().start, &format!("[target.'{key_text}'.native-deps] must be a table")));
            };
            let values = self.values.get("target").and_then(|t| t.get(&key_text)).and_then(|t| t.get("native-deps"));
            let mut entries: Vec<_> = deps.iter().map(|(k, _)| (k.span().start, k.get_ref().to_string())).collect();
            entries.sort();
            out.extend(entries.into_iter().filter_map(|(_, name)| {
                let spec = values.and_then(|v| v.get(&name)).map(value_text)?;
                Some(NativeDep { name, spec, target: Some(key_text.clone()) })
            }));
        }
        Ok(out)
    }

    /// `[dependencies]` in file order, each validated (a `subdir` is refused
    /// on its line — #3381).
    fn dependencies(&self, path: &Path, content: &str) -> Result<Vec<Dependency>, String> {
        let spans = spanned_entry(&self.spans, "dependencies").and_then(|t| t.get_ref().as_table());
        // One index and one line table for the whole section: a lookup per
        // entry was O(N) each, so N dependencies cost O(N²) (#4194).
        let by_name: std::collections::HashMap<&str, &SpannedValue<'_>> =
            spans.map(|t| t.iter().map(|(k, v)| (k.get_ref().as_ref(), v)).collect()).unwrap_or_default();
        let lines = LineStarts::new(content);
        let mut out = Vec::new();
        for (name, value) in self.entries_in_file_order("dependencies") {
            let entry = by_name.get(name.as_str()).copied();
            if let Some(dep) = dependency_from(name, &value, entry, path, content, &lines)? {
                out.push(dep);
            }
        }
        Ok(out)
    }

    /// `[package].<key>` as text. `key` must be one of [`PACKAGE_KEYS`] — the
    /// list the unknown-key warning judges against (#3382).
    fn package_field(&self, key: &str) -> Option<String> {
        debug_assert!(PACKAGE_KEYS.contains(&key), "[package].{key} is read but not in PACKAGE_KEYS");
        self.values.get("package").and_then(|p| p.get(key)).map(value_text)
    }

    /// `[permissions].<key>` as strings, each with the byte offset it is
    /// written at; `None` when the key is absent. Anything but an array of
    /// strings is refused on its line, never read as an empty list.
    fn permission_list(&self, path: &Path, content: &str, key: &str) -> Result<Option<Vec<(String, usize)>>, String> {
        debug_assert!(PERMISSION_KEYS.contains(&key), "[permissions].{key} is read but not in PERMISSION_KEYS");
        self.string_list(path, content, "permissions", key)
    }

    /// `[trust].owners` (#4093), each entry checked to be `host/owner` on
    /// its line.
    fn trust_owners(&self, path: &Path, content: &str) -> Result<Vec<String>, String> {
        let owners = self.string_list(path, content, "trust", "owners")?.unwrap_or_default();
        owners
            .into_iter()
            .map(|(o, at)| crate::trust::validate_owner_entry(&o).map(|()| o).map_err(|e| located(path, content, at, &e)))
            .collect()
    }

    /// `[<table>].<key>` as an array of strings, as in [`Self::permission_list`].
    fn string_list(&self, path: &Path, content: &str, table_name: &str, key: &str) -> Result<Option<Vec<(String, usize)>>, String> {
        let Some(perm) = spanned_entry(&self.spans, table_name) else { return Ok(None) };
        let Some(table) = perm.get_ref().as_table() else {
            return Err(located(path, content, perm.span().start, &format!("[{table_name}] must be a table")));
        };
        let Some(list) = spanned_entry(table, key) else { return Ok(None) };
        let not_a_list = || {
            located(path, content, list.span().start, &format!("[{table_name}].{key} must be an array of strings, like `{key} = [\"…\"]`"))
        };
        let items = list.get_ref().as_array().ok_or_else(not_a_list)?;
        items
            .into_iter()
            .map(|v| v.get_ref().as_str().map(|s| (s.to_string(), v.span().start)).ok_or_else(not_a_list))
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

/// One `[dependencies]` entry: a table naming `git` or `path`. Any other
/// shape is not a dependency this reader knows, as before. `entry` is the
/// entry's spanned value, for the line of a `subdir` that is refused.
fn dependency_from(
    name: String,
    value: &toml::Value,
    entry: Option<&SpannedValue<'_>>,
    path: &Path,
    content: &str,
    lines: &LineStarts,
) -> Result<Option<Dependency>, String> {
    let Some(table) = value.as_table() else { return Ok(None) };
    let field = |k: &str| {
        debug_assert!(DEPENDENCY_KEYS.contains(&k), "dependency key `{k}` is read but not in DEPENDENCY_KEYS");
        table.get(k).map(value_text)
    };
    let git = field("git").unwrap_or_default();
    let dep_path = field("path");
    if git.is_empty() && dep_path.is_none() {
        return Ok(None);
    }
    // The byte offset of `key` in this entry (its value), else of the entry.
    let offset_of = |key: &str| -> usize {
        let Some(entry) = entry else { return 0 };
        entry
            .get_ref()
            .as_table()
            .and_then(|t| spanned_entry(t, key))
            .map_or(entry.span().start, |v| v.span().start)
    };
    let declared_at = Some(format!("{}:{}", path.display(), lines.line(offset_of("git"))));
    let subdir = match table.get("subdir") {
        None => None,
        Some(raw) => {
            let refuse = |msg: String| located(path, content, offset_of("subdir"), &msg);
            let Some(raw) = raw.as_str() else {
                return Err(refuse(format!(
                    "`subdir` of dependency `{name}` must be a string, like `subdir = \"{name}\"`"
                )));
            };
            if let Some(p) = &dep_path {
                return Err(refuse(format!(
                    "`subdir` applies to git dependencies only — dependency `{name}` is a `path` \
                     dependency, and `path` already names the package directory\n  \
                     hint: write `path = \"{}/{}\"` and delete `subdir`",
                    p.trim_end_matches('/'),
                    raw.trim_matches('/'),
                )));
            }
            Some(normalize_subdir(raw).map_err(|why| {
                refuse(format!("invalid `subdir = \"{raw}\"` in dependency `{name}`: {why}"))
            })?)
        }
    };
    Ok(Some(Dependency {
        name,
        git,
        tag: field("tag"),
        branch: field("branch"),
        version: field("version"),
        path: dep_path,
        subdir,
        declared_at,
    }))
}

/// A `subdir` as written → its normalized spelling, or why it cannot name a
/// directory inside the repository (#3381). The rule is lexical, so it is
/// decided before anything is fetched: a relative path of `/`-separated
/// names. `.` components and repeated or trailing `/` are dropped; `..` is
/// refused outright (even `a/../b`, which stays inside — one spelling per
/// directory keeps the lock and the cache key canonical), as are an absolute
/// path, a `\` separator and a path that names the root itself. The fetch
/// additionally refuses a subdir that resolves outside the clone through a
/// symlink.
pub fn normalize_subdir(raw: &str) -> Result<String, String> {
    let hint_rel = "  hint: write the package directory relative to the repository root, like `subdir = \"pkgs/ceangal\"`";
    if raw.trim().is_empty() {
        return Err("it is empty\n  hint: name the package directory, or delete `subdir` when the repository root is the package".to_string());
    }
    if raw.contains('\\') {
        return Err(format!("`\\` is not a separator here — `subdir` uses `/` on every platform\n  hint: write `{}`", raw.replace('\\', "/")));
    }
    let has_drive = raw.len() >= 2 && raw.as_bytes()[1] == b':' && raw.as_bytes()[0].is_ascii_alphabetic();
    if raw.starts_with('/') || has_drive {
        return Err(format!("it is an absolute path; `subdir` is relative to the repository root\n{hint_rel}"));
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(format!("`..` is not allowed — `subdir` names a directory inside the repository\n{hint_rel}"));
            }
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return Err("it names the repository root\n  hint: delete `subdir` — a dependency without one uses the repository root".to_string());
    }
    Ok(parts.join("/"))
}

/// Validate that a package name is a valid Almide identifier (no hyphens).
/// Like Go, the package name IS the import name — no implicit conversion.
/// Extracted verbatim.
/// A plain name: ASCII letters, digits and `_`, not starting with a digit.
/// A package name is an import name, and `build`, `compile` and `install`
/// also use it as a FILE name — so a name with a separator, `..`, a leading
/// `.` or an absolute path would write outside the project (#4183, #4193).
pub fn check_plain_name(name: &str) -> Result<(), String> {
    let first_ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if first_ok && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(format!("`{}` is not a plain name (letters, digits and `_`, not starting with a digit)", name.escape_debug()))
    }
}

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

/// A `[permissions] fs` root as an absolute path (#4022): a relative one is
/// the project's own directory, anchored to the manifest like a relative
/// dependency path, so the scope does not move with the working directory
/// the program is started from. Canonical when it exists.
fn anchor_fs_root(root: &Path, dir: &str) -> String {
    let joined = if Path::new(dir).is_absolute() { PathBuf::from(dir) } else { root.join(dir) };
    let abs = if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir().map(|c| c.join(&joined)).unwrap_or(joined)
    };
    std::fs::canonicalize(&abs).unwrap_or(abs).to_string_lossy().into_owned()
}

/// A relative `path` dependency is relative to the directory of the
/// `almide.toml` that declares it (the Cargo rule), never to the process
/// working directory. Anchoring it here, where the manifest's own directory is
/// in hand, means every consumer of `Dependency::path` (the fetch walk, native
/// dep injection, `dep-path`) sees one resolved spelling. A dependency's own
/// relative path dependency used to be looked up from wherever `almide` was
/// started, so `b = { path = "../deps/b" }` whose manifest said
/// `d = { path = "../d" }` looked for `d` beside the app (#2844). A manifest
/// in the working directory (root `.`) keeps its paths as written.
fn anchor_relative_dep_paths(deps: Vec<Dependency>, root: &Path) -> Vec<Dependency> {
    if root == Path::new(".") {
        return deps;
    }
    deps.into_iter()
        .map(|mut dep| {
            if let Some(p) = dep.path.as_deref()
                && Path::new(p).is_relative()
            {
                // Canonical when it exists, so two dependencies naming the
                // same directory by different relative spellings (a diamond:
                // `deps/b/../d`, `deps/c/../d`) visit it once.
                let joined = root.join(p);
                let anchored = std::fs::canonicalize(&joined).unwrap_or(joined);
                dep.path = Some(anchored.to_string_lossy().into_owned());
            }
            dep
        })
        .collect()
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
/// table header written twice (#2583). TOML forbids both, and the `toml`
/// crate refuses them too (the manifest reader is that crate since #3253),
/// but only as a bare "duplicate key"; this names both lines and the fix.
/// The line-based reader before it accepted the second line and kept BOTH
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
const MANIFEST_TABLES: &[&str] = &["package", "dependencies", "permissions", "native-deps", "trust"];

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
                format!("dependency `{}` is declared twice in [dependencies]", shown(&key))
            } else {
                format!("key `{}` is declared twice in [{}]", shown(&key), shown(&dup.table))
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

/// The refusal for a capability name the vocabulary does not have, shared by
/// `[permissions].allow` and `--profile critical --allow` so the two read
/// alike (#3247). `site` names where it was written; `grantable` is that
/// site's vocabulary. A case-only miss (`io`) is suggested before an edit-
/// distance one, since the distance is case-blind and scores it 0.
pub fn unknown_capability_message(name: &str, site: &str, grantable: &[&str]) -> String {
    let near = grantable
        .iter()
        .find(|g| g.eq_ignore_ascii_case(name))
        .map(|g| g.to_string())
        .or_else(|| almide_base::diagnostic::suggest(name, grantable.iter().copied()));
    let hint = match near {
        Some(n) => format!("did you mean `{n}`?"),
        None => "write one of the capabilities above, or delete this name".to_string(),
    };
    format!(
        "unknown capability `{}` in {site} — grantable capabilities are {}\n  hint: {hint}",
        shown(name),
        grantable.join(", ")
    )
}

/// `[permissions].allow` names → the effect categories they grant. The ONE
/// matcher every enforcement path uses (`almide check`, `check --effects`,
/// `build` / `run`), so the vocabulary is `Effect::ALL` and cannot drift
/// between copies again. An unknown name is an error, never dropped (#3247).
///
/// A name is an ADR-0026 D2 leaf (`FS.read`, `Proc`) or one of the four
/// parents (`FS`, `Net`, `IO`, `Env`), which grants its leaves (#4155).
pub fn allowed_effects(allow: &[String]) -> Result<std::collections::HashSet<almide_ir::effect::Effect>, String> {
    use almide_ir::effect::Effect;
    let mut out = std::collections::HashSet::new();
    for name in allow {
        let leaves = Effect::granted_by(name).ok_or_else(|| {
            unknown_capability_message(name, "[permissions].allow", &Effect::grantable_names())
        })?;
        out.extend(leaves);
    }
    Ok(out)
}

/// The manifest whose `[permissions]` govern `entry`: the nearest
/// `almide.toml` in the entry file's directory or above it, else the one in
/// the working directory. Before, only the working directory was read, so
/// `almide run sub/app.almd` from outside the project ran with no
/// `[permissions]` at all.
pub fn manifest_for_entry(entry: &str) -> Option<std::path::PathBuf> {
    let start = std::path::Path::new(entry).parent().map(|p| if p.as_os_str().is_empty() { std::path::Path::new(".") } else { p });
    let mut dir = start.and_then(|d| d.canonicalize().ok());
    while let Some(d) = dir {
        let candidate = d.join("almide.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent().map(std::path::Path::to_path_buf);
    }
    let cwd = std::path::Path::new("almide.toml");
    cwd.is_file().then(|| cwd.to_path_buf())
}

/// The project [`manifest_for_entry`] names, parsed (`None` when there is
/// none or it does not parse — the manifest gate in `main` has already
/// refused a broken one in the working directory).
pub fn permissions_project_for(entry: &str) -> Option<Project> {
    manifest_for_entry(entry).and_then(|p| parse_toml(&p).ok())
}

/// Refuse an `almide.toml` before any command runs (#2583, #3247, #3253): a
/// key written twice, text that is not TOML, or a `[permissions].allow` name
/// that is not an effect category — each on the line that writes it. The
/// names come from the same parse `parse_toml` enforces, so a name on any
/// line of a multi-line array is judged like one on a single line.
pub fn check_manifest(path: &Path, content: &str) -> Result<(), String> {
    check_manifest_duplicates(path, content)?;
    let manifest = read_manifest(path, content)?;
    manifest.permission_list(path, content, "proc")?;
    manifest.permission_list(path, content, "fs")?;
    manifest.trust_owners(path, content)?;
    manifest.target_native_deps(path, content)?;
    manifest.dependencies(path, content)?;
    for (name, at) in manifest.permission_list(path, content, "allow")?.unwrap_or_default() {
        allowed_effects(std::slice::from_ref(&name)).map_err(|e| located(path, content, at, &e))?;
    }
    Ok(())
}

// ── Unknown keys (#3382) ────────────────────────────────────────────
//
// Each list below is the set of keys its table's reader reads, and the one
// place the unknown-key warning takes its vocabulary from. The readers
// `debug_assert!` that every key they read is listed, and
// `tests/manifest_unknown_key_test.rs` asserts the other direction: each
// listed key changes what `parse_toml` returns, so a listed key the reader
// stopped reading (or a new reader key nobody listed) fails a test instead of
// drifting.

/// The top-level tables `parse_toml` reads.
pub const TOP_LEVEL_KEYS: &[&str] = &["package", "dependencies", "native-deps", "permissions", "target", "trust"];
/// The keys `[package]` has.
pub const PACKAGE_KEYS: &[&str] = &["name", "version", "almide"];
/// `[package]` keys that describe the package and that no reader reads, so
/// they draw no warning: `edition` is what `almide init` writes, and the
/// others are the metadata manifests in use already carry (`description` is
/// in most of them). A key here must change nothing — the test asserts it —
/// so a reader that starts reading one moves it to [`PACKAGE_KEYS`].
pub const PACKAGE_METADATA_KEYS: &[&str] = &["description", "edition", "license", "repository"];
/// The keys one `[dependencies]` entry has.
pub const DEPENDENCY_KEYS: &[&str] = &["git", "tag", "branch", "version", "path", "subdir"];
/// The keys `[permissions]` has.
pub const PERMISSION_KEYS: &[&str] = &["allow", "proc", "fs", "native"];
/// The keys `[trust]` has (#4093).
pub const TRUST_KEYS: &[&str] = &["owners"];
/// The tables a `[target.<platform>]` table may hold (anything else is an
/// error, not a warning — it was refused before #3382).
pub const TARGET_PLATFORM_KEYS: &[&str] = &["native-deps"];

/// `a` and `b` differ by at most one insertion, deletion, substitution or
/// swap of two adjacent characters (Damerau-Levenshtein ≤ 1) — `brnach` is
/// one swap from `branch`, which plain Levenshtein counts as two.
fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a == b {
        return true;
    }
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let (ra, rb) = (&a[prefix..], &b[prefix..]);
    match (ra.len(), rb.len()) {
        (n, m) if n == m => ra[1..] == rb[1..] || (n >= 2 && ra[0] == rb[1] && ra[1] == rb[0] && ra[2..] == rb[2..]),
        (n, m) if n == m + 1 => ra[1..] == *rb,
        (n, m) if n + 1 == m => *ra == rb[1..],
        _ => false,
    }
}

/// A warning located at byte `offset` of the manifest.
fn manifest_warning(path: &Path, lines: &LineStarts, offset: usize, message: String, hint: String) -> Diagnostic {
    let mut d = Diagnostic::warning(message, hint, String::new());
    d.file = Some(path.display().to_string());
    d.line = Some(lines.line(offset));
    d
}

/// One warning per key of `table` that is not in `accepted`, in file order.
/// `site` names the table as a reader would; `what` is `key` or `table`.
fn unknown_key_warnings(
    path: &Path,
    lines: &LineStarts,
    table: &toml::de::DeTable<'_>,
    site: &str,
    what: &str,
    accepted: &[&str],
) -> Vec<Diagnostic> {
    let mut unknown: Vec<(usize, String)> = table
        .iter()
        .filter(|(k, _)| !accepted.contains(&k.get_ref().as_ref()))
        .map(|(k, _)| (k.span().start, k.get_ref().to_string()))
        .collect();
    unknown.sort();
    if unknown.is_empty() {
        return Vec::new();
    }
    let list = accepted.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ");
    unknown
        .into_iter()
        .map(|(at, key)| {
            let hint = match accepted.iter().find(|k| within_one_edit(&key, k)) {
                Some(k) => format!("did you mean `{k}`? The {what}s {site} accepts are {list}"),
                None => format!("delete it, or write one of the {what}s {site} accepts: {list}"),
            };
            let message = format!("unknown {what} `{}` in {site} is ignored — it changes nothing", shown(&key));
            manifest_warning(path, lines, at, message, hint)
        })
        .collect()
}

/// The warnings for keys `almide.toml` writes that no reader reads (#3382):
/// an unknown top-level table, an unknown key in `[package]`,
/// `[permissions]` or a `[dependencies]` entry (inline table, dotted keys or
/// `[dependencies.<name>]`), and a dependency entry that names neither `git`
/// nor `path`, which declares nothing. Each names its line, the key, the
/// keys that table reads, and the key one edit away when there is one.
/// `[native-deps]` entries are crate names, so any key is one; their specs
/// are Cargo's to judge. `[target.<platform>]` refuses an unknown table
/// already (an error). An unreadable manifest has no warnings — its error
/// comes from [`check_manifest`]. A warning never fails a command.
pub fn manifest_warnings(path: &Path, content: &str) -> Vec<Diagnostic> {
    let Ok(manifest) = read_manifest(path, content) else { return Vec::new() };
    let lines = LineStarts::new(content);
    let lines = &lines;
    let mut out = unknown_key_warnings(path, lines, &manifest.spans, "almide.toml", "table", TOP_LEVEL_KEYS);
    let sub = |name: &str| spanned_entry(&manifest.spans, name).and_then(|t| t.get_ref().as_table());
    if let Some(t) = sub("package") {
        let known: Vec<&str> = PACKAGE_KEYS.iter().chain(PACKAGE_METADATA_KEYS).copied().collect();
        out.extend(unknown_key_warnings(path, lines, t, "[package]", "key", &known));
    }
    if let Some(t) = sub("permissions") {
        out.extend(unknown_key_warnings(path, lines, t, "[permissions]", "key", PERMISSION_KEYS));
    }
    if let Some(t) = sub("trust") {
        out.extend(unknown_key_warnings(path, lines, t, "[trust]", "key", TRUST_KEYS));
    }
    let Some(deps) = sub("dependencies") else { return out };
    let mut entries: Vec<_> = deps.iter().collect();
    entries.sort_by_key(|(k, _)| k.span().start);
    for (name, entry) in entries {
        let name = shown(name.get_ref().as_ref());
        let ignored = |why: &str| {
            manifest_warning(
                path,
                lines,
                entry.span().start,
                format!("dependency `{name}` is ignored — {why}"),
                format!(
                    "write `{name} = {{ git = \"https://…\", tag = \"v0.1.0\" }}` or `{name} = {{ path = \"../{name}\" }}`, or delete the entry"
                ),
            )
        };
        let Some(t) = entry.get_ref().as_table() else {
            out.push(ignored("an entry is a table naming `git` or `path`"));
            continue;
        };
        out.extend(unknown_key_warnings(path, lines, t, &format!("dependency `{name}`"), "key", DEPENDENCY_KEYS));
        if spanned_entry(t, "git").is_none() && spanned_entry(t, "path").is_none() {
            out.push(ignored("it names neither `git` nor `path`"));
        }
    }
    out
}

/// `[package].name` of the manifest at `path`, read as TOML and not otherwise
/// validated; `None` when there is no readable manifest or no name.
pub fn manifest_package_name(path: &Path) -> Option<String> {
    let content = read_project_file(path).ok()?;
    read_manifest(path, &content).ok()?.package_field("name").filter(|n| !n.is_empty())
}

pub fn parse_toml(path: &Path) -> Result<Project, String> {
    let content = read_project_file(path)?;
    check_manifest_duplicates(path, &content)?;
    let manifest = read_manifest(path, &content)?;

    let name = manifest.package_field("name").unwrap_or_default();
    validate_package_name(&name)?;
    let package = Package {
        name,
        version: manifest.package_field("version").unwrap_or_else(|| "0.1.0".to_string()),
        almide_min: manifest.package_field("almide"),
    };
    let names = |key: &str| -> Result<Option<Vec<String>>, String> {
        Ok(manifest.permission_list(path, &content, key)?.map(|l| l.into_iter().map(|(n, _)| n).collect()))
    };
    let permissions = names("allow")?;
    let proc_allow = names("proc")?;
    let native_grants = names("native")?.unwrap_or_default();
    let trust_owners = manifest.trust_owners(path, &content)?;

    let root = project_root_from_toml_path(path);
    // Relative roots are the project's own directories: anchored to the
    // manifest, as relative dependency paths are.
    let fs_allow = names("fs")?.map(|l| l.iter().map(|d| anchor_fs_root(&root, d)).collect());
    let deps = manifest.dependencies(path, &content)?;
    let mut native_deps: Vec<NativeDep> = manifest
        .entries_in_file_order("native-deps")
        .into_iter()
        .map(|(name, value)| NativeDep { name, spec: value_text(&value), target: None })
        .collect();
    native_deps.extend(manifest.target_native_deps(path, &content)?);
    Ok(Project {
        package,
        dependencies: anchor_relative_dep_paths(deps, &root),
        permissions,
        proc_allow,
        fs_allow,
        native_grants,
        trust_owners,
        native_deps,
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
    // The field is a MINIMUM; `almide = ">=0.60"` spells the same thing and
    // is accepted as it (#4172). Any other operator stays an invalid pin.
    let required = required.trim().strip_prefix(">=").map(str::trim).unwrap_or(required.trim());
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
         or set ALMIDE_SKIP_VERSION_CHECK=1 to bypass (a release build honours it only with \
         `--allow-switch ALMIDE_SKIP_VERSION_CHECK`)",
        project.package.name, required, installed
    ))
}

/// Cache directory for dependencies
pub fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".almide").join("cache")
}

/// A locked dependency entry for almide.lock
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDep {
    pub name: String,
    pub git: String,
    pub ref_name: String,
    pub commit: String,
    /// The package directory inside the repository (#3381), as the
    /// manifest's normalized `subdir`; `None` = the repository root. Several
    /// entries may share one `(git, commit)` — one clone, distinct packages.
    pub subdir: Option<String>,
}

/// Parse almide.lock. The write format below is valid TOML (one inline table
/// per dependency), so the reader is the `toml` crate rather than a hand-rolled
/// line splitter: a ref containing `,` `}` or `"` round-trips instead of being
/// mangled, and a corrupted entry is an ERROR naming the dependency instead of
/// a line that silently vanishes from the lock set and gets re-resolved from
/// the network (#1465). The one-entry-per-line shape — the property that keeps
/// lock merges conflict-friendly — is untouched; only the reader changed.
pub fn parse_lock_file(path: &Path) -> Result<Vec<LockedDep>, String> {
    let content = read_project_file(path)?;
    // A lock written by a compiler that accepted a duplicated dependency in
    // almide.toml holds the same entry twice (#2583). The `toml` crate refuses
    // that as a bare "duplicate key"; say which lines and what to do instead.
    if let Some(dup) = find_duplicate_key(&content, None) {
        let what = match &dup.key {
            Some(key) => format!("lock entry `{}` appears twice", shown(key)),
            None => format!("table [{}] appears twice", shown(&dup.table)),
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
    // Every string of the lock is printed by some command (`Fetching …
    // (locked: <commit>)`), so a control or bidi character is refused (#4126).
    if let Ok(spans) = toml::de::DeTable::parse(&content) {
        refuse_control_text(path, &content, spans.get_ref(), &["dependencies"])?;
    }
    let table: toml::Table = content
        .parse()
        .map_err(|e: toml::de::Error| format!("{} is not valid TOML: {}", path.display(), shown_lines(&e.to_string())))?;
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
        let subdir = entry
            .get("subdir")
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    format!("{}: lock entry '{}' has a `subdir` that is not a string", path.display(), name)
                })
            })
            .transpose()?;
        locked.push(LockedDep { name, git, ref_name, commit, subdir });
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
        // `subdir` only when there is one, so a lock without subdir
        // dependencies stays byte-identical to what earlier compilers wrote.
        let subdir = dep.subdir.as_deref().map(|s| format!(", subdir = {}", toml_quoted(s))).unwrap_or_default();
        content.push_str(&format!(
            "{} = {{ git = {}, ref = {}, commit = {}{} }}\n",
            dep.name,
            toml_quoted(&dep.git),
            toml_quoted(&dep.ref_name),
            toml_quoted(&dep.commit),
            subdir
        ));
    }
    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Add `owner` (`host/owner`) to `[trust] owners` in the manifest at `path`
/// (#4093), keeping every other byte: into the existing array, under an
/// existing `[trust]` header, or as a new `[trust]` table at the end. `Ok(false)`
/// when the owner is already listed. The result is re-read before it is kept,
/// so a manifest this cannot edit cleanly is left untouched with an error.
pub fn add_trust_owner(path: &Path, owner: &str) -> Result<bool, String> {
    crate::trust::validate_owner_entry(owner)?;
    let content = read_project_file(path)?;
    let current = parse_toml(path)?;
    if current.trust_owners.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
        return Ok(false);
    }
    let quoted = format!("\"{owner}\"");
    let by_hand = || format!("{}: cannot add {quoted} to [trust] owners automatically\n  hint: add it by hand: [trust] owners = [{quoted}]", path.display());
    let manifest = read_manifest(path, &content)?;
    let edited = match spanned_entry(&manifest.spans, "trust") {
        None => {
            let sep = if content.ends_with('\n') || content.is_empty() { "" } else { "\n" };
            format!("{content}{sep}\n[trust]\nowners = [{quoted}]\n")
        }
        Some(table) => {
            let table = table.get_ref().as_table().ok_or_else(by_hand)?;
            match spanned_entry(table, "owners") {
                Some(list) => {
                    let span = list.span();
                    let text = &content[span.start..span.end];
                    let close = span.start + text.rfind(']').ok_or_else(by_hand)?;
                    let empty = list.get_ref().as_array().is_some_and(|a| a.is_empty());
                    let before = content[span.start + 1..close].trim_end();
                    let insert = if empty || before.ends_with(',') { quoted.clone() } else { format!(", {quoted}") };
                    format!("{}{}{}", &content[..close], insert, &content[close..])
                }
                None => {
                    let mut offset = 0;
                    let mut at = None;
                    for line in content.split_inclusive('\n') {
                        offset += line.len();
                        if line.trim() == "[trust]" {
                            at = Some(offset);
                            break;
                        }
                    }
                    let at = at.ok_or_else(by_hand)?;
                    let nl = if content[..at].ends_with('\n') { "" } else { "\n" };
                    format!("{}{nl}owners = [{quoted}]\n{}", &content[..at], &content[at..])
                }
            }
        }
    };
    std::fs::write(path, &edited).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    match parse_toml(path) {
        Ok(p) if p.trust_owners.iter().any(|o| o.eq_ignore_ascii_case(owner)) => Ok(true),
        _ => {
            let _ = std::fs::write(path, &content);
            Err(by_hand())
        }
    }
}
