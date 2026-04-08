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
/// Returns None if no candidate is close enough (threshold: distance < name.len()/3 + 1).
pub fn suggest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    let threshold = name.len() / 3 + 1;
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
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Error, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Warning, code: None,
            message: message.into(), hint: hint.into(), context: context.into(),
            file: None, line: None, col: None, end_col: None, secondary: Vec::new(),
        }
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
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
        if !self.hint.is_empty() {
            out.push_str(&format!("\n  hint: {}", self.hint));
        }
        out
    }
}
