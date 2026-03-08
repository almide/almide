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
}

impl Diagnostic {
    pub fn error(message: String, hint: &str, context: &str) -> Self {
        Diagnostic {
            level: Level::Error,
            message,
            hint: hint.to_string(),
            context: context.to_string(),
        }
    }

    pub fn error_s(message: String, hint: String, context: String) -> Self {
        Diagnostic {
            level: Level::Error,
            message,
            hint,
            context,
        }
    }

    pub fn warning(message: String, hint: &str, context: &str) -> Self {
        Diagnostic {
            level: Level::Warning,
            message,
            hint: hint.to_string(),
            context: context.to_string(),
        }
    }

    pub fn display(&self) -> String {
        let prefix = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        let mut out = format!("{}: {}", prefix, self.message);
        if !self.context.is_empty() {
            out.push_str(&format!("\n  in {}", self.context));
        }
        if !self.hint.is_empty() {
            out.push_str(&format!("\n  hint: {}", self.hint));
        }
        out
    }
}
