use crate::lexer::TokenType;
use super::{HintContext, HintResult, HintScope};

/// Detect missing comma between elements in lists, maps, and function arguments.
pub fn check(ctx: &HintContext) -> Option<HintResult> {
    let scope_name = match ctx.scope {
        HintScope::ListLiteral => "list elements",
        HintScope::MapLiteral => "map entries",
        HintScope::CallArgs => "function arguments",
        HintScope::FnParams => "function parameters",
        _ => return None,
    };

    if !is_expr_start(&ctx.got.token_type) {
        return None;
    }

    let example = match ctx.scope {
        HintScope::ListLiteral => "[a, b, c]",
        HintScope::MapLiteral => "[\"a\": 1, \"b\": 2]",
        HintScope::CallArgs => "f(a, b, c)",
        HintScope::FnParams => "fn f(a: Int, b: Int)",
        _ => return None,
    };

    Some(HintResult {
        message: Some(format!("Missing ',' between {}", scope_name)),
        hint: format!("Add a comma after the previous element. Example: {}", example),
    })
}

fn is_expr_start(tt: &TokenType) -> bool {
    matches!(tt,
        TokenType::Int | TokenType::Float | TokenType::String
        | TokenType::InterpolatedString | TokenType::True | TokenType::False
        | TokenType::Ident | TokenType::TypeName | TokenType::LParen
        | TokenType::LBracket | TokenType::LBrace | TokenType::Minus
        | TokenType::None | TokenType::Some | TokenType::Fn
        | TokenType::If | TokenType::Match | TokenType::Do
    )
}
