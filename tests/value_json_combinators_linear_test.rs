//! #4207: the JSON / Value combinators cost the product of their input sizes,
//! or rebuilt the whole document on every call.
//!
//! - `value.merge(a, b)` scanned the result for every key of b: O(|a|·|b|) on
//!   both legs (80,000 disjoint keys: 13.7 s native, > 35 s wasm).
//! - `value.pick` / `value.omit` scanned the key list for every field: O(n·k)
//!   (80,000 fields and keys: 11.8 s native, 26.9 s wasm).
//! - `value.set_path` cloned the whole document per call natively, so a fold
//!   of n calls copied n² nodes (40,000 fields: 18 s; 40,000 indices: 3.4 s).
//!
//! merge / pick / omit now look keys up through an index built once per call
//! (native: a hash map / set; stock wasm: the self-host `__vu_*_pairs` over the
//! keyed Map; the interp's prim bodies: a sorted index). Native `set_path`
//! updates a document at its last use in place (`@consume(v, x)` on the
//! `almide_rt_value_set_path_steps` intrinsic): only the path is walked, so the array
//! fold is linear; the object fold finds each key through the Object's key
//! index (#4297), so it is linear too.
//!
//! The wasm leg is pinned by wasmtime's FUEL at n and 2n (the 2n budget is
//! twice the n budget, so a super-linear body fails the larger cell first):
//! each budget is about three times what the indexed body spends at n
//! (measured, 40,000: 319M merge, 512M merge-self, 235M pick/omit; 2n about
//! twice that). The old bodies spent 3.5G on merge at 4,000 already. The
//! native leg gets a wall-clock bound at sizes where the old bodies took
//! longer than the bound. Every answer is differed against an expected
//! string, and the wasm answers against native.
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH"
        );
        eprintln!("skipping: wasmtime is not on PATH");
    }
    ok
}

const OBJ: &str = r#"fn obj(p: String, n: Int) -> Value =
  value.object(list.range(0, n) |> list.map((i) => ("${p}${i}", value.int(i))))

fn ends(ks: List[String], mid: Int) -> String =
  "${list.len(ks)} ${list.first(ks) ?? "-"} ${list.get(ks, mid) ?? "-"} ${list.last(ks) ?? "-"}"
"#;

fn merge_src(n: usize) -> String {
    let half = n / 2;
    format!(
        r#"import json

{OBJ}
fn main() -> Unit = {{
  let m = value.merge(obj("a", {n}), obj("b", {n}))
  println(ends(list.map(value.as_object(m) ?? [], (kv) => kv.0), {n}))
  let o = value.merge(obj("k", {n}), value.object(list.range(0, {half}) |> list.map((i) => ("k${{i * 2}}", value.string("v")))))
  println("${{ends(list.map(value.as_object(o) ?? [], (kv) => kv.0), {half})}} ${{json.stringify(value.pick(o, ["k0", "k1", "k2"]))}}")
}}
"#
    )
}

fn merge_want(n: usize) -> String {
    let half = n / 2;
    format!(
        "{} a0 b0 b{}\n{n} k0 k{half} k{} {{\"k0\":\"v\",\"k1\":1,\"k2\":\"v\"}}\n",
        2 * n,
        n - 1,
        n - 1
    )
}

fn merge_self_src(n: usize) -> String {
    format!(
        r#"import json
{OBJ}
effect fn main() -> Unit = {{
  let v = json.parse(json.stringify(obj("k", {n})))!
  let m = value.merge(v, v)
  println("${{ends(list.map(value.as_object(m) ?? [], (kv) => kv.0), 1)}} ${{json.stringify(value.pick(m, ["k1"]))}}")
}}
"#
    )
}

fn merge_self_want(n: usize) -> String {
    format!("{n} k0 k1 k{} {{\"k1\":1}}\n", n - 1)
}

fn pick_src(n: usize) -> String {
    format!(
        r#"{OBJ}
fn main() -> Unit = {{
  let v = obj("k", {n})
  let ks = list.reverse(list.range(0, {n}) |> list.map((i) => "k${{i}}"))
  println("${{ends(list.map(value.as_object(value.pick(v, ks)) ?? [], (kv) => kv.0), 1)}} ${{list.len(list.map(value.as_object(value.omit(v, ks)) ?? [], (kv) => kv.0))}}")
  let evens = list.range(0, {n} / 2) |> list.map((i) => "k${{i * 2}}")
  println("${{ends(list.map(value.as_object(value.omit(v, evens)) ?? [], (kv) => kv.0), 0)}} ${{ends(list.map(value.as_object(value.pick(v, evens)) ?? [], (kv) => kv.0), 1)}}")
}}
"#
    )
}

fn pick_want(n: usize) -> String {
    let half = n / 2;
    format!(
        "{n} k0 k1 k{} 0\n{half} k1 k1 k{} {half} k0 k2 k{}\n",
        n - 1,
        n - 1,
        n - 2
    )
}

fn set_obj_src(n: usize) -> String {
    format!(
        r#"import json
effect fn main() -> Unit = {{
  var acc = value.object([])
  for i in 0..<{n} {{
    acc = value.set_path(acc, value.key(value.root(), "k${{i}}"), value.int(i))!
  }}
  acc = value.set_path(acc, value.key(value.root(), "k0"), value.string("again"))!
  let ks = list.map(value.as_object(acc) ?? [], (kv) => kv.0)
  println("${{list.len(ks)}} ${{list.first(ks) ?? "-"}} ${{list.last(ks) ?? "-"}} ${{json.stringify(value.pick(acc, ["k0", "k1"]))}}")
}}
"#
    )
}

fn set_obj_want(n: usize) -> String {
    format!("{n} k0 k{} {{\"k0\":\"again\",\"k1\":1}}\n", n - 1)
}

fn set_arr_src(n: usize) -> String {
    format!(
        r#"import json
effect fn main() -> Unit = {{
  var acc = value.array(list.repeat(value.int(0), {n}))
  for i in 0..<{n} {{
    acc = value.set_path(acc, value.index(value.root(), i), value.int(i + 1))!
  }}
  acc = value.set_path(acc, value.index(value.root(), {n} - 1), value.string("last"))!
  let xs = value.as_array(acc) ?? []
  println("${{list.len(xs)}} ${{json.stringify(list.first(xs) ?? value.null())}} ${{json.stringify(list.get(xs, {n} - 2) ?? value.null())}} ${{json.stringify(list.last(xs) ?? value.null())}}")
}}
"#
    )
}

fn set_arr_want(n: usize) -> String {
    format!("{n} 1 {} \"last\"\n", n - 1)
}

fn almide(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide")
}

/// Native answer, and how long the built binary took to produce it.
fn native(dir: &Path, src: &str) -> (String, Duration) {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(dir, &["build", "probe.almd", "-o", "probe.bin"]);
    assert!(b.status.success(), "native build failed:\n{}", String::from_utf8_lossy(&b.stderr));
    let t = Instant::now();
    let o = Command::new(dir.join("probe.bin")).output().expect("spawn native");
    let took = t.elapsed();
    assert!(o.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    (String::from_utf8(o.stdout).expect("utf8"), took)
}

/// The stock wasm artifact's answer under a fuel budget.
fn stock_fueled(dir: &Path, src: &str, fuel: u64) -> Result<String, String> {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(dir, &["build", "probe.almd", "--target", "wasm", "-o", "probe.wasm"]);
    assert!(b.status.success(), "wasm build failed:\n{}", String::from_utf8_lossy(&b.stderr));
    let o = Command::new("wasmtime")
        .args(["run", "--dir=.", "-W"])
        .arg(format!("fuel={fuel}"))
        .arg("probe.wasm")
        .current_dir(dir)
        .output()
        .expect("spawn wasmtime");
    if o.status.success() {
        Ok(String::from_utf8(o.stdout).expect("utf8"))
    } else {
        Err(String::from_utf8_lossy(&o.stderr).chars().take(300).collect())
    }
}

type Cell = (&'static str, fn(usize) -> String, fn(usize) -> String);
const MERGE: Cell = ("value.merge (disjoint)", merge_src, merge_want);
const MERGE_SELF: Cell = ("value.merge (v, v)", merge_self_src, merge_self_want);
const PICK_OMIT: Cell = ("value.pick / value.omit", pick_src, pick_want);
const SET_OBJ: Cell = ("value.set_path (object fold)", set_obj_src, set_obj_want);
const SET_ARR: Cell = ("value.set_path (array fold)", set_arr_src, set_arr_want);

/// The wasm cells at n and 2n, each under its budget, each equal to native.
fn wasm_doubling((name, src, want): Cell, n: usize, fuel_n: u64) {
    if !has_wasmtime() {
        return;
    }
    let dir = tempfile::tempdir().expect("scratch");
    for (size, fuel) in [(n, fuel_n), (2 * n, fuel_n * 2)] {
        let program = src(size);
        let (native_out, _) = native(dir.path(), &program);
        assert_eq!(native_out, want(size), "{name} native answer at {size}");
        match stock_fueled(dir.path(), &program, fuel) {
            Ok(got) => assert_eq!(got, native_out, "{name} wasm differs from native at {size}"),
            Err(e) => panic!("{name} at {size} ran out of its {fuel} fuel budget on wasm — O(n·m) again?\n{e}"),
        }
    }
}

#[test]
fn value_merge_is_linear_on_wasm() {
    wasm_doubling(MERGE, 40_000, 1_000_000_000);
}

#[test]
fn value_merge_of_a_value_with_itself_is_linear_on_wasm() {
    wasm_doubling(MERGE_SELF, 40_000, 1_500_000_000);
}

#[test]
fn value_pick_and_omit_are_linear_on_wasm() {
    wasm_doubling(PICK_OMIT, 40_000, 750_000_000);
}

/// #4315: the wasm `set_path` rebuilt every Object it passed (pairs copied,
/// mapped, folded again by `value.object`), so the object fold was O(n²) —
/// 4.04 s at 8,000 keys, four times per doubling — and the array fold
/// super-linear. The structural leg now walks the path in place where the
/// document is the op's alone, and finds a key through the Object's index
/// (crates/almide-wasm/src/value_set_path.rs). Budgets: about three times
/// what the in-place walk spends at n (measured, 20,000: 54M for the object
/// fold, 20M for the array fold; 114M / 41M at 40,000). The copying body
/// spent 45G on the object fold at 20,000.
const OBJ_FOLD_FUEL: u64 = 165_000_000;
const ARR_FOLD_FUEL: u64 = 60_000_000;

#[test]
fn value_set_path_object_fold_is_linear_on_wasm() {
    wasm_doubling(SET_OBJ, 20_000, OBJ_FOLD_FUEL);
}

#[test]
fn value_set_path_array_fold_is_linear_on_wasm() {
    wasm_doubling(SET_ARR, 20_000, ARR_FOLD_FUEL);
}

#[test]
fn native_bodies_stay_fast_at_the_issue_sizes() {
    let dir = tempfile::tempdir().expect("scratch");
    // Old native times at these sizes: merge 13.7 s, merge-self ~20 s at
    // 160,000 (4.6 s at 80,000), pick/omit 11.8 s, the object fold 18 s, the
    // array fold ~14 s at 80,000 (3.4 s at 40,000).
    // `growth` is the full/quarter bound: 8 for a linear body (linear ~4x,
    // quadratic ~16x). The object fold had 32 while set_path found its key by
    // a scan of the pairs — n^2/2 key compares, 14.6x at 40,000 against
    // 10,000 (#4289); the Object's key index makes it linear (#4297).
    for ((name, src, want), size, growth) in [
        (MERGE, 80_000, 8),
        (MERGE_SELF, 160_000, 8),
        (PICK_OMIT, 80_000, 8),
        (SET_OBJ, 40_000, 8),
        (SET_ARR, 80_000, 8),
    ] {
        // The bound is RELATIVE, the way set_map_wasm_linear_test and
        // list_unique_wasm_linear_test do it: the same cell at a quarter of the
        // size, built once and timed interleaved on the same runner. Linear
        // work grows ~4x, quadratic ~16x. An absolute 8 s bound failed CI at
        // 8.5 s (the object fold, develop and #4295 alike) with the bodies
        // unchanged — a loaded runner, not a regression (#4289).
        let quarter = size / 4;
        let full_bin = built(dir.path(), "full", &src(size));
        let small_bin = built(dir.path(), "quarter", &src(quarter));
        let (mut full, mut small) = (Duration::MAX, Duration::MAX);
        for _ in 0..2 {
            let (got, took) = run_bin(&small_bin);
            assert_eq!(got, want(quarter), "{name} native answer at {quarter}");
            small = small.min(took);
            let (got, took) = run_bin(&full_bin);
            assert_eq!(got, want(size), "{name} native answer at {size}");
            full = full.min(took);
        }
        assert!(
            full < small * growth,
            "{name} took {full:?} on native at {size} against {small:?} at {quarter} — quadratic again?"
        );
    }
}

/// The key index (#4297) changes no answer: objects on both sides of its
/// 16-entry threshold — set_path inserts, overwrites and nested steps, a
/// pair list repeating keys, merge, pick, a parsed document repeating a key
/// past the threshold, field lookups — print the same bytes natively and on
/// wasm, and in insertion order (C-401).
#[test]
fn indexed_objects_answer_like_the_pair_scan_on_both_legs() {
    let src = r#"import json

effect fn fold(n: Int) -> Value = {
  var acc = value.object([])
  for i in 0..<n {
    acc = value.set_path(acc, value.key(value.root(), "k${(i * 7) % n}"), value.int(i))!
  }
  acc = value.set_path(acc, value.key(value.root(), "k3"), value.string("again"))!
  acc = value.set_path(acc, value.key(value.key(value.root(), "k5"), "in"), value.int(5))!
  acc = value.set_path(acc, value.key(value.root(), "z"), value.null())!
  acc
}

fn dups(n: Int) -> Value =
  value.object(list.range(0, n) |> list.map((i) => ("d${i % 5}", value.int(i))))

effect fn main() -> Unit = {
  for n in [3, 15, 16, 17, 40] {
    let v = fold(n)!
    println(json.stringify(v))
    println("${json.stringify(value.get(v, "k3") ?? value.null())} ${json.stringify(value.get(v, "k${n - 1}") ?? value.null())} ${json.stringify(value.get(v, "nope") ?? value.null())}")
    let d = dups(n)
    println(json.stringify(d))
    let m = value.merge(v, value.object([("k0", value.bool(true)), ("new", value.int(1)), ("k1", value.int(9))]))
    println(json.stringify(value.merge(m, d)))
    println(json.stringify(value.pick(m, ["new", "k1", "k0", "zz"])))
    let text = json.stringify(v)
    let back = json.parse(text)!
    println(json.stringify(value.set_path(back, value.key(value.root(), "k1"), value.int(-1))!))
    let rep = "${string.drop_end(text, 1)},\"k1\":0}"
    match json.parse(rep) {
      ok(_) => println("accepted"),
      err(e) => println(e),
    }
  }
}
"#;
    let dir = tempfile::tempdir().expect("scratch");
    let (native_out, _) = native(dir.path(), src);
    assert_eq!(native_out.lines().count(), 35, "native answer:\n{native_out}");
    assert_eq!(
        native_out.lines().next(),
        Some(r#"{"k0":0,"k1":1,"k2":2,"k3":"again","k5":{"in":5},"z":null}"#),
        "native answer:\n{native_out}"
    );
    assert!(
        native_out.lines().nth(28).is_some_and(|l| l.starts_with(r#"{"k0":0,"k7":1,"k14":2,"k21":3,"#)),
        "the 40-key fold lost insertion order:\n{native_out}"
    );
    assert!(native_out.contains("json: duplicate key"), "a repeated key past 16 was accepted:\n{native_out}");
    if !has_wasmtime() {
        return;
    }
    match stock_fueled(dir.path(), src, 50_000_000_000) {
        Ok(got) => assert_eq!(got, native_out, "wasm differs from native"),
        Err(e) => panic!("wasm run failed: {e}"),
    }
}

/// Build `src` natively to `<dir>/<stem>.bin`.
fn built(dir: &Path, stem: &str, src: &str) -> std::path::PathBuf {
    let file = format!("{stem}.almd");
    let bin = format!("{stem}.bin");
    std::fs::write(dir.join(&file), src).expect("write");
    let b = almide(dir, &["build", &file, "-o", &bin]);
    assert!(b.status.success(), "native build failed:\n{}", String::from_utf8_lossy(&b.stderr));
    dir.join(bin)
}

/// Run a built binary: its stdout and wall time.
fn run_bin(bin: &Path) -> (String, Duration) {
    let t = Instant::now();
    let o = Command::new(bin).output().expect("spawn native");
    let took = t.elapsed();
    assert!(o.status.success(), "native run failed:\n{}", String::from_utf8_lossy(&o.stderr));
    (String::from_utf8(o.stdout).expect("utf8"), took)
}

/// The interp runs the self-host prim bodies (stdlib/value_core.almd
/// `value_merge`, stdlib/value_utils.almd `__vu_sel`): its vote on small
/// random objects must equal native's.
#[test]
fn the_interp_bodies_agree_with_native_on_small_objects() {
    let src = r#"import json

fn lcg(s: Int) -> Int = (s * 1103515245 + 12345) % 2147483648

fn keyname(r: Int) -> String = match r % 6 {
  0 => "a",
  1 => "b",
  2 => "c",
  3 => "dd",
  4 => "",
  _ => "e",
}

fn gen_pairs(s: Int, n: Int, tag: Int, acc: List[(String, Value)]) -> (Int, List[(String, Value)]) =
  if n <= 0 then (s, acc)
  else {
    let s1 = lcg(s)
    let s2 = lcg(s1)
    gen_pairs(s2, n - 1, tag, acc + [(keyname(s1 / 7), value.int(tag * 100 + s2 % 100))])
  }

fn gen_keys(s: Int, n: Int, acc: List[String]) -> (Int, List[String]) =
  if n <= 0 then (s, acc)
  else {
    let s1 = lcg(s)
    gen_keys(s1, n - 1, acc + [keyname(s1 / 11)])
  }

fn case(seed: Int) -> String = {
  let s0 = lcg(seed + 1)
  let (s1, pa) = gen_pairs(s0, s0 % 7, 1, [])
  let (s2, pb) = gen_pairs(s1, s1 % 7, 2, [])
  let (s3, ks) = gen_keys(s2, s2 % 5, [])
  let a = value.object(pa)
  let b = value.object(pb)
  let scalar = if s3 % 3 == 0 then value.int(7) else a
  "${json.stringify(value.merge(a, b))} ${json.stringify(value.merge(b, a))} ${json.stringify(value.merge(a, a))} ${json.stringify(value.merge(scalar, b))} | ${json.stringify(value.pick(a, ks))} ${json.stringify(value.omit(a, ks))} ${json.stringify(value.pick(scalar, ks))} ${json.stringify(value.omit(scalar, ks))}"
}

fn main() -> Unit = {
  for i in 0..<200 {
    println(case(i))
  }
}
"#;
    let dir = tempfile::tempdir().expect("scratch");
    let (native_out, _) = native(dir.path(), src);
    assert_eq!(native_out.lines().count(), 200);
    let ir = almide::wasm_leg::lower_to_ir("spec/wasm_cross/probe.almd", src).expect("front failed");
    let out = almide_interp::Interpreter::new(&ir).run_main();
    assert!(
        matches!(out.status, almide_interp::RunStatus::Ok),
        "the interp did not run the probe to the end (a vote it must cast): {:?}",
        out.status
    );
    assert_eq!(out.stdout, native_out, "the interp's merge / pick / omit differ from native");
}
