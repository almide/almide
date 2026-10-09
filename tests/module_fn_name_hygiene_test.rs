//! A user MODULE's fns and compiler-synthesized fns live in disjoint IR name
//! spaces (#3504), as the entry program's have since #3483.
//!
//! #3483 escaped an entry fn spelled in the compiler's `__` space to
//! `almide_fn_<name>`, but a sibling module's fns kept their spelling, so they
//! still shared a name space with the helpers the compiler synthesizes into
//! the same module:
//!
//! - a module fn `__encode_list_int` beside a `Codec` type was called by the
//!   derived `P.encode` in place of the list encoder (rustc E0308 natively,
//!   E082 `ty-mismatch:Scalar(Str)-vs-Value` on wasm);
//! - a module fn `__fan_site_0` beside a `fan { list.map(..) }` block was
//!   merged with the instance-parallel chunk the wasm leg outlines under that
//!   name, and `util.__fan_site_0(1)` silently ran the chunk's body (printed
//!   `3`, not `101`).
//!
//! Lowering now escapes a user module's fn the same way, at its definition,
//! at every intra-module call, and at every cross-module call or fn-value
//! reference. The program runs on both legs against a twin whose module fn
//! names are neutral, so a collision shows as a divergence. `ALMIDE_BIN`
//! points the test at another build (a pre-fix one fails it).

use std::process::Command;

/// The issue's repro: a module fn spelling the list encoder a derived Codec
/// calls.
const CODEC_UTIL: &str = r#"import json

type P: Codec = { a: Int, b: List[Int] }

pub fn __encode_list_int(xs: List[Int]) -> String = "USER"

pub fn enc(p: P) -> String = json.stringify(P.encode(p))
"#;

const CODEC_MAIN: &str = r#"import self.util

effect fn main() -> Unit = {
  println(util.enc(util.P { a: 1, b: [2, 3] }))
  println(util.__encode_list_int([1]))
}
"#;

const CODEC_EXPECTED: &str = "{\"a\":1,\"b\":[2,3]}\nUSER\n";

/// A module fn spelling the wasm leg's first instance-parallel chunk, called
/// directly and as a fn value, beside a module-private `__` fn called from
/// inside the module.
const FAN_UTIL: &str = r#"pub fn __fan_site_0(x: Int) -> Int = x + 100

fn __twice(x: Int) -> Int = x * 2

pub fn quad(x: Int) -> Int = __twice(__twice(x))

pub effect fn par(xs: List[Int]) -> Result[(List[Int], List[Int]), String] = {
  let r = fan {
    list.map(xs, (x) => x * 3)
    list.map(xs, (x) => x + 1)
  }
  ok(r)
}
"#;

const FAN_MAIN: &str = r#"import self.util

effect fn main() -> Unit = {
  println(int.to_string(util.__fan_site_0(1)))
  println(int.to_string(util.quad(3)))
  let ys = list.map([1, 2], util.__fan_site_0)
  println(ys |> list.map((y) => int.to_string(y)) |> list.join(","))
  let (a, b) = util.par([1, 2, 3])!
  println((a + b) |> list.map((z) => int.to_string(z)) |> list.join(","))
}
"#;

const FAN_EXPECTED: &str = "101\n12\n101,102\n3,6,9,2,3,4\n";

/// The module fn spellings above and their neutral twins.
const RENAMES: &[(&str, &str)] = &[
    ("__encode_list_int", "zz_encode_list_int"),
    ("__fan_site_0", "zz_fan_site_0"),
    ("__twice", "zz_twice"),
];

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn twin(src: &str) -> String {
    RENAMES.iter().fold(src.to_string(), |s, (from, to)| s.replace(from, to))
}

/// Build the two-module package on one leg and run it; the stdout.
fn run(util: &str, main: &str, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"hyg\"\nversion = \"0.1.0\"\n").expect("toml");
    std::fs::create_dir(dir.path().join("src")).expect("src");
    std::fs::write(dir.path().join("src/util.almd"), util).expect("util");
    std::fs::write(dir.path().join("src/main.almd"), main).expect("main");
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let built = Command::new(almide_bin())
        .current_dir(dir.path())
        .args(["build", "src/main.almd", "--target", target, "-o"])
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
    assert!(out.status.success(), "{target} exited {:?}:\n{}", out.status.code(), log(&out));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The twin answers `expected` (the harness is sound), then the `__`-spelled
/// package answers the same.
fn check(util: &str, main: &str, expected: &str, target: &str) {
    assert_eq!(run(&twin(util), &twin(main), target), expected, "the neutral twin on {target}");
    assert_eq!(run(util, main, target), expected, "the `__`-spelled module fns on {target}");
}

#[test]
fn a_module_fn_spelling_a_codec_helper_answers_like_its_twin_on_native() {
    check(CODEC_UTIL, CODEC_MAIN, CODEC_EXPECTED, "rust");
}

#[test]
fn a_module_fn_spelling_a_codec_helper_answers_like_its_twin_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    check(CODEC_UTIL, CODEC_MAIN, CODEC_EXPECTED, "wasm");
}

#[test]
fn a_module_fn_spelling_a_fan_chunk_answers_like_its_twin_on_native() {
    check(FAN_UTIL, FAN_MAIN, FAN_EXPECTED, "rust");
}

#[test]
fn a_module_fn_spelling_a_fan_chunk_answers_like_its_twin_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    check(FAN_UTIL, FAN_MAIN, FAN_EXPECTED, "wasm");
}
