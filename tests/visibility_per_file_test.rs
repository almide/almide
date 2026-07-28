//! `local fn` is same-FILE, `mod fn` is same-PROJECT (#870).
//!
//! Every source file loads as its own namespace module, so file identity IS
//! module identity. `check_fn_visibility` previously (a) allowed the ENTRY
//! program to reach any module's `local fn` (the `self_module_name` bypass),
//! and (b) rejected a same-project cross-file `mod fn` call (module equality
//! stood in for project identity). The caller is now the module being
//! inferred (`current_module_prefix`), and project identity derives from the
//! module NAME shape: dotted → the first segment's package, bare → a dep
//! package iff it is a known dep import root, else the self package.
//!
//! The locks, over one package with `src/mod.almd` + two siblings:
//!   1. sibling → other sibling's `local fn`  — E420.
//!   2. entry (mod.almd) → sibling's `local fn` — E420 (was silently allowed).
//!   3. sibling → other sibling's `mod fn` — ALLOWED (was wrongly E420).
//!   4. entry → sibling's `mod fn` — ALLOWED.
//!   5. `pub fn` everywhere — ALLOWED (control).

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

/// (combined output, exit code) of `almide check src/mod.almd` inside `dir`.
fn check_in(dir: &Path) -> (String, i32) {
    let output = Command::new(almide_bin())
        .args(["check", "src/mod.almd"])
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (combined, output.status.code().unwrap_or(-1))
}

/// One package: helper.almd declares the three visibility tiers; `other_body`
/// and `mod_body` are the sibling caller and the entry program.
fn scratch(name: &str, other_body: &str, mod_body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue870-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("mkdir scratch");
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"vispkg\"\nversion = \"0.1.0\"\n",
    )
    .expect("write toml");
    std::fs::write(
        dir.join("src/helper.almd"),
        "local fn secret() -> Int = 42\nmod fn project_only() -> Int = 7\npub fn open() -> Int = 1\n",
    )
    .expect("write helper");
    std::fs::write(dir.join("src/other.almd"), other_body).expect("write other");
    std::fs::write(dir.join("src/mod.almd"), mod_body).expect("write mod");
    dir
}

const MAIN_CALLING_OTHER: &str =
    "import self.other as o\n\nfn main() -> Unit = println(int.to_string(o.go()))\n";

#[test]
fn sibling_call_to_local_fn_is_rejected() {
    if !tools_available() {
        return;
    }
    let dir = scratch(
        "sib-local",
        "import self.helper as h\n\npub fn go() -> Int = h.secret()\n",
        MAIN_CALLING_OTHER,
    );
    let (out, code) = check_in(&dir);
    assert_ne!(code, 0, "cross-file local fn call must fail: {}", out);
    assert!(out.contains("E420"), "expected E420, got: {}", out);
    assert!(out.contains("local fn"), "hint names the tier: {}", out);
}

#[test]
fn entry_call_to_local_fn_is_rejected() {
    if !tools_available() {
        return;
    }
    let dir = scratch(
        "entry-local",
        "pub fn go() -> Int = 0\n",
        "import self.helper as h\n\nfn main() -> Unit = println(int.to_string(h.secret()))\n",
    );
    let (out, code) = check_in(&dir);
    assert_ne!(code, 0, "entry-file local fn call must fail: {}", out);
    assert!(out.contains("E420"), "expected E420, got: {}", out);
}

#[test]
fn sibling_call_to_mod_fn_is_allowed() {
    if !tools_available() {
        return;
    }
    let dir = scratch(
        "sib-mod",
        "import self.helper as h\n\npub fn go() -> Int = h.project_only() + h.open()\n",
        MAIN_CALLING_OTHER,
    );
    let (out, code) = check_in(&dir);
    assert_eq!(code, 0, "same-project mod fn call must pass: {}", out);
}

#[test]
fn entry_call_to_mod_fn_is_allowed() {
    if !tools_available() {
        return;
    }
    let dir = scratch(
        "entry-mod",
        "pub fn go() -> Int = 0\n",
        "import self.helper as h\n\nfn main() -> Unit = println(int.to_string(h.project_only() + h.open()))\n",
    );
    let (out, code) = check_in(&dir);
    assert_eq!(code, 0, "same-project mod fn call from entry must pass: {}", out);
}
