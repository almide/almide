//! The modules a package's own `native/*.rs` calls back into (#3424).
//!
//! A native module that calls the package's Almide code spells the symbol a
//! CONSUMER of the package links: the module path `<pkg>_v<N>` (root,
//! `src/mod.almd`) or `<pkg>_v<N>.<sub>` (a sub-module) through
//! `almide_base::names::module_ident` — `crate::almide_rt_tf_0v0_entry`,
//! `crate::almide_rt_tf_0v0_1calc_double`. Built as a dependency, the package's modules are loaded under
//! that versioned name and the shim links. Built as ITSELF (its own tests, its
//! own `almide run`), the `native/` tree is copied into the crate all the same
//! (`cli::cargo_build::CrateInputs`), but the root module was loaded only when
//! the entry file imported it, and every `self` module lowered unversioned
//! (`almide_rt_tf_entry`) — so the shim failed rustc with E0425 on a name
//! that did not exist.
//!
//! The native Rust build therefore (1) loads every module the native code
//! names, the root always among them, and (2) lowers the package's own
//! modules under the versioned name the native code spells. Nothing changes
//! for a package whose native code does not call back.

use super::*;
use std::collections::HashMap;

/// What [`include_native_callback_modules`] adds to the build: the versioned
/// name each of the package's own modules lowers under, keyed by the module's
/// resolved name. Empty when the package's native code does not call back.
pub type SelfVersionedNames = HashMap<String, String>;

/// Load the modules the package's `native/*.rs` calls back into, and return
/// the versioned names the package's own modules must lower under so the
/// native code's `crate::almide_rt_<pkg>_0v<N>_*` names resolve. For the native
/// (Rust) build only: the wasm legs never compile `native/`.
pub fn include_native_callback_modules(
    source_file: &str,
    dep_paths: &[(project::PkgId, PathBuf)],
    resolved: &mut ResolvedModules,
) -> Result<SelfVersionedNames, String> {
    let base_dir = Path::new(source_file).parent().unwrap_or(Path::new("."));
    let Some(root) = find_project_root(base_dir) else { return Ok(HashMap::new()) };
    let Some(pkg) = project::parse_toml(&root.join("almide.toml")).ok().map(|p| p.package.name) else {
        return Ok(HashMap::new());
    };
    if pkg.is_empty() {
        return Ok(HashMap::new());
    }
    let native = native_sources(&root);
    let Some(major) = callback_major(&native, &pkg) else { return Ok(HashMap::new()) };
    let prefix = format!("{pkg}_v{major}");
    let src_dir = root.join("src");

    let mut loaded_names: HashSet<String> = resolved.modules.iter().map(|m| m.0.clone()).collect();
    let mut loading: HashSet<String> = HashSet::new();
    let mut ctx = ResolveCtx {
        base_dir,
        dep_paths,
        loaded: &mut resolved.modules,
        loaded_names: &mut loaded_names,
        loading: &mut loading,
        sources: &mut resolved.sources,
    };
    // The root module, unless the entry already imported it (under any alias).
    if src_dir.join("mod.almd").exists() && !ctx.loaded.iter().any(|m| m.3) {
        resolve_self_import(&[crate::intern::sym("self")], None, Some(&root), &mut ctx)?;
    }
    // Every sub-module the native code names.
    for segs in self_module_paths(&src_dir) {
        let path = format!("{prefix}.{}", segs.join("."));
        let ident = almide_base::names::module_ident(&path);
        // The pre-#3338 spelling too, kept as a deprecated alias (#3425).
        let legacy = almide_base::names::legacy_module_ident(&path);
        if !native.contains(&format!("almide_rt_{ident}_")) && !native.contains(&format!("almide_rt_{legacy}_")) {
            continue;
        }
        let mod_path: Vec<crate::intern::Sym> = segs.iter().map(|s| crate::intern::sym(s)).collect();
        let canonical = self_module_canonical(&mod_path);
        if !ctx.loaded_names.contains(&canonical) {
            load_self_module(&canonical, &mod_path, &src_dir, &mut ctx, false)?;
        }
    }
    Ok(versioned_self_names(resolved, &src_dir, &prefix))
}

/// The text of the package's `native/*.rs` modules (the files the native
/// build declares as `mod`s), concatenated.
fn native_sources(root: &Path) -> String {
    let Ok(entries) = std::fs::read_dir(root.join("native")) else { return String::new() };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "rs"))
        .collect();
    paths.sort();
    paths.iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect::<Vec<_>>().join("\n")
}

/// The major `N` of the first `almide_rt_<pkg>_0v<N>_` (module path
/// `<pkg>_v<N>`) the native code names — or of its pre-#3338 spelling
/// `almide_rt_<pkg>_v<N>_`, a deprecated alias (#3425).
fn callback_major(native: &str, pkg: &str) -> Option<u64> {
    let versioned = format!("{pkg}_v");
    let needles = [
        format!("almide_rt_{}_0v", almide_base::names::module_ident(pkg)),
        format!("almide_rt_{}", almide_base::names::legacy_module_ident(&versioned)),
    ];
    needles.iter().find_map(|needle| {
        native.match_indices(needle.as_str()).find_map(|(at, _)| {
            let rest = &native[at + needle.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let after = rest[digits.len()..].chars().next();
            (!digits.is_empty() && after == Some('_')).then(|| digits.parse().ok()).flatten()
        })
    })
}

/// The `self.<path>` of every non-test module under `src/` other than the
/// root: `src/a/b.almd` is `[a, b]`, `src/a/mod.almd` is `[a]`. Sorted.
fn self_module_paths(src_dir: &Path) -> Vec<Vec<String>> {
    fn walk(dir: &Path, prefix: &[String], out: &mut Vec<Vec<String>>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for path in entries.flatten().map(|e| e.path()) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if path.is_dir() {
                let sub: Vec<String> = prefix.iter().cloned().chain([name]).collect();
                walk(&path, &sub, out);
            } else if let Some(stem) = name.strip_suffix(".almd") {
                if stem.ends_with("_test") {
                    continue;
                }
                if stem == "mod" {
                    if !prefix.is_empty() {
                        out.push(prefix.to_vec());
                    }
                } else {
                    out.push(prefix.iter().cloned().chain([stem.to_string()]).collect());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(src_dir, &[], &mut out);
    out.sort();
    out.dedup();
    out
}

/// The versioned name of every resolved module read from the package's own
/// `src/`: the root is `<prefix>`, `src/a/b.almd` is `<prefix>.a.b` — the
/// names a consumer's build gives the same files.
fn versioned_self_names(resolved: &ResolvedModules, src_dir: &Path, prefix: &str) -> SelfVersionedNames {
    let Ok(src_dir) = src_dir.canonicalize() else { return HashMap::new() };
    let mut out = HashMap::new();
    for (name, _, pkg_id, _) in &resolved.modules {
        if pkg_id.is_some() {
            continue;
        }
        let Some((path, _)) = resolved.sources.get(name) else { continue };
        let Ok(path) = Path::new(path).canonicalize() else { continue };
        let Ok(rel) = path.strip_prefix(&src_dir) else { continue };
        let mut segs: Vec<String> = rel.iter().map(|s| s.to_string_lossy().to_string()).collect();
        let Some(last) = segs.pop() else { continue };
        let stem = last.strip_suffix(".almd").unwrap_or(&last).to_string();
        if stem != "mod" {
            segs.push(stem);
        }
        let versioned = if segs.is_empty() { prefix.to_string() } else { format!("{prefix}.{}", segs.join(".")) };
        out.insert(name.clone(), versioned);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::callback_major;

    #[test]
    fn the_major_is_read_from_the_callback_name() {
        assert_eq!(callback_major("crate::almide_rt_tf_0v0_entry(x)", "tf"), Some(0));
        assert_eq!(callback_major("crate::almide_rt_tf_0v12_1calc_double(x)", "tf"), Some(12));
    }

    #[test]
    fn the_major_is_read_from_the_pre_3338_spelling_too() {
        assert_eq!(callback_major("crate::almide_rt_tf_v0_entry(x)", "tf"), Some(0));
        assert_eq!(callback_major("crate::almide_rt_tf_v3_calc_double(x)", "tf"), Some(3));
        assert_eq!(callback_major("crate::almide_rt_my_pkg_v1_go(x)", "my_pkg"), Some(1));
    }

    #[test]
    fn native_code_that_does_not_call_back_names_no_major() {
        assert_eq!(callback_major("pub fn f() -> i64 { 1 }", "tf"), None);
        assert_eq!(callback_major("almide_rt_tf_0value(x)", "tf"), None);
        assert_eq!(callback_major("almide_rt_tfx_0v0_entry(x)", "tf"), None);
    }
}
