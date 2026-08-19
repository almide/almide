//! Byte-exact parity with the incumbent (almide@a877d2138).
//!
//! `golden/diag-golden.txt` was produced by compiling the SAME battery body
//! (`golden/battery_body.rs`) against the incumbent's almide-base +
//! src/diagnostic_render.rs and capturing its output — see PORTLOG.md unit 1
//! for the generator provenance. This test replays the body against the
//! ported crate; any byte of divergence fails.

mod env {
    pub use almide_diag::diagnostic::*;
    pub use almide_diag::render::{display, display_with_source, to_json};
    pub use almide_diag::span::Span;
}

mod battery {
    use super::env::*;
    include!("golden/battery_body.rs");
}

#[test]
fn matches_incumbent_golden_byte_for_byte() {
    let got = battery::golden_output();
    let want = include_str!("golden/diag-golden.txt");
    assert_eq!(got, want, "diagnostic rendering diverged from the incumbent golden (almide@a877d2138)");
}

/// Exhaustiveness discipline, adopted from Roc's comptime-enumerated renderer
/// parity suite (roc@707a8082, src/reporting/parity_test.zig:1-11): coverage
/// of the diagnostic surface is a checked property, not a review habit.
///
/// Compile-time half: the full destructure below has no `..`, so adding a
/// field to `Diagnostic` refuses to compile until this test — and therefore
/// the battery — is deliberately revisited.
///
/// Runtime half: the golden must WITNESS each field/variant in both its
/// populated and its absent form (post-port additions like `multi_fix`, which
/// the incumbent cannot produce, are excluded from the shared battery and
/// carry their own unit tests instead).
#[test]
fn battery_witnesses_every_incumbent_field_and_variant() {
    let d = env::Diagnostic::error("m", "h", "c");
    #[allow(clippy::let_underscore_untyped)]
    let env::Diagnostic {
        level: _, code: _, message: _, hint: _, context: _, file: _, line: _,
        col: _, end_col: _, secondary: _, try_snippet: _, here_snippet: _,
        try_replace_span: _, try_applicability: _, multi_fix: _,
    } = d;

    let g = battery::golden_output();
    // Level: both variants rendered.
    assert!(g.contains("\nerror[") && g.contains("\nwarning:"));
    // code: present and absent.
    assert!(g.contains("[E014]") && g.contains("\nerror: "));
    // Applicability: all four wire spellings.
    for a in ["machine-applicable", "maybe-incorrect", "has-placeholders", "unspecified"] {
        assert!(g.contains(a), "applicability {a} not witnessed");
    }
    // file/line/col permutations incl. absences.
    assert!(g.contains("--> app.almd:2:15") && g.contains("--> app.almd:1") && g.contains("--> lib/util.almd") && g.contains("at line 7"));
    // end_col: set and null in JSON.
    assert!(g.contains("\"end_col\":19") && g.contains("\"end_col\":null"));
    // secondary: with col, without col (null), with label text.
    assert!(g.contains("\"col\":null") && g.contains("declared as Int here"));
    // here / try rows, machine + deletion labels, guessed-span refusal.
    assert!(g.contains("\n  here: ") && g.contains("\n  try:"));
    assert!(g.contains("(machine-applicable — `almide fix` applies it)"));
    assert!(g.contains("delete the highlighted text"));
    assert!(g.contains("guessed-span"));
    // hint and context rows.
    assert!(g.contains("\n  hint: ") && g.contains("\n  in "));
    // fix engine: applied result and refusals.
    assert!(g.contains("Some(\"if not user_admin then x\")") && g.contains("oob-line:      None"));
}
