//! #3490: which fns a wasm artifact exports is decided by what the IR
//! records, not by how the fn is spelled.
//!
//! 1. A declared `@export(wasm, "sym")` on a fn spelled `__x` (legal since
//!    #3483) was skipped before the declaration was looked at: the build
//!    succeeded and the artifact silently lacked the export its host calls.
//! 2. `_start` (and `cabi_realloc`), which the WASI transform adds, were not
//!    in the reserved export set, so a program export of that name reached
//!    the transform and failed there as "this is an Almide bug". A declared
//!    one now gets the emitter's `duplicate wasm export name` wall; an
//!    undeclared pub fn of that name is simply not exported.
//!
//! Runs the `almide` binary (`ALMIDE_BIN`, else the one cargo built).

use std::process::{Command, Output};

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `almide build <source> --target wasm`, as (output, artifact bytes if any).
fn build_wasm(source: &str) -> (Output, Option<Vec<u8>>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.almd"), source).unwrap();
    let out = Command::new(almide_bin())
        .args(["build", "x.almd", "--target", "wasm", "-o", "x.wasm"])
        .current_dir(dir.path())
        .output()
        .expect("spawn almide");
    let bytes = std::fs::read(dir.path().join("x.wasm")).ok();
    (out, bytes)
}

fn exports(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Ok(wasmparser::Payload::ExportSection(r)) = payload {
            names.extend(r.into_iter().map(|e| e.unwrap().name.to_string()));
        }
    }
    names
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn a_declared_export_is_honoured_whatever_the_fn_is_spelled() {
    let src = "@export(wasm, \"add_one\")\npub fn __add_one(x: Int) -> Int = x + 1\n\n\
@export(wasm, \"add_two\")\npub fn add_two(x: Int) -> Int = x + 2\n\n\
effect fn main() -> Unit = {\n  println(\"m\")\n}\n";
    let (out, bytes) = build_wasm(src);
    assert!(out.status.success(), "{}", text(&out));
    let names = exports(&bytes.expect("artifact written"));
    assert!(names.iter().any(|n| n == "add_one"), "the declared `add_one` export ships: {names:?}");
    assert!(names.iter().any(|n| n == "add_two"), "{names:?}");
}

/// The issue's declared `_start` repro.
#[test]
fn a_declared_export_claiming_start_is_refused_by_the_emitter_wall() {
    let src = "@export(wasm, \"_start\")\npub fn boom() -> Int = 7\n\neffect fn main() -> Unit = {\n  println(\"m\")\n}\n";
    let (out, _) = build_wasm(src);
    let t = text(&out);
    assert!(!out.status.success(), "the build refuses:\n{t}");
    assert!(t.contains("duplicate wasm export name `_start`"), "the emitter names the clash:\n{t}");
    assert!(!t.contains("Almide bug"), "not an internal error from the transform:\n{t}");
}

/// The issue's implicit `_start` repro: an undeclared pub fn is no export
/// obligation, so a name the toolchain owns stays the toolchain's — the build
/// ships with `_start` as the WASI entry, instead of failing in the transform.
#[test]
fn an_undeclared_fn_named_start_stays_internal() {
    let src = "pub fn _start() -> Int = 7\n\neffect fn main() -> Unit = {\n  println(\"main ran ${_start()}\")\n}\n";
    let (out, bytes) = build_wasm(src);
    let t = text(&out);
    assert!(out.status.success(), "the build succeeds:\n{t}");
    assert!(!t.contains("Almide bug"), "{t}");
    let names = exports(&bytes.expect("artifact written"));
    assert_eq!(names.iter().filter(|n| *n == "_start").count(), 1, "one `_start`, the WASI entry: {names:?}");
}

/// A user fn spelled `__x` (default visibility is pub) is not exported and
/// does not wall: the `__` export space is the runtime's.
#[test]
fn an_undeclared_fn_spelled_in_the_runtime_space_stays_internal() {
    let src = "fn __helper(x: Int) -> Int = x + 1\n\neffect fn main() -> Unit = {\n  println(\"${__helper(1)}\")\n}\n";
    let (out, bytes) = build_wasm(src);
    assert!(out.status.success(), "{}", text(&out));
    let names = exports(&bytes.expect("artifact written"));
    assert!(!names.iter().any(|n| n == "__helper"), "{names:?}");
}

/// The other name the p1 transform writes.
#[test]
fn an_export_claiming_cabi_realloc_is_refused_by_the_emitter_wall() {
    let src = "@export(wasm, \"cabi_realloc\")\npub fn r(x: Int) -> Int = x\n\neffect fn main() -> Unit = {\n  println(\"m\")\n}\n";
    let (out, _) = build_wasm(src);
    let t = text(&out);
    assert!(!out.status.success(), "the build refuses:\n{t}");
    assert!(t.contains("duplicate wasm export name `cabi_realloc`"), "{t}");
    assert!(!t.contains("Almide bug"), "{t}");
}

/// A compiler-synthesized entry fn still never exports, whatever its
/// visibility: the Codec derive's helpers are pub and named in the `__` space
/// (`__encode_list_*`, ...); the only `__` exports are the runtime's own.
#[test]
fn synthesized_entry_fns_do_not_export() {
    let src = "import json\n\ntype P: Codec = { a: Int, xs: List[Int] }\n\npub fn keep(x: Int) -> Int = x\n\n\
effect fn main() -> Unit = {\n  println(json.stringify(P.encode(P { a: keep(1), xs: [2] })))\n}\n";
    let (out, bytes) = build_wasm(src);
    assert!(out.status.success(), "{}", text(&out));
    let names = exports(&bytes.expect("artifact written"));
    assert!(names.iter().any(|n| n == "keep"), "{names:?}");
    let synthesized: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("__") && !matches!(n.as_str(), "__heap" | "__heap_high" | "__alloc" | "__release"))
        .collect();
    assert!(synthesized.is_empty(), "no synthesized fn is exported: {names:?}");
}
