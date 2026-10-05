//! C-196 (ALS-T6): call-stack exhaustion is the defined abort — exactly
//! `Error: stack overflow` on stderr and exit 1, with the stdout written
//! before it kept — on native and on the embedded wasm host
//! (`almide run --target wasm`). Before, native died with Rust's
//! `thread 'main' has overflowed its stack` / `fatal runtime error: stack
//! overflow, aborting` (SIGABRT, exit 134) and the embedded host printed
//! `Error: wasm trap: call stack exhausted`.
//!
//! The depth each leg reaches is NOT compared: it is each target's own stack
//! (C-196 declares it per target). A depth of 1e8 exhausts every leg's stack.
//!
//! Three shapes, one per native emitter path and one threaded:
//! - `TRUST_SPINE`: a String recursion the v1 trust-spine render takes
//!   (`almide-mir` render_native, the default native route).
//! - `CODEGEN`: a record recursion that walls out of the v1 render, so the
//!   standard codegen (and its runtime prelude) builds it.
//! - `FAN`: the overflow happens inside a `fan` arm — natively on a spawned
//!   worker thread (Rust's 2 MiB default stack), on wasm in the guest's
//!   sequential evaluation.

use std::process::Command;

const TRUST_SPINE: &str = r#"fn grow(n: Int) -> String =
  if n == 0 then ""
  else {
    let s = grow(n - 1)
    if string.len(s) > 5 then "x" else s + "a"
  }

fn main() -> Unit = {
  println("before")
  println(grow(100000000))
}
"#;

const CODEGEN: &str = r#"type P = { name: String, n: Int }

fn grow(p: P) -> P =
  if p.n == 0 then p
  else {
    let q = grow({ name: p.name, n: p.n - 1 })
    if string.len(q.name) > 5 then q else { name: q.name + "a", n: q.n }
  }

fn main() -> Unit = {
  println("before")
  let r = grow({ name: "", n: 100000000 })
  println(r.name)
}
"#;

const FAN: &str = r#"fn grow(n: Int) -> String =
  if n == 0 then ""
  else {
    let s = grow(n - 1)
    if string.len(s) > 5 then "x" else s + "a"
  }

effect fn main() -> Unit = {
  println("before")
  let (a, b) = fan { string.len(grow(3)), string.len(grow(100000000)) }
  println("${a} ${b}")
}
"#;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn run(src: &std::path::Path, target: &str) -> (Option<i32>, String, String) {
    let out = Command::new(almide_bin())
        .args(["run", "--target", target])
        .arg(src)
        .output()
        .expect("spawn almide run");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_defined_abort(name: &str, program: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(format!("{name}.almd"));
    std::fs::write(&src, program).expect("write probe");
    let expected = (
        Some(1),
        "before\n".to_string(),
        "Error: stack overflow\n".to_string(),
    );
    let native = run(&src, "rust");
    assert_eq!(
        native, expected,
        "native leg of {name}: stack exhaustion is ALS-T6's defined abort (C-196)"
    );
    let wasm = run(&src, "wasm");
    assert_eq!(
        wasm, native,
        "embedded wasm leg of {name} must abort as native does (C-196)"
    );
}

#[test]
fn trust_spine_render_overflow_is_the_defined_abort() {
    assert_defined_abort("trust_spine", TRUST_SPINE);
}

#[test]
fn codegen_render_overflow_is_the_defined_abort() {
    assert_defined_abort("codegen", CODEGEN);
}

#[test]
fn fan_worker_overflow_is_the_defined_abort() {
    assert_defined_abort("fan", FAN);
}
