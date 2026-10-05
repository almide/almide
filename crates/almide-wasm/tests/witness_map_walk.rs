//! #2758 (#1696 step 4) — `for (k, v) in m` in the structural witness. The
//! walk's cursor takes a credit on the subject before the loop (a borrowed
//! subject is shared, `a`; an owned one is the cursor's own block, `i`) and
//! releases it after the loop (`d`). Each entry is one activation whose key
//! and value are views of the entry's slots. An exit from the body leaves
//! with the cursor's credit held, so that frame declines.

const PROGRAM: &str = r#"fn make(n: Int) -> Map[String, Int] = ["a": n, "b": n + 1]

fn walk(m: Map[String, Int]) -> Int = {
  var t = 0
  for (k, v) in m {
    t = t + v + string.len(k)
  }
  t
}

fn walk_fresh(n: Int) -> Int = {
  var t = 0
  for (_, v) in make(n) {
    t = t + v
  }
  t
}

fn check(s: String) -> Result[Int, String] = if s == "z" then err("z") else ok(1)

effect fn walk_exit(m: Map[String, Int]) -> Int = {
  var t = 0
  for (k, _) in m {
    t = t + check(k)!
  }
  t
}

effect fn main() -> Unit = {
  let m = make(3)
  println("${walk(m)} ${walk_fresh(4)} ${walk_exit(m)!}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("map_walk.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn map_walks_witness_the_cursor_credit_and_views() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The borrowed param holds no credit (an empty line); the cursor
        // shares the subject and releases it after the loop (`ad`); the key
        // is a view of its entry's slot (an empty activation line).
        ("walk", "\nad\n\n"),
        // arg_temps.rs names the produced subject first: a bound block (`i`),
        // read by the walk (`b`), released at the exit (`d`); the cursor
        // shares it like a borrowed one (`ad`).
        ("walk_fresh", "ibd\nad\n\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed"));
        assert_eq!(got, cert, "{name}");
        assert!(
            almide_verify::check(almide_verify::Property::Ownership, got.as_bytes()),
            "{name}: the portable checker must accept {got:?}"
        );
    }
    // The `!` exit leaves the loop with the cursor's credit held: declined.
    assert_eq!(w.get("walk_exit").map(String::as_str), Some("!decline:forin-map:exit\n"));
}
