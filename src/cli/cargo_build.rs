/// Cargo/rustc build orchestration for generated Rust code: Cargo.toml
/// templates, native-deps/native-module injection, the rlib fast paths, and
/// the `cargo build` / `cargo test --no-run` drivers. Split out of `mod.rs`
/// (which had grown past the max-lines threshold) — a pure text move, no
/// behavior change. `cargo_build_cdylib`, `cargo_build_generated`,
/// `cargo_build_generated_with_native` and `cargo_build_test_with_native`
/// are `pub(super)` because `build.rs`/`repl.rs`/`run.rs` (siblings under
/// `cli`) call them via `super::cargo_build_*`; everything else here is
/// used only within this file.

/// Cargo.toml template for generated Rust projects (without HTTP/TLS).
const GENERATED_CARGO_TOML: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

# Self-isolate from any ENCLOSING cargo workspace: without this, running almide
# with a project dir nested inside a Rust workspace (a repo's tools/ tree, the
# fuzzer's .scratch) makes cargo resolve the parent workspace and refuse the
# build ("current package believes it's in a workspace when it's not").
[workspace]

# `opt-level = 1` is LOAD-BEARING FOR CORRECTNESS, not a speed choice. Do not lower it.
#
# It was lowered to 0 once, for a real and large win: the cargo phase of `almide run` on a
# 2,103-line program is 3,215ms at level 1 and 724ms at level 0 (4.4x), measured with a real
# source edit each time and a phase trace inside the pipeline. It was reverted the same day
# because `spec/wasm_cross/mutual_tail_recursion.almd` began overflowing the native stack:
# **MUTUAL tail recursion is turned into a loop by LLVM's tail-call optimisation, which does
# not run at opt-level 0.** Wasm is unaffected (it has `return_call`), so the two targets
# diverged — a cross-target contract broken by a Cargo setting.
#
# What made the mistake possible: the pre-change check measured 200,000-deep SELF-recursion,
# which Almide's own TCO already turns into a loop, so it passed at both levels and proved
# nothing about the mutual case. A native semantic property must not depend on an
# optimisation level; until the compiler eliminates mutual tail calls itself (#1043), this
# line is what keeps the contract.
[profile.dev]
opt-level = 1
overflow-checks = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

/// Does the generated code use a runtime module whose source needs a crate?
/// (The cdylib/bin/test fast paths that skip cargo ask exactly this.)
pub(super) fn needs_runtime_crates(rs_code: &str) -> bool {
    !almide_codegen::runtime_crate_deps(rs_code).is_empty()
}

/// Every crate dependency of a generated project, on EVERY build route (bin,
/// `--cdylib`, `--repr-c`, test, `almide run`, #3346): `[native-deps]` first
/// (the user's spelling of a crate wins), then the runtime's
/// (`almide_codegen::runtime_crate_deps`) minus any crate the user already
/// declared for every target — so declaring `flate2` yourself (the #3346
/// workaround) never writes a second `flate2` key. A user crate declared only
/// under `[target.'cfg(...)'.native-deps]` (#3350) does not stand in for the
/// runtime's: the runtime needs it on every target, so both are written, one
/// in `[dependencies]` and one in that target's table (Cargo merges them).
pub(super) fn generated_crate_deps(rs_code: &str, native_deps: &[crate::project::NativeDep]) -> Vec<crate::project::NativeDep> {
    let mut deps = native_deps.to_vec();
    for (name, spec) in almide_codegen::runtime_crate_deps(rs_code) {
        if !deps.iter().any(|d| d.name == name && d.target.is_none()) {
            deps.push(crate::project::NativeDep { name: name.into(), spec: spec.into(), target: None });
        }
    }
    deps
}

/// The Cargo.toml table a native dep is written under: `[dependencies]`, or
/// `[target.<key>.dependencies]` for a `[target.<key>.native-deps]` entry
/// (#3350). The key is quoted as a TOML literal string — a `cfg(...)` holds
/// `"` — or as a basic string when it also holds `'`.
fn dependency_table_header(target: Option<&str>) -> String {
    match target {
        None => "[dependencies]".to_string(),
        Some(t) if !t.contains('\'') => format!("[target.'{t}'.dependencies]"),
        Some(t) => format!("[target.\"{}\".dependencies]", t.replace('\\', "\\\\").replace('"', "\\\"")),
    }
}

/// The byte range of the table `header` opens in `toml` — from just after
/// the header line to the next table header (or the end) — or `None` when
/// `toml` has no such table.
fn table_body(toml: &str, header: &str) -> Option<std::ops::Range<usize>> {
    let mut offset = 0;
    let mut start = None;
    for line in toml.split_inclusive('\n') {
        let trimmed = line.trim();
        if start.is_some() && trimmed.starts_with('[') {
            return start.map(|s| s..offset);
        }
        offset += line.len();
        if trimmed == header {
            start = Some(offset);
        }
    }
    start.map(|s| s..toml.len())
}

/// Does `body` (a table's lines) assign `name`?
fn assigns(body: &str, name: &str) -> bool {
    body.lines().any(|l| {
        l.trim_start().strip_prefix(name).is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// Write `dep` into its table of `toml` ([`dependency_table_header`]): at the
/// end of the table, so deps keep the order they are given in; creating the
/// table at the end of the file when absent. A crate the table already
/// declares is left as it is — the base template may carry it (e.g. rayon in
/// the ML profile), and a second key is a Cargo hard error (#646).
fn insert_cargo_dep(toml: &mut String, dep: &crate::project::NativeDep) {
    let header = dependency_table_header(dep.target.as_deref());
    let line = if dep.spec.starts_with('{') {
        format!("{} = {}\n", dep.name, dep.spec)
    } else {
        format!("{} = \"{}\"\n", dep.name, dep.spec)
    };
    if !toml.ends_with('\n') {
        toml.push('\n');
    }
    match table_body(toml, &header) {
        Some(body) if assigns(&toml[body.clone()], &dep.name) => {}
        Some(body) => {
            // After the table's last entry, before the blank lines that
            // separate it from the next table.
            let last = body.start + toml[body.clone()].trim_end().len();
            let at = if last == body.start {
                body.start
            } else {
                toml[last..].find('\n').map_or(toml.len(), |i| last + i + 1)
            };
            toml.insert_str(at, &line);
        }
        None => toml.push_str(&format!("\n{header}\n{line}")),
    }
}

/// `--cfg almide_par` enables the rayon-backed parallel runtime paths. The cfg
/// follows the DEPENDENCY: inject it only when the generated project's Cargo.toml
/// declares rayon (e.g. via `[native-deps]` — the nn repos do) — the base template
/// carries no external crates (#739), so an unconditional cfg would make ANY
/// matrix-using program fail to resolve `rayon::prelude` (E0433). Without the cfg
/// the runtime compiles its serial side, exactly like the raw-rustc test harness.
fn inject_almide_par_if_rayon(cmd: &mut std::process::Command, project_dir: &std::path::Path) {
    // Only an unconditional rayon counts: one under a `[target.…]` table
    // (#3350) is absent on other targets, where the cfg would break the build.
    let has_rayon = std::fs::read_to_string(project_dir.join("Cargo.toml"))
        .map(|t| table_body(&t, "[dependencies]").is_some_and(|body| assigns(&t[body], "rayon")))
        .unwrap_or(false);
    if has_rayon {
        cmd.env(
            "RUSTFLAGS",
            format!("{} --cfg almide_par", std::env::var("RUSTFLAGS").unwrap_or_default()),
        );
    }
}

/// Build a Cargo.toml string by writing each native dep into its table:
/// `[dependencies]`, or `[target.<key>.dependencies]` for a target-specific
/// one (#3350). See [`insert_cargo_dep`].
fn build_cargo_toml(base_toml: &str, native_deps: &[crate::project::NativeDep]) -> String {
    let mut toml = base_toml.to_string();
    for dep in native_deps {
        insert_cargo_dep(&mut toml, dep);
    }
    toml
}

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
            acc.push_str(&format!("mods={}]", pkg.mod_stems.join(",")));
        }
        for nd in &self.dep_native_deps {
            acc.push_str(&format!("dep:{}={}@{};", nd.name, nd.spec, nd.target.as_deref().unwrap_or("")));
        }
        acc
    }

    /// Write the collected files into `src_dir`, declare their modules in
    /// `code`, and append the dependency native deps to `project_dir`'s
    /// Cargo.toml.
    fn apply(&self, code: &mut String, src_dir: &std::path::Path, project_dir: &std::path::Path) -> Result<(), String> {
        for pkg in &self.packages {
            pkg.apply(code, src_dir)?;
        }
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
/// - **The recipe**: this file and `native_target.rs` as compiled into this
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
        concat!(include_str!("cargo_build.rs"), include_str!("native_target.rs")).as_bytes(),
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
fn toolchain_identity() -> &'static str {
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


/// Build generated Rust code as a cdylib shared library (.dylib/.so).
pub(super) fn cargo_build_cdylib(rs_code: &str, project_dir: &std::path::Path, lib_name: &str, release: bool, native_deps: &[crate::project::NativeDep], source_root: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    // Base manifest for a cdylib; `build_cargo_toml` folds in `[native-deps]` and
    // `CrateInputs::apply` appends any dependency-package native deps below — so a
    // cdylib wires native crates exactly like the bin path (#719). Previously this
    // wrote a dep-free manifest and `rs_code` verbatim, so `@extern(rust, …)`
    // modules were undeclared (E0433) and `[native-deps]` never reached cargo.
    let cdylib_base = format!(r#"[package]
name = "almide-cdylib"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
name = "{}"
crate-type = ["cdylib"]
path = "src/lib.rs"

# `opt-level = 1` is LOAD-BEARING FOR CORRECTNESS, not a speed choice. Do not lower it.
#
# It was lowered to 0 once, for a real and large win: the cargo phase of `almide run` on a
# 2,103-line program is 3,215ms at level 1 and 724ms at level 0 (4.4x), measured with a real
# source edit each time and a phase trace inside the pipeline. It was reverted the same day
# because `spec/wasm_cross/mutual_tail_recursion.almd` began overflowing the native stack:
# **MUTUAL tail recursion is turned into a loop by LLVM's tail-call optimisation, which does
# not run at opt-level 0.** Wasm is unaffected (it has `return_call`), so the two targets
# diverged — a cross-target contract broken by a Cargo setting.
#
# What made the mistake possible: the pre-change check measured 200,000-deep SELF-recursion,
# which Almide's own TCO already turns into a loop, so it passed at both levels and proved
# nothing about the mutual case. A native semantic property must not depend on an
# optimisation level; until the compiler eliminates mutual tail calls itself (#1043), this
# line is what keeps the contract.
[profile.dev]
opt-level = 1
overflow-checks = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#, lib_name.replace('-', "_"));
    let cargo_toml = build_cargo_toml(&cdylib_base, &generated_crate_deps(rs_code, native_deps));
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    // Copy `native/*.rs` shims into src/ + inject `mod <stem>;`, then pull native
    // modules + `[native-deps]` from dependency packages — same wiring as
    // `cargo_build_generated_with_native`.
    let mut lib_code = rs_code.to_string();
    CrateInputs::collect(source_root)?.apply(&mut lib_code, &src_dir, project_dir)?;
    std::fs::write(src_dir.join("lib.rs"), &lib_code)
        .map_err(|e| format!("failed to write lib.rs: {}", e))?;

    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("build").current_dir(project_dir).arg("--quiet");
    // The library is located under `project_dir/target` below; an inherited
    // `CARGO_TARGET_DIR` would send it elsewhere and turn the build into
    // "expected library not found" (the bin path's twin of #2230's fix).
    cmd.arg("--target-dir").arg(project_dir.join("target"));
    if release { cmd.arg("--release"); }
    // #2772: the requested target (or the host) is stated, never inherited.
    let triple = super::native_target::cross_target();
    super::native_target::pin_cargo_target(&mut cmd, triple.as_deref());
    let profile = if release { "release" } else { "debug" };
    let lib_filename = super::native_target::cdylib_file_name(&lib_name.replace('-', "_"), triple.as_deref());
    let lib_path = super::native_target::artifact_dir(project_dir, triple.as_deref(), profile).join(&lib_filename);
    // A library left by an earlier build must not stand in for this one.
    let _ = std::fs::remove_file(&lib_path);
    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(triple.as_deref()
            .and_then(|t| super::native_target::cross_toolchain_error(&stderr, t))
            .unwrap_or(stderr));
    }
    if !lib_path.exists() {
        return Err(format!("expected library not found at {}", lib_path.display()));
    }

    // Copy to current directory
    let dest = std::path::Path::new(".").join(&lib_filename);
    std::fs::copy(&lib_path, &dest)
        .map_err(|e| format!("failed to copy library: {}", e))?;
    Ok(dest)
}

/// Does the generated crate DEFINE an entry point?
///
/// THE ONE PLACE THIS QUESTION IS ANSWERED (#2370). It used to be asked three
/// times in three spellings: twice here as `code.contains("fn main(")` (the
/// auto-`main` guards) and once in `run.rs` as `code.contains("\nfn main(")`
/// plus a `"\npub fn main("` arm the copies here did not have. Three answers to
/// one question is how they drift, and they had: the unanchored pair counted a
/// `fn main(` inside a STRING LITERAL. A library package embedding Almide source
/// as test data — a tool that processes source is the natural case — matched its
/// own fixture text, so no `fn main() {}` was appended and rustc's `E0601: main
/// function not found` reached the author under "codegen produced invalid Rust —
/// this is an Almide bug".
///
/// Anchoring is what separates a definition from a mention: a crate-root item
/// begins its line, and inside a literal the newlines are `\n` escapes, so the
/// fixture is one physical line starting with something else. The `pub` arms are
/// load-bearing once anchored — `contains` matched them by accident, as a
/// substring of `pub fn main(`.
///
/// What this still is NOT: a parse. It is a narrowed convention, not the
/// mechanism. The mechanism is the IR — `build.rs` asks
/// `ir_program.functions.iter().any(|f| f.name.as_str() == "main")` — and moving
/// this to that would mean threading the fact from codegen down to the two
/// consumers here — tracked as #2372, so the narrowing has an owner rather than
/// living only in this comment. Both ways the old spellings were wrong are gated
/// in this file's `mod tests`, and both fail against the unanchored predicate.
pub(super) fn defines_entry_point(code: &str) -> bool {
    code.lines().any(|l| {
        l.starts_with("fn main(")
            || l.starts_with("pub fn main(")
            || l.starts_with("fn almide_main(")
            || l.starts_with("pub fn almide_main(")
    })
}

/// Build generated Rust code using cargo.
/// Returns the path to the built binary on success.
pub(super) fn cargo_build_generated(rs_code: &str, project_dir: &std::path::Path, release: bool) -> Result<std::path::PathBuf, String> {
    cargo_build_generated_with_native(rs_code, project_dir, release, &[], None, &CrateInputs::default())
}

/// Build generated Rust code with optional native Rust dependencies and source files.
/// `cargo_build_generated_with_native`'s dependency-free rlib fast path:
/// link the precompiled runtime instead of recompiling its ~2000 lines per
/// build. The runtime is compiled at the same opt-level as the binary (3 for
/// `--release`, 1 for debug/`almide run`), so it is fully optimized; only
/// its compilation is amortized. Net: shipping builds drop from ~27-33s to
/// ~1-2s (~20x).
///
/// Tradeoff: because the runtime is a separate crate (no LTO), non-generic
/// non-#[inline] runtime fns (e.g. string.trim/split) aren't inlined across
/// the crate boundary, costing up to ~10% runtime on string/list-heavy hot
/// loops (typically 2-5%). ThinLTO would recover it but erases the build win
/// (it re-optimizes the runtime at link time). The planned fix is #[inline]
/// on the hot runtime fns — see docs/roadmap. `ALMIDE_NO_RTLIB=1` forces the
/// monolithic cargo build (full cross-crate inlining) when a shipped binary
/// must squeeze out that last few percent (checked by the caller, before
/// this is even invoked).
///
/// Returns `None` on ANY failure (a `?`-propagated missing rlib/slim-main,
/// a write failure, or a nonzero rustc exit) — the caller falls through to
/// the cargo-based slow path unconditionally, so correctness never
/// regresses, only the speedup is forfeited. Extracted verbatim (the
/// original's `if let (Ok(_), Some(_))` tuple-match + nested `if`s become
/// this function's `?` chain — same "either piece missing → skip" semantics).
fn try_rlib_fast_build(rs_code: &str, project_dir: &std::path::Path, release: bool) -> Option<std::path::PathBuf> {
    let opt_level = if release { "3" } else { "1" };
    let rlib = ensure_runtime_rlib(opt_level).ok()?;
    let mut slim = crate::codegen::slim_main_with_external_runtime(rs_code)?;
    if !defines_entry_point(&slim) {
        slim.push_str("\nfn main() {}\n");
    }
    let rlib_dir = rlib.parent().unwrap_or_else(|| std::path::Path::new("."));
    let rs_path = project_dir.join("almide_gen_main.rs");
    let bin_path = project_dir.join(if cfg!(windows) { "almide-out.exe" } else { "almide-out" });
    std::fs::write(&rs_path, &slim).ok()?;
    let mut cmd = std::process::Command::new(crate::find_rustc());
    cmd.arg(&rs_path)
        .arg("-o").arg(&bin_path)
        .arg("--edition").arg("2021")
        .arg("-C").arg(format!("opt-level={opt_level}"))
        .arg("--extern").arg(format!("almide_rt={}", rlib.display()))
        .arg("-L").arg(rlib_dir)
        .arg("-A").arg("warnings");
    let output = cmd.output().ok()?;
    if output.status.success() {
        Some(bin_path)
    } else {
        // else: fall through to the self-contained cargo build
        None
    }
}

/// Write the generated Cargo.toml + `src/main.rs` for a cargo-based build:
/// creates `src/`, writes the manifest with every crate the program needs
/// ([`generated_crate_deps`], shared with the cdylib route), injects native
/// modules (`inputs`: the package's and its dependencies'), auto-generates
/// an empty `fn main()` for library-only code, and writes both files.
/// Matrix programs need NO extra deps: the flat AlmideMatrix runtime + the
/// embedded almide-kernel SIMD modules are pure Rust (the burn/BLAS splice
/// retired with the 0.28 flat-matrix runtime — its Vec<Vec<f64>> marker no
/// longer existed anywhere except inside the embedded kernel bridge, where
/// the splicer misfired, #739). Extracted verbatim — shared by
/// `cargo_build_generated_with_native` and `cargo_build_test_with_native`,
/// which had identical copies of this setup.
fn write_generated_cargo_project(
    rs_code: &str,
    project_dir: &std::path::Path,
    native_deps: &[crate::project::NativeDep],
    inputs: &CrateInputs,
) -> Result<std::path::PathBuf, String> {
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;

    let cargo_toml = build_cargo_toml(GENERATED_CARGO_TOML, &generated_crate_deps(rs_code, native_deps));
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    let mut final_code = rs_code.to_string();

    // The package's and its dependencies' native modules + dependency
    // native-deps — exactly the inputs the cache key was computed from.
    inputs.apply(&mut final_code, &src_dir, project_dir)?;

    // Library modules may not define main — auto-generate an empty one
    if !defines_entry_point(&final_code) {
        final_code.push_str("\nfn main() {}\n");
    }

    std::fs::write(src_dir.join("main.rs"), &final_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    Ok(src_dir)
}

/// Run `cargo build` in `project_dir` and locate the resulting binary.
/// Extracted verbatim from `cargo_build_generated_with_native`'s tail.
fn run_cargo_build_and_locate_binary(project_dir: &std::path::Path, release: bool) -> Result<std::path::PathBuf, String> {
    let triple = super::native_target::cross_target();
    let profile = if release { "release" } else { "debug" };
    let exe = super::native_target::exe_file_name("almide-out", triple.as_deref());
    let bin_path = super::native_target::artifact_dir(project_dir, triple.as_deref(), profile).join(exe);
    // #2772: remove the previous build's binary first. Cargo re-links it into
    // place on every successful build (fresh or not), so this costs nothing —
    // and if anything ever sends the output elsewhere, the result is a loud
    // "expected binary not found" instead of the stale binary copied out as
    // though it were this build's.
    let _ = std::fs::remove_file(&bin_path);

    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("build").current_dir(project_dir);
    // The binary is located under `project_dir/target` below; an inherited
    // `CARGO_TARGET_DIR` (a common user setting, and what `cargo test` hands
    // the harness's own child processes) would send it elsewhere and turn the
    // build into "expected binary not found" (#2228 hit it first-hand).
    cmd.arg("--target-dir").arg(project_dir.join("target"));
    if release {
        cmd.arg("--release");
    }
    // Suppress cargo's chatty output
    cmd.arg("--quiet");
    // #2772: the target is stated (`--target` for a cross build) and an
    // inherited `CARGO_BUILD_TARGET` removed, so the binary lands at
    // `bin_path` and nowhere else.
    super::native_target::pin_cargo_target(&mut cmd, triple.as_deref());

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if let Some(e) = triple.as_deref().and_then(|t| super::native_target::cross_toolchain_error(&stderr, t)) {
            return Err(e);
        }
        return Err(wrap_codegen_leak(stderr));
    }

    if !bin_path.exists() {
        return Err(format!(
            "expected binary not found at {}\n  \
             hint: a cargo config `build.target` redirects the output; use `almide build --target <triple>` instead",
            bin_path.display()
        ));
    }
    Ok(bin_path)
}

pub(super) fn cargo_build_generated_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    release: bool,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
    inputs: &CrateInputs,
) -> Result<std::path::PathBuf, String> {
    let uses_matrix = rs_code.contains("almide_rt_matrix_");

    // The rlib fast path links a HOST-built runtime with a bare host rustc, so a
    // cross build (#2772) always takes the cargo path.
    if !almide_base::env::flag("ALMIDE_NO_RTLIB")
        && super::native_target::cross_target().is_none()
        && !uses_matrix && !needs_runtime_crates(rs_code)
        && native_deps.is_empty() && source_root.is_none()
    {
        if let Some(bin_path) = try_rlib_fast_build(rs_code, project_dir, release) {
            return Ok(bin_path);
        }
    }

    write_generated_cargo_project(rs_code, project_dir, native_deps, inputs)?;

    run_cargo_build_and_locate_binary(project_dir, release)
}

/// Scrub rustc output when our codegen produces invalid Rust — replaces
/// generated paths with placeholders and prepends a bug-report banner so
/// users (and harness classifiers) don't mistake a compiler bug for a
/// user-facing language error. No-op when the output is clean.
fn wrap_codegen_leak(stderr: String) -> String {
    let mentions_main_rs = stderr.contains("src/main.rs");
    let leaks_rustc_code = contains_rustc_error_code(&stderr);
    if !(mentions_main_rs || leaks_rustc_code) {
        return stderr;
    }
    let scrubbed = stderr
        .replace("src/main.rs", "<generated.rs>")
        .replace("almide-out", "almide-generated");
    format!(
        "codegen produced invalid Rust — this is an Almide bug.\n\
         Please file a minimal repro at https://github.com/almide/almide/issues\n\
         \n\
         --- rustc output (edited to hide generated paths) ---\n\
         {}",
        scrubbed
    )
}

/// Path to the precompiled `almide_rt` runtime rlib, built once per process.
///
/// The runtime (string/list/map/... ops, RcCow, AlmideConcat, the equality
/// macros) is identical across every compiled file, yet the inline-source model
/// makes rustc recompile all ~2000 lines of it for each one (~2.2s/file). The
/// rlib model — Rust's own — compiles it once into an `.rlib`; per-file rustc
/// then just links it (~0.4s/file).
///
/// Keyed by a hash of the runtime source + rustc version + opt profile, so a
/// compiler upgrade or a runtime edit transparently rebuilds. Cross-process
/// builders serialize on a per-dir advisory lock; a warm cache is a stat.
///
/// `opt_level` selects the optimization the runtime is compiled at: "1" for
/// tests/debug (where runtime speed is irrelevant), "3" for release shipping
/// (so the linked runtime keeps full optimization). Each level is cached
/// separately and built at most once per process.
fn ensure_runtime_rlib(opt_level: &str) -> Result<std::path::PathBuf, String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, Result<std::path::PathBuf, String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(r) = guard.get(opt_level) {
            return r.clone();
        }
    }
    let result = build_runtime_rlib(opt_level);
    if let Ok(rlib) = &result {
        // This rlib is in use NOW: refresh its mtime (nothing else does — a
        // warm cache is a bare `exists()`), then let the daily sweep empty
        // the rlib dirs of runtimes and toolchains nobody has linked for a
        // week (#2504). The touch comes first, so the sweep can never evict
        // the dir this process is about to link against.
        super::run::touch_used(rlib);
        if let Some(dir) = rlib.parent() {
            super::run::sweep_rtlib_cache(dir);
        }
    }
    cache.lock().unwrap().insert(opt_level.to_string(), result.clone());
    result
}

fn build_runtime_rlib(opt_level: &str) -> Result<std::path::PathBuf, String> {
    let src = crate::codegen::emit_runtime_crate();
    let rustc = crate::find_rustc();
    // `-vV`, not `--version`: two toolchains of one version for different
    // hosts (x86_64 under Rosetta and arm64) must not share an rlib.
    let rustc_ver = toolchain_identity();
    // The profile string here MUST match the per-file rustc invocation that links it.
    let key = format!("{:016x}", super::hash64(format!("{src}|{rustc_ver}|opt{opt_level}|ed2021").as_bytes()));
    let dir = std::env::temp_dir().join(format!("almide-rtlib-{key}"));
    let rlib = dir.join("libalmide_rt.rlib");
    if rlib.exists() {
        return Ok(rlib);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("rtlib dir: {e}"))?;
    let _lock = super::run::BuildDirLock::acquire(&dir)?;
    if rlib.exists() {
        return Ok(rlib); // another builder won the race while we waited
    }
    let src_path = dir.join("almide_rt.rs");
    std::fs::write(&src_path, &src).map_err(|e| format!("rtlib src: {e}"))?;
    let output = std::process::Command::new(&rustc)
        .arg(&src_path)
        .arg("--crate-type").arg("lib")
        .arg("--crate-name").arg("almide_rt")
        .arg("--edition").arg("2021")
        .arg("-C").arg(format!("opt-level={opt_level}"))
        .arg("-C").arg("overflow-checks=no")
        .arg("-A").arg("warnings")
        .arg("--out-dir").arg(&dir)
        .output()
        .map_err(|e| format!("rtlib rustc: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    if !rlib.exists() {
        return Err("rtlib build produced no .rlib".to_string());
    }
    Ok(rlib)
}

/// `cargo_build_test_with_native`'s dependency-free fast path: an rlib-linked
/// build first (falls through to the fully self-contained inline build on
/// any failure). Extracted verbatim — once entered, the original code
/// always returned before reaching the slow (cargo-based) path below, so
/// this is a genuine alternative branch, not a mid-body early exit.
fn cargo_build_test_fast_path(rs_code: &str, project_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let bin_path = project_dir.join("almide_test_bin");

    // rlib fast path: link the precompiled runtime instead of recompiling
    // its ~2000 lines per file (~2.2s → ~0.4s). Any failure (e.g. a runtime
    // vs. user type collision that only manifests cross-crate) falls through
    // to the inline path below, so this never regresses correctness — at
    // worst a file pays one extra rustc. Opt out with ALMIDE_NO_RTLIB=1.
    if !almide_base::env::flag("ALMIDE_NO_RTLIB") {
        if let (Ok(rlib), Some(mut slim)) = (
            ensure_runtime_rlib("1"),
            crate::codegen::slim_main_with_external_runtime(rs_code),
        ) {
            if !slim.contains("fn main(") && !slim.contains("fn almide_main(") {
                slim.push_str("\nfn main() {}\n");
            }
            let rlib_dir = rlib.parent().unwrap_or_else(|| std::path::Path::new("."));
            let rs_path = project_dir.join("almide_test_main.rs");
            if std::fs::write(&rs_path, &slim).is_ok() {
                // opt-level=0 for the slim main: the runtime (the part that
                // benefits from optimization) is already compiled into the
                // rlib at opt-level=1, and test user code is short-lived, so
                // optimizing it just burns compile time. opt0 + rlib is ~2.7x
                // over the inline opt1 path; opt1 + rlib only ~1.6x.
                let output = std::process::Command::new(crate::find_rustc())
                    .arg(&rs_path)
                    .arg("--test")
                    .arg("-o").arg(&bin_path)
                    .arg("--edition").arg("2021")
                    .arg("-C").arg("opt-level=0")
                    .arg("-C").arg("overflow-checks=no")
                    .arg("--extern").arg(format!("almide_rt={}", rlib.display()))
                    .arg("-L").arg(rlib_dir)
                    .arg("-A").arg("warnings")
                    .output();
                if let Ok(output) = output {
                    if output.status.success() {
                        return Ok(bin_path);
                    }
                    // else: fall through to the self-contained inline build
                }
            }
        }
    }

    let mut final_code = rs_code.to_string();
    if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
        final_code.push_str("\nfn main() {}\n");
    }
    let rs_path = project_dir.join("almide_test_main.rs");
    std::fs::write(&rs_path, &final_code)
        .map_err(|e| format!("failed to write {}: {}", rs_path.display(), e))?;
    let output = std::process::Command::new(crate::find_rustc())
        .arg(&rs_path)
        .arg("--test")
        .arg("-o").arg(&bin_path)
        .arg("--edition").arg("2021")
        .arg("-C").arg("opt-level=1")
        .arg("-C").arg("overflow-checks=no")
        .arg("-A").arg("warnings")
        .output()
        .map_err(|e| format!("failed to run rustc: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(bin_path)
}

/// `cargo_build_test_with_native`'s compiler-message renderer for the slow
/// (cargo-based) path: `--quiet` suppresses cargo's own error display, so
/// rustc messages must be read back out of the `--message-format=json`
/// stdout stream. Extracted verbatim — a pure formatting step over its
/// parameters.
fn render_cargo_json_errors(stdout: &str, stderr: &str, verbose: bool) -> String {
    let mut rendered: Vec<String> = Vec::new();
    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                let level = json.get("message")
                    .and_then(|m| m.get("level"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("");
                if level == "error" || verbose {
                    if let Some(msg) = json.get("message")
                        .and_then(|m| m.get("rendered"))
                        .and_then(|r| r.as_str())
                    {
                        rendered.push(msg.to_string());
                    }
                }
            }
        }
    }
    if rendered.is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", rendered.join("\n"), stderr)
    }
}

/// Run `cargo test --no-run` in `project_dir` and locate the resulting test
/// binary from its `--message-format=json` output. Extracted verbatim from
/// `cargo_build_test_with_native`'s tail.
fn run_cargo_test_no_run_and_locate_binary(project_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    // Use `cargo test --no-run` to build the test binary without running it
    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("test").arg("--no-run").arg("--quiet").arg("--message-format=json")
        .current_dir(project_dir);
    // A test binary must run on this machine: never inherit a cross target.
    super::native_target::pin_cargo_target(&mut cmd, None);

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        // --quiet suppresses cargo's own error display, but rustc messages
        // come through stdout as JSON (--message-format=json). Extract the
        // "rendered" field from each compiler-message so the user sees the
        // real error spans, not just "1 previous error; N warnings emitted".
        let verbose = almide_base::env::flag("ALMIDE_TEST_VERBOSE");
        let combined = render_cargo_json_errors(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
            verbose,
        );
        return Err(wrap_codegen_leak(combined));
    }

    // Parse the JSON output to find the test binary path
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-artifact") {
                if let Some(exe) = json.get("executable").and_then(|e| e.as_str()) {
                    return Ok(std::path::PathBuf::from(exe));
                }
            }
        }
    }

    Err("could not determine test binary path from cargo output".to_string())
}

pub(super) fn cargo_build_test_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
    inputs: &CrateInputs,
) -> Result<std::path::PathBuf, String> {
    // Fast path: the generated test crate is dependency-free (the runtime is
    // inlined as source). `cargo test --no-run` serializes concurrent builds on
    // cargo's global `~/.cargo/.package-cache` lock — even across separate
    // project dirs — so a parallel test run is effectively sequential. A bare
    // `rustc --test` has no such lock, so per-file builds run truly in parallel.
    if !needs_runtime_crates(rs_code) && native_deps.is_empty() && source_root.is_none() {
        return cargo_build_test_fast_path(rs_code, project_dir);
    }

    write_generated_cargo_project(rs_code, project_dir, native_deps, inputs)?;

    run_cargo_test_no_run_and_locate_binary(project_dir)
}

/// Does a failed build's output carry rustc's internal-compiler-error banner?
///
/// rustc prints `error: the compiler unexpectedly panicked. This is a bug.`
/// (older / query-path ICEs say `internal compiler error`) through its
/// diagnostic emitter, so the phrase survives cargo's `--message-format=json`
/// re-rendering and the `wrap_codegen_leak` banner alike. This is the ONLY
/// signal the stale-incremental-session recovery keys on (#2500): a build
/// that merely fails to compile never matches, so a genuine error is never
/// retried.
pub(super) fn is_rustc_ice(stderr: &str) -> bool {
    stderr.contains("the compiler unexpectedly panicked") || stderr.contains("internal compiler error")
}

/// Remove every NON-EMPTY `<project_dir>/target/<profile>/incremental`
/// session store. Returns the directories that held a session and were
/// removed. The CALLER holds the dir's `BuildDirLock`: a session store is
/// rewritten by any build in the dir, so it is only ever touched under the
/// same lock that serializes those builds.
///
/// Empty is not "cleared": cargo creates `target/<profile>/incremental/`
/// even when incremental compilation is OFF (`CARGO_INCREMENTAL=0`, which
/// this repo's own CI sets for every job). Counting that empty directory as
/// something recovered would make the caller retry a genuine ICE once for
/// nothing, in exactly the environment where no session can have gone stale.
pub(super) fn clear_incremental_sessions(project_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(profiles) = std::fs::read_dir(project_dir.join("target")) else { return Vec::new() };
    let mut cleared = Vec::new();
    let mut clear = |inc: std::path::PathBuf| {
        let holds_a_session = std::fs::read_dir(&inc).map(|mut rd| rd.next().is_some()).unwrap_or(false);
        if holds_a_session && std::fs::remove_dir_all(&inc).is_ok() {
            cleared.push(inc);
        }
    };
    for entry in profiles.flatten() {
        clear(entry.path().join("incremental"));
        // A cross build (#2772) keeps its profiles one level down:
        // `target/<triple>/<profile>/incremental`.
        if let Ok(nested) = std::fs::read_dir(entry.path()) {
            for sub in nested.flatten() {
                clear(sub.path().join("incremental"));
            }
        }
    }
    cleared.sort();
    cleared
}

/// Run `build` once and, if it failed with rustc's ICE banner, clear the
/// dir's incremental session stores and run it once more (#2500).
///
/// An interrupted build (ENOSPC, a killed process) can leave a rustc
/// incremental session with its `work-products.bin` naming a `*.pre-lto.bc`
/// that was never written. rustc then panics on every later build that
/// reuses the session — the same program shape fails forever, the message
/// blames rustc and names a temp path, and nothing tells the user to delete
/// it. The session store is a pure cache, so the recovery is to drop it and
/// rebuild. Exactly one retry: if it also fails, the ORIGINAL error is
/// reported (the retry's, if different, is not what the user's build said).
/// A recovered build says so on stderr in one line.
///
/// The caller holds whatever lock serializes builds in `project_dir` (the
/// `BuildDirLock` of `build_native_cached` / the cdylib build; the REPL's
/// dir is private to its one interactive process).
pub(super) fn build_recovering_from_ice(
    project_dir: &std::path::Path,
    mut build: impl FnMut() -> Result<std::path::PathBuf, String>,
) -> Result<std::path::PathBuf, String> {
    let first = build();
    let Err(first_err) = &first else { return first };
    if !is_rustc_ice(first_err) {
        return first;
    }
    let cleared = clear_incremental_sessions(project_dir);
    if cleared.is_empty() {
        // Nothing stale to recover from: a genuine rustc ICE on this code.
        return first;
    }
    match build() {
        Ok(bin) => {
            let names = cleared.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
            crate::err(&format!(
                "note: rustc crashed on a stale incremental session; cleared {} and rebuilt successfully",
                names
            ));
            Ok(bin)
        }
        Err(_) => first,
    }
}

/// Detect rustc-style `error[E\d{4}]` codes leaking through our checker.
/// Almide's diagnostic codes are 3 digits (E001..E099); rustc uses 4 digits
/// (E0001..E9999). A 4-digit code in the output unambiguously means our
/// codegen produced invalid Rust — flag it for the bug-report wrapper so
/// dojo classifiers don't mistake it for a user-facing language error.
fn contains_rustc_error_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = b"error[E";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Count consecutive digits after `error[E`
            let mut j = i + needle.len();
            let mut digits = 0;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits += 1;
                j += 1;
            }
            if digits >= 4 && j < bytes.len() && bytes[j] == b']' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{build_recovering_from_ice, clear_incremental_sessions, contains_rustc_error_code, defines_entry_point, is_rustc_ice};

    #[test]
    fn detects_4_digit_rustc_code() {
        assert!(contains_rustc_error_code("error[E0599]: no method named 'foo' found"));
        assert!(contains_rustc_error_code("blah\nerror[E0382]: use of moved value\nblah"));
    }

    #[test]
    fn ignores_3_digit_almide_code() {
        assert!(!contains_rustc_error_code("error[E001]: type mismatch"));
        assert!(!contains_rustc_error_code("error[E013]: ..."));
    }

    #[test]
    fn ignores_no_brackets() {
        assert!(!contains_rustc_error_code("error: something went wrong"));
        assert!(!contains_rustc_error_code(""));
    }

    // #2370: the question "does this crate define an entry point" is answered
    // once, by `defines_entry_point`. These pin the two ways the old spellings
    // were wrong, so the narrowing cannot be quietly widened back.

    /// The defect: a library crate whose only `fn main(` is inside a STRING
    /// LITERAL. `contains("fn main(")` said yes, no empty main was appended,
    /// and rustc's E0601 was reported to the author as a compiler bug.
    #[test]
    fn a_main_inside_a_string_literal_is_not_a_definition() {
        let crate_src = concat!(
            "pub fn kinds(s: &str) -> Vec<String> { vec![s.to_string()] }\n",
            "#[test]\n",
            "fn t() {\n",
            "    assert_eq!(kinds(\"type P = {}\\nfn add(a: Int) -> Int = a\\n",
            "effect fn main() -> Unit = println(\\\"hi\\\")\\n\"), vec![\"x\".to_string()]);\n",
            "}\n",
        );
        assert!(crate_src.contains("fn main("), "the fixture must still contain the text, or it is not this bug");
        assert!(
            !defines_entry_point(crate_src),
            "a `fn main(` inside a literal counted as a definition — the crate gets no \
             entry point and rustc's E0601 surfaces as an Almide bug (#2370)"
        );
    }

    /// The other direction: anchoring must not LOSE a real entry point, or an
    /// empty `main` is appended next to the real one and the crate stops
    /// compiling. `pub` is load-bearing here — unanchored `contains` matched it
    /// only as a substring.
    #[test]
    fn every_spelling_of_a_real_entry_point_still_counts() {
        for src in [
            "fn main() {\n    println!(\"x\");\n}\n",
            "pub fn main() {}\n",
            "fn almide_main() -> i32 { 0 }\n",
            "pub fn almide_main() -> i32 { 0 }\n",
            "use std::io;\n\nfn main() {}\n",
        ] {
            assert!(defines_entry_point(src), "lost a real entry point in:\n{src}");
        }
    }

    /// An indented `fn main(` is not a crate-root item — it is inside a module
    /// or a block, and it is not the entry point rustc looks for.
    #[test]
    fn an_indented_main_is_not_the_crate_entry_point() {
        assert!(!defines_entry_point("mod inner {\n    pub fn main() {}\n}\n"));
    }

    // #2500: the stale-incremental-session recovery. The end-to-end shape (a
    // real rustc ICE from a deleted `*.pre-lto.bc`) is `tests/run_cache_recovery_test.rs`;
    // these pin the retry POLICY with a scripted build.

    const ICE: &str = "thread 'rustc' panicked at compiler/rustc_codegen_ssa/src/back/write.rs:2290:29:\n\
        failed to open bitcode file `.../incremental/almide_out-1/s-2-working/3.pre-lto.bc`: No such file or directory\n\
        error: the compiler unexpectedly panicked. This is a bug\n";

    #[test]
    fn the_ice_banner_is_the_only_trigger() {
        assert!(is_rustc_ice(ICE));
        assert!(is_rustc_ice("error: internal compiler error: unexpected panic"));
        // The codegen-bug wrapper keeps the banner inside its own text.
        assert!(is_rustc_ice(&super::wrap_codegen_leak(format!("{ICE}\nerror: could not compile `almide-out`"))));
        assert!(!is_rustc_ice("error[E0599]: no method named `foo`\nerror: could not compile `almide-out`"));
        assert!(!is_rustc_ice("thread 'main' panicked at src/main.rs:3:5"));
    }

    fn scripted(outcomes: Vec<Result<&'static str, &'static str>>) -> (impl FnMut() -> Result<std::path::PathBuf, String>, std::rc::Rc<std::cell::Cell<usize>>) {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let c = calls.clone();
        let mut it = outcomes.into_iter();
        (
            move || {
                c.set(c.get() + 1);
                it.next().expect("more build calls than scripted")
                    .map(std::path::PathBuf::from)
                    .map_err(String::from)
            },
            calls,
        )
    }

    #[test]
    fn an_ice_clears_the_sessions_and_retries_once() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental/almide_out-1/s-2-working");
        std::fs::create_dir_all(&inc).unwrap();
        std::fs::write(inc.join("work-products.bin"), b"x").unwrap();
        let (build, calls) = scripted(vec![Err(ICE), Ok("bin")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out.as_deref().ok(), Some(std::path::Path::new("bin")));
        assert_eq!(calls.get(), 2);
        assert!(!dir.path().join("target/debug/incremental").exists(), "the session store must be gone");
        assert!(dir.path().join("target/debug").is_dir(), "only the incremental store is cleared, not the profile dir");
    }

    #[test]
    fn a_retry_that_also_fails_reports_the_original_error_and_never_loops() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target/debug/incremental/almide_out-1")).unwrap();
        let (build, calls) = scripted(vec![Err(ICE), Err("error: the compiler unexpectedly panicked. This is a bug\n(second)")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out, Err(ICE.to_string()), "the user's build said the first error; that is what is reported");
        assert_eq!(calls.get(), 2, "exactly one retry, even though the retry was itself an ICE");
    }

    #[test]
    fn a_plain_compile_error_is_not_retried_and_keeps_its_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental/almide_out-1");
        std::fs::create_dir_all(&inc).unwrap();
        let (build, calls) = scripted(vec![Err("error[E0308]: mismatched types")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert!(out.is_err());
        assert_eq!(calls.get(), 1);
        assert!(inc.is_dir(), "a genuine compile error must not throw the session store away");
    }

    #[test]
    fn an_ice_with_no_session_store_is_a_genuine_ice_and_is_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        let (build, calls) = scripted(vec![Err(ICE)]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out, Err(ICE.to_string()));
        assert_eq!(calls.get(), 1);
    }

    /// `CARGO_INCREMENTAL=0` (what this repo's CI sets for every job) still
    /// leaves an EMPTY `target/<profile>/incremental/` behind. Nothing there
    /// can have gone stale, so an ICE under it is genuine and must not cost
    /// a retry.
    #[test]
    fn an_empty_session_dir_is_not_something_to_recover_from() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental");
        std::fs::create_dir_all(&inc).unwrap();
        assert!(clear_incremental_sessions(dir.path()).is_empty(), "an empty session dir is not a session");
        assert!(inc.is_dir(), "and it is not removed either");
        let (build, calls) = scripted(vec![Err(ICE)]);
        assert_eq!(build_recovering_from_ice(dir.path(), build), Err(ICE.to_string()));
        assert_eq!(calls.get(), 1, "no retry when there was no session to clear");
    }

    // ── #3350: target-specific native deps ──

    use super::{build_cargo_toml, generated_crate_deps, insert_cargo_dep, GENERATED_CARGO_TOML};
    use crate::project::NativeDep;

    fn dep(name: &str, spec: &str, target: Option<&str>) -> NativeDep {
        NativeDep { name: name.into(), spec: spec.into(), target: target.map(str::to_string) }
    }

    /// The `(table, name, spec)` triples of a generated manifest, read back as
    /// TOML — what Cargo will see, not what the text looks like.
    fn deps_of(manifest: &str) -> Vec<(String, String, String)> {
        let doc: toml::Table = toml::from_str(manifest).unwrap_or_else(|e| panic!("not TOML ({e}):\n{manifest}"));
        let mut out = Vec::new();
        let mut push = |table: &str, deps: &toml::Value| {
            for (name, spec) in deps.as_table().expect("a dependency table") {
                out.push((table.to_string(), name.clone(), spec.to_string()));
            }
        };
        if let Some(d) = doc.get("dependencies") {
            push("dependencies", d);
        }
        for (key, platform) in doc.get("target").and_then(toml::Value::as_table).into_iter().flatten() {
            if let Some(d) = platform.get("dependencies") {
                push(key, d);
            }
        }
        out
    }

    const ANDROID: &str = r#"cfg(target_os = "android")"#;
    const DESKTOP: &str = r#"cfg(not(any(target_os = "android", target_os = "ios")))"#;

    #[test]
    fn a_target_specific_native_dep_lands_only_under_its_target_table() {
        let manifest = build_cargo_toml(GENERATED_CARGO_TOML, &[
            dep("anyhow", "1", None),
            dep("arboard", "3", Some(DESKTOP)),
            dep("jni", "0.21", Some(ANDROID)),
            dep("ndk", "{ version = \"0.9\", default-features = false }", Some(ANDROID)),
        ]);
        let got = deps_of(&manifest);
        let table_of = |name: &str| got.iter().filter(|(_, n, _)| n == name).map(|(t, _, _)| t.as_str()).collect::<Vec<_>>();
        assert_eq!(table_of("anyhow"), ["dependencies"], "{manifest}");
        assert_eq!(table_of("arboard"), [DESKTOP], "{manifest}");
        assert_eq!(table_of("jni"), [ANDROID], "{manifest}");
        assert_eq!(table_of("ndk"), [ANDROID], "{manifest}");
        assert!(manifest.contains(&format!("[target.'{ANDROID}'.dependencies]\njni = \"0.21\"\nndk = ")), "file order within a table:\n{manifest}");
    }

    #[test]
    fn a_triple_and_a_key_holding_a_quote_are_written_as_cargo_reads_them() {
        let odd = r#"cfg(feature = "it's")"#;
        let manifest = build_cargo_toml(GENERATED_CARGO_TOML, &[
            dep("winapi", "0.3", Some("x86_64-pc-windows-gnu")),
            dep("odd", "1", Some(odd)),
        ]);
        let got = deps_of(&manifest);
        assert!(got.contains(&("x86_64-pc-windows-gnu".into(), "winapi".into(), "\"0.3\"".into())), "{manifest}");
        assert!(got.contains(&(odd.into(), "odd".into(), "\"1\"".into())), "{manifest}");
    }

    /// The runtime's crates (#3346) compose with target tables: a user crate
    /// declared only for one target does not stand in for the runtime's
    /// unconditional one, while an unconditional user crate still does.
    #[test]
    fn a_runtime_crate_is_declared_for_every_target_even_when_the_user_gates_one() {
        let code = "fn f() { almide_rt_zlib_deflate(); }";
        let gated = generated_crate_deps(code, &[dep("flate2", "{ version = \"1\", features = [\"zlib\"] }", Some(ANDROID))]);
        let got = deps_of(&build_cargo_toml(GENERATED_CARGO_TOML, &gated));
        let flate: Vec<&str> = got.iter().filter(|(_, n, _)| n == "flate2").map(|(t, _, _)| t.as_str()).collect();
        assert_eq!(flate, ["dependencies", ANDROID]);

        let plain = generated_crate_deps(code, &[dep("flate2", "1.0.30", None)]);
        let got = deps_of(&build_cargo_toml(GENERATED_CARGO_TOML, &plain));
        let flate: Vec<&(String, String, String)> = got.iter().filter(|(_, n, _)| n == "flate2").collect();
        assert_eq!(flate.len(), 1, "{got:?}");
        assert_eq!(flate[0].2, "\"1.0.30\"", "the user's spelling wins");
    }

    /// A dependency package's native deps are written into the finished
    /// manifest one at a time (`CrateInputs::apply`): into the right table,
    /// never twice, and a name that merely CONTAINS a declared one (the old
    /// `contains` check skipped `rand` because of `rand_core`) still lands.
    #[test]
    fn a_dependency_packages_native_deps_join_the_right_tables() {
        let mut manifest = build_cargo_toml(GENERATED_CARGO_TOML, &[dep("rand_core", "0.6", None), dep("jni", "0.21", Some(ANDROID))]);
        for d in [dep("rand", "0.8", None), dep("jni", "0.20", Some(ANDROID)), dep("ndk", "0.9", Some(ANDROID)), dep("objc2", "0.5", Some("cfg(target_os = \"ios\")"))] {
            insert_cargo_dep(&mut manifest, &d);
        }
        let got = deps_of(&manifest);
        assert!(got.contains(&("dependencies".into(), "rand".into(), "\"0.8\"".into())), "{manifest}");
        assert!(got.contains(&(ANDROID.into(), "jni".into(), "\"0.21\"".into())), "the first declaration stays:\n{manifest}");
        assert!(got.contains(&(ANDROID.into(), "ndk".into(), "\"0.9\"".into())), "{manifest}");
        assert!(got.iter().any(|(t, n, _)| t == "cfg(target_os = \"ios\")" && n == "objc2"), "{manifest}");
        assert_eq!(got.len(), 5, "{got:?}");
    }
}
