//! #4661: three shapes the certified native render ran through a list or a
//! copy the program never asked for.
//!
//! - A `list.map` / `list.filter` chain ending in `list.sum`, `list.len` or
//!   `list.fold` built every stage's list (`native_reduce_fusion.rs` folds it
//!   over the source instead).
//! - `for i in list.range(a, b)` built the list of indices
//!   (`native_range_loops.rs` counts instead).
//! - A String list element used as a map key was copied into a new block per
//!   update (`native_hof_owned.rs` `shared_place_arg` shares the element's
//!   handle).
//!
//! - A derived `T.decode` scanned the object for every field
//!   (`native_decode_hint.rs` hands each read its declared slot).
//!
//! Each program prints a fixed text on the native render (with the ownership
//! certificate required) and on wasm, and the allocation lane
//! (`ALMIDE_ALLOC_COUNT`, the counting allocator of
//! `native_borrow_oracle_test.rs`) pins what stdout cannot see: how many
//! blocks the program's own work allocates. Staged, each row's count grows
//! with its element count (thousands here); the bounds below are fixed.
//! No wall-clock assertion.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-reduce-fusion-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>, alloc: bool) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if alloc {
        cmd.env("ALMIDE_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The counting allocator's `allocs` for `src`.
fn allocs(src: &Path) -> u64 {
    let (_, stderr) = run(src, None, true);
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

/// The blocks `body`'s own work allocates: its count less that of a program
/// that only prints (the process's fixed cost, which differs by platform).
fn net_allocs(tag: &str, source: &str) -> u64 {
    let dir = scratch(tag);
    let (src, base) = (dir.join("prog.almd"), dir.join("baseline.almd"));
    std::fs::write(&src, source).unwrap();
    std::fs::write(&base, "fn main() -> Unit = println(\"baseline\")\n").unwrap();
    let net = allocs(&src).saturating_sub(allocs(&base));
    let _ = std::fs::remove_dir_all(&dir);
    net
}

/// `source` prints `expected` on the native render and on wasm.
fn prints(tag: &str, source: &str, expected: &str) {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target, false);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const CHAINS: &str = r#"fn kept(xs: List[Int]) -> Int = xs |> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0) |> list.len

fn main() -> Unit = {
  let a = list.range(0, 20000) |> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0) |> list.sum
  let b = list.range(0, 20000) |> list.map((x) => x + 1) |> list.sum
  let c = list.range(0, 20000) |> list.filter((x) => x % 3 == 0) |> list.map((x) => x * x) |> list.fold(7, (acc, x) => acc + x % 5)
  let d = list.range(5, 2) |> list.map((x) => x * 3) |> list.filter((x) => x > 0) |> list.sum
  let e = list.range(0, 20000) |> list.map((x) => float.from(x) * 0.5) |> list.filter((x) => x > 100.0) |> list.fold(0.0, (acc, x) => acc + x)
  println("${a} ${b} ${c} ${d} ${e} ${kept([1, 2, 3, 4, 5, 6])}")
}
"#;

const COUNTED_LOOP: &str = r#"fn main() -> Unit = {
  var t = 0
  for i in list.range(0, 20000) {
    if i % 7 == 0 then { continue } else ()
    if i > 15000 then { break } else ()
    t = t + i
  }
  var n = 5
  var rounds = 0
  for _i in list.range(0, n) {
    n = n + 10
    rounds = rounds + 1
  }
  for _i in list.range(4, 4) {
    rounds = rounds + 100
  }
  println("${t} ${rounds} ${n}")
}
"#;

const KEYED_COUNT: &str = r#"fn main() -> Unit = {
  let vocab = ["alpha", "be", "gamma-gamma-gamma-gamma-gamma-gamma", "", "delta"]
  var counts: Map[String, Int] = [:]
  for i in list.range(0, 5000) {
    let w = vocab[(i * 7 + 3) % 5]
    counts[w] = (map.get(counts, w) ?? 0) + 1
  }
  var by_id: Map[Int, Int] = [:]
  for i in list.range(0, 5000) {
    let k = i % 9
    by_id[k] = (map.get(by_id, k) ?? 0) + i
  }
  for (w, c) in map.to_list(counts) {
    println("${w}=${c}")
  }
  println("${map.len(by_id)} ${map.get(by_id, 4) ?? -1} ${list.len(vocab)}")
}
"#;

/// Callbacks whose order is visible keep the staged order: every element
/// passes one stage before the next stage starts.
const STAGED_ORDER: &str = r#"fn seen(tag: String, x: Int) -> Int = {
  println("${tag}${x}")
  x
}

fn main() -> Unit = {
  let a = list.range(0, 3) |> list.map((x) => seen("m", x) * 2) |> list.filter((x) => seen("f", x) > 0) |> list.sum
  let b = list.range(0, 3) |> list.map((x) => seen("m", x)) |> list.fold(0, (acc, x) => acc + seen("r", x))
  println("${a} ${b}")
}
"#;

/// A derived decode over documents in declaration order, reordered, short of
/// a field, and not an object: the slot is only where the lookup starts.
const DECODE: &str = r#"import json

type Inner: Codec = { city: String, zip: String }

type Outer: Codec = { id: Int, name: String, inner: Inner, on: Bool }

fn show(doc: String) -> String = match json.parse(doc) {
  ok(v) => match Outer.decode(v) {
    ok(o) => "${o.id} ${o.name} ${o.inner.city} ${o.inner.zip} ${o.on}",
    err(e) => "err: ${e}",
  },
  err(e) => "parse: ${e}",
}

fn main() -> Unit = {
  println(show("{\"id\": 1, \"name\": \"a\", \"inner\": {\"city\": \"c\", \"zip\": \"z\"}, \"on\": true}"))
  println(show("{\"on\": false, \"inner\": {\"zip\": \"z2\", \"city\": \"c2\"}, \"name\": \"b\", \"id\": 2}"))
  println(show("{\"name\": \"id\", \"id\": 3, \"extra\": 0, \"on\": true, \"inner\": {\"city\": \"zip\", \"zip\": \"city\"}}"))
  println(show("{\"id\": 4, \"name\": \"d\", \"inner\": {\"city\": \"c\"}, \"on\": true}"))
  println(show("{\"id\": 5, \"name\": \"e\", \"inner\": 7, \"on\": true}"))
  println(show("[1, 2]"))
}
"#;

#[test]
fn a_derived_decode_reads_the_same_fields_whatever_their_order() {
    prints("decode", DECODE, "1 a c z true\n2 b c2 z2 false\n3 id zip city true\nerr: missing field 'zip', at inner\nerr: expected Object, received Int, at inner\nerr: expected Object, received Array\n");
}

#[test]
fn a_derived_decode_starts_each_field_read_at_its_declared_slot() {
    let dir = scratch("decode-shape");
    let src = dir.join("prog.almd");
    std::fs::write(&src, DECODE).unwrap();
    let out = Command::new(almide())
        .arg("build").arg(&src).arg("-o").arg(dir.join("prog"))
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join("project"))
        .output()
        .expect("almide");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = std::fs::read_to_string(dir.join("project/src/main.rs")).expect("the generated program");
    for (key, slot) in [("id", 0), ("name", 1), ("inner", 2), ("on", 3), ("city", 0), ("zip", 1)] {
        let read = format!("almide_rt_value_field_ref_at(v0, \"{key}\", {slot}usize)");
        assert!(rust.contains(&read), "no `{read}` in the generated program");
    }
    assert!(!rust.contains("almide_rt_value_field_ref(v0, \""), "a literal-key read of a derived decode still scans from the start");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_chain_ending_in_a_reduction_prints_the_same_on_both_targets() {
    prints("chains", CHAINS, "299970000 200010000 13341 0 99984950 3\n");
}

#[test]
fn a_loop_over_list_range_counts_once_from_bounds_read_before_it() {
    prints("loop", COUNTED_LOOP, "96441429 5 55\n");
}

#[test]
fn a_keyed_count_by_a_list_element_prints_the_same_on_both_targets() {
    prints("keyed", KEYED_COUNT, "=1000\nalpha=1000\ngamma-gamma-gamma-gamma-gamma-gamma=1000\ndelta=1000\nbe=1000\n9 1390834 5\n");
}

#[test]
fn callbacks_that_print_run_stage_by_stage() {
    prints("order", STAGED_ORDER, "m0\nm1\nm2\nf0\nf2\nf4\nm0\nm1\nm2\nr0\nr1\nr2\n6 3\n");
}

#[test]
fn a_fused_chain_builds_no_list() {
    // Staged, each chain allocates the lists its stages fill.
    let n = net_allocs("chains-alloc", CHAINS);
    assert_eq!(n, 0, "the chains allocate {n} blocks of their own; fused they build no stage list");
}

#[test]
fn a_loop_over_list_range_builds_no_index_list() {
    // The loop whose body changes its own bound counts too: its bound is
    // bound ahead of it (#4662), as the list's length was read before the
    // first iteration.
    let n = net_allocs("loop-alloc", COUNTED_LOOP);
    assert_eq!(n, 0, "the loops allocate {n} blocks; counted they build no index list");
}

#[test]
fn a_string_key_read_from_a_list_is_not_copied_per_update() {
    // One key copy per update would be 5000 blocks; the maps and the
    // vocabulary themselves are a few dozen.
    let n = net_allocs("keyed-alloc", KEYED_COUNT);
    assert!(n < 100, "the keyed counts allocate {n} blocks: a key block per update is back");
}
