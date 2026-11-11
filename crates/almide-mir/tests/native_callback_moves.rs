//! #4661: shapes of the certified native render that cost a copy or a move
//! the program never asked for, each with the shape that must keep it.
//!
//! - A parameter handed by value to a combinator only its runtime body
//!   serves (`map.upsert` over a map of records: no body is linked for it)
//!   is taken over, so a fold's accumulator moves through its callback
//!   (`native_move_params`, `served_by_value`); the `native-calls` witness
//!   states the move on both sides.
//! - The halves of a split of a borrowed string parameter are slices of it
//!   (`render_native_slices`).
//! - A `map.set` / `map.upsert` whose result goes straight back into its
//!   slot edits the map in place (`render_native_inplace`).
//! - A slot rebound to a freshly built buffer is refilled, and a runtime
//!   list whose elements convert to themselves is not copied.
//! - A branch value that is the function's result returns where it is made
//!   (`render_native_tail_return`), and a small straight-line function is
//!   inlined (`render_native_inline`).

fn certified(src: &str) -> (String, Vec<(String, String, String)>) {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source_certified(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

fn body<'a>(rust: &'a str, f: &str) -> &'a str {
    let at = rust.find(&format!("fn almd_{f}(")).unwrap_or_else(|| panic!("no `{f}`:\n{rust}"));
    let b = &rust[at..];
    &b[..b.find("\n}\n").unwrap_or(b.len())]
}

const STATS: &str = "type Stats = { lo: Int, n: Int }\n";

/// A fold whose step hands its accumulator to `map.upsert` in one arm and
/// returns it as it is in the other.
fn fold_step(step_body: &str) -> String {
    format!(
        "{STATS}fn step(acc: Map[String, Stats], line: String) -> Map[String, Stats] = {step_body}\n\
         fn main() -> Unit = {{\n  let m = [\"a\", \"bb\", \"a\"] |> list.fold(map.new(), (acc, line) => step(acc, line))\n  println(int.to_string(map.len(m)))\n}}\n"
    )
}

const UPSERT_OR_KEEP: &str = "if string.len(line) > 0 then map.upsert(acc, line, Stats { lo: 1, n: 1 }, (s) => Stats { lo: s.lo, n: s.n + 1 }) else acc";

#[test]
fn an_accumulator_handed_to_a_runtime_only_combinator_moves_through_the_callback() {
    let (rust, certs) = certified(&fold_step(UPSERT_OR_KEEP));
    // `step` takes the map, and so does the lambda that hands it on.
    assert!(body(&rust, "step").starts_with("fn almd_step(mut v0: "), "step does not take its map:\n{}", body(&rust, "step"));
    assert!(!body(&rust, "step").contains(".clone()"), "step copies the map:\n{}", body(&rust, "step"));
    let lambda = body(&rust, "__lambda_main_0");
    assert!(lambda.contains(", mut v1: "), "the callback does not take the accumulator:\n{lambda}");
    assert!(lambda.contains("almd_step(v1, "), "the callback lends the accumulator:\n{lambda}");
    // The witness: `step`'s block declares the param a move, and a site hands it over.
    let (_, _, calls) = certs.iter().find(|(p, ..)| p == "native-calls").expect("a native-calls witness");
    let moving = |b: &&str| b.split('|').next().is_some_and(|m| m.split(' ').any(|x| x == "1"));
    assert!(calls.split('#').filter(moving).count() >= 2, "step and its callback are not both move blocks: {calls}");
    assert!(calls.contains('M'), "no site hands the map over: {calls}");
}

#[test]
fn an_accumulator_read_after_it_is_handed_on_is_copied_once_for_the_call() {
    // The map is read again after the upsert: the call is not its last use,
    // so that one call takes a copy and the map itself is still the body's.
    let (rust, _) = certified(&fold_step(
        "{\n  let grown = map.upsert(acc, line, Stats { lo: 1, n: 1 }, (s) => Stats { lo: s.lo, n: s.n + 1 })\n  if map.len(acc) > 5 then acc else grown\n}",
    ));
    let f = body(&rust, "step");
    assert!(f.contains("almide_rt_map_upsert_static(v0.clone(), "), "the call takes the map it is read after:\n{f}");
    assert_eq!(f.matches(".clone()").count(), 1, "more than the one copy:\n{f}");
    assert!(f.contains("almide_rt_map_len(&v0)"), "{f}");
}

#[test]
fn a_parameter_lent_to_a_linked_combinator_keeps_borrowing() {
    // `list.map` over a list of Ints has a linked body, which lends: taken
    // over, a caller that keeps its list would copy it for nothing.
    let (rust, _) = certified(
        "fn bump(xs: List[Int]) -> List[Int] = list.map(xs, (x) => x + 1)\n\
         fn main() -> Unit = {\n  let xs = [1, 2, 3]\n  println(int.to_string(list.len(bump(xs)) + list.len(xs)))\n}\n",
    );
    assert!(body(&rust, "bump").starts_with("fn almd_bump(v0: &[i64])"), "bump takes a list it only lends:\n{}", body(&rust, "bump"));
}

const SPLIT: &str = "fn value(line: String) -> Int = match string.split_once(line, \";\") {\n  some((name, text)) => string.len(name) + string.len(text),\n  none => 0,\n}\n\
     fn main() -> Unit = println(int.to_string(value(\"abc;de\")))\n";

#[test]
fn the_halves_of_a_split_of_a_borrowed_parameter_are_slices() {
    let (rust, _) = certified(SPLIT);
    let f = body(&rust, "value");
    assert!(f.contains("almide_rt_string_split_once_ref(v0, "), "the split builds owned halves:\n{f}");
    assert!(f.contains(": Option<(&str, &str)> = "), "{f}");
    assert!(!f.contains("AlmideString::from("), "a half is copied:\n{f}");
}

#[test]
fn a_half_that_is_stored_is_the_one_owned_copy() {
    // `name` is returned: the slice is copied once, where it is consumed.
    let (rust, _) = certified(
        "fn name_of(line: String) -> String = match string.split_once(line, \";\") {\n  some((name, _)) => name,\n  none => \"\",\n}\n\
         fn main() -> Unit = println(name_of(\"abc;de\"))\n",
    );
    let f = body(&rust, "name_of");
    assert!(f.contains("almide_rt_string_split_once_ref(v0, "), "{f}");
    assert_eq!(f.matches("AlmideString::from(").count(), 1, "the returned half is not copied exactly once:\n{f}");
}

#[test]
fn a_split_of_a_string_the_function_owns_keeps_its_owned_halves() {
    // The subject is a local the body rebinds: a slice of it would not
    // outlive the next assignment.
    let (rust, _) = certified(
        "fn count(s0: String) -> Int = {\n  var s = s0\n  var n = 0\n  while n < 9 {\n    match string.split_once(s, \",\") {\n      some((_, rest)) => {\n        s = rest\n        n = n + 1\n      },\n      none => {\n        n = n + 10\n      },\n    }\n  }\n  n\n}\n\
         fn main() -> Unit = println(int.to_string(count(\"a,b,c\")))\n",
    );
    assert!(!body(&rust, "count").contains("_ref("), "a slice of a rebound string:\n{}", body(&rust, "count"));
}

/// The rendered `main` whose loop body is `update` over `var m`.
fn loop_with(map_ty: &str, update: &str) -> String {
    let (rust, _) = certified(&format!(
        "fn main() -> Unit = {{\n  var m: {map_ty} = map.new()\n  var other: {map_ty} = map.new()\n  for i in list.range(0, 9) {{\n    {update}\n  }}\n  println(int.to_string(map.len(m) + map.len(other)))\n}}\n"
    ));
    let at = rust.find("fn main() {").expect("main");
    rust[at..].to_string()
}

#[test]
fn a_map_update_rebound_to_its_own_slot_is_made_in_place() {
    for (map_ty, update, twin) in [
        ("Map[Int, Int]", "m[i % 3] = i", "almide_rt_map_set_mut(&mut "),
        ("Map[Int, Int]", "m = map.set(m, i % 3, i)", "almide_rt_map_set_mut(&mut "),
        ("Map[Int, Int]", "m[i % 3] = (map.get(m, i % 3) ?? 0) + 1", "almide_rt_map_set_mut(&mut "),
        ("Map[Int, Int]", "m = map.upsert(m, i % 3, 1, (v) => v + 1)", "almide_rt_map_upsert_mut(&mut "),
    ] {
        let main = loop_with(map_ty, update);
        assert_eq!(main.matches(twin).count(), 1, "`{update}` is not in place:\n{main}");
        assert!(!main.contains("std::mem::take(&mut "), "`{update}` still moves the map out:\n{main}");
    }
    let main = loop_with("Map[Int, Int]", "let k = i % 3\n    m[k] = (map.get(m, k) ?? 0) + 1");
    assert_eq!(main.matches("almide_rt_map_upsert_mut(&mut ").count(), 1, "the fused update is not in place:\n{main}");
}

#[test]
fn a_map_update_bound_elsewhere_keeps_the_consuming_call() {
    // The result goes to another slot: `m` is copied or moved as before.
    let main = loop_with("Map[Int, Int]", "other = map.set(m, i, i)");
    assert_eq!(main.matches("_mut(&mut ").count(), 0, "a result bound elsewhere was edited in place:\n{main}");
    assert_eq!(main.matches("almide_rt_map_set(").count(), 1, "{main}");
}

const CHAIN: &str = "type Pair = { a: String, b: String }\n\
     effect fn part(s: String) -> String = if string.len(s) > 0 then ok(s) else err(\"empty\")\n\
     effect fn pair(x: String, y: String) -> Pair = {\n  let a = part(x)!\n  let b = part(y)!\n  ok(Pair { a: a, b: b })\n}\n\
     effect fn main() -> Unit = {\n  let p = pair(\"l\", \"r\")!\n  println(p.a + p.b)\n}\n";

#[test]
fn a_propagation_chain_returns_from_each_arm() {
    let (rust, _) = certified(CHAIN);
    let f = body(&rust, "pair");
    // Each failing arm and the final value return on the spot: no join is
    // assigned outward level by level.
    assert!(f.matches("return ").count() >= 3, "the chain joins its result outward:\n{f}");
    assert!(!rust.contains("__TAIL_"), "a tail marker survived:\n{rust}");
}

#[test]
fn a_scalar_result_and_a_value_used_after_its_branch_keep_their_joins() {
    // An Int join is not rewritten (nothing to save), and a join inside a
    // loop is read by the rest of the round: it is not the function's result.
    let (rust, _) = certified(
        "fn pick(c: Bool, s: String) -> String = {\n  var out = \"\"\n  for i in 0..<2 {\n    let t = if c then s + \"!\" else s + int.to_string(i)\n    out = out + t\n  }\n  out\n}\n\
         fn sign(n: Int) -> Int = if n < 0 then 0 - 1 else if n > 0 then 1 else 0\n\
         fn main() -> Unit = println(pick(true, \"a\") + int.to_string(sign(3)))\n",
    );
    assert!(!body(&rust, "pick").contains("return "), "a join inside a loop returned:\n{}", body(&rust, "pick"));
    assert!(!body(&rust, "sign").contains("return "), "a scalar join returned:\n{}", body(&rust, "sign"));
}

#[test]
fn a_small_helper_is_inlined_and_a_loop_or_a_recursion_is_not() {
    let (rust, _) = certified(
        "fn twice(x: Int) -> Int = x * 2\n\
         fn down(n: Int) -> Int = if n <= 0 then 0 else down(n - 1)\n\
         fn total(n: Int) -> Int = {\n  var t = 0\n  for i in 0..<n {\n    t = t + i\n  }\n  t\n}\n\
         fn main() -> Unit = println(int.to_string(twice(3) + down(4) + total(5)))\n",
    );
    assert!(rust.contains("#[inline(always)]\nfn almd_twice("), "{rust}");
    assert!(!rust.contains("#[inline(always)]\nfn almd_down("), "a recursive function is forced inline");
    assert!(!rust.contains("#[inline(always)]\nfn almd_total("), "a loop is forced inline");
}

#[test]
fn a_buffer_cleared_in_its_slot_is_refilled() {
    let (rust, _) = certified(
        "import bytes\n\nfn main() -> Unit = {\n  var buf = bytes.zeros(8)\n  var n = 0\n  for i in 0..<9 {\n    bytes.clear(buf)\n    bytes.push(buf, i)\n    n = n + bytes.len(buf)\n  }\n  println(int.to_string(n))\n}\n",
    );
    let main = &rust[rust.find("fn main() {").expect("main")..];
    assert_eq!(main.matches("almide_rt_bytes_zeros_into(&mut ").count(), 1, "the clear builds a fresh buffer:\n{main}");
    assert_eq!(main.matches("almide_rt_bytes_zeros(").count(), 1, "more than the first buffer is built:\n{main}");
}

#[test]
fn a_fresh_buffer_bound_to_a_new_name_is_built() {
    // Nothing is rebound: there is no slot to refill.
    let (rust, _) = certified(
        "import bytes\n\nfn main() -> Unit = {\n  let a = bytes.zeros(8)\n  let b = bytes.zeros(0)\n  println(int.to_string(bytes.len(a) + bytes.len(b)))\n}\n",
    );
    assert!(!rust[rust.find("fn main() {").expect("main")..].contains("_into(&mut "), "a new binding was refilled");
}

#[test]
fn a_flat_map_result_of_scalars_is_not_copied() {
    let (rust, _) = certified(
        "fn main() -> Unit = {\n  let xs = [1, 2, 3]\n  let data = xs |> list.flat_map((i) => [float.from(i) * 0.5, float.from(i)])\n  println(int.to_string(list.len(data)))\n}\n",
    );
    let main = &rust[rust.find("fn main() {").expect("main")..];
    assert!(!main.contains("let mut __copy"), "the result list is copied element by element:\n{main}");
}

/// The rendered `main` of a program that binds `data` to `chain`.
fn flat_mapped(chain: &str) -> String {
    let (rust, _) = certified(&format!(
        "fn pair(i: Int) -> List[Int] = [i, i + 1]\n\nfn main() -> Unit = {{\n  let n = 9\n  let data = {chain}\n  println(int.to_string(list.len(data)))\n}}\n"
    ));
    rust[rust.find("fn main() {").expect("main")..].to_string()
}

#[test]
fn a_flat_map_over_a_counted_range_appends_in_a_counted_loop() {
    for chain in ["list.range(0, n) |> list.flat_map((i) => [i, i * 2])", "list.range(2, n * 2) |> list.flat_map((i) => {\n    let t = i + n\n    [float.from(t), 0.5]\n  })"] {
        let main = flat_mapped(chain);
        assert!(!main.contains("almide_rt_list_flat_map"), "`{chain}` still walks an index list:\n{main}");
        // (A range only the run can size keeps one guarded build: the abort
        // of a range too long to be a list, `native_range_loops`.)
        assert!(main.matches("almide_rt_list_range(").count() <= 1, "`{chain}` builds its index list:\n{main}");
        assert!(main.contains(".extend_from_slice("), "`{chain}` does not append its literal:\n{main}");
    }
}

#[test]
fn a_flat_map_whose_callback_yields_no_literal_or_whose_source_is_a_list_keeps_the_combinator() {
    for (chain, why) in [
        ("list.range(0, n) |> list.flat_map((i) => pair(i))", "the callback's result is a call"),
        ("[1, 2, 3] |> list.flat_map((i) => [i, i])", "the source is a list"),
    ] {
        let main = flat_mapped(chain);
        assert!(main.contains("flat_map"), "`{chain}` was counted ({why}):\n{main}");
    }
}
