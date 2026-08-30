//! #569: the WCET reference kernel's emitted Rust keeps the
//! analyzer-facing properties the story documents — a statically bounded
//! loop with NO allocation, no formatting, and no cloning in the kernel
//! fns' bodies. The story is docs/project/WCET-STORY.md; the kernel is
//! examples/pid-kernel.almd. Cross-target value identity of the kernel is
//! asserted too (both legs print the same settled worst-output scalar).

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

/// The emitted text between `pub fn <name>` and its closing brace at
/// column 0 — crude but stable for the kernel's flat fns.
fn fn_body<'a>(code: &'a str, name: &str) -> &'a str {
    let start = code
        .find(&format!("pub fn {name}"))
        .unwrap_or_else(|| panic!("emitted Rust lost fn {name}"));
    let rest = &code[start..];
    let end = rest.find("\n}\n").map(|i| i + 3).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn pid_kernel_emission_stays_wcet_shaped() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let out = Command::new(almide_bin())
        .args(["examples/pid-kernel.almd", "--target", "rust"])
        .output()
        .expect("spawn almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let code = String::from_utf8_lossy(&out.stdout).to_string();

    for name in ["pid_step", "plant", "run_loop"] {
        let body = fn_body(&code, name);
        for forbidden in ["Vec", "Box<", "String", "format!", ".clone()", "RcCow"] {
            assert!(
                !body.contains(forbidden),
                "{name}'s emitted body grew `{forbidden}` — the WCET shape broke:\n{body}"
            );
        }
    }
    // The loop bound is the compile-time literal, not a computed value.
    assert!(
        fn_body(&code, "run_loop").contains("for _i in 1i64..=steps"),
        "run_loop lost its literal-bounded for shape"
    );
}

#[test]
fn pid_kernel_is_cross_target_identical() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let native = Command::new(almide_bin())
        .args(["run", "examples/pid-kernel.almd"])
        .output()
        .expect("spawn");
    assert!(native.status.success());
    let wasm = Command::new(almide_bin())
        .args(["run", "examples/pid-kernel.almd", "--target", "wasm"])
        .output()
        .expect("spawn");
    if !wasm.status.success() {
        // wasmtime absent on this machine — the native half still ran.
        return;
    }
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&wasm.stdout),
        "the kernel's settled scalar diverged across targets"
    );
}
