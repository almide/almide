//! #2758 (#1696 step 4) — MATCH GUARDS in the structural witness. A guard
//! runs between two arms' tests (patterns.rs `lower_arm_chain`): only on a
//! path through its arm's pattern test, before that arm's body, and — when it
//! is false — before every later arm. It is recorded on its own arm's path.
//! That is exact for the checker because the guard's value is a scalar Bool:
//! every credit it takes inside is settled inside it (a temporary born and
//! released, a share moved into a callee), so a path that falls through
//! leaves the guard in the state it entered with, and the guard's events are
//! judged against that same state on its own arm. A guard that binds a local
//! (a block, a lambda, a nested match) still declines: the local outlives it.
//! No new event letter.

const PROGRAM: &str = r#"type R = { k: String, n: Int }

fn heavier(x: R, r: R) -> Bool = x.n > r.n

fn eq_guard(o: R?, r: R) -> String = match o {
  some(x) if x == r => "same",
  some(x) if x != r => "diff:${x.k}",
  _ => "none",
}

fn call_guard(o: R?, r: R) -> String = match o {
  some(x) if heavier(x, r) => "heavier:${x.n}",
  some(_) => "lighter",
  none => "none",
}

fn len_guard(v: Result[Int, String]) -> String = match v {
  err(e) if string.len(e) > 3 => "long-err",
  err(_) => "short-err",
  ok(n) => "ok:${n}",
}

fn or_guard(v: Result[Int, String], x: Int?) -> Int = match v {
  err(_) if (x ?? 0) % 2 == 0 => 100,
  err(_) => 1,
  ok(n) => n,
}

effect fn seen(k: Int) -> Bool = {
  println("seen ${k}")
  ok(k > 1)
}

effect fn bang_guard(n: Int) -> Int = match n {
  1 if seen(n)! => 10,
  _ if seen(n + 1)! => 20,
  _ => 0,
}

fn binds_guard(o: Int?) -> Int = match o {
  some(x) if { let y = x + 1
  y > 2 } => x,
  _ => 0,
}

effect fn main() -> Unit = {
  let r = R { k: "a", n: 2 }
  println("${eq_guard(some(R { k: "a", n: 2 }), r)} ${eq_guard(some(R { k: "b", n: 1 }), r)}")
  println("${call_guard(some(R { k: "c", n: 5 }), r)} ${len_guard(err("long"))} ${or_guard(err("e"), some(2))}")
  println("${bang_guard(1)!} ${bang_guard(3)!} ${binds_guard(some(4))}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("match_guard.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn guarded_arms_witness_on_their_own_path_and_a_binding_guard_declines() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in ["eq_guard", "call_guard", "len_guard", "or_guard"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines match-guard: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
    }
    // The guards' operands are views and borrowed arguments (`heavier` only
    // reads its params, `==` reads both sides): no event of their own. Each
    // arm's String is a fresh value moving out (`im`).
    assert_eq!(w["eq_guard"], "\n\n\n{|im}\n\n{|im}\n{|im}\nim\n");
    assert_eq!(w["call_guard"], "\n\n\n{|im}\n{|im}\n{|im}\nim\n");
    assert_eq!(w["len_guard"], "\n\n{|im}\n{|im}\n{|im}\nim\n");
    assert_eq!(w["or_guard"], "\n\n");
    // A guard that binds a local declines: an explicit `let` block, and
    // `f(x)!`, whose carrier arg_temps parks in a `let` before the guard.
    assert_eq!(w["binds_guard"], "!decline:match-guard:binds\n");
    assert_eq!(w["bang_guard"], "!decline:match-guard:binds\n");
}
