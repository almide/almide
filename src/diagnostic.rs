/// Almide diagnostic: every error includes an actionable fix hint.

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
        let prefix = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        let mut out = format!("{}: {}", prefix, self.message);
        match (&self.file, self.line) {
            (Some(f), Some(l)) => out.push_str(&format!("\n  --> {}:{}", f, l)),
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

    pub fn display_with_source(&self, source: &str) -> String {
        let mut out = self.display();
        if let Some(line_num) = self.line {
            if let Some(source_line) = source.lines().nth(line_num.saturating_sub(1)) {
                let trimmed = source_line.trim_end();
                if !trimmed.is_empty() {
                    let width = format!("{}", line_num).len();
                    out.push_str(&format!("\n{:>width$} | {}", line_num, trimmed, width = width));
                }
            }
        }
        out
    }
}
