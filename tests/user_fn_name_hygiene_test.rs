//! User fns and compiler-synthesized fns live in disjoint IR name spaces
//! (#3483).
//!
//! The compiler names the fns it synthesizes — and the stdlib its internal
//! helpers — in the `__` space (`__lambda_*`, `__almd_scoped_N`,
//! `__almd_lift_N`, `__encode_list_*`, `__test_almd_*`, …) and recognises them
//! there. A user fn could spell any of those names, so it either collided with
//! the synthesized fn or was taken for one:
//!
//! - a user `__lambda_main_0`, `__almd_scoped_1`, `branch_lift_synth_0` or
//!   `optional_chain_synth_0` was a second definition of the helper's name
//!   (rustc E0428 natively), and wasm dispatched a user call to the helper
//!   (`__almd_scoped_1(1)` printed 2);
//! - a user `__encode_list_int` / `__encode_list_Inner` hijacked the Codec
//!   derive's call on wasm (`{"b":"user"}`) and suppressed the derived helper;
//! - a user `__encode_twice` was rewritten to the runtime `almide_rt___encode_twice`;
//! - a user `__fallible__apply1` was replaced, in the checker, by the generated
//!   fallible twin of `apply1`, and a valid program was refused;
//! - a user `__test_almd_foo` collided with `test "foo"` natively.
//!
//! Lowering now escapes an entry fn spelled in the `__` space (or in the escape
//! space itself) to `almide_fn_<name>`, the synthesizers that used plain names
//! moved into `__almd_*`, and the fallible twin is spelled with a `:` no
//! identifier contains. Each program runs on both legs and against a twin
//! whose user fn names are neutral, so a collision shows as a divergence.

use std::process::Command;

const COLLIDING: &str = r#"
import json

type P: Codec = { a: Int, b: List[Int] }

type Inner: Codec = { n: Int }

type Outer: Codec = { xs: List[Inner] }

type Q = { f: Int }

fn __lambda_main_0(x: Int) -> Int = x * 1000

fn __encode_list_int(xs: List[Int]) -> Value = value.str("user")

fn __encode_list_Inner(xs: List[Inner]) -> Value = value.str("user")

fn __encode_twice(x: Int) -> Int = x * 2

fn __almd_scoped_1(x: Int) -> Int = x + 1000

fn branch_lift_synth_0(x: Int) -> Int = x + 2000

fn __almd_lift_0(x: Int) -> Int = x + 3000

fn optional_chain_synth_0(x: Int) -> Int = x + 4000

fn __almd_optchain_0(x: Int) -> Int = x + 5000

fn __fallible__apply1(cb: (String) -> Int, s: String) -> Int = 6000

fn almide_fn___twice(x: Int) -> Int = x + 7000

fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

fn apply1(cb: (String) -> Int, s: String) -> Int = cb(s)

fn work(n: Int) -> Int = scoped { n * 2 }

fn join_all(xs: List[String]) -> String = {
  var out = ""
  for x in xs {
    let r = if x != "" then x + "!" else "empty"
    out = out + r
  }
  out
}

fn get(q: Q?) -> Int? = q?.f

effect fn main() -> Unit = {
  let k = 3
  println("${apply((y) => y + k, 1)} ${__lambda_main_0(2)}")
  println(json.stringify(P.encode(P { a: 1, b: [2, 3] })))
  println(json.stringify(Outer.encode(Outer { xs: [Inner { n: 1 }] })))
  println("${json.stringify(__encode_list_int([1]))} ${__encode_twice(21)}")
  println("${work(21)} ${__almd_scoped_1(1)}")
  println("${join_all(["a", "", "b"])} ${branch_lift_synth_0(1)} ${__almd_lift_0(1)}")
  println("${get(some(Q { f: 5 })) ?? 0} ${optional_chain_synth_0(1)} ${__almd_optchain_0(1)}")
  let r = apply1((s) => int.parse(s)!, "12") ?? -1
  println("${r} ${__fallible__apply1((s) => 0, "x")} ${almide_fn___twice(1)}")
}
"#;

/// The user fn spellings above and their neutral twins (longest first, so no
/// replacement rewrites part of another).
const RENAMES: &[(&str, &str)] = &[
    ("optional_chain_synth_0", "u_optchain"),
    ("__encode_list_Inner", "u_enc_inner"),
    ("__fallible__apply1", "u_fallible"),
    ("branch_lift_synth_0", "u_lift"),
    ("__almd_optchain_0", "u_almd_optchain"),
    ("__encode_list_int", "u_enc_int"),
    ("almide_fn___twice", "u_escaped"),
    ("__lambda_main_0", "u_lambda"),
    ("__almd_scoped_1", "u_scoped"),
    ("__encode_twice", "u_twice"),
    ("__almd_lift_0", "u_almd_lift"),
];

const EXPECTED: &str = "4 2000\n{\"a\":1,\"b\":[2,3]}\n{\"xs\":[{\"n\":1}]}\n\"user\" 42\n42 1001\na!emptyb! 2001 3001\n5 4001 5001\n12 6000 7001\n";

/// A user fn spelling a test block's IR name (`__test_almd_<label>`).
const TEST_COLLIDING: &str = r#"
fn __test_almd_foo() -> Int = 7

test "foo" {
  assert_eq(__test_almd_foo(), 7)
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn write(dir: &std::path::Path, program: &str) -> std::path::PathBuf {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    source
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Build and run `program` on one leg; the stdout.
fn run(program: &str, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), program);
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let built = Command::new(almide_bin())
        .args(["build", source.to_str().expect("path"), "--target", target, "-o"])
        .arg(&artifact)
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build");
    assert!(built.status.success(), "{target} build:\n{}", log(&built));
    let mut command = if target == "rust" {
        Command::new(&artifact)
    } else {
        let mut c = Command::new("wasmtime");
        c.arg("run").arg(&artifact);
        c
    };
    let out = command.output().expect("run");
    assert!(out.status.success(), "{target} exited {:?}", out.status.code());
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// `almide test` on one leg passes.
fn test_passes(program: &str, target: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main_test.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(almide_bin())
        .args(["test", source.to_str().expect("path"), "--target", target])
        .output()
        .expect("test");
    assert!(out.status.success(), "{target} test:\n{}", log(&out));
}

fn neutral_twin() -> String {
    RENAMES.iter().fold(COLLIDING.to_string(), |src, (from, to)| src.replace(from, to))
}

#[test]
fn user_fns_spelling_synthesized_names_answer_like_their_neutral_twin_on_native() {
    assert_eq!(run(COLLIDING, "rust"), EXPECTED);
    assert_eq!(run(&neutral_twin(), "rust"), EXPECTED);
    test_passes(TEST_COLLIDING, "rust");
}

#[test]
fn user_fns_spelling_synthesized_names_answer_like_their_neutral_twin_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(run(COLLIDING, "wasm"), EXPECTED);
    assert_eq!(run(&neutral_twin(), "wasm"), EXPECTED);
    test_passes(TEST_COLLIDING, "wasm");
}

/// The closed-by-construction property on the emitted Rust: every user fn in
/// the `__` space is defined and called under its escaped name, and each
/// synthesized helper keeps its own name — one definition each.
#[test]
fn a_user_fn_cannot_spell_a_synthesized_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), COLLIDING);
    let out = Command::new(almide_bin())
        .args([source.to_str().expect("path"), "--target", "rust"])
        .output()
        .expect("emit");
    assert!(out.status.success(), "{}", log(&out));
    let rust = String::from_utf8_lossy(&out.stdout);
    for user in ["__lambda_main_0", "__encode_twice", "__almd_scoped_1", "__almd_lift_0", "__fallible__apply1"] {
        assert!(rust.contains(&format!("pub fn almide_fn_{user}(")), "user fn `{user}` is escaped:\n{rust}");
    }
    assert!(rust.contains("almide_fn___encode_twice(21i64)"), "the user's call reaches the user fn, not a runtime helper:\n{rust}");
    assert!(rust.contains("pub fn almide_fn_almide_fn___twice("), "the escape space is escaped too:\n{rust}");
    for synth in ["__almd_scoped_1", "__almd_lift_0", "__fallible__apply1"] {
        assert_eq!(rust.matches(&format!("pub fn {synth}(")).count(), 1, "one `{synth}` definition:\n{rust}");
    }
}
