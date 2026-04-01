use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::{Sym, sym};
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
        loop {
            self.skip_newlines_if_followed_by(TokenType::PipeArrow);
            self.skip_newlines_if_followed_by(TokenType::ComposeArrow);
            if self.check(TokenType::ComposeArrow) {
                let span = Some(self.current_span());
                self.advance();
                self.skip_newlines();
                let right = self.parse_or()?;
                left = Expr::new(self.next_id(), span, ExprKind::Compose {
                    left: Box::new(left),
                    right: Box::new(right),
                });
            } else if self.check(TokenType::PipeArrow) {
                let span = Some(self.current_span());
                self.advance();
                self.skip_newlines();
                if self.check(TokenType::Match)
                    && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace)
                {
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
                    left = Expr::new(self.next_id(), span, ExprKind::Match {
                        subject: Box::new(left),
                        arms,
                    });
                } else {
                    let right = self.parse_or()?;
                    left = Expr::new(self.next_id(), span, ExprKind::Pipe {
                        left: Box::new(left),
                        right: Box::new(right),
                    });
                }
            } else {
                break;
            }
        }
        Ok(left)
    }

    pub(crate) fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        loop {
            self.skip_newlines_if_followed_by_any(&[TokenType::Or]);
            if !(self.check(TokenType::Or) || self.check(TokenType::PipePipe)) { break; }
            if self.check(TokenType::PipePipe) {
                return Err(self.check_hint_or_err(
                    None, super::hints::HintScope::Expression,
                    "'||' is not valid in Almide",
                ));
            }
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::new(self.next_id(), span, ExprKind::Binary {
                op: sym("or"),
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        loop {
            self.skip_newlines_if_followed_by_any(&[TokenType::And]);
            if !(self.check(TokenType::And) || self.check(TokenType::AmpAmp)) { break; }
            if self.check(TokenType::AmpAmp) {
                return Err(self.check_hint_or_err(
                    None, super::hints::HintScope::Expression,
                    "'&&' is not valid in Almide",
                ));
            }
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_comparison()?;
            left = Expr::new(self.next_id(), span, ExprKind::Binary {
                op: sym("and"),
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_range()?;
        self.skip_newlines_if_followed_by_any(&[TokenType::EqEq, TokenType::BangEq, TokenType::LtEq, TokenType::GtEq]);
        if !(self.check(TokenType::EqEq)
            || self.check(TokenType::BangEq)
            || self.check(TokenType::LAngle)
            || self.check(TokenType::RAngle)
            || self.check(TokenType::LtEq)
            || self.check(TokenType::GtEq)) { return Ok(left); }
        let span = Some(self.current_span());
        let op = sym(&self.current().value);
        self.advance();
        self.skip_newlines();
        let right = self.parse_range()?;
        let result = Expr::new(self.next_id(), span, ExprKind::Binary {
            op, left: Box::new(left), right: Box::new(right),
        });
        // Reject chained comparisons: a < b < c
        if self.check(TokenType::EqEq) || self.check(TokenType::BangEq)
            || self.check(TokenType::LAngle) || self.check(TokenType::RAngle)
            || self.check(TokenType::LtEq) || self.check(TokenType::GtEq)
        {
            let tok = self.current();
            return Err(format!(
                "Chained comparison operators are not allowed at line {}:{}\n  Hint: Use 'and' to combine comparisons. Write: a < b and b < c",
                tok.line, tok.col
            ));
        }
        Ok(result)
    }

    fn parse_range(&mut self) -> Result<Expr, String> {
        let left = self.parse_add_sub()?;
        if self.check(TokenType::DotDot) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Range {
                start: Box::new(left), end: Box::new(right), inclusive: false,
            }));
        }
        if self.check(TokenType::DotDotEq) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Range {
                start: Box::new(left), end: Box::new(right), inclusive: true,
            }));
        }
        Ok(left)
    }

    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul_div()?;
        loop {
            self.skip_newlines_if_followed_by_any(&[TokenType::Plus, TokenType::Minus, TokenType::PlusPlus]);
            if !(self.check(TokenType::Plus) || self.check(TokenType::Minus)
                || self.check(TokenType::PlusPlus)) { break; }
            let span = Some(self.current_span());
            let op = sym(&self.current().value);
            self.advance();
            self.skip_newlines();
            let right = self.parse_mul_div()?;
            left = Expr::new(self.next_id(), span, ExprKind::Binary {
                op, left: Box::new(left), right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_power()?;
        loop {
            self.skip_newlines_if_followed_by_any(&[TokenType::Star, TokenType::Slash, TokenType::Percent]);
            if !(self.check(TokenType::Star) || self.check(TokenType::Slash)
                || self.check(TokenType::Percent)) { break; }
            let span = Some(self.current_span());
            let op = sym(&self.current().value);
            self.advance();
            self.skip_newlines();
            let right = self.parse_power()?;
            left = Expr::new(self.next_id(), span, ExprKind::Binary {
                op, left: Box::new(left), right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        // ^ is right-associative (2 ^ 3 ^ 2 = 2 ^ (3 ^ 2))
        if self.check(TokenType::Caret) {
            let span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            let right = self.parse_power()?;
            left = Expr::new(self.next_id(), span, ExprKind::Binary {
                op: sym("^"), left: Box::new(left), right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Minus) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unary {
                op: sym("-"), operand: Box::new(operand),
            }));
        }
        if self.check(TokenType::Not) {
            let span = Some(self.current_span());
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unary {
                op: sym("not"), operand: Box::new(operand),
            }));
        }
        if self.check(TokenType::Bang) {
            let tok = self.current();
            return Err(format!(
                "'!' is not valid in Almide at line {}:{}\n  Hint: Use 'not' for boolean negation. Write: not x",
                tok.line, tok.col
            ));
        }
        self.parse_postfix()
    }

    pub(crate) fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(TokenType::Dot) {
                let span = Some(self.current_span());
                self.advance();
                if self.check(TokenType::Int) {
                    let idx_str = self.current().value.clone();
                    self.advance();
                    let index = idx_str.parse::<usize>().map_err(|_| {
                        format!("invalid tuple index '{}' at line {:?}", idx_str, span)
                    })?;
                    expr = Expr::new(self.next_id(), span, ExprKind::TupleIndex {
                        object: Box::new(expr), index,
                    });
                } else {
                    let field = self.expect_any_name()?;
                    expr = Expr::new(self.next_id(), span, ExprKind::Member {
                        object: Box::new(expr), field,
                    });
                }
            } else if self.check(TokenType::LBracket) && self.peek_type_args_call() {
                let span = Some(self.current_span());
                let ta = self.parse_type_args()?;
                self.expect(TokenType::LParen)?;
                let (args, named_args) = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                expr = Expr::new(self.next_id(), span, ExprKind::Call {
                    callee: Box::new(expr), args, named_args, type_args: Some(ta),
                });
            } else if self.check(TokenType::LBracket) && !self.newline_before_current() {
                let span = Some(self.current_span());
                let open = self.current().clone();
                self.advance();
                let index = self.parse_expr()?;
                self.expect_closing(TokenType::RBracket, open.line, open.col, "index access")?;
                expr = Expr::new(self.next_id(), span, ExprKind::IndexAccess {
                    object: Box::new(expr), index: Box::new(index),
                });
            } else if self.check(TokenType::LParen) && !self.newline_before_current() {
                let span = Some(self.current_span());
                let open = self.current().clone();
                self.advance();
                let (args, named_args) = self.parse_call_args()?;
                self.expect_closing(TokenType::RParen, open.line, open.col, "function call")?;
                expr = Expr::new(self.next_id(), span, ExprKind::Call {
                    callee: Box::new(expr), args, named_args, type_args: None,
                });
            } else if self.check(TokenType::Bang) && !self.newline_before_current() {
                // expr! — unwrap with error propagation
                let span = Some(self.current_span());
                self.advance();
                expr = Expr::new(self.next_id(), span, ExprKind::Unwrap {
                    expr: Box::new(expr),
                });
            } else if self.check(TokenType::QuestionQuestion) {
                // expr ?? fallback — unwrap with default
                let span = Some(self.current_span());
                self.advance();
                self.skip_newlines();
                let fallback = self.parse_unary()?;
                expr = Expr::new(self.next_id(), span, ExprKind::UnwrapOr {
                    expr: Box::new(expr), fallback: Box::new(fallback),
                });
            } else if self.check(TokenType::QuestionDot) && !self.newline_before_current() {
                // expr?.field — optional chaining
                let span = Some(self.current_span());
                self.advance();
                let field = self.expect_any_name()?;
                expr = Expr::new(self.next_id(), span, ExprKind::OptionalChain {
                    expr: Box::new(expr), field,
                });
            } else if self.check(TokenType::Question) && !self.newline_before_current() {
                // expr? — convert to Option
                let span = Some(self.current_span());
                self.advance();
                expr = Expr::new(self.next_id(), span, ExprKind::ToOption {
                    expr: Box::new(expr),
                });
            } else {
                break;
            }
        }
        Ok(expr)
    }

    pub(crate) fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<(Sym, Expr)>), String> {
        let mut args = Vec::new();
        let mut named_args = Vec::new();
        self.skip_newlines();
        if self.check(TokenType::RParen) { return Ok((args, named_args)); }
        self.parse_one_call_arg(&mut args, &mut named_args)?;
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RParen) { break; }
            self.parse_one_call_arg(&mut args, &mut named_args)?;
        }
        self.skip_newlines();
        if !self.check(TokenType::RParen) && !self.check(TokenType::EOF) {
            if let Some(result) = self.check_hint(None, super::hints::HintScope::CallArgs) {
                let tok = self.current();
                let msg = result.message.as_deref().unwrap_or("Unexpected token in arguments");
                return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
            }
        }
        Ok((args, named_args))
    }

    fn parse_one_call_arg(&mut self, args: &mut Vec<Expr>, named_args: &mut Vec<(Sym, Expr)>) -> Result<(), String> {
        if self.check(TokenType::Underscore) {
            let span = Some(self.current_span());
            self.advance();
            args.push(Expr::new(self.next_id(), span, ExprKind::Placeholder));
            return Ok(());
        }
        // Named argument: `name: expr`
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Colon) // not ::
        {
            let name = self.advance_and_get_sym();
            self.advance(); // skip :
            self.skip_newlines();
            let value = self.parse_expr()?;
            if !named_args.is_empty() || true {
                // Once named starts, check no positional after
                if named_args.is_empty() && !args.is_empty() {
                    // First named arg after positional — OK
                }
                named_args.push((name, value));
            }
        } else {
            if !named_args.is_empty() {
                let tok = self.current();
                return Err(format!("positional argument after named argument at line {}:{}", tok.line, tok.col));
            }
            args.push(self.parse_expr()?);
        }
        Ok(())
    }
}
