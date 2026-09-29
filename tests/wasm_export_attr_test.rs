//! #2752: `@export(wasm, "sym")` on the structural wasm leg. The attribute
//! used to route the whole module to the incumbent renderer (the structural
//! leg had no export mode); it now names the export on the one leg:
//!
//! - the fn is exported as `sym`, and not under its own name;
//! - a DCE root like any exported `pub fn` (#457), callable by the host;
//! - a declared export that does not lower refuses the module (E082) instead
//!   of shipping an artifact silently missing the entry point the host calls;
//! - two exports claiming one name are a wall, never a silent dedup.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn build(src: &str) -> (bool, Vec<u8>, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("lib.almd");
    let wasm = dir.path().join("lib.wasm");
    std::fs::write(&file, src).expect("source");
    let out = Command::new(almide())
        .args(["build", file.to_str().unwrap(), "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    let bytes = std::fs::read(&wasm).unwrap_or_default();
    (out.status.success(), bytes, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn exports(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ExportSection(section) = payload.expect("parse") {
            for export in section {
                names.push(export.expect("export").name.to_string());
            }
        }
    }
    names
}

#[test]
fn the_declared_symbol_names_the_export() {
    let src = "@export(wasm, \"render\")\nfn draw(t: Int) -> Int = t * 2\n\nfn helper(x: Int) -> Int = x + 1\n\neffect fn main() -> Unit = println(\"${helper(1)}\")\n";
    let (ok, bytes, err) = build(src);
    assert!(ok, "build failed:\n{err}");
    assert!(err.contains("structural leg"), "the module comes from the one wasm leg:\n{err}");
    let names = exports(&bytes);
    assert!(names.iter().any(|n| n == "render"), "`render` must be exported: {names:?}");
    assert!(!names.iter().any(|n| n == "draw"), "the fn's own name is not the export: {names:?}");
    // Callable by the host: a DCE root, not a stub.
    if Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        let dir = tempfile::tempdir().expect("tempdir");
        let wasm = dir.path().join("lib.wasm");
        std::fs::write(&wasm, &bytes).unwrap();
        let out = Command::new("wasmtime").args(["run", "--invoke", "render"]).arg(&wasm).arg("21").output().expect("wasmtime");
        assert!(out.status.success(), "invoking `render` trapped:\n{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
    }
}

#[test]
fn a_declared_export_that_does_not_lower_refuses_the_module() {
    // A mut op through a deeper field path: the leg refuses it on purpose
    // (tests/wasm_wall_e082_test.rs pins the spelling).
    let src = "type Inner = { xs: List[Int] }\ntype Outer = { inner: Inner }\n\n@export(wasm, \"grow\")\nfn grow(n: Int) -> Int = {\n  var o = Outer { inner: Inner { xs: [n] } }\n  list.push(o.inner.xs, 2)\n  list.len(o.inner.xs)\n}\n\neffect fn main() -> Unit = println(\"main\")\n";
    let (ok, bytes, err) = build(src);
    assert!(!ok && bytes.is_empty(), "a declared export that does not lower must refuse the build:\n{err}");
    assert!(err.contains("error[E082]") && err.contains("exported function `grow`"), "{err}");
}

#[test]
fn two_exports_of_one_name_are_a_wall() {
    let src = "@export(wasm, \"f\")\nfn a(x: Int) -> Int = x\n\n@export(wasm, \"f\")\nfn b(x: Int) -> Int = x + 1\n\neffect fn main() -> Unit = println(\"${a(1)} ${b(1)}\")\n";
    let (ok, _, err) = build(src);
    assert!(!ok, "a duplicate export name is an invalid module, never shipped:\n{err}");
    assert!(err.contains("duplicate wasm export name `f`"), "{err}");
}
