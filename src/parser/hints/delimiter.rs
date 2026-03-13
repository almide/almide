use crate::lexer::TokenType;
use super::{HintContext, HintResult};

/// Provide hints for missing closing delimiters.
pub fn check(ctx: &HintContext) -> Option<HintResult> {
    let expected = ctx.expected.as_ref()?;

    match (expected, &ctx.got.token_type, ctx.got.value.as_str()) {
        // Missing closing paren
        (TokenType::RParen, _, _) => Some(HintResult {
            message: None,
            hint: "Missing ')'. Check for an unclosed '(' earlier in this expression".into(),
        }),
        // Missing closing bracket
        (TokenType::RBracket, _, _) => Some(HintResult {
            message: None,
            hint: "Missing ']'. Check for an unclosed '[' earlier in this expression".into(),
        }),
        // Missing closing brace
        (TokenType::RBrace, _, _) => Some(HintResult {
            message: None,
            hint: "Missing '}'. Check for an unclosed '{' earlier in this block".into(),
        }),
        // Missing `=` before value
        (TokenType::Eq, TokenType::Ident, _) | (TokenType::Eq, TokenType::Int, _)
        | (TokenType::Eq, TokenType::String, _) | (TokenType::Eq, TokenType::LBrace, _) => {
            Some(HintResult {
                message: None,
                hint: "Missing '=' before value. Write: let x = value".into(),
            })
        }
        _ => None,
    }
}
