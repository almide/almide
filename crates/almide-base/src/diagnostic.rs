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
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Error, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
            try_snippet: None, here_snippet: None,
        }
    }

    pub fn warning(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Warning, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
            try_snippet: None, here_snippet: None,
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
