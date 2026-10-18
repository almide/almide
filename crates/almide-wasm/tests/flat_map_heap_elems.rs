//! #3756 — `list.flat_map` over heap elements is linear in bytes on the wasm
//! leg.
//!
//! Int / Float chunks rode the amortized push window (#1729) and a scalar
//! literal tail reserved the result (#2980), but a chunk of handles (String,
//! a tuple, …) still took `$concat(acc, chunk)` per source element: a fresh
//! accumulator of the whole length every iteration, so n chunks requested
//! O(n²) bytes — 20,000 Strings reached 1.73 GB under the canonical
//! allocator, 80,000 ran it out of memory. The inspection meter instantiates
//! type variables as Int, so this path was never measured there.
//!
//! The bytes requested must grow linearly with n, and every module balances.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("flat_map_heap_elems.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

/// Bytes requested by the program at `n`, after checking its output and
/// its alloc/free balance.
fn measured(program: fn(u32) -> String, n: u32, want: &str) -> u64 {
    let (out, bytes, a, f) = counts(&program(n));
    assert_eq!(out, want, "n={n}");
    assert_eq!(a, f, "n={n}: leaked {} block(s)", a.abs_diff(f));
    bytes
}

/// Quadrupling n must not take the bytes past six times: a linear path
/// lands near four, the per-element concat near sixteen.
fn assert_linear(name: &str, small: u64, large: u64) {
    assert!(
        large < 6 * small,
        "{name}: 4x the elements requested {large} bytes against {small} ({:.1}x) — a per-element copy of the accumulator",
        large as f64 / small as f64
    );
}

fn strings(n: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  let x = list.range(0, {n}) |> list.map((i) => \"k${{i}}\") |> list.flat_map((k) => [k, k])
  println(\"${{list.len(x)}} ${{list.last(x) ?? \"\"}}\")
}}
"
    )
}

fn tuples(n: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  let x = list.range(0, {n}) |> list.map((i) => (\"a${{i}}\", \"b${{i}}\")) |> list.flat_map(((a, b)) => [(b, a), (a, b)])
  let (p, q) = list.last(x) ?? (\"\", \"\")
  println(\"${{list.len(x)}} ${{p}} ${{q}}\")
}}
"
    )
}

/// A callback returning a captured list: the chunk is borrowed, so its
/// elements are shared into the result and the holder keeps its own.
fn captured(n: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  let pair = [\"x\", \"y\"]
  let x = list.range(0, {n}) |> list.flat_map((_i) => pair)
  println(\"${{list.len(x)}} ${{list.len(pair)}}\")
}}
"
    )
}

#[test]
fn flat_map_over_strings_is_linear() {
    let small = measured(strings, 2000, "4000 k1999\n");
    let large = measured(strings, 8000, "16000 k7999\n");
    assert_linear("String", small, large);
}

#[test]
fn flat_map_over_tuples_of_strings_is_linear() {
    let small = measured(tuples, 2000, "4000 a1999 b1999\n");
    let large = measured(tuples, 8000, "16000 a7999 b7999\n");
    assert_linear("tuple", small, large);
}

#[test]
fn flat_map_returning_a_captured_list_is_linear() {
    let small = measured(captured, 2000, "4000 2\n");
    let large = measured(captured, 8000, "16000 2\n");
    assert_linear("captured", small, large);
}
