//! #2755 / #2758 (#1696 step 4) — INLINED CALLBACKS in the structural
//! witness. `list.map` / `filter` / `find` / `any` / `all` / `count` / `fold`
//! lower a literal lambda's body in the calling frame, one loop activation per
//! element: each param is a view of its element, the body's sites are the
//! ordinary hooks, and the arm's use of the body's value is hooked (`map`'s
//! store into the result spine, `find`'s share into the some-cell, a heap
//! `fold`'s loop-carried accumulator) or carries no RC site (a Bool
//! predicate, a scalar `fold`). Every other inlining shape declines by arm
//! name, never certifies.

const PROGRAM: &str = r#"import fan

effect fn up(s: String) -> Result[String, String] = if s == "" then err("empty") else ok(s + "!")

effect fn fan_bang(xs: List[String]) -> List[String] = fan.map(xs, (s) => up(s))!

effect fn fan_first(xs: List[String]) -> String = fan.any(xs, (s) => up(s))!

fn bang(xs: List[String]) -> List[String] = list.map(xs, (s) => s + "!")

fn same(xs: List[String]) -> List[String] = list.map(xs, (s) => s)

fn kept(xs: List[String]) -> List[String] = list.filter(xs, (s) => string.len(s) > 1)

fn total(xs: List[String]) -> Int = list.fold(xs, 0, (acc, s) => acc + string.len(s))

fn joined(xs: List[String]) -> String = list.fold(xs, "", (acc, s) => acc + s)

fn fused(xs: List[String]) -> Int = list.fold(list.map(xs, (s) => string.len(s)), 0, (a, n) => a + n)

fn first(xs: List[String]) -> String? = list.find(xs, (s) => string.len(s) > 1)

fn any_long(xs: List[String]) -> Bool = list.any(xs, (s) => string.len(s) > 1)

fn every(xs: List[String]) -> Bool = list.all(xs, (s) => string.len(s) > 1)

fn many(xs: List[String]) -> Int = list.count(xs, (s) => string.len(s) > 1)

effect fn main() -> Unit = {
  let xs = ["a", "bc"]
  println("${bang(xs)} ${same(xs)} ${kept(xs)} ${total(xs)} ${joined(xs)} ${fused(xs)} ${first(xs) ?? ""}")
  println("${any_long(xs)} ${every(xs)} ${many(xs)}")
  println("${fan_bang(xs)!} ${fan_first(xs)!}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("callbacks.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn inlined_callbacks_witness_per_element_and_other_shapes_decline() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in ["bang", "same", "kept", "total", "joined", "first", "any_long", "every", "many", "fan_bang", "fan_first"] {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert!(!got.starts_with('!'), "{name}: {got:?}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // The fresh concat moves into the spine on each activation; the spine
    // moves out.
    assert!(w["bang"].ends_with("im\nim\n"), "{:?}", w["bang"]);
    // A pass-through body hands back the view: the spine takes its share
    // and moves it in (`am` on the element's view line).
    assert!(w["same"].contains("am\n"), "{:?}", w["same"]);
    // A heap accumulator is a loop-carried owner: born with the seed, the
    // step's fresh result replaces it (the old value's `d` closes each
    // activation), and the final value moves out.
    assert_eq!(w["joined"], "id\nim\n\nid\n\n\nim\nim\n");
    // `find`'s hit is a branch: the element's view shares into the fresh
    // some-cell (`am`), the other arm carries nothing.
    assert!(w["first"].contains("{|am}\n"), "{:?}", w["first"]);
    // fan's accumulator: each element's carrier leaves or is released
    // (`{id|im}`), and map's ok payload moves into the accumulator.
    assert!(w["fan_bang"].contains("{id|im}\n{|im}\n") || w["fan_bang"].contains("{id|im}\n\n{|im}\n"), "{:?}", w["fan_bang"]);
    assert!(w["fan_first"].contains("{id|im}\n"), "{:?}", w["fan_first"]);
    // A fold over a list call takes the fused lowering and declines by name.
    let declined = |n: &str| w.get(n).map(String::as_str).unwrap_or("<none>").to_string();
    assert_eq!(declined("fused"), "!decline:call-arg:Lambda:list.fold:fused\n");
}
