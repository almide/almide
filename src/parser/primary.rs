use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::sym;
use super::Parser;

impl Parser {
    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();
        let span = Some(Span { line: tok.line, col: tok.col, end_col: tok.end_col });

        if self.check(TokenType::Int) {
            self.advance();
            let clean = tok.value.replace('_', "");
            let parsed: i64 = if clean.starts_with("0x") || clean.starts_with("0X") {
                i64::from_str_radix(&clean[2..], 16).unwrap_or(0)
            } else {
                clean.parse().unwrap_or(0)
            };
            return Ok(Expr::new(self.next_id(), span, ExprKind::Int {
                value: serde_json::Value::Number(
                    serde_json::Number::from_f64(parsed as f64)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
                raw: tok.value.clone(),
            }));
        }
        if self.check(TokenType::Float) {
            self.advance();
            let v: f64 = tok.value.replace('_', "").parse().unwrap_or(0.0);
            return Ok(Expr::new(self.next_id(), span, ExprKind::Float { value: v }));
        }
        if self.check(TokenType::String) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::String { value: tok.value.clone() }));
        }
        if self.check(TokenType::InterpolatedString) {
            self.advance();
            let parts = self.parse_interpolation_parts(&tok.value, tok.line, tok.col)?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::InterpolatedString { parts }));
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Bool { value: true }));
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Bool { value: false }));
        }
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Hole));
        }
        if self.check(TokenType::Break) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Break));
        }
        if self.check(TokenType::Continue) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Continue));
        }
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::None));
        }
        if self.check(TokenType::Some) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "some()")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Some { expr: Box::new(expr) }));
        }
        if self.check(TokenType::Ok) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "ok()")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Ok { expr: Box::new(expr) }));
        }
        if self.check(TokenType::Err) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "err()")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Err { expr: Box::new(expr) }));
        }
        if self.check(TokenType::Todo) {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LParen)?;
            let msg = self.current().value.clone();
            self.expect(TokenType::String)?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "todo()")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Todo { message: msg }));
        }
        // try and await keywords removed (no implementation)
        if self.check(TokenType::If) {
            return self.parse_if_expr();
        }
        if self.check(TokenType::Match) {
            return self.parse_match_expr();
        }

        if self.check(TokenType::While) {
            return self.parse_while_expr();
        }
        if self.check(TokenType::For) {
            return self.parse_for_expr();
        }
        if self.check_ident("do") {
            let span = self.current_span();
            self.advance();
            return Err(format!("`do` blocks have been removed — use `while` for loops or remove `do` from effect fn bodies (line {})", span.line));
        }
        if self.check(TokenType::Fan) {
            // fan { ... } = fan block; fan.map/fan.race = module-like call
            if self.peek_at(1).map_or(false, |t| t.token_type == TokenType::Dot) {
                // Treat `fan` as an identifier for member access
                let span = Some(self.current_span());
                self.advance();
                return Ok(Expr::new(self.next_id(), span, ExprKind::Ident { name: sym("fan") }));
            }
            self.advance();
            return self.parse_fan_block();
        }
        if self.check(TokenType::LBrace) {
            return self.parse_brace_expr();
        }
        if self.check(TokenType::LBracket) {
            return self.parse_list_expr();
        }
        if self.check(TokenType::LParen) {
            return self.parse_paren_expr();
        }
        if self.check(TokenType::TypeName) {
            return self.parse_type_name_expr();
        }
        // Check hint system for rejected operators/keywords
        if let Some(result) = self.check_hint(None, super::hints::HintScope::Expression) {
            let msg = result.message.unwrap_or_else(|| format!("'{}' is not valid here", tok.value));
            return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
        }
        if self.check(TokenType::Ident) || self.check(TokenType::Ident) {
            let name = sym(&tok.value);
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Ident { name }));
        }

        Err(format!(
            "Expected expression at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn parse_paren_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        if self.peek_paren_lambda() {
            return self.parse_paren_lambda();
        }
        let open = self.current().clone();
        self.advance();
        if self.check(TokenType::RParen) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unit));
        }
        let first = self.parse_expr()?;
        if self.check(TokenType::Comma) {
            let mut elements = vec![first];
            while self.check(TokenType::Comma) {
                self.advance();
                if self.check(TokenType::RParen) { break; }
                elements.push(self.parse_expr()?);
            }
            self.expect_closing(TokenType::RParen, open.line, open.col, "tuple")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Tuple { elements }));
        }
        self.expect_closing(TokenType::RParen, open.line, open.col, "parenthesized expression")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Paren { expr: Box::new(first) }))
    }

    fn parse_type_name_expr(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();
        let span = Some(Span { line: tok.line, col: tok.col, end_col: tok.end_col });
        let name = sym(&tok.value);
        self.advance();

        if self.check(TokenType::LBracket) {
            let ta = self.parse_type_args()?;
            if self.check(TokenType::LParen) {
                let open_call = self.current().clone();
                self.advance();
                let (args, named_args) = self.parse_call_args()?;
                self.expect_closing(TokenType::RParen, open_call.line, open_call.col, "constructor call")?;
                return Ok(Expr::new(self.next_id(), span, ExprKind::Call {
                    callee: Box::new(Expr::new(self.next_id(), span, ExprKind::TypeName { name })),
                    args, named_args, type_args: Some(ta),
                }));
            }
            return Ok(Expr::new(self.next_id(), span, ExprKind::TypeName { name }));
        }
        if self.check(TokenType::LParen) {
            let open_call = self.current().clone();
            self.advance();
            let (args, named_args) = self.parse_call_args()?;
            self.expect_closing(TokenType::RParen, open_call.line, open_call.col, "constructor call")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Call {
                callee: Box::new(Expr::new(self.next_id(), span, ExprKind::TypeName { name })),
                args, named_args, type_args: None,
            }));
        }
        // Named record: Foo {x: 1, y: 2} or Foo { ...base, x: 1 }
        // Peek past optional newlines to check for { Ident : or { ...spread
        if self.peek_named_record() {
            self.skip_newlines();
            let open_rec = self.current().clone();
            self.advance(); // skip {
            self.skip_newlines();
            // Spread record: Foo { ...base, field: value }
            if self.check(TokenType::DotDotDot) {
                self.advance(); // skip ...
                let base = self.parse_expr()?;
                let mut fields = Vec::new();
                while self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                    if self.check(TokenType::RBrace) { break; }
                    let field_name = self.expect_ident()?;
                    self.expect(TokenType::Colon)?;
                    self.skip_newlines();
                    let field_value = self.parse_expr()?;
                    fields.push(FieldInit { name: field_name, value: field_value });
                }
                self.skip_newlines();
                self.expect_closing(TokenType::RBrace, open_rec.line, open_rec.col, "spread record")?;
                return Ok(Expr::new(self.next_id(), span, ExprKind::SpreadRecord {
                    base: Box::new(base), fields,
                }));
            }
            // Regular named record: Foo { x: 1, y: 2 }
            let mut fields = Vec::new();
            while !self.check(TokenType::RBrace) {
                self.skip_newlines();
                let field_name = self.expect_any_name()?;
                if self.check(TokenType::Colon) {
                    self.advance();
                    self.skip_newlines();
                    let field_value = self.parse_expr()?;
                    fields.push(FieldInit { name: field_name, value: field_value });
                } else {
                    fields.push(FieldInit {
                        name: field_name.clone(),
                        value: Expr::new(self.next_id(), None, ExprKind::Ident { name: field_name }),
                    });
                }
                self.skip_newlines();
                if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
            }
            self.expect_closing(TokenType::RBrace, open_rec.line, open_rec.col, "record construction")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Record { name: Some(name), fields }));
        }
        Ok(Expr::new(self.next_id(), span, ExprKind::TypeName { name }))
    }

    fn parse_while_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // skip 'while'
        self.skip_newlines();
        let cond = self.parse_expr()?;
        self.skip_newlines();
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines_into_stmts(&mut stmts);
        while !self.check(TokenType::RBrace) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines_into_stmts(&mut stmts);
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines_into_stmts(&mut stmts);
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "while body")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::While {
            cond: Box::new(cond), body: stmts,
        }))
    }

    fn parse_for_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // skip 'for'
        let (var_name, var_tuple) = if self.check(TokenType::LParen) {
            self.advance();
            let mut names = vec![self.expect_ident_or_underscore()?];
            while self.check(TokenType::Comma) {
                self.advance();
                names.push(self.expect_ident_or_underscore()?);
            }
            self.expect(TokenType::RParen)?;
            (names[0], Some(names))
        } else if self.check(TokenType::Underscore) {
            self.advance();
            (sym("_"), None)
        } else {
            (self.expect_ident()?, None)
        };
        self.expect(TokenType::In)?;
        let iterable = self.parse_expr()?;
        let open_for = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_newlines_into_stmts(&mut stmts);
        while !self.check(TokenType::RBrace) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines_into_stmts(&mut stmts);
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines_into_stmts(&mut stmts);
            }
        }
        self.expect_closing(TokenType::RBrace, open_for.line, open_for.col, "for body")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::ForIn {
            var: var_name, var_tuple, iterable: Box::new(iterable), body: stmts,
        }))
    }

    fn parse_interpolation_parts(&mut self, template: &str, str_line: usize, str_col: usize) -> Result<Vec<StringPart>, String> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;
        // Track column offset: opening " is at str_col, content starts at str_col+1
        let mut col_offset = 0usize;

        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                if !lit.is_empty() {
                    parts.push(StringPart::Lit { value: std::mem::take(&mut lit) });
                }
                let expr_col_start = col_offset + 2; // past ${
                i += 2; // skip ${
                col_offset += 2;
                let mut depth = 1;
                let mut expr_str = String::new();
                while i < chars.len() && depth > 0 {
                    if chars[i] == '{' { depth += 1; }
                    if chars[i] == '}' { depth -= 1; if depth == 0 { break; } }
                    expr_str.push(chars[i]);
                    i += 1;
                    col_offset += 1;
                }
                i += 1; // skip }
                col_offset += 1;
                // Sub-parse the expression with current id counter
                let mut tokens = crate::lexer::Lexer::tokenize(&expr_str);
                // Adjust spans: sub-lexer produces line=1,col=1-based; remap to parent source
                for t in &mut tokens {
                    t.line = str_line;
                    // col: sub-lexer 1-based → 0-based offset + parent string position
                    // str_col is the opening quote col, +1 for quote char, + template offset
                    t.col = str_col + 1 + expr_col_start + (t.col - 1);
                }
                let id_offset = self.expr_id_counter();
                let mut sub_parser = super::Parser::new_with_id_offset(tokens, id_offset);
                match sub_parser.parse_single_expr() {
                    Ok(parsed) => {
                        // Advance our id counter past sub-parser's allocations
                        self.next_expr_id = sub_parser.expr_id_counter();
                        parts.push(StringPart::Expr { expr: Box::new(parsed) });
                    }
                    Err(e) => {
                        // Error recovery: keep as literal, report diagnostic
                        let mut diag = crate::diagnostic::Diagnostic::error(
                            format!("invalid expression in interpolation: {}", e),
                            "Check the expression syntax inside ${...}",
                            format!("${{{}}}", expr_str),
                        );
                        diag.file = self.file.clone();
                        diag.line = Some(str_line);
                        diag.col = Some(str_col + 1 + expr_col_start);
                        self.errors.push(diag);
                        parts.push(StringPart::Lit { value: format!("${{{}}}", expr_str) });
                    }
                }
            } else {
                col_offset += 1;
                lit.push(chars[i]);
                i += 1;
            }
        }
        if !lit.is_empty() {
            parts.push(StringPart::Lit { value: lit });
        }
        Ok(parts)
    }
}
