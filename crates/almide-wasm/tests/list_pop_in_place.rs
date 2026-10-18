//! #3760 — `list.pop` shrinks the list in place on the wasm leg.
//!
//! Every pop built a shrunken copy of the whole list (`len - 1` slots,
//! memory-copied, every handle element re-credited) and wrote it back into
//! the `mut` receiver, so one pop cost O(n) where native's `Vec::pop` costs
//! O(1): the stdlib inspection measured the marginal cost doubling with n on
//! wasm for Int and String elements alike. After the copy-on-write judge has
//! made the receiver the var's alone, the pop now moves the last slot (and
//! its credit) into the `some` block and decrements the length.
//!
//! The bytes the last 200 of 400 pops request must not grow with the list,
//! a list another binding holds keeps its elements, and every module
//! balances.

mod harness;
use harness::run_wasm;

fn counts(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("list_pop_in_place.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    let c = r.alloc_count.expect("the armed module exports the counters");
    (r.stdout, c.bytes, c.allocs, c.frees)
}

/// The inspection's probe shape: `k` steps of `set(0)` then `pop` over a
/// list of `n` elements built by `build`, each written element `elem`.
fn program(ty: &str, build: &str, elem: &str, n: u32, k: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  var x: List[{ty}] = list.range(0, {n}) |> list.map((i) => {build})
  var acc = 0
  var i = 0
  while i < {k} {{
    x = list.set(x, 0, {elem})
    let r: Option[{ty}] = list.pop(x)
    acc = acc + (if option.is_some(r) then 1 else 0)
    i = i + 1
  }}
  println(\"${{acc}} ${{list.len(x)}}\")
}}
"
    )
}

fn step_bytes(ty: &str, build: &str, elem: &str, n: u32) -> (u64, String) {
    let (out, full, a, f) = counts(&program(ty, build, elem, n, 400));
    let (_, half, a0, f0) = counts(&program(ty, build, elem, n, 200));
    assert_eq!(a, f, "{ty} n={n}: leaked {} block(s)", a as i64 - f as i64);
    assert_eq!(a0, f0, "{ty} n={n} (200 pops): leaked {} block(s)", a0 as i64 - f0 as i64);
    (full - half, out)
}

fn assert_flat(ty: &str, build: &str, elem: &str) {
    for n in [2000u32, 8000] {
        let (got, out) = step_bytes(ty, build, elem, n);
        assert_eq!(out, format!("400 {}\n", n - 400), "{ty} n={n}");
        // A per-pop copy requests the whole list on each of the 200 steps
        // (at least 200 * 4 * 1600 bytes); an in-place pop requests only the
        // `some` block and whatever the written element itself needs. The
        // bound is 256 bytes a step, independent of n.
        let bound = 200 * 256;
        assert!(
            got < bound,
            "{ty}: the last 200 pops requested {got} bytes at n={n}, over {bound} — a per-pop copy"
        );
    }
}

#[test]
fn popping_an_owned_int_list_does_not_copy_it() {
    assert_flat("Int", "i", "i");
}

#[test]
fn popping_an_owned_string_list_does_not_copy_it() {
    assert_flat("String", "int.to_string(i)", "int.to_string(i)");
}

#[test]
fn a_list_another_binding_holds_keeps_its_elements() {
    let src = "effect fn main() -> Unit = {
  var x: List[String] = [\"a\", \"b\", \"c\"]
  let keep = x
  let r = list.pop(x) ?? \"-\"
  let s = list.pop(x) ?? \"-\"
  let k = list.join(keep, \",\")
  println(\"${list.len(keep)} ${list.len(x)} ${r} ${s} ${k}\")
  var e: List[String] = []
  let got = list.pop(e)
  println(\"${option.is_none(got)} ${list.len(e)}\")
}
";
    let (out, _, a, f) = counts(src);
    assert_eq!(out, "3 1 c b a,b,c\ntrue 0\n");
    assert_eq!(a, f, "leaked {} block(s)", a as i64 - f as i64);
}

#[test]
fn popping_through_a_mut_param_is_caller_visible() {
    let src = "fn drain(mut xs: List[String]) -> Int = {
  var n = 0
  while list.len(xs) > 1 {
    let r = list.pop(xs)
    n = n + string.len(r ?? \"\")
  }
  n
}

effect fn main() -> Unit = {
  var x: List[String] = [\"a\", \"bb\", \"ccc\", \"dddd\"]
  let keep = x
  let n = drain(x)
  let j = list.join(x, \",\")
  println(\"${n} ${list.len(x)} ${list.len(keep)} ${j}\")
  x = x + [\"z\"]
  println(list.join(x, \",\"))
}
";
    let (out, _, a, f) = counts(src);
    assert_eq!(out, "9 1 4 a\na,z\n");
    assert_eq!(a, f, "leaked {} block(s)", a as i64 - f as i64);
}
