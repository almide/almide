mod compounds;
mod declarations;
mod expressions;
mod helpers;
mod patterns;
mod primary;
mod statements;
mod types;

use crate::lexer::Token;
use crate::ast::*;

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse_single_expr(&mut self) -> Result<Expr, String> {
        self.parse_expr()
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        use crate::lexer::TokenType;

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
}
