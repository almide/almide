//! `[permissions]` is a property of the program, not of the target (#3275).
//!
//! The capability gate ran in the native build pipeline and in `check`, but
//! `build --target wasm` and `run --target wasm` never called it, so a project
//! that allows only `Env` built a wasm module that read a file. Both build
//! routes now pass through `compile_driver::optimize_gate_and_link`, which
//! runs the gate on the same post-optimize, pre-mono IR. These tests pin that
//! the same project gets the same verdict, with the same text, on both
//! targets.

use std::path::Path;
use std::process::{Command, Output};

/// The #3275 repro: the read happens in a lambda built by an effect fn and
/// called from a pure one, so only the effect inference (not E006) sees it.
const READS_A_FILE: &str = "import fs\n\n\
    effect fn make() -> (String) -> String = (p) => fs.read_text(p) ?? \"\"\n\n\
    fn use_it(f: (String) -> String) -> String = f(\"notes.txt\")\n\n\
    effect fn main() -> Unit = {\n  let f = make()!\n  println(use_it(f))\n}\n";

fn project(dir: &Path, allow: &str) {
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"permtargets\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("main.almd"), READS_A_FILE).unwrap();
    std::fs::write(dir.join("notes.txt"), "noted").unwrap();
}

fn almide(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(dir).output().expect("almide runs")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// The violation report, without whatever a route prints around it.
fn violations(o: &Output) -> Vec<String> {
    stderr(o)
        .lines()
        .filter(|l| l.contains("capability violation") || l.contains("is not in [permissions].allow"))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_disallowed_effect_is_refused_by_the_native_and_the_wasm_build_alike() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "\"Env\"");
    let native = almide(td.path(), &["build", "main.almd", "-o", "app"]);
    let wasm = almide(td.path(), &["build", "main.almd", "--target", "wasm", "-o", "app.wasm"]);
    for (route, o) in [("build", &native), ("build --target wasm", &wasm)] {
        assert!(!o.status.success(), "`almide {route}` built a program that reads a file under allow = [\"Env\"]:\n{}", stderr(o));
        assert!(
            stderr(o).contains("error: capability violation in `main`") && stderr(o).contains("IO is not in [permissions].allow"),
            "`almide {route}` should name the fn and the capability:\n{}",
            stderr(o)
        );
    }
    assert!(!td.path().join("app.wasm").exists(), "the refused wasm build still wrote its module");
    assert_eq!(violations(&native), violations(&wasm), "the two targets must report the same violations");

    // `run --target wasm` builds through the same route, and must not run the read.
    let run = almide(td.path(), &["run", "main.almd", "--target", "wasm"]);
    assert!(!run.status.success(), "`run --target wasm` ran the read:\n{}", stderr(&run));
    assert!(!String::from_utf8_lossy(&run.stdout).contains("noted"), "`run --target wasm` printed the file");
    assert_eq!(violations(&native), violations(&run));
}

#[test]
fn an_allowed_effect_builds_on_both_targets() {
    let td = tempfile::TempDir::new().unwrap();
    project(td.path(), "\"Env\", \"IO\"");
    let native = almide(td.path(), &["build", "main.almd", "-o", "app"]);
    assert!(native.status.success(), "native build refused an allowed program:\n{}", stderr(&native));
    let wasm = almide(td.path(), &["build", "main.almd", "--target", "wasm", "-o", "app.wasm"]);
    assert!(wasm.status.success(), "wasm build refused an allowed program:\n{}", stderr(&wasm));
    assert!(td.path().join("app.wasm").exists(), "the wasm build wrote no module");
}
