use super::Parser;
use crate::lexer::TokenType;

impl Parser {
    /// Diagnose invalid range/spread syntax without guessing an automatic rewrite.
    /// Call only after the enclosing grammar has excluded legal rest markers.
    pub(crate) fn reject_retired_range(&mut self, context: &str, hint: &str) -> Option<String> {
        if !matches!(self.current().token_type, TokenType::DotDot | TokenType::DotDotEq) {
            return None;
        }
        let message = format!("'{}' is not supported in {context}", self.current().value);
        let diagnostic = self.diag_error(&message, hint, context).with_code("E031");
        self.errors.push(diagnostic);
        Some(message)
    }
}
