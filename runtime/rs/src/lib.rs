//! Almide Runtime Library — a SOURCE TEMPLATE, not a crate you build.
//!
//! Native implementations for the stdlib's `@intrinsic("almide_rt_*")`
//! declarations. Naming convention: `almide_rt_{module}_{function}`.
//!
//! `crates/almide-codegen/build.rs` reads these files as TEXT and embeds them
//! (`RUST_RUNTIME_MODULES`); the compiler concatenates the modules a program
//! needs into ONE flat module, prepends a prelude it synthesises at emit time
//! (`rust_runtime_prelude` in `crates/almide-codegen/src/lib.rs`), and hands
//! that to rustc — as the shared `almide_rt` rlib for `almide run`/`almide
//! test`, or as the runtime preamble of a generated project for `almide build`.
//!
//! The `pub mod` list below is an INVENTORY, not a working module tree: this
//! package does not compile as a crate and is deliberately outside the
//! workspace. `cargo check -p almide_rt` reports 220 errors — the prelude
//! items exist only inside the compiler, and the modules call each other with
//! no path because the emitter flattens them. `hash.rs`, `net.rs` and `zlib.rs`
//! are embedded but have never been listed here at all. See `../README.md` for
//! the measurement.
//!
//! **The behavioural oracle for this directory is the executable `spec/`
//! suite**, which runs on native and both wasm legs. A `#[cfg(test)]` block
//! written here runs NOWHERE: no cargo build compiles this directory, and
//! `strip_test_blocks` deletes the block from every emitted crate. Write the
//! test in `spec/stdlib/` (or `spec/wasm_cross/` for a cross-target promise);
//! `scripts/check-runtime-test-placement.sh` enforces that in CI (#2507).

pub mod int;
pub mod string;
pub mod list;
pub mod bytes;
pub mod mem;
pub mod matrix;
pub mod value;
pub mod datetime;
pub mod env;
pub mod error;
pub mod fan;
pub mod float;
pub mod fs;
pub mod http;
pub mod io;
pub mod json;
pub mod libm;
pub mod sse;
pub mod map;
pub mod math;
pub mod option;
pub mod process;
pub mod random;
pub mod regex;
pub mod result;
pub mod set;
pub mod testing;
pub mod hex;
pub mod base64;
