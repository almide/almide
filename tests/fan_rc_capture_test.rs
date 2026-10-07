//! A fan arm that captures an `Rc`-backed value compiles and runs (#3459).
//!
//! A let-bound closure is an `Rc<dyn Fn>` on the native leg, and a `Bytes` is
//! an `AlmideRcCow` (an `Rc`). Neither is `Send`, so a `fan { … }` whose arms
//! captured one failed rustc with E0277 when the arms were spawned. Such a
//! fan now runs its arms inline in arm order (the sequential evaluation the
//! threaded form reproduces), a fn-typed param read in a fan arm is the `Rc`
//! handle rather than `&dyn Fn`, and a `fan.any` thunk is a plain `Box<dyn
//! Fn>` (its runtime is sequential).
//!
//! The pin, per shape: the emitted Rust compiles warning-free, and native and
//! wasm print the same, expected output.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-3459-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Emit Rust, compile it warning-free and run it; then run the wasm leg.
/// Returns (emitted Rust, native stdout, wasm stdout).
fn both_legs(name: &str, src: &str) -> (String, String, String) {
    let dir = scratch(name);
    let almd = dir.join("main.almd");
    std::fs::write(&almd, src).expect("write source");
    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "{name}: emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8_lossy(&emitted.stdout).into_owned();
    let native = compile_and_run(name, &dir, &rust);
    let wasm = Command::new(almide_bin()).arg("run").arg(&almd).args(["--target", "wasm"]).output().expect("spawn almide run");
    assert!(wasm.status.success(), "{name}: wasm run failed:\n{}", String::from_utf8_lossy(&wasm.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    (rust, native, String::from_utf8_lossy(&wasm.stdout).into_owned())
}

fn compile_and_run(name: &str, dir: &Path, rust: &str) -> String {
    let rs = dir.join("main.rs");
    std::fs::write(&rs, rust).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "{name}: the generated Rust does not compile warning-free:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let out = Command::new(&bin).output().expect("run binary");
    assert!(out.status.success(), "{name}: native run failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn rustc_available() -> bool {
    Command::new("rustc").arg("--version").output().is_ok()
}

fn check(name: &str, src: &str, expected: &str) -> String {
    let (rust, native, wasm) = both_legs(name, src);
    assert_eq!(native, expected, "{name}: native output");
    assert_eq!(wasm, expected, "{name}: wasm output");
    rust.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap().to_string()
}

#[test]
fn fan_block_calls_a_let_bound_closure() {
    if !rustc_available() { return; }
    let user = check("block", "\
effect fn main() -> Unit = {
  let k = 10
  let f = (i: Int) => i * k
  let (a, b) = fan { f(1), f(2) }
  println(int.to_string(a + b))
}
", "30\n");
    assert!(user.contains("__almide_fan_r0"), "expected the inline fan form:\n{user}");
    assert!(!user.contains("__almide_s.spawn"), "a fan arm holding an Rc closure must not be spawned:\n{user}");
}

#[test]
fn fan_map_takes_a_let_bound_closure() {
    if !rustc_available() { return; }
    check("map", "\
effect fn main() -> Unit = {
  let k = 3
  let f = (i: Int) => ok(i * k)
  let ys = fan.map([1, 2, 3], f)!
  println(int.to_string(list.sum(ys)))
}
", "18\n");
}

#[test]
fn closure_capturing_a_list_and_a_record_and_a_nested_closure() {
    if !rustc_available() { return; }
    check("nested", "\
type P = { x: Int, name: String }
effect fn main() -> Unit = {
  let xs = [1, 2, 3]
  let p = P { x: 5, name: \"a\" }
  let f = (i: Int) => i + list.sum(xs) + p.x
  let g = (i: Int) => f(i) * 2
  let (a, b) = fan { f(1), g(2) }
  println(int.to_string(a + b))
  println(p.name)
}
", "38\na\n");
}

#[test]
fn an_arm_error_still_lets_every_arm_run_in_order() {
    if !rustc_available() { return; }
    let src = "\
effect fn step(f: (Int) -> Int, i: Int) -> Int = {
  println(\"arm ${int.to_string(i)}\")
  if i == 1 then err(\"arm one failed\")! else f(i)
}
effect fn main() -> Unit = {
  let k = 7
  let f = (i: Int) => i * k
  let (c, d) = fan { step(f, 2), step(f, 3) }
  println(int.to_string(c + d))
  let (e, g) = fan { step(f, 1), step(f, 4) }
  println(int.to_string(e + g))
}
";
    let dir = scratch("effect");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, src).expect("write source");
    let expected = "arm 2\narm 3\n35\narm 1\narm 4\n";
    for target in ["rust", "wasm"] {
        let out = Command::new(almide_bin()).arg("run").arg(&almd).args(["--target", target]).output().expect("spawn almide run");
        assert_eq!(out.status.code(), Some(1), "{target}: the arm's Err ends main");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{target}: stdout");
        assert!(String::from_utf8_lossy(&out.stderr).contains("Error: arm one failed"), "{target}: stderr");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fn_param_and_bytes_and_fan_any() {
    if !rustc_available() { return; }
    check("misc", "\
effect fn twice(x: Int, f: (Int) -> Int) -> Int = {
  let (a, b) = fan { f(x), f(x + 1) }
  a + b
}
effect fn main() -> Unit = {
  println(int.to_string(twice(3, (i) => i + 1)!))
  let b = bytes.from_list([1, 2, 3])
  let (x, y) = fan { bytes.len(b), bytes.len(b) + 1 }
  println(int.to_string(x + y))
  let k = 10
  let g: (Int) -> Result[Int, String] = (i) => ok(i * k)
  println(int.to_string(fan.any { g(1), g(2) } ?? 0))
}
", "9\n7\n10\n");
}

#[test]
fn a_fan_without_an_rc_capture_still_spawns() {
    if !rustc_available() { return; }
    let user = check("threaded", "\
effect fn main() -> Unit = {
  let xs = [1, 2, 3]
  let (a, b) = fan { list.sum(xs), list.len(xs) }
  println(int.to_string(a + b))
}
", "9\n");
    assert!(user.contains("__almide_s.spawn"), "a fan over Send captures keeps its threads:\n{user}");
}
