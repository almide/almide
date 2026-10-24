//! `[permissions] native` (#4093): a dependency that brings native code is
//! refused unless the root `almide.toml` grants it by name.
//!
//! A dependency's `native/*.rs` is copied into the generated crate, its
//! `[native-deps]` crates are added to the generated `Cargo.toml` (so their
//! `build.rs` and proc macros run), and an `@extern(rust, "path", "fn")` fn
//! is spliced in as a call to `path::fn` — all measured before this change,
//! all of it compiled and run by cargo with no grant. Each case below builds
//! a small tree of path packages and drives the real binary.
//!
//! Layout: `nat` has `native/natval.rs` bound by an `@extern(rust)` fn;
//! `mid` imports `nat` and has no native code of its own; `ext` has only an
//! `@extern(rust)` fn naming a `std` path, no `native/`; `crates` declares
//! `[native-deps]`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4093-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("nat/almide.toml"), "[package]\nname = \"nat\"\nversion = \"0.3.0\"\n");
    write(root.join("nat/src/mod.almd"), "@extern(rust, \"crate::natval\", \"number\")\nfn number() -> Int = _\n");
    write(root.join("nat/native/natval.rs"), "pub fn number() -> i64 { 42 }\n");
    write(
        root.join("mid/almide.toml"),
        "[package]\nname = \"mid\"\nversion = \"0.1.0\"\n\n[dependencies]\nnat = { path = \"../nat\" }\n",
    );
    write(root.join("mid/src/mod.almd"), "import nat\n\nfn twice() -> Int = nat.number() * 2\n");
    write(root.join("ext/almide.toml"), "[package]\nname = \"ext\"\nversion = \"0.1.0\"\n");
    write(root.join("ext/src/mod.almd"), "@extern(rust, \"std::process\", \"id\")\nfn pid() -> Int = _\n");
    write(root.join("crates/almide.toml"), "[package]\nname = \"crates\"\nversion = \"0.1.0\"\n\n[native-deps]\nitoa = \"1\"\n");
    write(root.join("crates/src/mod.almd"), "fn one() -> Int = 1\n");
    root
}

/// An app at `root/<name>` depending on `deps` (`name = path`), with
/// `[permissions] native = grants` when `grants` is `Some`, whose `main`
/// prints `expr` and whose test asserts it equals `expected`.
fn app(root: &Path, name: &str, deps: &[&str], grants: Option<&[&str]>, expr: &str, expected: &str) -> PathBuf {
    let dir = root.join(name);
    let mut toml = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
    for d in deps {
        toml.push_str(&format!("{d} = {{ path = \"../{d}\" }}\n"));
    }
    if let Some(g) = grants {
        let list = g.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
        toml.push_str(&format!("\n[permissions]\nnative = [{list}]\n"));
    }
    write(dir.join("almide.toml"), &toml);
    let imports: String = deps.iter().map(|d| format!("import {d}\n")).collect();
    write(
        dir.join("src/main.almd"),
        &format!(
            "{imports}\neffect fn main() -> Unit = {{\n  println(int.to_string({expr}))\n}}\n\n\
             test \"value\" {{\n  assert_eq({expr}, {expected})\n}}\n"
        ),
    );
    dir
}

fn almide(dir: &Path, root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(args)
        .output()
        .expect("spawn almide")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// `run`, `build`, `build --target wasm` and `test` all exit 1 with the
/// refusal for `dep`; returns the `run` stderr.
fn assert_refused_everywhere(dir: &Path, root: &Path, dep: &str) -> String {
    let bin = root.join("out-bin");
    let wasm = root.join("out.wasm");
    let commands: [Vec<&str>; 4] = [
        vec!["run", "src/main.almd"],
        vec!["build", "src/main.almd", "-o", bin.to_str().unwrap()],
        vec!["build", "src/main.almd", "--target", "wasm", "-o", wasm.to_str().unwrap()],
        vec!["test", "src/main.almd"],
    ];
    let mut first = None;
    for args in &commands {
        let o = almide(dir, root, args);
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {}` was not refused:\n{err}", args.join(" "));
        assert!(
            err.contains(&format!("error: dependency `{dep}`")) && err.contains("brings native code that"),
            "`almide {}` did not name `{dep}`:\n{err}",
            args.join(" ")
        );
        assert!(
            err.contains("    [permissions]\n    native = [") && err.contains(&format!("\"{dep}\"]")),
            "no grant line to add:\n{err}"
        );
        assert!(!bin.exists() && !wasm.exists(), "a refused build wrote its output");
        first.get_or_insert(err);
    }
    first.unwrap()
}

#[test]
fn an_ungranted_native_dependency_is_refused_on_every_command_and_target() {
    let root = scratch("refused");
    let dir = app(&root, "app", &["nat"], None, "nat.number()", "42");
    let err = assert_refused_everywhere(&dir, &root, "nat");
    assert!(err.contains("nat 0.3.0 at"), "no version and location:\n{err}");
    assert!(err.contains("native/natval.rs") && err.contains("@extern fn `number` (src/mod.almd:2)"), "the native code is not named:\n{err}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_granted_native_dependency_builds_runs_and_tests() {
    let root = scratch("granted");
    let dir = app(&root, "app", &["nat"], Some(&["nat"]), "nat.number()", "42");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert!(o.status.success(), "granted run failed:\n{}", stderr(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "42\n");
    assert!(
        stderr(&o).contains("note: building native code from `nat` (granted by [permissions].native): native/natval.rs"),
        "the build did not name the granted native dependency:\n{}",
        stderr(&o)
    );
    let bin = root.join("app-bin");
    let o = almide(&dir, &root, &["build", "src/main.almd", "-o", bin.to_str().unwrap()]);
    assert!(o.status.success() && bin.exists(), "granted build failed:\n{}", stderr(&o));
    let o = almide(&dir, &root, &["test", "src/main.almd"]);
    assert!(o.status.success(), "granted test failed:\n{}", stderr(&o));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_transitive_ungranted_native_dependency_is_refused() {
    let root = scratch("transitive");
    // Granting the direct dependency does not grant what it pulls in.
    let dir = app(&root, "app", &["mid"], Some(&["mid"]), "mid.twice()", "84");
    let err = assert_refused_everywhere(&dir, &root, "nat");
    assert!(err.contains("`nat` (required by another dependency)"), "not marked transitive:\n{err}");
    assert!(err.contains("native = [\"mid\", \"nat\"]"), "the grant line drops the existing grant:\n{err}");
    // Granting it by name builds.
    let dir = app(&root, "app", &["mid"], Some(&["nat"]), "mid.twice()", "84");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert!(o.status.success(), "granted transitive run failed:\n{}", stderr(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "84\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_extern_rust_fn_without_native_dir_and_native_deps_are_native_code() {
    let root = scratch("extern");
    let dir = app(&root, "app", &["ext"], None, "ext.pid() * 0", "0");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert_eq!(o.status.code(), Some(1), "an @extern(rust) dependency was not refused:\n{}", stderr(&o));
    assert!(stderr(&o).contains("native code: @extern fn `pid` (src/mod.almd:2)"), "{}", stderr(&o));
    let dir = app(&root, "app2", &["crates"], None, "crates.one()", "1");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert_eq!(o.status.code(), Some(1), "a [native-deps] dependency was not refused:\n{}", stderr(&o));
    assert!(stderr(&o).contains("native code: [native-deps] itoa"), "{}", stderr(&o));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_root_packages_own_native_code_needs_no_grant() {
    let root = scratch("own");
    write(root.join("pure/almide.toml"), "[package]\nname = \"pure\"\nversion = \"0.1.0\"\n");
    write(root.join("pure/src/mod.almd"), "fn add(a: Int, b: Int) -> Int = a + b\n");
    let dir = root.join("app");
    write(dir.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\npure = { path = \"../pure\" }\n");
    write(dir.join("native/own.rs"), "pub fn seven() -> i64 { 7 }\n");
    write(
        dir.join("src/main.almd"),
        "import pure\n\n@extern(rust, \"crate::own\", \"seven\")\nfn seven() -> Int = _\n\n\
         effect fn main() -> Unit = {\n  println(int.to_string(pure.add(seven(), 1)))\n}\n",
    );
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert!(o.status.success(), "the root's own native code was refused:\n{}", stderr(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "8\n");
    assert!(!stderr(&o).contains("building native code from"), "a dependency without native code was announced");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn deps_lists_capabilities_native_code_and_grant() {
    let root = scratch("deps");
    write(root.join("pure/almide.toml"), "[package]\nname = \"pure\"\nversion = \"0.2.0\"\n");
    write(root.join("pure/src/mod.almd"), "fn add(a: Int, b: Int) -> Int = a + b\n");
    let dir = app(&root, "app", &["mid", "pure", "crates"], Some(&["crates"]), "pure.add(mid.twice(), crates.one())", "85");
    let o = almide(&dir, &root, &["deps"]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "deps failed:\n{}", stderr(&o));
    let every = "capabilities: Env, Fan, IO, Net, Rand, Time";
    let expected = [
        "mid = path ../mid\n  version: 0.1.0\n".to_string(),
        format!("  {every}\n  native code: none\n"),
        "pure = path ../pure\n  version: 0.2.0\n  capabilities: none\n  native code: none\n".to_string(),
        "crates = path ../crates\n  version: 0.1.0\n  capabilities: none\n  native code: [native-deps] itoa (granted by [permissions].native)\n".to_string(),
        "nat (required by another dependency) = ".to_string(),
        format!("  version: 0.3.0\n  {every}\n  native code: native/natval.rs; @extern fn `number` (src/mod.almd:2) (NOT granted: build, run and test refuse it until [permissions].native names \"nat\")\n"),
    ];
    for piece in &expected {
        assert!(out.contains(piece.as_str()), "`almide deps` is missing\n{piece}\n--- got ---\n{out}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
