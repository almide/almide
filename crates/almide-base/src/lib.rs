pub mod alloc_pressure;
pub mod intern;
pub mod span;
pub mod diagnostic;
pub mod env;
pub mod names;
pub mod profile;
pub mod fs_errno;
pub mod fs_scope;
/// fs_scope.rs as text, for the native build that splices it into a
/// program's runtime: read here, inside its own package, so a vendored build
/// finds it (#3361).
pub const FS_SCOPE_SOURCE: &str = include_str!("fs_scope.rs");
pub mod native_stack_guard;
pub mod rust_lit;

// Re-export commonly used items at crate root
pub use intern::{Sym, sym, resolve};
pub use span::Span;
pub use diagnostic::Diagnostic;
