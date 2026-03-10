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
        self.expect(TokenType::RBrace)?;
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
        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.parse_lambda_param()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_lambda_param()?);
            }
        }
        self.expect(TokenType::RParen)?;
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
        self.expect(TokenType::RBrace)?;
        Ok(Expr::DoBlock {
            stmts,
            expr: final_expr,
            span,
            resolved_type: None,
        })
    }

    pub(crate) fn parse_brace_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
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
            self.expect(TokenType::RBrace)?;
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
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::Record { name: None, fields, span, resolved_type: None });
        }

        let mut stmts = initial_comments;
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
        self.expect(TokenType::RBrace)?;
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
            if self.check(TokenType::Let) || self.check(TokenType::Var) {
                let stmt = self.parse_stmt()?;
                stmts.push(stmt);
                self.skip_newlines();
                if self.check(TokenType::Semicolon) {
                    self.advance();
                    self.skip_newlines();
                }
            } else {
                let expr = self.parse_expr()?;
                final_expr = Some(Box::new(expr));
                break;
            }
        }

        Ok(Expr::Block {
            stmts,
            expr: final_expr,
            span,
            resolved_type: None,
        })
    }

    pub(crate) fn parse_list_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::LBracket)?;
        self.skip_newlines();
        let mut elements = Vec::new();
        while !self.check(TokenType::RBracket) {
            elements.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(TokenType::RBracket)?;
        Ok(Expr::List { elements, span, resolved_type: None })
    }
}
