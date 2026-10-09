//! A user fn spelled `<f>__fuel` is not a metered clone of `<f>` (#3491).
//!
//! The v1 native render recognised a T1-2 metered clone by the `__fuel`
//! SUFFIX and copied the base fn's signature onto it. A user fn
//! `heavy__fuel` next to a user fn `heavy` got `heavy`'s signature, so the
//! trust-spine render walled (`shim __str_concat arg type mismatch`) and the
//! program fell back to the standard codegen; with a budget region — which
//! needs the trust spine — the native build failed outright, and the clone of
//! a region-reachable `heavy` was itself named `heavy__fuel`, a second
//! definition of the user fn. Clones are now named in the compiler's `__`
//! space and the clone→base map comes from the pass that made them.
//!
//! Each program pins the ROUTE (`ALMIDE_VERIFIED_DEBUG=1` names the v1
//! trust-spine render, not the walled fallback) and the output on both legs,
//! against a twin whose fn is spelled `heavyfuel`.

use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// The issue's repro: no budget region, so no clones are made.
const NO_REGION: &str = r#"fn heavy(x: Int) -> Int = x * 2

fn heavy__fuel(x: String, y: Int) -> String = "${x}-${y}"

effect fn main() -> Unit = {
  println(int.to_string(heavy(3)))
  println(heavy__fuel("a", 21))
}
"#;

/// `heavy` is reachable from a budget region, so it IS cloned.
const IN_REGION: &str = r#"fn heavy(n: Int) -> Int = {
  var i = 0
  var acc = 0
  while i < n {
    acc = (acc + i * 7) % 999983
    i = i + 1
  }
  acc
}

fn heavy__fuel(x: String, y: Int) -> String = "${x}-${y}"

effect fn main() -> Unit = {
  let r = fan.bounded(compute.ns(100000000)) { heavy(1000) } ?? -1
  println(int.to_string(r))
  println(heavy__fuel("a", 21))
}
"#;

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Run `program` on `target` with the route note on; `(stdout, stderr)`.
fn run(program: &str, target: &str) -> (String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(almide_bin())
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .env_remove("ALMIDE_FUEL_PROBE")
        .args(["run", source.to_str().expect("path"), "--target", target])
        .output()
        .expect("spawn almide run");
    assert!(out.status.success(), "{target} run:\n{}", log(&out));
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn twin(program: &str) -> String {
    program.replace("heavy__fuel", "heavyfuel")
}

fn stays_on_the_trust_spine(program: &str, expected: &str) {
    for source in [twin(program), program.to_string()] {
        let (stdout, stderr) = run(&source, "rust");
        assert!(
            stderr.contains("native: v1 trust-spine render") && !stderr.contains("walled"),
            "the native route left the v1 trust spine:\n{stderr}\n--- program ---\n{source}"
        );
        assert_eq!(stdout, expected, "native output:\n{source}");
    }
}

#[test]
fn a_user_fuel_suffix_fn_keeps_the_trust_spine_without_a_region() {
    stays_on_the_trust_spine(NO_REGION, "6\na-21\n");
}

#[test]
fn a_user_fuel_suffix_fn_keeps_the_trust_spine_beside_a_metered_clone() {
    stays_on_the_trust_spine(IN_REGION, "496551\na-21\n");
}

#[test]
fn a_user_fuel_suffix_fn_answers_like_its_twin_on_wasm() {
    for (program, expected) in [(NO_REGION, "6\na-21\n"), (IN_REGION, "496551\na-21\n")] {
        assert_eq!(run(&twin(program), "wasm").0, expected, "wasm twin");
        assert_eq!(run(program, "wasm").0, expected, "wasm");
    }
}
