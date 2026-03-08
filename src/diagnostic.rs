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
    pub fn error(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Error,
            message: message.into(),
            hint: hint.into(),
            context: context.into(),
        }
    }

    #[allow(dead_code)]
    pub fn warning(message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Self {
        Diagnostic {
            level: Level::Warning,
            message: message.into(),
            hint: hint.into(),
            context: context.into(),
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
