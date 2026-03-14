use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_if_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::If)?;
        self.skip_newlines();
        let cond = self.parse_expr()?;
        self.skip_newlines();
        self.expect(TokenType::Then)?;
        self.skip_newlines();
        let then = self.parse_if_branch()?;
        self.skip_newlines();
        let else_ = if self.check(TokenType::Else) {
            self.advance();
            self.skip_newlines();
            self.parse_if_branch()?
        } else {
            Expr::Unit { span: span.clone(), resolved_type: None }
        };
        Ok(Expr::If {
            cond: Box::new(cond),
            then: Box::new(then),
            else_: Box::new(else_),
            span,
            resolved_type: None,
        })
    }

    fn parse_if_branch(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Ident) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq) {
            let span = Some(self.current_span());
            let name = self.advance_and_get_value();
            self.advance();
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Expr::Block {
                stmts: vec![Stmt::Assign { name, value, span: None }],
                expr: None,
                span,
                resolved_type: None,
            });
        }
        self.parse_expr()
    }

    pub(crate) fn parse_match_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::Match)?;
        self.skip_newlines();
        let subject = self.parse_or()?;
        self.skip_newlines();
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut leading = self.skip_newlines_collect_comments();
        let mut arms = Vec::new();
        while !self.check(TokenType::RBrace) {
            let mut arm = self.parse_match_arm()?;
            arm.comments = std::mem::take(&mut leading);
            arms.push(arm);
            leading = self.skip_newlines_collect_comments();
            if self.check(TokenType::Comma) {
                self.advance();
                let more = self.skip_newlines_collect_comments();
                leading.extend(more);
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "match block")?;
        Ok(Expr::Match {
            subject: Box::new(subject),
            arms,
            span,
            resolved_type: None,
        })
    }

    pub(crate) fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        let pattern = self.parse_pattern()?;
        let mut guard: Option<Expr> = None;
        if self.check(TokenType::If) {
            self.advance();
            guard = Some(self.parse_expr()?);
        }
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(MatchArm { pattern, guard, body, comments: Vec::new() })
    }

    pub(crate) fn parse_lambda(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::Fn)?;
        let open = self.current().clone();
        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.parse_lambda_param()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_lambda_param()?);
            }
        }
        self.expect_closing(TokenType::RParen, open.line, open.col, "lambda parameters")?;
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(Expr::Lambda {
            params,
            body: Box::new(body),
            span,
            resolved_type: None,
        })
    }

    fn parse_lambda_param(&mut self) -> Result<LambdaParam, String> {
        if self.check(TokenType::LParen) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(TokenType::RParen) {
                names.push(self.expect_ident()?);
                if self.check(TokenType::Comma) {
                    self.advance();
                }
            }
            self.expect(TokenType::RParen)?;
            let first = names.first().cloned().unwrap_or_default();
            return Ok(LambdaParam { name: first, tuple_names: Some(names), ty: None });
        }
        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        Ok(LambdaParam { name, tuple_names: None, ty })
    }

    pub(crate) fn parse_do_block(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines_into_stmts(&mut stmts);
        let mut final_expr: Option<Box<Expr>> = None;
        while !self.check(TokenType::RBrace) {
            let stmt = self.parse_stmt()?;
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
        self.expect_closing(TokenType::RBrace, open.line, open.col, "do block")?;
        Ok(Expr::DoBlock {
            stmts,
            expr: final_expr,
            span,
            resolved_type: None,
        })
    }

    pub(crate) fn parse_brace_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut initial_comments = Vec::new();
        self.skip_newlines_into_stmts(&mut initial_comments);
        if self.check(TokenType::RBrace) {
            self.advance();
            // Empty braces `{}` are an empty block (Unit), not an empty record.
            return Ok(Expr::Block {
                stmts: Vec::new(),
                expr: None,
                span,
                resolved_type: None,
            });
        }
        if self.check(TokenType::DotDotDot) {
            self.advance();
            let base = self.parse_expr()?;
            let mut fields = Vec::new();
            while self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(TokenType::RBrace) {
                    break;
                }
                let field_name = self.expect_ident()?;
                self.expect(TokenType::Colon)?;
                self.skip_newlines();
                let field_value = self.parse_expr()?;
                fields.push(FieldInit {
                    name: field_name,
                    value: field_value,
                });
            }
            self.skip_newlines();
            self.expect_closing(TokenType::RBrace, open.line, open.col, "spread record")?;
            return Ok(Expr::SpreadRecord {
                base: Box::new(base),
                fields,
                span,
                resolved_type: None,
            });
        }
        if (self.check(TokenType::Ident) || self.check(TokenType::IdentQ))
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
        {
            let mut fields = Vec::new();
            while !self.check(TokenType::RBrace) {
                self.skip_newlines();
                let field_name = self.expect_any_name()?;
                if self.check(TokenType::Colon) {
                    self.advance();
                    self.skip_newlines();
                    let field_value = self.parse_expr()?;
                    fields.push(FieldInit {
                        name: field_name.clone(),
                        value: field_value,
                    });
                } else {
                    fields.push(FieldInit {
                        name: field_name.clone(),
                        value: Expr::Ident { name: field_name, span: None, resolved_type: None },
                    });
                }
                self.skip_newlines();
                if self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect_closing(TokenType::RBrace, open.line, open.col, "record literal")?;
            return Ok(Expr::Record { name: None, fields, span, resolved_type: None });
        }

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
                    // Collect the error, insert Error node, skip to next statement boundary
                    let err_span = Some(self.current_span());
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.skip_to_next_stmt();
                    stmts.push(Stmt::Error { span: err_span });
                }
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "block")?;
        Ok(Expr::Block {
            stmts,
            expr: final_expr,
            span,
            resolved_type: None,
        })
    }

    /// Parse a braceless block: a sequence of let/var statements ending in an expression.
    /// Used for `fn foo() = let x = 1; let y = 2; x + y` without braces.
    pub(crate) fn parse_braceless_block(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        loop {
            let stmt = self.parse_stmt()?;
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }

            // Check if we've reached the end of the braceless block
            // (next token is a top-level declaration keyword or EOF)
            if self.is_at_braceless_block_end() {
                // Last item — if it's an expression statement, promote to final expr
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

        Ok(Expr::Block {
            stmts,
            expr: final_expr,
            span,
            resolved_type: None,
        })
    }

    /// Check if the current token indicates the end of a braceless function body.
    fn is_at_braceless_block_end(&self) -> bool {
        matches!(self.current().token_type,
            TokenType::EOF
            | TokenType::Fn | TokenType::Effect | TokenType::Async
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

        // [] → empty List
        if self.check(TokenType::RBracket) {
            self.advance();
            return Ok(Expr::List { elements: vec![], span, resolved_type: None });
        }

        // [:] → empty Map
        if self.check(TokenType::Colon) {
            self.advance();
            self.expect_closing(TokenType::RBracket, open.line, open.col, "empty map")?;
            return Ok(Expr::EmptyMap { span, resolved_type: None });
        }

        // Parse first expression, then decide List vs Map
        let first = self.parse_expr()?;
        self.skip_newlines();

        if self.check(TokenType::Colon) {
            // Map literal: [key: value, ...]
            self.advance();
            self.skip_newlines();
            let first_value = self.parse_expr()?;
            self.skip_newlines();
            let mut entries = vec![(first, first_value)];
            while self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(TokenType::RBracket) { break; } // trailing comma
                let key = self.parse_expr()?;
                self.skip_newlines();
                self.expect(TokenType::Colon)?;
                self.skip_newlines();
                let value = self.parse_expr()?;
                self.skip_newlines();
                entries.push((key, value));
            }
            // Detect missing comma in map literal
            if !self.check(TokenType::RBracket) && !self.check(TokenType::EOF) {
                if let Some(result) = self.check_hint(None, super::hints::HintScope::MapLiteral) {
                    let tok = self.current();
                    let msg = result.message.as_deref().unwrap_or("Unexpected token in map");
                    return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
                }
            }
            self.expect_closing(TokenType::RBracket, open.line, open.col, "map literal")?;
            return Ok(Expr::MapLiteral { entries, span, resolved_type: None });
        }

        // List literal: [expr, ...]
        let mut elements = vec![first];
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RBracket) { break; }
            elements.push(self.parse_expr()?);
            self.skip_newlines();
        }
        // Detect missing comma: next token is an expression start but not ']'
        if !self.check(TokenType::RBracket) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::ListLiteral) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in list");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        self.expect_closing(TokenType::RBracket, open.line, open.col, "list literal")?;
        Ok(Expr::List { elements, span, resolved_type: None })
    }
}
