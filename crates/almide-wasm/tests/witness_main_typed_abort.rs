//! #3470 — main's abort on an error that is not a `String`, in the
//! structural witness. A failed `!` in `main` renders its message from the
//! error value (its repr, or a joined `List[String]`): a FRESH block born on
//! the aborting arm, discharged by the checker's abort terminal with
//! everything else the path holds. A `fan { … }` block in `main` keeps a
//! typed-error arm's err value and, after every arm, aborts from a one-arm
//! site per such arm the same way.

const PROGRAM: &str = r#"type Bad = | Bad(Int)

fn typed(n: Int) -> Result[Int, Bad] = if n > 3 then err(Bad(n)) else ok(n)

fn named(n: Int) -> Result[String, Bad] = if n > 3 then err(Bad(n)) else ok("n${n}")

fn many(n: Int) -> Result[Int, List[String]] = if n > 3 then err(["a", "b"]) else ok(n)

fn num(n: Int) -> Result[Int, String] = if n > 3 then err("big") else ok(n)

effect fn main() -> Unit = {
  let keep = "k${typed(1)!}"
  let s = named(2)!
  let m = many(3)!
  let (a, b, c) = fan {
    typed(1)
    num(2)
    named(3)
  }
  println("${keep} ${s} ${m} ${a} ${b} ${c}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("main_typed_abort.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn main_typed_error_aborts_witness_and_are_accepted() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let main = w.get("main").map(String::as_str).unwrap_or("<none>").to_string();
    // Recorded, not declined, and accepted by the proven checker's portable
    // twin: the rendered message is born on the aborting arm and the abort
    // terminal discharges it.
    assert!(!main.starts_with('!') && accepted(&main), "main: {main:?}");
    // One fresh message per non-String abort site, each living only on its
    // aborting arm (`{|it}`): the three `!`s (`Bad` twice, `List[String]`)
    // and the fan block's two typed-error arms. The String arm's message is
    // a view of its carrier's slot and records nothing.
    let born = main.lines().filter(|l| *l == "{|it}").count();
    assert_eq!(born, 5, "main: {main:?}");
}
