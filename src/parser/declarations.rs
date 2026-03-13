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

        // Detect JS-style `import { name }` — Almide uses `import name`
        if self.check(TokenType::LBrace) {
            let tok = self.current();
            return Err(format!(
                "Unexpected '{{' in import at line {}:{}\n  Hint: Almide imports don't use braces. Write: import json (not import {{ json }})",
                tok.line, tok.col
            ));
        }

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
            return Ok(Decl::Import { path, names: Some(names), alias: None, span: Some(span) });
        }

        // `as` alias: import self.http.client as c
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
        while self.check(TokenType::Dot) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Ident) {
            self.advance();
            parts.push(self.expect_ident()?);
        }
        Ok(parts)
    }

    // ---- Top-level Declarations ----

    pub(crate) fn parse_top_decl(&mut self) -> Result<Decl, String> {
        // Collect @extern annotations
        if self.check(TokenType::At) {
            let extern_attrs = self.collect_extern_attrs()?;
            return self.parse_fn_decl_with_attrs(extern_attrs);
        }
        if self.check(TokenType::Type) {
            return self.parse_type_decl();
        }
        if self.check(TokenType::Trait) {
            return self.parse_trait_decl();
        }
        if self.check(TokenType::Impl) {
            return self.parse_impl_decl();
        }
        // Top-level `let` (module-scope constant): `let NAME = expr` or `pub let NAME: Type = expr`
        if self.check(TokenType::Let) {
            return self.parse_top_let(Visibility::Public);
        }
        if self.check(TokenType::Fn) || self.check(TokenType::Pub) || self.check(TokenType::Effect) || self.check(TokenType::Async) || self.check(TokenType::Local) || self.check(TokenType::Mod) {
            // `pub let` — public top-level constant
            if self.check(TokenType::Pub) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Let) {
                self.advance(); // consume `pub`
                return self.parse_top_let(Visibility::Public);
            }
            // local/mod can precede let, fn, and type
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
        let hint = match tok.value.as_str() {
            "class" | "struct" => "\n  Hint: Use 'type Name = { field: Type, ... }' for record types, or 'type Name = | Case1 | Case2' for variants.",
            "def" | "func" | "function" | "fun" | "proc" => "\n  Hint: Use 'fn name(...) -> Type = expr' or 'effect fn name(...) -> Result[T, E] = expr'.",
            "while" | "for" | "loop" => "\n  Hint: Almide has no top-level loops. Define a function with 'fn' or 'effect fn'.",
            "const" | "val" | "var" => "\n  Hint: Use 'let NAME = value' for top-level constants, or 'let' inside functions for local bindings.",
            "enum" | "data" | "sealed" | "union" => "\n  Hint: Use 'type Name = | Case1(T) | Case2(T)' for variant types.",
            "interface" | "protocol" | "abstract" => "\n  Hint: Use 'trait Name { ... }' for traits.",
            "return" => "\n  Hint: Almide functions return the last expression — no 'return' keyword needed.",
            "import" => "\n  Hint: All imports must come before other declarations.",
            _ => "",
        };
        Err(format!(
            "Expected top-level declaration (fn, effect fn, type, let, trait, impl, test) at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    fn parse_top_let(&mut self, visibility: Visibility) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Let)?;
        let name = self.expect_any_name()?;
        // Optional type annotation: `let NAME: Type = expr`
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
            self.advance(); // skip @
            if !self.check_ident("extern") {
                let tok = self.current();
                return Err(format!("Expected 'extern' after '@' at line {}:{}", tok.line, tok.col));
            }
            self.advance(); // skip "extern"
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
            self.expect(TokenType::RParen)?;
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
        Ok(Decl::Type { name, ty, deriving, visibility, generics, span: Some(span) })
    }

    fn parse_trait_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        self.expect(TokenType::Trait)?;
        let name = self.expect_type_name()?;
        let generics = self.try_parse_generic_params()?;
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut methods: Vec<serde_json::Value> = Vec::new();
        while !self.check(TokenType::RBrace) {
            methods.push(self.parse_trait_method()?);
            self.skip_newlines();
        }
        self.expect(TokenType::RBrace)?;
        Ok(Decl::Trait { name, generics, methods, span: Some(span) })
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
        let generics = self.try_parse_generic_params()?;
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
            generics,
            methods,
            span: Some(span),
        })
    }

    pub(crate) fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        let span = self.current_span();
        if self.check(TokenType::Pub) {
            self.advance(); // pub is default, just consume it
        }
        let visibility = self.parse_visibility();
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
        let generics = self.try_parse_generic_params()?;
        self.expect(TokenType::LParen)?;
        let params = self.parse_param_list()?;
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Arrow)?;
        let return_type = self.parse_type_expr()?;

        // Detect missing `=` before body: `fn name() -> T { ... }` instead of `fn name() -> T = { ... }`
        if self.check(TokenType::LBrace) {
            let tok = self.current();
            return Err(format!(
                "Missing '=' before function body at line {}:{}\n  Hint: Almide requires '=' before the body. Write: fn {}(...) -> Type = {{ ... }}",
                tok.line, tok.col, name
            ));
        }

        // Body is optional — @extern-only functions have no `= expr`
        let body = if self.check(TokenType::Eq) {
            self.advance();
            self.skip_newlines();
            let mut body = if self.check(TokenType::Let) || self.check(TokenType::Var) {
                self.parse_braceless_block()?
            } else {
                self.parse_expr()?
            };

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
            generics,
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

    fn parse_visibility(&mut self) -> Visibility {
        if self.check(TokenType::Local) {
            self.advance();
            Visibility::Local
        } else if self.check(TokenType::Mod) {
            self.advance();
            Visibility::Mod
        } else {
            Visibility::Public
        }
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
