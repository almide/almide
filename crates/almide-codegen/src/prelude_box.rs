//! `AlmideBox`: the native indirection of a field that closes a cycle of a
//! recursive type (#4251), with a drop, clone and `==` that queue their nested
//! work instead of recursing once per level (#4252, #4250), and the explicit-
//! stack walk the generated `AlmideRepr` of a recursive type runs on.
//!
//! The body is ordinary Rust emitted verbatim (with the prelude's visibility)
//! into every generated program. The workspace forbids `unsafe`, so it is
//! exercised through generated programs: `spec/wasm_cross/recursive_*`
//! fixtures run 200 000-deep values through it, natively and on wasm.

const BODY: &str = include_str!("prelude_box_body.rs");

pub(crate) fn box_prelude(vis: &str) -> String {
    BODY.replace("pub ", vis)
}
