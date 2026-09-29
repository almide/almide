//! #2758 (#1696 step 4) — EFFECT frames in the structural witness. An
//! effect fn's body lowers at its raw ok type and func.rs wraps the value in
//! the ok carrier: the raw value is recorded exactly as a pure tail is (an
//! owned value's credit moves into the carrier's slot, `im`; a bound Var is
//! shared and moves, `am`), and the carrier itself is born and moves out of
//! the frame (`im`). A Unit body answers ok(()) with a carrier and no
//! payload.
//!
//! The early `!` (witness_unwrap.rs) is a branch whose arm propagates: the
//! parked carrier is shared, released with the frame and leaves (`iadm` on
//! that path), the payload read out of it is a view a consumer takes a
//! credit of (`ad`). A frame with several `!` sites folds each exit into a
//! v5 branch-return item (`i{admx|}d`). A raw position `ok` / `err` still
//! declines.

const PROGRAM: &str = r#"effect fn echo(s: String) -> String = s

effect fn fresh(n: Int) -> List[Int] = [n, n]

effect fn noop(n: Int) -> Unit = {
  let xs = [n]
  let _ = list.len(xs)
}

effect fn relay(s: String) -> String = echo(s)!

effect fn twice(s: String) -> String = {
  let a = echo(s)!
  let b = echo(a)!
  a + b
}

effect fn step(n: Int) -> Int = {
  let xs = fresh(n)!
  list.len(xs)
}

effect fn opt(o: Int?) -> Int = {
  let v = o!
  v + 1
}

effect fn checked(n: Int) -> Int = if n < 0 then err("neg") else ok(n)

effect fn main() -> Unit = {
  let a = echo("a")!
  let b = fresh(2)!
  noop(1)!
  let c = relay("c")!
  let d = checked(3)!
  let e = twice("e")!
  let f = step(1)!
  let g = opt(some(1))!
  println(a)
  println(c)
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("effect.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn effect_frames_witness_the_carrier_and_unhooked_exits_decline() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The borrowed param is shared into the slot; the carrier moves out.
        ("echo", "am\nim\n"),
        // The fresh spine's credit moves into the slot; the carrier moves out.
        ("fresh", "im\nim\n"),
        // A Unit body: the local is born and released, the carrier moves out.
        ("noop", "id\nim\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed; got {:?}", w.get(name)));
        assert_eq!(got, cert, "{name}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // A tail `f(x)!` whose callee returns this frame's Result is a
    // `return_call` (C-069): the borrowed param lends, nothing is held.
    assert_eq!(w.get("relay").map(String::as_str), Some("\n\n"));
    let bang = [
        // One site: the parked carrier propagates (`iadm`) or is released
        // (`id`); the view `xs` exists on the ok path only.
        ("step", "{iadm|id}\n{|ad}\n{|im}\n"),
        // Two sites: the second carrier's three paths fold (`{admx|}`); the
        // views `a` and `b` are lent to borrowed params, each bound once.
        ("twice", "\n{iadm|id}\n{|ad}\ni{admx|}d\n{|ad}\n{|im}\n{|im}\n"),
        // `!` on none: a fresh `err("none")` leaves on that arm.
        ("opt", "\n{|im}\n{|im}\n"),
    ];
    for (name, cert) in bang {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert_eq!(got, cert, "{name}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // The folded item is checked from the count at its site to exactly 0:
    // an exit that skipped the carrier's release is a leak the checker sees.
    assert!(!accepted("i{amx|}d\n"));
    assert_eq!(w.get("checked").map(String::as_str), Some("!decline:effect:carrier\n"));
}
