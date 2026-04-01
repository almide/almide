use crate::lexer::TokenType;
use super::{HintContext, HintResult};

/// Detect operator mistakes: wrong operators from other languages.
pub fn check(ctx: &HintContext) -> Option<HintResult> {
    let expected = match ctx.expected.as_ref() {
        Some(e) => e,
        None => return check_standalone(ctx),
    };

    match (expected, &ctx.got.token_type, ctx.got.value.as_str()) {
        // `if x = 5` → should be `==`
        (TokenType::Then, TokenType::Eq, _) => Some(HintResult {
            message: None,
            hint: "Did you mean '=='? Use '==' for comparison. Write: if x == 5 then ...".into(),
        }),
        // `then` missing
        (TokenType::Then, _, _) => Some(HintResult {
            message: None,
            hint: "if requires 'then'. Write: if condition then expr else expr".into(),
        }),
        // `if` without `else`
        (TokenType::Else, _, _) => Some(HintResult {
            message: None,
            hint: "if expressions MUST have an else branch. Use 'guard ... else' for early returns instead.".into(),
        }),
        // Arrow confusion: `fn f() = Int` instead of `fn f() -> Int`
        (TokenType::Arrow, TokenType::Eq, _) => Some(HintResult {
            message: None,
            hint: "Use '->' for return type, not '='. Write: fn name() -> Type = body".into(),
        }),
        // Generics: `<>` instead of `[]`
        (TokenType::RParen, TokenType::LAngle, _) => Some(HintResult {
            message: None,
            hint: "Use [] for generics, not <>. Example: List[String], Result[T, E]".into(),
        }),
        _ => None,
    }
}

/// Check for operator errors without a specific expected token (standalone context).
fn check_standalone(ctx: &HintContext) -> Option<HintResult> {
    match (&ctx.got.token_type, ctx.got.value.as_str()) {
        // `||` → `or`
        (TokenType::PipePipe, _) => Some(HintResult {
            message: Some("'||' is not valid in Almide".into()),
            hint: "Use 'or' for logical OR. Example: if a or b then ...".into(),
        }),
        // `&&` → `and`
        (TokenType::AmpAmp, _) => Some(HintResult {
            message: Some("'&&' is not valid in Almide".into()),
            hint: "Use 'and' for logical AND. Example: if a and b then ...".into(),
        }),
        // `!x` → `not x`
        (TokenType::Bang, _) => Some(HintResult {
            message: Some("'!' is not valid in Almide".into()),
            hint: "Use 'not x' for boolean negation, not '!x'.".into(),
        }),
        // `|x|` closure syntax — detected via lookahead
        (TokenType::Pipe, _) => {
            if let Some(next) = ctx.next {
                if matches!(next.token_type, TokenType::Ident | TokenType::Underscore) {
                    return Some(HintResult {
                        message: Some("'|x|' closure syntax is not valid in Almide".into()),
                        hint: "Use '(x) => expr' for lambdas. Example: list.map(xs, (x) => x + 1)".into(),
                    });
                }
            }
            None
        }
        // `;` — semicolons are not needed
        (TokenType::Semicolon, _) => Some(HintResult {
            message: Some("Semicolons are not used in Almide".into()),
            hint: "Remove the ';'. Almide uses newlines to separate statements.".into(),
        }),
        _ => None,
    }
}
