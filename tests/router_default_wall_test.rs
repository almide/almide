//! #2184: the incumbent leg's name routers no longer fall through to the plain
//! scalar impl for a heap-typed closure return.
//!
//! The mechanism the routers used to leave open is the #2154 one: a self-host
//! impl declared `f: (Int) -> Int` reached by a closure returning an i32
//! handle is a `call_indirect` type mismatch at run time, invisible to the
//! wasm validator, so the artifact shipped labelled verified. The router gate
//! (`crates/almide-mir/tests/router_signature_gate.rs`) found two routers
//! with that hole — `map.map` over a scalar-key map with a heap-valued
//! closure, `result.map_err` with a scalar error — and every router's output
//! now passes the caller-side signature check. These checks drive the shipped
//! binary: the structural leg answers each shape like native, and the forced
//! incumbent refuses it at build time by name instead of rendering the trap.

use std::process::Command;

/// The #2154 word-count idiom: a tuple sort key over map entries.
const ISSUE_2154_REPRO: &str = r#"
effect fn main() -> Unit = {
  let counts = map.from_list([("pear", 2), ("fig", 3), ("apple", 2), ("date", 1)])
  for (w, c) in list.sort_by(map.entries(counts), ((w, c)) => (0 - c, w)) {
    println("${w} ${c}")
  }
}
"#;

/// `map.map` over an Int-keyed map with a closure that returns a String — the
/// plain map_core twin declares `f: (Int) -> Int`.
const MAP_MAP_HEAP_VALUE: &str = r#"
effect fn main() -> Unit = {
  let m = map.from_list([(1, 10), (2, 20)])
  let s = map.map(m, (v) => "v${v}")
  println(map.get(s, 2) ?? "-")
}
"#;

/// `result.map_err` with a closure that returns an Int error — every
/// registered twin declares `f: (String) -> String`. The incumbent inlines
/// this combinator before any registry call when the closure is an inline
/// lambda or a named fn, so its refusal is reachable only through the router
/// gate's lattice; the end-to-end check here is the default route's answer.
const MAP_ERR_SCALAR_ERROR: &str = r#"
fn to_code(_e: String) -> Int = 404

fn bad(r: Result[Int, String]) -> Bool = result.is_err(result.map_err(r, to_code))

effect fn main() -> Unit = {
  println("${bad(err("missing"))} ${bad(ok(7))}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn write_program(dir: &std::path::Path, program: &str) -> std::path::PathBuf {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    source
}

/// `almide build --target <target>` on the default route; returns
/// (success, combined report).
fn build(source: &std::path::Path, target: &str, artifact: &std::path::Path, incumbent: bool) -> (bool, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["build", source.to_str().expect("path"), "--target", target, "-o", artifact.to_str().expect("path")])
        .env_remove("ALMIDE_COMPONENT_P3");
    if incumbent {
        cmd.env("ALMIDE_WASM_INCUMBENT", "1");
    } else {
        cmd.env_remove("ALMIDE_WASM_INCUMBENT");
    }
    let out = cmd.output().expect("build");
    let report = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), report)
}

fn run(artifact: &std::path::Path, wasm: bool) -> String {
    let out = if wasm {
        Command::new("wasmtime").arg("run").arg(artifact).output().expect("run")
    } else {
        Command::new(artifact).output().expect("run")
    };
    assert!(out.status.success(), "run exited {:?}:\n{}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Both legs answer alike, and the wasm bytes came from the structural leg.
fn agree_on_the_default_route(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_program(dir.path(), program);
    let native = dir.path().join("native");
    let (ok, report) = build(&source, "rust", &native, false);
    assert!(ok, "{label}: native build failed:\n{report}");
    let wasm = dir.path().join("m.wasm");
    let (ok, report) = build(&source, "wasm", &wasm, false);
    assert!(ok, "{label}: wasm build failed:\n{report}");
    assert!(report.contains("structural leg"), "{label}: the wasm build did not take the structural leg:\n{report}");
    let n = run(&native, false);
    let w = run(&wasm, true);
    assert_eq!(w, n, "{label}: the wasm leg answered differently from native");
    n
}

/// The forced incumbent refuses at build time, names the `_x` twin, and leaves
/// no artifact — the shape used to render as a run-time trap under a
/// `verified` label.
fn incumbent_refuses(program: &str, twin: &str, label: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_program(dir.path(), program);
    let wasm = dir.path().join("m.wasm");
    let (ok, report) = build(&source, "wasm", &wasm, true);
    assert!(!ok, "{label}: the incumbent leg built the shape instead of refusing it:\n{report}");
    assert!(report.contains(twin), "{label}: the refusal must name {twin}:\n{report}");
    assert!(!wasm.exists(), "{label}: a walled build must leave no artifact");
}

#[test]
fn the_2154_repro_agrees_on_both_legs_and_the_incumbent_refuses_it() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_default_route(ISSUE_2154_REPRO, "tuple sort key over map entries");
    assert_eq!(out, "fig 3\napple 2\npear 2\ndate 1\n");
    incumbent_refuses(ISSUE_2154_REPRO, "list.sort_by_x", "tuple sort key over map entries");
}

#[test]
fn map_map_with_a_heap_valued_closure_is_routed_or_refused_never_mislinked() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_default_route(MAP_MAP_HEAP_VALUE, "map.map core -> heap value");
    assert_eq!(out, "v20\n");
    incumbent_refuses(MAP_MAP_HEAP_VALUE, "map.map_x", "map.map core -> heap value");
}

#[test]
fn map_err_with_a_scalar_error_agrees_on_the_default_route() {
    if !wasmtime_available() {
        eprintln!("wasmtime unavailable — skipping");
        return;
    }
    let out = agree_on_the_default_route(MAP_ERR_SCALAR_ERROR, "result.map_err scalar E");
    assert_eq!(out, "true false\n");
}
