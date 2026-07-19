//! WASM runtime execution tests — verify WASM output matches Rust output.
//!
//! Two modes:
//! 1. Inline tests (below) — specific regression tests for known bug classes.
//! 2. Data-driven discovery — scans `spec/wasm_cross/*.almd`, runs each file
//!    on both Rust and WASM targets, asserts stdout matches.
//!
//! Each .almd file in spec/wasm_cross/ must have `fn main() -> Unit` that uses
//! `println(...)` to produce output. The test passes iff Rust and WASM outputs
//! are byte-identical.
//!
//! Requires: Node.js in PATH (for WASM execution).

use std::process::Command;
use std::path::Path;

fn almide_bin() -> String {
    // Try: ALMIDE_BIN env → cargo build output → PATH
    if let Ok(bin) = std::env::var("ALMIDE_BIN") { return bin; }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/release/almide");
    if cargo_bin.exists() { return cargo_bin.to_str().unwrap().to_string(); }
    "almide".to_string()
}


/// Compile and run an .almd program on the Rust target, return stdout.
fn run_rust(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    std::fs::write(&src_path, source).unwrap();

    let output = Command::new(almide_bin())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .expect("failed to run almide");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Rust compilation failed:\n{}", stderr);
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Compile an .almd program to WASM, run it with Node.js, return stdout.
fn run_wasm(source: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let wasm_path = dir.path().join("test.wasm");

    std::fs::write(&src_path, source).unwrap();

    // Compile to WASM
    let output = Command::new(almide_bin())
        .args(["build", src_path.to_str().unwrap(), "--target", "wasm", "-o", wasm_path.to_str().unwrap()])
        .output()
        .expect("failed to build WASM");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("WASM compilation failed:\n{}", stderr);
    }

    // Run with wasmtime (preferred) or Node.js WASI (fallback)
    let output = Command::new("wasmtime")
        .arg("--dir=/")
        .arg("-S")
        .arg("inherit-env=y")
        .arg(wasm_path.to_str().unwrap())
        .output();

    let output = match output {
        Ok(o) if o.status.code() != Some(127) => o, // wasmtime found
        _ => {
            // Fallback: Node.js WASI
            let js_runner = format!(r#"
const {{ readFileSync }} = require('fs');
const {{ WASI }} = require('wasi');
const wasi = new WASI({{ version: 'preview1', args: [], env: {{}} }});
const buf = readFileSync('{}');
const mod = new WebAssembly.Module(buf);
const inst = new WebAssembly.Instance(mod, wasi.getImportObject());
wasi.start(inst);
"#, wasm_path.to_str().unwrap().replace('\\', "/"));

            let js_path = dir.path().join("run.cjs");
            std::fs::write(&js_path, &js_runner).unwrap();

            Command::new("node")
                .arg(js_path.to_str().unwrap())
                .output()
                .expect("failed to run node or wasmtime")
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("WASM execution failed:\n{}", stderr);
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Assert that a program produces identical output on Rust and WASM targets.
fn assert_cross_target(source: &str) {
    // Skip if almide binary or node not available (e.g. CI without make install)
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("node").arg("--version").output().is_err() { return; }

    let rust_out = run_rust(source);
    let wasm_out = run_wasm(source);
    assert_eq!(
        rust_out, wasm_out,
        "\nCross-target mismatch!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
}

// ── List layout tests ──
// These specifically exercise list creation and access patterns that
// would break if the [len][cap][data] layout is inconsistent.

#[test]
    #[ignore = "#782 v1 gap (lowering subset) — native (almide run/test) works via fallback; only the direct wasm-build path walls. Tracked for the next lowering wave."]
fn wasm_list_literal_access() {
    assert_cross_target(r#"
fn main() -> Unit = println([10, 20, 30][1] |> int.to_string)
"#);
}

#[test]
fn wasm_list_push_basic() {
    assert_cross_target(r#"
fn main() -> Unit = {
  var xs: List[Int] = []
  for i in 0..5 { list.push(xs, i * 10) }
  println(int.to_string(xs[4]))
}
"#);
}

#[test]
fn wasm_list_push_capacity() {
    assert_cross_target(r#"
fn main() -> Unit = {
  var xs: List[Int] = []
  for i in 0..20 { list.push(xs, i) }
  println(int.to_string(list.len(xs)) + " " + int.to_string(xs[19]))
}
"#);
}

#[test]
fn wasm_string_split() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let parts = string.split("a,b,c", ",")
  println(parts[0] + " " + parts[1] + " " + parts[2])
}
"#);
}

#[test]
fn wasm_string_join() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = ["hello", "world", "test"]
  println(string.join(xs, "-"))
}
"#);
}

#[test]
fn wasm_guard_else_heap_temp() {
    // Regression (#755): a heap temp inside a `guard … else { … }` block was
    // never RC-processed on the WASM target. `Guard` is an `IrStmtKind`, so
    // `block_to_fnbody` funnelled it into perceus's `FnBody::Stmt`
    // pass-through arm, which recursed into nothing — the else block's heap
    // locals got no scope-end Dec and WASM RC verification refused the build
    // (`[perceus-belt] LEAK: no RcDec`). The else block's temp (`"hi " + w`)
    // is a fresh heap String here; it must be Dec'd before the guard's
    // divergent return, exactly as any other block-local.
    assert_cross_target(r#"
fn pick(b: Bool) -> Bool = b
fn main() -> Unit = {
  let w = "world"
  guard pick(false) else { let s = "hi " + w; println(s) }
  println("passed")
}
"#);
}

#[test]
fn wasm_list_map() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let ys = list.map(xs, (x) => x * x)
  println(int.to_string(ys[4]))
}
"#);
}

#[test]
fn wasm_list_filter() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5, 6]
  let evens = list.filter(xs, (x) => x % 2 == 0)
  println(int.to_string(list.len(evens)) + " " + int.to_string(evens[0]))
}
"#);
}

#[test]
fn wasm_list_concat() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let a = [1, 2, 3]
  let b = [4, 5, 6]
  let c = a + b
  println(int.to_string(list.len(c)) + " " + int.to_string(c[5]))
}
"#);
}

#[test]
fn wasm_map_keys_values() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let m = map.from_list([("x", 1), ("y", 2)])
  let ks = map.keys(m)
  println(int.to_string(list.len(ks)))
}
"#);
}

#[test]
fn wasm_set_to_list() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let s = set.from_list([1, 2, 3, 2, 1])
  let xs = set.to_list(s)
  println(int.to_string(list.len(xs)))
}
"#);
}

// ── List stdlib coverage ──

#[test]
fn wasm_list_reverse() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let rev = list.reverse(xs)
  println(int.to_string(rev[0]) + " " + int.to_string(rev[4]))
}
"#);
}

#[test]
fn wasm_list_slice() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [10, 20, 30, 40, 50]
  let s = list.slice(xs, 1, 4)
  println(int.to_string(list.len(s)) + " " + int.to_string(s[0]) + " " + int.to_string(s[2]))
}
"#);
}

#[test]
fn wasm_list_take_drop() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let t = list.take(xs, 3)
  let d = list.drop(xs, 3)
  println(int.to_string(list.len(t)) + " " + int.to_string(list.len(d)) + " " + int.to_string(d[0]))
}
"#);
}

#[test]
fn wasm_list_flat_map() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let ys = list.flat_map(xs, (x) => [x, x * 10])
  println(int.to_string(list.len(ys)) + " " + int.to_string(ys[5]))
}
"#);
}

#[test]
fn wasm_list_enumerate() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = ["a", "b", "c"]
  let pairs = list.enumerate(xs)
  println(int.to_string(list.len(pairs)))
}
"#);
}

#[test]
fn wasm_list_sort() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [5, 2, 8, 1, 4]
  let sorted = list.sort(xs)
  println(int.to_string(sorted[0]) + " " + int.to_string(sorted[4]))
}
"#);
}

#[test]
fn wasm_list_pop() {
    assert_cross_target(r#"
fn main() -> Unit = {
  var xs: List[Int] = [10, 20, 30]
  let last = list.pop(xs) ?? -1
  println(int.to_string(last) + " " + int.to_string(list.len(xs)))
}
"#);
}

#[test]
fn wasm_list_with_capacity() {
    assert_cross_target(r#"
fn main() -> Unit = {
  var xs = list.with_capacity(16)
  for i in 0..10 { list.push(xs, i) }
  println(int.to_string(list.len(xs)) + " " + int.to_string(xs[9]))
}
"#);
}

// ── String → List patterns ──

#[test]
fn wasm_string_lines() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let ls = string.lines("hello\nworld\ntest")
  println(int.to_string(list.len(ls)) + " " + ls[2])
}
"#);
    // #601: the cases where naive split-on-\n diverges from str::lines() —
    // a trailing terminator must NOT add an empty final line, and \r\n drops
    // the \r. (The case above is the coincidental-equal subset that masked
    // the divergence under a green "verified" badge.)
    assert_cross_target(r#"
fn show(s: String) -> String = "${int.to_string(list.len(string.lines(s)))}:${list.join(string.lines(s), "|")}"
fn main() -> Unit = {
  println(show("a\nb\n"))
  println(show("a\nb"))
  println(show("\n"))
  println(show("\n\n"))
  println(show("a\r\nb\r\nc"))
  println(show(""))
}
"#);
}

#[test]
fn wasm_string_split_empty() {
    // Non-matching delimiter ("x") returns [s]; an EMPTY delimiter ("") splits
    // per CODEPOINT with a leading + trailing empty string (native `s.split("")`).
    assert_cross_target(r#"
fn main() -> Unit = {
  let parts = string.split("hello", "x")
  println(int.to_string(list.len(parts)) + " " + parts[0])
  let cps = string.split("ab", "")
  println(int.to_string(list.len(cps)))
  println(cps[0] + "|" + cps[1] + "|" + cps[2] + "|" + cps[3])
  let multi = string.split("日本", "")
  println(int.to_string(list.len(multi)) + " " + multi[1] + multi[2])
}
"#);
}

// ── Value/JSON ──

#[test]
fn wasm_value_array_access() {
    assert_cross_target(r#"
import json

fn main() -> Unit = {
  let v = json.parse("[1, 2, 3]") ?? json.null()
  let arr = value.as_array(v) ?? []
  println(int.to_string(list.len(arr)))
}
"#);
}

#[test]
fn wasm_value_object_get() {
    assert_cross_target(r#"
import json

fn main() -> Unit = {
  let v = json.parse("{\"name\":\"test\",\"val\":42}") ?? json.null()
  let name_val = value.get(v, "name") ?? json.null()
  let name = value.as_string(name_val) ?? "none"
  println(name)
}
"#);
}

// ── Recursive/complex patterns ──

#[test]
fn wasm_list_nested_map_filter() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  let result = xs
    |> list.filter((x) => x % 2 == 0)
    |> list.map((x) => x * x)
  println(int.to_string(list.len(result)) + " " + int.to_string(result[4]))
}
"#);
}

#[test]
fn wasm_list_reduce() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let sum = list.fold(xs, 0, (acc, x) => acc + x)
  println(int.to_string(sum))
}
"#);
}

// ── Edge cases ──

#[test]
fn wasm_empty_list_operations() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let empty: List[Int] = []
  let mapped = list.map(empty, (x) => x + 1)
  let filtered = list.filter([1, 2, 3], (x) => x > 10)
  println(int.to_string(list.len(mapped)) + " " + int.to_string(list.len(filtered)))
}
"#);
}

#[test]
fn wasm_list_of_strings() {
    assert_cross_target(r#"
fn main() -> Unit = {
  var xs: List[String] = []
  list.push(xs, "hello")
  list.push(xs, "world")
  list.push(xs, "test")
  println(xs[1] + " " + int.to_string(list.len(xs)))
}
"#);
}

#[test]
fn wasm_list_contains() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [10, 20, 30, 40, 50]
  let has30 = list.contains(xs, 30)
  let has99 = list.contains(xs, 99)
  let r1 = if has30 then "true" else "false"
  let r2 = if has99 then "true" else "false"
  println(r1 + " " + r2)
}
"#);
}

#[test]
fn wasm_list_find() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let found = list.find(xs, (x) => x > 3) ?? -1
  println(int.to_string(found))
}
"#);
}

include!("wasm_runtime_test_parts/p2.rs");
include!("wasm_runtime_test_parts/p3.rs");
