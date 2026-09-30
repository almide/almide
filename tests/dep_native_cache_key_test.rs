//! The native build cache sees a DEPENDENCY package's `native/` tree (#3091),
//! and every native builder (`run`, `build`, `bench`) injects it (#3095).
//!
//! The cached binary used to be keyed by the building package's `native/`
//! contents only (#887), while the build also copied every dependency's
//! `native/*.rs` (and its asset subdirectories) into the crate. Editing only a
//! dependency's native module was therefore a cache hit: `almide run` printed
//! the old value and `almide build` shipped the previous binary, both exit 0.
//!
//! Layout: `app/` depends on `nat = { path = "../nat" }`; `nat` binds an
//! `@extern(rust)` fn to `nat/native/natval.rs`, which also `include_str!`s
//! `nat/native/data/word.txt`. Each case edits ONE of those two files and
//! nothing else, then runs again in the same build scratch dir.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn native_rs(n: i64) -> String {
    format!(
        "pub fn number() -> i64 {{ {n} }}\n\
         pub fn word() -> String {{ include_str!(\"data/word.txt\").trim().to_string() }}\n"
    )
}

/// A fresh `<temp>/…/{app,nat}` tree; returns its root.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3091-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("nat/almide.toml"), "[package]\nname = \"nat\"\nversion = \"0.1.0\"\n");
    write(
        root.join("nat/src/mod.almd"),
        "@extern(rust, \"crate::natval\", \"number\")\nfn number() -> Int = _\n\n\
         @extern(rust, \"crate::natval\", \"word\")\nfn word() -> String = _\n",
    );
    write(root.join("nat/native/natval.rs"), &native_rs(1));
    write(root.join("nat/native/data/word.txt"), "alpha\n");
    write(
        root.join("app/almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nnat = { path = \"../nat\" }\n",
    );
    write(
        root.join("app/src/main.almd"),
        "import nat\n\neffect fn main() -> Unit = {\n  println(int.to_string(nat.number()))\n  println(nat.word())\n}\n",
    );
    root
}

/// `almide run src/main.almd` in `app/`, building in `root/.run`, with no
/// ambient `RUSTFLAGS`: stdout.
fn run(root: &Path) -> String {
    run_with(root, None)
}

/// [`run`] with `RUSTFLAGS` set to `rustflags`, or removed when `None`.
fn run_with(root: &Path, rustflags: Option<&str>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(root.join("app"))
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(["run", "src/main.almd"]);
    if let Some(flags) = rustflags {
        cmd.env("RUSTFLAGS", flags);
    }
    let out = cmd
        .output()
        .expect("spawn almide");
    assert!(
        out.status.success(),
        "almide run failed\n--- stderr ---\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn editing_only_a_dependency_native_module_or_its_asset_rebuilds() {
    let root = scratch("edit");
    assert_eq!(run(&root), "1\nalpha\n", "baseline");

    // Only the dependency's native module changes.
    write(root.join("nat/native/natval.rs"), &native_rs(2));
    assert_eq!(run(&root), "2\nalpha\n", "a dependency's native/*.rs edit was a stale cache hit");

    // Only an asset the dependency's module include_str!s changes.
    write(root.join("nat/native/data/word.txt"), "beta\n");
    assert_eq!(run(&root), "2\nbeta\n", "a dependency's native asset edit was a stale cache hit");

    // Reverting is a hit on the first binary, not a stale one.
    write(root.join("nat/native/natval.rs"), &native_rs(1));
    write(root.join("nat/native/data/word.txt"), "alpha\n");
    assert_eq!(run(&root), "1\nalpha\n", "reverting the dependency did not restore its output");

    let _ = std::fs::remove_dir_all(&root);
}

/// The same crate built under different `RUSTFLAGS` is a different binary, so
/// the flags are part of the key: the dependency's module answers by
/// `cfg!`, and a hit on the flag-less binary would print the old value.
#[test]
fn rustflags_are_part_of_the_native_cache_key() {
    let root = scratch("rustflags");
    write(
        root.join("nat/native/natval.rs"),
        "pub fn number() -> i64 { if cfg!(almide_issue3091) { 7 } else { 1 } }\n\
         pub fn word() -> String { include_str!(\"data/word.txt\").trim().to_string() }\n",
    );
    assert_eq!(run(&root), "1\nalpha\n", "baseline");
    assert_eq!(
        run_with(&root, Some("--cfg almide_issue3091")),
        "7\nalpha\n",
        "a RUSTFLAGS change was a stale cache hit"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `almide bench` builds a package's native modules the way `run` does
/// (#3095): it used to pass no native config, so a package whose code calls
/// an `@extern(rust)` module failed to compile under `bench` alone.
#[test]
fn bench_builds_the_native_modules_of_the_package_and_its_dependencies() {
    let root = scratch("bench");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(root.join("app"))
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(["bench", "src/main.almd", "--runs", "1"])
        .output()
        .expect("spawn almide bench");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "almide bench failed on a package with a native dependency\n{stderr}");
    assert!(stderr.contains("median"), "no median headline:\n{stderr}");
    let _ = std::fs::remove_dir_all(&root);
}
