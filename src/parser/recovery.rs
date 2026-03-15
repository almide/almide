/// Error recovery: skip tokens to find sync points after syntax errors.

use crate::lexer::TokenType;
use super::Parser;

impl Parser {
    /// Unified sync-point recovery: skip tokens until a statement or declaration boundary.
    /// Used by block parsers to continue after a syntax error.
    /// `in_block`: true when inside `{ }`, stops at `}`. false for braceless/top-level contexts.
    pub(crate) fn recover_to_sync_point(&mut self, in_block: bool) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::RBrace if in_block => break,
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        // Statement-level sync points
                        TokenType::Let | TokenType::Var | TokenType::Guard
                        | TokenType::If | TokenType::Match | TokenType::For
                        | TokenType::While | TokenType::Do
                        | TokenType::Ident | TokenType::TypeName
                        | TokenType::RBrace | TokenType::EOF
                        // Declaration-level sync points
                        | TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Type | TokenType::Test | TokenType::Pub
                        | TokenType::Trait | TokenType::Impl
                        | TokenType::Local | TokenType::Mod
                        | TokenType::Strict | TokenType::At
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }

    pub(crate) fn skip_to_next_stmt(&mut self) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF | TokenType::RBrace => break,
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        TokenType::Let | TokenType::Var | TokenType::Guard
                        | TokenType::If | TokenType::Match | TokenType::For
                        | TokenType::While | TokenType::Do
                        | TokenType::Ident | TokenType::TypeName
                        | TokenType::RBrace | TokenType::EOF
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

    pub(crate) fn skip_to_next_decl(&mut self) {
        loop {
            let tt = &self.current().token_type;
            match tt {
                TokenType::EOF => break,
                TokenType::Fn | TokenType::Effect | TokenType::Async
                | TokenType::Pub | TokenType::Local | TokenType::Mod
                | TokenType::Type | TokenType::Trait | TokenType::Impl
                | TokenType::Test | TokenType::Strict | TokenType::At => {
                    break;
                }
                TokenType::Newline => {
                    self.advance();
                    if matches!(self.current().token_type,
                        TokenType::Fn | TokenType::Effect | TokenType::Async
                        | TokenType::Pub | TokenType::Local | TokenType::Mod
                        | TokenType::Type | TokenType::Trait | TokenType::Impl
                        | TokenType::Test | TokenType::Strict | TokenType::At
                        | TokenType::EOF
                    ) {
                        break;
                    }
                }
                _ => { self.advance(); }
            }
        }
    }
}
