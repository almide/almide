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

    fn parse_or(&mut self) -> Result<Expr, String> {
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
        let mut left = self.parse_add_sub()?;
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

    fn parse_postfix(&mut self) -> Result<Expr, String> {
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

    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();

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
            });
        }
        if self.check(TokenType::Float) {
            self.advance();
            let v: f64 = tok.value.parse().unwrap_or(0.0);
            return Ok(Expr::Float { value: v });
        }
        if self.check(TokenType::String) {
            self.advance();
            return Ok(Expr::String { value: tok.value.clone() });
        }
        if self.check(TokenType::InterpolatedString) {
            self.advance();
            return Ok(Expr::InterpolatedString { value: tok.value.clone() });
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Expr::Bool { value: true });
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Expr::Bool { value: false });
        }
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Expr::Hole);
        }
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Expr::None);
        }
        if self.check(TokenType::Some) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Some { expr: Box::new(expr) });
        }
        if self.check(TokenType::Ok) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Ok { expr: Box::new(expr) });
        }
        if self.check(TokenType::Err) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Err { expr: Box::new(expr) });
        }
        if self.check(TokenType::Todo) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let msg = self.current().value.clone();
            self.expect(TokenType::String)?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Todo { message: msg });
        }
        if self.check(TokenType::Try) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Try { expr: Box::new(expr) });
        }
        if self.check(TokenType::Await) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Await { expr: Box::new(expr) });
        }
        if self.check(TokenType::If) {
            return self.parse_if_expr();
        }
        if self.check(TokenType::Match) {
            return self.parse_match_expr();
        }
        if self.check(TokenType::Fn) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LParen) {
            return self.parse_lambda();
        }
        if self.check(TokenType::For) {
            self.advance();
            let var_name = self.expect_ident()?;
            self.expect(TokenType::In)?;
            let iterable = self.parse_expr()?;
            self.expect(TokenType::LBrace)?;
            self.skip_newlines();
            let mut stmts = Vec::new();
            while !self.check(TokenType::RBrace) {
                stmts.push(self.parse_stmt()?);
                self.skip_newlines();
                if self.check(TokenType::Semicolon) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::ForIn {
                var: var_name,
                iterable: Box::new(iterable),
                body: stmts,
            });
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
            self.advance();
            if self.check(TokenType::RParen) {
                self.advance();
                return Ok(Expr::Unit);
            }
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Paren { expr: Box::new(expr) });
        }
        if self.check(TokenType::TypeName) {
            let name = tok.value.clone();
            self.advance();
            if self.check(TokenType::LBracket) {
                self.parse_type_args()?;
                if self.check(TokenType::LParen) {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(TokenType::RParen)?;
                    return Ok(Expr::Call {
                        callee: Box::new(Expr::TypeName { name }),
                        args,
                    });
                }
                return Ok(Expr::TypeName { name });
            }
            if self.check(TokenType::LParen) {
                self.advance();
                let args = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                return Ok(Expr::Call {
                    callee: Box::new(Expr::TypeName { name }),
                    args,
                });
            }
            return Ok(Expr::TypeName { name });
        }
        if self.check(TokenType::Bang) {
            return Err(format!(
                "'!' is not valid in Almide at line {}:{}\n  Hint: Use 'not x' for boolean negation, not '!x'.",
                tok.line, tok.col
            ));
        }
        if self.check(TokenType::Ident) {
            let rejected_hint = match tok.value.as_str() {
                "while" | "loop" => Some("Almide has no 'while' or 'loop'. Use 'do { guard COND else ok(()) ... }' for loops."),
                "return" => Some("Almide has no 'return'. The last expression in a block is the return value. Use 'guard ... else' for early returns."),
                "print" => Some("Use 'println(s)' instead of 'print'. There is no 'print' function in Almide."),
                "null" | "nil" => Some("Almide has no null. Use Option[T] with 'some(v)' / 'none'."),
                "throw" => Some("Almide has no exceptions. Use Result[T, E] with 'ok(v)' / 'err(e)'."),
                "catch" | "except" => Some("Almide has no try/catch. Use 'match' on Result values instead."),
                _ => None,
            };
            if let Some(hint) = rejected_hint {
                return Err(format!(
                    "'{}' is not valid in Almide at line {}:{}\n  Hint: {}",
                    tok.value, tok.line, tok.col, hint
                ));
            }
        }
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
            let name = tok.value.clone();
            self.advance();
            return Ok(Expr::Ident { name });
        }

        let hint = match tok.value.as_str() {
            "while" | "loop" => "\n  Hint: Almide has no 'while' or 'loop'. Use 'do { guard COND else ok(()) ... }' for loops.",
            "for" => "\n  Hint: Use 'list.each(xs, fn(x) => ...)' or 'do { guard ... }' instead of 'for'.",
            "return" => "\n  Hint: Almide has no 'return'. The last expression in a block is the return value. Use 'guard ... else' for early returns.",
            "null" | "nil" | "None" => "\n  Hint: Almide has no null. Use Option[T] with 'some(v)' / 'none'.",
            "throw" => "\n  Hint: Almide has no exceptions. Use Result[T, E] with 'ok(v)' / 'err(e)'.",
            "catch" | "except" => "\n  Hint: Almide has no try/catch. Use 'match' on Result values instead.",
            "class" | "struct" => "\n  Hint: Use 'type Name = { field: Type, ... }' for record types.",
            "print" => "\n  Hint: Use 'println(s)' instead of 'print'. There is no 'print' function in Almide.",
            _ => "",
        };
        Err(format!(
            "Expected expression at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    pub(crate) fn parse_if_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::If)?;
        self.skip_newlines();
        let cond = self.parse_expr()?;
        self.skip_newlines();
        self.expect(TokenType::Then)?;
        self.skip_newlines();
        let then = self.parse_if_branch()?;
        self.skip_newlines();
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let else_ = self.parse_if_branch()?;
        Ok(Expr::If {
            cond: Box::new(cond),
            then: Box::new(then),
            else_: Box::new(else_),
        })
    }

    fn parse_if_branch(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Ident) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq) {
            let name = self.advance_and_get_value();
            self.advance();
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Expr::Block {
                stmts: vec![Stmt::Assign { name, value }],
                expr: None,
            });
        }
        self.parse_expr()
    }

    pub(crate) fn parse_match_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::Match)?;
        self.skip_newlines();
        let subject = self.parse_or()?;
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
        Ok(Expr::Match {
            subject: Box::new(subject),
            arms,
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
        Ok(MatchArm { pattern, guard, body })
    }

    pub(crate) fn parse_lambda(&mut self) -> Result<Expr, String> {
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
        })
    }

    fn parse_lambda_param(&mut self) -> Result<LambdaParam, String> {
        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        Ok(LambdaParam { name, ty })
    }

    pub(crate) fn parse_do_block(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;
        while !self.check(TokenType::RBrace) {
            let stmt = self.parse_stmt()?;
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }
            if self.check(TokenType::RBrace) {
                if let Stmt::Expr { expr } = stmt {
                    final_expr = Some(Box::new(expr));
                } else {
                    stmts.push(stmt);
                }
            } else {
                stmts.push(stmt);
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::DoBlock {
            stmts,
            expr: final_expr,
        })
    }

    pub(crate) fn parse_brace_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        if self.check(TokenType::RBrace) {
            self.advance();
            return Ok(Expr::Record { fields: Vec::new() });
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
                        value: Expr::Ident { name: field_name },
                    });
                }
                self.skip_newlines();
                if self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::Record { fields });
        }

        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;
        while !self.check(TokenType::RBrace) {
            let stmt = self.parse_stmt()?;
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }
            if self.check(TokenType::RBrace) {
                if let Stmt::Expr { expr } = stmt {
                    final_expr = Some(Box::new(expr));
                } else {
                    stmts.push(stmt);
                }
            } else {
                stmts.push(stmt);
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::Block {
            stmts,
            expr: final_expr,
        })
    }

    pub(crate) fn parse_list_expr(&mut self) -> Result<Expr, String> {
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
        Ok(Expr::List { elements })
    }
}
