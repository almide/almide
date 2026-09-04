//! #1911: checker warnings reach `almide run --target wasm` (and `build
//! --target wasm`) exactly as they reach the native run — a deprecation a
//! program carries must not be visible on one target and silent on the other.

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

#[test]
fn the_wasm_run_prints_the_same_checker_warning_as_native() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("warn.almd");
    std::fs::write(&path, "fn main() -> Unit = {\n  let o: Int? = some(7)\n  println(\"${option.unwrap_or(o, 1)}\")\n}\n").unwrap();
    let native = Command::new(almide_bin()).args(["run"]).arg(&path).output().unwrap();
    let wasm = Command::new(almide_bin()).args(["run"]).arg(&path).args(["--target", "wasm"]).output().unwrap();
    let warn_lines = |b: &[u8]| -> Vec<String> {
        String::from_utf8_lossy(b).lines().filter(|l| l.starts_with("warning[")).map(str::to_string).collect()
    };
    let n = warn_lines(&native.stderr);
    let w = warn_lines(&wasm.stderr);
    assert!(!n.is_empty(), "native must warn (E052): {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(n, w, "the wasm run must carry the same warning lines");
    assert_eq!(String::from_utf8_lossy(&native.stdout), String::from_utf8_lossy(&wasm.stdout));
}
