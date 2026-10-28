//! #4183: `almide install` puts the binary only at `<bin dir>/<plain name>`,
//! never writes through a symlink there, and does not replace a file it did
//! not install unless `--force` is given.
//!
//! Every case installs a local project with `--target wasm` (no cargo build)
//! into a scratch bin dir. Each refusal has its control: a plain name into an
//! empty bin dir installs, and reinstalling the same package replaces its own
//! binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn project(root: &Path, name: &str) -> PathBuf {
    let p = root.join(format!("proj-{}", name.replace(['/', '.'], "_")));
    std::fs::create_dir_all(p.join("src")).unwrap();
    std::fs::write(p.join("almide.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")).unwrap();
    std::fs::write(p.join("src/main.almd"), "effect fn main() -> Unit = println(\"hi\")\n").unwrap();
    p
}

fn install(root: &Path, proj: &Path, bin: &Path, extra: &[&str]) -> Output {
    Command::new(almide())
        .arg("install")
        .arg(proj)
        .args(["--bin-dir", bin.to_str().unwrap(), "--target", "wasm"])
        .args(extra)
        .current_dir(root)
        .env("HOME", root.join("home"))
        .env_remove("ALMIDE_INSTALL")
        .output()
        .expect("spawn almide install")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn is_wasm(p: &Path) -> bool {
    std::fs::read(p).map(|b| b.starts_with(b"\0asm")).unwrap_or(false)
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let bin = td.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    (td, bin)
}

#[test]
fn a_name_that_is_a_path_is_refused_and_nothing_is_written() {
    let (td, bin) = setup();
    for name in ["../outside/pwned", "sub/tool", ".hidden"] {
        let p = project(td.path(), name);
        let o = install(td.path(), &p, &bin, &[]);
        assert_eq!(o.status.code(), Some(1), "{name}: {}", text(&o));
        assert!(text(&o).contains("is not a plain name"), "{name}: {}", text(&o));
    }
    assert!(!td.path().join("outside").exists(), "nothing may be created outside the bin dir");
    // `--name` is held to the same rule.
    let p = project(td.path(), "tool");
    let o = install(td.path(), &p, &bin, &["--name", "../escaped"]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(!td.path().join("escaped").exists());
    assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 0, "the bin dir stays empty");
}

#[test]
fn a_plain_name_installs_and_reinstalls() {
    let (td, bin) = setup();
    let p = project(td.path(), "tool");
    let o = install(td.path(), &p, &bin, &[]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(is_wasm(&bin.join("tool")));
    // Its own binary is replaced on reinstall, without --force.
    let o = install(td.path(), &p, &bin, &[]);
    assert!(o.status.success(), "reinstall: {}", text(&o));
    assert!(is_wasm(&bin.join("tool")));
}

#[test]
fn a_file_almide_did_not_install_is_kept_without_force() {
    let (td, bin) = setup();
    std::fs::write(bin.join("almide"), "#!/bin/sh\necho the real one\n").unwrap();
    let p = project(td.path(), "almide");
    let o = install(td.path(), &p, &bin, &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("was not installed by almide"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(bin.join("almide")).unwrap(), "#!/bin/sh\necho the real one\n");
    // --force replaces it, and says so.
    let o = install(td.path(), &p, &bin, &["--force"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("replacing"), "{}", text(&o));
    assert!(is_wasm(&bin.join("almide")));
}

#[cfg(unix)]
#[test]
fn a_symlink_at_the_destination_is_never_written_through() {
    let (td, bin) = setup();
    let victim = td.path().join("victim");
    std::fs::write(&victim, "untouched").unwrap();
    std::os::unix::fs::symlink(&victim, bin.join("tool")).unwrap();
    let p = project(td.path(), "tool");
    let o = install(td.path(), &p, &bin, &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("is a symlink"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
    // With --force the LINK is replaced by the binary; its target is untouched.
    let o = install(td.path(), &p, &bin, &["--force"]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched", "the link's target must never be written");
    let md = std::fs::symlink_metadata(bin.join("tool")).unwrap();
    assert!(md.file_type().is_file() && is_wasm(&bin.join("tool")));
}
