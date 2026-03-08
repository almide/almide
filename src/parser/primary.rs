use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
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
            // Support `for (a, b) in ...` tuple destructuring
            let (var_name, var_tuple) = if self.check(TokenType::LParen) {
                self.advance();
                let mut names = vec![self.expect_ident()?];
                while self.check(TokenType::Comma) {
                    self.advance();
                    names.push(self.expect_ident()?);
                }
                self.expect(TokenType::RParen)?;
                (names[0].clone(), Some(names))
            } else {
                (self.expect_ident()?, None)
            };
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
                var_tuple,
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
            let first = self.parse_expr()?;
            if self.check(TokenType::Comma) {
                // Tuple: (a, b, ...)
                let mut elements = vec![first];
                while self.check(TokenType::Comma) {
                    self.advance();
                    if self.check(TokenType::RParen) { break; } // trailing comma
                    elements.push(self.parse_expr()?);
                }
                self.expect(TokenType::RParen)?;
                return Ok(Expr::Tuple { elements });
            }
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Paren { expr: Box::new(first) });
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
}
