use crate::lexer::TokenType;
use crate::ast::*;
use crate::intern::sym;
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
            Expr::Unit { id: self.next_id(), span: span.clone(), resolved_type: None }
        };
        Ok(Expr::If {
            cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_),
            id: self.next_id(), span, resolved_type: None,
        })
    }

    fn parse_if_branch(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq)
        {
            let span = Some(self.current_span());
            let name = self.advance_and_get_sym();
            self.advance(); // skip =
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Expr::Block {
                stmts: vec![Stmt::Assign { name, value, span: None }],
                expr: None,
                id: self.next_id(), span, resolved_type: None,
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
            subject: Box::new(subject), arms,
            id: self.next_id(), span, resolved_type: None,
        })
    }

    pub(crate) fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        let pattern = self.parse_pattern()?;
        let guard = if self.check(TokenType::If) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(MatchArm { pattern, guard, body, comments: Vec::new() })
    }

    pub(crate) fn parse_paren_lambda(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
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
            params, body: Box::new(body),
            id: self.next_id(), span, resolved_type: None,
        })
    }


    fn parse_lambda_param(&mut self) -> Result<LambdaParam, String> {
        if self.check(TokenType::LParen) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(TokenType::RParen) {
                names.push(self.expect_ident()?);
                if self.check(TokenType::Comma) { self.advance(); }
            }
            self.expect(TokenType::RParen)?;
            let first = names.first().copied().unwrap_or_else(|| sym(""));
            return Ok(LambdaParam { name: first, tuple_names: Some(names), ty: None });
        }
        let name = self.expect_ident()?;
        let ty = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        Ok(LambdaParam { name, tuple_names: None, ty })
    }

    pub(crate) fn parse_fan_block(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut exprs = Vec::new();
        self.skip_newlines();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::EOF) {
            // fan blocks only allow expressions — reject statements
            let tok = self.current().clone();
            match tok.token_type {
                TokenType::Let | TokenType::Var => {
                    return Err(format!("`{}` is not allowed inside fan block at line {}:{}\n  Hint: fan blocks only contain expressions, not statements", tok.value, tok.line, tok.col));
                }
                TokenType::For | TokenType::While => {
                    return Err(format!("`{}` is not allowed inside fan block at line {}:{}\n  Hint: fan blocks only contain expressions, not loops", tok.value, tok.line, tok.col));
                }
                _ => {}
            }
            let expr = self.parse_expr()?;
            exprs.push(expr);
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "fan block")?;
        if exprs.is_empty() {
            return Err(format!("fan block must contain at least one expression at line {}:{}", open.line, open.col));
        }
        Ok(Expr::Fan {
            exprs, id: self.next_id(), span, resolved_type: None,
        })
    }

}
