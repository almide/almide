//! C-463 (#4006, owner ruling 2026-11-24 W3): the routes the verified native
//! render does not serve stay, declared unverified. Each says so on stderr
//! when used, and the critical profile refuses the one it can name, the rustc
//! `--target wasi` route.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PROGRAM: &str = "fn main() -> Unit = println(\"hi\")\n";

fn stage(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-unverified-route-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("app.almd"), PROGRAM).expect("write");
    dir
}

#[test]
fn the_rust_source_route_warns_that_it_is_unverified() {
    let dir = stage("emit");
    let out = Command::new(almide()).args(["app.almd", "--target", "rust"]).current_dir(&dir).output().expect("spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("warning: `--target rust` source is an unverified route"), "{stderr}");
    assert!(!out.stdout.is_empty(), "the source is still written");
}

#[test]
fn the_critical_profile_refuses_the_rustc_wasi_route_and_admits_the_verified_ones() {
    let dir = stage("critical");
    let check = |args: &[&str]| {
        let out = Command::new(almide()).arg("check").arg("app.almd").args(args).current_dir(&dir).output().expect("spawn");
        (out.status.code(), String::from_utf8_lossy(&out.stderr).to_string())
    };
    let (code, stderr) = check(&["--profile", "critical", "--target", "wasi"]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("error: --profile critical refuses --target wasi"), "{stderr}");
    for target in ["wasm", "native"] {
        let (code, stderr) = check(&["--profile", "critical", "--target", target]);
        assert_eq!(code, Some(0), "--target {target}: {stderr}");
    }
    let (code, stderr) = check(&["--target", "wasi"]);
    assert_eq!(code, Some(0), "without the profile the route is judged: {stderr}");
}
