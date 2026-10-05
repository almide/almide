//! #2758 (#1696 step 4) — the NAMED LIST REST `[h, ..t]` in the structural
//! witness. The pattern builds `t` as a fresh block (`i`) that its UNGUARDED
//! arm releases once the body has run (`d`, patterns.rs `release_arm_rests`).
//! A guarded arm (a false guard falls through with the rest built) and a
//! path that leaves the arm before the release decline.

const PROGRAM: &str = r#"fn total(xs: List[Int]) -> Int = match xs {
  [] => 0,
  [h, ..t] => h + total(t),
}

fn tail_of(xs: List[String]) -> List[String] = match xs {
  [..t] => t
}

fn guarded(xs: List[String]) -> Int = match xs {
  [h, ..t] if h == "k" => list.len(t),
  _ => 0,
}

fn check(xs: List[String]) -> Result[Int, String] = if list.len(xs) > 9 then err("long") else ok(list.len(xs))

effect fn leaves(xs: List[String]) -> Int = match xs {
  [_, ..t] => check(t)!,
  _ => 0,
}

effect fn main() -> Unit = {
  println("${total([1, 2, 3])} ${list.len(tail_of(["a", "b"]))} ${guarded(["k", "j"])} ${leaves(["x", "y"])!}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("list_rest.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn named_list_rests_are_born_and_released_by_their_arm() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The rest exists on the second arm only: built (`i`), lent to the
        // borrowed param of the recursive call (`b`, the read), released.
        ("total", "\n{|ibd}\n"),
        // The arm's value shares the rest into the join (`am`) before the arm
        // releases its own credit (`d`); the join moves out (`im`).
        ("tail_of", "\nibamd\nim\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert_eq!(got, cert, "{name}");
        assert!(
            almide_verify::check(almide_verify::Property::Ownership, got.as_bytes()),
            "{name}: the portable checker must accept {got:?}"
        );
    }
    // A false guard falls through with the rest built: declined by the gate.
    assert_eq!(w.get("guarded").map(String::as_str), Some("!decline:pattern:list-rest\n"));
    // The `!` exit leaves the arm before its release: declined at emission.
    assert_eq!(w.get("leaves").map(String::as_str), Some("!decline:pattern:list-rest:early-exit\n"));
}
