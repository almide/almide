//! #2411: `list.push` / `list.clear` on a record var's FIELD build and run on
//! the wasm leg and agree with native.
//!
//! The structural leg's `push` arm accepted only a plain var receiver, so the
//! builder idiom — a `mut` record parameter whose list fields are pushed in a
//! loop, #2316's shape — walled on both wasm legs (`list-push-nonvar`; the
//! incumbent brick refused the loop for its own reason). It now routes through
//! the copy-on-write field write (`lower_field_assign`) as `h.f = h.f + [v]`.
//!
//! The matrix lives in `spec/wasm_cross/list_push_on_record_field.almd`
//! (C-226 / C-033). This file carries the shape the fixture cannot: the
//! DECLARED OMISSION stays a wall (a nested path is refused, not mis-valued),
//! pinned so that a later change that starts accepting `h.i.xs` has to say so
//! here rather than silently widening the subset — and the negative control
//! that a plain-var push still takes the original arm (its module bytes are
//! unchanged by this PR, which the size ratchet pins too).

use std::process::Command;

const NESTED_PATH: &str = r#"
type I = { xs: List[Int] }
type H = { i: I }
fn main() -> Unit = {
  var h = H { i: I { xs: [] } }
  list.push(h.i.xs, 1)
  println("${h.i.xs}")
}
"#;

const PLAIN_VAR: &str = r#"
fn main() -> Unit = {
  var xs: List[Int] = []
  list.push(xs, 1)
  list.push(xs, 2)
  println("${xs}")
}
"#;

const FIELD_IN_LOOP: &str = r#"
type H = { xs: List[Int], n: Int }
fn fill(mut h: H, k: Int) -> Unit = {
  for i in 0..<k {
    list.push(h.xs, i * i)
    h.n = h.n + 1
  }
  ()
}
fn main() -> Unit = {
  var h = H { xs: [], n: 0 }
  fill(h, 5)
  println("${h.xs} ${h.n}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn build(program: &str, target: &str, dir: &std::path::Path) -> std::process::Output {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    let artifact = dir.join(if target == "rust" { "native" } else { "m.wasm" });
    Command::new(almide_bin())
        .args(["build", source.to_str().expect("path"), "--target", target, "-o", artifact.to_str().expect("path")])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build")
}

/// Build on both legs, run under stock `wasmtime`, assert agreement.
fn agree(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut first: Option<String> = None;
    for target in ["rust", "wasm"] {
        let built = build(program, target, dir.path());
        assert!(built.status.success(), "{label}/{target} build:\n{}", String::from_utf8_lossy(&built.stderr));
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{label}/{target} exited {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &first {
            Some(native) => assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native"),
            None => first = Some(stdout),
        }
    }
    first.expect("native ran")
}

#[test]
fn a_push_onto_a_mut_parameter_field_in_a_loop_agrees() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(FIELD_IN_LOOP, "field in loop").trim(), "[0, 1, 4, 9, 16] 5");
}

#[test]
fn a_plain_var_push_still_takes_the_original_arm() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree(PLAIN_VAR, "plain var").trim(), "[1, 2]");
}

#[test]
fn a_nested_path_receiver_is_still_refused_honestly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let built = build(NESTED_PATH, "wasm", dir.path());
    assert!(!built.status.success(), "a nested receiver path is outside the field-write desugar and must wall, not build");
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(
        stderr.contains("list-push-nonvar"),
        "the refusal must name the receiver shape (list-push-nonvar), got:\n{stderr}"
    );
}
