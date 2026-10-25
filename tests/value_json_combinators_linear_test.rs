//! #4207: the JSON / Value combinators cost the product of their input sizes,
//! or rebuilt the whole document on every call.
//!
//! - `value.merge(a, b)` scanned the result for every key of b: O(|a|·|b|) on
//!   both legs (80,000 disjoint keys: 13.7 s native, > 35 s wasm).
//! - `value.pick` / `value.omit` scanned the key list for every field: O(n·k)
//!   (80,000 fields and keys: 11.8 s native, 26.9 s wasm).
//! - `json.set_path` cloned the whole document per call natively, so a fold
//!   of n calls copied n² nodes (40,000 fields: 18 s; 40,000 indices: 3.4 s).
//!
//! merge / pick / omit now look keys up through an index built once per call
//! (native: a hash map / set; stock wasm: the self-host `__vu_*_pairs` over the
//! keyed Map; the interp's prim bodies: a sorted index). Native `set_path`
//! updates a document at its last use in place (`@reuse(j)`, the
//! `almide_json_set_path_owned` twin): only the path is walked, so the array
//! fold is linear; the object fold still compares each new key against the
//! fields so far (an Object is a pair vector with no index), but no longer
//! copies them.
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
        r#"{OBJ}
fn main() -> Unit = {{
  let m = value.merge(obj("a", {n}), obj("b", {n}))
  println(ends(value.keys(m), {n}))
  let o = value.merge(obj("k", {n}), value.object(list.range(0, {half}) |> list.map((i) => ("k${{i * 2}}", value.string("v")))))
  println("${{ends(value.keys(o), {half})}} ${{value.stringify(value.pick(o, ["k0", "k1", "k2"]))}}")
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
  let v = json.parse(value.stringify(obj("k", {n})))!
  let m = value.merge(v, v)
  println("${{ends(value.keys(m), 1)}} ${{value.stringify(value.pick(m, ["k1"]))}}")
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
  println("${{ends(value.keys(value.pick(v, ks)), 1)}} ${{list.len(value.keys(value.omit(v, ks)))}}")
  let evens = list.range(0, {n} / 2) |> list.map((i) => "k${{i * 2}}")
  println("${{ends(value.keys(value.omit(v, evens)), 0)}} ${{ends(value.keys(value.pick(v, evens)), 1)}}")
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
    acc = json.set_path(acc, json.field(json.root(), "k${{i}}"), value.int(i))!
  }}
  acc = json.set_path(acc, json.field(json.root(), "k0"), value.string("again"))!
  let ks = value.keys(acc)
  println("${{list.len(ks)}} ${{list.first(ks) ?? "-"}} ${{list.last(ks) ?? "-"}} ${{value.stringify(value.pick(acc, ["k0", "k1"]))}}")
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
    acc = json.set_path(acc, json.index(json.root(), i), value.int(i + 1))!
  }}
  acc = json.set_path(acc, json.index(json.root(), -1), value.string("last"))!
  let xs = value.as_array(acc) ?? []
  println("${{list.len(xs)}} ${{value.stringify(list.first(xs) ?? value.null())}} ${{value.stringify(list.get(xs, {n} - 2) ?? value.null())}} ${{value.stringify(list.last(xs) ?? value.null())}}")
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
const SET_OBJ: Cell = ("json.set_path (object fold)", set_obj_src, set_obj_want);
const SET_ARR: Cell = ("json.set_path (array fold)", set_arr_src, set_arr_want);

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

#[test]
fn native_bodies_stay_fast_at_the_issue_sizes() {
    let dir = tempfile::tempdir().expect("scratch");
    // Old native times at these sizes: merge 13.7 s, merge-self ~20 s at
    // 160,000 (4.6 s at 80,000), pick/omit 11.8 s, the object fold 18 s, the
    // array fold ~14 s at 80,000 (3.4 s at 40,000).
    for ((name, src, want), size) in [
        (MERGE, 80_000),
        (MERGE_SELF, 160_000),
        (PICK_OMIT, 80_000),
        (SET_OBJ, 40_000),
        (SET_ARR, 80_000),
    ] {
        let (got, took) = native(dir.path(), &src(size));
        assert_eq!(got, want(size), "{name} native answer");
        assert!(took < Duration::from_secs(8), "{name} took {took:?} on native at {size} — quadratic again?");
    }
}

/// The interp runs the self-host prim bodies (stdlib/value_core.almd
/// `value_merge`, stdlib/value_utils.almd `__vu_sel`): its vote on small
/// random objects must equal native's.
#[test]
fn the_interp_bodies_agree_with_native_on_small_objects() {
    let src = r#"fn lcg(s: Int) -> Int = (s * 1103515245 + 12345) % 2147483648

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
  "${value.stringify(value.merge(a, b))} ${value.stringify(value.merge(b, a))} ${value.stringify(value.merge(a, a))} ${value.stringify(value.merge(scalar, b))} | ${value.stringify(value.pick(a, ks))} ${value.stringify(value.omit(a, ks))} ${value.stringify(value.pick(scalar, ks))} ${value.stringify(value.omit(scalar, ks))}"
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
