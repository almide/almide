use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::sym;
use super::Parser;

impl Parser {
    pub(crate) fn parse_if_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::If)?;
        self.skip_newlines();
        // `if let name = scrutinee { then } else { else_ }` — Swift-style implicit
        // unwrap (the frontend desugars to a Some/Ok match). The scrutinee is parsed
        // with `parse_or` so a trailing `{` opens the then-block (not a record), the
        // same trick `match` uses for its subject.
        if self.check(TokenType::Let) {
            self.expect(TokenType::Let)?;
            self.skip_newlines();
            let name = self.expect_ident()?;
            self.skip_newlines();
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let scrutinee = self.parse_or()?;
            self.skip_newlines();
            let then = self.parse_brace_expr()?;
            self.skip_newlines();
            self.expect(TokenType::Else)?;
            self.skip_newlines();
            let else_ = self.parse_brace_expr()?;
            return Ok(Expr::new(self.next_id(), span, ExprKind::IfLet {
                name,
                scrutinee: Box::new(scrutinee),
                then: Box::new(then),
                else_: Box::new(else_),
            }));
        }
        self.parse_if_rest(span)
    }

    /// The condition, the branches and the `else` chain after `if`.
    fn parse_if_rest(&mut self, span: Option<crate::ast::Span>) -> Result<Expr, String> {
        let cond = self.parse_expr()?;
        let branch_on_next_line = self.check(TokenType::Newline);
        self.skip_newlines();
        // `if (c) { a } else { b }` (C / Rust / JS): one error naming `then`,
        // and the braces parse as the branches — stopping here lost the parse
        // at `else` and failed the enclosing fn's type as well.
        if self.check(TokenType::LBrace) {
            let diag = self.diag_error(
                "if requires 'then', not a brace block",
                "Write `if cond then a else b`. Braces make a block value: `if cond then { … } else { … }`.",
                "if",
            ).with_try("if x > 0 then 1 else 2");
            self.errors.push(diag);
        } else if branch_on_next_line && !self.check(TokenType::Then) {
            // `if c` with the branch on the next line (Python / Ruby): one
            // error, and the branch parses as the `then` it means.
            let diag = self.diag_error(
                "if requires 'then' before its branch",
                "Write `if cond then a else b`; the branch may go on the next line after `then`.",
                "if",
            ).with_try("if x > 0 then 1 else 2");
            self.errors.push(diag);
        } else {
            self.expect(TokenType::Then)?;
        }
        self.skip_newlines();
        let then = self.parse_if_branch()?;
        self.skip_newlines();
        let else_ = if self.check(TokenType::Else) {
            self.advance();
            self.skip_newlines();
            self.parse_if_branch()?
        } else if let Some(guide) = self.elif_guide() {
            // `elif` / `elsif` / `elseif` (Python / Ruby / PHP): one error
            // naming `else if`, and the chain goes on as the `else if` it
            // means — stopping here lost the parse and the fn's type with it.
            let diag = self.diag_error(guide.message.unwrap_or_default(), guide.hint, "if")
                .with_try("if x > 0 then 1 else if x < 0 then 2 else 3");
            self.errors.push(diag);
            let elif_span = Some(self.current_span());
            self.advance();
            self.skip_newlines();
            self.parse_if_rest(elif_span)?
        } else {
            Expr::new(self.next_id(), span.clone(), ExprKind::Unit)
        };
        Ok(Expr::new(self.next_id(), span, ExprKind::If {
            cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_),
        }))
    }

    /// The syntax guide's diagnosis of an `elif` / `elsif` / `elseif` here.
    fn elif_guide(&self) -> Option<super::hints::HintResult> {
        let tok = self.current();
        if tok.token_type != TokenType::Ident || !matches!(tok.value.as_str(), "elif" | "elsif" | "elseif") {
            return None;
        }
        super::hints::syntax_guide::check_rejected_ident(tok.value.as_str())
    }

    /// A `then` / `else` branch: an expression, or — unbraced — any
    /// assignment a statement takes (#3757). The branch defers to the
    /// statement parser's own target recognition (`x = v`, and every place
    /// target `try_parse_place_assign` accepts: `xs[i] = v`, `o.ui.dirty = v`,
    /// `o.ui.xs[i] = v`) instead of a shorter lookahead of its own, which
    /// knew only the one-level forms and sent a nested target down the
    /// expression path to die as "Assignments return Unit". A place prefix
    /// that turns out not to be assigned (`xs[i]`, `o.f(x)`) backtracks and
    /// parses as the expression it is.
    fn parse_if_branch(&mut self) -> Result<Expr, String> {
        let named = self.check(TokenType::Ident) || self.check(TokenType::TypeName);
        let place = matches!(
            self.peek_at(1).map(|t| &t.token_type),
            Some(TokenType::Eq | TokenType::LBracket | TokenType::Dot)
        );
        if !(named && place) {
            return self.parse_expr();
        }
        let span = Some(self.current_span());
        match self.parse_stmt()? {
            Stmt::Expr { expr, .. } => Ok(expr),
            stmt => Ok(Expr::new(self.next_id(), span, ExprKind::Block { stmts: vec![stmt], expr: None })),
        }
    }

    pub(crate) fn parse_match_expr(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        self.expect(TokenType::Match)?;
        self.skip_newlines();
        let subject = self.parse_block_head(|p| p.parse_or())?;
        self.skip_newlines();
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let (mut leading, _) = self.skip_newlines_collect_comments();
        let mut arms = Vec::new();
        while !self.check(TokenType::RBrace) {
            let mut arm = self.parse_match_arm()?;
            arm.comments = std::mem::take(&mut leading);
            arms.push(arm);
            let (l, _) = self.skip_newlines_collect_comments();
            leading = l;
            if self.check(TokenType::Comma) {
                self.advance();
                let (more, _) = self.skip_newlines_collect_comments();
                leading.extend(more);
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "match block")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Match {
            subject: Box::new(subject), arms,
        }))
    }

    pub(crate) fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        self.skip_case_prefix();
        let pattern = self.parse_pattern()?;
        // #1461: or-pattern alternatives — `a | b | c => body`.
        let pattern = if self.check(TokenType::Pipe) {
            let mut alts = vec![pattern];
            while self.check(TokenType::Pipe) {
                self.advance();
                self.skip_newlines();
                alts.push(self.parse_pattern()?);
            }
            crate::ast::Pattern::Or { alts }
        } else {
            pattern
        };
        let guard = if self.check(TokenType::If) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect_arm_arrow()?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(MatchArm { pattern, guard, body, comments: Vec::new() })
    }

    /// `case p => e` (Scala / Swift): one error, and the arm parses without
    /// the `case`. A pattern that binds the name `case` (`case => …`) is not
    /// a prefix and stays.
    fn skip_case_prefix(&mut self) {
        if !self.check_ident("case") { return; }
        let binds_case = self.peek_at(1).is_some_and(|t| matches!(t.token_type,
            TokenType::FatArrow | TokenType::If | TokenType::Pipe | TokenType::Comma | TokenType::Newline));
        if binds_case { return; }
        let diag = self.diag_error(
            "'case' is not Almide: a match arm is 'pattern => expr'",
            "Drop the `case`: `match x { 1 => \"one\", _ => \"other\" }`.",
            "match",
        ).with_try("match x { 1 => 2, _ => 3 }");
        self.errors.push(diag);
        self.advance();
    }

    /// The arm's `=>`. `->` (Haskell / OCaml / Kotlin) and `:` (Python dicts)
    /// are one error each, and the arm goes on as `=>` — stopping here lost
    /// the parse and failed the fn's type too.
    fn expect_arm_arrow(&mut self) -> Result<(), String> {
        // `head :: tail` (a Haskell / OCaml cons pattern) is its own hint,
        // not an arm arrow.
        let cons = self.peek_at(1).is_some_and(|t| t.token_type == TokenType::Colon);
        let got = match self.current().token_type {
            TokenType::Arrow => "->",
            TokenType::Colon if !cons => ":",
            _ => return self.expect(TokenType::FatArrow).map(|_| ()),
        };
        let diag = self.diag_error(
            format!("'{got}' is not a match arm arrow: use '=>'"),
            "A match arm is `pattern => expr`. `->` is the return type arrow in `fn f(x: Int) -> Int`.",
            "match",
        ).with_try("match x { 1 => 2, _ => 3 }");
        self.errors.push(diag);
        self.advance();
        Ok(())
    }

    pub(crate) fn parse_paren_lambda(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.parse_lambda_param()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_lambda_param()?);
            }
        }
        self.expect_closing(TokenType::RParen, open.line, open.col, "lambda parameters")?;
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Lambda {
            params, body: Box::new(body),
        }))
    }


    pub(crate) fn parse_lambda_param(&mut self) -> Result<LambdaParam, String> {
        if self.check(TokenType::LParen) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(TokenType::RParen) {
                names.push(self.expect_ident_or_underscore()?);
                if self.check(TokenType::Comma) { self.advance(); }
            }
            self.expect(TokenType::RParen)?;
            let first = names.first().copied().unwrap_or_else(|| sym(""));
            return Ok(LambdaParam { name: first, tuple_names: Some(names), ty: None });
        }
        let name = self.expect_ident_or_underscore()?;
        let ty = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        Ok(LambdaParam { name, tuple_names: None, ty })
    }

    pub(crate) fn parse_fan_block(&mut self) -> Result<Expr, String> {
        let span = Some(self.current_span());
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        let mut exprs = Vec::new();
        self.skip_newlines();
        if self.at_record_field_head() {
            return self.parse_fan_record_fields(span, open);
        }
        while !self.check(TokenType::RBrace) && !self.check(TokenType::EOF) {
            // fan blocks only allow expressions — reject statements
            let tok = self.current().clone();
            match tok.token_type {
                TokenType::Let | TokenType::Var => {
                    return Err(format!("`{}` is not allowed inside fan block at line {}:{}\n  Hint: fan blocks only contain expressions, not statements", tok.value, tok.line, tok.col));
                }
                TokenType::For | TokenType::While => {
                    return Err(format!("`{}` is not allowed inside fan block at line {}:{}\n  Hint: fan blocks only contain expressions, not loops", tok.value, tok.line, tok.col));
                }
                _ => {}
            }
            let expr = self.parse_expr()?;
            exprs.push(expr);
            self.skip_newlines();
            // Same separator rule as every fan arm block: `,`/newline between
            // parallel siblings; `;` is a targeted error (it means sequencing).
            if self.check(TokenType::Semicolon) {
                return Err(self.fan_arm_semicolon_error("fan block"));
            }
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "fan block")?;
        if exprs.is_empty() {
            return Err(format!("fan block must contain at least one expression at line {}:{}", open.line, open.col));
        }
        Ok(Expr::new(self.next_id(), span, ExprKind::Fan {
            exprs,
            names: Vec::new(),
        }))
    }

    /// `name:` — a record field head (the field name may be a soft keyword,
    /// as in the record literal: `{ ok: … }`).
    fn at_record_field_head(&self) -> bool {
        (self.check(TokenType::Ident) || self.is_soft_keyword_name())
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
    }

    /// The record form `fan { name: expr, … }` (#1388), entered after `{` when
    /// the first arm is a field head. Every arm must be a field — the form is
    /// the record literal's, minus punning (`fan { x }` is the one-arm tuple
    /// form) — and a name may appear once. Same separators and statement ban
    /// as the tuple form.
    fn parse_fan_record_fields(&mut self, span: Option<Span>, open: crate::lexer::Token) -> Result<Expr, String> {
        let mut exprs = Vec::new();
        let mut names: Vec<crate::intern::Sym> = Vec::new();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::EOF) {
            let tok = self.current().clone();
            if !self.at_record_field_head() {
                return Err(format!(
                    "every arm of a record-form fan block must be a `name: expr` field at line {}:{} (got {:?} '{}')\n  Hint: `fan {{ a: f(), b: g() }}` names every arm; to collect a tuple instead, write no names at all: `fan {{ f(), g() }}`",
                    tok.line, tok.col, tok.token_type, tok.value
                ));
            }
            let name = self.expect_any_name()?;
            if names.contains(&name) {
                return Err(format!(
                    "field `{}` appears twice in this fan block at line {}:{}\n  Hint: each field of the record a fan block builds is named once",
                    name.as_str(), tok.line, tok.col
                ));
            }
            self.expect(TokenType::Colon)?;
            self.skip_newlines();
            let head = self.current().clone();
            if matches!(head.token_type, TokenType::Let | TokenType::Var | TokenType::For | TokenType::While) {
                return Err(format!("`{}` is not allowed inside fan block at line {}:{}\n  Hint: a field's value is an expression — wrap statements in a block: {{ let x = f(); g(x) }}", head.value, head.line, head.col));
            }
            exprs.push(self.parse_expr()?);
            names.push(name);
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                return Err(self.fan_arm_semicolon_error("fan block"));
            }
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "fan block")?;
        Ok(Expr::new(self.next_id(), span, ExprKind::Fan { exprs, names }))
    }

}
