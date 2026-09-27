//! A module's type that names a sibling type declared further down resolves
//! to the module's own type, whatever order the program's modules register in.
//!
//! Registration used to resolve each type body in source order. So
//! `type R = { u: U }` above `type U = { .. }` found no `m.U` yet, the
//! reference stayed a bare `U`, and the bare key belongs to whichever
//! module's `U` registered last. That order follows import order and
//! directory order, so the same program checked on macOS and failed on
//! Linux (almide-dojo's CI: almai declares `Usage` in eight modules, and
//! `LLMResponse.usage` resolved to `core.Usage` with E013 on
//! `completion_tokens`). These cells check every import order of three
//! modules that all declare `U`, so no single order can hide the dependence.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const USER_MODULE: &str = "type R = { u: U, tag: String }\n\n\
fn mk() -> R = R { u: U { total: 3 }, tag: \"t\" }\n\
fn total(r: R) -> Int = r.u.total\n\n\
// declared AFTER its use in R\n\
type U = { total: Int }\n";

/// A package whose modules `a` and `b` declare their own `U` beside `m`'s,
/// with `main` importing the three in `order`.
fn project(order: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"fwd\"\nversion = \"0.1.0\"\n").expect("manifest");
    for other in ["a", "b"] {
        std::fs::write(
            src.join(format!("{other}.almd")),
            format!("type U = {{ {other}_only: Int }}\nfn mk() -> U = U {{ {other}_only: 1 }}\n"),
        )
        .expect("module");
    }
    std::fs::write(src.join("m.almd"), USER_MODULE).expect("module m");
    let imports: String = order.iter().map(|m| format!("import self.{m}\n")).collect();
    std::fs::write(
        src.join("main.almd"),
        format!(
            "{imports}\neffect fn main() -> Unit = {{\n  println(int.to_string(m.total(m.mk())))\n  \
             println(int.to_string(a.mk().a_only + b.mk().b_only))\n}}\n"
        ),
    )
    .expect("main");
    dir
}

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(args).output().expect("run almide");
    (
        out.status.success(),
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
    )
}

const ORDERS: [[&str; 3]; 6] = [
    ["a", "b", "m"],
    ["a", "m", "b"],
    ["b", "a", "m"],
    ["b", "m", "a"],
    ["m", "a", "b"],
    ["m", "b", "a"],
];

#[test]
fn a_forward_reference_to_an_own_type_checks_in_every_import_order() {
    for order in ORDERS {
        let dir = project(&order);
        let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
        assert!(ok, "import order {order:?}: the program must check, got:\n{text}");
    }
}

#[test]
fn a_forward_reference_to_an_own_type_runs_its_own_type_on_both_legs() {
    let wasmtime = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    // The order that picked `a.U` on macOS before the fix.
    let dir = project(&["a", "m", "b"]);
    let (ok, native) = run(dir.path(), &["run", "src/main.almd"]);
    assert!(ok, "native run failed:\n{native}");
    assert!(native.ends_with("3\n2\n"), "native printed:\n{native}");
    if wasmtime {
        let (ok, wasm) = run(dir.path(), &["run", "src/main.almd", "--target", "wasm"]);
        assert!(ok, "wasm run failed:\n{wasm}");
        assert!(wasm.ends_with("3\n2\n"), "wasm printed:\n{wasm}");
    }
}
