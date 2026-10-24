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
}

impl NativeCode {
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.assets == 0 && self.crates.is_empty() && self.externs.is_empty()
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
        for (rel, is_module) in native_files(&native_dir, Path::new("")) {
            if is_module {
                code.modules.push(format!("native/{}", rel.display()));
            } else {
                code.assets += 1;
            }
        }
    }
    let toml = package_dir.join("almide.toml");
    if let Some(proj) = toml.exists().then(|| crate::project::parse_toml(&toml).ok()).flatten() {
        for nd in proj.native_deps {
            if !code.crates.contains(&nd.name) {
                code.crates.push(nd.name);
            }
        }
    }
    let mut files = Vec::new();
    almd_files(source_dir, &mut files);
    files.sort();
    for file in files {
        let rel = file.strip_prefix(package_dir).unwrap_or(&file).display().to_string();
        code.externs.extend(native_externs(&file).into_iter().map(|(name, line)| format!("`{name}` ({rel}:{line})")));
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

fn almd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" && name != "native" {
                almd_files(&path, out);
            }
        } else if name.ends_with(".almd") {
            out.push(path);
        }
    }
}

/// `(fn name, line)` of every top-level fn in `file` bound natively — an
/// `@extern` attribute whose target the native build binds (`rust`/`rs`,
/// `c`; the same predicate the checker and codegen use). A file that does
/// not parse contributes nothing here; the build reports it.
fn native_externs(file: &Path) -> Vec<(String, usize)> {
    let Ok(source) = std::fs::read_to_string(file) else { return Vec::new() };
    let tokens = crate::lexer::Lexer::tokenize(&source);
    let Ok(program) = crate::parser::Parser::new(tokens).parse() else { return Vec::new() };
    program
        .decls
        .iter()
        .filter_map(|d| match d {
            crate::ast::Decl::Fn { name, extern_attrs, span, .. }
                if extern_attrs.iter().any(|a| crate::types::extern_abi::native_extern_kind(a.target.as_str()).is_some()) =>
            {
                Some((name.to_string(), span.map_or(0, |s| s.line)))
            }
            _ => None,
        })
        .collect()
}

/// Every fetched dependency of `proj` with the native code it brings.
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
            direct: proj.dependencies.iter().any(|d| d.name == fd.pkg_id.name),
            code: native_code_of(&fd.package_dir, &fd.source_dir),
            granted: proj.native_grants.iter().any(|g| *g == fd.pkg_id.name),
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
