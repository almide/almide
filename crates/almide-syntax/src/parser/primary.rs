use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::sym;
use super::Parser;

impl Parser {
    /// The primary dispatcher — four token families in order (literal,
    /// constructor, control head, grouping), then the hint system, plain
    /// identifiers, and the error tail. Each family returns `None` for
    /// "not mine" so the chain reads like the token classes it covers.
    pub(crate) fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();
        let span = Some(Span { line: tok.line, col: tok.col, end_col: tok.end_col });

        if let Some(r) = self.parse_primary_literal(&tok, span) {
            return r;
        }
        if let Some(r) = self.parse_primary_ctor(&tok, span) {
            return r;
        }
        if let Some(r) = self.parse_primary_control() {
            return r;
        }
        if let Some(r) = self.parse_primary_grouping() {
            return r;
        }
        // Check hint system for rejected operators/keywords
        if let Some(result) = self.check_hint(None, super::hints::HintScope::Expression) {
            let msg = result.message.unwrap_or_else(|| format!("'{}' is not valid here", tok.value));
            return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
        }
        if self.check(TokenType::Ident) {
            let name = sym(&tok.value);
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Ident { name }));
        }

        self.parse_primary_error(&tok)
    }

    /// Literal tokens: one token in, one leaf node out.
    fn parse_primary_literal(&mut self, tok: &crate::lexer::Token, span: Option<Span>) -> Option<Result<Expr, String>> {
        let kind = match tok.token_type {
            TokenType::Int => {
                self.advance();
                return Some(Ok(self.parse_int_literal(tok, span)));
            }
            TokenType::Float => {
                let v: f64 = tok.value.replace('_', "").parse().unwrap_or(0.0);
                // A Float token's `value` IS its source spelling (the lexer
                // never decodes numbers), so it doubles as `raw`.
                ExprKind::Float { value: v, raw: Some(tok.value.clone()) }
            }
            TokenType::String => ExprKind::String { value: tok.value.clone(), raw: tok.raw.clone() },
            TokenType::InterpolatedString => {
                self.advance();
                let parts = match self.parse_interpolation_parts(&tok.value, tok.line, tok.col, tok.raw.as_deref()) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                return Some(Ok(Expr::new(self.next_id(), span, ExprKind::InterpolatedString { parts, raw: tok.raw.clone() })));
            }
            TokenType::True => ExprKind::Bool { value: true },
            TokenType::False => ExprKind::Bool { value: false },
            TokenType::Underscore => ExprKind::Hole,
            TokenType::Break => ExprKind::Break,
            TokenType::Continue => ExprKind::Continue,
            TokenType::None => ExprKind::None,
            _ => return Option::None,
        };
        self.advance();
        Some(Ok(Expr::new(self.next_id(), span, kind)))
    }

    /// Constructor keywords: `some(..)` / `ok(..)` / `err(..)` / `todo(..)`.
    fn parse_primary_ctor(&mut self, tok: &crate::lexer::Token, span: Option<Span>) -> Option<Result<Expr, String>> {
        match tok.token_type {
            TokenType::Some => Some(self.parse_some_expr(span)),
            TokenType::Ok => Some(self.parse_ok_expr(span)),
            TokenType::Err => Some(self.parse_err_expr(span)),
            TokenType::Todo => Some(self.parse_todo_expr(span)),
            _ => Option::None,
        }
    }

    /// Control-flow heads: if / match / while / for / fan (and the removed
    /// `do` blocks, which get a targeted error).
    fn parse_primary_control(&mut self) -> Option<Result<Expr, String>> {
        if self.check(TokenType::If) {
            return Some(self.parse_if_expr());
        }
        if self.check(TokenType::Match) {
            return Some(self.parse_match_expr());
        }
        if self.check(TokenType::While) {
            return Some(self.parse_while_expr());
        }
        if self.check(TokenType::For) {
            return Some(self.parse_for_expr());
        }
        if self.check_ident("do") {
            let span = self.current_span();
            self.advance();
            return Some(Err(format!("`do` blocks have been removed — use `while` for loops or remove `do` from effect fn bodies (line {})", span.line)));
        }
        if self.check(TokenType::Fan) {
            return Some(self.parse_fan_primary());
        }
        if self.at_scoped_block_head() {
            return Some(self.parse_scoped_block());
        }
        Option::None
    }

    /// Grouping openers: block / list / paren / type-name expression.
    /// (Backtick-escaped keywords are lexed as Ident, so they reach the
    /// normal Ident path in the dispatcher — no special handling here.)
    fn parse_primary_grouping(&mut self) -> Option<Result<Expr, String>> {
        if self.check(TokenType::LBrace) {
            return Some(self.parse_brace_expr());
        }
        if self.check(TokenType::LBracket) {
            return Some(self.parse_list_expr());
        }
        if self.check(TokenType::LParen) {
            return Some(self.parse_paren_expr());
        }
        if self.check(TokenType::TypeName) {
            return Some(self.parse_type_name_expr());
        }
        Option::None
    }

    /// Builds the "no primary matched" error, including targeted diagnostics
    /// for two common LLM/other-language mistakes: ML-style `let .. in ..`
    /// expressions, and a bare `=` in expression position (chained/misplaced
    /// assignment). Falls back to a generic "Expected expression" error.
    fn parse_primary_error(&mut self, tok: &crate::lexer::Token) -> Result<Expr, String> {
        if let Some(error) = self.reject_retired_range(
            "a prefix expression", "Ranges require a start: `start..<end` or `start...end`. Calls do not support argument spread. To combine lists use `xs + [item]`; `..rest` is a list-pattern rest marker in the final slot.")
        {
            return Err(error);
        }
        // `let x = expr in body` — ML-style let-in expression
        if tok.token_type == TokenType::Let {
            let msg = "'let' is not an expression in Almide";
            let hint = "Lists are immutable — use `+` to build a new list: `some(stack + [item])`. \
                        If you need a temporary binding, use a block: `{ let x = expr; body }`";
            let diag = self.diag_error(msg, hint, "let-in");
            self.errors.push(diag);
            return Err(format!("{} at line {}:{}", msg, tok.line, tok.col));
        }

        // Bare `=` in expression position usually means the user tried to
        // chain assignment (`let r = x = 5`) or start an assignment where
        // an expression is required. Almide assignments are statements that
        // return Unit, so they can't appear on the right of `let`.
        if tok.token_type == TokenType::Eq {
            let msg = "Assignments return Unit and can't appear here";
            let hint = "Almide assignment `x = 5` is a statement, not an expression. \
                        Use separate statements: `x = 5; let r = x` — or pick the value \
                        directly: `let r = 5`.";
            let diag = self.diag_error(msg, hint, "assignment-in-expr");
            self.errors.push(diag);
            return Err(format!("{} at line {}:{}", msg, tok.line, tok.col));
        }

        // A character the lexer could not tokenize (full-width punctuation,
        // invisible Unicode, ...). Dedicated wording — "(got Unknown '。')"
        // reads like a compiler bug, not a source bug (#1308).
        if tok.token_type == TokenType::Unknown {
            return Err(self.unknown_char_error(&tok.value.clone(), tok.line, tok.col));
        }

        Err(format!(
            "Expected expression at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn parse_int_literal(&mut self, tok: &crate::lexer::Token, span: Option<Span>) -> Expr {
        let clean = tok.value.replace('_', "");
        let parsed: i64 = if clean.starts_with("0x") || clean.starts_with("0X") {
            i64::from_str_radix(&clean[2..], 16).unwrap_or(0)
        } else {
            clean.parse().unwrap_or(0)
        };
        Expr::new(self.next_id(), span, ExprKind::Int {
            value: serde_json::Value::Number(
                serde_json::Number::from_f64(parsed as f64)
                    .unwrap_or_else(|| serde_json::Number::from(0)),
            ),
            raw: tok.value.clone(),
        })
    }


    /// #1111: a bare builtin ctor (`some` / `ok` / `err`, no parens) IS a
    /// function value — synthesized as its eta-expansion
    /// `(x) => ctor(x)`, so every consumer (checker, both backends, the
    /// interp) sees the lambda form that already works. The parameter
    /// name is gensym-ish (`__ctor_arg`) — lowering assigns fresh VarIds,
    /// so collision with user names is impossible by construction.
    fn bare_ctor_fn_value(
        &mut self,
        span: Option<Span>,
        make: fn(Box<Expr>) -> ExprKind,
    ) -> Expr {
        let arg = almide_base::intern::sym("__ctor_arg");
        let var = Expr::new(self.next_id(), span, ExprKind::Ident { name: arg });
        let body = Expr::new(self.next_id(), span, make(Box::new(var)));
        Expr::new(self.next_id(), span, ExprKind::Lambda {
            params: vec![crate::ast::LambdaParam { name: arg, tuple_names: None, ty: None }],
            body: Box::new(body),
        })
    }

    fn parse_some_expr(&mut self, span: Option<Span>) -> Result<Expr, String> {
        self.advance();
        let open = self.current().clone();
        // #1111: a bare `some` (no parens) is the ctor as a FUNCTION
        // VALUE — `list.map(xs, some)` — synthesized as `(x) => some(x)`.
        if open.token_type != TokenType::LParen {
            return Ok(self.bare_ctor_fn_value(span, |e| ExprKind::Some { expr: e }));
        }
        self.expect(TokenType::LParen)?;
        let expr = self.parse_expr()?;
        self.expect_closing(TokenType::RParen, open.line, open.col, "some()")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Some { expr: Box::new(expr) }))
    }

    fn parse_ok_expr(&mut self, span: Option<Span>) -> Result<Expr, String> {
        self.advance();
        let open = self.current().clone();
        // #1111: a bare `ok` is the ctor as a function value.
        if open.token_type != TokenType::LParen {
            return Ok(self.bare_ctor_fn_value(span, |e| ExprKind::Ok { expr: e }));
        }
        self.expect(TokenType::LParen)?;
        let expr = self.parse_expr()?;
        self.expect_closing(TokenType::RParen, open.line, open.col, "ok()")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Ok { expr: Box::new(expr) }))
    }

    fn parse_err_expr(&mut self, span: Option<Span>) -> Result<Expr, String> {
        self.advance();
        let open = self.current().clone();
        // #1111: a bare `err` is the ctor as a function value.
        if open.token_type != TokenType::LParen {
            return Ok(self.bare_ctor_fn_value(span, |e| ExprKind::Err { expr: e }));
        }
        self.expect(TokenType::LParen)?;
        let expr = self.parse_expr()?;
        self.expect_closing(TokenType::RParen, open.line, open.col, "err()")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Err { expr: Box::new(expr) }))
    }

    fn parse_todo_expr(&mut self, span: Option<Span>) -> Result<Expr, String> {
        self.advance();
        let open = self.current().clone();
        self.expect(TokenType::LParen)?;
        let msg = self.current().value.clone();
        self.expect(TokenType::String)?;
        self.expect_closing(TokenType::RParen, open.line, open.col, "todo()")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Todo { message: msg }))
    }

    /// At the COMMA of an over-arity fan head `fan.X(a, …)`: consume the rest
    /// of the argument list and report whether it ENDS in a 1-param lambda —
    /// the declared MAPPER spelling — vs the retired thunk spelling (0-param
    /// lambdas). Only called on error paths (every caller returns Err either
    /// way), so the token consumption never leaks into a successful parse.
    fn parse_paren_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        if self.peek_paren_lambda() {
            return self.parse_paren_lambda();
        }
        let open = self.current().clone();
        self.advance();
        // A parenthesized/tuple literal may span lines (#1570) — the same
        // delimited-context rule every other bracketed literal (list, record,
        // map) already follows: newlines are insignificant between `(` and
        // `)`, around elements and after commas.
        let pending = self.skip_newlines_collecting();
        if self.check(TokenType::RParen) {
            self.advance();
            return Ok(Expr::new(self.next_id(), span, ExprKind::Unit));
        }
        let first = self.parse_expr()?;
        self.attach_leading_comments(first.id, pending);
        self.skip_newlines();
        // Type ascription inside parens: `(expr: Type)` — e.g. `([]: List[String])`.
        // The bare call-arg form `[]: T` is accepted as a call argument, but a
        // record-field value (`{ tags: ([]: List[String]) }`) or a `let`
        // initializer must parenthesize it; without this the `:` after the inner
        // expr was an unexpected token ("Expected ')'"). Mirrors the call-arg
        // ascription (`parser/expressions.rs`); `::` is excluded so a path token
        // never trips it.
        if self.check(TokenType::Colon)
            && self.peek_at(1).map(|t| &t.token_type) != Some(&TokenType::Colon)
        {
            let asc_span = first.span;
            self.advance(); // skip ':'
            let ty = self.parse_type_expr()?;
            self.expect_closing(TokenType::RParen, open.line, open.col, "type-ascribed expression")?;
            return Ok(Expr::new(self.next_id(), asc_span, ExprKind::TypeAscription {
                expr: Box::new(first),
                ty,
            }));
        }
        if self.check(TokenType::Comma) {
            let mut elements = vec![first];
            while self.check(TokenType::Comma) {
                self.advance();
                let pending = self.skip_newlines_collecting();
                if self.check(TokenType::RParen) { break; }
                let element = self.parse_expr()?;
                self.attach_leading_comments(element.id, pending);
                elements.push(element);
                self.skip_newlines();
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

        // `Name[...]` is a type application ONLY when the brackets name a type
        // and a call follows (`Pair[Int, String](a, b)`). Uppercase VALUE
        // bindings are house style (`let STATIONS = [...]`), so `STATIONS[i]`
        // must stay an index: fall through to the bare TypeName and let the
        // postfix loop read the brackets, with the same gate the postfix
        // position already uses for `fs[0]()` (#1142).
        if self.check(TokenType::LBracket) && self.peek_type_args_call() {
            let ta = self.parse_type_args()?;
            let open_call = self.current().clone();
            self.advance();
            let (args, named_args) = self.parse_call_args()?;
            self.expect_closing(TokenType::RParen, open_call.line, open_call.col, "constructor call")?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::Call {
                callee: Box::new(Expr::new(self.next_id(), span, ExprKind::TypeName { name })),
                args, named_args, type_args: Some(ta),
            }));
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
            let pending = self.skip_newlines_collecting();
            if self.check(TokenType::DotDotDot) {
                return self.parse_spread_record(span, open_rec, pending);
            }
            let mut record = self.parse_record_literal(span, open_rec, pending)?;
            if let ExprKind::Record { name: record_name, .. } = &mut record.kind { *record_name = Some(name); }
            return Ok(record);
        }
        Ok(Expr::new(self.next_id(), span, ExprKind::TypeName { name }))
    }

    fn parse_while_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.advance(); // skip 'while'
        self.skip_newlines();
        let cond = self.parse_block_head(|p| p.parse_expr())?;
        self.skip_newlines();
        // Detect `while cond do ... done` (Pascal/Ruby). Almide uses `{ ... }`.
        // Match `do` whether on the same line as `cond` or the next line —
        // LLMs write both forms. Dojo data on binary-search / matrix-ops
        // fails frequently on this pattern, and the right fix is usually
        // recursion, not a mutable `while` loop (pure fn).
        if self.check(TokenType::Ident) && self.current().value == "do" {
            let tok = self.current().clone();
            let diag = self.diag_error(
                "`while ... do ... done` is Pascal/Ruby syntax",
                "Almide uses `while cond { ... }` (curly braces). But `while` requires a `var` accumulator — pure/effect fns usually want recursion instead.",
                "while body",
            ).with_try(
                "// Almide `while` needs braces (not `do ... done`):\n\
                 var i = 0\n\
                 while cond(i) { i = i + 1 }\n\
                 \n\
                 // For pure fn, prefer recursion over `var` + while:\n\
                 fn loop(i: Int, acc: T) -> T =\n\
                   if !cond(i) then acc else loop(i + 1, next(acc, i))"
            );
            self.errors.push(diag);
            return Err(format!("`while ... do` is not valid in Almide at line {}:{}", tok.line, tok.col));
        }
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
        let iterable = self.parse_block_head(|p| p.parse_expr())?;
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

    fn parse_interpolation_parts(&mut self, template: &str, str_line: usize, str_col: usize, raw: Option<&str>) -> Result<Vec<StringPart>, String> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let chars: Vec<char> = template.chars().collect();
        let mut i = 0;
        // Track column offset: opening " is at str_col, content starts at str_col+1
        let mut col_offset = 0usize;
        // #2250: the literal's VERBATIM source text (delimiters included), for
        // locating each hole where it really sits. `template` is the decoded
        // value — a heredoc has its blank first line skipped and its common
        // indent stripped — so `col_offset` counts characters that are not
        // on the string's first line at all.
        let raw_chars: Option<Vec<char>> = raw.map(|r| r.chars().collect());
        let mut origin = LiteralOrigin { line: str_line, col: str_col, raw: raw_chars.as_deref(), cursor: 0 };

        while i < chars.len() {
            // #1076: `\\` and `\$` reach this splitter as undecoded pairs
            // (the lexer keeps them so an escaped `\${` — or a literal
            // backslash before a real hole — stays distinguishable from a
            // live hole). Decode them here, before the hole check.
            if chars[i] == '\\' && i + 1 < chars.len() && (chars[i + 1] == '\\' || chars[i + 1] == '$') {
                lit.push(chars[i + 1]);
                i += 2;
                col_offset += 2;
            } else if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                if !lit.is_empty() {
                    parts.push(StringPart::Lit { value: std::mem::take(&mut lit) });
                }
                let part = self.parse_interpolation_expr_part(&chars, &mut i, &mut col_offset, &mut origin);
                parts.push(part);
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

    /// Parses one `${ .. }` interpolation hole, starting at `chars[*i] == '$'`.
    /// Advances `*i`/`*col_offset` past the whole `${..}` span (brace-depth
    /// scan, then sub-parse) and returns the resulting `StringPart` — either
    /// the parsed sub-expression, or (on a parse error) the raw text kept as
    /// a literal with a diagnostic recorded on `self.errors`.
    fn parse_interpolation_expr_part(
        &mut self,
        chars: &[char],
        i: &mut usize,
        col_offset: &mut usize,
        origin: &mut LiteralOrigin<'_>,
    ) -> StringPart {
        let (str_line, str_col) = (origin.line, origin.col);
        let raw_chars = origin.raw;
        let expr_col_start = *col_offset + 2; // past ${
        *i += 2; // skip ${
        *col_offset += 2;
        let mut depth = 1;
        let mut expr_str = String::new();
        while *i < chars.len() && depth > 0 {
            // #1073: a nested string literal is captured atomically — its
            // quotes and braces are literal text, not structure. Delegates
            // to the same scanner the lexer's interpolation scan uses, so
            // the two passes agree on where the literal ends.
            if chars[*i] == '"' || chars[*i] == '\'' {
                let start = *i;
                *i = crate::lexer::scan_nested_string_literal(chars, *i, &mut expr_str);
                *col_offset += *i - start;
                continue;
            }
            if chars[*i] == '{' { depth += 1; }
            if chars[*i] == '}' { depth -= 1; if depth == 0 { break; } }
            expr_str.push(chars[*i]);
            *i += 1;
            *col_offset += 1;
        }
        *i += 1; // skip }
        *col_offset += 1;
        // #2250: where the hole really sits. Counting `col_offset` from the
        // opening quote is right only for a one-line string: in a heredoc the
        // decoded template has lost its blank first line and its common indent,
        // and the offset counts every character of the lines before the hole,
        // so a call on the heredoc's third line was stamped with the string's
        // line and a column past the end of it — and `check --json` handed that
        // position to fix-it harnesses. The hole's own text is verbatim in the
        // source, so find it there (from the previous hole on) and count lines.
        let hole_text: Vec<char> = "${".chars().chain(expr_str.chars()).chain(std::iter::once('}')).collect();
        let anchor = raw_chars.and_then(|raw| locate_hole(raw, origin.cursor, &hole_text, str_line, str_col));
        if let Some((idx, _, _)) = anchor {
            origin.cursor = idx + hole_text.len();
        }
        // Sub-parse the expression with current id counter
        let mut tokens = crate::lexer::Lexer::tokenize(&expr_str);
        // Adjust spans: sub-lexer produces line=1,col=1-based; remap to parent source
        // col: sub-lexer 1-based → 0-based offset + parent string position
        // str_col is the opening quote col, +1 for quote char, + template offset
        let to_parent = |c: usize| str_col + 1 + expr_col_start + (c - 1);
        for t in &mut tokens {
            match anchor {
                // The hole was located: its `$` is at (hole_line, hole_col), the
                // expression starts two columns later, and a hole that itself
                // spans lines puts its later tokens on the following source
                // lines, shifted by the indent the heredoc decoder stripped.
                Some((idx, hole_line, hole_col)) => {
                    if t.line == 1 {
                        t.line = hole_line;
                        t.col = hole_col + 2 + (t.col - 1);
                        t.end_col = hole_col + 2 + (t.end_col - 1);
                    } else {
                        let shift = continuation_shift(raw_chars.unwrap_or(&[]), idx, &expr_str, t.line);
                        t.line = hole_line + t.line - 1;
                        t.col += shift;
                        t.end_col += shift;
                    }
                }
                // No verbatim text to search (an AST fed back from JSON): the
                // first-line arithmetic is all there is.
                Option::None => {
                    t.line = str_line;
                    t.col = to_parent(t.col);
                    // end_col rides the SAME shift (#2095). Remapping only the start left
                    // every token in an interpolation with a parent-space start next to a
                    // sub-string-space end — a number smaller than its own beginning —
                    // which downstream reads as "no end". The cost was not only the
                    // fix-it the issue names: a diagnostic with no measurable end draws a
                    // one-column caret, so `"${list.nope(x)}"` underlined a single column
                    // where the same call outside the string underlined all nine.
                    t.end_col = to_parent(t.end_col);
                }
            }
        }
        let id_offset = self.expr_id_counter();
        let mut sub_parser = super::Parser::new_with_id_offset(tokens, id_offset);
        match sub_parser.parse_single_expr() {
            Ok(parsed) => {
                // Advance our id counter past sub-parser's allocations
                self.next_expr_id = sub_parser.expr_id_counter();
                // RECOVERABLE diagnostics the sub-parse pushed (an E031
                // retired-range fix-it, an E038 fallback rule, …) must
                // surface: dropping them made `"${list.len(1..=4)}"` parse
                // and RUN where the direct spelling errors (sweep
                // 2026-08-18). Token positions were remapped before the
                // sub-parse, so the diagnostics already point into the
                // parent string; only the file name is ours to fill.
                for mut d in std::mem::take(&mut sub_parser.errors) {
                    if d.file.is_none() {
                        d.file = self.file.clone();
                    }
                    self.errors.push(d);
                }
                StringPart::Expr { expr: Box::new(parsed) }
            }
            Err(e) => {
                // Error recovery: keep as literal, report diagnostic
                let mut diag = crate::diagnostic::Diagnostic::error(
                    format!("invalid expression in interpolation: {}", e),
                    "Check the expression syntax inside ${...}",
                    format!("${{{}}}", expr_str),
                );
                diag.file = self.file.clone();
                let (line, col) = match anchor {
                    Some((_, hole_line, hole_col)) => (hole_line, hole_col + 2),
                    Option::None => (str_line, str_col + 1 + expr_col_start),
                };
                diag.line = Some(line);
                diag.col = Some(col);
                self.errors.push(diag);
                StringPart::Lit { value: format!("${{{}}}", expr_str) }
            }
        }
    }
}

/// #2250: where an interpolated literal sits in the source: the line and
/// column of its opening quote, its verbatim text (delimiters included) when
/// the parser has one, and how far into that text the holes located so far
/// reach, so a repeated spelling maps to its own occurrence.
pub(crate) struct LiteralOrigin<'a> {
    pub line: usize,
    pub col: usize,
    pub raw: Option<&'a [char]>,
    pub cursor: usize,
}

/// #2250: find `hole` (a verbatim `${…}`) in the literal's source text `raw`
/// (delimiters included, `raw[0]` being the opening quote at column
/// `str_col` of line `str_line`), searching from char index `from`. Returns
/// the hole's char index and the line and column of its `$`.
fn locate_hole(raw: &[char], from: usize, hole: &[char], str_line: usize, str_col: usize) -> Option<(usize, usize, usize)> {
    if hole.is_empty() || from >= raw.len() { return None; }
    let idx = (from..raw.len().checked_sub(hole.len())? + 1)
        .find(|&j| raw[j..j + hole.len()] == *hole)?;
    let newlines = raw[..idx].iter().filter(|c| **c == '\n').count();
    let col = match raw[..idx].iter().rposition(|c| *c == '\n') {
        Option::None => str_col + idx,
        Some(nl) => idx - nl,
    };
    Some((idx, str_line + newlines, col))
}

/// #2250: how far a token on line `sub_line` (≥ 2) of a multi-line hole moves
/// right in the source: the heredoc decoder stripped the common indent from
/// that continuation line, so the sub-lexer's column is short by exactly the
/// whitespace the source line has and the decoded line has not.
fn continuation_shift(raw: &[char], hole_idx: usize, expr_str: &str, sub_line: usize) -> usize {
    let raw_line_no = raw[..hole_idx.min(raw.len())].iter().filter(|c| **c == '\n').count() + sub_line - 1;
    let raw_line = raw.split(|c| *c == '\n').nth(raw_line_no);
    let decoded_line = expr_str.split('\n').nth(sub_line - 1);
    match (raw_line, decoded_line) {
        (Some(r), Some(d)) => {
            let raw_ws = r.iter().take_while(|c| c.is_whitespace()).count();
            let decoded_ws = d.chars().take_while(|c| c.is_whitespace()).count();
            raw_ws.saturating_sub(decoded_ws)
        }
        _ => 0,
    }
}
