//! #2807: a structural-leg wall says WHERE it came from.
//!
//! The wall's reason string (`ty-mismatch:…`, `expr:Continue`) is a census key
//! and carries no location; `almide_wasm::decline_site::last()` is the side
//! channel the CLI prints as `--> in fn `name` (…, line N)`. The shape used
//! here is `guard … else continue`, a decline the structural leg records as
//! `expr:Continue` today. If that shape starts lowering, swap in any other
//! declined expression: the assertion is about the location, not the reason.

use almide_wasm::decline_site::{self, DeclineSite};

const SRC: &str = "fn keep(xs: List[String]) -> Int = {
  var n = 0
  for x in xs {
    guard string.len(x) > 0 else continue
    n = n + 1
  }
  n
}

effect fn main() -> Unit = println(int.to_string(keep([\"a\", \"\"])))
";

#[test]
fn a_structural_wall_names_its_function_and_line() {
    let ir = almide_spine::s5::lower_to_ir("decline_site.almd", SRC).expect("front");
    let err = almide_wasm::emit_program(&ir).expect_err("the shape must decline");
    assert!(matches!(err, almide_wasm::EmitError::Unsupported(_)), "{err:?}");
    assert_eq!(
        decline_site::last(),
        Some(DeclineSite { function: "keep".into(), module: None, line: Some(4) }),
        "the wall must point at `keep`, line 4 (the `guard … else continue`)"
    );
}

#[test]
fn a_successful_emit_clears_the_last_site() {
    let bad = almide_spine::s5::lower_to_ir("decline_site.almd", SRC).expect("front");
    let _ = almide_wasm::emit_program(&bad);
    let ok = almide_spine::s5::lower_to_ir("ok.almd", "effect fn main() -> Unit = println(\"hi\")\n")
        .expect("front");
    almide_wasm::emit_program(&ok).expect("emit");
    assert_eq!(decline_site::last(), None, "a site from an earlier emit must not survive");
}

#[test]
fn the_site_renders_module_and_line() {
    let s = DeclineSite { function: "cli.dispatch_ok".into(), module: Some("cli".into()), line: Some(42) };
    assert_eq!(s.to_string(), "fn `cli.dispatch_ok` (module `cli`, line 42)");
}
