//! #2807: a structural-leg wall says WHERE it came from.
//!
//! The wall's reason string (`ty-mismatch:…`, `expr:Continue`) is a census key
//! and carries no location; `almide_wasm::decline_site::last()` is the side
//! channel the CLI prints as `--> in fn `name` (…, line N)`. The shape used
//! here is a `continue` inside a VALUE-position block, a decline the structural
//! leg records as `expr:Continue` today (the statement-position forms lower
//! since #2745). If that shape starts lowering, swap in any other declined
//! expression: the assertion is about the location, not the reason.

use almide_wasm::decline_site::{self, DeclineSite};

const SRC: &str = "fn keep(xs: List[String]) -> Int = {
  var n = 0
  for x in xs {
    let k = {
      if string.len(x) == 0 then continue
      1
    }
    n = n + k
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
        Some(DeclineSite { function: "keep".into(), module: None, line: Some(4), top_let: false }),
        "the wall must point at `keep`, line 4 (the value-position block holding the `continue`)"
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
    let s = DeclineSite { function: "cli.dispatch_ok".into(), module: Some("cli".into()), line: Some(42), top_let: false };
    assert_eq!(s.to_string(), "fn `cli.dispatch_ok` (module `cli`, line 42)");
}

#[test]
fn a_top_let_site_renders_as_a_top_let() {
    // #2807: `main` lowers every module's top-let initializers; a wall there is
    // the top-let's, in ITS module — never an entry-file line of `main`.
    let s = DeclineSite { function: "rules.BINDS".into(), module: Some("rules".into()), line: Some(1167), top_let: true };
    assert_eq!(s.to_string(), "top-level let `rules.BINDS` (module `rules`, line 1167)");
}
