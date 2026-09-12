//! #2106: preserve variant-case trivia without making it semantic type data.
use crate::ast::ExprComments;
use crate::lexer::{Token, TokenType};

pub(super) fn collect(tokens: &[Token]) -> Vec<ExprComments> {
    let mut cases: Vec<ExprComments> = Vec::new();
    let mut pending = Vec::new();
    let mut depth = 0usize;
    let mut expect_case = true;
    let mut last_line = 0;
    for token in tokens {
        match token.token_type {
            TokenType::Comment if depth == 0 => {
                match cases.last_mut() {
                    Some(case) if token.line == last_line => case.line_trailing.push(token.value.clone()),
                    _ => pending.push(token.value.clone()),
                }
            }
            TokenType::TypeName if depth == 0 && expect_case => {
                cases.push(ExprComments { leading: std::mem::take(&mut pending), ..Default::default() });
                expect_case = false;
            }
            TokenType::Pipe if depth == 0 => expect_case = true,
            TokenType::LParen | TokenType::LBrace | TokenType::LBracket => depth += 1,
            TokenType::RParen | TokenType::RBrace | TokenType::RBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
        if !matches!(token.token_type, TokenType::Comment | TokenType::Newline) { last_line = token.line; }
    }
    cases
}
