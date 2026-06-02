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
}

#[test]
fn wasm_string_split_empty() {
    assert_cross_target(r#"
fn main() -> Unit = {
  let parts = string.split("hello", "x")
  println(int.to_string(list.len(parts)) + " " + parts[0])
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

// ── Data-driven discovery test ──
// Scans spec/wasm_cross/*.almd, runs each on both targets, compares output.

#[test]
fn wasm_cross_target_spec() {
    // Skip if prerequisites unavailable
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("node").arg("--version").output().is_err() { return; }

    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    if !spec_dir.exists() { return; }

    let mut entries: Vec<_> = std::fs::read_dir(&spec_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());

    if entries.is_empty() {
        return;
    }

    let mut passed = 0;
    let mut failed = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();

        let rust_out = run_rust(&source);
        let wasm_result = std::panic::catch_unwind(|| run_wasm(&source));

        match wasm_result {
            Ok(wasm_out) if wasm_out == rust_out => {
                passed += 1;
            }
            Ok(wasm_out) => {
                failed.push(format!(
                    "{}: output mismatch\n  Rust: {:?}\n  WASM: {:?}",
                    name, rust_out, wasm_out
                ));
            }
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                failed.push(format!("{}: WASM failed\n  {}", name, msg.lines().next().unwrap_or("")));
            }
        }
    }

    eprintln!("\nwasm_cross_target_spec: {}/{} passed", passed, entries.len());
    if !failed.is_empty() {
        panic!(
            "\n{} cross-target failures:\n\n{}",
            failed.len(),
            failed.join("\n\n")
        );
    }
}

// ── Closure Architecture v2, P0: cross-module closure identity ──
// A submodule function returning a non-capturing lambda, applied across the
// module boundary. Before P0 the WASM emitter correlated a raw Lambda to its
// LambdaInfo by `lambda_id`, which resets to 0 per module — so a module lambda
// collided with a main-program lambda of the same id, AND module lambdas were
// never registered in the WASM closure pre-scan. `lib.neg()(5)` then returned
// main's `add` body on WASM (native `1005 / -5`, WASM `1005 / 1005`), and a
// no-main-lambda variant emitted invalid WASM ("table index out of bounds").
// GlobalizeClosureIdsPass (program-unique ids) + scanning module functions in
// the pre-scan fix both. These must run on WASM and match the native result.

/// Write a multi-file project into a tempdir and assert its `main.almd`
/// produces identical stdout on the Rust and WASM targets.
fn assert_cross_target_project(files: &[(&str, &str)]) {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    for (rel, content) in files {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, content).unwrap();
    }

    // Native: compile + run from inside the project dir (so almide.toml resolves).
    let r = Command::new(&bin)
        .current_dir(dir.path())
        .args(["run", "main.almd"])
        .output()
        .expect("native run");
    assert!(
        r.status.success(),
        "native compile/run failed:\n{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let rust_out = String::from_utf8_lossy(&r.stdout).trim().to_string();

    // WASM: build, then run with wasmtime (skip the run if wasmtime is absent —
    // the native arm and the build still gate, and the byte gate covers wasm32).
    let b = Command::new(&bin)
        .current_dir(dir.path())
        .args(["build", "main.almd", "--target", "wasm", "-o", "out.wasm"])
        .output()
        .expect("wasm build");
    assert!(
        b.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let w = match Command::new("wasmtime")
        .current_dir(dir.path())
        .arg("--dir=/")
        .arg("out.wasm")
        .output()
    {
        Ok(o) if o.status.code() != Some(127) => o,
        _ => return, // wasmtime unavailable
    };
    assert!(
        w.status.success(),
        "wasm execution failed (invalid module?):\n{}",
        String::from_utf8_lossy(&w.stderr)
    );
    let wasm_out = String::from_utf8_lossy(&w.stdout).trim().to_string();

    assert_eq!(
        rust_out, wasm_out,
        "\ncross-module closure mismatch!\nRust: {:?}\nWASM: {:?}",
        rust_out, wasm_out
    );
}

const CROSS_PKG_TOML: &str = "[package]\nname = \"clostest\"\nversion = \"0.1.0\"\n\n[targets]\nwasm = true\n";
const CROSS_PKG_LIB: &str = "pub fn neg() -> (Int) -> Int = (n) => 0 - n\n";

#[test]
fn wasm_cross_module_returned_non_capturing_lambda() {
    assert_cross_target_project(&[
        ("almide.toml", CROSS_PKG_TOML),
        ("src/lib.almd", CROSS_PKG_LIB),
        (
            "main.almd",
            "import self.lib\n\
             fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)\n\
             fn add() -> (Int) -> Int = (n) => n + 1000\n\
             fn main() -> Unit = {\n\
             \x20 println(int.to_string(apply(add(), 5)))\n\
             \x20 println(int.to_string(apply(lib.neg(), 5)))\n\
             }\n",
        ),
    ]);
}

#[test]
fn wasm_cross_module_lambda_without_local_decoy() {
    // No main-program lambda for the module lambda to mis-resolve to: before P0
    // this emitted invalid WASM ("unknown table 0: table index out of bounds").
    assert_cross_target_project(&[
        ("almide.toml", CROSS_PKG_TOML),
        ("src/lib.almd", CROSS_PKG_LIB),
        (
            "main.almd",
            "import self.lib\n\
             fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)\n\
             fn main() -> Unit = {\n\
             \x20 println(int.to_string(apply(lib.neg(), 5)))\n\
             }\n",
        ),
    ]);
}

/// Like `assert_cross_target`, but for programs whose `main` is an `effect fn`.
/// Such a main returns `Result[Unit, _]`, and wasmtime's `_start` prints the
/// wrapped return value (a heap pointer) as a trailing line. Compare only the
/// program's own output (the first `rust_out.lines().len()` lines). A trap
/// still surfaces: `run_wasm` panics when wasmtime exits non-zero.
fn assert_cross_target_effect_main(source: &str) {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() { return; }
    if Command::new("node").arg("--version").output().is_err() { return; }

    let rust_out = run_rust(source);
    let wasm_out = run_wasm(source);
    let rust_lines: Vec<&str> = rust_out.lines().collect();
    let wasm_lines: Vec<&str> = wasm_out.lines().collect();
    assert!(
        wasm_lines.len() >= rust_lines.len(),
        "\nWASM produced fewer lines than Rust (output dropped)!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
    assert_eq!(
        rust_lines.as_slice(),
        &wasm_lines[..rust_lines.len()],
        "\nCross-target mismatch (effect main)!\nRust: {:?}\nWASM: {:?}\nSource:\n{}",
        rust_out, wasm_out, source
    );
}

#[test]
fn wasm_effect_fn_returns_closure_auto_try_binding() {
    // P3-WASM regression: an effect fn returning a closure, bound via auto-`?`.
    // The binding's var type lagged at `Result[Fn, _]` (auto_try runs after
    // lowering), so `add5(10)` mis-resolved to a Named call `add5` instead of a
    // Computed call through the local. WASM then trapped on the unresolved call
    // and Perceus freed the closure before the call. The `!` form always worked;
    // this guards the `?` form (the common case). Expected: 15.
    assert_cross_target_effect_main(
        "effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n\n\
         effect fn main() -> Unit = {\n\
         \x20 let add5 = make_adder_e(5)\n\
         \x20 println(int.to_string(add5(10)))\n\
         }\n",
    );
}

#[test]
fn wasm_effect_fn_returns_closure_used_twice() {
    // Same auto-`?` closure binding, but the closure is called more than once —
    // exercises Perceus inc/dec balance for the binding across multiple uses.
    assert_cross_target_effect_main(
        "effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n\n\
         effect fn main() -> Unit = {\n\
         \x20 let add = make_adder_e(100)\n\
         \x20 println(int.to_string(add(1) + add(2)))\n\
         }\n",
    );
}

#[test]
fn rust_process_exec_forwards_bound_list() {
    // Regression (bug report 2026-06-02): forwarding a *bound* List[String] to
    // process.exec emitted `&[String]` at the call site while the runtime shim took
    // `&Vec<String>`, so `almide check` passed but `almide build` failed (E0308) —
    // the worst failure mode (codegen produces invalid Rust). A List *literal*
    // compiled fine, which made it sneaky. `process.*` is native-only, so this is a
    // Rust-target build+run test: `run_rust` compiles via `almide run` and panics if
    // the build fails, then we assert the output.
    // Skip when the `almide` binary isn't available (e.g. the CI Test Rust job runs
    // `cargo test` without building it) — same guard the cross-target tests use.
    if std::process::Command::new(almide_bin()).arg("--version").output().is_err() { return; }
    let out = run_rust(
        "import process\n\
         effect fn run(cmd: String, a: List[String]) -> String =\n\
         \x20 match process.exec(cmd, a) {\n\
         \x20   ok(o) => o,\n\
         \x20   err(_) => \"\",\n\
         \x20 }\n\
         effect fn main() -> Unit = {\n\
         \x20 let out = run(\"echo\", [\"hello\"])\n\
         \x20 println(out)\n\
         }\n",
    );
    assert_eq!(out, "hello");
}

#[test]
fn wasm_non_copy_mutable_capture_through_closure() {
    // Closure v2 P6: mutating a captured non-Copy `var` through a closure must be
    // visible to the enclosing scope — on BOTH targets. Before P6 it silently
    // returned 0: Rust used `RcCow` (copy-on-write clones on a shared mutation),
    // WASM captured the list by value (no shared cell). Copy scalars already worked
    // (P3 shared cell); this is the non-Copy (`SharedMut` / heap-cell) analogue.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let inner = () => { list.push(acc, 1) }\n\
         \x20 inner()\n\
         \x20 inner()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_nested_non_copy_mutable_capture() {
    // A non-Copy `var` bound inside a closure, mutated by a nested closure — the
    // tail read of the shared cell must not outlive it (a Rust borrow-lifetime
    // hazard) and the cell must thread through the WASM env. Cross-target = 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let outer = () => {\n\
         \x20   var acc: List[Int] = []\n\
         \x20   let inner = () => { list.push(acc, 1) }\n\
         \x20   inner()\n\
         \x20   inner()\n\
         \x20   inner()\n\
         \x20   list.len(acc)\n\
         \x20 }\n\
         \x20 println(int.to_string(outer()))\n\
         }\n",
    );
}

#[test]
fn wasm_sibling_closures_share_mutable_capture() {
    // Closure v2 P6: two SIBLING closures capture the same non-Copy `var` — one
    // mutates it, the other only reads it. The reader must observe the writer's
    // mutation (they share one cell). On WASM the reader was lifted to a
    // ClosureCreate/EnvLoad that loaded the raw cell ptr and used it as the list,
    // so `list.len` read garbage (a leaked pointer) instead of 0; the fix keeps any
    // shared-cell-capturing lambda raw so both closures share one heap cell.
    // wipe() clears -> reader sees len 0. Cross-target = 0.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [7, 8, 9]\n\
         \x20 let wipe = () => { list.clear(xs) }\n\
         \x20 let size = () => { list.len(xs) }\n\
         \x20 wipe()\n\
         \x20 println(int.to_string(size()))\n\
         }\n",
    );
}

#[test]
fn wasm_reader_closure_observes_writer_closure() {
    // Same shared-cell sibling-closure case via in-place `list.pop`: the writer
    // drains 3 of 4 elements, a separate reader closure reports the length. The
    // reader must observe the pops through the shared cell. Cross-target = 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [5, 6, 7, 8]\n\
         \x20 let drain = () => { list.pop(xs) }\n\
         \x20 let report = () => { list.len(xs) }\n\
         \x20 drain()\n\
         \x20 drain()\n\
         \x20 drain()\n\
         \x20 println(int.to_string(report()))\n\
         }\n",
    );
}

#[test]
fn wasm_bytes_mutable_capture_through_closure() {
    // Closure v2 P6: a captured `var buf: Bytes` mutated through a closure must
    // become a shared cell, like List/Map/String. `bytes.push` (and the rest of the
    // bytes `&mut` builders) is an in-place mutator, but bytes' stdlib `mut`
    // annotations are incomplete, so the cell-detection keys off the runtime's
    // actual mutation surface. Before the fix WASM lost the appends (len 0). f()
    // pushes twice -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var buf: Bytes = bytes.new(0)\n\
         \x20 let f = () => { bytes.push(buf, 65) }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(bytes.len(buf)))\n\
         }\n",
    );
}

#[test]
fn wasm_string_push_clear_in_place() {
    // string.push / string.clear had NO WASM dispatch arm — they ICE'd the emitter
    // (native worked). They are `mut s`/in-place mutators in is_inplace_mutator, so
    // a captured String mutated through a closure also relied on this. push appends
    // (via __string_append, write-back like list.push), clear sets len 0. 2 pushes
    // of "ab" -> len 4. Cross-target = 4.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var s: String = \"\"\n\
         \x20 let f = () => { string.push(s, \"ab\") }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(string.len(s)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_stored_in_tuple() {
    // A capture-mutating closure stored in a tuple, destructured into a `let`, and
    // called. Rust typed the binding `(impl Fn(), i64)` → E0562 (impl Trait in a
    // binding type). The let-type's nested Fn subtree is now erased to `_` so Rust
    // infers the concrete closure type. 2 calls -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let pair = (() => { list.push(acc, 1) }, 0)\n\
         \x20 let (g, _) = pair\n\
         \x20 g()\n\
         \x20 g()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_call_closure_through_list_index() {
    // `fs[0]()` — calling a closure indexed out of a list. The parser read `[0](` as
    // a const-generic type-args call (`fs::<0>()`), emitting an invalid bare-name
    // call (native E0425 / wasm trap). `[...]( ` is now a type-args call only when
    // the brackets name a type; an int index makes it `(fs[0])()`. 2 calls -> len 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let fs = [() => { list.push(acc, 1) }]\n\
         \x20 fs[0]()\n\
         \x20 fs[0]()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_in_anonymous_record() {
    // A closure stored in an ANONYMOUS record field, then called via `r.run()`.
    // Rust gave `E0277`: the generic anon-record struct demanded `T: Clone + Debug
    // + PartialEq`, which a closure fails. The struct now derives Clone only when a
    // field is a closure (like a `type`-declared record's `has_fn_fields` path).
    // A `type`-declared record already worked. f pushes twice -> len 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let r = { run: () => { list.push(acc, 1) } }\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_indexassign_noncopy_element_through_closure() {
    // IndexAssign of a NON-Copy element through a closure capturing the list:
    // `xs: List[String]; () => { xs[0] = xs[0] + "!" }`. WASM trapped — the
    // captured cell's `RcDec` ran a TYPED rc_dec over the cell ptr as if it were the
    // list, reading cell[0] (the object ptr) as an element count and decref'ing
    // garbage addresses. A plain rc_dec on the cell (matching the plain rc_inc on
    // capture) fixes it. List[Int] (Copy elems) never hit the element-drop loop.
    // Two appends -> "a!!".
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[String] = [\"a\", \"b\"]\n\
         \x20 let f = () => { xs[0] = xs[0] + \"!\" }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(xs[0])\n\
         }\n",
    );
}

#[test]
fn wasm_closures_stored_in_map() {
    // Two DIFFERENT closures stored in a `Map[String, () -> Unit]`, then one
    // extracted via get_or and called. Rust gave `E0308` — the map's erased `_`
    // value type was inferred from the first closure, so the second (and the get_or
    // default) couldn't unify. The closures going into `map.insert` / `map.get_or`
    // are now `RcWrap`'d to `Rc<dyn Fn>`, so `_` infers one uniform boxed type (as
    // a List[Fn] literal already does). f then h push -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 var m: Map[String, () -> Unit] = map.new()\n\
         \x20 map.insert(m, \"a\", () => { list.push(acc, 1) })\n\
         \x20 map.insert(m, \"b\", () => { list.push(acc, 1) })\n\
         \x20 let f = map.get_or(m, \"a\", () => {})\n\
         \x20 let h = map.get_or(m, \"b\", () => {})\n\
         \x20 f()\n\
         \x20 h()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_typed_param_closure_capturing_mutable() {
    // A closure with a TYPED param `(k: String) => …` that captures a mutated var
    // (so it stays a raw, capture-clone-wrapped closure) dropped the param type in
    // Rust codegen → `move |k| …` → E0282. The bind site now annotates the tail
    // lambda's params through the capture-clone block. Word-count -> 3,2,2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var m: Map[String, Int] = map.new()\n\
         \x20 let bump = (k: String) => {\n\
         \x20   let cur: Int = map.get_or(m, k, 0)\n\
         \x20   map.insert(m, k, cur + 1)\n\
         \x20 }\n\
         \x20 for w in [\"a\", \"b\", \"a\", \"a\", \"b\"] { bump(w) }\n\
         \x20 println(int.to_string(map.get_or(m, \"a\", -1)))\n\
         \x20 println(int.to_string(map.get_or(m, \"b\", -1)))\n\
         \x20 println(int.to_string(map.len(m)))\n\
         }\n",
    );
}

#[test]
fn wasm_reassign_concat_capture_through_closure() {
    // A non-Copy `var` GROWN by reassignment-concat through a closure
    // (`xs = xs + [list.len(xs)]`, reading its own length each step) must behave
    // identically on both targets. On Rust the `xs = xs + [v]` → `xs.push(v)`
    // peephole fired even though `xs` is a `SharedMut`, producing `xs.get().push(v)`
    // — a push onto a discarded clone, so the reassignment was lost (native stayed
    // [0], then panicked on `xs[3]`). The peephole now skips shared cells, leaving a
    // cell-aware `xs.set(xs.get() + [v])`. Three appends -> [0,1,2,3], len 4.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var xs: List[Int] = [0]\n\
         \x20 let app = () => { xs = xs + [list.len(xs)] }\n\
         \x20 app()\n\
         \x20 app()\n\
         \x20 app()\n\
         \x20 println(int.to_string(list.len(xs)))\n\
         }\n",
    );
}

#[test]
fn wasm_module_global_list_mutated_through_closure() {
    // A module-level mutable global (`var g`) mutated through a closure must behave
    // identically on both targets. On Rust the global lowers to a `thread_local!`
    // `ModuleRc`; it was ALSO (wrongly) classified `shared_mut`, so the enclosing
    // read emitted a lowercase `g.get()` that doesn't exist (`error[E0425]`) while
    // the closure body used `G.with(…)`. Globals are now excluded from shared_mut.
    // f() pushes twice -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "var g: List[Int] = []\n\
         effect fn main() -> Unit = {\n\
         \x20 let f = () => { list.push(g, 7) }\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(g)))\n\
         }\n",
    );
}

#[test]
fn wasm_module_global_map_mutated_through_closure() {
    // Same, via `map.insert` invoked as an EXPRESSION on a `ModuleRc` global. The
    // Rust walker only special-cased list push/pop/clear, so map/string/bytes
    // mutators on a global mutated a discarded `(**c.borrow()).clone()` (silently
    // wrong); the mutator set is now the shared `is_inplace_mutator`. Inserts two
    // distinct keys -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "var g: Map[String, Int] = map.new()\n\
         effect fn main() -> Unit = {\n\
         \x20 let f = () => { map.insert(g, \"a\", 1) }\n\
         \x20 let h = () => { map.insert(g, \"b\", 2) }\n\
         \x20 f()\n\
         \x20 h()\n\
         \x20 println(int.to_string(map.len(g)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_selected_by_if_branch() {
    // `let f = if c then A else B` where A, B are distinct closures. Native gave
    // E0308 "if and else have incompatible types" (each branch a different
    // anonymous closure). The unified boxing pass boxes both branches to
    // `Rc<dyn Fn>` so the `if` unifies. 2 calls -> len 2. Cross-target = 2.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let f = if true then (() => { list.push(acc, 1) }) else (() => { list.push(acc, 2) })\n\
         \x20 f()\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_selected_by_match_arm() {
    // Same join via `match`. Native gave E0308 "match arms have incompatible
    // types". Boxing each arm body unifies them. 1 call -> len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let k = 1\n\
         \x20 let f = match k { 1 => (() => { list.push(acc, 10) }), _ => (() => { list.push(acc, 20) }) }\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_in_map_from_list() {
    // A closure as the value of a `map.from_list([(k, closure)])` literal. The old
    // boxing covered `map.insert`/`get_or` only; from_list's closures live inside a
    // list-of-tuples literal. The unified pass boxes Fn positions inside a uniform
    // container's tuple elements, so the inner list-of-tuples is handled. len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let m: Map[String, () -> Unit] = map.from_list([(\"a\", () => { list.push(acc, 1) })])\n\
         \x20 let f = map.get_or(m, \"a\", () => {})\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_pushed_into_list_fn() {
    // `list.push(fs, closure)` onto a `List[() -> Unit]` var. Boxing fired for list
    // LITERALS only; pushing a closure into an existing `List[Fn]` was native
    // E0562 "impl Trait not allowed in paths". The container-mutator rule boxes the
    // pushed closure against the receiver's element type. len 1.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 var fs: List[() -> Unit] = []\n\
         \x20 list.push(fs, () => { list.push(acc, 1) })\n\
         \x20 let f = fs[0]\n\
         \x20 f()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_list_in_record_field() {
    // A list of TWO distinct closures stored in a record field `{ fs: [A, B] }`.
    // The old List[Fn] boxing fired only for a direct `Bind`; here the list is a
    // record field value, so native failed to box (E0308). The unified pass boxes
    // any `List[Fn]` node regardless of parent. 3 calls -> len 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let r = { fs: [() => { list.push(acc, 1) }, () => { list.push(acc, 2) }] }\n\
         \x20 (r.fs[0])()\n\
         \x20 (r.fs[1])()\n\
         \x20 (r.fs[0])()\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_bytes_set_at_shared_through_closure() {
    // `bytes.set_at` had no WASM dispatch arm -> fell through to an ICE in
    // emit_wasm. Native ran fine. A `set_at` arm (in-place index store, no realloc,
    // Unit return) was added. The Bytes buffer is shared by reference across the
    // writer/reader closures, so after the writer sets index 0 to 99 the reader
    // observes 99. Cross-target = 1, then 99, then 99.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var b: Bytes = bytes.from_list([1, 1, 1])\n\
         \x20 let writer = () => { bytes.set_at(b, 0, 99); bytes.get_or(b, 0, -1) }\n\
         \x20 let reader = () => { bytes.get_or(b, 0, -1) }\n\
         \x20 println(int.to_string(reader()))\n\
         \x20 println(int.to_string(writer()))\n\
         \x20 println(int.to_string(reader()))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_as_variant_payload() {
    // A closure stored as a tuple-variant payload `Run(() -> Unit)`. Native gave
    // E0562 "impl Trait not allowed in field types" — the variant field rendered
    // `impl Fn`. The payload type is now `Rc<dyn Fn>`, the enum derives Clone only,
    // and the constructor boxes the closure. One Run fires, one Noop fires. len 2.
    assert_cross_target_effect_main(
        "type Action = Run(() -> Unit) | Noop\n\
         effect fn main() -> Unit = {\n\
         \x20 var acc: List[Int] = []\n\
         \x20 let a = Run(() => { list.push(acc, 1) })\n\
         \x20 match a { Run(f) => f(), Noop => {} }\n\
         \x20 let b: Action = Noop\n\
         \x20 match b { Run(f) => f(), Noop => { list.push(acc, 9) } }\n\
         \x20 println(int.to_string(list.len(acc)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_with_fn_typed_param() {
    // A closure whose own parameter is function-typed: `(g: () -> Unit) => ...`.
    // Native gave E0562 "impl Trait not allowed in closure parameters". The param
    // is now `Rc<dyn Fn>` and the call site boxes the closure it passes. The inner
    // closure runs 3 times, each adding 10 to a captured var -> p=30.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let run3 = (g: () -> Unit) => { g(); g(); g() }\n\
         \x20 var p = 0\n\
         \x20 run3(() => { p = p + 10 })\n\
         \x20 println(\"p=\" + int.to_string(p))\n\
         }\n",
    );
}

#[test]
fn wasm_global_named_rust_keyword() {
    // A global named `box` (a Rust reserved word). The thread_local static is
    // declared `BOX` (raw name uppercased) but reads/writes used the keyword-
    // escaped `r#box`, whose uppercase `R#BOX` is invalid Rust ("unknown prefix").
    // Reads, writes, and the closure mutation now all route through the raw name.
    assert_cross_target_effect_main(
        "var box: List[Int] = []\n\
         effect fn main() -> Unit = {\n\
         \x20 let r = { run: () => { list.push(box, 42) } }\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 r.run()\n\
         \x20 println(int.to_string(list.len(box)))\n\
         }\n",
    );
}

#[test]
fn wasm_global_name_collides_with_stdlib_param() {
    // A mutable global `n: Int` collides by name with the `n` parameter of stdlib
    // numeric helpers (e.g. `int.to_int8_checked(n)`). The storage classifier's
    // by-name fallback misclassified that parameter as the global, so its body
    // read `N.with(...)` (i64) where an f64 was expected -> rustc E0308. The
    // fallback is now restricted to ALMIDE_RT_-prefixed cross-module names.
    assert_cross_target_effect_main(
        "var n: Int = 0\n\
         var xs: List[Int] = []\n\
         var s: String = \"\"\n\
         effect fn main() -> Unit = {\n\
         \x20 let step = () => { n = n + 1; list.push(xs, n); s = s + \"x\" }\n\
         \x20 step(); step(); step()\n\
         \x20 println(int.to_string(n))\n\
         \x20 println(int.to_string(list.len(xs)))\n\
         \x20 println(int.to_string(string.len(s)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_called_as_hof_lambda_param() {
    // A closure that arrives as a higher-order-function lambda PARAMETER, called
    // inside the lambda body: `list.fold(fns, 0, (acc, f) => acc + f(100))`. The
    // call `f(100)` kept the type `fn(Int) -> Int` instead of its return `Int`
    // (the param's concrete type is only fixed by the enclosing fold's
    // unification, after the body was checked), so `acc + f(100)` tripped the IR
    // verifier — a native ICE, and a structurally-broken WASM module. A Computed
    // call's result type is now the callee's Fn return. fns = [+1, *2, -3];
    // fns[0](10) = 11; fold over f(100) = 101 + 200 + 97 = 398.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let fns: List[(Int) -> Int] = [(x) => x + 1, (x) => x * 2, (x) => x - 3]\n\
         \x20 println(int.to_string(fns[0](10)))\n\
         \x20 let total = list.fold(fns, 0, (acc, f) => acc + f(100))\n\
         \x20 println(int.to_string(total))\n\
         }\n",
    );
}

#[test]
fn wasm_variant_payload_parametered_closure() {
    // A variant whose payload is a closure with a non-Unit signature
    // (`Thunk((Int) -> Int)`). The variant field rendered `impl Fn(i64) -> i64`
    // (E0562 in field types) for the parametered closure; it now renders
    // `Rc<dyn Fn>`. Thunk((x) => x*x), matched and called with 9 -> 81.
    assert_cross_target_effect_main(
        "type Node = Leaf(Int) | Thunk((Int) -> Int)\n\
         effect fn main() -> Unit = {\n\
         \x20 let n = Thunk((x) => x * x)\n\
         \x20 let r = match n { Leaf(v) => v, Thunk(f) => f(9) }\n\
         \x20 println(int.to_string(r))\n\
         }\n",
    );
}

#[test]
fn wasm_record_field_list_of_closures() {
    // A struct/record field whose type CONTAINS a closure nested in a container
    // (`stages: List[(Int) -> Int]`). The field type rendered `Vec<impl Fn>`
    // (E0562: impl Trait in field types); `render_type_field_fn` now recurses
    // into List/Map/Tuple, boxing every Fn to `Rc<dyn Fn>`. stages[1] = (x)=>x*3;
    // (stages[1])(10) = 30; len 3.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let r = { stages: [(x) => x + 2, (x) => x * 3], extra: 0 }\n\
         \x20 println(int.to_string((r.stages[1])(10)))\n\
         \x20 println(int.to_string(list.len(r.stages)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_returning_closure_in_list() {
    // Curried closures (`(Int) -> (Int) -> Int`) stored in a list: calling one
    // yields an inner closure, which is then called. The inner returned closure
    // rendered `Box<dyn Fn>` (not Clone → E0599 when cloned at the call site), and
    // its value lacked the trait-object cast (E0271). Nested returned closures are
    // now `Rc<dyn Fn>` with an explicit cast on both the type and the value sides.
    // make_add(10)(5) = 15; make_mul(3)(4) = 12.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let factories: List[(Int) -> (Int) -> Int] = [(n) => (x) => x + n, (n) => (x) => x * n]\n\
         \x20 let make_add = factories[0]\n\
         \x20 let make_mul = factories[1]\n\
         \x20 println(int.to_string((make_add(10))(5)))\n\
         \x20 println(int.to_string((make_mul(3))(4)))\n\
         }\n",
    );
}

#[test]
fn wasm_closure_map_from_hof_then_coalesce() {
    // Closures collected into a map by a HOF (`map.from_list(list.map(...))`),
    // then read with `??`. The map values were a CONCRETE closure type (the
    // boxing pass missed HOF-produced closures), so the `??` fallback (boxed)
    // couldn't unify → E0308. `list.map`'s mapper result is now boxed, so the map
    // is uniformly `Rc<dyn Fn>`. greeters["x"]() -> "hi-x".
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 let names = [\"x\", \"z\"]\n\
         \x20 let greeters: Map[String, () -> String] = map.from_list(list.map(names, (nm) => (nm, () => \"hi-\" + nm)))\n\
         \x20 let gx = greeters[\"x\"] ?? (() => \"?\")\n\
         \x20 println(gx())\n\
         }\n",
    );
}

#[test]
fn wasm_closure_map_set_then_coalesce() {
    // A `Map[String, (Int) -> Int]` built with `map.set` (immutable update), read
    // with `map.get(...) ?? fallback`. The stored closures and the `??` fallback
    // are now both boxed to `Rc<dyn Fn>` (map.set value boxing + the get_or-as-
    // module-call default boxing), so they unify. add(10)=11, neg(7)=-7.
    assert_cross_target_effect_main(
        "effect fn main() -> Unit = {\n\
         \x20 var m: Map[String, (Int) -> Int] = map.new()\n\
         \x20 m = map.set(m, \"add\", (x) => x + 1)\n\
         \x20 m = map.set(m, \"neg\", (x) => 0 - x)\n\
         \x20 let fa = map.get(m, \"add\") ?? ((x) => x)\n\
         \x20 let fn_ = map.get(m, \"neg\") ?? ((x) => x)\n\
         \x20 println(int.to_string(fa(10)))\n\
         \x20 println(int.to_string(fn_(7)))\n\
         }\n",
    );
}
