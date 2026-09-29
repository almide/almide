//! #2932 — `map.get_or` / `list.get_or` hand back an OWNED value and
//! release everything else they touched, so a loop over them balances:
//! every block the module allocates is freed (`allocs == frees`).
//!
//! Before the fix both arms declared their result a view. The default is
//! lowered with a Retain credit, so a heap default (`some(0)`, a computed
//! string, a list) leaked one block per call, hit or miss; and
//! `list.get_or`'s hit leaked the Option shell `$list_get` allocates even
//! for a scalar list. The printed output was right — the leak showed only
//! in the counters, or as an OOM under `--heap-cap` (heap_cap_test's
//! map-literal churn).
//!
//! Each probe's stdout is the native leg's, recorded from `almide run`.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("get_or.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.allocs, c.frees)
}

/// The issue's repro, both halves: a hit and a miss per iteration over a
/// `Map[String, Option[Int]]` with an Option default.
const MAP_OPTION_DEFAULT: &str = r#"effect fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 1000 {
    let m = ["k1": some(i + 1)]
    acc = acc + (map.get_or(m, "k1", some(0)) ?? 0)
    acc = acc + (map.get_or(m, "k2", some(1)) ?? 0)
    i = i + 1
  }
  println(int.to_string(acc))
}
"#;

/// `list.get_or` over a scalar list: the hit's Option shell.
const LIST_SCALAR: &str = r#"effect fn main() -> Unit = {
  var i = 0
  var acc = 0
  while i < 1000 {
    let xs = [i, 2]
    acc = acc + list.get_or(xs, 0, 7)
    acc = acc + list.get_or(xs, 9, 7)
    i = i + 1
  }
  println(int.to_string(acc))
}
"#;

/// Heap values of every shape both arms select between — Option, String,
/// List, record — as fresh defaults, a shared (let-bound) default reused
/// across iterations, and results that outlive their container.
const HEAP_SHAPES: &str = r#"type P = { name: String, n: Int }

fn pick(m: Map[String, List[String]], k: String, d: List[String]) -> List[String] = map.get_or(m, k, d)

effect fn main() -> Unit = {
  let d = ["dflt"]
  let dp = { name: "zz", n: 0 }
  var out: List[String] = []
  for i in 0..<200 {
    let m = ["a": ["x" + int.to_string(i), "y"], "b": ["q"]]
    let r1 = pick(m, "a", d)
    let r2 = pick(m, "c", d)
    let r3 = map.get_or(m, "b", d)
    let ps = [{ name: "p" + int.to_string(i), n: i }]
    let q = list.get_or(ps, 0, dp)
    let q2 = list.get_or(ps, 3, dp)
    let so = ["s": some("v" + int.to_string(i))]
    let o = map.get_or(so, "s", none) ?? "none"
    let o2 = map.get_or(so, "t", some("w")) ?? "none"
    let ls = ["a" + int.to_string(i), "b"]
    let e = list.get_or(ls, 1, "c" + int.to_string(i))
    if i % 50 == 0 then {
      out = out + [list.join(r1, ","), list.join(r2, ","), int.to_string(list.len(r3)), q.name, q2.name, o, o2, e]
    } else ()
  }
  println(list.join(out, "|"))
  println(list.join(d, ",") + dp.name)
}
"#;

#[test]
fn map_get_or_with_an_option_default_frees_every_block() {
    let (out, allocs, frees) = counts(MAP_OPTION_DEFAULT);
    assert_eq!(out, "501500\n");
    assert_eq!(allocs, frees, "map.get_or leaked {} block(s)", allocs - frees);
}

#[test]
fn list_get_or_frees_the_option_shell_of_a_hit() {
    let (out, allocs, frees) = counts(LIST_SCALAR);
    assert_eq!(out, "506500\n");
    assert_eq!(allocs, frees, "list.get_or leaked {} block(s)", allocs - frees);
}

#[test]
fn get_or_over_heap_values_balances_and_keeps_shared_defaults_alive() {
    let (out, allocs, frees) = counts(HEAP_SHAPES);
    assert_eq!(
        out,
        "x0,y|dflt|1|p0|zz|v0|w|b|x50,y|dflt|1|p50|zz|v50|w|b|x100,y|dflt|1|p100|zz|v100|w|b|x150,y|dflt|1|p150|zz|v150|w|b\n\
         dfltzz\n"
    );
    assert_eq!(allocs, frees, "get_or over heap values leaked {} block(s)", allocs - frees);
}
