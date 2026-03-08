use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_pattern(&mut self) -> Result<Pattern, String> {
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Pattern::Wildcard);
        }
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Pattern::None);
        }
        if self.check(TokenType::Some) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Some { inner: Box::new(inner) });
        }
        if self.check(TokenType::Ok) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Ok { inner: Box::new(inner) });
        }
        if self.check(TokenType::Err) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Err { inner: Box::new(inner) });
        }
        if self.check(TokenType::LParen) {
            self.advance();
            let first = self.parse_pattern()?;
            if self.check(TokenType::Comma) {
                let mut elements = vec![first];
                while self.check(TokenType::Comma) {
                    self.advance();
                    elements.push(self.parse_pattern()?);
                }
                self.expect(TokenType::RParen)?;
                return Ok(Pattern::Tuple { elements });
            }
            // Single parenthesized pattern
            self.expect(TokenType::RParen)?;
            return Ok(first);
        }
        if self.check(TokenType::Int) || self.check(TokenType::Float) || self.check(TokenType::String) {
            let expr = self.parse_primary()?;
            return Ok(Pattern::Literal { value: Box::new(expr) });
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::Bool { value: true }),
            });
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::Bool { value: false }),
            });
        }
        if self.check(TokenType::TypeName) {
            let name = self.current().value.clone();
            self.advance();
            if self.check(TokenType::LParen) {
                self.advance();
                let mut args = Vec::new();
                if !self.check(TokenType::RParen) {
                    args.push(self.parse_pattern()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        args.push(self.parse_pattern()?);
                    }
                }
                self.expect(TokenType::RParen)?;
                return Ok(Pattern::Constructor { name, args });
            }
            if self.check(TokenType::LBrace) {
                self.advance();
                self.skip_newlines();
                let mut fields = Vec::new();
                while !self.check(TokenType::RBrace) {
                    let field_name = self.expect_ident()?;
                    if self.check(TokenType::Colon) {
                        self.advance();
                        let pattern = self.parse_pattern()?;
                        fields.push(FieldPattern {
                            name: field_name,
                            pattern: Some(pattern),
                        });
                    } else {
                        fields.push(FieldPattern {
                            name: field_name,
                            pattern: None,
                        });
                    }
                    if self.check(TokenType::Comma) {
                        self.advance();
                        self.skip_newlines();
                    }
                }
                self.expect(TokenType::RBrace)?;
                return Ok(Pattern::RecordPattern { name, fields });
            }
            return Ok(Pattern::Constructor { name, args: Vec::new() });
        }
        if self.check(TokenType::Ident) {
            let name = self.current().value.clone();
            self.advance();
            return Ok(Pattern::Ident { name });
        }

        let tok = self.current();
        Err(format!(
            "Expected pattern at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }
}
