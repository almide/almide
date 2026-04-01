use crate::lexer::TokenType;
use super::{HintContext, HintResult, HintScope};

/// Detect common keyword typos from other languages at the top level.
pub fn check(ctx: &HintContext) -> Option<HintResult> {
    if ctx.scope != HintScope::TopLevel {
        return None;
    }

    let value = ctx.got.value.as_str();

    // Function keyword typos
    if matches!(value, "def" | "func" | "function" | "fun" | "proc") {
        return Some(HintResult {
            message: Some(format!("'{}' is not a keyword in Almide", value)),
            hint: "Use 'fn name(...) -> Type = expr' or 'effect fn name(...) -> Result[T, E] = expr'.".into(),
        });
    }

    // Type keyword typos
    if matches!(value, "class" | "struct" | "enum" | "data" | "sealed" | "union") {
        return Some(HintResult {
            message: Some(format!("'{}' is not a keyword in Almide", value)),
            hint: "Use 'type Name = { field: Type, ... }' for record types, or 'type Name = | Case1 | Case2' for variants.".into(),
        });
    }

    // Protocol keyword typos
    if matches!(value, "interface" | "trait" | "abstract") {
        return Some(HintResult {
            message: Some(format!("'{}' is not a keyword in Almide", value)),
            hint: "Use 'protocol Name { ... }' for protocols.".into(),
        });
    }

    // Variable declaration typos
    if matches!(value, "const" | "val") {
        return Some(HintResult {
            message: Some(format!("'{}' is not a keyword in Almide", value)),
            hint: "Use 'let NAME = value' for top-level constants, or 'let' inside functions for local bindings.".into(),
        });
    }

    // Loop at top level
    if matches!(value, "while" | "for" | "loop") {
        return Some(HintResult {
            message: Some(format!("'{}' cannot appear at the top level", value)),
            hint: "Almide has no top-level loops. Define a function with 'fn' or 'effect fn'.".into(),
        });
    }

    // Return
    if value == "return" {
        return Some(HintResult {
            message: Some("'return' is not needed in Almide".into()),
            hint: "Almide functions return the last expression — no 'return' keyword needed.".into(),
        });
    }

    // Import after declarations
    if value == "import" && ctx.got.token_type == TokenType::Import {
        return Some(HintResult {
            message: None,
            hint: "All imports must come before other declarations.".into(),
        });
    }

    None
}
