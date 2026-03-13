use crate::lexer::{Token, TokenType};
use crate::ast::Span;
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
            // Static EOF token as fallback — lexer always adds EOF, so this is unreachable
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

    /// Returns true if the current token is on a different line than the previous token.
    pub(crate) fn newline_before_current(&self) -> bool {
        if self.pos == 0 {
            return false;
        }
        let prev = &self.tokens[self.pos - 1];
        let curr = self.current();
        curr.line > prev.line
    }

    /// Look ahead to check if `[...]` is followed by `(` — indicating type args before a call.
    pub(crate) fn peek_type_args_call(&self) -> bool {
        // Current token should be `[`
        if self.current().token_type != TokenType::LBracket {
            return false;
        }
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
                        // Check if next token after `]` is `(`
                        return self.peek_at(i + 1).map(|t| t.token_type == TokenType::LParen).unwrap_or(false);
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

    pub(crate) fn expect_any_fn_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
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

    pub(crate) fn expect_any_param_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::Var) {
            return Ok(self.advance_and_get_value());
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

    /// Expect a closing delimiter, providing the opening delimiter's position for better error messages.
    /// On failure, produces a Diagnostic with a secondary span pointing to where the delimiter was opened.
    pub(crate) fn expect_closing(&mut self, close: TokenType, open_line: usize, open_col: usize, context: &str) -> Result<&Token, String> {
        if self.check(close.clone()) {
            return Ok(self.advance());
        }
        let tok_line = self.current().line;
        let tok_col = self.current().col;
        let close_name = match close {
            TokenType::RParen => "')'",
            TokenType::RBracket => "']'",
            TokenType::RBrace => "'}'",
            _ => "closing delimiter",
        };
        let open_name = match close {
            TokenType::RParen => "'('",
            TokenType::RBracket => "'['",
            TokenType::RBrace => "'{'",
            _ => "opening delimiter",
        };
        let msg = format!(
            "Expected {} to close {} opened at line {}:{}",
            close_name, context, open_line, open_col
        );
        let hint = format!(
            "Add {} or check for a missing delimiter inside the {}",
            close_name, context
        );
        let mut diag = self.diag_error(&msg, &hint, "");
        diag.secondary.push(crate::diagnostic::SecondarySpan {
            line: open_line,
            col: Some(open_col),
            label: format!("{} opened here", open_name),
        });
        // Return as Err(String) for compatibility with existing error propagation,
        // but also push the rich diagnostic so it can be displayed with source context.
        self.errors.push(diag);
        Err(format!(
            "{} at line {}:{}",
            msg, tok_line, tok_col
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
