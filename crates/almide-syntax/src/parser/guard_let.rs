//! `guard let` / `guard var` (ALS-ST6, #4543): after the `guard` keyword.
//!
//! ```text
//! guard (let|var) name [= scrutinee] else [failure-arm =>] expr
//! failure-arm := err(name) | err(_) | none
//! ```
//!
//! - `guard let x else E` is the shorthand for `guard let x = x else E`: the
//!   scrutinee is the binding `x` itself, rebound to its payload.
//! - `else err(e) => E` names the failure: `e` is the Result's error inside
//!   `E`. `else none => E` is the Option spelling of the plain `else E` (fmt
//!   writes it as `else E`).
//!
//! Refused here, each with one diagnostic and a recovery that keeps the
//! statement: a field path in the shorthand (`guard let a.b else`), a nested
//! failure pattern (`err(NotFound(p)) =>`, which would not cover every
//! error), an `if` guard or `|` alternatives on the arm, a success-side arm
//! (`ok(v) =>`), and a second failure arm. A guard that splits its failure
//! into cases is a `match`.

use crate::ast::*;
use crate::intern::Sym;
use crate::lexer::TokenType;
use super::Parser;

const SPLIT_HINT: &str = "A `guard let` has ONE failure arm. To handle failures case by case, write a `match`: \
     `match r { ok(x) => …, err(NotFound(p)) => …, err(e) => … }`.";

impl Parser {
    /// `let|var name [= scrutinee] else …` — the current token is `let` or `var`.
    pub(crate) fn parse_guard_let_rest(&mut self, span: Span) -> Result<Stmt, String> {
        let mutable = self.check(TokenType::Var);
        self.advance();
        self.skip_newlines();
        let name_span = self.current_span();
        let name = self.expect_ident()?;
        if let Some(stmt) = self.guard_let_field_path(name, mutable, span)? {
            return Ok(stmt);
        }
        self.skip_newlines();
        let shorthand = self.check(TokenType::Else);
        let scrutinee = if shorthand {
            Expr::new(self.next_id(), Some(name_span), ExprKind::Ident { name })
        } else {
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            self.parse_expr()?
        };
        self.finish_guard_let(name, scrutinee, mutable, shorthand, span)
    }

    /// `else [failure-arm =>] expr`, then the one-arm check.
    fn finish_guard_let(&mut self, name: Sym, scrutinee: Expr, mutable: bool, shorthand: bool, span: Span) -> Result<Stmt, String> {
        self.skip_newlines();
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let fail = self.parse_guard_failure_arm()?;
        let else_ = self.parse_expr()?;
        if fail.is_some() {
            self.refuse_second_failure_arm()?;
        }
        Ok(Stmt::GuardLet { name, scrutinee, else_, mutable, shorthand, fail, span: Some(span) })
    }

    /// `guard let a.b else …`: the shorthand names a binding, not a place.
    /// One error with the spelled-out form as the fix, and the statement
    /// parses as that form (`guard let b = a.b else …`).
    fn guard_let_field_path(&mut self, root: Sym, mutable: bool, span: Span) -> Result<Option<Stmt>, String> {
        if !self.check(TokenType::Dot) {
            return Ok(None);
        }
        // `name(.field)+` then `else` — anything else after the dot is left to
        // the `=` the plain form expects.
        let mut path = vec![root.to_string()];
        let mut off = 0;
        while self.peek_at(off).is_some_and(|t| t.token_type == TokenType::Dot)
            && self.peek_at(off + 1).is_some_and(|t| t.token_type == TokenType::Ident)
        {
            path.push(self.peek_at(off + 1).map(|t| t.value.clone()).unwrap_or_default());
            off += 2;
        }
        if path.len() < 2 || !self.peek_at(off).is_some_and(|t| t.token_type == TokenType::Else) {
            return Ok(None);
        }
        let kw = if mutable { "var" } else { "let" };
        let last = path.last().cloned().unwrap_or_default();
        let spelled = format!("guard {kw} {last} = {} else …", path.join("."));
        let diag = self.diag_error(
            format!("`guard {kw} {}` names a field path, but the shorthand takes a local binding", path.join(".")),
            format!("`guard {kw} x else …` rebinds the local or parameter `x`. Name the field's payload yourself: `{spelled}`"),
            "guard let shorthand",
        ).with_try(spelled);
        self.errors.push(diag);
        let mut object = Expr::new(self.next_id(), None, ExprKind::Ident { name: root });
        while self.check(TokenType::Dot) {
            self.advance();
            let field = self.expect_ident()?;
            object = Expr::new(self.next_id(), None, ExprKind::Member { object: Box::new(object), field });
        }
        let name = crate::intern::sym(&last);
        self.finish_guard_let(name, object, mutable, false, span).map(Some)
    }

    /// The offset just past a failure-arm pattern starting at `pos + off`,
    /// when one starts there and is followed by `=>`, `if` or `|` — pure
    /// lookahead, nothing consumed. `none`, `_`, and `err` / `ok` / `some`
    /// with a parenthesised payload are arm heads; an expression such as
    /// `err("bad")` (no arrow after it) is not.
    fn guard_arm_head_at(&self, off: usize) -> bool {
        let tt = |i: usize| self.peek_at(i).map(|t| t.token_type);
        let arrow = |i: usize| matches!(tt(i), Some(TokenType::FatArrow | TokenType::If | TokenType::Pipe));
        match tt(off) {
            Some(TokenType::None | TokenType::Underscore) => arrow(off + 1),
            Some(TokenType::Err | TokenType::Ok | TokenType::Some) if tt(off + 1) == Some(TokenType::LParen) => {
                let mut depth = 0usize;
                let mut i = off + 1;
                loop {
                    match tt(i) {
                        Some(TokenType::LParen | TokenType::LBracket | TokenType::LBrace) => depth += 1,
                        Some(TokenType::RParen | TokenType::RBracket | TokenType::RBrace) => {
                            depth -= 1;
                            if depth == 0 {
                                return arrow(i + 1);
                            }
                        }
                        Some(TokenType::EOF) | None => return false,
                        _ => {}
                    }
                    i += 1;
                }
            }
            _ => false,
        }
    }

    /// The failure arm after `else`, when the else is written as one. A
    /// refused arm keeps its written pattern (the statement is already an
    /// error) so the names it binds do not cascade into undefined-name ones.
    fn parse_guard_failure_arm(&mut self) -> Result<Option<Pattern>, String> {
        if !self.guard_arm_head_at(0) {
            return Ok(None);
        }
        // Every refusal below points at the arm's first token.
        let head = self.current().clone();
        let at = |mut d: crate::diagnostic::Diagnostic| {
            d.line = Some(head.line);
            d.col = Some(head.col);
            d.end_col = (head.end_col > head.col).then_some(head.end_col);
            d
        };
        let pattern = self.parse_pattern()?;
        let fail = match &pattern {
            Pattern::None => Some(Pattern::None),
            Pattern::Err { inner } if matches!(**inner, Pattern::Ident { .. } | Pattern::Wildcard) => Some(pattern.clone()),
            Pattern::Err { .. } => {
                let diag = self.diag_error(
                    "a `guard let` failure arm binds the whole error: `err(e) =>`, not a nested pattern",
                    format!("A nested pattern would leave the other errors unhandled. {SPLIT_HINT}"),
                    "guard let failure arm",
                ).with_try("guard let x = r else err(e) => fallback(e)");
                self.errors.push(at(diag));
                // Keep the written pattern so its names stay bound in the
                // else: no undefined-name cascade behind this one error.
                Some(pattern.clone())
            }
            Pattern::Wildcard => {
                let diag = self.diag_error(
                    "`else _ =>` is not a `guard let` failure arm",
                    "Write the else value directly: `guard let x = r else fallback`, or `else err(e) => …` to use a Result's error.",
                    "guard let failure arm",
                ).with_try("guard let x = r else fallback");
                self.errors.push(at(diag));
                None
            }
            _ => {
                let diag = self.diag_error(
                    "a `guard let` failure arm names the failure side: `err(e) =>` for a Result, `none =>` for an Option",
                    "The success side is the binding before `else`; the arm after `else` is what runs when the value is missing.",
                    "guard let failure arm",
                ).with_try("guard let x = r else err(e) => fallback(e)");
                self.errors.push(at(diag));
                Some(pattern.clone())
            }
        };
        if self.check(TokenType::If) || self.check(TokenType::Pipe) {
            let what = if self.check(TokenType::If) { "`if` guard" } else { "`|` alternatives" };
            let diag = self.diag_error(
                format!("a `guard let` failure arm takes no {what}"),
                SPLIT_HINT,
                "guard let failure arm",
            );
            self.errors.push(diag);
            // Recover: skip to the arrow.
            while !self.check(TokenType::FatArrow) && !self.check(TokenType::EOF) && !self.check(TokenType::Newline) {
                self.advance();
            }
        }
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        Ok(fail)
    }

    /// `guard let x = r else err(A) => a, err(B) => b` (or the second arm on
    /// the next line): one error, and the extra arms are consumed.
    fn refuse_second_failure_arm(&mut self) -> Result<(), String> {
        let mut off = 0;
        if self.peek_at(off).is_some_and(|t| t.token_type == TokenType::Comma) {
            off += 1;
        }
        while self.peek_at(off).is_some_and(|t| matches!(t.token_type, TokenType::Newline | TokenType::Comment)) {
            off += 1;
        }
        if !self.guard_arm_head_at(off) {
            return Ok(());
        }
        for _ in 0..off {
            self.advance();
        }
        let diag = self.diag_error("a `guard let` has one failure arm", SPLIT_HINT, "guard let failure arm");
        self.errors.push(diag);
        loop {
            self.parse_pattern()?;
            while !self.check(TokenType::FatArrow) && !self.check(TokenType::EOF) && !self.check(TokenType::Newline) {
                self.advance();
            }
            self.expect(TokenType::FatArrow)?;
            self.skip_newlines();
            self.parse_expr()?;
            let mut off = 0;
            if self.peek_at(off).is_some_and(|t| t.token_type == TokenType::Comma) {
                off += 1;
            }
            while self.peek_at(off).is_some_and(|t| matches!(t.token_type, TokenType::Newline | TokenType::Comment)) {
                off += 1;
            }
            if !self.guard_arm_head_at(off) {
                return Ok(());
            }
            for _ in 0..off {
                self.advance();
            }
        }
    }
}
