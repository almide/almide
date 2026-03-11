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

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub message: String,
    pub hint: String,
    pub context: String,
    pub file: Option<String>,
    pub line: Option<usize>,
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
        }
    }

    #[allow(dead_code)]
    pub fn at(mut self, file: &str, line: usize) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(line);
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
                if color {
                    out.push_str(&format!("\n  {}-->{} {}:{}", BLUE, RESET, f, l));
                } else {
                    out.push_str(&format!("\n  --> {}:{}", f, l));
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
        if let Some(line_num) = self.line {
            if let Some(source_line) = source.lines().nth(line_num.saturating_sub(1)) {
                let trimmed = source_line.trim_end();
                if !trimmed.is_empty() {
                    let width = format!("{}", line_num).len();
                    if color {
                        out.push_str(&format!("\n{}{:>width$}{} {}|{} {}",
                            BLUE, line_num, RESET, BLUE, RESET, trimmed, width = width));
                    } else {
                        out.push_str(&format!("\n{:>width$} | {}", line_num, trimmed, width = width));
                    }
                }
            }
        }
        out
    }
}
