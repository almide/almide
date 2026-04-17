// almide-lang: re-export map for almide-syntax + almide-types.
// Downstream crates can depend on almide-lang to get both AST and type system,
// or depend on almide-syntax / almide-types individually.

pub use almide_syntax::ast;
pub use almide_syntax::lexer;
pub use almide_syntax::parser;
pub use almide_syntax::parse_cached;

pub use almide_types::types;
pub use almide_types::stdlib_info;

// Re-export almide-base for convenience
pub use almide_base;
pub use almide_base::intern;
pub use almide_base::diagnostic;
pub use almide_base::span;
