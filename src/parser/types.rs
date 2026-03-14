use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_type_expr(&mut self) -> Result<TypeExpr, String> {
        self.enter_depth()?;
        let result = self.parse_type_expr_inner();
        self.exit_depth();
        result
    }

    fn parse_type_expr_inner(&mut self) -> Result<TypeExpr, String> {
        if self.check(TokenType::Newtype) {
            self.advance();
            let inner = self.parse_type_expr()?;
            return Ok(TypeExpr::Newtype { inner: Box::new(inner) });
        }
        if self.check(TokenType::Pipe) {
            return self.parse_variant_type();
        }
        if self.check(TokenType::LBrace) {
            return self.parse_record_type();
        }
        if self.check(TokenType::Fn) {
            return self.parse_fn_type();
        }
        if self.check(TokenType::LParen) {
            return self.parse_tuple_type();
        }

        let name = self.expect_type_name()?;
        if self.check(TokenType::LBracket) {
            let args = self.parse_type_args()?;
            if self.check(TokenType::Pipe) {
                return self.try_parse_inline_variant(name, Vec::new());
            }
            return Ok(TypeExpr::Generic { name, args });
        }
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
            if self.check(TokenType::Pipe) {
                return self.try_parse_inline_variant(name, fields);
            }
            return Ok(TypeExpr::Simple { name });
        }
        if self.check(TokenType::Pipe) {
            return self.try_parse_inline_variant(name, Vec::new());
        }
        Ok(TypeExpr::Simple { name })
    }

    fn parse_tuple_type(&mut self) -> Result<TypeExpr, String> {
        self.expect(TokenType::LParen)?;
        if self.check(TokenType::RParen) {
            self.advance();
            return Ok(TypeExpr::Simple { name: "Unit".to_string() });
        }
        let first = self.parse_type_expr()?;
        if self.check(TokenType::RParen) {
            // (Type) — parenthesized, not a tuple
            self.advance();
            return Ok(first);
        }
        // (Type, Type, ...) — tuple
        let mut elements = vec![first];
        while self.check(TokenType::Comma) {
            self.advance();
            elements.push(self.parse_type_expr()?);
        }
        self.expect(TokenType::RParen)?;
        Ok(TypeExpr::Tuple { elements })
    }

    fn parse_variant_type(&mut self) -> Result<TypeExpr, String> {
        let mut cases = Vec::new();
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

    fn try_parse_inline_variant(&mut self, first_name: String, first_args: Vec<TypeExpr>) -> Result<TypeExpr, String> {
        let mut cases = Vec::new();
        let mut all_simple = first_args.is_empty();
        if !first_args.is_empty() {
            cases.push(VariantCase::Tuple { name: first_name.clone(), fields: first_args });
        } else {
            cases.push(VariantCase::Unit { name: first_name.clone() });
        }
        let mut simple_names = vec![first_name];
        while self.check(TokenType::Pipe) {
            self.advance();
            self.skip_newlines();
            let case_name = self.expect_type_name()?;
            if self.check(TokenType::LParen) {
                all_simple = false;
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
                all_simple = false;
                self.advance();
                let fields = self.parse_field_type_list()?;
                self.expect(TokenType::RBrace)?;
                cases.push(VariantCase::Record { name: case_name, fields });
            } else {
                cases.push(VariantCase::Unit { name: case_name.clone() });
                simple_names.push(case_name);
            }
            self.skip_newlines();
        }
        // If all cases are simple names without payloads, produce Union instead of Variant
        if all_simple {
            let members = simple_names.into_iter()
                .map(|n| TypeExpr::Simple { name: n })
                .collect();
            Ok(TypeExpr::Union { members })
        } else {
            Ok(TypeExpr::Variant { cases })
        }
    }

    fn parse_record_type(&mut self) -> Result<TypeExpr, String> {
        self.expect(TokenType::LBrace)?;
        self.skip_newlines();
        let mut fields = Vec::new();
        let mut open = false;
        while !self.check(TokenType::RBrace) {
            self.skip_newlines();
            // Check for `..` to mark as open record type
            if self.check(TokenType::DotDot) {
                self.advance();
                open = true;
                self.skip_newlines();
                break;
            }
            let field_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            let field_type = self.parse_type_expr()?;
            let default = if self.check(TokenType::Eq) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            fields.push(FieldType { name: field_name, ty: field_type, default });
            self.skip_newlines();
            if self.check(TokenType::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }
        self.expect(TokenType::RBrace)?;
        if open {
            Ok(TypeExpr::OpenRecord { fields })
        } else {
            Ok(TypeExpr::Record { fields })
        }
    }

    pub(crate) fn parse_field_type_list(&mut self) -> Result<Vec<FieldType>, String> {
        let mut fields = Vec::new();
        while !self.check(TokenType::RBrace) {
            self.skip_newlines();
            let field_name = self.expect_ident()?;
            self.expect(TokenType::Colon)?;
            let field_type = self.parse_type_expr()?;
            let default = if self.check(TokenType::Eq) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            fields.push(FieldType { name: field_name, ty: field_type, default });
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

    pub(crate) fn parse_type_args(&mut self) -> Result<Vec<TypeExpr>, String> {
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

    pub(crate) fn try_parse_generic_params(&mut self) -> Result<Option<Vec<GenericParam>>, String> {
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
        let mut structural_bound = None;
        if self.check(TokenType::Colon) {
            self.advance();
            // Check for structural bound: `T: { name: String, .. }`
            if self.check(TokenType::LBrace) {
                structural_bound = Some(self.parse_record_type()?);
            } else {
                bounds.push(self.expect_type_name()?);
                while self.check(TokenType::Plus) {
                    self.advance();
                    bounds.push(self.expect_type_name()?);
                }
            }
        }
        Ok(GenericParam {
            name,
            bounds: if bounds.is_empty() { None } else { Some(bounds) },
            structural_bound,
        })
    }
}
