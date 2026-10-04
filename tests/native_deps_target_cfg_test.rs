//! `[target.'cfg(...)'.native-deps]` reaches Cargo as a target-specific
//! dependency (#3350): a build compiles the crate only when the target matches.
//!
//! `[native-deps]` used to be copied into `[dependencies]` for every target, so
//! a crate that does not build on some target (arboard on Android, jni off it)
//! broke those builds. The probe here is a local path crate whose only content
//! is a `compile_error!`: gated on a cfg the host does not satisfy, the build
//! must succeed (Cargo never compiles it); gated on one the host does satisfy,
//! the build must fail with that crate's error (the gate is a real target
//! test, not a dropped dependency). A host build is a cross build as far as a
//! non-host cfg is concerned, so no cross toolchain is needed. No network: the
//! only dependency is the path crate.

use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER: &str = "almide_issue3350_probe_was_compiled";

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn cargo_available() -> bool {
    Command::new("cargo").arg("--version").output().is_ok()
}

/// A cfg the host satisfies, and one it does not.
fn host_cfg() -> &'static str {
    if cfg!(target_os = "macos") {
        r#"cfg(target_os = "macos")"#
    } else if cfg!(target_os = "windows") {
        r#"cfg(target_os = "windows")"#
    } else {
        r#"cfg(target_os = "linux")"#
    }
}

const NON_HOST_CFG: &str = r#"cfg(any(target_os = "android", target_os = "ios"))"#;

/// A package whose only native dep is the poisoned probe crate, gated on `cfg`.
fn package(tag: &str, cfg: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3350-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let probe = root.join("probe");
    std::fs::create_dir_all(probe.join("src")).expect("mkdir probe");
    std::fs::write(
        probe.join("Cargo.toml"),
        "[package]\nname = \"almide_issue3350_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .expect("write probe manifest");
    std::fs::write(probe.join("src/lib.rs"), format!("compile_error!(\"{MARKER}\");\n")).expect("write probe lib");
    let app = root.join("app");
    std::fs::create_dir_all(app.join("src")).expect("mkdir app");
    let probe_path = probe.to_str().expect("utf-8 path").replace('\\', "/");
    std::fs::write(
        app.join("almide.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [target.'{cfg}'.native-deps]\nalmide_issue3350_probe = {{ path = \"{probe_path}\" }}\n"
        ),
    )
    .expect("write almide.toml");
    std::fs::write(app.join("src/main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").expect("write main");
    app
}

fn build(app: &Path, extra: &[&str]) -> (bool, String) {
    let out = Command::new(almide())
        .args(["build", "src/main.almd"])
        .args(extra)
        .current_dir(app)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .output()
        .expect("spawn almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_dep_gated_on_another_target_is_not_compiled() {
    if !cargo_available() {
        eprintln!("skip: cargo unavailable");
        return;
    }
    for (route, extra) in [("bin", &["-o", "out/app"][..]), ("cdylib", &["--cdylib"][..])] {
        let app = package(&format!("off-{route}"), NON_HOST_CFG);
        let (ok, text) = build(&app, extra);
        assert!(ok && !text.contains(MARKER), "{route}: a dep gated on {NON_HOST_CFG} was compiled for the host:\n{text}");
        let _ = std::fs::remove_dir_all(app.parent().expect("root"));
    }
}

#[test]
fn a_dep_gated_on_the_host_target_is_compiled() {
    if !cargo_available() {
        eprintln!("skip: cargo unavailable");
        return;
    }
    let app = package("on", host_cfg());
    let (ok, text) = build(&app, &["-o", "out/app"]);
    assert!(!ok && text.contains(MARKER), "a dep gated on {} was not compiled for the host:\n{text}", host_cfg());
    let _ = std::fs::remove_dir_all(app.parent().expect("root"));
}

/// The check-time refusal reaches the CLI: `almide check` names the key and
/// its line before any build starts.
#[test]
fn a_malformed_cfg_is_refused_before_building() {
    let app = package("bad", "cfg(target_os = android)");
    let out = Command::new(almide()).args(["check", "src/main.almd"]).current_dir(&app).output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("invalid platform `cfg(target_os = android)`"), "{text}");
    let _ = std::fs::remove_dir_all(app.parent().expect("root"));
}
