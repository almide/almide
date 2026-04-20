use almide::diagnostic::{Diagnostic, Level};
use almide::diagnostic_render;

// ---- Error creation ----

#[test]
fn diagnostic_error_fields() {
    let d = Diagnostic::error("type mismatch", "expected Int, got String", "fn add");
    assert_eq!(d.level, Level::Error);
    assert_eq!(d.message, "type mismatch");
    assert_eq!(d.hint, "expected Int, got String");
    assert_eq!(d.context, "fn add");
    assert!(d.file.is_none());
    assert!(d.line.is_none());
    assert!(d.col.is_none());
}

#[test]
fn diagnostic_warning_fields() {
    let d = Diagnostic::warning("unused variable", "prefix with _", "let x");
    assert_eq!(d.level, Level::Warning);
    assert_eq!(d.message, "unused variable");
}

// ---- Display (no color mode) ----

#[test]
fn diagnostic_display_error() {
    let d = Diagnostic::error("type mismatch", "expected Int", "fn f");
    let out = d.display();
    assert!(out.contains("error: type mismatch"), "got: {}", out);
    assert!(out.contains("hint: expected Int"), "got: {}", out);
    assert!(out.contains("in fn f"), "got: {}", out);
}

#[test]
fn diagnostic_display_warning() {
    let d = Diagnostic::warning("unused", "prefix with _", "let x");
    let out = d.display();
    assert!(out.contains("warning: unused"), "got: {}", out);
}

#[test]
fn diagnostic_display_with_file_line() {
    let d = Diagnostic::error("err", "hint", "ctx")
        .at("main.almd", 5);
    let out = d.display();
    assert!(out.contains("main.almd:5"), "got: {}", out);
}

#[test]
fn diagnostic_display_with_file_only() {
    let mut d = Diagnostic::error("err", "hint", "ctx");
    d.file = Some("test.almd".into());
    let out = d.display();
    assert!(out.contains("test.almd"), "got: {}", out);
}

#[test]
fn diagnostic_display_with_line_only() {
    let mut d = Diagnostic::error("err", "hint", "ctx");
    d.line = Some(10);
    let out = d.display();
    assert!(out.contains("at line 10"), "got: {}", out);
}

#[test]
fn diagnostic_display_empty_hint() {
    let d = Diagnostic::error("err", "", "ctx");
    let out = d.display();
    assert!(!out.contains("hint:"), "empty hint should be omitted");
}

#[test]
fn diagnostic_display_empty_context() {
    let d = Diagnostic::error("err", "hint", "");
    let out = d.display();
    assert!(!out.contains("in "), "empty context should be omitted");
}

#[test]
fn diagnostic_display_with_col() {
    let mut d = Diagnostic::error("err", "hint", "");
    d.file = Some("app.almd".into());
    d.line = Some(3);
    d.col = Some(7);
    let out = d.display();
    assert!(out.contains("app.almd:3:7"), "got: {}", out);
}

// ---- Display with source ----

#[test]
fn diagnostic_display_with_source_shows_line() {
    let mut d = Diagnostic::error("type mismatch", "expected Int", "");
    d.file = Some("test.almd".into());
    d.line = Some(2);
    d.col = Some(5);
    let source = "fn f() -> Int =\n  let x = \"hello\"\n  x";
    let out = diagnostic_render::display_with_source(&d, source);
    assert!(out.contains("let x = \"hello\""), "should show source line, got: {}", out);
    assert!(out.contains("^"), "should show caret underline, got: {}", out);
}

#[test]
fn diagnostic_display_with_source_no_line() {
    let d = Diagnostic::error("err", "hint", "ctx");
    let out = diagnostic_render::display_with_source(&d, "fn f() -> Int = 1");
    // Should still show basic display without source snippet
    assert!(out.contains("error: err"));
}

#[test]
fn diagnostic_display_with_source_line_out_of_range() {
    let mut d = Diagnostic::error("err", "", "");
    d.line = Some(999);
    let out = diagnostic_render::display_with_source(&d, "fn f() -> Int = 1");
    // Should not crash, just show basic display
    assert!(out.contains("error: err"));
}

// ---- Secondary spans ----

#[test]
fn diagnostic_with_secondary() {
    let d = Diagnostic::error("type mismatch", "expected Int", "")
        .with_secondary(3, Some(5), "declared as String here");
    assert_eq!(d.secondary.len(), 1);
    assert_eq!(d.secondary[0].line, 3);
    assert_eq!(d.secondary[0].label, "declared as String here");
}

#[test]
fn diagnostic_secondary_in_source() {
    let mut d = Diagnostic::error("type mismatch", "", "");
    d.file = Some("test.almd".into());
    d.line = Some(3);
    d.col = Some(1);
    d = d.with_secondary(1, Some(5), "declared here");
    let source = "fn f() -> Int =\n  let x = 1\n  \"hello\"";
    let out = diagnostic_render::display_with_source(&d, source);
    assert!(out.contains("declared here"), "got: {}", out);
}

// ---- at_span ----

#[test]
fn diagnostic_at_span() {
    let span = almide::ast::Span { line: 10, col: 3, end_col: 8 };
    let d = Diagnostic::error("err", "hint", "ctx")
        .at_span("file.almd", span);
    assert_eq!(d.file, Some("file.almd".into()));
    assert_eq!(d.line, Some(10));
    assert_eq!(d.col, Some(3));
}

// ---- Multiple secondary spans ----

#[test]
fn diagnostic_multiple_secondaries() {
    let d = Diagnostic::error("conflict", "", "")
        .with_secondary(1, Some(1), "first declaration")
        .with_secondary(5, Some(1), "second declaration");
    assert_eq!(d.secondary.len(), 2);
}

#[test]
fn diagnostic_multiple_secondaries_in_source() {
    let mut d = Diagnostic::error("type conflict", "", "");
    d.file = Some("test.almd".into());
    d.line = Some(5);
    d.col = Some(1);
    d = d.with_secondary(1, Some(5), "declared as Int here")
         .with_secondary(3, Some(5), "used as String here");
    let source = "let x: Int = 1\nfn f() -> Int = x\nlet y = x\nfn g() -> String = x\n\"bad\"";
    let out = diagnostic_render::display_with_source(&d, source);
    assert!(out.contains("declared as Int here"));
    assert!(out.contains("used as String here"));
}

// ---- End col (caret range) ----

#[test]
fn diagnostic_end_col_caret_range() {
    let mut d = Diagnostic::error("err", "", "");
    d.line = Some(1);
    d.col = Some(5);
    d.end_col = Some(10);
    let out = diagnostic_render::display_with_source(&d, "let x = \"hello\"");
    // Should show multiple carets
    assert!(out.contains("^^^^^"), "expected 5 carets for col 5..10, got: {}", out);
}

// ---- Clone ----

#[test]
fn diagnostic_clone() {
    let d = Diagnostic::error("err", "hint", "ctx")
        .at("file.almd", 5)
        .with_secondary(3, Some(1), "label");
    let d2 = d.clone();
    assert_eq!(d.message, d2.message);
    assert_eq!(d.secondary.len(), d2.secondary.len());
}

// ---- Empty source line ----

#[test]
fn diagnostic_empty_source_line() {
    let mut d = Diagnostic::error("err", "", "");
    d.line = Some(2);
    let out = diagnostic_render::display_with_source(&d, "line1\n\nline3");
    // Empty source line should not crash
    assert!(out.contains("error: err"));
}

// ---- Secondary without col ----

#[test]
fn diagnostic_secondary_no_col() {
    let d = Diagnostic::error("err", "", "")
        .with_secondary(1, None, "somewhere on this line");
    assert_eq!(d.secondary[0].col, None);
}

// ---- Level equality ----

#[test]
fn level_eq() {
    assert_eq!(Level::Error, Level::Error);
    assert_eq!(Level::Warning, Level::Warning);
    assert_ne!(Level::Error, Level::Warning);
}

// ---- Phase 3: try_replace round-trip ----

#[test]
fn with_try_replace_populates_both_fields() {
    let d = Diagnostic::error("e", "h", "c").with_try_replace(2, 5, 13, "int.parse");
    assert_eq!(d.try_snippet.as_deref(), Some("int.parse"));
    assert_eq!(d.try_replace_span, Some((2, 5, 13)));
}

#[test]
fn apply_try_to_round_trips_through_full_source() {
    // Simulates the Phase 3 harness workflow: a diagnostic names the
    // offending token span + the replacement; `apply_try_to` rewrites
    // the source to the expected fixed form.
    let broken = "fn parse(s: String) -> Result[Int, String] =\n    string.length(s) |> int.parse\n";
    //                                                                   ^^^^^^^^^^^^^ line 2, cols 5..18 (exclusive)
    let d = Diagnostic::error("e", "h", "c").with_try_replace(2, 5, 18, "string.len");
    let rewritten = d.apply_try_to(broken).unwrap();
    assert_eq!(
        rewritten,
        "fn parse(s: String) -> Result[Int, String] =\n    string.len(s) |> int.parse\n"
    );
}
