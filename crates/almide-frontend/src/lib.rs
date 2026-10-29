pub mod check;
/// E062 — the `env` shebang that runs on macOS and dies on Linux (#2159).
pub mod lint_shebang;
pub mod canonicalize;
pub mod lower;
pub mod ir_link;
pub mod import_table;
pub mod dialect_check;
pub mod deprecation;
/// Where a stdlib spelling removed at a dialect epoch went (read from the ledger).
pub mod removed_spellings;
pub mod attr_vocab;
/// Media-type ownership read off declarations (ADR-0032 D4, D7).
pub mod media_owners;
pub mod secret_type;
pub mod stdlib;
pub mod bundled_sigs;
/// Which auto-imported bundled modules a program reaches (#4071).
pub mod bundled_reach;
/// E008 generalized (ADR-0020 §3): no `var` reachable from a concurrent body.
pub mod concurrent_reach;
pub mod concurrent_reach_types;

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
