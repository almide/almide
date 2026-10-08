//! The `Cargo.toml` of a generated crate: the template, the runtime crates
//! it always declares, and the `[native-deps]` it gains per target (#3350).
//! Part of the native build recipe — `build_environment_key` hashes this
//! file's source into every cache key.

/// Cargo.toml template for generated Rust projects (without HTTP/TLS).
pub(super) const GENERATED_CARGO_TOML: &str = r#"[package]
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
pub(super) fn insert_cargo_dep(toml: &mut String, dep: &crate::project::NativeDep) {
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
pub(super) fn inject_almide_par_if_rayon(cmd: &mut std::process::Command, project_dir: &std::path::Path) {
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
pub(super) fn build_cargo_toml(base_toml: &str, native_deps: &[crate::project::NativeDep]) -> String {
    let mut toml = base_toml.to_string();
    for dep in native_deps {
        insert_cargo_dep(&mut toml, dep);
    }
    toml
}

#[cfg(test)]
mod tests {
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
