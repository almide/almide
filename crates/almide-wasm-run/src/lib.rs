//! The greenfield wasm runtime host (library surface): `run_wasm` /
//! `run_wasm_with` execute an emitted module against the `almide.*`
//! import set; `RunResult` carries the cross-target observables.

mod host;
mod http_call_host;
pub(crate) mod component_alloc;
pub mod component_availability;
/// The stock-WASI transform (`to_wasi`, `P1_SERVED_OPS`, …): its own crate
/// since #2554 (it never needed the engine); the path stays.
pub use almide_wasi as wasi;
pub mod wasi_p2;
pub mod wasi_p3;

pub use host::{
    run_wasm, run_wasm_capped, run_wasm_real_stdin, run_wasm_real_stdin_args, run_wasm_unbounded,
    run_wasm_with,
    AllocCount, RunResult,
};
