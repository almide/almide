use crate::lexer::TokenType;
use crate::ast::*;
use crate::intern::{sym, Sym};
use super::Parser;

impl Parser {
    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if self.check(TokenType::Let) { return self.parse_let_stmt(); }
        if self.check(TokenType::Var) { return self.parse_var_stmt(); }
        if self.check(TokenType::Guard) { return self.parse_guard_stmt(); }

        // name = value (simple assignment). The target may be an UPPERCASE
        // name too: module globals follow the LIMIT/COUNT convention, and the
        // Ident-only lookahead sent `COUNT = 1` down the expression path to
        // die as "Assignments return Unit" (diagnostic sweep 2026-08-18). A
        // TypeName target that is not an assignable binding is the checker's
        // E003/E009 to report, with the assignment shape intact.
        let assign_target = self.check(TokenType::Ident) || self.check(TokenType::TypeName);
        if assign_target
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_assign_stmt();
        }

        // A place assignment (#3064): `xs[i] = v`, `obj.field = v`, and the
        // nested targets GRAMMAR.md lists — `o.inner.xs = v`, `o.m[k] = v`.
        // A field name may be any token `expect_any_name` accepts (ident,
        // TypeName, or a soft keyword) so `obj.ok = v` routes here, exactly
        // as `obj.ok` reads in expression position.
        if assign_target
            && matches!(self.peek_at(1).map(|t| &t.token_type), Some(TokenType::LBracket | TokenType::Dot))
        {
            if let Some(stmt) = self.try_parse_place_assign()? {
                return Ok(stmt);
            }
        }

        let span = self.current_span();
        let expr = self.parse_expr()?;
        Ok(Stmt::Expr { expr, span: Some(span) })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Let)?;

        // Detect `let rec name(args) = ...` (OCaml / SML / F#). Almide
        // doesn't need `rec` — top-level fns are recursive by default.
        // Only when a name follows: `let rec = …` / `let rec: T = …` bind a
        // local called `rec` (#2643).
        let names_after = self.peek_at(1).is_some_and(|t| t.token_type == TokenType::Ident);
        if self.check(TokenType::Ident) && self.current().value == "rec" && names_after {
            let tok = self.current().clone();
            let diag = self.diag_error(
                "`let rec` is OCaml/SML syntax; Almide functions are recursive by default",
                "Define recursive functions at top level: `fn name(args) -> ReturnType = body`. Almide has no `let rec` — call the fn directly, including from its own body.",
                "let rec",
            ).with_try("fn fact(n: Int) -> Int =\n    if n == 0 then 1 else n * fact(n - 1)");
            self.errors.push(diag);
            return Err(format!("'let rec' is not valid in Almide at line {}:{}", tok.line, tok.col));
        }

        // Record destructuring: let { a, b } = expr. This form is shorthand
        // only — each name becomes BOTH the field label and a usable local — so
        // it stays `expect_ident`: a soft-keyword local (`let { ok } = …`) could
        // be bound but never read (in value position `ok` is the constructor),
        // so accepting it would only enable a dead binding. Soft keywords are
        // names in label/member position (`{ ok: … }`, `.ok`), not as bindings.
        if self.check(TokenType::LBrace) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(TokenType::RBrace) {
                names.push(self.expect_ident()?);
                if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
            }
            self.expect(TokenType::RBrace)?;
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            let fields = names.into_iter()
                .map(|n| FieldPattern { name: n, pattern: None })
                .collect();
            return Ok(Stmt::LetDestructure {
                pattern: Pattern::RecordPattern { name: sym(""), fields, rest: false },
                value, span: Some(span),
            });
        }

        // Tuple destructuring: let (a, b) = expr
        if self.check(TokenType::LParen) {
            let pattern = self.parse_destructure_tuple()?;
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Stmt::LetDestructure { pattern, value, span: Some(span) });
        }

        // Detect `let mut` (Rust style)
        if self.check(TokenType::Mut) {
            return Err(self.check_hint_or_err(
                Some(TokenType::Mut), super::hints::HintScope::Block,
                "'let mut' is not valid in Almide",
            ));
        }

        // Allow `let _ = expr`
        let name = if self.check(TokenType::Underscore) {
            self.advance();
            sym("_")
        } else {
            self.expect_ident()?
        };
        let ty = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        // Detect `let x = expr in <body>` (OCaml/Haskell). Almide lets chain
        // by newline/semicolon inside a block — no `in` keyword.
        // Look across an intervening newline so dojo-observed forms like
        //     let abs_n = int.abs(n)
        //       in if abs_n == 0 ...
        // also trigger the let-in diagnostic instead of falling through to
        // a generic "Expected expression (got In 'in')" parse error.
        self.skip_newlines_if_followed_by(TokenType::In);
        if self.check(TokenType::In) {
            let diag = self.diag_error(
                "`let ... in <expr>` is OCaml/Haskell syntax",
                "In Almide, multiple lets chain by newlines inside a block — no `in` keyword.",
                "let ... in",
            ).with_code("E049");
            let diag = match self.letin_deletion_span() {
                // Machine-applicable: `in` is not a keyword in Almide at
                // all, and the body it introduces is already the
                // newline-chained continuation. Deleting the token yields
                // the identical program — nothing is decided for the author.
                Some((line, col, end_col)) => diag.with_machine_fix(line, col, end_col, ""),
                // No exact span (the `in` is the last token) — keep the
                // display-only snippet rather than guess a range.
                None => diag.with_try("let x = 1\nlet y = 2\nx + y"),
            };
            self.errors.push(diag);
            // Recover: consume `in` and the trailing expression so the partial
            // `Stmt::Let { name, value }` survives in the AST. This lets
            // downstream diagnostics (E001 fn-body Unit-leak) cite the actual
            // binding name in their try: snippet, instead of falling back to
            // a generic <computation> placeholder.
            self.advance(); // consume `in`
            self.skip_newlines();
            let _orphan = self.parse_expr();
        }
        Ok(Stmt::Let { name, ty, value, span: Some(span) })
    }

    fn parse_var_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Var)?;
        let name = self.expect_ident()?;
        let ty = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Var { name, ty, value, span: Some(span) })
    }

    fn parse_guard_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        self.expect(TokenType::Guard)?;
        // `guard let name = scrutinee else { … }` — Swift-style: name binds the unwrapped
        // value for the rest of the block (the frontend desugars the block tail into a
        // Some/Ok match). The scrutinee is followed by `else`, so a full expr is fine.
        if self.check(TokenType::Let) {
            self.expect(TokenType::Let)?;
            self.skip_newlines();
            let name = self.expect_ident()?;
            self.skip_newlines();
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let scrutinee = self.parse_expr()?;
            self.skip_newlines();
            self.expect(TokenType::Else)?;
            self.skip_newlines();
            let else_ = self.parse_expr()?;
            return Ok(Stmt::GuardLet { name, scrutinee, else_, span: Some(span) });
        }
        let cond = self.parse_expr()?;
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let else_ = self.parse_expr()?;
        Ok(Stmt::Guard { cond, else_, span: Some(span) })
    }

    fn parse_assign_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        let name = sym(&self.current().value);
        self.advance();
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr_ascribed()?;
        Ok(Stmt::Assign { name, value, span: Some(span) })
    }

    /// Parse `root(.field | [index])+ = value` when the tokens spell one;
    /// restore and return `None` otherwise (a call, a comparison, a read).
    ///
    /// Supported targets: a chain of fields, optionally ending in ONE index —
    /// `s.f`, `o.inner.xs`, `xs[i]`, `o.m[k]`. An index anywhere but last
    /// (`xs[i].f = v`, `g[i][j] = v`) would need the element read back out of
    /// its container and written again; it is refused here, at the target,
    /// with the rewrite spelled out.
    fn try_parse_place_assign(&mut self) -> Result<Option<Stmt>, String> {
        let saved = self.pos;
        let saved_errors = self.errors.len();
        let span = self.current_span();
        let target = sym(&self.current().value);
        self.advance();
        let mut fields: Vec<Sym> = Vec::new();
        let mut index: Option<Expr> = None;
        let mut misplaced_index: Option<usize> = None;
        loop {
            if self.check(TokenType::Dot)
                && self.peek_at(1).map(|t| Self::is_name_token(&t.token_type)).unwrap_or(false)
            {
                if index.is_some() && misplaced_index.is_none() {
                    misplaced_index = Some(saved);
                }
                self.advance();
                fields.push(self.expect_any_name()?);
            } else if self.check(TokenType::LBracket) {
                if index.is_some() && misplaced_index.is_none() {
                    misplaced_index = Some(saved);
                }
                self.advance();
                let Ok(e) = self.parse_expr() else { break };
                if !self.check(TokenType::RBracket) { break; }
                self.advance();
                index = Some(e);
            } else {
                break;
            }
        }
        let is_assign = self.check(TokenType::Eq)
            && self.peek_at(1).map(|t| &t.token_type) != Some(&TokenType::Eq);
        if !is_assign {
            self.pos = saved;
            self.errors.truncate(saved_errors);
            return Ok(None);
        }
        if let Some(at) = misplaced_index {
            self.pos = at;
            let msg = "an index can only be the last step of an assignment target";
            let hint = "Almide writes a container slot, or a field of a record, in place — not a \
                        field of a slot. Read the element into a `var`, assign to it, and store \
                        it back: `var e = xs[i]` then `e.f = v` then `xs[i] = e`.";
            let diag = self.diag_error(msg, hint, "nested-index-assign");
            self.errors.push(diag);
            let tok = self.current().clone();
            return Err(format!("{} at line {}:{}", msg, tok.line, tok.col));
        }
        self.advance(); // '='
        self.skip_newlines();
        let value = self.parse_expr_ascribed()?;
        let span = Some(span);
        Ok(Some(match index {
            Some(index) => Stmt::IndexAssign { target, path: fields, index: Box::new(index), value, span },
            None => {
                let field = fields.pop().expect("a field target has at least one field");
                Stmt::FieldAssign { target, path: fields, field, value, span }
            }
        }))
    }

    fn parse_destructure_tuple(&mut self) -> Result<Pattern, String> {
        self.expect(TokenType::LParen)?;
        let mut elements = Vec::new();
        while !self.check(TokenType::RParen) {
            if self.check(TokenType::LParen) {
                elements.push(self.parse_destructure_tuple()?);
            } else if self.check(TokenType::Underscore) {
                self.advance();
                elements.push(Pattern::Wildcard);
            } else {
                let name = self.expect_ident()?;
                elements.push(Pattern::Ident { name });
            }
            if self.check(TokenType::Comma) { self.advance(); self.skip_newlines(); }
        }
        self.expect(TokenType::RParen)?;
        Ok(Pattern::Tuple { elements })
    }
}
