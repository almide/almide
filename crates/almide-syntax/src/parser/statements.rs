use crate::lexer::TokenType;
use crate::ast::*;
use crate::intern::sym;
use super::Parser;

impl Parser {
    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if self.check(TokenType::Let) { return self.parse_let_stmt(); }
        if self.check(TokenType::Var) { return self.parse_var_stmt(); }
        if self.check(TokenType::Guard) { return self.parse_guard_stmt(); }

        // name = value (simple assignment)
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_assign_stmt();
        }

        // xs[i] = value (index assignment)
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBracket)
        {
            if let Some(stmt) = self.try_parse_index_assign()? {
                return Ok(stmt);
            }
        }

        // obj.field = value (field assignment). The field name may be any token
        // `expect_any_name` accepts (ident, TypeName, or a soft keyword) so
        // `obj.ok = v` routes here, exactly as `obj.ok` reads in expression
        // position — see parse_field_assign_stmt / parse_postfix.
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Dot)
            && self.peek_at(2).map(|t| Self::is_name_token(&t.token_type)).unwrap_or(false)
            && self.peek_at(3).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(4).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_field_assign_stmt();
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
        if self.check(TokenType::Ident) && self.current().value == "rec" {
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
            ).with_try("let x = 1\nlet y = 2\nx + y");
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
        let value = self.parse_expr()?;
        Ok(Stmt::Assign { name, value, span: Some(span) })
    }

    fn try_parse_index_assign(&mut self) -> Result<Option<Stmt>, String> {
        let saved = self.pos;
        let span = self.current_span();
        let target = sym(&self.current().value);
        self.advance();
        self.expect(TokenType::LBracket)?;
        let index = self.parse_expr()?;
        self.expect(TokenType::RBracket)?;
        if self.check(TokenType::Eq)
            && self.peek_at(1).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            self.advance();
            self.skip_newlines();
            let value = self.parse_expr()?;
            Ok(Some(Stmt::IndexAssign { target, index: Box::new(index), value, span: Some(span) }))
        } else {
            self.pos = saved;
            Ok(None)
        }
    }

    fn parse_field_assign_stmt(&mut self) -> Result<Stmt, String> {
        let span = self.current_span();
        let target = sym(&self.current().value);
        self.advance(); // ident
        self.advance(); // .
        let field = self.expect_any_name()?;
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::FieldAssign { target, field, value, span: Some(span) })
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
