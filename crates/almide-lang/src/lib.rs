// almide-lang: re-export map for almide-syntax + almide-types.
// Downstream crates can depend on almide-lang to get both AST and type system,
// or depend on almide-syntax / almide-types individually.

pub use almide_syntax::ast;
pub use almide_syntax::lexer;
pub use almide_syntax::parser;
pub use almide_syntax::parse_cached;

pub use almide_types::types;
/// The language dialect epoch (`@dialect(N)`) — one constant and one
/// standing enum, cross-checked against `proofs/dialect-epochs.toml`.
pub use almide_types::dialect;
/// The comment-blanked embedded stdlib (#878) — see `almide_types::embedded`.
pub use almide_types::embedded;
pub use almide_types::stdlib_info;
/// The self-hosted stdlib runtime registry (call name -> impl fn + embedded
/// source) — one table read by the wasm renderer AND the interp oracle.
pub use almide_types::self_host_registry;
/// ADR-0001 time-unit surface (closed unit set, clocks, S4 clock column) —
/// single source for checker, lowering, and the matrix gates.
pub use almide_types::time_units;

// Re-export almide-base for convenience
pub use almide_base;
pub use almide_base::intern;
pub use almide_base::diagnostic;
pub use almide_base::span;
