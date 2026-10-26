//! #4242 (C-046): a record spread with no override, `{ ...a }`, is a copy of
//! its base. Native rendered the update form with an empty field list,
//! `A { , ..a }` — invalid Rust, so the build failed while wasm printed. The
//! program below runs on native, the embedded wasm host and the
//! interpreter, and every leg prints the same bytes.

#![allow(dead_code)]

include!("wasm_runtime_test_parts/interp_leg.rs");

use std::path::PathBuf;
use std::process::Command;

const SRC: &str = r#"type A = { key: String, n: Int }

type Box = { inner: A, tag: String }

fn bump(a: A) -> A = { ...a, n: a.n + 1 }

fn copy(a: A) -> A = { ...a }

fn main() -> Unit = {
  let a = A { key: "x", n: 1 }
  let b: A = { ...a }
  let c = { ...b }
  println("${b.key} ${b.n}")
  println("${c}")
  println("${bump({ ...c })}")
  println("${copy(a) == a}")
  let bx = Box { inner: { ...a }, tag: "t" }
  let by = { ...bx }
  println("${by}")
  let xs = [a, b] |> list.map((r) => { ...r })
  println("${xs}")
}
"#;

const OUT: &str = r#"x 1
A { key: "x", n: 1 }
A { key: "x", n: 2 }
true
Box { inner: A { key: "x", n: 1 }, tag: "t" }
[A { key: "x", n: 1 }, A { key: "x", n: 1 }]
"#;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !ok && std::env::var_os("ALMIDE_EXPECT_TOOLS").is_some() {
        panic!("ALMIDE_EXPECT_TOOLS is set but wasmtime is not on PATH");
    }
    ok
}

#[test]
fn a_spread_with_no_override_copies_its_base_on_every_leg() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("record_spread_no_override");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("spread_copy.almd");
    std::fs::write(&file, SRC).unwrap();
    let run = |extra: &[&str]| {
        let o = Command::new(almide()).arg("run").arg(&file).args(extra).output().expect("almide runs");
        assert!(o.status.success(), "{extra:?}: the run failed:\n{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    assert_eq!(run(&[]), OUT, "native");
    if wasmtime_available() {
        assert_eq!(run(&["--target", "wasm"]), OUT, "wasm");
    }
    match run_interp_capture(SRC) {
        InterpLeg::Ran(_, o, _) => assert_eq!(o, OUT, "interp"),
        InterpLeg::Skip(why) => panic!("the interpreter must vote: {why}"),
    }
    // The emitted Rust carries no empty update.
    let o = Command::new(almide()).arg(&file).args(["--target", "rust"]).output().expect("almide emits");
    let rust = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success() && !rust.contains("{ ,"), "the emitted Rust holds an empty update:\n{rust}");
}
