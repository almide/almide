//! Test-harness shim: the REAL host lives in almide-wasm-run — one
//! implementation serves the product runner and every gate here, so
//! the host the gates verify IS the host that ships.

#[allow(unused_imports)]
pub use almide_wasm_run::{run_wasm, run_wasm_with, RunResult};
