//! #2758 (#1696 step 4) — the carrier constructors of an EFFECT body, a
//! call through a record field, and a scalar cell capture in the structural
//! witness. A bare `err(e)` where the raw type is expected RAISES: the err
//! block is built, the exit's releases are recorded like a `!` propagation's,
//! and the block leaves. `ok(v)` there is transparent, which agrees with every
//! consumer only for a scalar or certainly-fresh `v`; any other declines. A
//! record field's Fn value is a view lent to the lifted body. A cell capture
//! whose occupant is a scalar has no RC site in the lambda. A callback that
//! raises instantiates the self-hosted `list.__fallible_map`: an ordinary
//! call whose literal lambda is a closure value built and released here.

const PROGRAM: &str = r#"effect fn raise(x: Int) -> Int = if x == 0 then err("zero") else ok(x)

effect fn keep(s: String, x: Int) -> String = if x == 0 then err("zero") else ok(s)

type Op = { run: (Int) -> Int }

fn apply_op(o: Op, x: Int) -> Int = o.run(x)

effect fn counter() -> Int = {
  var n = 0
  let bump = (k: Int) => {
    n = n + k
    n
  }
  bump(2) + bump(3)
}

effect fn raised_all(xs: List[Int]) -> List[Int] = list.map(xs, (n) => raise(n)!)!

effect fn main() -> Unit = {
  println("${raise(2)!} ${keep("s", 1)!} ${apply_op(Op { run: (n) => n + 1 }, 4)} ${counter()!}")
  println("${list.len(raised_all([1])!)}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("effect_raise.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn raised_errs_field_callees_and_scalar_cells_witness_exactly() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let get = |n: &str| w.get(n).map(String::as_str).unwrap_or("<none>").to_string();
    // The literal payload moves into the err block, the block leaves on the
    // raising arm, the ok carrier on the other.
    assert_eq!(get("raise"), "{|im}\n{|im}\n{|im}\n");
    assert!(accepted(&get("raise")));
    // `ok(s)` over a borrowed param: the consumer would read it as fresh.
    assert_eq!(get("keep"), "!decline:effect:carrier:ok-borrowed\n");
    // The record param and the field's Fn value are views.
    assert!(accepted(&get("apply_op")) && !get("apply_op").contains(|c| "iadm".contains(c)), "{:?}", get("apply_op"));
    // The lambda capturing the Int cell `n` certifies.
    let cell_lambda = w.iter().find(|(k, c)| k.starts_with("<lambda#") && !c.starts_with('!'));
    assert!(cell_lambda.is_some(), "a scalar-cell lambda must certify: {w:?}");
    // The fallible HOF's closure env is born, lent to the call (read, `b`,
    // #3259) and released.
    assert!(get("raised_all").starts_with("ibd\n"), "{:?}", get("raised_all"));
    assert!(accepted(&get("raised_all")), "{:?}", get("raised_all"));
}
