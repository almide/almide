use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::{Sym, sym};
use super::Parser;

impl Parser {
    pub(crate) fn parse_pattern(&mut self) -> Result<Pattern, String> {
        if let Some(p) = self.parse_structural_pattern() {
            return p;
        }
        if let Some(p) = self.parse_literal_pattern()? {
            return Ok(p);
        }
        if let Some(p) = self.parse_name_pattern() {
            return p;
        }
        Err(self.pattern_expected_error())
    }

    /// The patterns a single leading token selects: `_`, `none`, the
    /// `some`/`ok`/`err` wrappers, and the `(…)` / `[…]` bracket forms. `None`
    /// means the head is not one of them.
    fn parse_structural_pattern(&mut self) -> Option<Result<Pattern, String>> {
        if self.check(TokenType::Underscore) {
            self.advance();
            return Some(Ok(Pattern::Wildcard));
        }
        if self.check(TokenType::None) {
            self.advance();
            return Some(Ok(Pattern::None));
        }
        if self.check(TokenType::Some) {
            return Some(self.parse_some_pattern());
        }
        if self.check(TokenType::Ok) {
            return Some(self.parse_ok_pattern());
        }
        if self.check(TokenType::Err) {
            return Some(self.parse_err_pattern());
        }
        if self.check(TokenType::LParen) {
            return Some(self.parse_tuple_or_paren_pattern());
        }
        // List pattern: [], [a], [a, b, ...]
        if self.check(TokenType::LBracket) {
            return Some(self.parse_list_pattern());
        }
        None
    }

    /// The literal patterns: a negative numeric (`-1`, `-3.14`), a plain
    /// int/float/string, or a bool keyword. `Ok(None)` means the head is not a
    /// literal; an `Err` is a real parse failure inside one.
    fn parse_literal_pattern(&mut self) -> Result<Option<Pattern>, String> {
        if self.check(TokenType::Minus)
            && self.peek_at(1).map(|t| matches!(t.token_type, TokenType::Int | TokenType::Float)).unwrap_or(false)
        {
            return self.parse_negative_literal_pattern().map(Some);
        }
        if self.check(TokenType::Int) || self.check(TokenType::Float) || self.check(TokenType::String) {
            let expr = self.parse_primary()?;
            return Ok(Some(Pattern::Literal { value: Box::new(expr) }));
        }
        let value = match () {
            _ if self.check(TokenType::True) => true,
            _ if self.check(TokenType::False) => false,
            _ => return Ok(None),
        };
        let span = Some(self.current_span());
        self.advance();
        Ok(Some(Pattern::Literal {
            value: Box::new(Expr::new(self.next_id(), span, ExprKind::Bool { value })),
        }))
    }

    /// The name-headed patterns: a constructor (`Ctor`), a module-qualified
    /// constructor (`binary.Unreachable`), or a plain binder.
    fn parse_name_pattern(&mut self) -> Option<Result<Pattern, String>> {
        if self.check(TokenType::TypeName) {
            return Some(self.parse_constructor_pattern());
        }
        if self.check(TokenType::Ident) && self.peek_dot_type_name() {
            return Some(self.parse_qualified_constructor_pattern());
        }
        if self.check(TokenType::Ident) {
            let name = sym(&self.current().value);
            self.advance();
            return Some(Ok(Pattern::Ident { name }));
        }
        None
    }

    fn parse_some_pattern(&mut self) -> Result<Pattern, String> {
        self.advance();
        self.expect(TokenType::LParen)?;
        let inner = self.parse_pattern()?;
        self.expect(TokenType::RParen)?;
        Ok(Pattern::Some { inner: Box::new(inner) })
    }

    fn parse_ok_pattern(&mut self) -> Result<Pattern, String> {
        self.advance();
        self.expect(TokenType::LParen)?;
        let inner = self.parse_pattern()?;
        self.expect(TokenType::RParen)?;
        Ok(Pattern::Ok { inner: Box::new(inner) })
    }

    fn parse_err_pattern(&mut self) -> Result<Pattern, String> {
        self.advance();
        self.expect(TokenType::LParen)?;
        let inner = self.parse_pattern()?;
        self.expect(TokenType::RParen)?;
        Ok(Pattern::Err { inner: Box::new(inner) })
    }

    fn parse_tuple_or_paren_pattern(&mut self) -> Result<Pattern, String> {
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
        Ok(first)
    }

    fn parse_list_pattern(&mut self) -> Result<Pattern, String> {
        self.advance();
        let mut elements = Vec::new();
        let mut rest: Option<Option<Sym>> = None;
        if !self.check(TokenType::RBracket) {
            loop {
                // `..`/`..name` (#1461): the rest form, LAST position only
                // — a suffix after it has no lowering and refuses here.
                if self.check(TokenType::DotDot) {
                    self.advance();
                    let name = if self.check(TokenType::Ident) {
                        Some(self.advance_and_get_sym())
                    } else {
                        None
                    };
                    rest = Some(name);
                    if self.check(TokenType::Comma) {
                        let tok = self.current();
                        return Err(format!(
                            "Rest pattern must be last at line {}:{}: `[a, ..t]` binds the whole tail — nothing can follow it",
                            tok.line, tok.col
                        ));
                    }
                    break;
                }
                elements.push(self.parse_pattern()?);
                if !self.check(TokenType::Comma) {
                    break;
                }
                self.advance();
                if self.check(TokenType::RBracket) {
                    break;
                }
            }
        }
        self.expect(TokenType::RBracket)?;
        Ok(Pattern::List { elements, rest })
    }

    fn parse_negative_literal_pattern(&mut self) -> Result<Pattern, String> {
        let span = Some(self.current_span());
        self.advance(); // skip -
        let operand = self.parse_primary()?;
        Ok(Pattern::Literal {
            value: Box::new(Expr::new(self.next_id(), span, ExprKind::Unary {
                op: sym("-"), operand: Box::new(operand),
            })),
        })
    }

    fn parse_qualified_constructor_pattern(&mut self) -> Result<Pattern, String> {
        let module = self.advance_and_get_sym();
        self.advance(); // skip '.'
        // Merge into a single constructor name for downstream resolution
        let ctor = self.advance_and_get_sym();
        let name = sym(&format!("{}.{}", module, ctor));
        self.parse_constructor_pattern_with_name(name)
    }

    /// Builds the "Expected pattern" error, including targeted hints for
    /// common LLM-imported patterns from other languages. DotDotDot / DotDot
    /// in list-pattern position = rest spread (Rust / JS). Colon-Colon = cons
    /// pattern (Haskell / OCaml / Elm). Both don't exist in Almide list
    /// patterns; point to the idiomatic recursion form using list.first /
    /// list.drop.
    fn pattern_expected_error(&self) -> String {
        let tok = self.current();
        let hint: String = match (&tok.token_type, tok.value.as_str()) {
            (_, "=>") => "\n  Hint: Missing pattern before '=>'. Use '_' for wildcard, or a variable name".into(),
            // #1677: `ok(_) => c = c + 1` — the arm body parsed as `c`, the
            // parser moved on to the next arm, and `=` is where it noticed.
            // The broken rule is the statement/expression boundary, not
            // pattern syntax — name it, or the hint sends the reader to
            // enumerate patterns (the one thing that was right).
            (TokenType::Eq, _) => "\n  Hint: assignment is a statement, not an expression — a match arm that assigns needs a block body:\n    ok(_) => { c = c + 1 }".into(),
            (TokenType::DotDotDot, _) | (TokenType::DotDot, _) => {
                "\n  Hint: the rest form is spelled with TWO dots and lives inside a list pattern: `[h, ..t]` binds the tail, `[h, ..]` ignores it.\n\
                  Here the dots sit where a whole pattern was expected — put them as the LAST element of `[...]`.\n\
                  Note: `{ x, .. }` IS valid inside record patterns.".into()
            }
            _ => "\n  Hint: Valid patterns: _, variable, Type(args), (a, b), [], [a, b], some(x), ok(x), err(x), none, true, false, 42, \"text\"".into(),
        };
        format!(
            "Expected pattern at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        )
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
                let field_name = self.expect_any_name()?;
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
