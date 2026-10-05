//! #3396: an imported module's top-level `let` bound to a lambda is E061 at
//! the module's `let`, exactly as when the module is checked on its own.
//!
//! The entry checker (`infer_program`) ran the top-level `let` rules; the
//! module checker (`infer_module`) did not, so `almide run main.almd` reached
//! lowering and died with `internal compiler error: IR verify: call to unknown
//! function 'k.inc'`. Both import spellings — `import k` with `k.inc(..)` and
//! the selective `import k.{inc}` with a bare `inc(..)` — and every driver
//! (`check`, native `run`, wasm `run`) must report the diagnostic instead.

use std::path::Path;
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

fn project(main: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("k.almd"), "let inc = (x: Int) => x + 1\n").unwrap();
    std::fs::write(dir.path().join("main.almd"), main).unwrap();
    dir
}

const QUALIFIED: &str = "import k\n\neffect fn main() -> Unit = println(int.to_string(k.inc(1)))\n";
const SELECTIVE: &str = "import k.{inc}\n\neffect fn main() -> Unit = println(int.to_string(inc(1)))\n";

fn assert_e061(out: &str, how: &str) {
    assert!(!out.contains("internal compiler error"), "{how}: an ICE instead of E061:\n{out}");
    assert!(out.contains("E061"), "{how}: expected E061 for the module's lambda-valued let:\n{out}");
    assert!(out.contains("k.almd"), "{how}: E061 must point at the module's file:\n{out}");
}

#[test]
fn check_reports_e061_in_an_imported_module() {
    for (name, main) in [("import k", QUALIFIED), ("import k.{inc}", SELECTIVE)] {
        let p = project(main);
        let (ok, out) = run(p.path(), &["check", "main.almd"]);
        assert!(!ok, "{name}: check must fail:\n{out}");
        assert_e061(&out, &format!("check via `{name}`"));
    }
}

#[test]
fn run_reports_e061_in_an_imported_module_on_both_targets() {
    for (name, main) in [("import k", QUALIFIED), ("import k.{inc}", SELECTIVE)] {
        let p = project(main);
        for target in ["rust", "wasm"] {
            let (ok, out) = run(p.path(), &["run", "main.almd", "--target", target]);
            assert!(!ok, "{name} / {target}: run must fail:\n{out}");
            assert_e061(&out, &format!("run --target {target} via `{name}`"));
        }
    }
}
