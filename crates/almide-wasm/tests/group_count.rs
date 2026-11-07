//! Group counts retain key order and callback observations without building
//! per-key element lists. Compare against an explicitly materialized group.
mod harness;
use harness::{run_wasm, RunResult};

fn emit(src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("group_count.almd", src).expect("front");
    let _guard = almide_wasm::alloc_count::CountGuard::set();
    almide_wasm::emit_program(&ir).expect("emit")
}
fn run(src: &str) -> RunResult {
    let r = run_wasm(&emit(src)).expect("run");
    assert_eq!(r.exit, 0, "{}", r.stderr);
    r
}

#[test]
fn group_counts_avoid_group_storage_and_keep_counts() {
    let fused = r#"fn main() -> Unit = {
      let counts = list.range(0, 10000) |> list.group_by((x) => x % 5) |> map.map((xs) => list.len(xs))
      println("${map.get(counts, 3) ?? 0} ${map.len(counts)}")
    }"#;
    let staged = fused.replace("let counts =", "let groups =").replace(" |> map.map((xs) => list.len(xs))", "\nlet counts = map.map(groups, (xs) => list.len(xs))");
    let a = run(fused);
    let b = run(&staged);
    assert_eq!(a.stdout, "2000 5\n");
    assert_eq!(a.stdout, b.stdout);
    let ac = a.alloc_count.unwrap();
    let bc = b.alloc_count.unwrap();
    assert!(ac.bytes + 50_000 < bc.bytes, "counts must not allocate group element storage: {ac:?} / {bc:?}");
    assert_eq!(ac.allocs, ac.frees, "counts leak");
}

#[test]
fn owned_keys_aliases_empty_inputs_and_order_are_preserved() {
    let r = run(r#"fn count(xs: List[String]) -> Map[String, Int] =
      xs |> list.group_by((s) => s + "!") |> map.map((ys) => list.len(ys))
    fn main() -> Unit = {
      let xs = ["b", "a", "b", "é", "a"]
      let counts = count(xs)
      for (key, n) in map.to_list(counts) { println("${key} ${n}") }
      println("${xs[0]} ${xs[4]} ${list.len(xs)}")
      println("${map.len(count([]))}")
      let bs = [true, false, true] |> list.group_by((b) => b) |> map.map((ys) => list.len(ys))
      println("${map.get(bs, true) ?? 0} ${map.get(bs, false) ?? 0}")
    }"#);
    assert_eq!(r.stdout, "b! 2\na! 2\né! 1\nb a 5\n0\n2 1\n");
    let c = r.alloc_count.unwrap();
    assert_eq!(c.allocs, c.frees, "owned keys and source elements must not leak");
}

#[test]
fn key_effects_run_in_source_order_and_nontrivial_counts_stay_staged() {
    let r = run(r#"effect fn main() -> Unit = {
      var log = ""
      let counts = [2, 1, 2] |> list.group_by((x) => {
        log = log + "k${x}"
        x
      }) |> map.map((xs) => list.len(xs))
      println("${log} ${map.get(counts, 2) ?? 0}")
      let plus = [2, 1, 2] |> list.group_by((x) => {
        println("g${x}")
        x
      }) |> map.map((xs) => { println("c${list.len(xs)}"); list.len(xs) + 1 })
      println("${map.get(plus, 2) ?? 0} ${map.get(plus, 1) ?? 0}")
    }"#);
    assert_eq!(r.stdout, "k2k1k2 2\ng2\ng1\ng2\nc2\nc1\n3 2\n");
}

#[test]
fn count_frames_keep_accepted_ownership_certificates() {
    let src = r#"fn counts(xs: List[String]) -> Map[String, Int] =
      xs |> list.group_by((s) => s + "!") |> map.map((ys) => list.len(ys))
    fn streamed_counts(xs: List[String]) -> Map[String, Int] =
      xs |> list.map((s) => s + "!") |> list.group_by((s) => s) |> map.map((ys) => list.len(ys))
    fn main() -> Unit = println("${map.len(counts(["a", "a", "b"]))} ${map.len(streamed_counts(["a", "b"]))}")"#;
    let ir = almide_spine::s5::lower_to_ir("group_count_witness.almd", src).expect("front");
    almide_wasm::witness::start_collecting();
    almide_wasm::emit_program(&ir).expect("emit");
    let rows: std::collections::BTreeMap<_, _> = almide_wasm::witness::take().into_iter().collect();
    for name in ["counts", "streamed_counts"] {
        let cert = rows.get(name).expect("counts certificate");
        assert!(!cert.starts_with('!'), "{name}: {cert}");
        assert!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()), "{name}: {cert}");
    }
}

#[test]
fn mapped_counts_stream_ranges_without_intermediate_storage() {
    let fused = r#"fn main() -> Unit = {
      let counts = list.range(0, 10000) |> list.map((i) => "k${i % 5}") |> list.group_by((s) => s) |> map.map((xs) => list.len(xs))
      println("${map.get(counts, "k3") ?? 0} ${map.len(counts)}")
    }"#;
    let staged = fused.replace("let counts =", "let words =").replace(" |> list.group_by", "\nlet counts = words |> list.group_by");
    let a = run(fused);
    let b = run(&staged);
    assert_eq!(a.stdout, "2000 5\n");
    assert_eq!(a.stdout, b.stdout);
    let ac = a.alloc_count.unwrap();
    let bc = b.alloc_count.unwrap();
    assert!(ac.bytes + 100_000 < bc.bytes, "range and mapped list must disappear: {ac:?} / {bc:?}");
    assert_eq!(ac.allocs, ac.frees, "owned mapped keys must settle on both lookup arms");
}

#[test]
fn mapped_counts_keep_source_snapshots_and_capture_order() {
    let r = run(r#"effect fn main() -> Unit = {
      var xs = ["b", "a", "b"]
      let alias = xs
      var log = ""
      let counts = xs |> list.map((s) => {
        log = log + s
        xs = ["changed"]
        s
      }) |> list.group_by((s) => s) |> map.map((ys) => list.len(ys))
      println("${log} ${map.get(counts, "b") ?? 0} ${xs[0]} ${alias[1]}")
      var ys = [1, 2, 3]
      let nums = ys |> list.map((i) => { ys[1] = 9; i }) |> list.group_by((i) => i) |> map.map((xs) => list.len(xs))
      println("${map.get(nums, 2) ?? 0} ${ys[1]}")
      let empty = list.range(5, 3) |> list.map((i) => { println("unexpected"); i }) |> list.group_by((i) => i) |> map.map((xs) => list.len(xs))
      println("${map.len(empty)}")
    }"#);
    assert_eq!(r.stdout, "bab 2 changed a\n1 9\n0\n");
    let c = r.alloc_count.unwrap();
    assert_eq!(c.allocs, c.frees, "source snapshots and borrowed mapped keys must settle");
}

#[test]
fn mapped_abort_and_nonidentity_key_keep_staged_observations() {
    let fused = r#"effect fn main() -> Unit = {
      let counts = [1, 2, 3] |> list.map((i) => { println("m${i}"); 12 / (3 - i) }) |> list.group_by((i) => i) |> map.map((xs) => list.len(xs))
      println("${map.len(counts)}")
    }"#;
    let staged = fused.replace("let counts =", "let words =").replace(" |> list.group_by", "\nlet counts = words |> list.group_by");
    let a = run_wasm(&emit(fused)).unwrap();
    let b = run_wasm(&emit(&staged)).unwrap();
    assert_ne!(a.exit, 0);
    assert_eq!((a.stdout, a.stderr, a.exit), (b.stdout, b.stderr, b.exit));
    let r = run(r#"effect fn main() -> Unit = {
      let counts = [1, 2] |> list.map((i) => { println("m${i}"); i }) |> list.group_by((i) => { println("g${i}"); i + 1 }) |> map.map((xs) => list.len(xs))
      println("${map.len(counts)}")
    }"#);
    assert_eq!(r.stdout, "m1\nm2\ng1\ng2\n2\n");
}

