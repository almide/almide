use crate::lexer::TokenType;
use crate::ast::*;
use crate::ast::ExprKind;
use crate::intern::{Sym, sym};
use super::Parser;

/// Parse an integer literal raw spelling (same syntax the lexer
/// accepts for `TokenType::Int`): decimal, `0x` hex, and `_`
/// digit separators.
fn parse_int_literal(raw: &str) -> Result<i64, String> {
    let clean = raw.replace('_', "");
    if let Some(hex) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        clean.parse::<i64>().map_err(|e| e.to_string())
    }
}

/// Convert a generic `Attribute` that the user wrote as `@extern(...)`
/// into the legacy typed `ExternAttr`. Mirrors the old hand-written
/// parser so diagnostics stay recognizable.
fn extract_extern_attr(attr: &Attribute) -> Result<ExternAttr, String> {
    let args = &attr.args;
    if args.len() != 3 {
        return Err(format!(
            "@extern expects 3 positional arguments (target, \"module\", \"function\"); got {}",
            args.len()
        ));
    }
    let target = match &args[0] {
        AttrArg { name: None, value: AttrValue::Ident { name } } => *name,
        _ => return Err("@extern first argument must be a bare identifier target (e.g. `rust`)".into()),
    };
    let module = match &args[1] {
        AttrArg { name: None, value: AttrValue::String { value } } => sym(value),
        _ => return Err("@extern second argument must be a string literal module".into()),
    };
    let function = match &args[2] {
        AttrArg { name: None, value: AttrValue::String { value } } => sym(value),
        _ => return Err("@extern third argument must be a string literal function".into()),
    };
    Ok(ExternAttr { target, module, function })
}

/// Convert a generic `Attribute` that the user wrote as `@export(...)`
/// into the legacy typed `ExportAttr`.
fn extract_export_attr(attr: &Attribute) -> Result<ExportAttr, String> {
    let args = &attr.args;
    if args.len() != 2 {
        return Err(format!(
            "@export expects 2 positional arguments (target, \"symbol\"); got {}",
            args.len()
        ));
    }
    let target = match &args[0] {
        AttrArg { name: None, value: AttrValue::Ident { name } } => *name,
        _ => return Err("@export first argument must be a bare identifier target (e.g. `c`)".into()),
    };
    let symbol = match &args[1] {
        AttrArg { name: None, value: AttrValue::String { value } } => sym(value),
        _ => return Err("@export second argument must be a string literal symbol".into()),
    };
    Ok(ExportAttr { target, symbol })
}

impl Parser {
    // ── Module & Import ───────────────────────────────────────────

    pub(crate) fn parse_module_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Module)?;
        let path = self.parse_module_path()?;
        Ok(Decl::Module { path, span: Some(span) })
    }

    pub(crate) fn parse_import_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Import)?;

        if self.check(TokenType::LBrace) {
            let tok = self.current();
            return Err(format!(
                "Unexpected '{{' in import at line {}:{}\n  Hint: Almide imports don't use braces. Write: import json (not import {{ json }})",
                tok.line, tok.col
            ));
        }

        let path = self.parse_module_path()?;

        // Selective import: import mod.{ A, B }
        if self.check(TokenType::Dot)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace)
        {
            self.advance();
            let open = self.current().clone();
            self.expect(TokenType::LBrace)?;
            let mut names = Vec::new();
            names.push(self.expect_any_name()?);
            while self.check(TokenType::Comma) {
                self.advance();
                if self.check(TokenType::RBrace) { break; }
                names.push(self.expect_any_name()?);
            }
            self.expect_closing(TokenType::RBrace, open.line, open.col, "selective import")?;
            return Ok(Decl::Import { path, names: Some(names), alias: None, span: Some(span) });
        }

        let alias = if self.check_ident("as") {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        Ok(Decl::Import { path, names: None, alias, span: Some(span) })
    }

    fn parse_module_path(&mut self) -> Result<Vec<Sym>, String> {
        let mut parts = Vec::new();
        parts.push(self.expect_ident()?);
        while self.check(TokenType::Dot)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Ident)
        {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        Ok(parts)
    }

    // ── Top-level Declarations ────────────────────────────────────

    pub(crate) fn parse_top_decl(&mut self) -> Result<Decl, String> {
        if self.check(TokenType::At) {
            let (extern_attrs, export_attrs, attrs) = self.collect_fn_attrs()?;
            return self.parse_fn_decl_with_attrs(extern_attrs, export_attrs, attrs);
        }
        if self.check(TokenType::Type) {
            return self.parse_type_decl();
        }
        if self.check(TokenType::Protocol) {
            return self.parse_protocol_decl();
        }
        if self.check(TokenType::Impl) {
            return self.parse_impl_decl();
        }
        if self.check(TokenType::Let) {
            return self.parse_top_let(Visibility::Public);
        }
        if self.check(TokenType::Fn) || self.check(TokenType::Pub)
            || self.check(TokenType::Effect)
            || self.check(TokenType::Local) || self.check(TokenType::Mod)
        {
            if self.check(TokenType::Pub)
                && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Let)
            {
                self.advance();
                return self.parse_top_let(Visibility::Public);
            }
            if self.check(TokenType::Local) || self.check(TokenType::Mod) {
                if self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Type) {
                    return self.parse_type_decl();
                }
                if self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Let) {
                    let vis = self.parse_visibility();
                    return self.parse_top_let(vis);
                }
            }
            return self.parse_fn_decl();
        }
        if self.check(TokenType::Strict) {
            return self.parse_strict_decl();
        }
        if self.check(TokenType::Test) {
            return self.parse_test_decl();
        }
        let tok = self.current();
        if let Some(result) = self.check_hint(None, super::hints::HintScope::TopLevel) {
            let msg = result.message.as_deref().unwrap_or("Unexpected token at top level");
            return Err(format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint));
        }
        Err(format!(
            "Expected top-level declaration (fn, effect fn, type, let, trait, impl, test) at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn parse_top_let(&mut self, visibility: Visibility) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Let)?;
        let name = self.expect_any_name()?;
        let ty = if self.check(TokenType::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Decl::TopLet { name, ty, value, visibility, span: Some(span) })
    }

    /// Parse `@name(args)?` declarations that precede a fn decl.
    ///
    /// Returns three buckets: `@extern` and `@export` are routed to
    /// their legacy typed structs so existing consumers keep working;
    /// any other `@name(...)` lands in a generic `Vec<Attribute>` that
    /// new tooling (stdlib unification, MLIR schedules, rewrite rules)
    /// can inspect without pattern-matching over the typed structs.
    fn collect_fn_attrs(&mut self) -> Result<(Vec<ExternAttr>, Vec<ExportAttr>, Vec<Attribute>), String> {
        let mut extern_attrs = Vec::new();
        let mut export_attrs = Vec::new();
        let mut attrs = Vec::new();
        while self.check(TokenType::At) {
            let attr = self.parse_attribute()?;
            match attr.name.as_str() {
                "extern" => extern_attrs.push(extract_extern_attr(&attr)?),
                "export" => export_attrs.push(extract_export_attr(&attr)?),
                _ => attrs.push(attr),
            }
            self.skip_newlines();
        }
        Ok((extern_attrs, export_attrs, attrs))
    }

    /// Parse a single `@name` or `@name(args...)`. Positioned at the
    /// leading `@` token.
    fn parse_attribute(&mut self) -> Result<Attribute, String> {
        let span = self.current_span();
        self.expect(TokenType::At)?;
        let name = self.expect_ident_like_name()?;
        let args = if self.check(TokenType::LParen) {
            let open = self.current().clone();
            self.advance();
            self.skip_newlines();
            let mut args = Vec::new();
            if !self.check(TokenType::RParen) {
                args.push(self.parse_attr_arg()?);
                while self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                    if self.check(TokenType::RParen) { break; }
                    args.push(self.parse_attr_arg()?);
                }
                self.skip_newlines();
            }
            self.expect_closing(TokenType::RParen, open.line, open.col, "attribute argument list")?;
            args
        } else {
            // `@pure` / `@inline` style — no parens, no args.
            Vec::new()
        };
        Ok(Attribute { name, args, span: Some(span) })
    }

    /// Parse one arg inside `@name(...)`: either `value` (positional)
    /// or `name=value` (named). The lookahead for `name=` is an ident
    /// followed by `=` that is NOT `==`.
    fn parse_attr_arg(&mut self) -> Result<AttrArg, String> {
        let is_named = self.check(TokenType::Ident)
            && matches!(self.peek_at(1).map(|t| &t.token_type), Some(&TokenType::Eq));
        let name = if is_named {
            let n = self.expect_ident()?;
            self.advance(); // consume `=`
            Some(n)
        } else {
            None
        };
        let value = self.parse_attr_value()?;
        Ok(AttrArg { name, value })
    }

    /// Attribute values are a narrow subset of expressions: literals
    /// and bare identifiers. Negative ints are accepted via a `-`
    /// prefix so callers can write `@foo(-1)`.
    fn parse_attr_value(&mut self) -> Result<AttrValue, String> {
        let tok = self.current().clone();
        match tok.token_type {
            TokenType::String => {
                self.advance();
                Ok(AttrValue::String { value: tok.value })
            }
            TokenType::Int => {
                self.advance();
                parse_int_literal(&tok.value).map(|v| AttrValue::Int { value: v })
                    .map_err(|e| format!("Invalid integer literal in attribute at line {}:{} — {}", tok.line, tok.col, e))
            }
            TokenType::Minus => {
                self.advance();
                let inner = self.current().clone();
                if inner.token_type != TokenType::Int {
                    return Err(format!(
                        "Expected integer literal after '-' in attribute at line {}:{}",
                        inner.line, inner.col
                    ));
                }
                self.advance();
                parse_int_literal(&inner.value).map(|v| AttrValue::Int { value: -v })
                    .map_err(|e| format!("Invalid integer literal in attribute at line {}:{} — {}", tok.line, tok.col, e))
            }
            TokenType::True => {
                self.advance();
                Ok(AttrValue::Bool { value: true })
            }
            TokenType::False => {
                self.advance();
                Ok(AttrValue::Bool { value: false })
            }
            TokenType::Ident => {
                let name = sym(&tok.value);
                self.advance();
                Ok(AttrValue::Ident { name })
            }
            _ => {
                Err(format!(
                    "Expected attribute value (string, int, bool, or identifier) at line {}:{} (got {:?} '{}')",
                    tok.line, tok.col, tok.token_type, tok.value
                ))
            }
        }
    }

    /// Accept an identifier or soft-keyword in attribute name position
    /// (`extern`, `export`, `pure`, `inline_rust`, ...). Uses the same
    /// acceptance set as `expect_any_name` but without forcing a fn
    /// name context.
    fn expect_ident_like_name(&mut self) -> Result<Sym, String> {
        // `extern` / `export` / `effect` / other soft keywords might
        // be tokenized as keywords; fall back to their raw spelling.
        let tok = self.current().clone();
        match tok.token_type {
            TokenType::Ident => {
                let name = sym(&tok.value);
                self.advance();
                Ok(name)
            }
            _ => {
                // Accept keyword-ish tokens by raw spelling so that
                // @extern / @effect etc. still reach the generic path
                // should a caller need them. Error only if the value
                // is empty (not a word-like token).
                if tok.value.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') {
                    let name = sym(&tok.value);
                    self.advance();
                    Ok(name)
                } else {
                    Err(format!(
                        "Expected attribute name after '@' at line {}:{} (got {:?} '{}')",
                        tok.line, tok.col, tok.token_type, tok.value
                    ))
                }
            }
        }
    }

    fn parse_fn_decl_with_attrs(&mut self, extern_attrs: Vec<ExternAttr>, export_attrs: Vec<ExportAttr>, attrs: Vec<Attribute>) -> Result<Decl, String> {
        let mut decl = self.parse_fn_decl()?;
        if let Decl::Fn {
            extern_attrs: ref mut ea,
            export_attrs: ref mut xa,
            attrs: ref mut aa,
            ..
        } = decl {
            *ea = extern_attrs;
            *xa = export_attrs;
            *aa = attrs;
        }
        Ok(decl)
    }

    fn parse_type_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        let visibility = self.parse_visibility();
        self.expect(TokenType::Type)?;
        let name = self.expect_type_name()?;
        let generics = self.try_parse_generic_params()?;
        // Conventions: type Name: Eq, Show = ...
        let deriving = if self.check(TokenType::Colon) {
            self.advance();
            let mut d = Vec::new();
            d.push(self.expect_type_name()?);
            while self.check(TokenType::Comma) {
                self.advance();
                d.push(self.expect_type_name()?);
            }
            Some(d)
        } else {
            None
        };
        self.skip_newlines();
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let ty = self.parse_type_expr()?;
        // In type declarations, Union of all-uppercase Simple names is a Variant (enum)
        // e.g., `type Color = Red | Green | Blue` → Variant, not Union
        let ty = match ty {
            TypeExpr::Union { ref members } if members.iter().all(|m| matches!(m, TypeExpr::Simple { name } if name.starts_with(char::is_uppercase))) => {
                TypeExpr::Variant {
                    cases: members.iter().map(|m| {
                        if let TypeExpr::Simple { name } = m { VariantCase::Unit { name: name.clone() } } else { unreachable!() }
                    }).collect(),
                }
            }
            other => other,
        };
        Ok(Decl::Type { name, ty, deriving, visibility, generics, span: Some(span) })
    }

    fn parse_protocol_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Protocol)?;
        let name = self.expect_type_name()?;
        let generics = self.try_parse_generic_params()?;
        let open = self.current().clone();
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut methods: Vec<ProtocolMethod> = Vec::new();
        while !self.check(TokenType::RBrace) {
            methods.push(self.parse_protocol_method()?);
            self.skip_newlines();
        }
        self.expect_closing(TokenType::RBrace, open.line, open.col, "protocol body")?;
        Ok(Decl::Protocol { name, generics, methods, span: Some(span) })
    }

    fn parse_protocol_method(&mut self) -> Result<ProtocolMethod, String> {
        let mut effect = false;
        if self.check(TokenType::Effect) { self.advance(); effect = true; }
        self.expect(TokenType::Fn)?;
        let name = self.expect_any_fn_name()?;
        let _generics = self.try_parse_generic_params()?;
        let open_tm = self.current().clone();
        self.expect(TokenType::LParen)?;
        let params = self.parse_param_list()?;
        self.expect_closing(TokenType::RParen, open_tm.line, open_tm.col, "protocol method parameters")?;
        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type_expr()?;
        Ok(ProtocolMethod { name, params, return_type, effect })
    }

    fn parse_impl_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Impl)?;
        let trait_name = self.expect_type_name()?;
        let generics = self.try_parse_generic_params()?;
        self.expect(TokenType::For)?;
        let for_name = self.expect_type_name()?;
        if self.check(TokenType::LBracket) { self.parse_type_args()?; }
        let open_impl = self.current().clone();
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut methods = Vec::new();
        while !self.check(TokenType::RBrace) {
            methods.push(self.parse_fn_decl()?);
            self.skip_newlines();
        }
        self.expect_closing(TokenType::RBrace, open_impl.line, open_impl.col, "impl body")?;
        Ok(Decl::Impl {
            trait_: trait_name, for_: for_name, generics, methods,
            span: Some(span),
        })
    }

    pub(crate) fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        if self.check(TokenType::Pub) { self.advance(); }
        let visibility = self.parse_visibility();
        let async_ = false;
        let mut effect = false;
        if self.check(TokenType::Effect) { self.advance(); effect = true; }
        self.expect(TokenType::Fn)?;
        let name = self.expect_any_fn_name()?;
        // Once the fn name is known, any later parse error in this decl is a
        // cascading source — record the name so the checker can suppress
        // downstream "undefined function 'name'" noise from call sites.
        let recorded_name = name.clone();
        let mut failed = true;
        let result = (|| -> Result<Decl, String> {
            let generics = self.try_parse_generic_params()?;
            let open_fn = self.current().clone();
            self.expect(TokenType::LParen)?;
            let params = self.parse_param_list()?;
            self.expect_closing(TokenType::RParen, open_fn.line, open_fn.col, "function parameters")?;
            self.expect(TokenType::Arrow)?;
            let return_type = self.parse_type_expr()?;

            if self.check(TokenType::LBrace) {
                let tok = self.current();
                return Err(format!(
                    "Missing '=' before function body at line {}:{}\n  Hint: Almide requires '=' before the body. Write: fn {}(...) -> Type = {{ ... }}",
                    tok.line, tok.col, name
                ));
            }

            let body = if self.check(TokenType::Eq) {
                self.advance();
                self.skip_newlines();
                let mut body = if self.check(TokenType::Let) || self.check(TokenType::Var)
                    || self.check(TokenType::Guard)
                {
                    self.parse_braceless_block()?
                } else {
                    self.parse_expr()?
                };

                let returns_result = matches!(&return_type,
                    TypeExpr::Generic { name, .. } if name == "Result"
                );
                if effect && returns_result {
                    body = self.wrap_effect_result_body(body);
                }

                Some(body)
            } else {
                None
            };

            Ok(Decl::Fn {
                name: name.clone(),
                r#async: if async_ { Some(true) } else { None },
                effect: if effect { Some(true) } else { None },
                visibility,
                extern_attrs: Vec::new(),
                export_attrs: Vec::new(),
                attrs: Vec::new(),
                generics, params, return_type, body,
                span: Some(span),
            })
        })();
        if result.is_ok() { failed = false; }
        if failed {
            self.failed_fn_names.insert(recorded_name.to_string());
        }
        result
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

    fn parse_strict_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Strict)?;
        let mode = self.expect_ident()?.to_string();
        Ok(Decl::Strict { mode, span: Some(span) })
    }

    fn parse_test_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Test)?;
        let name = self.current().value.clone();
        self.expect(TokenType::String)?;
        let body = self.parse_brace_expr()?;
        Ok(Decl::Test { name, body, span: Some(span) })
    }

    fn parse_visibility(&mut self) -> Visibility {
        if self.check(TokenType::Local) { self.advance(); Visibility::Local }
        else if self.check(TokenType::Mod) { self.advance(); Visibility::Mod }
        else { Visibility::Public }
    }

    pub(crate) fn parse_param_list(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        if self.check(TokenType::RParen) { return Ok(params); }

        if self.check_ident("self") {
            params.push(Param {
                name: sym("self"),
                ty: TypeExpr::Simple { name: sym("Self") },
                default: None,
            });
            self.advance();
            if self.check(TokenType::Comma) { self.advance(); }
        }

        let mut has_default = false;
        while !self.check(TokenType::RParen) {
            self.skip_newlines();
            if self.check(TokenType::RParen) { break; }
            let param_name = self.expect_any_param_name()?;
            self.expect(TokenType::Colon)?;
            let param_type = self.parse_type_expr()?;
            let default = if self.check(TokenType::Eq) {
                self.advance();
                has_default = true;
                Some(Box::new(self.parse_expr()?))
            } else {
                if has_default {
                    return Err(format!("parameter '{}' must have a default value (all parameters after the first default must also have defaults)", param_name));
                }
                None
            };
            params.push(Param { name: param_name, ty: param_type, default });
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            } else { break; }
        }
        self.skip_newlines();
        Ok(params)
    }
}
