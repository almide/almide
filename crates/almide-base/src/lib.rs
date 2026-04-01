pub mod intern;
pub mod span;
pub mod diagnostic;

// Re-export commonly used items at crate root
pub use intern::{Sym, sym, resolve};
pub use span::Span;
pub use diagnostic::Diagnostic;
