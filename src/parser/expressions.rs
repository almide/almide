use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_or()?;
        while self.check(TokenType::PipeArrow) {
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
                };
            } else {
                let right = self.parse_or()?;
                left = Expr::Pipe {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
        }
        Ok(left)
    }

    pub(crate) fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.check(TokenType::Or) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: "or".to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.check(TokenType::And) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: "and".to_string(),
                left: Box::new(left),
                right: Box::new(right),
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
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> Result<Expr, String> {
        let left = self.parse_add_sub()?;
        if self.check(TokenType::DotDot) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive: false,
            });
        }
        if self.check(TokenType::DotDotEq) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::Range {
                start: Box::new(left),
                end: Box::new(right),
                inclusive: true,
            });
        }
        Ok(left)
    }

    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul_div()?;
        while self.check(TokenType::Plus) || self.check(TokenType::Minus) || self.check(TokenType::PlusPlus) {
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_mul_div()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        while self.check(TokenType::Star) || self.check(TokenType::Slash) || self.check(TokenType::Percent) || self.check(TokenType::Caret) {
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "-".to_string(),
                operand: Box::new(operand),
            });
        }
        if self.check(TokenType::Not) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "not".to_string(),
                operand: Box::new(operand),
            });
        }
        self.parse_postfix()
    }

    pub(crate) fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(TokenType::Dot) {
                self.advance();
                let field = self.expect_any_name()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    field,
                };
            } else if self.check(TokenType::LParen) {
                self.advance();
                let args = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
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
        Ok(args)
    }

    fn parse_one_call_arg(&mut self, args: &mut Vec<Expr>) -> Result<(), String> {
        if self.check(TokenType::Underscore) {
            self.advance();
            args.push(Expr::Placeholder);
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
