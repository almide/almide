//! #1423 stage 3 — the check-time availability diagnostic (E081): a call
//! to a declared BOTH-LEGS-wall fn on `--target wasm` errs at check time
//! with the reason (and alternative where declared), instead of the late
//! generic render wall. E081 fires only on the wasm target and lives in
//! the BUILD path, so it is pinned here rather than through the
//! check-harness fixtures (the E054 precedent).

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

const UNAVAILABLE: &str = r#"import http

effect fn main() -> Unit = {
  let r = http.get("https://example.com")!
  println(r)
}
"#;

const AVAILABLE: &str = r#"import fs

effect fn main() -> Unit = {
  println(if fs.exists("x") then "y" else "n")
}
"#;

#[test]
fn declared_unavailable_call_errs_at_check_time_with_the_reason() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = std::env::temp_dir().join("almide-e081");
    std::fs::create_dir_all(&d).expect("mkdir");
    let src = d.join("net.almd");
    std::fs::write(&src, UNAVAILABLE).expect("write");
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", "/dev/null"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!o.status.success(), "the unavailable call must refuse:\n{log}");
    assert!(log.contains("error[E081]"), "must carry the E081 code:\n{log}");
    assert!(log.contains("`http.get` is not available on --target wasm"), "must name the fn:\n{log}");
    assert!(log.contains("reason:"), "must carry the declared reason:\n{log}");
    assert!(
        log.contains("target-availability.toml"),
        "must point at the availability matrix:\n{log}"
    );

    // The native target is untouched.
    let o = Command::new(almide_bin())
        .args(["check", src.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(o.status.success(), "the native check must stay clean");
}

#[test]
fn available_fs_calls_stay_clean_on_wasm() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = std::env::temp_dir().join("almide-e081");
    std::fs::create_dir_all(&d).expect("mkdir");
    let src = d.join("fsok.almd");
    std::fs::write(&src, AVAILABLE).expect("write");
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", "/dev/null"])
        .output()
        .expect("spawn");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "an available program must build:\n{log}");
    assert!(!log.contains("E081"), "no false E081:\n{log}");
}
