//! #3604: a `list.range(a, b)` chain that COLLECTS fails the way the list it
//! stands for fails.
//!
//! The native leg already counts a `list.range(a, b)` chain source instead of
//! building the `Vec<i64>` (`list_range_bounds`). For a collecting chain that
//! rendered `(a..b).map(f).collect::<Vec<_>>()`, and `collect` sizes its vector
//! from the range: on a span no machine can hold it panicked with
//! `capacity overflow` (exit 101, a Rust panic message on stderr), where the
//! wasm leg and `almide_rt_list_range` itself abort with C-197's
//! `Error: out of memory` (exit 1). The chain now fills one vector reserved
//! for the span with `try_reserve_exact` and the C-197 abort — the reservation
//! the runtime twin makes — and then extends it from the counted range.
//!
//! ## Family completeness rule
//!
//! Covered — one row per already-counted collecting shape in `ROWS`, each
//! asserted on the emitted shape, on its output, and on its huge-span abort,
//! native byte-identical to wasm: a `list.range(a, b)` source collected after
//! steps that keep the length (`map`, `enumerate`), so the span IS the
//! result's length.
//!
//! Deliberately left out (asserted in `left_out_shapes_are_unchanged`):
//! - `list.range` collected after a length-changing step (`filter`,
//!   `filter_map`, `flat_map`, `take`): there is no span to reserve, and
//!   `collect` reserves only the step's lower bound, so it never sizes from
//!   the span and has no capacity panic to replace.
//! - the literal `a..<b` and `a...b` collected: they keep the built head
//!   (`range_literal_fold_bounds`), whose reservation is already the C-197
//!   abort. Counting that outer head was measured and not taken: it made the
//!   idiom spellings 12–20% slower under LLVM's generic aarch64 tuning
//!   (research/benchmark/perf/spectralnorm/README.md).
//! - fold-shaped collectors: they build no vector, so they have no failure
//!   form to align.
//! - `list.enumerate(list.range(a, b))` with nothing after it: a runtime call,
//!   not a chain; it already aborts through `almide_rt_list_range`.
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `(name, result type, chain over the span 0..n, expected output for n = 4
/// and n = -2)`.
const ROWS: &[(&str, &str, &str, &str)] = &[
    ("range_map", "List[Int]", "list.map(list.range(0, n), (i) => i * 2)", "[0, 2, 4, 6] []"),
    (
        "range_map_map",
        "List[Float]",
        "list.range(0, n) |> list.map((i) => i + 1) |> list.map((x) => float.from_int(x) / 2.0)",
        "[0.5, 1, 1.5, 2] []",
    ),
    (
        "range_enumerate_map",
        "List[Int]",
        "list.map(list.enumerate(list.range(0, n)), (p) => p.0 * 10 + p.1)",
        "[0, 11, 22, 33] []",
    ),
];

/// 2^62: `try_reserve_exact` refuses it before allocating on every machine,
/// and the wasm leg's i32 floor is far below it, so the abort costs nothing.
const HUGE: &str = "4611686018427387904";

fn program(ty: &str, chain: &str, n: &str) -> String {
    format!(
        "fn shape(n: Int) -> {ty} = {chain}\n\
         effect fn main() -> Unit = {{\n\
         \x20 println(\"start\")\n\
         \x20 println(\"${{shape({n})}} ${{shape(-2)}}\")\n\
         }}\n"
    )
}

struct Run {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn emit(dir: &std::path::Path, src: &str) -> String {
    let path = dir.join("emit.almd");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(almide()).arg(&path).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let i = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}:\n{rust}"));
    rust[i..].split("\n}\n").next().unwrap()
}

/// Build `src` on both legs and run each, returning (native, wasm).
fn run_both(dir: &std::path::Path, tag: &str, src: &str) -> (Run, Run) {
    let path = dir.join(format!("{tag}.almd"));
    std::fs::write(&path, src).unwrap();
    let native = dir.join(format!("{tag}-native"));
    let wasm = dir.join(format!("{tag}.wasm"));
    for (target, artifact) in [("rust", &native), ("wasm", &wasm)] {
        let built = Command::new(almide())
            .arg("build")
            .arg(&path)
            .args(["--target", target, "-o"])
            .arg(artifact)
            .env_remove("ALMIDE_COMPONENT_P3")
            .output()
            .unwrap();
        assert!(built.status.success(), "{tag}: {target} build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    }
    let n = Command::new(&native).output().unwrap();
    let w = Command::new("wasmtime").arg("run").arg(&wasm).output().unwrap();
    let run = |o: std::process::Output| Run { code: o.status.code(), stdout: o.stdout, stderr: o.stderr };
    (run(n), run(w))
}

fn assert_same(tag: &str, n: &Run, w: &Run) {
    let show = |r: &Run| {
        format!("exit {:?}\nstdout: {:?}\nstderr: {:?}", r.code, String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr))
    };
    assert!(
        n.code == w.code && n.stdout == w.stdout && n.stderr == w.stderr,
        "{tag}: native and wasm disagree\n-- native --\n{}\n-- wasm --\n{}",
        show(n),
        show(w)
    );
}

/// The native build ships the v0 codegen for every row — the v1 render walls
/// on a collected list — so `--target rust` is the shipped text. If v1 starts
/// rendering a row, this pin fails and the shape check must read v1's output.
#[test]
fn the_v0_codegen_is_what_ships_for_every_row() {
    for (name, ty, chain, _) in ROWS {
        assert!(
            almide_mir::pipeline::try_render_rust_source(&program(ty, chain, "4")).is_err(),
            "{name}: the v1 native render now ships this program; read its output instead"
        );
    }
}

#[test]
fn every_counted_collecting_range_chain_reserves_its_span() {
    let dir = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (name, ty, chain, _) in ROWS {
        let rust = emit(dir.path(), &program(ty, chain, "4"));
        let body = fn_body(&rust, "shape");
        let reserved = body.contains("try_reserve_exact(")
            && body.contains("almide_abort(\"out of memory\")")
            && body.contains("(__almide_rs..__almide_re)");
        if !reserved || body.contains(".collect::<Vec<_>>()") || body.contains("almide_rt_list_range(") {
            failures.push(format!("{name}: not a counted chain into a span-reserved vector:\n{body}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_row_prints_the_same_on_both_legs() {
    let dir = tempfile::tempdir().unwrap();
    for (name, ty, chain, expected) in ROWS {
        let (n, w) = run_both(dir.path(), name, &program(ty, chain, "4"));
        assert_eq!(n.code, Some(0), "{name}: native failed: {}", String::from_utf8_lossy(&n.stderr));
        assert_eq!(String::from_utf8_lossy(&n.stdout), format!("start\n{expected}\n"), "{name}: native output");
        assert_same(name, &n, &w);
    }
}

/// The span no machine holds: both legs print what ran before it, then abort
/// with C-197's message and exit 1 — never a Rust `capacity overflow` panic.
#[test]
fn every_row_aborts_the_same_on_a_huge_span() {
    let dir = tempfile::tempdir().unwrap();
    for (name, ty, chain, _) in ROWS {
        let tag = format!("{name}_huge");
        let (n, w) = run_both(dir.path(), &tag, &program(ty, chain, HUGE));
        assert_eq!(n.code, Some(1), "{tag}: native exit:\n{}", String::from_utf8_lossy(&n.stderr));
        assert_eq!(String::from_utf8_lossy(&n.stdout), "start\n", "{tag}: native stdout");
        assert_eq!(String::from_utf8_lossy(&n.stderr), "Error: out of memory\n", "{tag}: native stderr");
        assert_same(&tag, &n, &w);
    }
}

/// The family's left-out cells keep their shapes: a filtered `list.range`
/// still collects without a span reservation, a range literal keeps its
/// built head when collected and counts when folded, and the enumerate
/// adapter keeps its `.enumerate()` index.
#[test]
fn left_out_shapes_are_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let rust = emit(
        dir.path(),
        "fn filtered(n: Int) -> List[Int] = list.filter(list.range(0, n), (i) => i % 2 == 0)\n\
         fn literal(n: Int) -> List[Int] = list.map(0..<n, (i) => i * 2)\n\
         fn folded(n: Int) -> Int = list.fold(0..<n, 0, (a, i) => a + i)\n\
         fn indexed(xs: List[Int]) -> Int = list.fold(list.enumerate(xs), 0, (a, p) => a + p.0 * p.1)\n\
         effect fn main() -> Unit = println(\"${filtered(5)} ${literal(3)} ${folded(4)} ${indexed([1, 2])}\")\n",
    );
    let filtered = fn_body(&rust, "filtered");
    assert!(filtered.contains("(0i64..n).filter(") && !filtered.contains("try_reserve_exact("), "{filtered}");
    let literal = fn_body(&rust, "literal");
    assert!(literal.contains("__almide_rv") && !literal.contains("__almide_cv"), "{literal}");
    let folded = fn_body(&rust, "folded");
    assert!(folded.contains("(0i64..n).fold("), "{folded}");
    let indexed = fn_body(&rust, "indexed");
    assert!(indexed.contains(".enumerate().map(|(__ei, __ex)| (__ei as i64, __ex))"), "{indexed}");
}
