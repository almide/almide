//! almide-rt-core (#1715): the prelude-independent cores of the native
//! runtime, ONE source serving two consumers:
//!
//! - `runtime/rs/src/http.rs` `include!`s `http_client_core.rs` verbatim, so
//!   the splice template's client IS this text (the embed resolver inlines
//!   the include at generation time — crates/almide-codegen/buildscript);
//! - `almide-wasm-run` links this crate and calls the same functions for the
//!   embedded host's fs_call ops 43..=47.
//!
//! That retires the textual copy `almide-wasm-run/src/http_client.rs`
//! carried: the C-328 equality (error texts, timeout wording, the
//! close-without-close_notify tolerance, chunked framing) now holds by
//! SHARED CODE, with the embedded_cross net still pinning the observable.
//!
//! Discipline for this file tree: everything here must compile BOTH as a
//! crate module AND as flat splice text — no `crate::` paths, no `mod`
//! declarations inside the shared sources, `use` lines at top level only
//! (the splice assembler hoists and dedups them).
pub mod http_client_core;
