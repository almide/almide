//! A program's per-thread globals share ONE `thread_local!` (#3347).
//!
//! Every top-level `var` (a `Cell` / `RefCell<Rc<_>>` slot) and every
//! closure-holding top-level `let` (#2537) used to get its own
//! `thread_local!`. On targets without native TLS each one is an OS TLS key
//! (Android's bionic allows ~128 per process), so a large program aborted at
//! start-up with "out of TLS keys". They are now fields of one struct in one
//! `thread_local!`, whatever the program's size.
//!
//! The pin: a program with 300 top-level vars, a List var and a closure-holding let
//! emits exactly one `thread_local!` in user code, compiles warning-free, and
//! prints the same output natively and on wasm.

use std::path::PathBuf;
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-3347-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

const VARS: usize = 300;

fn program() -> String {
    let decls: String = (0..VARS).map(|i| format!("var g{i}: Int = {i}\n")).collect();
    let bumps: String = (0..VARS).map(|i| format!("  g{i} = g{i} + 1\n")).collect();
    let sum: String = (0..VARS).map(|i| format!("  total = total + g{i}\n")).collect();
    format!(
        "type Step = {{ run: (Int) -> Int }}\n\n{decls}var seen: List[Int] = []\n\
         let STEPS = [Step {{ run: (x) => x + 1 }}]\n\n\
         effect fn main() -> Unit = {{\n{bumps}  var total = 0\n{sum}  seen = seen + [list.fold(STEPS, 41, (acc, s) => s.run(acc))]\n  seen = seen + [g299]\n  \
         println(\"total=${{total}} seen=${{list.len(seen)}} first=${{list.first(seen) ?? 0}} last=${{list.last(seen) ?? 0}}\")\n}}\n"
    )
}

const EXPECTED: &str = "total=45150 seen=2 first=42 last=300\n";

#[test]
fn three_hundred_globals_take_one_thread_local() {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("skip: rustc unavailable");
        return;
    }
    let dir = scratch();
    let almd = dir.join("main.almd");
    std::fs::write(&almd, program()).expect("write source");

    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let full = String::from_utf8_lossy(&emitted.stdout).into_owned();
    let user = full.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap();
    let n = user.matches("thread_local!").count();
    assert_eq!(n, 1, "expected exactly one thread_local! in user code, found {n}");

    let rs = dir.join("main.rs");
    std::fs::write(&rs, &full).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "the generated Rust does not compile warning-free:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let native = Command::new(&bin).output().expect("run binary");
    assert_eq!(String::from_utf8_lossy(&native.stdout), EXPECTED, "native output");

    let wasm = Command::new(almide_bin()).arg("run").arg(&almd).args(["--target", "wasm"]).output().expect("spawn almide run");
    assert!(wasm.status.success(), "wasm run failed:\n{}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(String::from_utf8_lossy(&wasm.stdout), EXPECTED, "wasm output");
    let _ = std::fs::remove_dir_all(&dir);
}
