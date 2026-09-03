// ── eval_test.rs, part 2: sort_by, fan, depth, and mut-param write-back ──
//
// include!-spliced into `tests/eval_test.rs` (the 800-line file discipline,
// #1856); the `lower` / `run` / `expect_out` helpers are the test binary's own.

// ── list.sort_by is KEY-EXTRACTION, not a comparator ─────────────
//
// The stdlib contract is `sort_by[A, B](xs, f: (A) -> B)`: `f` extracts a sort
// KEY from each element and the list is STABLY sorted by the keys' natural
// ordering (native `xs.sort_by_key(|x| f(x.clone()))`, `B: Ord`). These assert
// the interp matches that — and the byte-identical native/wasm output probed in
// /tmp/sort{i,f}.almd.

#[test]
fn sort_by_int_key_is_stable() {
    // Int key with ties: stability must preserve input order among equal keys
    // (native sort_by_key + wasm strict-`>` bubble sort both keep (3,"a") before
    // (3,"c") and (1,"b") before (1,"d")).
    let src = r#"
fn main() -> Unit = {
  let xs = [(3, "a"), (1, "b"), (3, "c"), (1, "d"), (2, "e")]
  let sorted = list.sort_by(xs, (p) => p.0)
  println("${sorted}")
}"#;
    expect_out(src, r#"[(1, "b"), (1, "d"), (2, "e"), (3, "a"), (3, "c")]
"#);
}

#[test]
fn sort_by_negative_int_key() {
    // Negative keys order by signed `i64::cmp` (not unsigned), matching native.
    let src = r#"
fn main() -> Unit = {
  let ns = [3, -1, 0, -5, 2]
  println("${list.sort_by(ns, (n) => n)}")
}"#;
    expect_out(src, "[-5, -1, 0, 2, 3]\n");
}

#[test]
fn sort_by_string_key() {
    // String key: lexicographic `str::cmp`, stable on duplicates. Matches the
    // native/wasm probe (["apple","apple","banana","cherry"]).
    let src = r#"
fn main() -> Unit = {
  let ws = ["banana", "apple", "cherry", "apple"]
  let sorted = list.sort_by(ws, (w) => w)
  println("${sorted}")
}"#;
    expect_out(src, r#"["apple", "apple", "banana", "cherry"]
"#);
}

#[test]
fn sort_by_derived_string_len_key() {
    // The canonical spec example (spec/lang/function_test.almd): sort by a
    // DERIVED Int key (string length), stable on equal lengths.
    let src = r#"
fn main() -> Unit = {
  let xs = ["bb", "a", "ccc", "dd"]
  let sorted = list.sort_by(xs, (s) => string.len(s))
  println("${sorted}")
}"#;
    // lengths 1, 2, 2, 3 → "a", then "bb"/"dd" (input order on the len-2 tie),
    // then "ccc".
    expect_out(src, r#"["a", "bb", "dd", "ccc"]
"#);
}

#[test]
fn sort_by_float_derived_key_is_stable() {
    // A *Float key* is a compile error in both backends (`f64: !Ord`), so it can
    // never reach the interp on a runnable program. But a key DERIVED to an Ord
    // type FROM float elements is fine and common: here we sort float pairs by an
    // Int key. This exercises the key-extraction path over Float-bearing elements
    // and confirms ordering + stability without ever needing a Float key.
    let src = r#"
fn main() -> Unit = {
  let pts = [(2, 3.5), (1, 9.9), (2, 1.1), (1, 0.0)]
  let sorted = list.sort_by(pts, (p) => p.0)
  println("${sorted}")
}"#;
    // Int key 1,1,2,2; ties keep input order → (1,9.9),(1,0.0),(2,3.5),(2,1.1).
    expect_out(src, "[(1, 9.9), (1, 0), (2, 3.5), (2, 1.1)]\n");
}

#[test]
fn sort_by_empty_list() {
    let src = r#"
fn main() -> Unit = {
  let xs: List[Int] = []
  println("${list.sort_by(xs, (n) => n)}")
}"#;
    expect_out(src, "[]\n");
}

// ── fan block materializes a TUPLE (both backends), not a list ───
//
// `fan { a; b; c }` (the block form) lowers to `IrExprKind::Fan` and BOTH
// backends materialize a tuple of the (auto-`?`-unwrapped) results: native
// `(j0, j1, j2)`, wasm a packed tuple. A single-expr fan is the bare value (no
// 1-tuple). Probed byte-identical native/wasm in /tmp/fan{1,2}.almd.

#[test]
fn fan_block_yields_tuple_destructurable() {
    let src = r#"
effect fn double(x: Int) -> Int = x * 2
effect fn main() -> Unit = {
  let r = fan {
    double(1)
    double(2)
    double(3)
  }
  let (a, b, c) = r
  println("${a} ${b} ${c}")
  println("${r}")
}"#;
    // Destructure proves it is a 3-tuple; the repr line proves the display form
    // `(2, 4, 6)` (a list would render `[2, 4, 6]`).
    expect_out(src, "2 4 6\n(2, 4, 6)\n");
}

#[test]
fn fan_block_single_expr_is_bare_value() {
    // Exactly one expr: the result is the bare value, NOT a 1-tuple — matching
    // both backends' single-expr fan path.
    let src = r#"
effect fn double(x: Int) -> Int = x * 2
effect fn main() -> Unit = {
  let r = fan {
    double(5)
  }
  println("${r}")
}"#;
    expect_out(src, "10\n");
}

// ── Recursion depth bound is HOST-STACK-INDEPENDENT ──────────────

#[test]
fn deep_recursion_hits_depth_guard_cleanly() {
    // `sum_to(5000)` nests 5000 interp call frames — past MAX_DEPTH (4000). The
    // evaluator runs on a dedicated big-stack thread, so this terminates as a
    // CLEAN `FuelExhausted` (the depth guard) rather than overflowing the native
    // stack of the default cargo-test worker thread (~2 MiB) and aborting the
    // whole process. This test is itself driven from that default-stack worker,
    // so it is the real regression scenario.
    let src = r#"
fn sum_to(n: Int) -> Int =
  if n <= 0 then 0 else n + sum_to(n - 1)
fn main() -> Unit = {
  println("${sum_to(5000)}")
}"#;
    let ir = lower(src);
    let out = Interpreter::new(&ir).run_main();
    assert_eq!(
        out.status,
        RunStatus::FuelExhausted,
        "deep recursion must trip the depth guard cleanly, not overflow; got {:?}",
        out.status
    );
}

#[test]
fn moderate_recursion_completes_under_depth_guard() {
    // A recursion depth WELL under MAX_DEPTH must complete normally on the
    // big-stack worker — proving the guard does not fire early and the dedicated
    // thread carries the depth a 2 MiB host stack could not (sum_to(3000) blows
    // a 2 MiB native stack but is fine on the interp's worker).
    let src = r#"
fn sum_to(n: Int) -> Int =
  if n <= 0 then 0 else n + sum_to(n - 1)
fn main() -> Unit = {
  println("${sum_to(3000)}")
}"#;
    // 3000 * 3001 / 2 = 4_501_500
    expect_out(src, "4501500\n");
}

// ── #556: Map/Set order-independent ==, NaN-compare IEEE false ──

#[test]
fn map_eq_order_independent() {
    expect_out(
        &main_print(r#"  println("${["a": 1, "b": 2] == ["b": 2, "a": 1]}")"#),
        "true\n",
    );
}

// NOTE: Set `==` order-independence is fixed in value.rs alongside Map, but
// `set.from_list` is not yet bridged in the interp (F5 latent — the fix takes
// effect when the bridge is widened), so no runtime test here.

#[test]
fn nan_compare_is_false_not_abort() {
    expect_out(
        &main_print("  let nan = 0.0 / 0.0\n  println(\"${nan < 1.0} ${nan >= 1.0}\")"),
        "false false\n",
    );
}

// ── #561: huge ranges are fuel-bounded, never materialized ──

#[test]
fn huge_range_for_in_is_fuel_bounded_not_oom() {
    // A 2-billion range with a SMALL fuel budget must terminate as
    // FuelExhausted in O(fuel) steps and O(1) memory — the eager
    // materialization used to allocate the whole range (tens of GB) and
    // abort the process before the first fuel check.
    let src = "fn main() -> Unit = {\n  var s = 0\n  for i in 0..<2000000000 {\n    s = s + 1\n  }\n  println(\"${s}\")\n}";
    let ir = lower(src);
    let out = almide_interp::Interpreter::new(&ir).with_fuel(10_000).run_main();
    assert!(
        matches!(out.status, almide_interp::RunStatus::FuelExhausted),
        "huge range must FuelExhaust, got {:?}",
        out.status
    );
}

// ── #1022: mut-parameter copy-in/copy-out (C-132's interp leg) ──

#[test]
fn mut_param_list_push_reaches_the_caller() {
    // Statement position, value-returning callee in Bind position, and a
    // nested-expression position — every call position writes back.
    let src = r#"
fn push9(mut v: List[Int], x: Int) -> Int = {
  list.push(v, x)
  list.len(v) - 1
}

fn main() -> Unit = {
  var v = [1, 2]
  push9(v, 7)
  let i = push9(v, 8)
  let t = 100 + push9(v, 9)
  println("${v} ${i} ${t}")
}
"#;
    expect_out(src, "[1, 2, 7, 8, 9] 3 104\n");
}

#[test]
fn mut_param_copy_out_is_cow_for_aliases() {
    // An alias bound BEFORE the call keeps its own elements — the copy-out
    // assigns the callee's final buffer into the caller's slot only (the same
    // value-semantics promise as C-033 alias_cow).
    let src = r#"
fn grow(mut v: List[Int]) -> Unit = list.push(v, 9)

fn main() -> Unit = {
  var v = [1]
  let snap = v
  grow(v)
  println("${v} ${snap}")
}
"#;
    expect_out(src, "[1, 9] [1]\n");
}

#[test]
fn mut_param_record_field_argument_writes_back() {
    // The record-FIELD argument form (`push9(b.items, 7)`) — the backends
    // FieldAssign the buffer back; the interp's copy-out mirrors it.
    let src = r#"
type Box = { items: List[Int] }

fn add(mut v: List[Int], x: Int) -> Unit = list.push(v, x)

fn main() -> Unit = {
  var b = Box { items: [1] }
  add(b.items, 5)
  add(b.items, 6)
  println("${b.items}")
}
"#;
    expect_out(src, "[1, 5, 6]\n");
}

#[test]
fn mut_param_chains_through_a_nested_callee() {
    // A callee forwarding its OWN mut param to another mut-param fn: the inner
    // copy-out lands on the outer callee's frame binding, and the outer
    // copy-out carries it the rest of the way to the caller.
    let src = r#"
fn inner(mut v: List[Int]) -> Unit = list.push(v, 2)
fn outer(mut v: List[Int]) -> Unit = {
  list.push(v, 1)
  inner(v)
}

fn main() -> Unit = {
  var v: List[Int] = []
  outer(v)
  println("${v}")
}
"#;
    expect_out(src, "[1, 2]\n");
}

#[test]
fn mut_param_map_insert_reaches_the_caller() {
    // The C-061 shape: a `mut Map` param mutated by `map.insert`, insert-new
    // and overwrite both landing in the caller's slot. (The read-back
    // `map.get_or` route is covered by the real `mut_map_param` fixture in the
    // 3-way gate — its Int-key self-host body is out of scope at this
    // harness's no-mono cut.)
    let src = r#"
fn put(mut m: Map[Int, Int], k: Int, v: Int) -> Unit = map.insert(m, k, v)

fn main() -> Unit = {
  var counts: Map[Int, Int] = [:]
  put(counts, 1, 100)
  put(counts, 1, 111)
  put(counts, 2, 200)
  println("${map.len(counts)}")
  println("${counts}")
}
"#;
    expect_out(src, "2\n[1: 111, 2: 200]\n");
}
