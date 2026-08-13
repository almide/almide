/// Hint integration and diagnostic helpers for parser error reporting.

use crate::lexer::TokenType;
use crate::diagnostic::Diagnostic;
use super::Parser;
use super::hints;

impl Parser {
    // ── Hint integration ──────────────────────────────────────────

    pub(crate) fn check_hint(&self, expected: Option<TokenType>, scope: hints::HintScope) -> Option<hints::HintResult> {
        let prev = if self.pos > 0 { Some(&self.tokens[self.pos - 1]) } else { None };
        let next = if self.pos + 1 < self.tokens.len() { Some(&self.tokens[self.pos + 1]) } else { None };
        let ctx = hints::HintContext {
            expected,
            got: self.current(),
            prev,
            next,
            scope,
        };
        hints::check_hint(&ctx)
    }

    pub(crate) fn check_hint_or_err(&self, expected: Option<TokenType>, scope: hints::HintScope, default_msg: &str) -> String {
        if let Some(result) = self.check_hint(expected, scope) {
            let tok = self.current();
            let msg = result.message.as_deref().unwrap_or(default_msg);
            format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint)
        } else {
            default_msg.to_string()
        }
    }

    // ── Diagnostic helpers ────────────────────────────────────────

    /// The unknown-character error (#1308): a character no lexer rule matched
    /// (full-width punctuation, invisible Unicode, ...). Pushes the rich
    /// diagnostic and returns the string form for the `Err` channel — callers
    /// that dedupe on `errors.len()` (entry.rs) will skip the string twin.
    pub(crate) fn unknown_char_error(&mut self, value: &str, line: usize, col: usize) -> String {
        let ch = value.chars().next().unwrap_or('\u{FFFD}');
        let msg = format!("Unexpected character '{}' (U+{:04X})", ch, ch as u32);
        let hint = "This character is not Almide syntax. Full-width or invisible Unicode \
                    characters often sneak in from copy-paste — delete it, or move it into \
                    a string or comment.";
        let diag = self.diag_error(msg.clone(), hint, "unknown-char");
        self.errors.push(diag);
        format!("{} at line {}:{}", msg, line, col)
    }

    pub(crate) fn diag_error(&self, message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Diagnostic {
        let mut d = Diagnostic::error(message, hint, context);
        let tok = self.current();
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = Some(tok.line);
        d.col = Some(tok.col);
        if tok.end_col > tok.col {
            d.end_col = Some(tok.end_col);
        }
        d
    }

    pub(crate) fn string_to_diagnostic(&self, msg: &str) -> Diagnostic {
        let (line, col) = if let Some(idx) = msg.find("at line ") {
            let rest = &msg[idx + 8..];
            let nums: Vec<&str> = rest.splitn(3, |c: char| !c.is_ascii_digit()).collect();
            let l = nums.first().and_then(|s| s.parse::<usize>().ok());
            let c = nums.get(1).and_then(|s| s.parse::<usize>().ok());
            (l, c)
        } else {
            (None, None)
        };
        let (message, hint) = if let Some(idx) = msg.find("\n  Hint: ") {
            (msg[..idx].to_string(), msg[idx + 9..].to_string())
        } else {
            (msg.to_string(), String::new())
        };
        let mut d = Diagnostic::error(message, hint, "");
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = line;
        d.col = col;
        d
    }
}
