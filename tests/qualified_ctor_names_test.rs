//! #3176: a module-qualified constructor or record name resolves inside the
//! named module alone. `draw.Pane { id: 2, w: 3 }` was rejected with E021
//! because the constructor table, keyed by the bare name, answered with
//! `shapes.Tree`'s `| Pane`. `spec/integration/qualified_ctor_names` holds the
//! family: a qualified record beside another module's case and the reverse,
//! two modules' same-named cases, qualified patterns nested in `Option` and
//! tuples, ctor values, per-module field defaults and boxed fields, a
//! module's own names, and the entry program's types shadowing an import's
//! constructors. Several cells failed only on the native leg (generated Rust
//! that named the wrong enum), and `almide test` runs the wasm leg first, so
//! this net runs the project's `src/main.almd` on both legs and demands one
//! output.
//!
//! `tests/fixtures/issue3176_ambiguous` is the unqualified cell: a bare
//! record-case literal two imported modules declare is E019, not a silent
//! pick of whichever module registered first.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "1 2 3
5 6 t
5 7 p
3 z 1z
5
1 first 2 second
3 7 12
8 in 10 k
q lv 3 entry 11 4 first";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide(dir: PathBuf, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("spawn almide");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn run(target: &str) -> (bool, String, String) {
    almide(root().join("spec/integration/qualified_ctor_names"), &["run", "src/main.almd", "--target", target])
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_qualified_constructor_resolves_inside_its_module_on_every_leg() {
    let (ok, out, err) = run("rust");
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out, err) = run("wasm");
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "wasm leg");
}

#[test]
fn a_bare_record_case_two_imports_declare_is_ambiguous() {
    let (ok, out, err) = almide(root().join("tests/fixtures/issue3176_ambiguous"), &["check", "src/main.almd"]);
    let all = format!("{out}{err}");
    assert!(!ok, "check accepted an ambiguous bare record case:\n{all}");
    assert!(all.contains("E019") && all.contains("ambiguous constructor 'Leaf'"), "expected E019 on `Leaf`:\n{all}");
    assert!(all.contains("`shapes.Leaf` or `other.Leaf`"), "expected the qualified spellings in the hint:\n{all}");
}
