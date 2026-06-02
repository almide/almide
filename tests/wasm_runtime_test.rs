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
