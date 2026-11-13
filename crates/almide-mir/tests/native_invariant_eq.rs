//! #4661: work the certified native render repeated per loop round although
//! nothing it reads can change, each with the shape that must keep it.
//!
//! - An equality of heap operands a loop cannot write is compared once and
//!   remembered (`native_invariant_eq`): the comparison is rendered once
//!   however many sites of the loop spell it. An operand the loop writes or
//!   binds, a comparison inside a callback and a scalar comparison stay
//!   where they are, one comparison per site.
//! - An in-place map update whose key is a borrowed string place hands the
//!   place by reference (`render_native_inplace`, the `_by_ref` twin): the
//!   key's handle is shared only by an entry the update adds. Any other key
//!   keeps the by-value twin.
//! - A copy of a borrowed element read is made by reading the element's
//!   place again only while the index it was read at stands
//!   (`moved_index_reads`).

fn certified(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source_certified(&src).expect("renders on v1").0)
        .expect("spawn")
        .join()
        .expect("join")
}

fn body<'a>(rust: &'a str, f: &str) -> &'a str {
    let head = if f == "main" { "fn main() {".to_string() } else { format!("fn almd_{f}(") };
    let at = rust.find(&head).unwrap_or_else(|| panic!("no `{f}`:\n{rust}"));
    let b = &rust[at..];
    &b[..b.find("\n}\n").unwrap_or(b.len())]
}

/// `fn run(a: P, b0: P, n: Int) -> Int` whose loop is `rounds`, called once.
fn compare_loop(before: &str, rounds: &str) -> String {
    format!(
        "type P = {{ name: String, xs: List[Int] }}\n\
         fn run(a: P, b0: P, n: Int) -> Int = {{\n  var acc = 0\n  {before}\n  for i in 0..<n {{\n{rounds}\n  }}\n  acc\n}}\n\
         fn main() -> Unit = {{\n  let p: P = {{ name: \"n\", xs: [1, 2] }}\n  println(int.to_string(run(p, p, 3)))\n}}\n"
    )
}

/// The comparisons of record names `run` renders.
fn name_compares(rust: &str) -> usize {
    body(rust, "run").matches("rt_string_eq(").count()
}

const TWO_SITES: &str = "    if a == b then { acc = acc + 1 } else { () }\n    let same = a == b\n    if same then { acc = acc + i } else { () }";

#[test]
fn an_equality_of_operands_the_loop_cannot_write_is_compared_once() {
    let rust = certified(&compare_loop("let b = b0", TWO_SITES));
    assert_eq!(name_compares(&rust), 1, "two sites over fixed operands are not one comparison:\n{}", body(&rust, "run"));
    // Field places of fixed operands are fixed too, and `!=` is remembered
    // apart from `==`.
    let fields = "    if a.xs == b.xs then { acc = acc + 1 } else { () }\n    if a.xs != b.xs then { acc = acc + 2 } else { () }\n    if a.xs == b.xs then { acc = acc + i } else { () }";
    let rust = certified(&compare_loop("let b = b0", fields));
    assert_eq!(body(&rust, "run").matches("almide_rt_list_eq_int(").count(), 2, "`==` and `!=` over one pair of fields are not two comparisons:\n{}", body(&rust, "run"));
}

#[test]
fn an_operand_the_loop_writes_or_binds_is_compared_at_every_site() {
    // `b` is a `var` the loop rewrites.
    let written = format!("{TWO_SITES}\n    b = {{ name: \"x\", xs: [i] }}");
    let rust = certified(&compare_loop("var b = b0", &written));
    assert_eq!(name_compares(&rust), 2, "a rewritten operand is remembered:\n{}", body(&rust, "run"));
    // `b` is bound by the loop itself.
    let bound = format!("    let b: P = {{ name: \"x\", xs: [i] }}\n{TWO_SITES}");
    let rust = certified(&compare_loop("", &bound));
    assert_eq!(name_compares(&rust), 2, "an operand bound per round is remembered:\n{}", body(&rust, "run"));
}

#[test]
fn a_comparison_in_a_callback_or_over_scalars_stays_where_it_is() {
    // Inside a callback the memo would be a captured variable the callback
    // writes: each callback compares for itself (both are inlined into the
    // loop here, where a remembered comparison would be one).
    let lambda = "    let ys = list.map([1, 2], (y) => { if a == b then y else 0 - y })\n    let zs = list.map([3], (y) => { if a == b then y else 0 - y })\n    acc = acc + list.sum(ys) + list.sum(zs)";
    let rust = certified(&compare_loop("let b = b0", lambda));
    assert_eq!(name_compares(&rust), 2, "two callbacks do not each compare:\n{}", body(&rust, "run"));
    // A scalar comparison binds no memo: the loop has the two variables it
    // declares (the counter's step and nothing else) ahead of it.
    let scalars = "    if n == i then { acc = acc + 1 } else { () }";
    let with = certified(&compare_loop("let b = b0", scalars));
    let without = certified(&compare_loop("let b = b0", "    if n > i then { acc = acc + 1 } else { () }"));
    let lets = |r: &str| body(r, "run").matches("let mut ").count();
    assert_eq!(lets(&with), lets(&without), "a scalar equality bound a memo:\n{}", body(&with, "run"));
}

/// `var m: Map[K, Int]` updated once per round by `update`, the keys in `ks`.
fn update_loop(key_ty: &str, ks: &str, update: &str) -> String {
    format!(
        "type E = {{ name: String, n: Int }}\n\
         fn main() -> Unit = {{\n  let ks: List[{key_ty}] = {ks}\n  let n = list.len(ks)\n  var m: Map[{}, Int] = [:]\n  for i in 0..<6 {{\n    {update}\n  }}\n  println(int.to_string(map.len(m) + n))\n}}\n",
        if key_ty == "E" { "String" } else { key_ty }
    )
}

#[test]
fn a_borrowed_string_place_is_handed_to_an_in_place_update_by_reference() {
    let strings = "[\"a\", \"b\", \"a\"]";
    for (update, twin) in [
        ("let w = ks[i % n]\n    m[w] = (map.get(m, w) ?? 0) + 1", "almide_rt_map_upsert_mut_by_ref(&mut "),
        ("m = map.upsert(m, ks[i % n], 1, (v) => v + 1)", "almide_rt_map_upsert_mut_by_ref(&mut "),
        ("m = map.set(m, ks[i % n], i)", "almide_rt_map_set_mut_by_ref(&mut "),
    ] {
        let rust = certified(&update_loop("String", strings, update));
        let main = body(&rust, "main");
        assert_eq!(main.matches(twin).count(), 1, "`{update}` does not hand its key by reference:\n{main}");
        assert!(!main.contains(".clone()"), "`{update}` still shares the key's handle per round:\n{main}");
    }
    // A record's string field is such a place too.
    let records = "[{ name: \"a\", n: 1 }, { name: \"b\", n: 2 }]";
    let rust = certified(&update_loop("E", records, "m = map.upsert(m, ks[i % n].name, 1, (v) => v + 1)"));
    assert_eq!(body(&rust, "main").matches("almide_rt_map_upsert_mut_by_ref(&mut ").count(), 1, "a field key is not handed by reference:\n{}", body(&rust, "main"));
}

#[test]
fn a_key_that_is_no_borrowed_string_place_keeps_the_by_value_twin() {
    // An Int key is its value, and a key built for the update is the
    // update's own to hand over.
    for (key_ty, ks, update) in [
        ("Int", "[1, 2, 1]", "m = map.upsert(m, ks[i % n], 1, (v) => v + 1)"),
        ("String", "[\"a\", \"b\"]", "m = map.upsert(m, ks[i % n] + \"!\", 1, (v) => v + 1)"),
        ("String", "[\"a\", \"b\"]", "m = map.upsert(m, \"k\", 1, (v) => v + 1)"),
    ] {
        let rust = certified(&update_loop(key_ty, ks, update));
        let main = body(&rust, "main");
        assert_eq!(main.matches("almide_rt_map_upsert_mut(&mut ").count(), 1, "`{update}` over {key_ty} keys left the in-place twin:\n{main}");
        assert!(!main.contains("_by_ref("), "`{update}` over {key_ty} keys hands a key by reference:\n{main}");
    }
}

/// `fn walk(ws: List[String]) -> List[String]` copying an element read at `j`.
fn copy_loop(between: &str) -> String {
    format!(
        "fn walk(ws: List[String]) -> List[String] = {{\n  var out: List[String] = []\n  var j = 0\n  for _i in 0..<2 {{\n    let w = ws[j]\n{between}    let c = w\n    out = out + [c]\n  }}\n  out\n}}\n\
         fn main() -> Unit = println(list.join(walk([\"a\", \"b\", \"c\"]), \",\"))\n"
    )
}

#[test]
fn a_copy_reads_its_place_again_only_while_the_index_stands() {
    // The index stands: the copy shares the element's handle.
    let rust = certified(&copy_loop(""));
    assert!(body(&rust, "walk").contains("].clone()"), "a copy of a borrowed element does not share its handle:\n{}", body(&rust, "walk"));
    // The index is rewritten between the read and the copy: the place names
    // another element by then, so the copy is made from the reference.
    let rust = certified(&copy_loop("    j = j + 1\n"));
    let walk = body(&rust, "walk");
    assert!(!walk.contains("].clone()"), "a copy reads its place after the index moved:\n{walk}");
    assert!(walk.contains("AlmideString::from("), "the copy is not made from the read:\n{walk}");
}
