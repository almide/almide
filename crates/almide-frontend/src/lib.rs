pub mod check;
pub mod canonicalize;
pub mod lower;
pub mod ir_link;
pub mod import_table;
pub mod dialect_check;
pub mod deprecation;
pub mod stdlib;
pub mod bundled_sigs;

/// The one implementation of integer-literal decoding.
mod literals;

/// TypeEnv — the mutable type-checking environment.
mod type_env;

/// Re-exports almide-lang types + local TypeEnv so `crate::types::*` works.
pub mod types;

// Re-export common items for convenience within the crate.
pub use almide_lang::ast;
pub use almide_base::intern;
pub use almide_base::diagnostic;
