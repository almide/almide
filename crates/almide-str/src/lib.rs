//! almide-str: ADR-0013's shared string buffer for the native runtime.
//!
//! [`AlmideStr`] is one heap block `[rc, len, cap | bytes]`: the wasm leg's
//! layout, shared by count and edited through a unique view. Stage 2 of
//! docs/roadmap/active/adr-0013-string-sharing.md introduces it here first,
//! as a crate of its own: it is unit-tested and run under Miri before any
//! generated program uses it.
//!
//! The crate does not opt into the workspace's `unsafe_code = "forbid"`. The
//! block is raw allocation, and every `unsafe` block states its invariant on
//! a `SAFETY:` line, and the crate's own `[lints]` deny
//! `unsafe_op_in_unsafe_fn`. `almide_str.rs` is written to the same splice discipline
//! as almide-rt-core, so the runtime can `include!` it verbatim.

// ADR-0037's list block is spliced after the string block (it shares the
// marking window and the imports), so the crate builds them as one module too.
mod almide_str {
    include!("almide_str.rs");
    include!("almide_list.rs");
}

pub use almide_str::{almide_str_mark_reachable, AlmideStr, AlmideStrHeader, AlmideStrStatic, AlmideUniqueStr, AlmideIntText, AlmideIntoStrList, almide_str_list};
pub use almide_str::AlmideList;

/// The source text of `almide_str.rs`, which every generated native program
/// splices in as its Almide String.
pub const SOURCE: &str = include_str!("almide_str.rs");
