//! #1986 — a user fn's droppable result arrives with exactly ONE owned
//! credit (the callee's ret-inc, or its certainly-fresh alloc), and the
//! caller's bind / assign / return routes must not add another. Before
//! the fix every returned List/Str/Bytes ended at rc 1 forever: 64 B per
//! call, invisible to stdout and pinned silently by the alloc ledger as a
//! higher watermark. Pinned here as a FLAT high-water mark across N.

mod harness;
use harness::run_wasm;

fn program(n: u32, body: &str) -> String {
    format!(
        r#"fn mk(n: Int) -> List[Int] = {{
  let a = [n, n, n]
  a
}}

fn mk2(n: Int) -> List[Int] = [n, n, n]

fn via(n: Int) -> List[Int] = mk(n)

fn take(xs: List[Int]) -> Int = 3

fn bind_then_tail(n: Int) -> Int = {{
  let x = mk(n)
  take(x)
}}

fn grow(n: Int) -> String = {{
  let s = "x" + "y"
  s
}}

fn pair(n: Int) -> (Int, Int) = (n, n + 1)

fn words(n: Int) -> List[String] = ["a", "b"]

fn maybe(n: Int) -> Int? = if n >= 0 then some(n) else none

fn scratch_tail(n: Int) -> String = {{
  let blk = prim.alloc_list(468)
  let p = prim.handle(blk) + 12
  grow(p)
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

fn heap_of(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("credit.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

fn flat(name: &str, body: &str, expect_1000: &str, expect_8000: &str) {
    flat_in(program, name, body, expect_1000, expect_8000);
}

fn flat_in(program: fn(u32, &str) -> String, name: &str, body: &str, expect_1000: &str, expect_8000: &str) {
    let (h1, o1) = heap_of(&program(1000, body));
    let (h8, o8) = heap_of(&program(8000, body));
    assert_eq!(o1, expect_1000, "{name}: output at N=1000");
    assert_eq!(o8, expect_8000, "{name}: output at N=8000");
    assert_eq!(h1, h8, "{name}: the high-water mark must not grow with N (N=1000 {h1} B, N=8000 {h8} B — {} B per call leaked)", (h8 - h1) / 7000);
}

/// #2010 stage 2b: a List of heap handles releases its ELEMENTS with the
/// spine (the typed drop glue) — a list of two fresh strings per call
/// stays flat (was 2 × 32 B per call at stage 2a, spine-only).
#[test]
fn a_list_of_strings_releases_its_elements() {
    flat(
        "let xs = [str(i), str(i + 1)]",
        "    let xs = [int.to_string(i), int.to_string(i + 1)]\n    total = total + list.len(xs)",
        "2000",
        "16000",
    );
}

/// #2010 stage 2b: a spine COPIED from another spine (slice / take /
/// filter / concat / reverse …) holds its own element credits — the copy
/// and the source each release exactly what they hold, flat and without
/// a double free (the double-free trap is armed on the corpus sweep).
#[test]
fn a_copied_spine_holds_its_own_element_credits() {
    flat(
        "let ys = list.slice(xs, 0, 2) + list.reverse(xs)",
        "    let xs = [int.to_string(i), \"b\", \"c\"]\n    let ys = list.slice(xs, 0, 2) + list.reverse(xs)\n    let zs = xs |> list.filter((s) => string.len(s) > 0)\n    total = total + list.len(ys) + list.len(zs)",
        "8000",
        "64000",
    );
}

/// #2010 stage 2c: an Option / Result / tuple block whose payload is a
/// heap handle releases the payload with the block (the typed shape
/// drop) — `some(str)`, `ok(list)` and `(str, list)` per call stay flat.
#[test]
fn a_shape_block_releases_its_handle_payload() {
    flat(
        "let o = some(str); let r: Result[List[Int], String] = ok([i]); let t = (str, [i])",
        "    let o = some(int.to_string(i))\n    let r: Result[List[Int], String] = ok([i])\n    let t = (int.to_string(i), [i, i])\n    let (a, b) = t\n    total = total + string.len(o ?? \"\") + list.len(r ?? []) + string.len(a) + list.len(b)",
        "8780",
        "85780",
    );
}

/// #2010 Map stage b: a Map releases its ENTRIES with its entries array —
/// handle keys and values through the typed entry walk; the functional
/// set / remove copies and a bind of a shared map take their own entry
/// credits; the in-place overwrite releases the value it replaced.
#[test]
fn a_map_releases_its_entries() {
    flat(
        "let m = map.from_list([(str, [str])]); map.insert(m, k, v); let m2 = m",
        "    var m = map.from_list([(int.to_string(i % 7), [int.to_string(i % 7)]), (\"b\", [\"y\"])])\n    map.insert(m, \"c\" + int.to_string(i % 7), [\"z\"])\n    map.insert(m, \"b\", [int.to_string(i % 7)])\n    let m2 = m\n    let m3 = map.set(m2, \"b\", [\"q\"])\n    let m4 = map.remove(m3, \"c\" + int.to_string(i % 7))\n    total = total + map.len(m) + map.len(m2) + map.len(m3) + map.len(m4)",
        "11000",
        "88000",
    );
}

/// #2010 Map stage b, the Set half: a Set releases its MEMBERS; every
/// algebra result and `to_list` copy hold their own member credits.
#[test]
fn a_set_releases_its_members() {
    flat(
        "let s = set.from_list([str, \"b\"]); set.insert / union / to_list",
        "    let s = set.from_list([int.to_string(i % 7), \"b\"])\n    let t = set.insert(s, \"c\" + int.to_string(i % 7))\n    let u = set.union(t, set.from_list([\"d\", int.to_string(i % 7)]))\n    total = total + set.len(s) + set.len(t) + set.len(u) + list.len(set.to_list(u))",
        "13000",
        "104000",
    );
}

/// #2010 stage 2c: a list of options of strings releases every level.
#[test]
fn a_list_of_options_of_strings_releases_every_level() {
    flat(
        "let xs: List[String?] = [some(str), none]",
        "    let xs: List[String?] = [some(int.to_string(i)), none, some(\"k\")]\n    let ys = list.filter(xs, (o) => option.is_some(o))\n    total = total + list.len(xs) + list.len(ys)",
        "5000",
        "40000",
    );
}

#[test]
fn a_bound_call_result_is_released() {
    flat("let x = mk(i)", "    let x = mk(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn a_certainly_fresh_tail_result_is_released() {
    flat("let x = mk2(i)", "    let x = mk2(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn a_call_in_tail_position_hands_over_one_credit() {
    flat("let x = via(i)", "    let x = via(i)\n    total = total + list.len(x)", "3000", "24000");
}

#[test]
fn an_assigned_call_result_is_released() {
    flat(
        "x = mk(i)",
        "    var x = mk(i)\n    x = mk(i + 1)\n    total = total + list.len(x)",
        "3000",
        "24000",
    );
}

#[test]
fn a_fresh_string_result_is_released() {
    flat("let s = grow(i)", "    let s = grow(i)\n    total = total + string.len(s)", "2000", "16000");
}


#[test]
fn a_return_call_releases_the_owned_locals_before_the_jump() {
    // The tail call replaces the frame, so the epilogue never runs — the
    // bound local must be released before the jump (a sibling of #1986,
    // found by the phase-B1 witness: the local's stream ended `iam`).
    flat("let x = mk(i); take(x)", "    total = total + bind_then_tail(i)", "3000", "24000");
}

/// #2004 — a call result used directly as an ARGUMENT moves its one
/// credit into the callee: no share-guard +1 (that is for borrowed
/// args). Before, `take(mk(i))` and `string.len(int.to_string(i))` left
/// the temporary at rc 1 forever (32 B and 16 B per call).
#[test]
fn a_call_result_argument_moves_into_the_callee() {
    flat("take(mk(i))", "    total = total + take(mk(i))", "3000", "24000");
    flat(
        "string.len(int.to_string(i))",
        "    total = total + string.len(int.to_string(i))",
        "2890",
        "30890",
    );
}

/// #2004 — the other READERS of a droppable value born in the expression
/// (arg_temps.rs binds them so the frame owns them): a loop over a call
/// result, a match on one, an index into one, an interpolation of one,
/// nested concatenation over born-here operands.
#[test]
fn a_born_here_value_read_by_a_consumer_is_released() {
    flat("for x in list.range(0, 3)", "    for x in list.range(0, 3) { total = total + x }", "3000", "24000");
    flat(
        "match string.slice(..)",
        "    total = total + (match string.slice(\"abcdef\", 1, 3) { \"bc\" => 1, _ => 0 })",
        "1000",
        "8000",
    );
    flat("list.range(0, 4)[2]", "    total = total + list.range(0, 4)[2]", "2000", "16000");
    flat(
        "\"<${int.to_string(i)}>\"",
        "    total = total + string.len(\"<${int.to_string(i)}>\")",
        "4890",
        "46890",
    );
    flat("mk(i) + [4] + [5, 6]", "    let t = mk(i) + [4] + [5, 6]\n    total = total + list.len(t)", "6000", "48000");
    // `i % 10`: a constant-length digit, so the block class of the held
    // string does not shift with N (a 4-digit tail moved the mark by one
    // block, not per call).
    flat(
        "\"a\" + int.to_string(i % 10) + \"b\"",
        "    let t = \"a\" + int.to_string(i % 10) + \"b\"\n    total = total + string.len(t)",
        "3000",
        "24000",
    );
}

/// The ERROR exits — `f()!` propagating an err, and a raised `err(..)` —
/// are function exits like any other: the frame's owned locals and its
/// droppable params must be released before the `return`. Measured as a
/// per-call growth that must not exceed the ok path's (both paths leak
/// the same Result carrier today; the err path leaked the param and the
/// local on top).
fn err_exit_heap(n: u32, fail: bool) -> (u64, String) {
    let cmp = if fail { ">=" } else { "<" };
    // `guard` PASSES when its condition holds — the failing probe wants
    // the negation.
    let ncmp = if fail { "<" } else { ">=" };
    let src = format!(
        r#"effect fn boom(n: Int) -> Int = if n {cmp} 0 then err("no") else ok(1)

effect fn work(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n, n]
  let v = boom(n)!
  v + list.len(xs) + list.len(y)
}}

effect fn raise(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n]
  if n {cmp} 0 then err("raised") else ok(list.len(xs) + list.len(y))
}}

effect fn guarded(xs: List[Int], n: Int) -> Int = {{
  let y = [n, n]
  guard n {ncmp} 0 else err("guarded")
  ok(list.len(xs) + list.len(y))
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
    total = total + (match work([i, i], i) {{ ok(v) => v, err(_) => 1 }})
    total = total + (match raise([i], i) {{ ok(v) => v, err(_) => 1 }})
    total = total + (match guarded([i], i) {{ ok(v) => v, err(_) => 1 }})
  }}
  println("${{total}}")
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("errexit.almd", &src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

#[test]
fn an_error_exit_releases_the_frame_like_the_ok_exit() {
    let (ok1, o1) = err_exit_heap(1000, false);
    let (ok8, o8) = err_exit_heap(8000, false);
    let (er1, e1) = err_exit_heap(1000, true);
    let (er8, e8) = err_exit_heap(8000, true);
    assert_eq!((o1.as_str(), o8.as_str()), ("12000", "96000"));
    assert_eq!((e1.as_str(), e8.as_str()), ("3000", "24000"));
    let ok_growth = (ok8 - ok1) / 7000;
    let err_growth = (er8 - er1) / 7000;
    assert!(
        err_growth <= ok_growth,
        "the err exit leaks what the ok exit releases: {err_growth} B per call on the err path vs {ok_growth} B on the ok path"
    );
}

/// #2509 — the OK path of `f(x)!` releases the carrier the effect ABI built
/// for the call. The usual shape never showed the gap: arg_temps parks a
/// Result-TYPED operand in a local, whose dec releases the carrier AND the
/// credit it holds on the payload. A MOVE-MODE effect call (the C-132 `mut`
/// parameter rewrite) is typed with the RAW payload, so the park never fired
/// and the carrier had no owner at all — the argument's count grew by one per
/// call and the buffer was never freed. Latent while nothing read the count;
/// #2503's rc-gated copy read it and a 64 KiB buffer through a 20,000-call
/// effect loop went out of memory. Pinned flat across N, with the buffer FRESH
/// per call so the leak is a watermark and not only a count.
#[test]
fn an_effect_call_releases_its_heap_argument_credit() {
    fn mut_param(n: u32, body: &str) -> String {
        format!(
            r#"effect fn poke(mut b: Bytes, v: Int) -> Unit = bytes.set_at(b, 0, v)

effect fn width(b: Bytes) -> Int = ok(bytes.len(b))

effect fn main() -> Unit = {{
  var total = 0
  var held = bytes.new(64)
  var i = 0
  while i < {n} {{
{body}
    i = i + 1
  }}
  println("${{total}}")
}}
"#
        )
    }
    // A fresh buffer per call: a leaked argument credit keeps every one alive.
    flat_in(
        mut_param,
        "poke(fresh buffer)!",
        "    var b = bytes.new(64)\n    poke(b, 7)!\n    total = total + bytes.get_or(b, 0, 0)",
        "7000",
        "56000",
    );
    // One long-lived buffer: the carrier block itself, 32 B per call.
    flat_in(
        mut_param,
        "poke(held buffer)!",
        "    poke(held, 7)!\n    total = total + bytes.get_or(held, 0, 0)",
        "7000",
        "56000",
    );
    // The plain (non-mut) heap argument through the same `!`: the carrier is
    // typed Result and parked in a temporary — the shape that always balanced.
    flat_in(
        mut_param,
        "width(fresh buffer)!",
        "    let t = bytes.new(64)\n    total = total + width(t)!",
        "64000",
        "512000",
    );
}

/// The registry-table tail call (`lower_linked_call`'s `return_call`) is
/// the third tail site — found by scripts/check-exit-sites.sh the day it
/// went in (#1995): a user fn whose tail is `string.to_upper(s)` replaced its
/// frame with no release, so the owned `s` stayed at rc 1 on every call.
#[test]
fn a_linked_tail_call_releases_the_frame() {
    fn heap(n: u32) -> (u64, String) {
        let src = format!(
            r#"fn wrap(s: String) -> String = string.to_upper(s)

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
    // Bound, not inline: a fresh temporary handed to a native op is a
    // leak class of its own (#2004) and would mask this row; `to_upper`
    // rather than `trim` because string_trim leaks inside its own body
    // (#2005, 96 B per call) — the wrapper is what this row measures.
    let s = " x" + "y"
    let t = wrap(s)
    total = total + string.len(t)
  }}
  println("${{total}}")
}}
"#
        );
        let ir = almide_spine::s5::lower_to_ir("linked_tail.almd", &src).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
        let out = run_wasm(&bytes).expect("run");
        assert_eq!(out.exit, 0, "{}", out.stderr);
        (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
    }
    let (h1, o1) = heap(1000);
    let (h8, o8) = heap(8000);
    assert_eq!(o1, "3000");
    assert_eq!(o8, "24000");
    assert_eq!(h1, h8, "linked tail call: the high-water mark must not grow with N (N=1000 {h1} B, N=8000 {h8} B — {} B per call leaked)", (h8 - h1) / 7000);
}

/// #1990 — a `bytes.append_*` loop must grow LINEARLY: the linked impl
/// returns a fresh buffer on every call and the old one has to go.
/// Before the fix the old buffer stayed at rc 1 on every append (the
/// module-space wrapper never released its param at the tail site and a
/// `prim.alloc_*` result carried an extra credit), so N=2000 peaked at
/// 3.84× N=1000 — quadratic.
fn append_heap(n: u32) -> (u64, String) {
    let src = format!(
        r#"effect fn main() -> Unit = {{
  var b = bytes.new(0)
  for i in 0..<{n} {{
    bytes.append_u16_le(b, i)
  }}
  println("${{bytes.len(b)}}")
}}
"#
    );
    let ir = almide_spine::s5::lower_to_ir("append.almd", &src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

#[test]
fn a_registry_tail_call_releases_its_owned_param() {
    let (h1, o1) = append_heap(1000);
    let (h2, o2) = append_heap(2000);
    assert_eq!(o1, "2000");
    assert_eq!(o2, "4000");
    // Linear: doubling N at most doubles the live buffer (plus the fixed
    // floor). The quadratic leak measured 3.84×.
    assert!(
        (h2 as f64) < 2.5 * (h1 as f64),
        "bytes.append_* loop is not linear: N=1000 {h1} B, N=2000 {h2} B"
    );
}

/// #2004, the builtin half: `println` / `eprintln` are Named builtins with
/// no module-call wrapper around them, so `lower_print` is its own
/// wrapper — the fresh string it printed is released right after the
/// write (was 16 B per call).
#[test]
fn a_printed_call_result_is_released() {
    flat(
        "eprintln(int.to_string(i))",
        "    eprintln(int.to_string(i))\n    total = total + 1",
        "1000",
        "8000",
    );
}

/// #2005: a prim-tier body (raw-address rule: releases stay on the
/// epilogue) that ENDS in a tail call handed its owned scratch block to
/// the dead epilogue — `float.to_string` leaked its 4 KB scratch list on
/// every call. Such a frame keeps the call in non-tail form so the
/// epilogue runs (`tail_transfer_ok`).
#[test]
fn a_prim_body_with_owned_locals_releases_them_after_its_tail_call() {
    flat(
        "let s = scratch_tail(i)",
        "    let s = scratch_tail(i)\n    total = total + string.len(s)",
        "2000",
        "16000",
    );
}

/// #2010 stage 1: a flat-payload block — a tuple or an Option of scalars —
/// is droppable like a Str: the bind owns the call's one credit and the
/// exit plan releases it (before: every `(Int, Int)` return and every
/// `some(n)` stayed on the bump graveyard forever).
#[test]
fn a_flat_tuple_result_is_released() {
    flat(
        "let p = pair(i)",
        "    let p = pair(i)\n    let (a, b) = p\n    total = total + b - a",
        "1000",
        "8000",
    );
}

#[test]
fn a_flat_option_result_is_released() {
    flat(
        "let o = maybe(i)",
        "    let o = maybe(i)\n    total = total + (o ?? 0) - i + 1",
        "1000",
        "8000",
    );
}

/// #2010 stage 2a: a List of ANY element type releases its SPINE like a
/// List of scalars (the elements keep their own credits until stage 2b's
/// glue) — every `List[String]` return stayed on the graveyard before.
#[test]
fn a_list_of_strings_spine_is_released() {
    flat(
        "let w = words(i)",
        "    let w = words(i)\n    total = total + list.len(w)",
        "2000",
        "16000",
    );
}

/// #2317: a user fn's VARIANT result hands back one credit too. A
/// constructor call was classed borrowed, so every route took a second
/// credit: `make`'s `if d == 0 then Leaf else Node(…)` returned each node
/// at rc 2 and no tree was ever freed (2.1 MB per `make(16)` round).
const TREES: &str = r#"type Tree = Leaf | Node(Tree, Tree)

type Slot[T] = Empty | Full(T)

fn make(d: Int) -> Tree = if d == 0 then Leaf else Node(make(d - 1), make(d - 1))

fn spine(d: Int) -> Tree = match d {
  0 => Leaf,
  _ => Node(spine(d - 1), Leaf),
}

fn leafy(d: Int) -> Tree = Node(Leaf, Leaf)

fn graft(base: Tree, d: Int) -> Tree = if d == 0 then base else Node(graft(base, d - 1), Leaf)

fn slot(d: Int) -> Slot[Tree] = if d == 0 then Empty else Full(make(d))

fn size(t: Tree) -> Int = match t {
  Leaf => 1,
  Node(l, r) => size(l) + size(r) + 1,
}
"#;

fn tree_program(n: u32, body: &str) -> String {
    format!("{TREES}\neffect fn main() -> Unit = {{\n  var total = 0\n  for i in 0..<{n} {{\n{body}\n  }}\n  println(\"${{total}}\")\n}}\n")
}

#[test]
fn a_constructor_tail_hands_back_one_credit() {
    flat_in(tree_program, "leafy(i)", "    let t = leafy(i)\n    total = total + size(t)", "3000", "24000");
}

#[test]
fn an_if_over_constructors_hands_back_one_credit() {
    flat_in(
        tree_program,
        "make(3), read twice",
        "    let t = make(3)\n    total = total + size(t) + size(t)",
        "30000",
        "240000",
    );
}

#[test]
fn a_match_over_constructors_hands_back_one_credit() {
    flat_in(tree_program, "spine(4)", "    let t = spine(4)\n    total = total + size(t)", "9000", "72000");
}

/// `graft`'s `then` arm returns its borrowed param, its `else` arm a fresh
/// node: each arm hands the join one credit (the borrowed one takes its +1
/// inside), so neither path is returned with two.
#[test]
fn an_if_mixing_a_borrowed_param_and_a_fresh_arm_hands_back_one_credit() {
    flat_in(
        tree_program,
        "graft(keep, 3)",
        "    let keep = make(2)\n    let t = graft(keep, 3)\n    total = total + size(t) + size(keep)",
        "20000",
        "160000",
    );
}

/// A constructor nested in a constructor moves into its slot, and one
/// passed straight to a borrowed param is released after the call.
#[test]
fn a_nested_build_hands_back_one_credit() {
    flat_in(
        tree_program,
        "Node(Node(Leaf, make(2)), spine(1))",
        "    let t = Node(Node(Leaf, make(2)), spine(1))\n    total = total + size(t) + size(Node(make(1), Leaf))",
        "18000",
        "144000",
    );
}

/// A generic case (`Full(T)` of `Slot[T]`) resolves through the call's own
/// type, exactly as its lowering does.
#[test]
fn a_generic_constructor_hands_back_one_credit() {
    flat_in(
        tree_program,
        "slot(i % 3)",
        "    let s = slot(i % 3)\n    total = total + match s {\n      Empty => 0,\n      Full(t) => size(t),\n    }",
        "3330",
        "26663",
    );
}

/// `value.keys(v)` copies the object's key HANDLES into a fresh list, and
/// the list's typed drop releases every element — so the list has to take
/// its own credit per key, as `map.keys` does (#2010 stage 2b). Without the
/// inc, each dropped key list spent one of the Value's credits: two calls
/// freed the keys under the object, and the next allocations overwrote them
/// (`{"beta":20,…}` stringified as `{"�":20,"zzz…3":10,…}`). Found while
/// closing #2515: the leaked destructure tuple of `json.parse`'s key loop
/// had been holding one spare credit per key, which is what one extra
/// release used to spend.
#[test]
fn a_value_keys_list_holds_its_own_key_credits() {
    let src = r#"import json

fn count(b: Value) -> Int = {
  let ks = value.keys(b)
  list.len(ks)
}

fn main() -> Unit = {
  let b = json.parse("{\"beta\":20,\"alpha\":10,\"gamma\":30}") ?? value.null()
  let n = count(b) + count(b) + count(b)
  let junk = list.map([1, 2, 3, 4, 5, 6], (x) => "zzzzzzzzzzzzzzzzzzz" + int.to_string(x))
  println("${n} ${json.stringify(b)} ${json.get_int(b, "alpha") ?? -1}")
  println(list.join(junk, ","))
}
"#;
    let (_, out) = heap_of(src);
    assert_eq!(
        out,
        "9 {\"beta\":20,\"alpha\":10,\"gamma\":30} 10\nzzzzzzzzzzzzzzzzzzz1,zzzzzzzzzzzzzzzzzzz2,zzzzzzzzzzzzzzzzzzz3,zzzzzzzzzzzzzzzzzzz4,zzzzzzzzzzzzzzzzzzz5,zzzzzzzzzzzzzzzzzzz6"
    );
}

/// Per-call high-water growth of `body` in `program`, after checking both
/// outputs — the measurement the two #2515/#2516 cell pairs share.
fn growth_in(program: fn(u32, &str) -> String, name: &str, body: &str, expect_1000: &str, expect_8000: &str) -> u64 {
    let (h1, o1) = heap_of(&program(1000, body));
    let (h8, o8) = heap_of(&program(8000, body));
    assert_eq!(o1, expect_1000, "{name}: output at N=1000");
    assert_eq!(o8, expect_8000, "{name}: output at N=8000");
    (h8 - h1) / 7000
}

/// #2515 — `let (a, b) = <owned tuple>`: the destructure read the fields out
/// of a subject nobody owned, so the tuple and everything it held stayed at
/// rc 1 forever (160 B per call with a fresh 64 B buffer inside). The owned
/// subject is now named first (`let t = mk(i); let (a, b) = t`, arg_temps.rs),
/// so the Bind route owns it and the frame exit releases it; the binds stay
/// borrowed views of its fields, exactly as they are under a written-out name.
fn destructure_program(n: u32, body: &str) -> String {
    format!(
        r#"fn mkt(v: Int) -> (Bytes, Int) = (bytes.new(64), v)

fn mks(v: Int) -> (String, List[Int]) = (int.to_string(v % 10) + "s", [v, v])

fn stamp(mut b: Bytes, v: Int) -> Int = {{
  bytes.set_at(b, 0, v)
  v
}}

effect fn estamp(mut b: Bytes, v: Int) -> Int = {{
  bytes.set_at(b, 0, v)
  ok(v)
}}

fn first(v: Int) -> Bytes = {{
  let (b, k) = mkt(v)
  b
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

#[test]
fn an_owned_tuple_destructure_releases_the_tuple_and_its_contents() {
    let rows = [
        ("let (b, k) = mkt(i)", "    let (b, k) = mkt(i)\n    total = total + bytes.len(b) + k - i", "64000", "512000"),
        (
            "let (s, xs) = mks(i)",
            "    let (s, xs) = mks(i)\n    total = total + string.len(s) + list.len(xs)",
            "4000",
            "32000",
        ),
        ("let (b, _) = mkt(i) in a tail", "    let b = first(i)\n    total = total + bytes.len(b)", "64000", "512000"),
        (
            "stamp(fresh buffer) — the mut-param value return",
            "    var b = bytes.new(64)\n    let r = stamp(b, 7)\n    total = total + r + bytes.get_or(b, 0, 0)",
            "14000",
            "112000",
        ),
        (
            "estamp(fresh buffer)! — the mut-param effect value return",
            "    var b = bytes.new(64)\n    let r = estamp(b, 7)!\n    total = total + r + bytes.get_or(b, 0, 0)",
            "14000",
            "112000",
        ),
    ]
    .map(|(name, body, e1, e8)| (name, growth_in(destructure_program, name, body, e1, e8)));
    let leaked: Vec<_> = rows.iter().filter(|(_, g)| *g != 0).collect();
    assert!(leaked.is_empty(), "B per call leaked: {leaked:?}");
}

/// #2515, the BORROWED-subject cell: a destructure of a var the frame
/// already owns must neither take nor spend a credit on it — it stays at
/// today's (flat) count; an over-release would read freed memory here.
#[test]
fn a_borrowed_tuple_destructure_keeps_its_subject() {
    let g = growth_in(
        destructure_program,
        "let t = mkt(i); let (b, k) = t; read t again",
        "    let t = mkt(i)\n    let (b, k) = t\n    let (c, _) = t\n    total = total + bytes.len(b) + bytes.len(c) + k - i",
        "128000",
        "1024000",
    );
    assert_eq!(g, 0, "borrowed destructure: {g} B per call");
}

/// #2516 — `f(x)?` over an OWNED carrier: the some-cell receives the
/// payload's credit (the carrier's spine is released under it, #2509), so
/// the node is owned and the bind must not add the conservative `+1` that
/// left the cell at rc 1 forever (144 B per call).
fn to_option_program(n: u32, body: &str) -> String {
    format!(
        r#"effect fn mko(v: Int) -> Bytes = if v % 3 == 2 then err("skip") else ok(bytes.new(64))

fn mkr(v: Int) -> Result[Bytes, String] = if v % 3 == 2 then err("skip") else ok(bytes.new(64))

fn width(o: Bytes?) -> Int = match o {{
  some(b) => bytes.len(b),
  none => 1,
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

#[test]
fn an_owned_carrier_to_option_releases_the_some_cell() {
    let g = growth_in(
        to_option_program,
        "let o = mko(i)?",
        "    let o = mko(i)?\n    total = total + width(o)",
        "43021",
        "344042",
    );
    assert_eq!(g, 0, "owned `?`: {g} B per call leaked");
}

/// #2516, the BORROWED-carrier cell: the some-cell's payload slot is a view
/// of a carrier some other holder releases, so the node stays borrowed and
/// the bind keeps its `+1` — the cell is not released (the payload must not
/// be spent twice). Pinned at today's count, which this change must not move.
#[test]
fn a_borrowed_carrier_to_option_keeps_todays_count() {
    let g = growth_in(
        to_option_program,
        "let r: Result = mkr(i); let o = r?",
        "    let r: Result[Bytes, String] = mkr(i)\n    let o = r?\n    let p = r?\n    total = total + width(o) + width(p)",
        "86042",
        "688084",
    );
    assert_eq!(g, 21, "borrowed `?`: {g} B per call (today: the two some-cells, 2 × 16 B on two calls in three)");
}
