pub mod alloc_pressure;
pub mod intern;
pub mod span;
pub mod diagnostic;
pub mod env;
pub mod names;
pub mod profile;
pub mod fs_errno;
/// C-418's one escaper for user data in runtime error texts (#4126).
pub mod err_text;
pub mod native_stack_guard;
pub mod rust_lit;

// Re-export commonly used items at crate root
pub use intern::{Sym, sym, resolve};
pub use span::Span;
pub use diagnostic::Diagnostic;
