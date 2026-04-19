/// Almide diagnostic: every error includes an actionable fix hint.

// ── "Did you mean?" suggestions ────────────────────────────────

/// Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1].to_ascii_lowercase() == b[j - 1].to_ascii_lowercase() { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Find the best "did you mean?" suggestion from a list of candidates.
/// Returns None if no candidate is close enough.
/// Threshold grows with name length: `max(3, name.len() / 2)` so that
/// longer names like `to_code_points` → `codepoint` (dist 5) still match.
pub fn suggest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let threshold = (name.len() / 2).max(3);
    let mut best: Option<(&str, usize)> = None;
    for c in candidates {
        let dist = levenshtein(name, c);
        if dist < threshold && dist > 0 {
            if best.map_or(true, |(_, d)| dist < d) {
                best = Some((c, dist));
            }
        }
    }
    best.map(|(s, _)| s.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Level {
    Error,
    Warning,
}

/// A secondary source location with a label (e.g. "declared as Int here").
#[derive(Debug, Clone)]
pub struct SecondarySpan {
    pub line: usize,
    pub col: Option<usize>,
    pub label: String,
}

// NOTE: This struct is part of the public API consumed by almide/playground.
// Do not rename fields or remove public methods without updating playground/crate/src/lib.rs.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub code: Option<&'static str>,
    pub message: String,
    pub hint: String,
    pub context: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub end_col: Option<usize>,
    pub secondary: Vec<SecondarySpan>,
    /// Optional copy-pasteable fix snippet (Elm-style `try:`). Shown after the
    /// hint so LLMs and humans can apply the fix without re-reading the docs.
    /// Multi-line; renderer prints each line with a `    ` indent under `try:`.
    pub try_snippet: Option<String>,
    /// Inline compact snippet of the offending source fragment (the
    /// `here:` row of the Here/Try/Hint three-part format). Single line,
    /// typically the trimmed source at the primary span. Rendered as
    /// `  here: <snippet>` above `hint:` / `try:` when present.
    ///
    /// Complements the gutter-formatted source rendered by
    /// `display_with_source` — useful for plain `display()` contexts
    /// like JSON output, CI logs, and IDE hover previews where the
    /// multi-line gutter form is unwanted.
    pub here_snippet: Option<String>,
    /// Exact source range that `try_snippet` is a drop-in replacement for.
    /// When set, `apply_try_to(source)` rewrites `source[line:col..line:end_col]`
    /// to `try_snippet`, and the result is expected to compile.
    ///
    /// Semantics:
    /// - `line` / `col` / `end_col` are 1-indexed char offsets to match
    ///   the primary-span convention on this struct.
    /// - `col` is inclusive, `end_col` is exclusive (consistent with
    ///   `Diagnostic::at_span` treating `end_col > col` as the exclusive
    ///   upper bound of the highlight range).
    /// - None means the `try_snippet` is display-only (may contain
    ///   human-readable placeholders or comments that don't compile).
    ///
    /// The Phase 3 target of `docs/roadmap/active/diagnostics-here-try-hint.md`
    /// is to populate this on every diagnostic that emits a
    /// mechanically-applicable fix — `tests/diagnostic_harness_test.rs`
    /// will auto-apply and verify against `fixed.almd`.
    pub try_replace_span: Option<(usize, usize, usize)>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Error, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
            try_snippet: None, here_snippet: None, try_replace_span: None,
        }
    }

    pub fn warning(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Warning, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
            try_snippet: None, here_snippet: None, try_replace_span: None,
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// Attach a copy-pasteable fix snippet.
    pub fn with_try(mut self, snippet: impl Into<String>) -> Self {
        self.try_snippet = Some(snippet.into());
        self
    }

    /// Attach a mechanically-applicable fix: `snippet` replaces the
    /// source range `[line:col..line:end_col]` verbatim (1-indexed,
    /// `end_col` exclusive, same convention as `at_span`).
    ///
    /// Sets both `try_snippet` (for display) and `try_replace_span`
    /// (for machine apply). When both are present, `apply_try_to`
    /// performs the substitution and the result is guaranteed by the
    /// diagnostic author to compile cleanly.
    pub fn with_try_replace(
        mut self,
        line: usize,
        col: usize,
        end_col: usize,
        snippet: impl Into<String>,
    ) -> Self {
        let s = snippet.into();
        self.try_replace_span = Some((line, col, end_col));
        self.try_snippet = Some(s);
        self
    }

    /// Apply `try_snippet` to `source` at `try_replace_span`, returning
    /// the rewritten source. `None` when either field is missing or the
    /// span can't be located (out-of-bounds line / col). Callers verify
    /// the result compiles — the diagnostic author's job is to emit a
    /// range whose replacement produces valid Almide code.
    pub fn apply_try_to(&self, source: &str) -> Option<String> {
        let snippet = self.try_snippet.as_ref()?;
        let (line, col, end_col) = self.try_replace_span?;
        if line == 0 || col == 0 || end_col < col { return None; }
        // Locate the byte range of `line` (1-indexed) within `source`.
        let mut line_start = 0usize;
        let mut cur_line = 1usize;
        for (i, b) in source.bytes().enumerate() {
            if cur_line == line { break; }
            if b == b'\n' {
                cur_line += 1;
                line_start = i + 1;
            }
        }
        if cur_line != line { return None; }
        let line_tail = &source[line_start..];
        let line_end = line_tail.find('\n').map(|i| line_start + i).unwrap_or(source.len());
        let line_slice = &source[line_start..line_end];
        // Byte offset of the `target`-th char within `line_slice`
        // (1-indexed). Accepts `target = char_count + 1` as the
        // exclusive end-of-line marker.
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
        if end_off < start_off || end_off > line_end { return None; }
        let mut out = String::with_capacity(source.len() + snippet.len());
        out.push_str(&source[..start_off]);
        out.push_str(snippet);
        out.push_str(&source[end_off..]);
        Some(out)
    }

    /// Attach an inline source snippet — the `here:` line of the
    /// Here/Try/Hint three-part format (roadmap: diagnostics-here-try-hint).
    /// Single line, trimmed. Multi-line input is collapsed to its
    /// first non-empty line so the inline label stays compact.
    pub fn with_here(mut self, snippet: impl Into<String>) -> Self {
        let s = snippet.into();
        let one = s
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
        self.here_snippet = Some(one);
        self
    }

    /// Add a secondary source location with a label.
    pub fn with_secondary(mut self, line: usize, col: Option<usize>, label: impl Into<String>) -> Self {
        self.secondary.push(SecondarySpan { line, col, label: label.into() });
        self
    }

    pub fn at(mut self, file: &str, line: usize) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(line);
        self
    }

    pub fn at_span(mut self, file: &str, span: crate::span::Span) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(span.line);
        self.col = Some(span.col);
        if span.end_col > span.col {
            self.end_col = Some(span.end_col);
        }
        self
    }

    /// Plain-text display (no color, no source annotation).
    /// NOTE: Called by almide/playground — do not rename without updating playground.
    pub fn display(&self) -> String {
        let prefix = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        let code_str = self.code.map(|c| format!("[{}]", c)).unwrap_or_default();
        let mut out = format!("{}{}: {}", prefix, code_str, self.message);
        match (&self.file, self.line) {
            (Some(f), Some(l)) => {
                let loc = match self.col {
                    Some(c) => format!("{}:{}:{}", f, l, c),
                    None => format!("{}:{}", f, l),
                };
                out.push_str(&format!("\n  --> {}", loc));
            }
            (Some(f), None) => out.push_str(&format!("\n  --> {}", f)),
            (None, Some(l)) => out.push_str(&format!("\n  at line {}", l)),
            _ => {}
        }
        if !self.context.is_empty() {
            out.push_str(&format!("\n  in {}", self.context));
        }
        if let Some(here) = &self.here_snippet {
            if !here.is_empty() {
                out.push_str(&format!("\n  here: {}", here));
            }
        }
        if !self.hint.is_empty() {
            out.push_str(&format!("\n  hint: {}", self.hint));
        }
        if let Some(snippet) = &self.try_snippet {
            out.push_str("\n  try:");
            for line in snippet.lines() {
                out.push_str(&format!("\n      {}", line));
            }
        }
        out
    }
}

#[cfg(test)]
mod apply_try_tests {
    use super::*;

    #[test]
    fn no_try_snippet_returns_none() {
        let d = Diagnostic::error("e", "h", "c");
        assert!(d.apply_try_to("abc").is_none());
    }

    #[test]
    fn no_replace_span_returns_none() {
        let d = Diagnostic::error("e", "h", "c").with_try("fix");
        assert!(d.apply_try_to("abc").is_none());
    }

    #[test]
    fn replaces_bang_with_not() {
        // source: "if !user_admin then x"
        //         123456789012345...
        //            ^            col 4 is `!`, col 5..15 is `user_admin`.
        // Replace just `!` (col 4..5) with `not `.
        let d = Diagnostic::error("e", "h", "c").with_try_replace(1, 4, 5, "not ");
        let out = d.apply_try_to("if !user_admin then x").unwrap();
        assert_eq!(out, "if not user_admin then x");
    }

    #[test]
    fn replace_whole_token_round_trip() {
        // Rename `parseInt` → `int.parse`. `parseInt` starts at col 7 and
        // ends at col 15 (exclusive) in "let x=parseInt(s)".
        let d = Diagnostic::error("e", "h", "c").with_try_replace(1, 7, 15, "int.parse");
        let out = d.apply_try_to("let x=parseInt(s)").unwrap();
        assert_eq!(out, "let x=int.parse(s)");
    }

    #[test]
    fn replace_on_second_line() {
        let src = "fn main() -> Int =\n    parseInt(s)\n";
        // Line 2: `    parseInt(s)`. `parseInt` at cols 5..13 exclusive.
        let d = Diagnostic::error("e", "h", "c").with_try_replace(2, 5, 13, "int.parse");
        let out = d.apply_try_to(src).unwrap();
        assert_eq!(out, "fn main() -> Int =\n    int.parse(s)\n");
    }

    #[test]
    fn replace_zero_width_inserts() {
        // `end_col == col` — insert `snippet` at that column without
        // deleting anything. Useful for "missing import" style fixes.
        let d = Diagnostic::error("e", "h", "c").with_try_replace(1, 1, 1, "import json\n");
        let out = d.apply_try_to("effect fn main() = ...").unwrap();
        assert_eq!(out, "import json\neffect fn main() = ...");
    }

    #[test]
    fn out_of_bounds_line_returns_none() {
        let d = Diagnostic::error("e", "h", "c").with_try_replace(5, 1, 2, "x");
        assert!(d.apply_try_to("only one line").is_none());
    }

    #[test]
    fn out_of_bounds_col_returns_none() {
        let d = Diagnostic::error("e", "h", "c").with_try_replace(1, 100, 110, "x");
        assert!(d.apply_try_to("short").is_none());
    }
}
