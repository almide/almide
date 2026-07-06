//! `[package].almide` compiler-version pin (roadmap: compiler-version-pin).
//!
//! All checks go through the env-free `check_compiler_version_with(project,
//! skip)` core — process env is process-GLOBAL, and parallel `cargo test`
//! threads racing on `set_var`/`remove_var` made env-reading sibling tests
//! flaky (the recurring `check_rejects_malformed_pin` CI failure). The thin
//! env-reading wrapper `check_compiler_version` stays untested here by design:
//! it only reads the var and delegates.

use almide::project::{parse_toml, check_compiler_version_with, Package, Project};

fn write_toml(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let p = dir.join("almide.toml");
    std::fs::write(&p, body).unwrap();
    p
}

fn mk_project(almide_min: Option<&str>) -> Project {
    Project {
        package: Package {
            name: "t".into(),
            version: "0.1.0".into(),
            almide_min: almide_min.map(String::from),
        },
        dependencies: vec![],
        permissions: vec![],
        native_deps: vec![],
        root: std::path::PathBuf::new(),
    }
}

#[test]
fn parse_reads_almide_field() {
    let td = tempfile::TempDir::new().unwrap();
    let path = write_toml(td.path(), "[package]\nname = \"x\"\nversion = \"0.1.0\"\nalmide = \"0.14.0\"\n");
    let p = parse_toml(&path).unwrap();
    assert_eq!(p.package.almide_min.as_deref(), Some("0.14.0"));
}

#[test]
fn parse_omitted_field_is_none() {
    let td = tempfile::TempDir::new().unwrap();
    let path = write_toml(td.path(), "[package]\nname = \"x\"\nversion = \"0.1.0\"\n");
    let p = parse_toml(&path).unwrap();
    assert!(p.package.almide_min.is_none());
}

#[test]
fn check_passes_on_sufficient_version() {
    // Installed version is env!("CARGO_PKG_VERSION"). Pin it to 0.0.1 → always OK.
    let p = mk_project(Some("0.0.1"));
    assert!(check_compiler_version_with(&p, false).is_ok());
}

#[test]
fn check_skipped_when_field_omitted() {
    let p = mk_project(None);
    assert!(check_compiler_version_with(&p, false).is_ok());
}

#[test]
fn check_errors_on_insufficient_version() {
    let p = mk_project(Some("99.0.0"));
    let err = check_compiler_version_with(&p, false).unwrap_err();
    assert!(err.contains("requires almide >= 99.0.0"), "msg:\n{}", err);
    assert!(err.contains("installed version"), "msg:\n{}", err);
}

#[test]
fn check_skip_bypasses_insufficient_version() {
    let p = mk_project(Some("99.0.0"));
    assert!(check_compiler_version_with(&p, true).is_ok(), "skip should bypass");
}

#[test]
fn check_rejects_malformed_pin() {
    let p = mk_project(Some("not-a-version"));
    let err = check_compiler_version_with(&p, false).unwrap_err();
    assert!(err.contains("invalid"), "msg:\n{}", err);
}

#[test]
fn check_skip_bypasses_malformed_pin() {
    // The bypass short-circuits BEFORE pin parsing — same as the wrapper with
    // ALMIDE_SKIP_VERSION_CHECK set.
    let p = mk_project(Some("not-a-version"));
    assert!(check_compiler_version_with(&p, true).is_ok());
}
