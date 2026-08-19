//! Multi-part fix-its: one atomic fix that touches several source sites.
//!
//! Greenfield evolution E1, adopted from rustc's `Substitution { parts }`
//! model (rust@93c9086f, compiler/rustc_errors/src/lib.rs:193-201) after the
//! 9-compiler diagnostics survey (../almide-references/RESEARCH-diagnostics.md).
//! The single-fix path (`try_replace_span` / `apply_try_to`) is untouched;
//! this module adds the N-site form under the same #1312 discipline:
//!
//! - **Refusal over corruption**: any part with a guessed span (1-indexed
//!   line/col of 0, inverted range) or any two overlapping parts refuse the
//!   whole fix — the builder stores nothing rather than half a rewrite.
//! - **Single read path**: `machine_multi_fix()` is the only accessor the
//!   fix engine may use, and it returns `None` unless the fix is tagged
//!   `MachineApplicable`.
//! - Span conventions match `apply_try_to`: 1-indexed chars, `col` inclusive,
//!   `end_col` exclusive, `end_col == col` inserts, `col == line_len + 1`
//!   addresses end-of-line.

use crate::diagnostic::{Applicability, Diagnostic};

/// One site of a multi-part fix. Same span semantics as `try_replace_span`.
#[derive(Debug, Clone, PartialEq)]
pub struct FixPart {
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
    pub snippet: String,
}

/// An atomic multi-site fix. Parts are stored sorted by (line, col) and are
/// guaranteed non-overlapping by construction — the builders refuse anything
/// else, so a stored `MultiFix` is always applicable as a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiFix {
    pub parts: Vec<FixPart>,
    pub applicability: Applicability,
}

fn validate(parts: &[FixPart]) -> bool {
    if parts.is_empty() {
        return false;
    }
    for p in parts {
        if p.line == 0 || p.col == 0 || p.end_col < p.col {
            return false;
        }
    }
    for w in parts.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        // Sorted by (line, col); on a shared line the earlier part must end
        // at or before the later one starts. Equal spans are overlapping.
        if a.line == b.line && (a.end_col > b.col || (a.col == b.col && a.end_col == b.end_col)) {
            return false;
        }
    }
    true
}

/// Replace `[line:col..line:end_col)` (1-indexed chars) with `snippet`.
/// Mirrors `Diagnostic::apply_try_to`'s span resolution for a single part.
fn replace_span(source: &str, line: usize, col: usize, end_col: usize, snippet: &str) -> Option<String> {
    let mut line_start = 0usize;
    let mut cur_line = 1usize;
    for (i, b) in source.bytes().enumerate() {
        if cur_line == line {
            break;
        }
        if b == b'\n' {
            cur_line += 1;
            line_start = i + 1;
        }
    }
    if cur_line != line {
        return None;
    }
    let line_tail = &source[line_start..];
    let line_end = line_tail.find('\n').map(|i| line_start + i).unwrap_or(source.len());
    let line_slice = &source[line_start..line_end];
    let col_to_byte = |target: usize| -> Option<usize> {
        match line_slice.char_indices().nth(target - 1) {
            Some((b, _)) => Some(b),
            None => {
                let n = line_slice.chars().count();
                if target == n + 1 { Some(line_slice.len()) } else { None }
            }
        }
    };
    let start_off = line_start + col_to_byte(col)?;
    let end_off = line_start + col_to_byte(end_col)?;
    if end_off < start_off || end_off > line_end {
        return None;
    }
    let mut out = String::with_capacity(source.len() + snippet.len());
    out.push_str(&source[..start_off]);
    out.push_str(snippet);
    out.push_str(&source[end_off..]);
    Some(out)
}

impl Diagnostic {
    /// Attach a **machine-applicable** multi-part fix: every part is applied,
    /// or none is. Invalid or overlapping parts refuse the whole fix (the
    /// diagnostic keeps rendering; nothing is stored to apply) — the worst
    /// case is a missing fix-it, never a corrupted file.
    pub fn with_machine_fix_parts(self, parts: Vec<(usize, usize, usize, &str)>) -> Self {
        self.with_fix_parts(parts, Applicability::MachineApplicable)
    }

    /// The suggested (never auto-applied) multi-part form.
    pub fn with_suggested_fix_parts(self, parts: Vec<(usize, usize, usize, &str)>) -> Self {
        self.with_fix_parts(parts, Applicability::MaybeIncorrect)
    }

    fn with_fix_parts(mut self, parts: Vec<(usize, usize, usize, &str)>, applicability: Applicability) -> Self {
        let mut parts: Vec<FixPart> = parts
            .into_iter()
            .map(|(line, col, end_col, snippet)| FixPart { line, col, end_col, snippet: snippet.to_string() })
            .collect();
        parts.sort_by_key(|p| (p.line, p.col));
        if !validate(&parts) {
            self.multi_fix = None;
            return self;
        }
        self.multi_fix = Some(MultiFix { parts, applicability });
        self
    }

    /// The one read path the fix engine may use for multi-part fixes.
    /// `None` unless tagged `MachineApplicable` — mirrors `machine_fix()`.
    pub fn machine_multi_fix(&self) -> Option<&MultiFix> {
        let mf = self.multi_fix.as_ref()?;
        if mf.applicability.is_machine_applicable() { Some(mf) } else { None }
    }

    /// Apply every part of the multi-part fix to `source`, or `None` if any
    /// part fails to locate (out-of-bounds line/col). Parts are applied in
    /// reverse document order so earlier spans stay valid. Atomic: a partial
    /// application is never returned.
    pub fn apply_multi_to(&self, source: &str) -> Option<String> {
        let mf = self.multi_fix.as_ref()?;
        let mut out = source.to_string();
        for p in mf.parts.iter().rev() {
            out = replace_span(&out, p.line, p.col, p.end_col, &p.snippet)?;
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_site_atomic_rename() {
        // Rename `foo` at both use sites in one fix.
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 5, 8, "bar"), (2, 9, 12, "bar")]);
        assert_eq!(
            d.apply_multi_to("let foo = 1\nlet y = foo + 1\n").as_deref(),
            Some("let bar = 1\nlet y = bar + 1\n")
        );
    }

    #[test]
    fn same_line_parts_apply_right_to_left() {
        // `f(a, b)` -> `g(a; b)`: three sites, one line, one atomic fix.
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 1, 2, "g"), (1, 4, 5, ";")]);
        assert_eq!(d.apply_multi_to("f(a, b)").as_deref(), Some("g(a; b)"));
    }

    #[test]
    fn unsorted_input_is_normalized() {
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(2, 1, 2, "B"), (1, 1, 2, "A")]);
        assert_eq!(d.apply_multi_to("x\ny\n").as_deref(), Some("A\nB\n"));
    }

    #[test]
    fn overlapping_parts_refuse_the_whole_fix() {
        for parts in [
            vec![(1, 1, 5, "x"), (1, 3, 7, "y")], // crossing
            vec![(1, 2, 4, "x"), (1, 2, 4, "y")], // identical spans
        ] {
            let d = Diagnostic::error("e", "h", "c").with_machine_fix_parts(parts);
            assert!(d.multi_fix.is_none());
            assert!(d.apply_multi_to("abcdefgh").is_none());
        }
        // Touching (end == next start) is NOT overlap.
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 1, 3, "X"), (1, 3, 5, "Y")]);
        assert_eq!(d.apply_multi_to("abcd").as_deref(), Some("XY"));
    }

    #[test]
    fn guessed_span_in_any_part_refuses_the_whole_fix() {
        for bad in [(0, 1, 2), (1, 0, 2), (1, 5, 3)] {
            let d = Diagnostic::error("e", "h", "c")
                .with_machine_fix_parts(vec![(1, 1, 2, "ok"), (bad.0, bad.1, bad.2, "boom")]);
            assert!(d.multi_fix.is_none(), "kept guessed span {:?}", bad);
        }
    }

    #[test]
    fn empty_parts_store_nothing() {
        assert!(Diagnostic::error("e", "h", "c").with_machine_fix_parts(vec![]).multi_fix.is_none());
    }

    #[test]
    fn out_of_bounds_application_is_atomic() {
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 1, 2, "A"), (9, 1, 2, "B")]);
        assert!(d.apply_multi_to("only\ntwo\n").is_none());
    }

    #[test]
    fn suggested_parts_are_never_machine_readable() {
        let d = Diagnostic::error("e", "h", "c")
            .with_suggested_fix_parts(vec![(1, 1, 2, "x")]);
        assert!(d.multi_fix.is_some());
        assert!(d.machine_multi_fix().is_none());
        let m = Diagnostic::error("e", "h", "c").with_machine_fix_parts(vec![(1, 1, 2, "x")]);
        assert!(m.machine_multi_fix().is_some());
    }

    #[test]
    fn insertion_and_eol_semantics_match_single_fix() {
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 1, 1, ">"), (1, 5, 5, "<")]);
        assert_eq!(d.apply_multi_to("abcd").as_deref(), Some(">abcd<"));
    }

    #[test]
    fn multi_fix_serializes_parts_with_applicability() {
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 1, 2, "g"), (1, 4, 5, ";")]);
        let j = crate::render::to_json(&d);
        assert!(j.contains(
            r#""suggestions":[{"parts":[{"line":1,"col":1,"end_col":2,"replacement":"g"},{"line":1,"col":4,"end_col":5,"replacement":";"}],"applicability":"machine-applicable"}]"#
        ), "unexpected JSON: {j}");
        // Single fix and multi fix coexist as separate suggestion entries.
        let both = Diagnostic::error("e", "h", "c")
            .with_machine_fix(1, 1, 2, "x")
            .with_machine_fix_parts(vec![(2, 1, 2, "y")]);
        let j = crate::render::to_json(&both);
        assert!(j.contains(r#""replacement":"x","applicability":"machine-applicable"},{"parts":"#), "unexpected JSON: {j}");
    }

    #[test]
    fn unicode_columns_are_char_indexed() {
        let d = Diagnostic::error("e", "h", "c")
            .with_machine_fix_parts(vec![(1, 5, 7, "世界"), (2, 1, 2, "y")]);
        assert_eq!(d.apply_multi_to("let 挨拶 = 1\nx = 2").as_deref(), Some("let 世界 = 1\ny = 2"));
    }
}
