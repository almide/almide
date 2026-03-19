/// Parser methods for brace expressions (records, blocks, maps) and list expressions.
use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_brace_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut initial_comments = Vec::new();
        self.skip_newlines_into_stmts(&mut initial_comments);
        if self.check(TokenType::RBrace) {
            self.advance();
            return Ok(Expr::Block {
                stmts: Vec::new(), expr: None,
                id: self.next_id(), span, resolved_type: None,
            });
        }
        // Spread record: { ...base, field: value }
        if self.check(TokenType::DotDotDot) {
            return self.parse_spread_record(span, open);
        }
        // Record literal: { field: value, ... }
        if (self.check(TokenType::Ident) || self.check(TokenType::IdentQ))
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
        {
            return self.parse_record_literal(span, open);
        }
        // Block expression
        self.parse_block_body(initial_comments, span, open)
    }

    fn parse_spread_record(&mut self, span: Option<Span>, open: crate::lexer::Token) -> Result<Expr, String> {
        self.advance(); // skip ...
        let base = self.parse_expr()?;
        let mut fields = Vec::new();
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RBrace) { break; }
            let field_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            self.skip_newlines();
            let field_value = self.parse_expr()?;
            fields.push(FieldInit { name: field_name, value: field_value });
        }
        self.skip_newlines();
        self.expect_closing(TokenType::RBrace, open.line, open.col, "spread record")?;
        Ok(Expr::SpreadRecord {
            base: Box::new(base), fields,
            id: self.next_id(), span, resolved_type: None,
        })
    }

    fn parse_record_literal(&mut self, span: Option<Span>, open: crate::lexer::Token) -> Result<Expr, String> {
        let mut fields = Vec::new();
        while !self.check(TokenType::RBrace) {
            self.skip_newlines();
            let field_name = self.expect_any_name()?;
            if self.check(TokenType::Colon) {
                self.advance();
                self.skip_newlines();
                let field_value = self.parse_expr()?;
                fields.push(FieldInit { name: field_name, value: field_value });
            } else {
                fields.push(FieldInit {
                    name: field_name.clone(),
                    value: Expr::Ident { name: field_name, id: self.next_id(), span: None, resolved_type: None },
                });
            }
            self.skip_newlines();
            if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "record literal")?;
        Ok(Expr::Record { name: None, fields, id: self.next_id(), span, resolved_type: None })
    }

    fn parse_block_body(&mut self, initial_comments: Vec<Stmt>, span: Option<Span>, open: crate::lexer::Token) -> Result<Expr, String> {
        let mut stmts = initial_comments;
        let mut final_expr: Option<Box<Expr>> = None;
        while !self.check(TokenType::RBrace) && !self.check(TokenType::EOF) {
            match self.parse_stmt() {
                Ok(stmt) => {
                    let mut trailing = Vec::new();
                    self.skip_newlines_into_stmts(&mut trailing);
                    if self.check(TokenType::Semicolon) {
                        self.advance();
                        self.skip_newlines_into_stmts(&mut trailing);
                    }
                    if self.check(TokenType::RBrace) {
                        if let Stmt::Expr { expr, .. } = stmt {
                            final_expr = Some(Box::new(expr));
                        } else {
                            stmts.push(stmt);
                        }
                        stmts.extend(trailing);
                    } else {
                        stmts.push(stmt);
                        stmts.extend(trailing);
                    }
                }
                Err(msg) => {
                    let err_span = Some(self.current_span());
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.recover_to_sync_point(true);
                    stmts.push(Stmt::Error { span: err_span });
                }
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "block")?;
        Ok(Expr::Block {
            stmts, expr: final_expr,
            id: self.next_id(), span, resolved_type: None,
        })
    }

    pub(crate) fn parse_braceless_block(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        loop {
            match self.parse_stmt() {
                Ok(stmt) => {
                    self.skip_newlines();
                    if self.check(TokenType::Semicolon) {
                        self.advance();
                        self.skip_newlines();
                    }
                    if self.is_at_braceless_block_end() {
                        if let Stmt::Expr { expr, .. } = stmt {
                            final_expr = Some(Box::new(expr));
                        } else {
                            stmts.push(stmt);
                        }
                        break;
                    } else {
                        stmts.push(stmt);
                    }
                }
                Err(msg) => {
                    let err_span = Some(self.current_span());
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.recover_to_sync_point(false);
                    stmts.push(Stmt::Error { span: err_span });
                    if self.is_at_braceless_block_end() {
                        break;
                    }
                }
            }
        }

        Ok(Expr::Block {
            stmts, expr: final_expr,
            id: self.next_id(), span, resolved_type: None,
        })
    }

    fn is_at_braceless_block_end(&self) -> bool {
        matches!(self.current().token_type,
            TokenType::EOF
            | TokenType::Fn | TokenType::Effect
            | TokenType::Pub | TokenType::Local | TokenType::Mod
            | TokenType::Type | TokenType::Trait | TokenType::Impl
            | TokenType::Test | TokenType::Strict | TokenType::At
            | TokenType::RBrace
        )
    }

    pub(crate) fn parse_list_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBracket)?;
        self.skip_newlines();

        if self.check(TokenType::RBracket) {
            self.advance();
            return Ok(Expr::List { elements: vec![], id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Colon) {
            self.advance();
            self.expect_closing(TokenType::RBracket, open.line, open.col, "empty map")?;
            return Ok(Expr::EmptyMap { id: self.next_id(), span, resolved_type: None });
        }

        let first = self.parse_expr()?;
        self.skip_newlines();

        if self.check(TokenType::Colon) {
            return self.parse_map_literal(first, span, open);
        }

        // List literal
        let mut elements = vec![first];
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RBracket) { break; }
            elements.push(self.parse_expr()?);
            self.skip_newlines();
        }
        if !self.check(TokenType::RBracket) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::ListLiteral) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in list");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        self.expect_closing(TokenType::RBracket, open.line, open.col, "list literal")?;
        Ok(Expr::List { elements, id: self.next_id(), span, resolved_type: None })
    }

    fn parse_map_literal(&mut self, first_key: Expr, span: Option<Span>, open: crate::lexer::Token) -> Result<Expr, String> {
        self.advance(); // skip :
        self.skip_newlines();
        let first_value = self.parse_expr()?;
        self.skip_newlines();
        let mut entries = vec![(first_key, first_value)];
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RBracket) { break; }
            let key = self.parse_expr()?;
            self.skip_newlines();
            self.expect(TokenType::Colon)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            self.skip_newlines();
            entries.push((key, value));
        }
        if !self.check(TokenType::RBracket) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::MapLiteral) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in map");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        self.expect_closing(TokenType::RBracket, open.line, open.col, "map literal")?;
        Ok(Expr::MapLiteral { entries, id: self.next_id(), span, resolved_type: None })
    }
}
