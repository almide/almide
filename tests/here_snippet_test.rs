//! Here/Try/Hint three-part format — Phase 1 MVP
//! (roadmap: diagnostics-here-try-hint).
//!
//! Phase 1 delivers only the `Diagnostic.here_snippet` field + builder
//! + auto-population from source during `display_with_source`. Full
//! migration, harness, and CI gate are deferred to Phase 2-5.

use almide::diagnostic::Diagnostic;
use almide::diagnostic_render::{display, display_with_source};

#[test]
fn diagnostic_has_here_snippet_field() {
    let d = Diagnostic::error("msg", "hint", "ctx").with_here("let x = y");
    assert_eq!(d.here_snippet.as_deref(), Some("let x = y"));
}

#[test]
fn with_here_collapses_multiline_to_first_nonempty_line() {
    let d = Diagnostic::error("m", "h", "c").with_here("\n\n  let x = 1  \nunused tail\n");
    assert_eq!(d.here_snippet.as_deref(), Some("let x = 1"));
}

#[test]
fn display_renders_here_line_before_hint() {
    let d = Diagnostic::error("m", "h", "c")
        .with_here("let x = y")
        .at("main.almd", 3);
    let out = display(&d);
    let here_at = out.find("here: let x = y").expect("here rendered");
    let hint_at = out.find("hint: h").expect("hint rendered");
    assert!(here_at < hint_at, "`here:` must precede `hint:`\n{}", out);
}

#[test]
fn display_without_here_keeps_legacy_two_line_format() {
    let d = Diagnostic::error("m", "h", "c").at("main.almd", 3);
    let out = display(&d);
    assert!(!out.contains("here:"), "no `here:` line when unset:\n{}", out);
    assert!(out.contains("hint: h"));
}

#[test]
fn display_with_source_auto_populates_here_from_span() {
    let d = Diagnostic::error("undefined", "typo?", "ctx")
        .at("main.almd", 2);
    let src = "fn main() = {\n    grettings(\"world\")\n}\n";
    let out = display_with_source(&d, src);
    assert!(out.contains("here: grettings(\"world\")"),
        "auto-populated `here:` missing:\n{}", out);
}

#[test]
fn explicit_with_here_wins_over_auto_population() {
    let d = Diagnostic::error("m", "h", "c")
        .at("main.almd", 1)
        .with_here("CUSTOM");
    let src = "let x = 1\n";
    let out = display_with_source(&d, src);
    assert!(out.contains("here: CUSTOM"), "explicit wins:\n{}", out);
    assert!(!out.contains("here: let x = 1"),
        "auto-pop should NOT overwrite:\n{}", out);
}

#[test]
fn json_emits_here_and_try_fields() {
    let d = Diagnostic::error("m", "h", "c")
        .with_here("let x = y")
        .with_try("let x = yval");
    let j = almide::diagnostic_render::to_json(&d);
    assert!(j.contains(r#""here":"let x = y""#), "json here:\n{}", j);
    assert!(j.contains(r#""try":"let x = yval""#), "json try:\n{}", j);
}

#[test]
fn json_emits_null_when_here_unset() {
    let d = Diagnostic::error("m", "h", "c");
    let j = almide::diagnostic_render::to_json(&d);
    assert!(j.contains(r#""here":null"#), "json here null:\n{}", j);
    assert!(j.contains(r#""try":null"#), "json try null:\n{}", j);
}
