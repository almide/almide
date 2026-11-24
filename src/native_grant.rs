//! `[permissions] native` (#4093): which dependency packages may bring native
//! code into the build.
//!
//! A dependency's native code reaches cargo three ways, all measured:
//!
//! - its `native/*.rs` modules (and the asset files beside them) are copied
//!   into the generated crate (`cli::crate_inputs::CrateInputs`);
//! - its `[native-deps]` (and `[target.<platform>.native-deps]`) crates are
//!   added to the generated `Cargo.toml`, so their `build.rs` and proc macros
//!   run during `almide build` / `run` / `test`;
//! - an `@extern(rust, "path", "fn")` fn is spliced into the crate as a call
//!   to `path::fn` — any Rust path, `std` included, with no `native/` at all —
//!   and an `@extern(c, …)` fn links a C symbol.
//!
//! All of it runs before, and outside, every `[permissions]` check. So a
//! dependency that brings any of it is refused unless the ROOT manifest names
//! it in `[permissions] native = [...]`. The root package's own native code
//! needs no grant: the developer wrote it. The rule is a property of the
//! dependency graph, not of the target, so every `--target` refuses the same
//! graph (as `[permissions]` itself is, #3275).

use std::path::{Path, PathBuf};

use crate::project::{FetchedDep, Project};

/// The native code one package brings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NativeCode {
    /// `native/*.rs` module files, as `native/<file>`, sorted.
    pub modules: Vec<String>,
    /// Other files under `native/` subdirectories (assets the modules
    /// `include_str!`), counted.
    pub assets: usize,
    /// `[native-deps]` crate names, in manifest order (a crate named for
    /// several platforms once).
    pub crates: Vec<String>,
    /// `@extern(rust|c)` fns, as `` `name` (file:line) ``, in file order.
    pub externs: Vec<String>,
    /// Fns whose body is a Rust template or a runtime binding:
    /// `@inline_rust`, and defensively `@intrinsic` / `@wasm_intrinsic`
    /// (E085 refuses those outside the stdlib), as
    /// `` @inline_rust fn `name` (file:line) `` (#4100).
    pub templates: Vec<String>,
    /// What the scan could not read: a `native/` directory it cannot list,
    /// an `almide.toml` that does not parse, an `.almd` file it cannot read
    /// or parse. Nothing proves such a package brings no native code, so it
    /// is refused like one that does.
    pub unscanned: Vec<String>,
}

impl NativeCode {
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.assets == 0 && self.crates.is_empty() && self.externs.is_empty() && self.templates.is_empty()
            && self.unscanned.is_empty()
    }

    /// One line naming every piece, e.g.
    /// `native/natval.rs; [native-deps] itoa; @extern fn `number` (src/mod.almd:2)`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.modules.is_empty() {
            parts.push(self.modules.join(", "));
        }
        if self.assets > 0 {
            parts.push(format!("{} native/ asset file(s)", self.assets));
        }
        if !self.crates.is_empty() {
            parts.push(format!("[native-deps] {}", self.crates.join(", ")));
        }
        if !self.externs.is_empty() {
            parts.push(format!("@extern fn {}", self.externs.join(", ")));
        }
        parts.extend(self.templates.iter().cloned());
        if !self.unscanned.is_empty() {
            parts.push(format!("not scannable (read as native code): {}", self.unscanned.join(", ")));
        }
        parts.join("; ")
    }
}

/// What [`native_code_of`] found in one dependency, and whether the root
/// grants it.
#[derive(Debug, Clone)]
pub struct DepNative {
    pub name: String,
    pub version: String,
    pub package_dir: PathBuf,
    /// Declared in the root's `[dependencies]` (else reached through one).
    pub direct: bool,
    pub code: NativeCode,
    pub granted: bool,
}

/// The native code of the package checked out at `package_dir`, whose
/// `.almd` sources live under `source_dir`.
pub fn native_code_of(package_dir: &Path, source_dir: &Path) -> NativeCode {
    let mut code = NativeCode::default();
    let native_dir = package_dir.join("native");
    if native_dir.is_dir() {
        if std::fs::read_dir(&native_dir).is_err() {
            code.unscanned.push("native/ (cannot be listed)".to_string());
        }
        for (rel, is_module) in native_files(&native_dir, Path::new("")) {
            if is_module {
                code.modules.push(format!("native/{}", rel.display()));
            } else {
                code.assets += 1;
            }
        }
    }
    let toml = package_dir.join("almide.toml");
    if std::fs::symlink_metadata(&toml).is_ok() {
        match crate::project::parse_toml(&toml) {
            Ok(proj) => {
                for nd in proj.native_deps {
                    if !code.crates.contains(&nd.name) {
                        code.crates.push(nd.name);
                    }
                }
            }
            Err(_) => code.unscanned.push("almide.toml (does not parse)".to_string()),
        }
    }
    let mut files = Vec::new();
    almd_files(source_dir, &mut files);
    files.sort();
    for file in files {
        let rel = file.strip_prefix(package_dir).unwrap_or(&file).display().to_string();
        let Some((externs, templates)) = native_fns(&file) else {
            code.unscanned.push(format!("{rel} (cannot be read or parsed)"));
            continue;
        };
        code.externs.extend(externs.into_iter().map(|(name, line)| format!("`{name}` ({rel}:{line})")));
        code.templates.extend(templates.into_iter().map(|(attr, name, line)| format!("@{attr} fn `{name}` ({rel}:{line})")));
    }
    code
}

/// `(path below native/, is a module)` for every file the native build
/// copies: `*.rs` directly in `native/` are modules, files in its
/// subdirectories travel as assets (`CrateInputs` copies the same set).
fn native_files(dir: &Path, rel: &Path) -> Vec<(PathBuf, bool)> {
    let mut names: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.file_name()).collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let path = dir.join(&name);
        let sub = rel.join(&name);
        if path.is_dir() {
            out.extend(native_files(&path, &sub).into_iter().map(|(p, _)| (p, false)));
        } else if rel.as_os_str().is_empty() {
            if path.extension().is_some_and(|e| e == "rs") {
                out.push((sub, true));
            }
        } else {
            out.push((sub, false));
        }
    }
    out
}

/// Every `.almd` file under `dir`, walked through the module resolver's own
/// enumeration ([`crate::resolve::source_entries`]): the grant scans read
/// exactly the tree a package's modules are loaded from (#4809 — a scan that
/// skipped `target/`, `native/` and dot-directories let a dependency's
/// `@extern` there build without the root's grant).
pub(crate) fn almd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let (files, dirs) = crate::resolve::source_entries(dir);
    out.extend(files);
    for d in dirs {
        almd_files(&d, out);
    }
}

/// The attributes that make a fn's body Rust text or a runtime binding
/// (#4100). `@inline_rust` is legal in a package (aes and sha1 use it, with
/// a `native/` module behind it); the other two are refused outside the
/// stdlib by E085, and are listed so a package that slipped past it is still
/// named here.
const TEMPLATE_ATTRS: &[&str] = &["inline_rust", "intrinsic", "wasm_intrinsic"];

/// The top-level fns in `file` that bring native code: `(name, line)` of
/// each `@extern` whose target the native build binds (`rust`/`rs`, `c`; the
/// predicate the checker and codegen use), and `(attribute, name, line)` of
/// each fn carrying one of [`TEMPLATE_ATTRS`]. `None` when the file cannot
/// be read or does not parse: the caller counts it as unscanned, never as
/// bringing nothing.
#[allow(clippy::type_complexity)]
fn native_fns(file: &Path) -> Option<(Vec<(String, usize)>, Vec<(String, String, usize)>)> {
    let (mut externs, mut templates) = (Vec::new(), Vec::new());
    let source = std::fs::read_to_string(file).ok()?;
    let tokens = crate::lexer::Lexer::tokenize(&source);
    let mut parser = crate::parser::Parser::new(tokens);
    let program = parser.parse().ok()?;
    // A recovered parse drops the declarations it could not read, and with
    // them any `@extern` they carried.
    if parser.errors.iter().any(|d| d.level == crate::diagnostic::Level::Error) {
        return None;
    }
    for d in &program.decls {
        let crate::ast::Decl::Fn { name, extern_attrs, attrs, span, .. } = d else { continue };
        let line = span.map_or(0, |s| s.line);
        if extern_attrs.iter().any(|a| crate::types::extern_abi::native_extern_kind(a.target.as_str()).is_some()) {
            externs.push((name.to_string(), line));
        }
        if let Some(a) = attrs.iter().find(|a| TEMPLATE_ATTRS.contains(&a.name.as_str())) {
            templates.push((a.name.to_string(), name.to_string(), line));
        }
    }
    Some((externs, templates))
}

/// Every fetched dependency of `proj` with the native code it brings.
/// Does the root's `[permissions] native` grant `fd`? The grant names a
/// package; when the root manifest itself depends on a package of that name,
/// the grant is for THAT origin only, so a same-named package from another
/// source is not covered by it (#4834).
fn is_granted(proj: &Project, fd: &FetchedDep) -> bool {
    proj.native_grants.iter().any(|g| *g == fd.pkg_id.name)
        && proj
            .dependencies
            .iter()
            .filter(|d| d.name == fd.pkg_id.name)
            .all(|d| crate::project::PackageOrigin::of(d) == fd.origin)
}

pub fn survey(proj: &Project, fetched: &[FetchedDep]) -> Vec<DepNative> {
    fetched
        .iter()
        .map(|fd| DepNative {
            name: fd.pkg_id.name.clone(),
            // The package's own `[package].version`: `fd.version` is the
            // requested one, empty for a path dependency.
            version: crate::project::parse_toml(&fd.package_dir.join("almide.toml"))
                .map(|p| p.package.version)
                .unwrap_or_else(|_| fd.version.clone()),
            package_dir: fd.package_dir.clone(),
            direct: proj.dependencies.iter().any(|d| d.name == fd.pkg_id.name && crate::project::PackageOrigin::of(d) == fd.origin),
            code: native_code_of(&fd.package_dir, &fd.source_dir),
            granted: is_granted(proj, fd),
        })
        .collect()
}

/// The refusal for the dependencies in `deps` that bring native code the
/// root does not grant, or `None` when there are none.
pub fn refusal(proj: &Project, deps: &[DepNative]) -> Option<String> {
    let refused: Vec<&DepNative> = deps.iter().filter(|d| !d.code.is_empty() && !d.granted).collect();
    if refused.is_empty() {
        return None;
    }
    let manifest = proj.root.join("almide.toml");
    let mut msg = String::new();
    for d in &refused {
        let how = if d.direct { "" } else { " (required by another dependency)" };
        msg.push_str(&format!(
            "error: dependency `{}`{} brings native code that {} does not grant\n  {} {} at {}\n  native code: {}\n",
            d.name, how, manifest.display(), d.name, d.version, d.package_dir.display(), d.code.describe()
        ));
    }
    let mut names = proj.native_grants.clone();
    names.extend(refused.iter().map(|d| d.name.clone()));
    let line = format!("native = [{}]", names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", "));
    msg.push_str("  note: native code is compiled and run by cargo during the build, before and outside every [permissions] check\n");
    msg.push_str(&format!("  hint: if you trust {}, grant it in {}:\n    [permissions]\n    {}",
        refused.iter().map(|d| format!("`{}`", d.name)).collect::<Vec<_>>().join(", "),
        manifest.display(), line));
    Some(msg)
}

/// One line per granted dependency that brings native code.
pub fn granted_lines(deps: &[DepNative]) -> Vec<String> {
    deps.iter()
        .filter(|d| d.granted && !d.code.is_empty())
        .map(|d| format!("note: building native code from `{}` (granted by [permissions].native): {}", d.name, d.code.describe()))
        .collect()
}

#[cfg(test)]
mod tests {
    /// ADR-0035 D3, extending #4809: the grant scan and the module resolver
    /// walk a package with the same primitive as the tree hash
    /// (`package_tree::dir_entries`), so on one fixture tree — dot
    /// directories, nesting, a non-module file and links to a file and to a
    /// directory outside the package — they list the same `.almd` files as
    /// `package_files`, and none behind a link.
    #[test]
    fn the_grant_scan_and_the_resolver_list_what_package_files_lists() {
        let root = std::env::temp_dir().join(format!("almide-one-enumeration-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let pkg = root.join("pkg");
        let outside = root.join("outside");
        for (path, body) in [
            ("pkg/almide.toml", "[package]\nname = \"pkg\"\n"),
            ("pkg/src/mod.almd", "fn a() -> Int = 1\n"),
            ("pkg/src/native/n.almd", "fn n() -> Int = 1\n"),
            ("pkg/src/.hidden/h.almd", "fn h() -> Int = 1\n"),
            ("pkg/src/target/deep/t.almd", "fn t() -> Int = 1\n"),
            ("pkg/src/notes.txt", "x"),
            ("outside/evil.almd", "fn evil() -> Int = 1\n"),
        ] {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, pkg.join("src/linkdir")).unwrap();
            std::os::unix::fs::symlink(outside.join("evil.almd"), pkg.join("src/linked.almd")).unwrap();
        }
        let src = pkg.join("src");
        let mut scanned = Vec::new();
        super::almd_files(&src, &mut scanned);
        scanned.sort();
        let listed: Vec<std::path::PathBuf> = crate::package_tree::package_files(&pkg, crate::package_tree::Links::Skip)
            .unwrap()
            .into_iter()
            .filter(|(rel, _)| rel.starts_with("src/") && rel.ends_with(".almd"))
            .map(|(_, p)| p)
            .collect();
        let mut listed = listed;
        listed.sort();
        assert_eq!(scanned, listed);
        assert_eq!(scanned.len(), 4, "{scanned:?}");
        assert!(!scanned.iter().any(|p| p.to_string_lossy().contains("link")), "a link was followed: {scanned:?}");
        // In a dependency's cache entry the same link is refused outright.
        #[cfg(unix)]
        assert!(crate::package_tree::package_files(&pkg, crate::package_tree::Links::Refuse).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
