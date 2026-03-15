use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();
        let span = Some(Span { line: tok.line, col: tok.col });

        if self.check(TokenType::Int) {
            self.advance();
            return Ok(Expr::Int {
                value: serde_json::Value::Number(
                    tok.value.parse::<i64>()
                        .ok()
                        .and_then(|n| serde_json::Number::from_f64(n as f64))
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
                raw: tok.value.clone(),
                id: self.next_id(), span, resolved_type: None,
            });
        }
        if self.check(TokenType::Float) {
            self.advance();
            let v: f64 = tok.value.parse().unwrap_or(0.0);
            return Ok(Expr::Float { value: v, id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::String) {
            self.advance();
            return Ok(Expr::String { value: tok.value.clone(), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::InterpolatedString) {
            self.advance();
            return Ok(Expr::InterpolatedString { value: tok.value.clone(), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Expr::Bool { value: true, id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Expr::Bool { value: false, id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Expr::Hole { id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Break) {
            self.advance();
            return Ok(Expr::Break { id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Continue) {
            self.advance();
            return Ok(Expr::Continue { id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Expr::None { id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Some) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "some()")?;
            return Ok(Expr::Some { expr: Box::new(expr), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Ok) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "ok()")?;
            return Ok(Expr::Ok { expr: Box::new(expr), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Err) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "err()")?;
            return Ok(Expr::Err { expr: Box::new(expr), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Todo) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let msg = self.current().value.clone();
            self.expect(TokenType::String)?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "todo()")?;
            return Ok(Expr::Todo { message: msg, id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Try) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Try { expr: Box::new(expr), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::Await) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Await { expr: Box::new(expr), id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::If) {
            return self.parse_if_expr();
        }
        if self.check(TokenType::Match) {
            return self.parse_match_expr();
        }

        if self.check(TokenType::While) {
            return self.parse_while_expr();
        }
        if self.check(TokenType::For) {
            return self.parse_for_expr();
        }
        if self.check(TokenType::Do) {
            self.advance();
            return self.parse_do_block();
        }
        if self.check(TokenType::LBrace) {
            return self.parse_brace_expr();
        }
        if self.check(TokenType::LBracket) {
            return self.parse_list_expr();
        }
        if self.check(TokenType::LParen) {
            return self.parse_paren_expr();
        }
        if self.check(TokenType::TypeName) {
            return self.parse_type_name_expr();
        }
        // Check hint system for rejected operators/keywords
        if let Some(result) = self.check_hint(None, super::hints::HintScope::Expression) {
            let msg = result.message.unwrap_or_else(|| format!("'{}' is not valid here", tok.value));
            return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
        }
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
            let name = tok.value.clone();
            self.advance();
            return Ok(Expr::Ident { name, id: self.next_id(), span, resolved_type: None });
        }

        Err(format!(
            "Expected expression at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn parse_paren_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        if self.peek_paren_lambda() {
            return self.parse_paren_lambda();
        }
        let open = self.current().clone();
        self.advance();
        if self.check(TokenType::RParen) {
            self.advance();
            return Ok(Expr::Unit { id: self.next_id(), span, resolved_type: None });
        }
        let first = self.parse_expr()?;
        if self.check(TokenType::Comma) {
            let mut elements = vec![first];
            while self.check(TokenType::Comma) {
                self.advance();
                if self.check(TokenType::RParen) { break; }
                elements.push(self.parse_expr()?);
            }
            self.expect_closing(TokenType::RParen, open.line, open.col, "tuple")?;
            return Ok(Expr::Tuple { elements, id: self.next_id(), span, resolved_type: None });
        }
        self.expect_closing(TokenType::RParen, open.line, open.col, "parenthesized expression")?;
        Ok(Expr::Paren { expr: Box::new(first), id: self.next_id(), span, resolved_type: None })
    }

    fn parse_type_name_expr(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();
        let span = Some(Span { line: tok.line, col: tok.col });
        let name = tok.value.clone();
        self.advance();

        if self.check(TokenType::LBracket) {
            let ta = self.parse_type_args()?;
            if self.check(TokenType::LParen) {
                let open_call = self.current().clone();
                self.advance();
                let (args, named_args) = self.parse_call_args()?;
                self.expect_closing(TokenType::RParen, open_call.line, open_call.col, "constructor call")?;
                return Ok(Expr::Call {
                    callee: Box::new(Expr::TypeName { name, id: self.next_id(), span, resolved_type: None }),
                    args, named_args, type_args: Some(ta),
                    id: self.next_id(), span, resolved_type: None,
                });
            }
            return Ok(Expr::TypeName { name, id: self.next_id(), span, resolved_type: None });
        }
        if self.check(TokenType::LParen) {
            let open_call = self.current().clone();
            self.advance();
            let (args, named_args) = self.parse_call_args()?;
            self.expect_closing(TokenType::RParen, open_call.line, open_call.col, "constructor call")?;
            return Ok(Expr::Call {
                callee: Box::new(Expr::TypeName { name, id: self.next_id(), span, resolved_type: None }),
                args, named_args, type_args: None,
                id: self.next_id(), span, resolved_type: None,
            });
        }
        // Named record: Foo {x: 1, y: 2} or Foo {\n  x: 1, ...}
        // Peek past optional newlines to check for { Ident : pattern
        if self.peek_named_record() {
            self.skip_newlines();
            let open_rec = self.current().clone();
            self.advance();
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
            self.expect_closing(TokenType::RBrace, open_rec.line, open_rec.col, "record construction")?;
            return Ok(Expr::Record { name: Some(name), fields, id: self.next_id(), span, resolved_type: None });
        }
        Ok(Expr::TypeName { name, id: self.next_id(), span, resolved_type: None })
    }

    fn parse_while_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // skip 'while'
        self.skip_newlines();
        let cond = self.parse_expr()?;
        self.skip_newlines();
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines_into_stmts(&mut stmts);
        while !self.check(TokenType::RBrace) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines_into_stmts(&mut stmts);
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines_into_stmts(&mut stmts);
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "while body")?;
        Ok(Expr::While {
            cond: Box::new(cond), body: stmts,
            id: self.next_id(), span, resolved_type: None,
        })
    }

    fn parse_for_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // skip 'for'
        let (var_name, var_tuple) = if self.check(TokenType::LParen) {
            self.advance();
            let mut names = vec![self.expect_ident()?];
            while self.check(TokenType::Comma) {
                self.advance();
                names.push(self.expect_ident()?);
            }
            self.expect(TokenType::RParen)?;
            (names[0].clone(), Some(names))
        } else if self.check(TokenType::Underscore) {
            self.advance();
            ("_".to_string(), None)
        } else {
            (self.expect_ident()?, None)
        };
        self.expect(TokenType::In)?;
        let iterable = self.parse_expr()?;
        let open_for = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines_into_stmts(&mut stmts);
        while !self.check(TokenType::RBrace) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines_into_stmts(&mut stmts);
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines_into_stmts(&mut stmts);
            }
        }
        self.expect_closing(TokenType::RBrace, open_for.line, open_for.col, "for body")?;
        Ok(Expr::ForIn {
            var: var_name, var_tuple, iterable: Box::new(iterable), body: stmts,
            id: self.next_id(), span, resolved_type: None,
        })
    }
}
