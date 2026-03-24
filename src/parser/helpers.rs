/// Token helpers: position tracking, lookahead, expect, advance, newline/comment skipping.

use crate::lexer::{Token, TokenType};
use crate::intern::{Sym, sym};
use crate::ast::{Span, Stmt};
use super::Parser;

impl Parser {
    pub(crate) fn current_span(&self) -> Span {
        let tok = self.current();
        Span { line: tok.line, col: tok.col }
    }

    pub(crate) fn current(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else if let Some(last) = self.tokens.last() {
            last
        } else {
            static EOF_TOKEN: Token = Token {
                token_type: TokenType::EOF,
                value: String::new(),
                line: 0,
                col: 0,
            };
            &EOF_TOKEN
        }
    }

    pub(crate) fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    pub(crate) fn newline_before_current(&self) -> bool {
        if self.pos == 0 { return false; }
        self.tokens[self.pos - 1].line < self.current().line
    }

    pub(crate) fn peek_type_args_call(&self) -> bool {
        if self.current().token_type != TokenType::LBracket { return false; }
        let mut depth = 0;
        let mut i = 0;
        loop {
            let tok = match self.peek_at(i) {
                Some(t) => t,
                None => return false,
            };
            match tok.token_type {
                TokenType::LBracket => depth += 1,
                TokenType::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return self.peek_at(i + 1).map(|t| t.token_type == TokenType::LParen).unwrap_or(false);
                    }
                }
                TokenType::EOF => return false,
                _ => {}
            }
            i += 1;
        }
    }

    /// Peek past optional newlines for `{ Ident :` pattern — named record.
    pub(crate) fn peek_named_record(&self) -> bool {
        let mut i = 0;
        while self.peek_at(i).map(|t| &t.token_type) == Some(&TokenType::Newline) { i += 1; }
        self.peek_at(i).map(|t| &t.token_type) == Some(&TokenType::LBrace)
            && {
                let mut j = i + 1;
                while self.peek_at(j).map(|t| &t.token_type) == Some(&TokenType::Newline) { j += 1; }
                // { ident: ... } or { ...spread }
                (self.peek_at(j).map(|t| &t.token_type) == Some(&TokenType::Ident)
                    && self.peek_at(j + 1).map(|t| &t.token_type) == Some(&TokenType::Colon))
                || self.peek_at(j).map(|t| &t.token_type) == Some(&TokenType::DotDotDot)
            }
    }

    pub(crate) fn peek_paren_lambda(&self) -> bool {
        if self.current().token_type != TokenType::LParen { return false; }
        let mut depth = 1;
        let mut i = 1;
        loop {
            let tok = match self.peek_at(i) {
                Some(t) => t,
                None => return false,
            };
            match tok.token_type {
                TokenType::LParen => depth += 1,
                TokenType::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.peek_at(i + 1)
                            .map(|t| t.token_type == TokenType::FatArrow)
                            .unwrap_or(false);
                    }
                }
                TokenType::EOF => return false,
                _ => {}
            }
            i += 1;
        }
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

    pub(crate) fn advance_and_get_sym(&mut self) -> Sym {
        let val = sym(&self.current().value);
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

    fn hint_for_expected(&self, expected: &TokenType, _got: &Token) -> String {
        if let Some(result) = self.check_hint(Some(expected.clone()), super::hints::HintScope::Expression) {
            result.hint
        } else {
            String::new()
        }
    }

    pub(crate) fn expect_ident(&mut self) -> Result<Sym, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_sym());
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

    pub(crate) fn expect_type_name(&mut self) -> Result<Sym, String> {
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_sym());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::Ident {
            "\n  Hint: Type names must start with an uppercase letter, e.g. Int, String, MyType"
        } else {
            ""
        };
        Err(format!(
            "Expected type name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_name(&mut self) -> Result<Sym, String> {
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) || self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_sym());
        }
        let tok = self.current();
        let hint = match &tok.token_type {
            TokenType::Int | TokenType::Float | TokenType::String => {
                "\n  Hint: Expected a name (identifier), not a literal value"
            }
            _ if tok.value == "=" || tok.value == ":" => {
                "\n  Hint: A name is required before '='. Example: fn my_func() -> Int = ..."
            }
            _ => "",
        };
        Err(format!(
            "Expected name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_fn_name(&mut self) -> Result<Sym, String> {
        // Convention method: fn Dog.eq(...) → name = "Dog.eq"
        if self.check(TokenType::TypeName)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Dot)
        {
            let type_name = self.advance_and_get_value();
            self.advance(); // skip .
            let method = if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
                self.advance_and_get_value()
            } else {
                let tok = self.current();
                return Err(format!("Expected method name after '{}.', got {:?} at line {}:{}", type_name, tok.token_type, tok.line, tok.col));
            };
            return Ok(sym(&format!("{}.{}", type_name, method)));
        }
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_sym());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::TypeName {
            "\n  Hint: Function names must start with a lowercase letter. Use camelCase, e.g. fn myFunc()"
        } else {
            ""
        };
        Err(format!(
            "Expected function name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_any_param_name(&mut self) -> Result<Sym, String> {
        if self.check(TokenType::Ident) || self.check(TokenType::Var) {
            return Ok(self.advance_and_get_sym());
        }
        let tok = self.current();
        let hint = if tok.token_type == TokenType::TypeName {
            "\n  Hint: Parameter names must start with a lowercase letter. Example: fn greet(name: String)"
        } else if tok.value == ")" {
            "\n  Hint: Trailing comma before ')' is not allowed"
        } else {
            ""
        };
        Err(format!(
            "Expected parameter name at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn expect_closing(&mut self, close: TokenType, open_line: usize, open_col: usize, context: &str) -> Result<&Token, String> {
        if self.check(close.clone()) { return Ok(self.advance()); }
        let (tok_line, tok_col) = (self.current().line, self.current().col);
        let (close_name, open_name) = match close {
            TokenType::RParen => ("')'", "'('"),
            TokenType::RBracket => ("']'", "'['"),
            TokenType::RBrace => ("'}'", "'{'"),
            _ => ("closing delimiter", "opening delimiter"),
        };
        let msg = format!("Expected {} to close {} opened at line {}:{}", close_name, context, open_line, open_col);
        let hint = format!("Add {} or check for a missing delimiter inside the {}", close_name, context);
        let mut diag = self.diag_error(&msg, &hint, "");
        diag.secondary.push(crate::diagnostic::SecondarySpan {
            line: open_line, col: Some(open_col),
            label: format!("{} opened here", open_name),
        });
        self.errors.push(diag);
        Err(format!("{} at line {}:{}", msg, tok_line, tok_col))
    }

    // ── Newline / comment skipping ────────────────────────────────

    pub(crate) fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            self.advance();
        }
    }

    /// Skip newlines only if the next non-newline token matches `tt`.
    /// This allows multiline continuation for operators like `|>`.
    pub(crate) fn skip_newlines_if_followed_by(&mut self, tt: TokenType) {
        let saved = self.pos;
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            self.advance();
        }
        if !self.check(tt) {
            self.pos = saved; // restore — the newlines are significant
        }
    }

    /// Skip newlines if the next non-newline token matches any of the given types.
    /// Enables multiline expression continuation when a line starts with a binary operator.
    pub(crate) fn skip_newlines_if_followed_by_any(&mut self, tts: &[TokenType]) {
        let saved = self.pos;
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            self.advance();
        }
        if !tts.iter().any(|tt| self.check(tt.clone())) {
            self.pos = saved;
        }
    }

    pub(crate) fn skip_newlines_into_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        while self.check(TokenType::Newline) || self.check(TokenType::Comment) {
            if self.check(TokenType::Comment) {
                stmts.push(Stmt::Comment { text: self.current().value.clone() });
            }
            self.advance();
        }
    }

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
