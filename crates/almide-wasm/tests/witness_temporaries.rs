//! #2755 (#1696 step 4) — TEMPORARIES in the structural witness, on the
//! flat certificate alphabet `{i, a, d, m}`. A nested call's owned result is
//! the temporary its enclosing site records (`im` into an owned param, `id`
//! when parked for a borrowed one); a concat's operands are bound first by
//! arg_temps.rs, so each is a local (`i` … `d`) and the concat itself is a
//! fresh value; a constructor's payload store is a share-and-move (`am`) or
//! a move of a temporary (`im`). Every certificate here balances AND is
//! accepted by the portable checker. The shapes whose sites are NOT hooks
//! decline with a counted reason instead of certifying.

const PROGRAM: &str = r#"type Box =
  | Full(String)
  | Empty

fn take(xs: List[Int]) -> Int = 3

fn mk(n: Int) -> List[Int] = {
  let a = [n, n]
  a
}

fn nest_owned() -> Int = take(mk(3))

fn nest_borrowed() -> Int = list.len(mk(3))

fn nest_deep(n: Int) -> Int = take(mk(list.len(mk(n))))

fn cat(s: String) -> String = s + "!" + s

fn wrap(xs: List[Int]) -> Option[List[Int]] = some(xs)

fn wrap_fresh() -> Option[List[Int]] = some(mk(1))

fn boxed(s: String) -> Box = Full(s)

fn listed(s: String) -> List[String] = [s, "x"]

fn short(a: Bool, xs: List[Int]) -> Bool = a and take(xs) > 0

fn eq_fresh(x: Option[Int]) -> Bool = x == some(1)

effect fn main() -> Unit = {
  println("${nest_owned()} ${nest_borrowed()} ${nest_deep(2)} ${cat("a")}")
  println("${list.len(wrap([1]) ?? [])} ${list.len(wrap_fresh() ?? [])} ${list.len(listed("y"))}")
  let b = boxed("z")
  println("${short(true, [1])} ${eq_fresh(some(1))}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("temporaries.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn temporaries_witness_exactly_and_unhooked_shapes_decline() {
    // ONE test: the witness sink is process-global, so two tests collecting
    // in parallel threads would steal each other's frames.
    let w = witnesses();
    let expect = [
        // mk's owned result is born and moves into take's owned param.
        ("nest_owned", "im\n"),
        // `list.len` BORROWS its argument (arm.rs `ArgMode::Borrow`): the
        // scope parks the temporary and releases it after the op.
        ("nest_borrowed", "id\n"),
        // Two levels: the inner temporary is parked for `list.len`, the
        // outer moves into take.
        ("nest_deep", "id\nim\n"),
        // `s` is only read (borrowed: no credit, an empty stream); the inner
        // concat is bound (`i` … `d`), the outer one moves out.
        ("cat", "\nid\nim\n"),
        // The borrowed param shares into the payload slot; the cell moves out.
        ("wrap", "am\nim\n"),
        // mk's result moves into the slot, the cell moves out.
        ("wrap_fresh", "im\nim\n"),
        // A variant case's payload store is the same share-and-move.
        ("boxed", "am\nim\n"),
        // A list literal's element stores: the Var shares in, the static
        // literal is born and moves in, the spine moves out.
        ("listed", "am\nim\nim\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed; got {:?}", w.get(name)));
        assert_eq!(got, cert, "{name}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // The shapes whose sites are NOT hooks decline instead of certifying.
    // `and`'s right operand runs on one arm of a branch site (#2756): the
    // param's share into `take` happens on that path only.
    let short = w.get("short").map(String::as_str).unwrap_or("<none>");
    assert_eq!(short, "{iamd|id}\n");
    assert!(accepted(short));
    // A fresh heap operand of `==` is an unowned temporary no hook records
    // (#2972's leak) — declined, never certified.
    assert_eq!(w.get("eq_fresh").map(String::as_str), Some("!decline:heap-operand:OptionSome\n"));
}
