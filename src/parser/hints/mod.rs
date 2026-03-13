pub mod missing_comma;
pub mod keyword_typo;
pub mod delimiter;
pub mod operator;
pub mod syntax_guide;

use crate::lexer::{Token, TokenType};

/// The scope in which a hint is being requested.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HintScope {
    TopLevel,
    FnParams,
    CallArgs,
    ListLiteral,
    MapLiteral,
    Block,
    MatchArms,
    Pattern,
    Expression,
    LambdaParams,
    TraitBody,
    ImplBody,
}

/// Context passed to hint checkers when the parser encounters an unexpected token.
pub struct HintContext<'a> {
    /// The token type that was expected (if applicable).
    pub expected: Option<TokenType>,
    /// The actual token found.
    pub got: &'a Token,
    /// The previous token (if available).
    pub prev: Option<&'a Token>,
    /// The scope in which the error occurred.
    pub scope: HintScope,
}

/// A hint result that can override or augment the default error message.
pub struct HintResult {
    /// Override the main error message (None = keep default).
    pub message: Option<String>,
    /// Hint text shown below the error.
    pub hint: String,
}

/// Check all hint modules for a matching hint.
/// Returns the first match found, or None.
pub fn check_hint(ctx: &HintContext) -> Option<HintResult> {
    // Order matters: more specific checks first
    if let Some(r) = missing_comma::check(ctx) { return Some(r); }
    if let Some(r) = operator::check(ctx) { return Some(r); }
    if let Some(r) = keyword_typo::check(ctx) { return Some(r); }
    if let Some(r) = delimiter::check(ctx) { return Some(r); }
    if let Some(r) = syntax_guide::check(ctx) { return Some(r); }
    None
}
