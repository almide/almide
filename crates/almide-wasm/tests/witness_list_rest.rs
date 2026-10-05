//! #2758 (#1696 step 4) — the NAMED LIST REST `[h, ..t]` in the structural
//! witness. The pattern builds `t` as a fresh block (`i`) that its UNGUARDED
//! arm releases once the body has run (`d`, arm_rests.rs `release_arm_rests`).
//! A guarded arm (a false guard falls through with the rest built) declines.
//! #3377: a path that leaves the arm early — a `!`, a guard return, a
//! recursive tail call, a `break` / `continue` — releases the rest on its own
//! edge, so it is certified with exactly one `d` per path.

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

fn guard_exit(xs: List[String]) -> Int = match xs {
  [h, ..t] => {
    guard h == "k" else list.len(t)
    0
  },
  _ => 1,
}

fn walk(xs: List[String], acc: Int) -> Int = match xs {
  [h, ..t] => walk(t, acc + string.len(h)),
  [] => acc,
}

fn jumps(xs: List[String]) -> Int = {
  var acc = 0
  for i in 0..<3 {
    match xs {
      [h, ..t] => {
        if i == 1 then continue else ()
        if h == "q" then break else ()
        acc = acc + list.len(t)
      },
      _ => (),
    }
  }
  acc
}

effect fn main() -> Unit = {
  println("${total([1, 2, 3])} ${list.len(tail_of(["a", "b"]))} ${guarded(["k", "j"])} ${leaves(["x", "y"])!}")
  println("${guard_exit(["a"])} ${walk(["a", "b"], 0)} ${jumps(["q", "r"])}")
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
        // #3377: the `!` exit's path releases the rest on its edge (`d`
        // before the exit) as the fall-through does after the arm: one path,
        // both settled. The arm value (`check(t)!`'s Int) owns nothing.
        ("leaves", "\n{|ibamd}\n{|im}\n"),
        // The guard return releases the rest before it leaves (`ibdx`); the
        // fall-through releases it after the body (`id`).
        ("guard_exit", "\n\n{ibdx|}{|id}\n"),
        // The recursive tail call shares the rest into the callee's owned
        // param (`am`) and the loop-back releases the arm's own credit (`d`).
        ("walk", "ibd\n\n{|ibamd}\n"),
        // `continue` and `break` release the rest on the jump edge (`idx`).
        ("jumps", "\n\n\n{idx|}{|ibd}\n"),
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
}
