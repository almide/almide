//! `almide compile` rewrites its `.almdi` artifact whenever the artifact would
//! differ, not only when the module's own source text changed (#3096).
//!
//! Freshness used to be a hash of the module's source alone, so a change to
//! anything else the artifact carries — the package version written into its
//! interface, an imported module's types, the compiler — printed "is up to
//! date" and left the stale file in place.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// `almide compile` in `dir`: (stderr, the artifact's bytes).
fn compile(dir: &Path) -> (String, Vec<u8>) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .arg("compile")
        .output()
        .expect("spawn almide compile");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(out.status.success(), "almide compile failed\n{stderr}");
    let artifact = std::fs::read(dir.join("target/compile/lib3096.almdi")).expect("artifact written");
    (stderr, artifact)
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn a_version_bump_alone_rewrites_the_artifact_and_an_unchanged_build_does_not() {
    let dir = std::env::temp_dir().join(format!("almide-issue3096-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(dir.join("almide.toml"), "[package]\nname = \"lib3096\"\nversion = \"0.1.0\"\n");
    write(dir.join("src/mod.almd"), "fn add(a: Int, b: Int) -> Int = a + b\n");

    let (_, first) = compile(&dir);
    assert!(contains(&first, "\"version\":\"0.1.0\""), "the interface carries the package version");

    // Nothing changed: the artifact is fresh and left alone.
    let (stderr, again) = compile(&dir);
    assert!(stderr.contains("is up to date"), "an unchanged build rewrote the artifact\n{stderr}");
    assert_eq!(first, again);

    // Only the package version changes; the module source is untouched.
    write(dir.join("almide.toml"), "[package]\nname = \"lib3096\"\nversion = \"0.2.0\"\n");
    let (stderr, bumped) = compile(&dir);
    assert!(!stderr.contains("is up to date"), "a version bump was reported up to date\n{stderr}");
    assert!(contains(&bumped, "\"version\":\"0.2.0\""), "the artifact kept the old version");

    let _ = std::fs::remove_dir_all(&dir);
}
