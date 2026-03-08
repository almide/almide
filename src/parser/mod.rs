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
            comment_map: Vec::new(),
        };

        let mut pending = self.skip_newlines_collect_comments();

        // Module declaration (optional)
        if self.check(TokenType::Module) {
            program.comment_map.push(std::mem::take(&mut pending));
            program.module = Some(self.parse_module_decl()?);
            pending = self.skip_newlines_collect_comments();
        }

        // Import declarations
        while self.check(TokenType::Import) {
            program.comment_map.push(std::mem::take(&mut pending));
            program.imports.push(self.parse_import_decl()?);
            pending = self.skip_newlines_collect_comments();
        }

        // Top-level declarations
        while !self.check(TokenType::EOF) {
            let more = self.skip_newlines_collect_comments();
            pending.extend(more);
            if self.check(TokenType::EOF) {
                break;
            }
            program.comment_map.push(std::mem::take(&mut pending));
            program.decls.push(self.parse_top_decl()?);
            pending = self.skip_newlines_collect_comments();
        }

        // Trailing comments (after last decl)
        if !pending.is_empty() {
            program.comment_map.push(pending);
        }

        Ok(program)
    }
}
