//! Emit-time wasm validation (the grain pattern, Survey 4 law 2): the CLI
//! validates every assembled module with `wasmparser::validate` BEFORE
//! writing or running it, so a renderer bug that types out becomes a named
//! compiler wall ("emitted wasm failed validation — this is an Almide
//! bug", with offset and function) instead of a wasmtime translation
//! error at load.
//!
//! The probe program is almide#1431's minimal repro (bare `err(..)` match
//! subject inside a non-main effect fn — invalid i32/i64 at render, as of
//! 0.57.x). The assertion is deliberately fix-proof: EITHER the build
//! fails with the named wall (today), OR it succeeds and the bytes on
//! disk validate (after #1431 lands). What may never happen: invalid
//! bytes reaching disk, or a failure that is not the named wall.

use std::fs;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const REPRO_1431: &str = r#"effect fn e0(x: Int) -> Int = {
  match err("charlie") {
    ok(_v) => println("no"),
    err(_e) => println("yes"),
  }
  7
}

effect fn main() -> Unit = {
  match e0(0) {
    ok(_v) => println("done"),
    err(_e) => println("bad"),
  }
}
"#;

#[test]
fn invalid_emitted_wasm_is_a_named_wall_never_bytes_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro1431.almd");
    let out = dir.path().join("repro1431.wasm");
    fs::write(&src, REPRO_1431).expect("write repro");

    let result = Command::new(almide())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("run almide build --target wasm");

    if result.status.success() {
        // #1431 has been fixed: the module must actually be valid.
        let bytes = fs::read(&out).expect("built wasm must exist on success");
        wasmparser::validate(&bytes)
            .expect("build succeeded but wrote invalid wasm — emit-time validation is not running");
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("emitted wasm failed validation — this is an Almide bug"),
            "build failed, but not through the named validation wall; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains("offset") && stderr.contains("function"),
            "the validation wall must locate the failure (offset + function); stderr:\n{stderr}"
        );
        assert!(
            !out.exists(),
            "the wall fired but invalid bytes still reached disk"
        );
    }
}
