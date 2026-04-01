// Re-export type definitions from almide-lang.
pub use almide_lang::types::*;

// TypeEnv stays in the main crate (depends on import_table which depends on stdlib).
mod env;
pub use env::TypeEnv;
