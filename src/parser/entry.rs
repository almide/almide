/// Parser entry points: parse() and parse_single_expr().

use crate::lexer::TokenType;
use crate::ast::Program;
use super::Parser;

impl Parser {
    pub fn parse_single_expr(&mut self) -> Result<crate::ast::Expr, String> {
        self.parse_expr()
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program {
            module: None,
            imports: Vec::new(),
            decls: Vec::new(),
            comment_map: Vec::new(),
        };

        let mut pending = self.skip_newlines_collect_comments();

        // Legacy module declaration
        if self.check(TokenType::Module) {
            program.comment_map.push(std::mem::take(&mut pending));
            let module_decl = self.parse_module_decl()?;
            program.decls.push(module_decl);
            pending = self.skip_newlines_collect_comments();
        }

        // Import declarations (with recovery)
        while self.check(TokenType::Import) {
            program.comment_map.push(std::mem::take(&mut pending));
            match self.parse_import_decl() {
                Ok(import) => program.imports.push(import),
                Err(msg) => {
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.skip_to_next_decl();
                }
            }
            pending = self.skip_newlines_collect_comments();
        }

        // Top-level declarations with error recovery
        while !self.check(TokenType::EOF) {
            let more = self.skip_newlines_collect_comments();
            pending.extend(more);
            if self.check(TokenType::EOF) { break; }
            program.comment_map.push(std::mem::take(&mut pending));

            match self.parse_top_decl() {
                Ok(decl) => program.decls.push(decl),
                Err(msg) => {
                    self.errors.push(self.string_to_diagnostic(&msg));
                    self.skip_to_next_decl();
                }
            }
            pending = self.skip_newlines_collect_comments();
        }

        if !pending.is_empty() {
            program.comment_map.push(pending);
        }

        if !self.errors.is_empty() && program.decls.is_empty() && program.imports.is_empty() && program.module.is_none() {
            let messages: Vec<String> = self.errors.iter().map(|d| d.display()).collect();
            return Err(messages.join("\n"));
        }

        Ok(program)
    }
}
