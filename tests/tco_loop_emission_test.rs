//! The tail-call loop is `loop { … }`, never `while true { … }` (#3460).
//!
//! `TailCallOpt` builds its loop as an IR `While` over the literal `true`, and
//! the walker spelled it `while true`, which rustc's `while_true` lint warns
//! on. A literal-true `While` now renders as `loop`. The pin: three TCO shapes
//! (plain, binding a closure, a source `while true` with a `break`) emit no
//! `while true`, compile warning-free and print the expected value.

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn emit_compile_run(name: &str, src: &str) -> (String, String) {
    let dir = std::env::temp_dir().join(format!("almide-3460-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, src).expect("write source");
    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "{name}: emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8_lossy(&emitted.stdout).into_owned();
    let rs = dir.join("main.rs");
    std::fs::write(&rs, &rust).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "{name}: the generated Rust does not compile warning-free:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let out = Command::new(&bin).output().expect("run binary");
    let _ = std::fs::remove_dir_all(&dir);
    let user = rust.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap().to_string();
    (user, String::from_utf8_lossy(&out.stdout).into_owned())
}

fn assert_loop(name: &str, src: &str, expected: &str) {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("skip: rustc unavailable");
        return;
    }
    let (user, stdout) = emit_compile_run(name, src);
    assert!(!user.contains("while true"), "{name}: emitted `while true`:\n{user}");
    assert!(user.contains("loop {"), "{name}: expected a `loop`:\n{user}");
    assert_eq!(stdout, expected, "{name}: native output");
}

#[test]
fn tail_recursive_fn_binding_a_closure() {
    assert_loop("closure", "\
fn count(n: Int, acc: Int) -> Int = {
  let f = (x: Int) => x + 1
  if n == 0 then acc else count(n - 1, f(acc))
}

effect fn main() -> Unit = {
  println(int.to_string(count(5, 0)))
}
", "5\n");
}

#[test]
fn plain_tail_recursive_fn() {
    assert_loop("plain", "\
fn count(n: Int, acc: Int) -> Int =
  if n == 0 then acc else count(n - 1, acc + 1)

effect fn main() -> Unit = {
  println(int.to_string(count(7, 0)))
}
", "7\n");
}

#[test]
fn source_while_true_with_break() {
    assert_loop("source", "\
effect fn main() -> Unit = {
  var i = 0
  while true {
    i = i + 1
    if i >= 4 then break
  }
  println(int.to_string(i))
}
", "4\n");
}
