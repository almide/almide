//! A Rust-keyword fn name in a PACKAGE SUBMODULE (#1494, the #659 residue).
//!
//! #659 fixed the single-file case by raw-escaping a keyword fn name
//! (`move` → `r#move`) at both the definition and the call. That escape ran
//! BEFORE the module-origin mangle, so a submodule's `fn move` emitted
//! `pub fn almide_rt_util_r#move(...)` — invalid Rust (`almide_rt_util_r` is
//! a reserved literal prefix since Rust 2021) — while every call-emitting
//! path mangled without escaping and asked for `almide_rt_util_move`. A
//! prefixed symbol is never a keyword, so the escape now applies only to
//! unprefixed names (`render_fn_safe_name`), and def and call agree by
//! construction. Contract C-088 carries the submodule cell.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// A package whose SUBMODULE defines fns named with Rust keywords, called
/// qualified from main. The single-file spelling of the same thing is pinned
/// by `spec/wasm_cross/reserved_keyword_fn.almd`; only the module-origin
/// prefix path is at stake here.
fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1494-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("mkdir src");
    std::fs::write(
        root.join("almide.toml"),
        "[package]\nname = \"kwpkg\"\nversion = \"0.1.0\"\n",
    )
    .expect("write toml");
    std::fs::write(
        root.join("src").join("util.almd"),
        "fn move(x: Int) -> Int = x + 1\n\nfn box(x: Int) -> Int = x * 2\n",
    )
    .expect("write util");
    std::fs::write(
        root.join("src").join("main.almd"),
        "import self.util\n\neffect fn main() -> Unit = {\n  println(int.to_string(util.move(1)))\n  println(int.to_string(util.box(21)))\n}\n",
    )
    .expect("write main");
    root
}

/// The exact reported repro: `almide build` used to die in rustc with
/// "unknown prefix `almide_rt_util_r`" before any program output existed.
#[test]
fn keyword_fn_in_submodule_builds_and_runs_native() {
    if !tools_available() {
        return;
    }
    let root = scratch("native");
    let bin_path = root.join("app");
    let build = Command::new(almide_bin())
        .args(["build", "src/main.almd", "-o", bin_path.to_str().unwrap()])
        .current_dir(&root)
        .output()
        .expect("spawn almide build");
    assert!(
        build.status.success(),
        "native build of a keyword-named submodule fn failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&bin_path).output().expect("run built binary");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(stdout, "2\n42\n", "wrong output from built binary: {stdout}");
}

/// The wasm leg never had the keyword problem; pin that both legs agree so
/// the fix cannot trade one target's validity for the other's.
#[test]
fn keyword_fn_in_submodule_matches_on_wasm() {
    if !tools_available() {
        return;
    }
    if Command::new("wasmtime").arg("--version").output().is_err() {
        return;
    }
    let root = scratch("wasm");
    let run = Command::new(almide_bin())
        .args(["run", "src/main.almd", "--target", "wasm"])
        .current_dir(&root)
        .output()
        .expect("spawn almide run --target wasm");
    assert!(
        run.status.success(),
        "wasm run failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "2\n42\n");
}
