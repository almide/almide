//! WASM runtime execution tests — inline regressions: verify that WASM output
//! matches Rust output for specific known bug classes (list layout, closures,
//! cross-module identity, effect-main termination, WASI preopen resolution).
//!
//! This is the inline half of the former `wasm_runtime_test` binary. The
//! data-driven gates that scan `spec/wasm_cross/*.almd` and
//! `spec/wasm_fail/*.almd` are their own binaries now
//! (`wasm_runtime_cross_target`, `wasm_runtime_opt_parity`,
//! `wasm_runtime_interp_oracle`, `wasm_runtime_interp_ledger`,
//! `wasm_runtime_fail_corpus`) so the CI shard packer
//! (scripts/ci-test-shard.sh) can spread them; the parts under
//! `wasm_runtime_test_parts/` are shared by `include!`.
//!
//! Requires: the `almide` binary (`ALMIDE_BIN`, else target/release/almide)
//! and wasmtime (Node.js WASI is the fallback) — a test self-skips without them.

include!("wasm_runtime_test_parts/common.rs");
include!("wasm_runtime_test_parts/p1.rs");
include!("wasm_runtime_test_parts/p2.rs");
include!("wasm_runtime_test_parts/p3.rs");
