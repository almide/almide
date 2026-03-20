use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

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

    fn parse_module_path(&mut self) -> Result<Vec<String>, String> {
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
            let extern_attrs = self.collect_extern_attrs()?;
            return self.parse_fn_decl_with_attrs(extern_attrs);
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

    fn collect_extern_attrs(&mut self) -> Result<Vec<ExternAttr>, String> {
        let mut attrs = Vec::new();
        while self.check(TokenType::At) {
            self.advance();
            if !self.check_ident("extern") {
                let tok = self.current();
                return Err(format!("Expected 'extern' after '@' at line {}:{}", tok.line, tok.col));
            }
            self.advance();
            let open_ext = self.current().clone();
            self.expect(TokenType::LParen)?;
            let target = self.expect_ident()?;
            self.expect(TokenType::Comma)?;
            let module = {
                let tok = self.current();
                if tok.token_type != TokenType::String {
                    return Err(format!("Expected string literal for extern module at line {}:{}", tok.line, tok.col));
                }
                let val = tok.value.clone();
                self.advance();
                val
            };
            self.expect(TokenType::Comma)?;
            let function = {
                let tok = self.current();
                if tok.token_type != TokenType::String {
                    return Err(format!("Expected string literal for extern function at line {}:{}", tok.line, tok.col));
                }
                let val = tok.value.clone();
                self.advance();
                val
            };
            self.expect_closing(TokenType::RParen, open_ext.line, open_ext.col, "@extern annotation")?;
            attrs.push(ExternAttr { target, module, function });
            self.skip_newlines();
        }
        Ok(attrs)
    }

    fn parse_fn_decl_with_attrs(&mut self, extern_attrs: Vec<ExternAttr>) -> Result<Decl, String> {
        let mut decl = self.parse_fn_decl()?;
        if let Decl::Fn { extern_attrs: ref mut attrs, .. } = decl {
            *attrs = extern_attrs;
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
            name,
            r#async: if async_ { Some(true) } else { None },
            effect: if effect { Some(true) } else { None },
            visibility,
            extern_attrs: Vec::new(),
            generics, params, return_type, body,
            span: Some(span),
        })
    }

    fn wrap_effect_result_body(&mut self, body: Expr) -> Expr {
        if let Expr::Block { ref stmts, ref expr, .. } = body {
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
                Some(e) => matches!(e.as_ref(), Expr::Unit { .. }),
            };
            if needs_ok {
                let mut new_stmts = effective_stmts;
                if let Some(trailing) = effective_expr {
                    new_stmts.push(Stmt::Expr { expr: *trailing, span: None });
                }
                return Expr::Block {
                    stmts: new_stmts,
                    expr: Some(Box::new(Expr::Ok {
                        expr: Box::new(Expr::Unit { id: self.next_id(), span: None, resolved_type: None }),
                        id: self.next_id(), span: None, resolved_type: None,
                    })),
                    id: self.next_id(), span: None, resolved_type: None,
                };
            } else if expr.is_none() {
                return Expr::Block {
                    stmts: effective_stmts, expr: effective_expr,
                    id: self.next_id(), span: None, resolved_type: None,
                };
            }
        }
        body
    }

    fn parse_strict_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Strict)?;
        let mode = self.expect_ident()?;
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
                name: "self".to_string(),
                ty: TypeExpr::Simple { name: "Self".to_string() },
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
