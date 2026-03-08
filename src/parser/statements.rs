use crate::lexer::TokenType;
use crate::ast::*;
use super::Parser;

impl Parser {
    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, String> {
        if self.check(TokenType::Let) {
            return self.parse_let_stmt();
        }
        if self.check(TokenType::Var) {
            return self.parse_var_stmt();
        }
        if self.check(TokenType::Guard) {
            return self.parse_guard_stmt();
        }

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
}
