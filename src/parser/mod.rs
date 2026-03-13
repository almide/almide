mod compounds;
mod declarations;
mod expressions;
mod helpers;
pub mod hints;
mod patterns;
mod primary;
mod statements;
mod types;

use crate::lexer::Token;
use crate::ast::*;
use crate::diagnostic::Diagnostic;

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
    pub errors: Vec<Diagnostic>,
    pub(crate) file: Option<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new(), file: None }
    }

    pub fn with_file(mut self, file: &str) -> Self {
        self.file = Some(file.to_string());
        self
    }

    /// Check all hint modules for a matching hint at the current position.
    pub(crate) fn check_hint(&self, expected: Option<crate::lexer::TokenType>, scope: hints::HintScope) -> Option<hints::HintResult> {
        let prev = if self.pos > 0 { Some(&self.tokens[self.pos - 1]) } else { None };
        let ctx = hints::HintContext {
            expected,
            got: self.current(),
            prev,
            scope,
        };
        hints::check_hint(&ctx)
    }

    /// Check hints and return an Err with the hint message if found, otherwise a generic error.
    pub(crate) fn check_hint_or_err(&self, expected: Option<crate::lexer::TokenType>, scope: hints::HintScope, default_msg: &str) -> String {
        if let Some(result) = self.check_hint(expected, scope) {
            let tok = self.current();
            let msg = result.message.as_deref().unwrap_or(default_msg);
            format!("{} at line {}:{}\n  Hint: {}", msg, tok.line, tok.col, result.hint)
        } else {
            default_msg.to_string()
        }
    }

    /// Create a Diagnostic error with file/line/col from the current token.
    pub(crate) fn diag_error(&self, message: impl Into<String>, hint: impl Into<String>, context: impl Into<String>) -> Diagnostic {
        let mut d = Diagnostic::error(message, hint, context);
        let tok = self.current();
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = Some(tok.line);
        d.col = Some(tok.col);
        d
    }

    /// Convert a legacy String error (from parse methods that still return Result<T, String>)
    /// into a Diagnostic, extracting line:col if present in the message.
    pub(crate) fn string_to_diagnostic(&self, msg: &str) -> Diagnostic {
        // Try to extract "at line N:M" from the message
        let (line, col) = if let Some(idx) = msg.find("at line ") {
            let rest = &msg[idx + 8..];
            let nums: Vec<&str> = rest.splitn(3, |c: char| !c.is_ascii_digit()).collect();
            let l = nums.first().and_then(|s| s.parse::<usize>().ok());
            let c = nums.get(1).and_then(|s| s.parse::<usize>().ok());
            (l, c)
        } else {
            (None, None)
        };
        // Split hint from message (our format uses "\n  Hint: ...")
        let (message, hint) = if let Some(idx) = msg.find("\n  Hint: ") {
            (msg[..idx].to_string(), msg[idx + 9..].to_string())
        } else {
            (msg.to_string(), String::new())
        };
        let mut d = Diagnostic::error(message, hint, "");
        if let Some(f) = &self.file {
            d.file = Some(f.clone());
        }
        d.line = line;
        d.col = col;
        d
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

        // Legacy module declaration (ignored, package identity comes from almide.toml)
        if self.check(TokenType::Module) {
            program.comment_map.push(std::mem::take(&mut pending));
            let module_decl = self.parse_module_decl()?;
            program.decls.push(module_decl); // kept for deprecation warning in checker
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
                    self.errors.push(self.string_to_diagnostic(&msg));
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
            let messages: Vec<String> = self.errors.iter().map(|d| d.display()).collect();
            return Err(messages.join("\n"));
        }

        Ok(program)
    }

    /// Skip tokens until we reach a token that could start a new statement within a block.
    /// Stops at: newline followed by statement-starting keyword, `}`, or EOF.
    pub(crate) fn skip_to_next_stmt(&mut self) {
        use crate::lexer::TokenType;
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF | TokenType::RBrace => break,
                TokenType::Newline => {
                    self.advance();
                    let next_tt = &self.current().token_type;
                    if matches!(next_tt,
                        TokenType::Let | TokenType::Var | TokenType::Guard
                        | TokenType::If | TokenType::Match | TokenType::For
                        | TokenType::While | TokenType::Do
                        | TokenType::Ident | TokenType::TypeName
                        | TokenType::RBrace | TokenType::EOF
                        // Also stop at declaration keywords (we may have exited a block)
                        | TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Type | TokenType::Test | TokenType::Pub
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }

    /// Skip tokens until we reach a token that could start a new top-level declaration.
    fn skip_to_next_decl(&mut self) {
        use crate::lexer::TokenType;
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::Fn | TokenType::Effect | TokenType::Async | TokenType::Pub | TokenType::Local | TokenType::Mod
                | TokenType::Type | TokenType::Trait | TokenType::Impl
                | TokenType::Test | TokenType::Strict | TokenType::At => {
                    // Check if this is at the start of a line (after newline)
                    // by looking if the previous token was a newline or we're at the very start
                    break;
                }
                TokenType::Newline => {
                    self.advance();
                    // After newline, check if next token starts a declaration
                    let next_tt = &self.current().token_type;
                    if matches!(next_tt,
                        TokenType::Fn | TokenType::Effect | TokenType::Async | TokenType::Pub | TokenType::Local | TokenType::Mod
                        | TokenType::Type | TokenType::Trait | TokenType::Impl
                        | TokenType::Test | TokenType::Strict | TokenType::At | TokenType::EOF
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }
}
