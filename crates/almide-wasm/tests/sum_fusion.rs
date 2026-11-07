//! Sum shares fold's safe stream lowering: no intermediate list, sequential
//! arithmetic, staged effects/traps, and the same per-element ownership proof.
mod harness;
use harness::{run_wasm, RunResult};

fn run(src: &str) -> RunResult {
    let ir = almide_spine::s5::lower_to_ir("sum_fusion.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("emit")
    };
    run_wasm(&bytes).expect("run")
}

fn expression(expr: &str) -> RunResult {
    run(&format!("fn main() -> Unit = {{ let total = {expr}\n println(\"${{total}}\") }}"))
}

#[test]
fn range_map_filter_sum_allocates_no_lists() {
    for n in [1000_i64, 100_000] {
        for head in [format!("list.range(0, {n})"), format!("(0..<{n})")] {
            let r = expression(&format!("{head} |> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0) |> list.sum"));
            assert_eq!(r.exit, 0, "{}", r.stderr);
            assert_eq!(r.stdout, format!("{}\n", (0..n).map(|x| x * 3).filter(|x| x % 2 == 0).sum::<i64>()));
            let c = r.alloc_count.expect("counters");
            assert_eq!((c.allocs, c.bytes, c.frees), (0, 0, 0), "the numeric chain must allocate nothing");
        }
    }
}

#[test]
fn sum_preserves_empty_ranges_wrapping_and_float_left_order() {
    for (expr, want) in [
        ("list.range(5, 2) |> list.map((x) => x + 1) |> list.sum", "0\n"),
        ("list.range(0, 4) |> list.filter((x) => x < 0) |> list.sum", "0\n"),
        ("[9223372036854775807, 1] |> list.map((x) => x) |> list.sum", "-9223372036854775808\n"),
        ("[10000000000000000.0, 1.0, -10000000000000000.0] |> list.map((x) => x) |> list.sum", "0\n"),
        ("[1.0, 2.0, 3.0] |> list.map((x) => x / 4.0) |> list.sum", "1.5\n"),
        ("list.range(9223372036854775805, 9223372036854775807) |> list.map((x) => x) |> list.sum", "-5\n"),
    ] {
        let r = expression(expr);
        assert_eq!(r.exit, 0, "{expr}: {}", r.stderr);
        assert_eq!(r.stdout, want, "{expr}");
    }
}

#[test]
fn fused_owned_map_results_are_released_on_kept_and_skipped_edges() {
    let r = run(r#"fn total(xs: List[String]) -> Int = xs
      |> list.map((s) => s + "!")
      |> list.filter((s) => s != "drop!")
      |> list.map((s) => string.len(s))
      |> list.sum
    fn main() -> Unit = {
      let xs = ["ab", "drop", "c"]
      println("${total(xs)} ${xs}")
    }"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "5 [\"ab\", \"drop\", \"c\"]\n");
    let c = r.alloc_count.expect("counters");
    assert_eq!(c.allocs, c.frees, "owned stage values must not leak");
}

#[test]
fn effectful_and_captured_write_stages_keep_stage_order() {
    let r = run(r#"effect fn main() -> Unit = {
      var log = ""
      let total = [1, 2] |> list.map((x) => {
        log = log + "m${x}"
        println("m${x}")
        x * 10
      }) |> list.filter((x) => {
        log = log + "f${x}"
        println("f${x}")
        x > 10
      }) |> list.sum
      println("${total} ${log}")
    }"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "m1\nm2\nf10\nf20\n20 m1m2f10f20\n");
}

#[test]
fn two_partial_stages_keep_the_first_stages_abort() {
    // The first element would fail the second stage, but the first stage
    // must finish (and fail on element 2) before the second stage starts.
    let r = expression("[1, 0] |> list.map((x) => 10 / x) |> list.map((x) => [7][x]) |> list.sum");
    assert_ne!(r.exit, 0);
    assert!(r.stderr.contains("division by zero"), "first stage must abort: {}", r.stderr);
}

#[test]
fn captured_map_writes_keep_stage_order() {
    let r = run(r#"effect fn main() -> Unit = {
  var seen = [0: 0]
  let total = [1, 2] |> list.map((x) => {
    seen[x] = x
    x
  }) |> list.filter((x) => map.len(seen) == 3) |> list.sum
  println("${total}")
}
"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "3\n");
}
