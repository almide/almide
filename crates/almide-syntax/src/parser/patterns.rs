use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::{Sym, sym};
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
            self.expect(TokenType::RParen)?;
            return Ok(first);
        }
        // List pattern: [], [a], [a, b, ...]
        if self.check(TokenType::LBracket) {
            self.advance();
            let mut elements = Vec::new();
            if !self.check(TokenType::RBracket) {
                elements.push(self.parse_pattern()?);
                while self.check(TokenType::Comma) {
                    self.advance();
                    if self.check(TokenType::RBracket) { break; }
                    elements.push(self.parse_pattern()?);
                }
            }
            self.expect(TokenType::RBracket)?;
            return Ok(Pattern::List { elements });
        }
        // Negative numeric literal: -1, -3.14
        if self.check(TokenType::Minus)
            && self.peek_at(1).map(|t| matches!(t.token_type, TokenType::Int | TokenType::Float)).unwrap_or(false)
        {
            let span = Some(self.current_span());
            self.advance(); // skip -
            let operand = self.parse_primary()?;
            return Ok(Pattern::Literal {
                value: Box::new(Expr::new(self.next_id(), span, ExprKind::Unary {
                    op: sym("-"), operand: Box::new(operand),
                })),
            });
        }
        if self.check(TokenType::Int) || self.check(TokenType::Float) || self.check(TokenType::String) {
            let expr = self.parse_primary()?;
            return Ok(Pattern::Literal { value: Box::new(expr) });
        }
        if self.check(TokenType::True) {
            let span = Some(self.current_span());
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::new(self.next_id(), span, ExprKind::Bool { value: true })),
            });
        }
        if self.check(TokenType::False) {
            let span = Some(self.current_span());
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::new(self.next_id(), span, ExprKind::Bool { value: false })),
            });
        }
        if self.check(TokenType::TypeName) {
            return self.parse_constructor_pattern();
        }
        // Module-qualified constructor pattern: module.TypeName (e.g. binary.Unreachable)
        if self.check(TokenType::Ident) && self.peek_dot_type_name() {
            let module = self.advance_and_get_sym();
            self.advance(); // skip '.'
            // Merge into a single constructor name for downstream resolution
            let ctor = self.advance_and_get_sym();
            let name = sym(&format!("{}.{}", module, ctor));
            return self.parse_constructor_pattern_with_name(name);
        }
        if self.check(TokenType::Ident) {
            let name = sym(&self.current().value);
            self.advance();
            return Ok(Pattern::Ident { name });
        }

        let tok = self.current();
        let hint = match tok.value.as_str() {
            "=>" => "\n  Hint: Missing pattern before '=>'. Use '_' for wildcard, or a variable name",
            _ => "\n  Hint: Valid patterns: _, variable, Type(args), (a, b), [], [a, b], some(x), ok(x), err(x), none, true, false, 42, \"text\"",
        };
        Err(format!(
            "Expected pattern at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    fn parse_constructor_pattern(&mut self) -> Result<Pattern, String> {
        let name = sym(&self.current().value);
        self.advance();
        self.parse_constructor_pattern_with_name(name)
    }

    fn parse_constructor_pattern_with_name(&mut self, name: Sym) -> Result<Pattern, String> {
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
            let mut rest = false;
            while !self.check(TokenType::RBrace) {
                if self.check(TokenType::DotDot) {
                    self.advance();
                    rest = true;
                    if self.check(TokenType::Comma) { self.advance(); }
                    self.skip_newlines();
                    break;
                }
                let field_name = self.expect_ident()?;
                if self.check(TokenType::Colon) {
                    self.advance();
                    let pattern = self.parse_pattern()?;
                    fields.push(FieldPattern { name: field_name, pattern: Some(pattern) });
                } else {
                    fields.push(FieldPattern { name: field_name, pattern: None });
                }
                if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Pattern::RecordPattern { name, fields, rest });
        }
        Ok(Pattern::Constructor { name, args: Vec::new() })
    }
}
