use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if self.check(TokenType::Let) {
            return self.parse_let_stmt();
        }
        if self.check(TokenType::Var) {
            return self.parse_var_stmt();
        }
        if self.check(TokenType::Guard) {
            return self.parse_guard_stmt();
        }

        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_assign_stmt();
        }

        // xs[i] = value (index assignment)
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBracket)
        {
            if let Some(stmt) = self.try_parse_index_assign()? {
                return Ok(stmt);
            }
        }

        // obj.field = value (field assignment)
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Dot)
            && self.peek_at(2).map(|t| matches!(t.token_type, TokenType::Ident)).unwrap_or(false)
            && self.peek_at(3).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(4).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_field_assign_stmt();
        }

        let span = self.current_span();
        let expr = self.parse_expr()?;
        Ok(Stmt::Expr { expr, span: Some(span) })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Let)?;

        if self.check(TokenType::LBrace) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(TokenType::RBrace) {
                names.push(self.expect_ident()?);
                if self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            let fields = names.into_iter()
                .map(|n| FieldPattern { name: n, pattern: None })
                .collect();
            return Ok(Stmt::LetDestructure {
                pattern: Pattern::RecordPattern { name: String::new(), fields, rest: false },
                value, span: Some(span),
            });
        }

        if self.check(TokenType::LParen) {
            let pattern = self.parse_destructure_tuple()?;
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Stmt::LetDestructure { pattern, value, span: Some(span) });
        }

        // Detect `let mut` (Rust style) — hint to use `var` instead
        if self.check(TokenType::Ident) && self.current().value == "mut" {
            return Err(self.check_hint_or_err(Some(TokenType::Ident), super::hints::HintScope::Block,
                "'let mut' is not valid in Almide"));
        }

        // Allow `let _ = expr` to discard values
        let name = if self.check(TokenType::Underscore) {
            self.advance();
            "_".to_string()
        } else {
            self.expect_ident()?
        };
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Let { name, ty, value, span: Some(span) })
    }

    fn parse_var_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Var)?;
        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Var { name, ty, value, span: Some(span) })
    }

    fn parse_guard_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Guard)?;
        let cond = self.parse_expr()?;
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let else_ = self.parse_expr()?;
        Ok(Stmt::Guard { cond, else_, span: Some(span) })
    }

    fn parse_assign_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        let name = self.current().value.clone();
        self.advance();
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Assign { name, value, span: Some(span) })
    }

    /// Try to parse `xs[i] = value`. Returns None if not an assignment (e.g. just `xs[i]`).
    fn try_parse_index_assign(&mut self) -> Result<Option<Stmt>, String> {
        let saved = self.pos;
        let span = self.current_span();
        let target = self.current().value.clone();
        self.advance(); // skip ident
        self.expect(TokenType::LBracket)?;
        let index = self.parse_expr()?;
        self.expect(TokenType::RBracket)?;
        // Check if followed by `=` (but not `==`)
        if self.check(TokenType::Eq) && self.peek_at(1).map(|t| &t.token_type) != Some(&TokenType::Eq) {
            self.advance(); // skip =
            self.skip_newlines();
            let value = self.parse_expr()?;
            Ok(Some(Stmt::IndexAssign { target, index: Box::new(index), value, span: Some(span) }))
        } else {
            // Not an assignment — rewind and let parse_expr handle it
            self.pos = saved;
            Ok(None)
        }
    }

    fn parse_field_assign_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        let target = self.current().value.clone();
        self.advance(); // skip ident
        self.advance(); // skip .
        let field = self.expect_ident()?;
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::FieldAssign { target, field, value, span: Some(span) })
    }

    /// Parse a destructure pattern for tuples: `(a, b)` or `((a, b), c)`
    fn parse_destructure_tuple(&mut self) -> Result<Pattern, String> {
        self.expect(TokenType::LParen)?;
        let mut elements = Vec::new();
        while !self.check(TokenType::RParen) {
            if self.check(TokenType::LParen) {
                // Nested tuple
                elements.push(self.parse_destructure_tuple()?);
            } else {
                let name = self.expect_ident()?;
                elements.push(Pattern::Ident { name });
            }
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(TokenType::RParen)?;
        Ok(Pattern::Tuple { elements })
    }
}
