use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, String> {
        self.enter_depth()?;
        let result = self.parse_pipe();
        self.exit_depth();
        result
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_or()?;
        while self.check(TokenType::PipeArrow) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::Match) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace) {
                self.advance();
                self.skip_newlines();
                self.expect(TokenType::LBrace)?;
                self.skip_newlines();
                let mut arms = Vec::new();
                while !self.check(TokenType::RBrace) {
                    arms.push(self.parse_match_arm()?);
                    self.skip_newlines();
                    if self.check(TokenType::Comma) {
                        self.advance();
                        self.skip_newlines();
                    }
                }
                self.expect(TokenType::RBrace)?;
                left = Expr::Match {
                    subject: Box::new(left),
                    arms,
                    id: self.next_id(),
                    span,
                    resolved_type: None,
                };
            } else {
                let right = self.parse_or()?;
                left = Expr::Pipe {
                    left: Box::new(left),
                    right: Box::new(right),
                    id: self.next_id(),
                    span,
                    resolved_type: None,
                };
            }
        }
        Ok(left)
    }

    pub(crate) fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.check(TokenType::Or) || self.check(TokenType::PipePipe) {
            if self.check(TokenType::PipePipe) {
                return Err(self.check_hint_or_err(None, super::hints::HintScope::Expression,
                    "'||' is not valid in Almide"));
            }
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: "or".to_string(),
                left: Box::new(left),
                right: Box::new(right),
                id: self.next_id(),
                span,
                resolved_type: None,
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.check(TokenType::And) || self.check(TokenType::AmpAmp) {
            if self.check(TokenType::AmpAmp) {
                return Err(self.check_hint_or_err(None, super::hints::HintScope::Expression,
                    "'&&' is not valid in Almide"));
            }
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: "and".to_string(),
                left: Box::new(left),
                right: Box::new(right),
                id: self.next_id(),
                span,
                resolved_type: None,
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_range()?;
        while self.check(TokenType::EqEq)
            || self.check(TokenType::BangEq)
            || self.check(TokenType::LAngle)
            || self.check(TokenType::RAngle)
            || self.check(TokenType::LtEq)
            || self.check(TokenType::GtEq)
        {
            let span = Some(self.current_span());
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                id: self.next_id(),
                span,
                resolved_type: None,
            };
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, String> {
        let left = self.parse_add_sub()?;
        if self.check(TokenType::DotDot) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive: false,
                id: self.next_id(),
                span,
                resolved_type: None,
            });
        }
        if self.check(TokenType::DotDotEq) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive: true,
                id: self.next_id(),
                span,
                resolved_type: None,
            });
        }
        Ok(left)
    }

    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul_div()?;
        while self.check(TokenType::Plus) || self.check(TokenType::Minus) || self.check(TokenType::PlusPlus) {
            let span = Some(self.current_span());
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_mul_div()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                id: self.next_id(),
                span,
                resolved_type: None,
            };
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        while self.check(TokenType::Star) || self.check(TokenType::Slash) || self.check(TokenType::Percent) || self.check(TokenType::Caret) {
            let span = Some(self.current_span());
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                id: self.next_id(),
                span,
                resolved_type: None,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Minus) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "-".to_string(),
                operand: Box::new(operand),
                id: self.next_id(),
                span,
                resolved_type: None,
            });
        }
        if self.check(TokenType::Not) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "not".to_string(),
                operand: Box::new(operand),
                id: self.next_id(),
                span,
                resolved_type: None,
            });
        }
        self.parse_postfix()
    }

    pub(crate) fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(TokenType::Dot) {
                let span = Some(self.current_span());
                self.advance();
                // Tuple index access: t.0, t.1, etc.
                if self.check(TokenType::Int) {
                    let idx_str = self.current().value.clone();
                    self.advance();
                    let index = idx_str.parse::<usize>().map_err(|_| format!("invalid tuple index '{}' at line {:?}", idx_str, span))?;
                    expr = Expr::TupleIndex {
                        object: Box::new(expr),
                        index,
                        id: self.next_id(),
                        span,
                        resolved_type: None,
                    };
                } else {
                    let field = self.expect_any_name()?;
                    expr = Expr::Member {
                        object: Box::new(expr),
                        field,
                        id: self.next_id(),
                        span,
                        resolved_type: None,
                    };
                }
            } else if self.check(TokenType::LBracket) && self.peek_type_args_call() {
                // Call with explicit type arguments: f[Int, String](args)
                let span = Some(self.current_span());
                let ta = self.parse_type_args()?;
                self.expect(TokenType::LParen)?;
                let args = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    type_args: Some(ta),
                    id: self.next_id(),
                    span,
                    resolved_type: None,
                };
            } else if self.check(TokenType::LBracket) && !self.newline_before_current() {
                let span = Some(self.current_span());
                let open = self.current().clone();
                self.advance(); // skip [
                let index = self.parse_expr()?;
                self.expect_closing(TokenType::RBracket, open.line, open.col, "index access")?;
                expr = Expr::IndexAccess {
                    object: Box::new(expr),
                    index: Box::new(index),
                    id: self.next_id(),
                    span,
                    resolved_type: None,
                };
            } else if self.check(TokenType::LParen) && !self.newline_before_current() {
                let span = Some(self.current_span());
                let open = self.current().clone();
                self.advance();
                let args = self.parse_call_args()?;
                self.expect_closing(TokenType::RParen, open.line, open.col, "function call")?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    type_args: None,
                    id: self.next_id(),
                    span,
                    resolved_type: None,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    pub(crate) fn parse_call_args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        self.skip_newlines();
        if self.check(TokenType::RParen) {
            return Ok(args);
        }
        self.parse_one_call_arg(&mut args)?;
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RParen) {
                break;
            }
            self.parse_one_call_arg(&mut args)?;
        }
        self.skip_newlines();
        // Detect missing comma between arguments
        if !self.check(TokenType::RParen) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::CallArgs) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in arguments");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        Ok(args)
    }

    fn parse_one_call_arg(&mut self, args: &mut Vec<Expr>) -> Result<(), String> {
        if self.check(TokenType::Underscore) {
            let span = Some(self.current_span());
            self.advance();
            args.push(Expr::Placeholder { id: self.next_id(), span, resolved_type: None });
            return Ok(());
        }
        if self.check(TokenType::Ident) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon) {
            self.advance();
            self.advance();
            self.skip_newlines();
            let value = self.parse_expr()?;
            args.push(value);
        } else {
            args.push(self.parse_expr()?);
        }
        Ok(())
    }
}
