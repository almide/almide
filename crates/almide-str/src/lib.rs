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

mod almide_str;

pub use almide_str::{AlmideStr, AlmideStrHeader, AlmideStrStatic, UniqueStr};
