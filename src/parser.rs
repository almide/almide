use crate::lexer::{Token, TokenType};
use crate::ast::*;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse_single_expr(&mut self) -> Result<Expr, String> {
        self.parse_expr()
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program {
            module: None,
            imports: Vec::new(),
            decls: Vec::new(),
        };

        self.skip_newlines();

        // Module declaration (optional)
        if self.check(TokenType::Module) {
            program.module = Some(self.parse_module_decl()?);
            self.skip_newlines();
        }

        // Import declarations
        while self.check(TokenType::Import) {
            program.imports.push(self.parse_import_decl()?);
            self.skip_newlines();
        }

        // Top-level declarations
        while !self.check(TokenType::EOF) {
            self.skip_newlines();
            if self.check(TokenType::EOF) {
                break;
            }
            program.decls.push(self.parse_top_decl()?);
            self.skip_newlines();
        }

        Ok(program)
    }

    // ---- Module & Import ----

    fn parse_module_decl(&mut self) -> Result<Decl, String> {
        self.expect(TokenType::Module)?;
        let path = self.parse_module_path()?;
        Ok(Decl::Module { path })
    }

    fn parse_import_decl(&mut self) -> Result<Decl, String> {
        self.expect(TokenType::Import)?;
        let path = self.parse_module_path()?;

        // Check for selective import: import foo.{Bar, Baz}
        if self.check(TokenType::Dot) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace) {
            self.advance(); // skip .
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
            return Ok(Decl::Import { path, names: Some(names) });
        }

        Ok(Decl::Import { path, names: None })
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

    fn parse_top_decl(&mut self) -> Result<Decl, String> {
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
        self.expect(TokenType::Type)?;
        let name = self.expect_type_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let ty = self.parse_type_expr()?;
        // Check for deriving clause
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
        Ok(Decl::Type { name, ty, deriving })
    }

    fn parse_trait_decl(&mut self) -> Result<Decl, String> {
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
        Ok(Decl::Trait { name, methods })
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

        // Build as serde_json::Value since that's what ast.rs expects for Trait methods
        let mut map = serde_json::Map::new();
        map.insert("name".to_string(), serde_json::Value::String(name));
        if async_ {
            map.insert("async".to_string(), serde_json::Value::Bool(true));
        }
        if effect {
            map.insert("effect".to_string(), serde_json::Value::Bool(true));
        }
        // Serialize params
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
        self.expect(TokenType::Impl)?;
        let trait_name = self.expect_type_name()?;
        let _generics = self.try_parse_generic_params()?;
        self.expect(TokenType::For)?;
        let for_name = self.expect_type_name()?;
        // Skip generic args on for type if present
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
        })
    }

    fn parse_fn_decl(&mut self) -> Result<Decl, String> {
        // optional pub
        if self.check(TokenType::Pub) {
            self.advance();
        }
        // optional async
        let mut async_ = false;
        if self.check(TokenType::Async) {
            self.advance();
            async_ = true;
        }
        // optional effect
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

        // Implicit ok(()) for effect fn returning Result: auto-append ok(())
        // when body block doesn't end with an explicit ok/err expression
        let returns_result = matches!(&return_type,
            TypeExpr::Generic { name, .. } if name == "Result"
        );
        if effect && returns_result {
            if let Expr::Block { ref stmts, ref expr } = body {
                let needs_ok = match expr {
                    None => true,
                    Some(e) => !matches!(e.as_ref(), Expr::Ok { .. } | Expr::Err { .. }),
                };
                if needs_ok {
                    let mut new_stmts = stmts.clone();
                    if let Some(trailing) = expr {
                        new_stmts.push(Stmt::Expr { expr: *trailing.clone() });
                    }
                    body = Expr::Block {
                        stmts: new_stmts,
                        expr: Some(Box::new(Expr::Ok { expr: Box::new(Expr::Unit) })),
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
        })
    }

    fn parse_strict_decl(&mut self) -> Result<Decl, String> {
        self.expect(TokenType::Strict)?;
        let mode = self.expect_ident()?;
        Ok(Decl::Strict { mode })
    }

    fn parse_test_decl(&mut self) -> Result<Decl, String> {
        self.expect(TokenType::Test)?;
        let name = self.current().value.clone();
        self.expect(TokenType::String)?;
        let body = self.parse_brace_expr()?;
        Ok(Decl::Test { name, body })
    }

    // ---- Params ----

    fn parse_param_list(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        if self.check(TokenType::RParen) {
            return Ok(params);
        }

        // Handle 'self' as first param
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

    // ---- Types ----

    fn parse_type_expr(&mut self) -> Result<TypeExpr, String> {
        // Check for newtype
        if self.check(TokenType::Newtype) {
            self.advance();
            let inner = self.parse_type_expr()?;
            return Ok(TypeExpr::Newtype { inner: Box::new(inner) });
        }

        // Check for variant type (starts with |)
        if self.check(TokenType::Pipe) {
            return self.parse_variant_type();
        }

        // Check for record type
        if self.check(TokenType::LBrace) {
            return self.parse_record_type();
        }

        // Check for fn type
        if self.check(TokenType::Fn) {
            return self.parse_fn_type();
        }

        // Simple or generic type (may start an inline variant)
        let name = self.expect_type_name()?;
        if self.check(TokenType::LBracket) {
            let args = self.parse_type_args()?;
            // Check for inline variant: Name[T] | ...
            if self.check(TokenType::Pipe) {
                return self.try_parse_inline_variant(name, Vec::new());
            }
            return Ok(TypeExpr::Generic { name, args });
        }
        // Check for inline variant: Name(T) | Name2(T2)
        if self.check(TokenType::LParen) {
            self.advance();
            let mut fields = Vec::new();
            if !self.check(TokenType::RParen) {
                fields.push(self.parse_type_expr()?);
                while self.check(TokenType::Comma) {
                    self.advance();
                    fields.push(self.parse_type_expr()?);
                }
            }
            self.expect(TokenType::RParen)?;
            // If followed by |, it's an inline variant
            if self.check(TokenType::Pipe) {
                return self.try_parse_inline_variant(name, fields);
            }
            // Otherwise just a simple type
            return Ok(TypeExpr::Simple { name });
        }
        // Check for unit inline variant: Name | Name2
        if self.check(TokenType::Pipe) {
            return self.try_parse_inline_variant(name, Vec::new());
        }
        Ok(TypeExpr::Simple { name })
    }

    fn parse_variant_type(&mut self) -> Result<TypeExpr, String> {
        let mut cases = Vec::new();
        while self.check(TokenType::Pipe) {
            self.advance(); // skip |
            self.skip_newlines();
            let case_name = self.expect_type_name()?;
            if self.check(TokenType::LParen) {
                self.advance();
                let mut fields = Vec::new();
                if !self.check(TokenType::RParen) {
                    fields.push(self.parse_type_expr()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        fields.push(self.parse_type_expr()?);
                    }
                }
                self.expect(TokenType::RParen)?;
                cases.push(VariantCase::Tuple { name: case_name, fields });
            } else if self.check(TokenType::LBrace) {
                self.advance();
                let fields = self.parse_field_type_list()?;
                self.expect(TokenType::RBrace)?;
                cases.push(VariantCase::Record { name: case_name, fields });
            } else {
                cases.push(VariantCase::Unit { name: case_name });
            }
            self.skip_newlines();
        }
        Ok(TypeExpr::Variant { cases })
    }

    fn try_parse_inline_variant(&mut self, first_name: String, first_args: Vec<TypeExpr>) -> Result<TypeExpr, String> {
        let mut cases = Vec::new();
        if !first_args.is_empty() {
            cases.push(VariantCase::Tuple { name: first_name, fields: first_args });
        } else {
            cases.push(VariantCase::Unit { name: first_name });
        }
        while self.check(TokenType::Pipe) {
            self.advance();
            self.skip_newlines();
            let case_name = self.expect_type_name()?;
            if self.check(TokenType::LParen) {
                self.advance();
                let mut fields = Vec::new();
                if !self.check(TokenType::RParen) {
                    fields.push(self.parse_type_expr()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        fields.push(self.parse_type_expr()?);
                    }
                }
                self.expect(TokenType::RParen)?;
                cases.push(VariantCase::Tuple { name: case_name, fields });
            } else if self.check(TokenType::LBrace) {
                self.advance();
                let fields = self.parse_field_type_list()?;
                self.expect(TokenType::RBrace)?;
                cases.push(VariantCase::Record { name: case_name, fields });
            } else {
                cases.push(VariantCase::Unit { name: case_name });
            }
            self.skip_newlines();
        }
        Ok(TypeExpr::Variant { cases })
    }

    fn parse_record_type(&mut self) -> Result<TypeExpr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let fields = self.parse_field_type_list()?;
        self.skip_newlines();
        self.expect(TokenType::RBrace)?;
        Ok(TypeExpr::Record { fields })
    }

    fn parse_field_type_list(&mut self) -> Result<Vec<FieldType>, String> {
        let mut fields = Vec::new();
        while !self.check(TokenType::RBrace) {
            self.skip_newlines();
            let field_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            let field_type = self.parse_type_expr()?;
            fields.push(FieldType { name: field_name, ty: field_type });
            self.skip_newlines();
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        Ok(fields)
    }

    fn parse_fn_type(&mut self) -> Result<TypeExpr, String> {
        self.expect(TokenType::Fn)?;
        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.parse_type_expr()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_type_expr()?);
            }
        }
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::Arrow)?;
        let ret = self.parse_type_expr()?;
        Ok(TypeExpr::Fn { params, ret: Box::new(ret) })
    }

    fn parse_type_args(&mut self) -> Result<Vec<TypeExpr>, String> {
        self.expect(TokenType::LBracket)?;
        let mut args = Vec::new();
        if !self.check(TokenType::RBracket) {
            args.push(self.parse_type_expr()?);
            while self.check(TokenType::Comma) {
                self.advance();
                args.push(self.parse_type_expr()?);
            }
        }
        self.expect(TokenType::RBracket)?;
        Ok(args)
    }

    fn try_parse_generic_params(&mut self) -> Result<Option<Vec<GenericParam>>, String> {
        if !self.check(TokenType::LBracket) {
            return Ok(None);
        }
        self.advance();
        let mut params = Vec::new();
        if !self.check(TokenType::RBracket) {
            params.push(self.parse_generic_param()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_generic_param()?);
            }
        }
        self.expect(TokenType::RBracket)?;
        Ok(Some(params))
    }

    fn parse_generic_param(&mut self) -> Result<GenericParam, String> {
        let name = self.expect_type_name()?;
        let mut bounds = Vec::new();
        if self.check(TokenType::Colon) {
            self.advance();
            bounds.push(self.expect_type_name()?);
            while self.check(TokenType::Plus) {
                self.advance();
                bounds.push(self.expect_type_name()?);
            }
        }
        Ok(GenericParam {
            name,
            bounds: if bounds.is_empty() { None } else { Some(bounds) },
        })
    }

    // ---- Statements ----

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if self.check(TokenType::Let) {
            return self.parse_let_stmt();
        }
        if self.check(TokenType::Var) {
            return self.parse_var_stmt();
        }
        if self.check(TokenType::Guard) {
            return self.parse_guard_stmt();
        }

        // Try assign: ident = expr (but not ==)
        if self.check(TokenType::Ident)
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq)
            && self.peek_at(2).map(|t| &t.token_type) != Some(&TokenType::Eq)
        {
            return self.parse_assign_stmt();
        }

        let expr = self.parse_expr()?;
        Ok(Stmt::Expr { expr })
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenType::Let)?;

        // Destructuring: let { a, b } = expr
        if self.check(TokenType::LBrace) {
            self.advance();
            let mut fields = Vec::new();
            while !self.check(TokenType::RBrace) {
                fields.push(self.expect_ident()?);
                if self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            self.expect(TokenType::Eq)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Stmt::LetDestructure { fields, value });
        }

        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Let { name, ty, value })
    }

    fn parse_var_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenType::Var)?;
        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Var { name, ty, value })
    }

    fn parse_guard_stmt(&mut self) -> Result<Stmt, String> {
        self.expect(TokenType::Guard)?;
        let cond = self.parse_expr()?;
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let else_ = self.parse_expr()?;
        Ok(Stmt::Guard { cond, else_ })
    }

    fn parse_assign_stmt(&mut self) -> Result<Stmt, String> {
        let name = self.current().value.clone();
        self.advance();
        self.expect(TokenType::Eq)?;
        self.skip_newlines();
        let value = self.parse_expr()?;
        Ok(Stmt::Assign { name, value })
    }

    // ---- Expressions ----

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_pipe()
    }

    fn parse_pipe(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_or()?;
        while self.check(TokenType::PipeArrow) {
            self.advance();
            self.skip_newlines();
            // Support |> match { ... }
            if self.check(TokenType::Match) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LBrace) {
                self.advance(); // consume 'match'
                self.skip_newlines();
                self.expect(TokenType::LBrace)?;
                self.skip_newlines();
                let mut arms = Vec::new();
                while !self.check(TokenType::RBrace) {
                    arms.push(self.parse_match_arm()?);
                    self.skip_newlines();
                    if self.check(TokenType::Comma) {
                        self.advance();
                        self.skip_newlines();
                    }
                }
                self.expect(TokenType::RBrace)?;
                left = Expr::Match {
                    subject: Box::new(left),
                    arms,
                };
            } else {
                let right = self.parse_or()?;
                left = Expr::Pipe {
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while self.check(TokenType::Or) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: "or".to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while self.check(TokenType::And) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: "and".to_string(),
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_add_sub()?;
        while self.check(TokenType::EqEq)
            || self.check(TokenType::BangEq)
            || self.check(TokenType::LAngle)
            || self.check(TokenType::RAngle)
            || self.check(TokenType::LtEq)
            || self.check(TokenType::GtEq)
        {
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_add_sub()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_add_sub(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_mul_div()?;
        while self.check(TokenType::Plus) || self.check(TokenType::Minus) || self.check(TokenType::PlusPlus) {
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_mul_div()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        while self.check(TokenType::Star) || self.check(TokenType::Slash) || self.check(TokenType::Percent) || self.check(TokenType::Caret) {
            let op = self.current().value.clone();
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Minus) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "-".to_string(),
                operand: Box::new(operand),
            });
        }
        if self.check(TokenType::Not) {
            self.advance();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary {
                op: "not".to_string(),
                operand: Box::new(operand),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.check(TokenType::Dot) {
                self.advance();
                let field = self.expect_any_name()?;
                expr = Expr::Member {
                    object: Box::new(expr),
                    field,
                };
            } else if self.check(TokenType::LParen) {
                self.advance();
                let args = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        self.skip_newlines();
        if self.check(TokenType::RParen) {
            return Ok(args);
        }

        self.parse_one_call_arg(&mut args)?;
        while self.check(TokenType::Comma) {
            self.advance();
            self.skip_newlines();
            if self.check(TokenType::RParen) {
                break;
            }
            self.parse_one_call_arg(&mut args)?;
        }
        self.skip_newlines();
        Ok(args)
    }

    fn parse_one_call_arg(&mut self, args: &mut Vec<Expr>) -> Result<(), String> {
        // Placeholder: _ in call args
        if self.check(TokenType::Underscore) {
            self.advance();
            args.push(Expr::Placeholder);
            return Ok(());
        }
        // Named arg: ident ":" expr — treat as positional since AST doesn't have namedArgs
        if self.check(TokenType::Ident) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon) {
            // Skip name and colon, just parse the value expression
            self.advance(); // skip name
            self.advance(); // skip :
            self.skip_newlines();
            let value = self.parse_expr()?;
            args.push(value);
        } else {
            args.push(self.parse_expr()?);
        }
        Ok(())
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.current().clone();

        // Literals
        if self.check(TokenType::Int) {
            self.advance();
            return Ok(Expr::Int {
                value: serde_json::Value::Number(
                    tok.value.parse::<i64>()
                        .ok()
                        .and_then(|n| serde_json::Number::from_f64(n as f64))
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
                raw: tok.value.clone(),
            });
        }
        if self.check(TokenType::Float) {
            self.advance();
            let v: f64 = tok.value.parse().unwrap_or(0.0);
            return Ok(Expr::Float { value: v });
        }
        if self.check(TokenType::String) {
            self.advance();
            return Ok(Expr::String { value: tok.value.clone() });
        }
        if self.check(TokenType::InterpolatedString) {
            self.advance();
            return Ok(Expr::InterpolatedString { value: tok.value.clone() });
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Expr::Bool { value: true });
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Expr::Bool { value: false });
        }

        // Hole
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Expr::Hole);
        }

        // None
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Expr::None);
        }

        // Some(expr)
        if self.check(TokenType::Some) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Some { expr: Box::new(expr) });
        }

        // Ok(expr)
        if self.check(TokenType::Ok) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Ok { expr: Box::new(expr) });
        }

        // Err(expr)
        if self.check(TokenType::Err) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Err { expr: Box::new(expr) });
        }

        // Todo
        if self.check(TokenType::Todo) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let msg = self.current().value.clone();
            self.expect(TokenType::String)?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Todo { message: msg });
        }

        // Try
        if self.check(TokenType::Try) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Try { expr: Box::new(expr) });
        }

        // Await
        if self.check(TokenType::Await) {
            self.advance();
            let expr = self.parse_postfix()?;
            return Ok(Expr::Await { expr: Box::new(expr) });
        }

        // If
        if self.check(TokenType::If) {
            return self.parse_if_expr();
        }

        // Match
        if self.check(TokenType::Match) {
            return self.parse_match_expr();
        }

        // Lambda: fn(...) => expr
        if self.check(TokenType::Fn) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::LParen) {
            return self.parse_lambda();
        }

        // For...in loop
        if self.check(TokenType::For) {
            self.advance();
            let var_name = self.expect_ident()?;
            self.expect(TokenType::In)?;
            let iterable = self.parse_expr()?;
            self.expect(TokenType::LBrace)?;
            self.skip_newlines();
            let mut stmts = Vec::new();
            while !self.check(TokenType::RBrace) {
                stmts.push(self.parse_stmt()?);
                self.skip_newlines();
                if self.check(TokenType::Semicolon) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::ForIn {
                var: var_name,
                iterable: Box::new(iterable),
                body: stmts,
            });
        }

        // Do block
        if self.check(TokenType::Do) {
            self.advance();
            return self.parse_do_block();
        }

        // Block or Record/Spread
        if self.check(TokenType::LBrace) {
            return self.parse_brace_expr();
        }

        // List
        if self.check(TokenType::LBracket) {
            return self.parse_list_expr();
        }

        // Paren or Unit ()
        if self.check(TokenType::LParen) {
            self.advance();
            if self.check(TokenType::RParen) {
                self.advance();
                return Ok(Expr::Unit);
            }
            let expr = self.parse_expr()?;
            self.expect(TokenType::RParen)?;
            return Ok(Expr::Paren { expr: Box::new(expr) });
        }

        // Type constructor (call)
        if self.check(TokenType::TypeName) {
            let name = tok.value.clone();
            self.advance();

            // Generic call: TypeName[...](...)
            if self.check(TokenType::LBracket) {
                self.parse_type_args()?; // consume type args
                if self.check(TokenType::LParen) {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(TokenType::RParen)?;
                    return Ok(Expr::Call {
                        callee: Box::new(Expr::TypeName { name }),
                        args,
                    });
                }
                return Ok(Expr::TypeName { name });
            }

            // Simple call: TypeName(...)
            if self.check(TokenType::LParen) {
                self.advance();
                let args = self.parse_call_args()?;
                self.expect(TokenType::RParen)?;
                return Ok(Expr::Call {
                    callee: Box::new(Expr::TypeName { name }),
                    args,
                });
            }

            return Ok(Expr::TypeName { name });
        }

        // Reject '!' with helpful hint
        if self.check(TokenType::Bang) {
            return Err(format!(
                "'!' is not valid in Almide at line {}:{}\n  Hint: Use 'not x' for boolean negation, not '!x'.",
                tok.line, tok.col
            ));
        }

        // Reject known invalid keywords/identifiers with helpful hints
        if self.check(TokenType::Ident) {
            let rejected_hint = match tok.value.as_str() {
                "while" | "loop" => Some("Almide has no 'while' or 'loop'. Use 'do { guard COND else ok(()) ... }' for loops."),
                "return" => Some("Almide has no 'return'. The last expression in a block is the return value. Use 'guard ... else' for early returns."),
                "print" => Some("Use 'println(s)' instead of 'print'. There is no 'print' function in Almide."),
                "null" | "nil" => Some("Almide has no null. Use Option[T] with 'some(v)' / 'none'."),
                "throw" => Some("Almide has no exceptions. Use Result[T, E] with 'ok(v)' / 'err(e)'."),
                "catch" | "except" => Some("Almide has no try/catch. Use 'match' on Result values instead."),
                _ => None,
            };
            if let Some(hint) = rejected_hint {
                return Err(format!(
                    "'{}' is not valid in Almide at line {}:{}\n  Hint: {}",
                    tok.value, tok.line, tok.col, hint
                ));
            }
        }

        // Identifier (with ? or !)
        if self.check(TokenType::Ident) || self.check(TokenType::IdentQ) {
            let name = tok.value.clone();
            self.advance();
            return Ok(Expr::Ident { name });
        }

        let hint = match tok.value.as_str() {
            "while" | "loop" => "\n  Hint: Almide has no 'while' or 'loop'. Use 'do { guard COND else ok(()) ... }' for loops.",
            "for" => "\n  Hint: Use 'list.each(xs, fn(x) => ...)' or 'do { guard ... }' instead of 'for'.",
            "return" => "\n  Hint: Almide has no 'return'. The last expression in a block is the return value. Use 'guard ... else' for early returns.",
            "null" | "nil" | "None" => "\n  Hint: Almide has no null. Use Option[T] with 'some(v)' / 'none'.",
            "throw" => "\n  Hint: Almide has no exceptions. Use Result[T, E] with 'ok(v)' / 'err(e)'.",
            "catch" | "except" => "\n  Hint: Almide has no try/catch. Use 'match' on Result values instead.",
            "class" | "struct" => "\n  Hint: Use 'type Name = { field: Type, ... }' for record types.",
            "print" => "\n  Hint: Use 'println(s)' instead of 'print'. There is no 'print' function in Almide.",
            _ => "",
        };
        Err(format!(
            "Expected expression at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    fn parse_if_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::If)?;
        self.skip_newlines();
        let cond = self.parse_expr()?;
        self.skip_newlines();
        self.expect(TokenType::Then)?;
        self.skip_newlines();
        let then = self.parse_if_branch()?;
        self.skip_newlines();
        self.expect(TokenType::Else)?;
        self.skip_newlines();
        let else_ = self.parse_if_branch()?;
        Ok(Expr::If {
            cond: Box::new(cond),
            then: Box::new(then),
            else_: Box::new(else_),
        })
    }

    fn parse_if_branch(&mut self) -> Result<Expr, String> {
        if self.check(TokenType::Ident) && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Eq) {
            let name = self.advance_and_get_value();
            self.advance(); // skip =
            self.skip_newlines();
            let value = self.parse_expr()?;
            return Ok(Expr::Block {
                stmts: vec![Stmt::Assign { name, value }],
                expr: None,
            });
        }
        self.parse_expr()
    }

    fn parse_match_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::Match)?;
        self.skip_newlines();
        let subject = self.parse_or()?;
        self.skip_newlines();
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(TokenType::RBrace) {
            arms.push(self.parse_match_arm()?);
            self.skip_newlines();
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::Match {
            subject: Box::new(subject),
            arms,
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        let pattern = self.parse_pattern()?;
        let mut guard: Option<Expr> = None;
        if self.check(TokenType::If) {
            self.advance();
            guard = Some(self.parse_expr()?);
        }
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(MatchArm { pattern, guard, body })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, String> {
        // Wildcard
        if self.check(TokenType::Underscore) {
            self.advance();
            return Ok(Pattern::Wildcard);
        }

        // none
        if self.check(TokenType::None) {
            self.advance();
            return Ok(Pattern::None);
        }

        // some(pattern)
        if self.check(TokenType::Some) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Some { inner: Box::new(inner) });
        }

        // ok(pattern)
        if self.check(TokenType::Ok) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Ok { inner: Box::new(inner) });
        }

        // err(pattern)
        if self.check(TokenType::Err) {
            self.advance();
            self.expect(TokenType::LParen)?;
            let inner = self.parse_pattern()?;
            self.expect(TokenType::RParen)?;
            return Ok(Pattern::Err { inner: Box::new(inner) });
        }

        // Literal patterns
        if self.check(TokenType::Int) || self.check(TokenType::Float) || self.check(TokenType::String) {
            let expr = self.parse_primary()?;
            return Ok(Pattern::Literal { value: Box::new(expr) });
        }
        if self.check(TokenType::True) {
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::Bool { value: true }),
            });
        }
        if self.check(TokenType::False) {
            self.advance();
            return Ok(Pattern::Literal {
                value: Box::new(Expr::Bool { value: false }),
            });
        }

        // Type constructor pattern
        if self.check(TokenType::TypeName) {
            let name = self.current().value.clone();
            self.advance();
            if self.check(TokenType::LParen) {
                self.advance();
                let mut args = Vec::new();
                if !self.check(TokenType::RParen) {
                    args.push(self.parse_pattern()?);
                    while self.check(TokenType::Comma) {
                        self.advance();
                        args.push(self.parse_pattern()?);
                    }
                }
                self.expect(TokenType::RParen)?;
                return Ok(Pattern::Constructor { name, args });
            }
            if self.check(TokenType::LBrace) {
                self.advance();
                self.skip_newlines();
                let mut fields = Vec::new();
                while !self.check(TokenType::RBrace) {
                    let field_name = self.expect_ident()?;
                    if self.check(TokenType::Colon) {
                        self.advance();
                        let pattern = self.parse_pattern()?;
                        fields.push(FieldPattern {
                            name: field_name,
                            pattern: Some(pattern),
                        });
                    } else {
                        fields.push(FieldPattern {
                            name: field_name,
                            pattern: None,
                        });
                    }
                    if self.check(TokenType::Comma) {
                        self.advance();
                        self.skip_newlines();
                    }
                }
                self.expect(TokenType::RBrace)?;
                return Ok(Pattern::RecordPattern { name, fields });
            }
            return Ok(Pattern::Constructor { name, args: Vec::new() });
        }

        // Identifier pattern
        if self.check(TokenType::Ident) {
            let name = self.current().value.clone();
            self.advance();
            return Ok(Pattern::Ident { name });
        }

        let tok = self.current();
        Err(format!(
            "Expected pattern at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn parse_lambda(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::Fn)?;
        self.expect(TokenType::LParen)?;
        let mut params = Vec::new();
        if !self.check(TokenType::RParen) {
            params.push(self.parse_lambda_param()?);
            while self.check(TokenType::Comma) {
                self.advance();
                params.push(self.parse_lambda_param()?);
            }
        }
        self.expect(TokenType::RParen)?;
        self.expect(TokenType::FatArrow)?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        Ok(Expr::Lambda {
            params,
            body: Box::new(body),
        })
    }

    fn parse_lambda_param(&mut self) -> Result<LambdaParam, String> {
        let name = self.expect_ident()?;
        let mut ty: Option<TypeExpr> = None;
        if self.check(TokenType::Colon) {
            self.advance();
            ty = Some(self.parse_type_expr()?);
        }
        Ok(LambdaParam { name, ty })
    }

    fn parse_do_block(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        while !self.check(TokenType::RBrace) {
            let stmt = self.parse_stmt()?;
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }

            if self.check(TokenType::RBrace) {
                if let Stmt::Expr { expr } = stmt {
                    final_expr = Some(Box::new(expr));
                } else {
                    stmts.push(stmt);
                }
            } else {
                stmts.push(stmt);
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::DoBlock {
            stmts,
            expr: final_expr,
        })
    }

    fn parse_brace_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();

        // Empty braces -> empty record
        if self.check(TokenType::RBrace) {
            self.advance();
            return Ok(Expr::Record { fields: Vec::new() });
        }

        // Spread: { ...expr, ... }
        if self.check(TokenType::DotDotDot) {
            self.advance();
            let base = self.parse_expr()?;
            let mut fields = Vec::new();
            while self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
                if self.check(TokenType::RBrace) {
                    break;
                }
                let field_name = self.expect_ident()?;
                self.expect(TokenType::Colon)?;
                self.skip_newlines();
                let field_value = self.parse_expr()?;
                fields.push(FieldInit {
                    name: field_name,
                    value: field_value,
                });
            }
            self.skip_newlines();
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::SpreadRecord {
                base: Box::new(base),
                fields,
            });
        }

        // Try to detect record: ident ":" expr
        if (self.check(TokenType::Ident) || self.check(TokenType::IdentQ))
            && self.peek_at(1).map(|t| &t.token_type) == Some(&TokenType::Colon)
        {
            // Record literal
            let mut fields = Vec::new();
            while !self.check(TokenType::RBrace) {
                self.skip_newlines();
                let field_name = self.expect_any_name()?;
                if self.check(TokenType::Colon) {
                    self.advance();
                    self.skip_newlines();
                    let field_value = self.parse_expr()?;
                    fields.push(FieldInit {
                        name: field_name.clone(),
                        value: field_value,
                    });
                } else {
                    // Shorthand: { name } == { name: name }
                    fields.push(FieldInit {
                        name: field_name.clone(),
                        value: Expr::Ident { name: field_name },
                    });
                }
                self.skip_newlines();
                if self.check(TokenType::Comma) {
                    self.advance();
                    self.skip_newlines();
                }
            }
            self.expect(TokenType::RBrace)?;
            return Ok(Expr::Record { fields });
        }

        // Block
        let mut stmts = Vec::new();
        let mut final_expr: Option<Box<Expr>> = None;

        while !self.check(TokenType::RBrace) {
            let stmt = self.parse_stmt()?;
            self.skip_newlines();
            if self.check(TokenType::Semicolon) {
                self.advance();
                self.skip_newlines();
            }

            if self.check(TokenType::RBrace) {
                if let Stmt::Expr { expr } = stmt {
                    final_expr = Some(Box::new(expr));
                } else {
                    stmts.push(stmt);
                }
            } else {
                stmts.push(stmt);
            }
        }
        self.expect(TokenType::RBrace)?;
        Ok(Expr::Block {
            stmts,
            expr: final_expr,
        })
    }

    fn parse_list_expr(&mut self) -> Result<Expr, String> {
        self.expect(TokenType::LBracket)?;
        self.skip_newlines();
        let mut elements = Vec::new();
        while !self.check(TokenType::RBracket) {
            elements.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(TokenType::RBracket)?;
        Ok(Expr::List { elements })
    }

    // ---- Helpers ----

    fn current(&self) -> &Token {
        if self.pos < self.tokens.len() {
            &self.tokens[self.pos]
        } else {
            // Return a reference to a synthetic EOF - we handle this via the last token
            self.tokens.last().unwrap_or_else(|| {
                panic!("Parser: no tokens available")
            })
        }
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn check(&self, token_type: TokenType) -> bool {
        self.current().token_type == token_type
    }

    fn check_ident(&self, name: &str) -> bool {
        self.current().token_type == TokenType::Ident && self.current().value == name
    }

    fn advance(&mut self) -> &Token {
        let pos = self.pos;
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        &self.tokens[pos]
    }

    fn advance_and_get_value(&mut self) -> String {
        let val = self.current().value.clone();
        self.advance();
        val
    }

    fn expect(&mut self, token_type: TokenType) -> Result<&Token, String> {
        if !self.check(token_type.clone()) {
            let tok = self.current();
            let hint = self.hint_for_expected(&token_type, tok);
            let mut msg = format!(
                "Expected {:?} at line {}:{} (got {:?} '{}')",
                token_type, tok.line, tok.col, tok.token_type, tok.value
            );
            if !hint.is_empty() {
                msg.push_str(&format!("\n  Hint: {}", hint));
            }
            return Err(msg);
        }
        Ok(self.advance())
    }

    fn hint_for_expected(&self, expected: &TokenType, got: &Token) -> String {
        match (expected, &got.token_type, got.value.as_str()) {
            (TokenType::Else, _, _) => {
                "if expressions MUST have an else branch. Use 'guard ... else' for early returns instead.".into()
            }
            (TokenType::RParen, TokenType::LAngle, _) => {
                "Use [] for generics, not <>. Example: List[String], Result[T, E]".into()
            }
            (TokenType::Then, _, _) => {
                "if requires 'then'. Write: if condition then expr else expr".into()
            }
            _ => String::new(),
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        let hint = match (&tok.token_type, tok.value.as_str()) {
            (TokenType::Underscore, _) => "\n  Hint: '_' can only be used in match patterns, not as a variable name.",
            (TokenType::Test, _) => "\n  Hint: 'test' is a reserved keyword.",
            _ => "",
        };
        Err(format!(
            "Expected identifier at line {}:{} (got {:?} '{}'){}",
            tok.line, tok.col, tok.token_type, tok.value, hint
        ))
    }

    fn expect_type_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected type name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn expect_any_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::TypeName) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn expect_any_fn_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        if self.check(TokenType::IdentQ) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected function name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn expect_any_param_name(&mut self) -> Result<String, String> {
        if self.check(TokenType::Ident) {
            return Ok(self.advance_and_get_value());
        }
        // Allow var keyword as param name
        if self.check(TokenType::Var) {
            return Ok(self.advance_and_get_value());
        }
        let tok = self.current();
        Err(format!(
            "Expected parameter name at line {}:{} (got {:?} '{}')",
            tok.line, tok.col, tok.token_type, tok.value
        ))
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenType::Newline) {
            self.advance();
        }
    }
}
