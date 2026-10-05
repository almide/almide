//! Selective imports bind every kind of name a module declares, the same way
//! in the checker and in the lowering.
//!
//! #3384: variant constructors named in a selective import passed
//! `almide check`, then the lowering routed the bare `A(..)` to a module
//! function `t.A` that does not exist (IR verify: call to unknown function).
//! #3388: a top-level `let` named in a selective import was E003, although
//! docs/CHEATSHEET.md documents `import self.classifier.{classify, NUMBERS}`.
//!
//! `spec/integration/modules/selective_ctor_let_test.almd` holds the shapes;
//! `almide test` runs one leg, so this net runs it on both, and re-states the
//! cheatsheet's `import self.<module>.{..}` form in a package.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(almide()).current_dir(dir).args(args).output().expect("spawn almide");
    (
        o.status.success(),
        format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// A package laid out as the cheatsheet's "Project layout" section shows it.
fn cheatsheet_package() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"pk\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(
        root.join("src/t.almd"),
        "type T =\n  | V(Int)\n  | A(T, T)\n\nfn size(t: T) -> Int = match t {\n  V(_) => 1,\n  A(f, a) => size(f) + size(a),\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/classifier.almd"),
        "import self.t.{T}\n\nlet NUMBERS = [1, 2, 3]\nlet ZERO = V(0)\n\nfn classify(n: Int) -> String = if n > 1 then \"big\" else \"small\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.almd"),
        "import self.t.{T, V, A, size}\nimport self.classifier.{classify, NUMBERS, ZERO}\n\n\
         fn mk() -> T = A(V(1), ZERO)\n\n\
         effect fn main() -> Unit = {\n  println(\"${NUMBERS |> list.map((n) => classify(n))} ${size(mk())}\")\n}\n",
    )
    .unwrap();
    dir
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn selective_ctor_and_let_imports_run_on_every_leg() {
    let integration = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration");
    let file = "modules/selective_ctor_let_test.almd";
    let (ok, out) = run(&integration, &["test", file, "--target", "rust"]);
    assert!(ok, "native leg failed:\n{out}");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out) = run(&integration, &["test", file, "--target", "wasm"]);
    assert!(ok, "wasm leg failed:\n{out}");
}

#[test]
fn the_cheatsheet_selective_import_form_checks_clean() {
    let pkg = cheatsheet_package();
    let (ok, out) = run(pkg.path(), &["check", "src/main.almd"]);
    assert!(ok, "the documented `import self.m.{{f, NUMBERS}}` form must check:\n{out}");
    assert!(!out.contains("E003"), "a selectively imported let must bind its bare name:\n{out}");
    // The constructors and the type are spelled bare, which IS the use of
    // `import self.t.{..}` — the import is not reported (and `almide fix`
    // would not delete it).
    assert!(!out.contains("unused import"), "a bare type / ctor spelling uses its selective import:\n{out}");
}

#[cfg_attr(debug_assertions, ignore = "compiles the program (CI: release-shape job)")]
#[test]
fn the_cheatsheet_selective_import_form_runs() {
    let pkg = cheatsheet_package();
    let (ok, out) = run(pkg.path(), &["run", "src/main.almd"]);
    assert!(ok, "native run failed (the #3384 shape ICEd in IR verify):\n{out}");
    assert!(out.contains("[\"small\", \"big\", \"big\"] 2"), "unexpected output:\n{out}");
    if !wasmtime_available() {
        return;
    }
    let (ok, out) = run(pkg.path(), &["run", "src/main.almd", "--target", "wasm"]);
    assert!(ok, "wasm run failed:\n{out}");
    assert!(out.contains("[\"small\", \"big\", \"big\"] 2"), "unexpected wasm output:\n{out}");
}
