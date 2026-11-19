//! Minimal Version Selection over a diamond (#1458).
//!
//! `docs/specs/package-system.md` documents MVS ("keep maximum requested
//! version"); the resolver implemented first-wins — whichever requirement the
//! depth-first walk met first was fetched, and the other was silently
//! dropped, so the build changed with declaration order. Pinned here with a
//! diamond over ONE repository at two tags: the app asks for alib v1.2.0
//! directly and for blib, which asks for alib v1.5.0. Selection must be
//! 1.5.0 in BOTH declaration orders.
//!
//! (Before #4834 this used two path directories as the two versions. Two
//! directories are two packages that happen to declare one name, which is
//! now refused — tests/dep_package_identity_test.rs — so MVS is pinned where
//! it applies: versions of one source.)

#[path = "support/git_host.rs"]
mod git_host;

use std::path::{Path, PathBuf};
use std::process::Command;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// The diamond under a fresh root; `alib_first` puts alib before blib.
fn scratch(name: &str, alib_first: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1458-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let alib = root.join("alib");
    for ver in ["1.2.0", "1.5.0"] {
        write(&alib.join("almide.toml"), &format!("[package]\nname = \"alib\"\nversion = \"{ver}\"\n"));
        write(&alib.join("src/mod.almd"), &format!("fn version() -> String = \"{ver}\"\n"));
        if ver == "1.2.0" {
            git_host::git(&alib, &["init", "-q"]);
        }
        git_host::git(&alib, &["add", "-A"]);
        git_host::git(&alib, &["commit", "-qm", ver]);
        git_host::git(&alib, &["tag", &format!("v{ver}")]);
    }
    let url = git_host::shared().url_of(&alib);
    write(
        &root.join("b/almide.toml"),
        &format!("[package]\nname = \"blib\"\nversion = \"0.1.0\"\n\n[dependencies]\nalib = {{ git = \"{url}\", tag = \"v1.5.0\" }}\n"),
    );
    write(&root.join("b/src/mod.almd"), "fn go() -> Int = 1\n");
    let a_line = format!("alib = {{ git = \"{url}\", tag = \"v1.2.0\" }}\n");
    let b_line = format!("blib = {{ path = \"{}\" }}\n", root.join("b").display());
    let deps = if alib_first { format!("{a_line}{b_line}") } else { format!("{b_line}{a_line}") };
    write(&root.join("app/almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"));
    write(&root.join("app/src/main.almd"), "import alib\neffect fn main() -> Unit = println(alib.version())\n");
    root
}

fn selected_alib(root: &Path) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(root.join("app"))
        .env("HOME", root.join("home"))
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .args(["run", "src/main.almd", "--target", "wasm"]);
    let o = git_host::shared().env(&mut cmd).output().expect("spawn almide");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn the_maximum_requested_version_wins() {
    let root = scratch("fwd", true);
    assert_eq!(selected_alib(&root), "1.5.0");
    let _ = std::fs::remove_dir_all(&root);
}

/// The other declaration order — the case first-wins got wrong: the direct
/// 1.2.0 requirement was met first and 1.5.0 was silently dropped.
#[test]
fn selection_is_declaration_order_independent() {
    let root = scratch("rev", false);
    assert_eq!(selected_alib(&root), "1.5.0");
    let _ = std::fs::remove_dir_all(&root);
}
