//! What a native build copies into the generated crate (`CrateInputs`) and
//! what outside it shapes the binary (`build_environment_key`) — the two
//! halves of the native build cache key (#3091).

use super::cargo_toml::insert_cargo_dep;

/// Everything a build copies INTO the generated crate besides the generated
/// source itself: the `native/` tree of the building package and of every
/// dependency package reached through `almide.toml`, plus those dependency
/// packages' `[native-deps]` (#3091).
///
/// It is collected ONCE, before the native build cache is consulted, and the
/// same value is both hashed into the cache key ([`CrateInputs::cache_key`])
/// and written into the crate ([`CrateInputs::apply`]). The key therefore
/// covers exactly the bytes the build ships, by construction: there is no
/// second list of "things that shape the binary" to keep in step with the
/// copy. That second list is what #3091 was — the key hashed the building
/// package's `native/` (#887) while the copy also pulled in every
/// dependency's, so editing only a dependency's native module was a cache hit
/// that shipped the previous binary.
#[derive(Default, Debug)]
pub(super) struct CrateInputs {
    /// One entry per package, in injection order (the building package
    /// first, then its dependencies depth-first).
    packages: Vec<PackageNatives>,
    /// The `[native-deps]` of dependency packages, in visit order. (The
    /// building package's own are passed separately as `native_deps`.)
    dep_native_deps: Vec<crate::project::NativeDep>,
}

#[derive(Default, Debug)]
struct PackageNatives {
    /// `(path under the crate's src/, contents)`, sorted by path.
    files: Vec<(std::path::PathBuf, Vec<u8>)>,
    /// The `native/*.rs` stems that get a `mod <stem>;`, sorted.
    mod_stems: Vec<String>,
    /// The package's `[package].name`, when it has `native/*.rs` modules:
    /// whose items get their pre-#3338 aliases (#3425).
    name: Option<String>,
}

impl CrateInputs {
    /// Read the `native/` trees and dependency `[native-deps]` reachable from
    /// `source_root`. Nothing is written.
    ///
    /// The dependency packages are the ones module resolution selected:
    /// `project_fetch::fetch_all_deps`, the function the compiler resolves
    /// `import`s through, honouring `almide.lock` and MVS. Each package's
    /// `native/` is read from the same checkout its `.almd` sources came
    /// from (#3094). This used to be a second walk that re-fetched each
    /// manifest entry WITHOUT the lock, so a locked branch dependency built
    /// the branch head's natives against the locked commit's `@extern`s, and
    /// an MVS-raised package copied the natives of both versions.
    pub(super) fn collect(source_root: Option<&std::path::Path>) -> Result<Self, String> {
        let mut inputs = CrateInputs::default();
        let Some(root) = source_root else { return Ok(inputs) };
        inputs.packages.push(PackageNatives::read(root)?);
        let toml_path = root.join("almide.toml");
        if !toml_path.exists() { return Ok(inputs); }
        let proj = crate::project::parse_toml(&toml_path).map_err(|e| format!("parse almide.toml: {}", e))?;
        if proj.dependencies.is_empty() { return Ok(inputs); }
        for dep in crate::project_fetch::fetch_all_deps(&proj)? {
            inputs.packages.push(PackageNatives::read(&dep.package_dir)?);
            let dep_toml = dep.package_dir.join("almide.toml");
            if let Some(dep_proj) = dep_toml.exists().then(|| crate::project::parse_toml(&dep_toml).ok()).flatten() {
                inputs.dep_native_deps.extend(dep_proj.native_deps);
            }
        }
        Ok(inputs)
    }

    /// The cache-key component for these inputs: every file's crate path and
    /// content digest, every `mod` declaration, every dependency native dep.
    /// Empty when there is nothing to inject.
    pub(super) fn cache_key(&self) -> String {
        let mut acc = String::new();
        for (i, pkg) in self.packages.iter().enumerate() {
            acc.push_str(&format!("pkg{}[", i));
            for (path, bytes) in &pkg.files {
                acc.push_str(&format!("{}:{:016x};", path.display(), super::hash64(bytes)));
            }
            acc.push_str(&format!("mods={}", pkg.mod_stems.join(",")));
            if let Some(name) = &pkg.name {
                acc.push_str(&format!(";name={}", name));
            }
            acc.push(']');
        }
        for nd in &self.dep_native_deps {
            acc.push_str(&format!("dep:{}={}@{};", nd.name, nd.spec, nd.target.as_deref().unwrap_or("")));
        }
        acc
    }

    /// The pre-#3338 aliases of the items of every package whose `native/`
    /// modules this crate carries, resolved against `code` (#3425).
    fn legacy_aliases(&self, code: &str) -> super::native_legacy_aliases::LegacyAliases {
        let pkgs: Vec<String> = self.packages.iter().filter_map(|p| p.name.clone()).collect();
        if pkgs.is_empty() {
            return Default::default();
        }
        super::native_legacy_aliases::legacy_aliases(code, &pkgs)
    }

    /// Warn, once per name, for every pre-#3338 spelling a package's
    /// `native/*.rs` names (#3425). Called before the build cache is
    /// consulted, so a cache hit warns too.
    pub(super) fn warn_legacy_callbacks(&self, code: &str) {
        if crate::warnings_suppressed() || self.packages.iter().all(|p| p.name.is_none()) {
            return;
        }
        let aliases = self.legacy_aliases(code);
        for pkg in &self.packages {
            let Some(name) = &pkg.name else { continue };
            for (rel, bytes) in pkg.files.iter().filter(|(rel, _)| rel.extension().is_some_and(|e| e == "rs")) {
                let file = format!("native/{} of package `{}`", rel.display(), name);
                let text = String::from_utf8_lossy(bytes);
                for w in super::native_legacy_aliases::warnings(&file, &text, &aliases) {
                    crate::err(&w);
                }
            }
        }
    }

    /// Write the collected files into `src_dir`, declare their modules in
    /// `code`, and append the dependency native deps to `project_dir`'s
    /// Cargo.toml.
    pub(super) fn apply(&self, code: &mut String, src_dir: &std::path::Path, project_dir: &std::path::Path) -> Result<(), String> {
        for pkg in &self.packages {
            pkg.apply(code, src_dir)?;
        }
        // The old spellings the packages' native code may still call (#3425).
        // Nothing is added to a crate without a package `native/` module.
        code.push_str(&super::native_legacy_aliases::render(&self.legacy_aliases(code)));
        if !self.dep_native_deps.is_empty() {
            let cargo_path = project_dir.join("Cargo.toml");
            let mut cargo = std::fs::read_to_string(&cargo_path).unwrap_or_default();
            for nd in &self.dep_native_deps {
                insert_cargo_dep(&mut cargo, nd);
            }
            let _ = std::fs::write(&cargo_path, &cargo);
        }
        Ok(())
    }
}

impl PackageNatives {
    /// `<root>/native/*.rs` become modules; subdirectories (assets such as
    /// `native/wgsl/*.wgsl`, `include_str!`d by the modules) travel with them
    /// whole. Other plain files in `native/` are not copied.
    fn read(root: &std::path::Path) -> Result<Self, String> {
        let mut pkg = PackageNatives::default();
        let native_dir = root.join("native");
        if !native_dir.is_dir() { return Ok(pkg); }
        for entry in sorted_entries(&native_dir)? {
            let path = native_dir.join(&entry);
            if path.extension().is_some_and(|e| e == "rs") && path.is_file() {
                let stem = path.file_stem()
                    .ok_or_else(|| format!("native module path has no file stem: {}", path.display()))?
                    .to_string_lossy().to_string();
                let content = std::fs::read(&path)
                    .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
                pkg.files.push((std::path::PathBuf::from(&entry), content));
                pkg.mod_stems.push(stem);
            } else if path.is_dir() {
                read_tree(&path, std::path::Path::new(&entry), &mut pkg.files)?;
            }
        }
        if !pkg.mod_stems.is_empty() {
            pkg.name = crate::project::parse_toml(&root.join("almide.toml")).ok()
                .map(|p| p.package.name)
                .filter(|n| !n.is_empty());
        }
        Ok(pkg)
    }

    fn apply(&self, code: &mut String, src_dir: &std::path::Path) -> Result<(), String> {
        for (rel, bytes) in &self.files {
            let dst = src_dir.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
            }
            std::fs::write(&dst, bytes).map_err(|e| format!("failed to write {}: {}", dst.display(), e))?;
        }
        if self.mod_stems.is_empty() { return Ok(()); }
        let mod_decls: String = self.mod_stems.iter().map(|s| format!("mod {};\n", s)).collect();
        if let Some(pos) = code.find("\nuse ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else if let Some(pos) = code.find("\nfn ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else {
            *code = format!("{}\n{}", mod_decls, code);
        }
        Ok(())
    }
}

/// The entry names of `dir`, sorted, so the collected inputs (and the key
/// hashed from them) do not depend on the filesystem's listing order.
fn sorted_entries(dir: &std::path::Path) -> Result<Vec<std::ffi::OsString>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("failed to read {}: {}", dir.display(), e))?;
    let mut names: Vec<_> = entries.flatten().map(|e| e.file_name()).collect();
    names.sort();
    Ok(names)
}

/// Every file under `dir`, recorded at `rel/<path below dir>`.
fn read_tree(dir: &std::path::Path, rel: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) -> Result<(), String> {
    for name in sorted_entries(dir)? {
        let src = dir.join(&name);
        let dst = rel.join(&name);
        if src.is_dir() {
            read_tree(&src, &dst, out)?;
        } else {
            let bytes = std::fs::read(&src).map_err(|e| format!("failed to read {}: {}", src.display(), e))?;
            out.push((dst, bytes));
        }
    }
    Ok(())
}

/// Everything OUTSIDE the generated crate that shapes the binary cargo or
/// rustc produces from it — the cache-key component for the build's
/// environment, next to [`CrateInputs::cache_key`] for its contents (#3091).
///
/// - **The recipe**: this file, `cargo_build.rs`, `cargo_toml.rs`,
///   `cargo_ice.rs` and `native_target.rs` as compiled into this
///   almide — the Cargo.toml templates (opt-level is load-bearing for
///   correctness, see [`GENERATED_CARGO_TOML`]), the rustc/cargo invocations
///   of every build path, the injection. The sources themselves are hashed,
///   so an edit to any of them is a new key without anyone remembering to
///   bump a revision.
/// - **The toolchain**: `rustc -vV` (version, commit, host). A toolchain
///   update that changes codegen is a new binary.
/// - **Flags cargo and rustc read from the environment**: `RUSTFLAGS` and its
///   spellings, `RUSTC`/`RUSTC_WRAPPER`, `CARGO_PROFILE_*` (which override the
///   templates' profiles), per-target rustflags/linker, and
///   `ALMIDE_NO_RTLIB` (which picks the build path).
/// - **Cargo config files** cargo would read for a build in `project_dir`:
///   `.cargo/config{,.toml}` in it and every ancestor, and in `CARGO_HOME`.
pub(super) fn build_environment_key(project_dir: &std::path::Path) -> String {
    let recipe = super::hash64(
        concat!(
            include_str!("cargo_build.rs"),
            include_str!("cargo_toml.rs"),
            include_str!("crate_inputs.rs"),
            include_str!("cargo_ice.rs"),
            include_str!("native_target.rs"),
            include_str!("native_legacy_aliases.rs"),
        )
        .as_bytes(),
    );
    let mut acc = format!("recipe={:016x};rustc={};", recipe, toolchain_identity());
    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| env_shapes_the_binary(k))
        .collect();
    vars.sort();
    for (k, v) in vars {
        acc.push_str(&format!("{}={};", k, v));
    }
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")));
    let config_dirs = project_dir.ancestors().map(|d| d.join(".cargo")).chain(cargo_home);
    for dir in config_dirs {
        for name in ["config", "config.toml"] {
            let path = dir.join(name);
            if let Ok(bytes) = std::fs::read(&path) {
                acc.push_str(&format!("{}:{:016x};", path.display(), super::hash64(&bytes)));
            }
        }
    }
    acc
}

/// Is `name` an environment variable that changes what cargo/rustc emit?
fn env_shapes_the_binary(name: &str) -> bool {
    matches!(
        name,
        "RUSTFLAGS" | "CARGO_ENCODED_RUSTFLAGS" | "CARGO_BUILD_RUSTFLAGS" | "RUSTC" | "RUSTC_WRAPPER"
            | "CARGO_BUILD_RUSTC" | "CARGO_BUILD_RUSTC_WRAPPER" | "ALMIDE_NO_RTLIB"
    ) || name.starts_with("CARGO_PROFILE_")
        || (name.starts_with("CARGO_TARGET_") && (name.ends_with("_RUSTFLAGS") || name.ends_with("_LINKER")))
}

/// `rustc -vV` of the rustc on PATH, once per process. Empty when it cannot
/// be run (the build then fails on its own).
pub(super) fn toolchain_identity() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        std::process::Command::new(crate::find_rustc())
            .arg("-vV")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().replace('\n', "|"))
            .unwrap_or_default()
    })
}
