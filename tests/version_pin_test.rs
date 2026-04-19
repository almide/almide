//! `[package].almide` compiler-version pin (roadmap: compiler-version-pin).

use almide::project::{parse_toml, check_compiler_version, Package, Project};

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
    assert!(check_compiler_version(&p).is_ok());
}

#[test]
fn check_errors_on_insufficient_version() {
    let p = mk_project(Some("99.0.0"));
    let err = check_compiler_version(&p).unwrap_err();
    assert!(err.contains("requires almide >= 99.0.0"), "msg:\n{}", err);
    assert!(err.contains("installed version"), "msg:\n{}", err);
}

#[test]
fn check_skipped_when_field_omitted() {
    let p = mk_project(None);
    assert!(check_compiler_version(&p).is_ok());
}

#[test]
fn check_bypassed_by_env_var() {
    let p = mk_project(Some("99.0.0"));
    // SAFETY: test is not concurrent-safe on shared env; we set and clear.
    // Alternative is a separate binary fixture, but this is smaller.
    unsafe {
        std::env::set_var("ALMIDE_SKIP_VERSION_CHECK", "1");
    }
    let ok = check_compiler_version(&p).is_ok();
    unsafe {
        std::env::remove_var("ALMIDE_SKIP_VERSION_CHECK");
    }
    assert!(ok, "skip env var should bypass");
}

#[test]
fn check_rejects_malformed_pin() {
    let p = mk_project(Some("not-a-version"));
    let err = check_compiler_version(&p).unwrap_err();
    assert!(err.contains("invalid"), "msg:\n{}", err);
}
