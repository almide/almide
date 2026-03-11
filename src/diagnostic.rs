/// Almide diagnostic: every error includes an actionable fix hint.

use std::sync::atomic::{AtomicBool, Ordering};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);

/// Call once at startup to enable colors if stderr is a TTY.
pub fn init_color() {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        COLOR_ENABLED.store(true, Ordering::Relaxed);
    }
}

fn use_color() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

// ANSI codes
const RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[1;33m";
const CYAN: &str = "\x1b[1;36m";
const BLUE: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

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

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
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
            level: Level::Error,
            message: message.into(),
            hint: hint.into(),
            context: context.into(),
            file: None,
            line: None,
            col: None,
            end_col: None,
            secondary: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Warning,
            message: message.into(),
            hint: hint.into(),
            context: context.into(),
            file: None,
            line: None,
            col: None,
            end_col: None,
            secondary: Vec::new(),
        }
    }

    /// Add a secondary source location with a label.
    #[allow(dead_code)]
    pub fn with_secondary(mut self, line: usize, col: Option<usize>, label: impl Into<String>) -> Self {
        self.secondary.push(SecondarySpan { line, col, label: label.into() });
        self
    }

    #[allow(dead_code)]
    pub fn at(mut self, file: &str, line: usize) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(line);
        self
    }

    #[allow(dead_code)]
    pub fn at_span(mut self, file: &str, span: crate::ast::Span) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(span.line);
        self.col = Some(span.col);
        self
    }

    pub fn display(&self) -> String {
        let color = use_color();
        let (prefix_color, prefix) = match self.level {
            Level::Error => (RED, "error"),
            Level::Warning => (YELLOW, "warning"),
        };
        let mut out = if color {
            format!("{}{}{}: {}{}{}", prefix_color, prefix, RESET, BOLD, self.message, RESET)
        } else {
            format!("{}: {}", prefix, self.message)
        };
        match (&self.file, self.line) {
            (Some(f), Some(l)) => {
                let loc = if let Some(c) = self.col {
                    format!("{}:{}:{}", f, l, c)
                } else {
                    format!("{}:{}", f, l)
                };
                if color {
                    out.push_str(&format!("\n  {}-->{} {}", BLUE, RESET, loc));
                } else {
                    out.push_str(&format!("\n  --> {}", loc));
                }
            }
            (Some(f), None) => {
                if color {
                    out.push_str(&format!("\n  {}-->{} {}", BLUE, RESET, f));
                } else {
                    out.push_str(&format!("\n  --> {}", f));
                }
            }
            (None, Some(l)) => out.push_str(&format!("\n  at line {}", l)),
            _ => {}
        }
        if !self.context.is_empty() {
            if color {
                out.push_str(&format!("\n  {}in {}{}", DIM, self.context, RESET));
            } else {
                out.push_str(&format!("\n  in {}", self.context));
            }
        }
        if !self.hint.is_empty() {
            if color {
                out.push_str(&format!("\n  {}hint:{} {}", CYAN, RESET, self.hint));
            } else {
                out.push_str(&format!("\n  hint: {}", self.hint));
            }
        }
        out
    }

    pub fn display_with_source(&self, source: &str) -> String {
        let color = use_color();
        let mut out = self.display();
        let source_lines: Vec<&str> = source.lines().collect();

        // Render secondary spans first (declaration sites, etc.)
        for sec in &self.secondary {
            if let Some(src_line) = source_lines.get(sec.line.saturating_sub(1)) {
                let trimmed = src_line.trim_end();
                if !trimmed.is_empty() {
                    let max_line = self.line.unwrap_or(sec.line).max(sec.line);
                    let width = format!("{}", max_line).len();
                    let gutter_pad = " ".repeat(width);
                    if color {
                        out.push_str(&format!("\n{}{} {}|{}", gutter_pad, BLUE, RESET, BLUE));
                        out.push_str(&format!("\n{}{:>width$}{} {}|{} {}",
                            BLUE, sec.line, RESET, BLUE, RESET, trimmed, width = width));
                    } else {
                        out.push_str(&format!("\n{} |", gutter_pad));
                        out.push_str(&format!("\n{:>width$} | {}", sec.line, trimmed, width = width));
                    }
                    // Dash underline with label for secondary
                    if let Some(col) = sec.col {
                        let col0 = col.saturating_sub(1);
                        let dash_len = if !sec.label.is_empty() { sec.label.len().max(1) } else { 1 };
                        let pad = " ".repeat(col0);
                        let dashes = "-".repeat(dash_len);
                        if color {
                            out.push_str(&format!("\n{}{} {}|{} {}{}{}{}{}",
                                gutter_pad, BLUE, RESET, BLUE, RESET,
                                pad, CYAN, dashes, RESET));
                            if !sec.label.is_empty() {
                                out.push_str(&format!(" {}{}{}", CYAN, sec.label, RESET));
                            }
                        } else {
                            out.push_str(&format!("\n{} | {}{}", gutter_pad, pad, dashes));
                            if !sec.label.is_empty() {
                                out.push_str(&format!(" {}", sec.label));
                            }
                        }
                    }
                }
            }
        }

        // Render primary span
        if let Some(line_num) = self.line {
            if let Some(source_line) = source_lines.get(line_num.saturating_sub(1)) {
                let trimmed = source_line.trim_end();
                if !trimmed.is_empty() {
                    let width = format!("{}", line_num).len();
                    let gutter_pad = " ".repeat(width);
                    // Separator between secondary and primary if they exist
                    if self.secondary.is_empty() || self.secondary.iter().all(|s| s.line == line_num) {
                        if color {
                            out.push_str(&format!("\n{}{} {}|{}", gutter_pad, BLUE, RESET, BLUE));
                        } else {
                            out.push_str(&format!("\n{} |", gutter_pad));
                        }
                    } else {
                        // Ellipsis between distant spans
                        if color {
                            out.push_str(&format!("\n{}{}...{}", BLUE, " ".repeat(width.saturating_sub(2)), RESET));
                        } else {
                            out.push_str(&format!("\n{}...", " ".repeat(width.saturating_sub(2))));
                        }
                    }
                    if color {
                        out.push_str(&format!("\n{}{:>width$}{} {}|{} {}",
                            BLUE, line_num, RESET, BLUE, RESET, trimmed, width = width));
                    } else {
                        out.push_str(&format!("\n{:>width$} | {}", line_num, trimmed, width = width));
                    }
                    // Caret underline
                    if let Some(col) = self.col {
                        let col0 = col.saturating_sub(1);
                        let caret_len = if let Some(end_col) = self.end_col {
                            let end0 = end_col.saturating_sub(1);
                            if end0 > col0 { end0 - col0 } else { 1 }
                        } else {
                            if !self.context.is_empty() { self.context.len().max(1) } else { 1 }
                        };
                        let pad = " ".repeat(col0);
                        let carets = "^".repeat(caret_len);
                        let (caret_color, caret_reset) = if color {
                            match self.level {
                                Level::Error => (RED, RESET),
                                Level::Warning => (YELLOW, RESET),
                            }
                        } else {
                            ("", "")
                        };
                        if color {
                            out.push_str(&format!("\n{}{} {}|{} {}{}{}{}",
                                gutter_pad, BLUE, RESET, BLUE, RESET,
                                pad, caret_color, carets));
                            out.push_str(caret_reset);
                        } else {
                            out.push_str(&format!("\n{} | {}{}", gutter_pad, pad, carets));
                        }
                    }
                }
            }
        }
        out
    }
}
