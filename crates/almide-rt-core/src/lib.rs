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
//!
//! C-418's escaper (almide-base's err_text.rs, #4126) is a bare name in the
//! cores that embed user data in an error text. In the generated crate one
//! runtime module (runtime/rs/src/errtext.rs) defines it once; here each such
//! core is an inline module that inlines the same file beside itself, so the
//! bare name resolves without a `crate::` path.
pub mod http_client_core;
pub mod http_server_core;
/// The subprocess family (#2589, ADR-0025): `runtime/rs/src/process.rs`
/// includes it, the embedded host serves `almide:process/spawn` with it.
pub mod process_core {
    include!("../../almide-base/src/err_text.rs");
    include!("process_core.rs");
}
/// The tcp family (#3620): `runtime/rs/src/net.rs` includes it, the embedded
/// host serves its net ops with it.
pub mod net_core {
    include!("../../almide-base/src/err_text.rs");
    include!("net_core.rs");
}
/// net's Unix-domain sockets and shared-memory files (#3620): the same two
/// consumers as `net_core`; the embedded host serves its interim ops
/// 104..=116 with it. Its syscall module (net_unix_sys.rs, included) is the
/// crate's one sanctioned `unsafe` site — see Cargo.toml's lints.
pub mod net_unix_core {
    include!("../../almide-base/src/err_text.rs");
    include!("net_unix_core.rs");
}
/// The rooted fs family by descriptor (#4084): `runtime/rs/src/fs.rs`
/// includes it, the embedded host serves its rooted ops 120..=125 with it.
/// Its syscall module (fs_beneath_sys.rs, included) is the crate's second
/// sanctioned `unsafe` site. It also includes fs_scoped_core.rs (and its
/// fs_scope.rs): every fs call under `[permissions] fs` made beneath the
/// granted root's descriptor, through the `almide_sfs` facade.
pub mod fs_beneath_core;
