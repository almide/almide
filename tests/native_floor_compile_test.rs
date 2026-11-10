//! #4006 family D: the native runtime floor derived from declarations
//! (crates/almide-mir/src/native_floor.rs) compiles.
//!
//! A stdlib fn declared `@intrinsic("almide_rt_X")` with a native-typed
//! signature joins the verified native render's floor, and the render carries
//! `almide_rt_X`'s v0 body and every runtime item it reaches. The resolver
//! that picks those items is lexical; this test is what holds it honest: every
//! body the declarations admit is compiled together under the native header
//! with the rustc the CLI uses. A body the resolver let through that rustc
//! rejects fails here, not in a user's `almide build`.

use std::process::Command;

#[test]
fn the_declared_native_floor_compiles() {
    let calls = almide_mir::native_floor::declared_floor_calls();
    eprintln!("declared native floor: {} stdlib fns", calls.len());
    for c in &calls {
        eprintln!("  {c}");
    }
    assert!(!calls.is_empty(), "the declarations admit no native floor fn — the derivation is broken");
    let src = almide_mir::native_floor::declared_floor_source();
    let dir = std::env::temp_dir().join(format!("almide-native-floor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("floor.rs");
    std::fs::write(&file, &src).expect("write the floor source");
    // The admitted floor includes dependency-bearing host operations (TLS).
    // Use the same dependency declaration table as every product build route,
    // compiling the complete emitted floor, including its unreachable bodies.
    let mut manifest = String::from("[package]\nname = \"native-floor-check\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[workspace]\n[[bin]]\nname = \"floor\"\npath = \"floor.rs\"\n[dependencies]\n");
    for (name, spec) in almide_codegen::runtime_crate_deps(&src) {
        if spec.starts_with('{') {
            manifest.push_str(&format!("{name} = {spec}\n"));
        } else {
            manifest.push_str(&format!("{name} = {spec:?}\n"));
        }
    }
    std::fs::write(dir.join("Cargo.toml"), manifest).expect("write floor manifest");
    let out = Command::new(env!("CARGO"))
        .args(["build", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/native-floor-check"))
        .output()
        .expect("compile the floor with its runtime dependencies");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "the declared native floor does not compile:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
