//! A span inside `${…}` must measure the same as the same span outside it (#2095).
//!
//! `parse_interpolation_expr_part` sub-lexes the interpolation body and remaps the
//! sub-lexer's coordinates into the parent source. It used to remap only `col`,
//! leaving every token with a parent-space start next to a sub-string-space end —
//! a number smaller than its own beginning, which downstream reads as "no end".
//!
//! Two things ride on a measurable end, so both are asserted here: a fix-it needs
//! a range to replace, and the caret is drawn from the span's width. The same
//! typo therefore underlined nine columns and offered a rename outside a string,
//! and underlined one column and offered nothing inside one.
//!
//! The assertions compare the two POSITIONS against each other rather than
//! against a literal width. A test that pinned "9 carets" would keep passing if
//! both sides regressed together, which is exactly the failure this guards.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[derive(Debug)]
struct Diag {
    line: usize,
    col: usize,
    end_col: Option<usize>,
    has_fix: bool,
    suggestions: usize,
}

impl Diag {
    /// `None` when the diagnostic carries no end — the shape this issue is about.
    fn width(&self) -> Option<usize> {
        self.end_col.map(|e| e.saturating_sub(self.col))
    }
}

/// Run `almide check --json` and return one record per diagnostic, in order.
fn diagnostics(dir: &std::path::Path, source: &str) -> Vec<Diag> {
    let file = dir.join("interp.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .args(["check", file.to_str().unwrap(), "--json"])
        .output()
        .expect("run almide check --json");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("line").and_then(|x| x.as_u64()).is_some())
        .map(|v| Diag {
            line: v["line"].as_u64().unwrap() as usize,
            col: v["col"].as_u64().unwrap() as usize,
            end_col: v.get("end_col").and_then(|x| x.as_u64()).map(|x| x as usize),
            has_fix: v.get("try_replace").map(|x| !x.is_null()).unwrap_or(false),
            suggestions: v
                .get("suggestions")
                .and_then(|x| x.as_array())
                .map(|a| a.len())
                .unwrap_or(0),
        })
        .collect()
}

/// The same call, once as a plain expression and once inside an interpolation.
const BOTH_POSITIONS: &str = "fn main() -> Unit = {\n\
\x20 let a = list.nope([1])\n\
\x20 println(\"${list.nope([1])}\")\n\
}\n";

#[test]
fn an_interpolated_span_measures_the_same_as_a_plain_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let diags = diagnostics(dir.path(), BOTH_POSITIONS);
    assert_eq!(diags.len(), 2, "expected one diagnostic per position: {diags:?}");

    let (plain, interpolated) = (&diags[0], &diags[1]);
    assert_eq!(plain.line, 2, "{diags:?}");
    assert_eq!(interpolated.line, 3, "{diags:?}");

    assert_eq!(
        interpolated.width(),
        plain.width(),
        "the same call measured differently inside `${{}}`: {diags:?}"
    );
    assert!(
        interpolated.width().is_some(),
        "an interpolated span with no end cannot be underlined or replaced: {diags:?}"
    );
}

#[test]
fn an_interpolated_call_still_gets_its_fix_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let diags = diagnostics(dir.path(), BOTH_POSITIONS);
    let (plain, interpolated) = (&diags[0], &diags[1]);

    assert!(plain.has_fix, "the plain position lost its fix-it: {diags:?}");
    assert_eq!(
        interpolated.has_fix, plain.has_fix,
        "a fix-it offered outside `${{}}` and withheld inside it: {diags:?}"
    );
    assert_eq!(
        interpolated.suggestions, plain.suggestions,
        "suggestion counts differ by position: {diags:?}"
    );
}

/// The remap runs per interpolation, and `col_offset` advances by a different
/// rule across a nested string literal (`scan_nested_string_literal`). An
/// off-by-N would hide in exactly these two shapes, so each is measured against
/// the plain baseline rather than trusted.
#[test]
fn nesting_does_not_shift_the_end() {
    let dir = tempfile::tempdir().expect("tempdir");
    let baseline = diagnostics(dir.path(), BOTH_POSITIONS)[0]
        .width()
        .expect("the plain position measures");

    // (source, which diagnostic carries the nested call)
    let cases = [
        // A second interpolation that follows one containing a string literal:
        // the literal advances col_offset through the nested-scan path.
        "fn main() -> Unit = {\n  println(\"a ${string.len(\"x\")} b ${list.nope([1])} c\")\n}\n",
        // An interpolation inside a string literal inside an interpolation.
        "fn main() -> Unit = {\n  println(\"${\"inner ${list.nope([1])}\"}\")\n}\n",
    ];
    for source in cases {
        let diags = diagnostics(dir.path(), source);
        let nested = diags
            .iter()
            .find(|d| d.width().is_some())
            .unwrap_or_else(|| panic!("no measurable span in:\n{source}\n{diags:?}"));
        assert_eq!(
            nested.width(),
            Some(baseline),
            "nesting shifted the end in:\n{source}\n{diags:?}"
        );
        assert!(nested.has_fix, "nested position lost its fix-it:\n{source}\n{diags:?}");
    }
}
