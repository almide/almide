pub mod check;
pub mod canonicalize;
pub mod lower;
pub mod import_table;
pub mod stdlib;
mod bundled_sigs;

/// TypeEnv — the mutable type-checking environment.
mod type_env;

/// Re-exports almide-lang types + local TypeEnv so `crate::types::*` works.
pub mod types;

// Re-export common items for convenience within the crate.
pub use almide_lang::ast;
pub use almide_base::intern;
pub use almide_base::diagnostic;
