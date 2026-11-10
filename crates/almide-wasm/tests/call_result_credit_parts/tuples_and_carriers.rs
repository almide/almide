// call_result_credit.rs, part 2 (include!-spliced: the 800-line file discipline).
// The rows over a destructured tuple, a carrier turned into an Option, and
// operands bound before one that propagates.

/// #2515 — `let (a, b) = <owned tuple>`: the destructure read the fields out
/// of a subject nobody owned, so the tuple and everything it held stayed at
/// rc 1 forever (160 B per call with a fresh 64 B buffer inside). The owned
/// subject is now named first (`let t = mk(i); let (a, b) = t`, arg_temps.rs),
/// so the Bind route owns it and the frame exit releases it; the binds stay
/// borrowed views of its fields, exactly as they are under a written-out name.
fn destructure_program(n: u32, body: &str) -> String {
    format!(
        r#"fn mkt(v: Int) -> (Bytes, Int) = (bytes.zeros(64), v)

fn mks(v: Int) -> (String, List[Int]) = (int.to_string(v % 10) + "s", [v, v])

fn stamp(mut b: Bytes, v: Int) -> Int = {{
  bytes.put_u8(b, 0, v)
  v
}}

effect fn estamp(mut b: Bytes, v: Int) -> Int = {{
  bytes.put_u8(b, 0, v)
  ok(v)
}}

fn first(v: Int) -> Bytes = {{
  let (b, k) = mkt(v)
  b
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

#[test]
fn an_owned_tuple_destructure_releases_the_tuple_and_its_contents() {
    let rows = [
        ("let (b, k) = mkt(i)", "    let (b, k) = mkt(i)\n    total = total + bytes.len(b) + k - i", "64000", "512000"),
        (
            "let (s, xs) = mks(i)",
            "    let (s, xs) = mks(i)\n    total = total + string.len(s) + list.len(xs)",
            "4000",
            "32000",
        ),
        ("let (b, _) = mkt(i) in a tail", "    let b = first(i)\n    total = total + bytes.len(b)", "64000", "512000"),
        (
            "stamp(fresh buffer) — the mut-param value return",
            "    var b = bytes.zeros(64)\n    let r = stamp(b, 7)\n    total = total + r + (bytes.get(b, 0) ?? 0)",
            "14000",
            "112000",
        ),
        (
            "estamp(fresh buffer)! — the mut-param effect value return",
            "    var b = bytes.zeros(64)\n    let r = estamp(b, 7)!\n    total = total + r + (bytes.get(b, 0) ?? 0)",
            "14000",
            "112000",
        ),
    ]
    .map(|(name, body, e1, e8)| (name, growth_in(destructure_program, name, body, e1, e8)));
    let leaked: Vec<_> = rows.iter().filter(|(_, g)| *g != 0).collect();
    assert!(leaked.is_empty(), "B per call leaked: {leaked:?}");
}

/// #2515, the BORROWED-subject cell: a destructure of a var the frame
/// already owns must neither take nor spend a credit on it — it stays at
/// today's (flat) count; an over-release would read freed memory here.
#[test]
fn a_borrowed_tuple_destructure_keeps_its_subject() {
    let g = growth_in(
        destructure_program,
        "let t = mkt(i); let (b, k) = t; read t again",
        "    let t = mkt(i)\n    let (b, k) = t\n    let (c, _) = t\n    total = total + bytes.len(b) + bytes.len(c) + k - i",
        "128000",
        "1024000",
    );
    assert_eq!(g, 0, "borrowed destructure: {g} B per call");
}

/// #2516 — `f(x)?` over an OWNED carrier: the some-cell receives the
/// payload's credit (the carrier's spine is released under it, #2509), so
/// the node is owned and the bind must not add the conservative `+1` that
/// left the cell at rc 1 forever (144 B per call).
fn to_option_program(n: u32, body: &str) -> String {
    format!(
        r#"effect fn mko(v: Int) -> Bytes = if v % 3 == 2 then err("skip") else ok(bytes.zeros(64))

fn mkr(v: Int) -> Result[Bytes, String] = if v % 3 == 2 then err("skip") else ok(bytes.zeros(64))

fn width(o: Bytes?) -> Int = match o {{
  some(b) => bytes.len(b),
  none => 1,
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

#[test]
fn an_owned_carrier_to_option_releases_the_some_cell() {
    let g = growth_in(
        to_option_program,
        "let o = mko(i)?",
        "    let o = mko(i)?\n    total = total + width(o)",
        "43021",
        "344042",
    );
    assert_eq!(g, 0, "owned `?`: {g} B per call leaked");
}

/// #2516 / #2969, the BORROWED-carrier cell: the some-cell shares the payload
/// of a carrier some other holder releases (its own `+1`), so the cell is an
/// owned value like the owned-carrier one — the bind takes it without a
/// second credit and the frame releases it, payload share included.
#[test]
fn a_borrowed_carrier_to_option_releases_the_some_cell() {
    let g = growth_in(
        to_option_program,
        "let r: Result = mkr(i); let o = r?",
        "    let r: Result[Bytes, String] = mkr(i)\n    let o = r?\n    let p = r?\n    total = total + width(o) + width(p)",
        "86042",
        "688084",
    );
    assert_eq!(g, 0, "borrowed `?`: {g} B per call leaked");
}

/// A `!` in a LATER operand of a call or a map literal returns while the
/// operands before it are already evaluated. Their credits — a produced
/// block's own, or the `+1` an owning parameter takes for a variable — were
/// held by the operand stack alone and never released: one block per failed
/// call. The high-water mark must not grow with the number of failures.
fn later_operand_err_program(n: u32, body: &str) -> String {
    format!(
        r#"effect fn word(x: String) -> String = {{
  guard x != "stop" else err("stopped")
  x + "!"
}}

fn tag(label: String) -> String = label + "."

fn join2(a: String, b: String) -> String = a + b

fn join3(a: String, b: String, c: String) -> String = a + b + c

effect fn produced(x: String) -> Int = string.len(join3(tag(x), word(x)!, tag(x)))

effect fn variable(x: String) -> Int = {{
  let y = x + "~"
  string.len(join2(y, word(x)!))
}}

effect fn nested(x: String) -> Int = string.len(join2(tag(x), join2(tag(x + "in"), word(x)!)))

effect fn module_op(x: String) -> Int = string.len(string.replace(tag(x), word(x)!, "z"))

effect fn map_entry(x: String) -> Int = {{
  let m = [tag(x): word(x)!]
  map.len(m)
}}

effect fn main() -> Unit = {{
  var total = 0
  for i in 0..<{n} {{
{body}
  }}
  println("${{total}}")
}}
"#
    )
}

#[test]
fn a_later_operand_that_propagates_releases_the_operands_before_it() {
    for f in ["produced", "variable", "nested", "module_op", "map_entry"] {
        let failing = format!("    total = total + (match {f}(\"stop\") {{ ok(v) => v, err(_) => 1 }})");
        flat_in(later_operand_err_program, &format!("{f} (err)"), &failing, "1000", "8000");
        // The ok path is the control: the same sites, nothing propagating.
        let passing = format!("    total = total + (match {f}(\"s\") {{ ok(_) => 2, err(_) => 1 }})");
        flat_in(later_operand_err_program, &format!("{f} (ok)"), &passing, "2000", "16000");
    }
}
