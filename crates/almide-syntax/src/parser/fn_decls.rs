//! The `fn` declaration parser.
//!
//! Split out of `declarations.rs` (max-lines): everything from `[pub] [effect]
//! fn name` through the return type and body lift lives here, along with the
//! effect-body `ok(...)` wrap it needs.

use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::Sym;
use crate::lexer::TokenType;
use super::Parser;

impl Parser {
    pub(crate) fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        if self.check(TokenType::Pub) { self.advance(); }
        let visibility = self.parse_visibility();
        let mut effect = false;
        if self.check(TokenType::Effect) { self.advance(); effect = true; }
        self.expect(TokenType::Fn)?;
        let name = self.expect_any_fn_name()?;
        // Once the fn name is known, any later parse error in this decl is a
        // cascading source — record the name so the checker can suppress
        // downstream "undefined function 'name'" noise from call sites.
        let recorded_name = name;
        let result = self.parse_fn_decl_after_name(name, effect, visibility, span);
        if result.is_err() {
            self.failed_fn_names.insert(recorded_name.to_string());
        }
        result
    }

    /// Everything after `[pub] [effect] fn <name>`: generics, parameter list,
    /// return type and body. Split out of [`Self::parse_fn_decl`] (it used to be
    /// an immediately-invoked closure there, purely so the failure bookkeeping
    /// could wrap it) — the caller records the name on `Err` exactly as before.
    fn parse_fn_decl_after_name(
        &mut self,
        name: Sym,
        effect: bool,
        visibility: Visibility,
        span: crate::ast::Span,
    ) -> Result<Decl, String> {
        let generics = self.try_parse_generic_params()?;
        let open_fn = self.current().clone();
        self.expect(TokenType::LParen)?;
        let params = self.parse_param_list()?;
        self.expect_closing(TokenType::RParen, open_fn.line, open_fn.col, "function parameters")?;
        self.reject_missing_return_type(name, effect)?;
        let return_type = self.parse_fn_return_type()?;
        let body = self.parse_fn_decl_body(name, effect, &return_type)?;
        Ok(Decl::Fn {
            name,
            effect: if effect { Some(true) } else { None },
            visibility,
            extern_attrs: Vec::new(),
            export_attrs: Vec::new(),
            attrs: Vec::new(),
            generics, params, return_type, body,
            span: Some(span),
        })
    }

    /// #1072: `fn main() {` is the first-contact spelling of every writer coming
    /// from Rust/Go/TS, and `fn f() = ...` of one who already learned the `=`
    /// rule. The parser knows the decl, the name, and the full distance to legal
    /// — teach the complete form in one hint instead of a bare "Expected Arrow"
    /// that costs two failures to climb.
    fn reject_missing_return_type(&mut self, name: Sym, effect: bool) -> Result<(), String> {
        if !self.check(TokenType::LBrace) && !self.check(TokenType::Eq) {
            return Ok(());
        }
        let tok = self.current();
        let kw = if effect { "effect fn " } else { "fn " };
        let (parens, ret) = if name == "main" { ("()", "Unit") } else { ("(...)", "Type") };
        let mut msg = format!(
            "Missing return type at line {}:{}\n  Hint: every fn declares its return type and takes '=' before its body:\n        {kw}{name}{parens} -> {ret} = {{ ... }}",
            tok.line, tok.col
        );
        if name == "main" && !effect {
            msg.push_str("\n        a main that performs IO (fs, http, ...) should be:  effect fn main() -> Unit = { ... }");
        }
        Err(msg)
    }

    /// `-> Ty`, `-> Ty!`, or `-> Ty!E`.
    ///
    /// ADR-0002 Phase 1 (#1103): `-> T!` marks a pure-fallible return — sugar for
    /// `Result[T, String]`, carried as the pseudo-generic `!` so the formatter can
    /// print the surface spelling back. Legal ONLY in fn-decl return position
    /// (this parse site); the resolver maps it, and everything downstream sees the
    /// plain Result.
    ///
    /// ADR-0012 D2 (#1193): the marker may carry a TYPED error — `-> T!E` ≡
    /// `Result[T, E]`, the 2-arg pseudo-generic. Bare `!` stays 1-arg (the
    /// `!String` default), so `T!` keeps meaning `T!String` and every existing
    /// program is unaffected. Same-line only, one token of lookahead.
    fn parse_fn_return_type(&mut self) -> Result<TypeExpr, String> {
        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type_expr()?;
        if !self.check(TokenType::Bang) || self.newline_before_current() {
            return Ok(return_type);
        }
        self.advance();
        if self.marker_error_follows() {
            let e = self.parse_marker_error_type()?;
            return Ok(TypeExpr::Generic {
                name: crate::intern::sym("!"),
                args: vec![return_type, e],
            });
        }
        Ok(TypeExpr::Generic { name: crate::intern::sym("!"), args: vec![return_type] })
    }

    /// `= <expr>` (or nothing, for a declaration-only fn).
    ///
    /// ADR-0002 Phase 1 (#1103): a `-> T!` return gets the SAME body lift as an
    /// effect fn returning Result — a T-typed tail wraps in `ok(...)`, and `!`
    /// propagates (D3: lift ergonomics are a property of the fallibility axis,
    /// not the effect axis).
    fn parse_fn_decl_body(
        &mut self,
        name: Sym,
        effect: bool,
        return_type: &TypeExpr,
    ) -> Result<Option<Expr>, String> {
        if self.check(TokenType::LBrace) {
            let tok = self.current();
            return Err(format!(
                "Missing '=' before function body at line {}:{}\n  Hint: Almide requires '=' before the body. Write: fn {}(...) -> Type = {{ ... }}",
                tok.line, tok.col, name
            ));
        }
        if !self.check(TokenType::Eq) {
            return Ok(None);
        }
        self.advance();
        self.skip_newlines();
        let body_span = self.current_span();
        let parsed = if self.check(TokenType::Let)
            || self.check(TokenType::Var)
            || self.check(TokenType::Guard)
        {
            self.parse_braceless_block()
        } else {
            self.parse_expr()
        };
        let mut body = match parsed {
            Ok(b) => b,
            Err(msg) => {
                // #1489: an expression-bodied fn with a broken body used to
                // lose the WHOLE declaration (the entry loop's
                // skip_to_next_decl), so the checker never saw the signature
                // and every later diagnostic behind it vanished under one
                // parse error. Recover here instead: report, resync to the
                // next declaration, and keep the decl with an Error body —
                // the checker treats it as Unknown and keeps checking
                // everything else (the block-body twin of Stmt::Error,
                // contract C-263's statement-granular recovery).
                self.errors.push(self.string_to_diagnostic(&msg));
                self.skip_to_next_decl();
                Expr::new(self.next_id(), Some(body_span), ExprKind::Error)
            }
        };
        let returns_result =
            matches!(return_type, TypeExpr::Generic { name, .. } if name == "Result");
        let returns_fallible = matches!(return_type, TypeExpr::Generic { name, .. } if name == "!");
        if (effect && returns_result) || returns_fallible {
            body = self.wrap_effect_result_body(body);
        }
        Ok(Some(body))
    }

    fn wrap_effect_result_body(&mut self, body: Expr) -> Expr {
        if let ExprKind::Block { ref stmts, ref expr } = body.kind {
            let (effective_stmts, effective_expr) = if expr.is_none() && !stmts.is_empty() {
                let last_non_comment = stmts.iter().rposition(|s| !matches!(s, Stmt::Comment { .. }));
                if let Some(idx) = last_non_comment {
                    if let Stmt::Expr { expr: last_expr, .. } = &stmts[idx] {
                        let mut remaining = stmts[..idx].to_vec();
                        remaining.extend_from_slice(&stmts[idx+1..]);
                        (remaining, Some(Box::new(last_expr.clone())))
                    } else {
                        (stmts.clone(), None)
                    }
                } else {
                    (stmts.clone(), None)
                }
            } else {
                (stmts.clone(), expr.clone())
            };
            let needs_ok = match &effective_expr {
                None => true,
                Some(e) => matches!(e.kind, ExprKind::Unit),
            };
            if needs_ok {
                let mut new_stmts = effective_stmts;
                if let Some(trailing) = effective_expr {
                    new_stmts.push(Stmt::Expr { expr: *trailing, span: None });
                }
                return Expr::new(self.next_id(), None, ExprKind::Block {
                    stmts: new_stmts,
                    expr: Some(Box::new(Expr::new(self.next_id(), None, ExprKind::Ok {
                        expr: Box::new(Expr::new(self.next_id(), None, ExprKind::Unit)),
                    }))),
                });
            } else if expr.is_none() {
                return Expr::new(self.next_id(), None, ExprKind::Block {
                    stmts: effective_stmts, expr: effective_expr,
                });
            }
        }
        body
    }
}
