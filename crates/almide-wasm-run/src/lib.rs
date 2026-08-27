//! The greenfield wasm runtime host (library surface): `run_wasm` /
//! `run_wasm_with` execute an emitted module against the `almide.*`
//! import set; `RunResult` carries the cross-target observables.

mod host;
pub mod wasi;
pub mod wasi_p2;

pub use host::{run_wasm, run_wasm_capped, run_wasm_real_stdin, run_wasm_with, RunResult};
