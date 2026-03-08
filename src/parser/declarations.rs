use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    // ---- Module & Import ----

    pub(crate) fn parse_module_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Module)?;
        let path = self.parse_module_path()?;
        Ok(Decl::Module { path, span: Some(span) })
    }

    pub(crate) fn parse_import_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Import)?;
        let path = self.parse_module_path()?;

        if self.check(TokenType::Dot) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace) {
            self.advance();
            self.expect(TokenType::LBrace)?;
            let mut names = Vec::new();
            names.push(self.expect_any_name()?);
            while self.check(TokenType::Comma) {
                self.advance();
                if self.check(TokenType::RBrace) {
                    break;
                }
                names.push(self.expect_any_name()?);
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Decl::Import { path, names: Some(names), span: Some(span) });
        }

        Ok(Decl::Import { path, names: None, span: Some(span) })
    }

    fn parse_module_path(&mut self) -> Result<Vec<String>, String> {
        let mut parts = Vec::new();
        parts.push(self.expect_ident()?);
        while self.check(TokenType::Dot) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Ident) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        Ok(parts)
    }

    // ---- Top-level Declarations ----

    pub(crate) fn parse_top_decl(&mut self) -> Result<Decl, String> {
        if self.check(TokenType::Type) {
            return self.parse_type_decl();
        }
        if self.check(TokenType::Trait) {
            return self.parse_trait_decl();
        }
        if self.check(TokenType::Impl) {
            return self.parse_impl_decl();
        }
        if self.check(TokenType::Fn) || self.check(TokenType::Pub) || self.check(TokenType::Effect) || self.check(TokenType::Async) {
            return self.parse_fn_decl();
        }
        if self.check(TokenType::Strict) {
            return self.parse_strict_decl();
        }
        if self.check(TokenType::Test) {
            return self.parse_test_decl();
        }
        let tok = self.current();
        let hint = match tok.value.as_str() {
            "class" | "struct" => "\n  Hint: Use 'type Name = { field: Type, ... }' for record types, or 'type Name = | Case1 | Case2' for variants.",
            "def" | "func" | "function" => "\n  Hint: Use 'fn name(...) -> Type = expr' or 'effect fn name(...) -> Result[T, E] = expr'.",
            "while" | "for" | "loop" => "\n  Hint: Almide has no top-level loops. Define a function with 'fn' or 'effect fn'.",
            "const" | "val" => "\n  Hint: Use 'let' for immutable bindings, 'var' for mutable ones (inside functions).",
            _ => "",
        };
        Err(format!(
            "Expected top-level declaration (fn, effect fn, type, trait, impl, test) at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    fn parse_type_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Type)?;
        let name = self.expect_type_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let ty = self.parse_type_expr()?;
        self.skip_newlines();
        let mut deriving: Option<Vec<String>> = None;
        if self.check(TokenType::Deriving) {
            self.advance();
            let mut d = Vec::new();
            d.push(self.expect_type_name()?);
            while self.check(TokenType::Comma) {
                self.advance();
                d.push(self.expect_type_name()?);
            }
            deriving = Some(d);
        }
        Ok(Decl::Type { name, ty, deriving, span: Some(span) })
    }

    fn parse_trait_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Trait)?;
        let name = self.expect_type_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut methods: Vec<serde_json::Value> = Vec::new();
        while !self.check(TokenType::RBrace) {
            methods.push(self.parse_trait_method()?);
            self.skip_newlines();
        }
        self.expect(TokenType::RBrace)?;
        Ok(Decl::Trait { name, methods, span: Some(span) })
    }

    fn parse_trait_method(&mut self) -> Result<serde_json::Value, String> {
        let mut async_ = false;
        if self.check(TokenType::Async) {
            self.advance();
            async_ = true;
        }
        let mut effect = false;
        if self.check(TokenType::Effect) {
            self.advance();
            effect = true;
        }
        self.expect(TokenType::Fn)?;
        let name = self.expect_any_fn_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::LParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type_expr()?;

        let mut map = serde_json::Map::new();
        map.insert("name".to_string(), serde_json::Value::String(name));
        if async_ {
            map.insert("async".to_string(), serde_json::Value::Bool(true));
        }
        if effect {
            map.insert("effect".to_string(), serde_json::Value::Bool(true));
        }
        let params_json: Vec<serde_json::Value> = params
            .iter()
            .map(|p| {
                let mut pm = serde_json::Map::new();
                pm.insert("name".to_string(), serde_json::Value::String(p.name.clone()));
                if let Ok(ty_json) = serde_json::to_value(&p.ty) {
                    pm.insert("type".to_string(), ty_json);
                }
                serde_json::Value::Object(pm)
            })
            .collect();
        map.insert("params".to_string(), serde_json::Value::Array(params_json));
        if let Ok(rt_json) = serde_json::to_value(&return_type) {
            map.insert("returnType".to_string(), rt_json);
        }
        Ok(serde_json::Value::Object(map))
    }

    fn parse_impl_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Impl)?;
        let trait_name = self.expect_type_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::For)?;
        let for_name = self.expect_type_name()?;
        if self.check(TokenType::LBracket) {
            self.parse_type_args()?;
        }
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut methods = Vec::new();
        while !self.check(TokenType::RBrace) {
            methods.push(self.parse_fn_decl()?);
            self.skip_newlines();
        }
        self.expect(TokenType::RBrace)?;
        Ok(Decl::Impl {
            trait_: trait_name,
            for_: for_name,
            methods,
            span: Some(span),
        })
    }

    pub(crate) fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        if self.check(TokenType::Pub) {
            self.advance();
        }
        let mut async_ = false;
        if self.check(TokenType::Async) {
            self.advance();
            async_ = true;
        }
        let mut effect = false;
        if self.check(TokenType::Effect) {
            self.advance();
            effect = true;
        }
        self.expect(TokenType::Fn)?;
        let name = self.expect_any_fn_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::LParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type_expr()?;
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let mut body = self.parse_expr()?;

        let returns_result = matches!(&return_type,
            TypeExpr::Generic { name, .. } if name == "Result"
        );
        if effect && returns_result {
            if let Expr::Block { ref stmts, ref expr, .. } = body {
                let (effective_stmts, effective_expr) = if expr.is_none() && !stmts.is_empty() {
                    // Find last non-comment stmt
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
                    body = Expr::Block {
                        stmts: new_stmts,
                        expr: Some(Box::new(Expr::Ok { expr: Box::new(Expr::Unit { span: None, resolved_type: None }), span: None, resolved_type: None })),
                        span: None, resolved_type: None,
                    };
                } else if expr.is_none() {
                    body = Expr::Block {
                        stmts: effective_stmts,
                        expr: effective_expr,
                        span: None, resolved_type: None,
                    };
                }
            }
        }

        Ok(Decl::Fn {
            name,
            r#async: if async_ { Some(true) } else { None },
            effect: if effect { Some(true) } else { None },
            params,
            return_type,
            body,
            span: Some(span),
        })
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

    pub(crate) fn parse_param_list(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        if self.check(TokenType::RParen) {
            return Ok(params);
        }

        if self.check_ident("self") {
            params.push(Param {
                name: "self".to_string(),
                ty: TypeExpr::Simple { name: "Self".to_string() },
            });
            self.advance();
            if self.check(TokenType::Comma) {
                self.advance();
            }
        }

        while !self.check(TokenType::RParen) {
            let param_name = self.expect_any_param_name()?;
            self.expect(TokenType::Colon)?;
            let param_type = self.parse_type_expr()?;
            params.push(Param {
                name: param_name,
                ty: param_type,
            });
            if self.check(TokenType::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(params)
    }
}
