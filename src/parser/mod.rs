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
    pub errors: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new() }
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

        // Top-level declarations with error recovery
        while !self.check(TokenType::EOF) {
            let more = self.skip_newlines_collect_comments();
            pending.extend(more);
            if self.check(TokenType::EOF) {
                break;
            }
            program.comment_map.push(std::mem::take(&mut pending));

            match self.parse_top_decl() {
                Ok(decl) => {
                    program.decls.push(decl);
                }
                Err(msg) => {
                    self.errors.push(msg);
                    // Skip to next declaration boundary
                    self.skip_to_next_decl();
                }
            }
            pending = self.skip_newlines_collect_comments();
        }

        // Trailing comments (after last decl)
        if !pending.is_empty() {
            program.comment_map.push(pending);
        }

        // If we collected errors but also parsed some declarations, return the partial program.
        // If no declarations were parsed and there are errors, return the first error.
        if !self.errors.is_empty() && program.decls.is_empty() && program.module.is_none() {
            return Err(self.errors.join("\n"));
        }

        Ok(program)
    }

    /// Skip tokens until we reach a token that could start a new top-level declaration.
    fn skip_to_next_decl(&mut self) {
        use crate::lexer::TokenType;
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::Fn | TokenType::Effect | TokenType::Async | TokenType::Pub | TokenType::Local
                | TokenType::Type | TokenType::Trait | TokenType::Impl
                | TokenType::Test | TokenType::Strict => {
                    // Check if this is at the start of a line (after newline)
                    // by looking if the previous token was a newline or we're at the very start
                    break;
                }
                TokenType::Newline => {
                    self.advance();
                    // After newline, check if next token starts a declaration
                    let next_tt = &self.current().token_type;
                    if matches!(next_tt,
                        TokenType::Fn | TokenType::Effect | TokenType::Async | TokenType::Pub | TokenType::Local
                        | TokenType::Type | TokenType::Trait | TokenType::Impl
                        | TokenType::Test | TokenType::Strict | TokenType::EOF
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }
}
