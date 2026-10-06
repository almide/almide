//! `var (x, y) = p` / `var { a, b } = r` (#3149, ruling A): `var` takes the
//! patterns `let` takes — a tuple (nested, with `_`) or a record shorthand —
//! and every bound name is mutable. The parser keeps the written shape as a
//! `LetDestructure { mutable: true }` so fmt reprints it; the checker expands
//! it into one `var` per name (`crate::var_destructure`).

use crate::ast::*;
use crate::intern::sym;
use crate::lexer::TokenType;
use super::Parser;

impl Parser {
    /// Is the token after `var` the start of a pattern rather than a name?
    pub(crate) fn at_var_destructure(&self) -> bool {
        matches!(
            self.peek_at(1).map(|t| &t.token_type),
            Some(TokenType::LParen | TokenType::LBrace)
        )
    }

    /// Parse `var <pattern> = value` — the `let` destructuring grammar after
    /// `var`.
    pub(crate) fn parse_var_destructure(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Var)?;
        let pattern = if self.check(TokenType::LParen) {
            self.parse_destructure_tuple()?
        } else {
            self.parse_destructure_record()?
        };
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::LetDestructure { pattern, value, mutable: true, span: Some(span) })
    }

    /// `{ a, b }` after `let` / `var`: shorthand only — each name is both the
    /// field label and the local.
    pub(crate) fn parse_destructure_record(&mut self) -> Result<Pattern, String> {
        self.expect(TokenType::LBrace)?;
        let mut names = Vec::new();
        while !self.check(TokenType::RBrace) {
            names.push(self.expect_ident()?);
            if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
        }
        self.expect(TokenType::RBrace)?;
        let fields = names.into_iter().map(|n| FieldPattern { name: n, pattern: None }).collect();
        Ok(Pattern::RecordPattern { name: sym(""), fields, rest: false })
    }
}
