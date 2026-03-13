use crate::lexer::TokenType;
use super::{HintContext, HintResult, HintScope};

/// Guide users from other languages toward Almide idioms.
/// Covers: return, null, throw, catch, loop, print, let mut, etc.
pub fn check(ctx: &HintContext) -> Option<HintResult> {
    // Expression-level rejected keywords
    if ctx.scope == HintScope::Expression || ctx.scope == HintScope::Block {
        if ctx.got.token_type == TokenType::Ident {
            if let Some(r) = check_rejected_ident(ctx.got.value.as_str()) {
                return Some(r);
            }
        }
    }

    // `let mut` → `var`
    if ctx.expected == Some(TokenType::Ident) || ctx.expected.is_none() {
        if ctx.got.token_type == TokenType::Ident && ctx.got.value == "mut" {
            if let Some(prev) = ctx.prev {
                if prev.token_type == TokenType::Let {
                    return Some(HintResult {
                        message: Some("'let mut' is not valid in Almide".into()),
                        hint: "Use 'var' for mutable variables. Example: var x = 0".into(),
                    });
                }
            }
        }
    }

    None
}

fn check_rejected_ident(name: &str) -> Option<HintResult> {
    match name {
        "loop" => Some(HintResult {
            message: Some("'loop' is not valid in Almide".into()),
            hint: "Use 'while true { ... }' or 'do { guard COND else ok(()) ... }' for loops.".into(),
        }),
        "return" => Some(HintResult {
            message: Some("'return' is not needed in Almide".into()),
            hint: "The last expression in a block is the return value. Use 'guard ... else' for early returns.".into(),
        }),
        "print" => Some(HintResult {
            message: Some("'print' is not a function in Almide".into()),
            hint: "Use 'println(s)' instead of 'print'.".into(),
        }),
        "null" | "nil" => Some(HintResult {
            message: Some(format!("'{}' does not exist in Almide", name)),
            hint: "Almide has no null. Use Option[T] with 'some(v)' / 'none'.".into(),
        }),
        "throw" => Some(HintResult {
            message: Some("'throw' is not valid in Almide".into()),
            hint: "Almide has no exceptions. Use Result[T, E] with 'ok(v)' / 'err(e)'.".into(),
        }),
        "catch" | "except" => Some(HintResult {
            message: Some(format!("'{}' is not valid in Almide", name)),
            hint: "Almide has no try/catch. Use 'match' on Result values instead.".into(),
        }),
        "self" | "this" => Some(HintResult {
            message: Some(format!("'{}' is not valid in Almide", name)),
            hint: "Almide has no 'self' or 'this'. Pass the value as the first parameter. Example: fn greet(user: User) -> String".into(),
        }),
        "new" => Some(HintResult {
            message: Some("'new' is not needed in Almide".into()),
            hint: "Construct records directly: Type { field: value }. Example: User { name: \"Taro\", age: 25 }".into(),
        }),
        "void" => Some(HintResult {
            message: Some("'void' does not exist in Almide".into()),
            hint: "Use 'Unit' for functions that return nothing. Example: fn log(msg: String) -> Unit".into(),
        }),
        "undefined" => Some(HintResult {
            message: Some("'undefined' does not exist in Almide".into()),
            hint: "Almide has no undefined. Use Option[T] with 'some(v)' / 'none'.".into(),
        }),
        "switch" => Some(HintResult {
            message: Some("'switch' is not valid in Almide".into()),
            hint: "Use 'match' for pattern matching. Example: match value { pattern -> result }".into(),
        }),
        "elif" | "elsif" | "elseif" => Some(HintResult {
            message: Some(format!("'{}' is not valid in Almide", name)),
            hint: "Use nested 'if/then/else'. Example: if a then x else if b then y else z".into(),
        }),
        "extends" | "implements" | "inherit" | "inherits" => Some(HintResult {
            message: Some(format!("'{}' is not valid in Almide", name)),
            hint: "Almide uses structural typing. No inheritance or interfaces — use open records: { field: Type, .. }".into(),
        }),
        "lambda" => Some(HintResult {
            message: Some("'lambda' is not valid in Almide".into()),
            hint: "Use 'fn(x) => expr' for lambdas. Example: list.map(xs, fn(x) => x + 1)".into(),
        }),
        _ => None,
    }
}
