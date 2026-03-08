use crate::lexer::{Token, TokenType};
use super::Parser;

impl Parser {
    pub(crate) fn current(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else {
            self.tokens.last().unwrap_or_else(|| {
                panic!("Parser: no tokens available")
            })
        }
    }

    pub(crate) fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    pub(crate) fn check(&self, token_type: TokenType) -> bool {
        self.current().token_type == token_type
    }

    pub(crate) fn check_ident(&self, name: &str) -> bool {
        self.current().token_type == TokenType::Ident && self.current().value == name
    }

    pub(crate) fn advance(&mut self) -> &Token {
        let pos = self.pos;
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        &self.tokens[pos]
    }

    pub(crate) fn advance_and_get_value(&mut self) -> String {
        let val = self.current().value.clone();
        self.advance();
        val
    }

    pub(crate) fn expect(&mut self, token_type: TokenType) -> Result<&Token, String> {
        if !self.check(token_type.clone()) {
            let tok = self.current();
            let hint = self.hint_for_expected(&token_type, tok);
            let mut msg = format!(
                "Expected {:?} at line {}:{} (got {:?} '{}')",
                token_type, tok.line, tok.col, tok.token_type, tok.value
            );
            if !hint.is_empty() {
                msg.push_str(&format!("\n  Hint: {}", hint));
            }
            return Err(msg);
        }
        Ok(self.advance())
    }

    fn hint_for_expected(&self, expected: &TokenType, got: &Token) -> String {
        match (expected, &got.token_type, got.value.as_str()) {
            (TokenType::Else, _, _) => {
                "if expressions MUST have an else branch. Use 'guard ... else' for early returns instead.".into()
            }
            (TokenType::RParen, TokenType::LAngle, _) => {
                "Use [] for generics, not <>. Example: List[String], Result[T, E]".into()
            }
            (TokenType::Then, _, _) => {
                "if requires 'then'. Write: if condition then expr else expr".into()
            }
            _ => String::new(),
        }
    }

    pub(crate) fn expect_ident(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = match (&tok.token_type, tok.value.as_str()) {
            (TokenType::Underscore, _) => "\n  Hint: '_' can only be used in match patterns, not as a variable name.",
            (TokenType::Test, _) => "\n  Hint: 'test' is a reserved keyword.",
            _ => "",
        };
        Err(format!(
            "Expected identifier at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_type_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected type name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    pub(crate) fn expect_any_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    pub(crate) fn expect_any_fn_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected function name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    pub(crate) fn expect_any_param_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::Var) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected parameter name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    pub(crate) fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            self.advance();
        }
    }

    /// Skip newlines, collecting Comment tokens as Stmt::Comment into a Vec.
    pub(crate) fn skip_newlines_into_stmts(&mut self, stmts: &mut Vec<crate::ast::Stmt>) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            if self.check(TokenType::Comment) {
                stmts.push(crate::ast::Stmt::Comment { text: self.current().value.clone() });
            }
            self.advance();
        }
    }

    /// Skip newlines and collect any Comment tokens encountered.
    pub(crate) fn skip_newlines_collect_comments(&mut self) -> Vec<String> {
        let mut comments = Vec::new();
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            if self.check(TokenType::Comment) {
                comments.push(self.current().value.clone());
            }
            self.advance();
        }
        comments
    }
}
