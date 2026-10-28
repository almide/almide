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
/// The v1 native renderer's `string.__err_text` shim: the escaper's own
/// source (this crate's err_text.rs, so the table stays one copy) and the
/// call the rendered program makes, `rt_string___err_text` — the v0
/// oracle's `almide_rt_string___err_text` expression.
pub const ERR_TEXT_NATIVE_SHIM: &str = concat!(
    include_str!("err_text.rs"),
    "\nfn rt_string___err_text(s: &str) -> AlmideString { AlmideString::from(almide_rt_errtext_escape(s)) }"
);
pub mod json_limits;
pub mod native_stack_guard;
pub mod rust_lit;

// Re-export commonly used items at crate root
pub use intern::{Sym, sym, resolve};
pub use span::Span;
pub use diagnostic::Diagnostic;
