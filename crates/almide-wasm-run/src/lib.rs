//! The greenfield wasm runtime host (library surface): `run_wasm` /
//! `run_wasm_with` execute an emitted module against the `almide.*`
//! import set; `RunResult` carries the cross-target observables.

mod host;
mod http_call_host;
mod host_process;
mod host_fs_scope;
mod host_serve;
mod net_host;
mod net_unix_host;
#[cfg(test)]
#[path = "tests/http_host_test.rs"]
mod http_host_test;
pub(crate) mod component_alloc;
pub mod component_availability;
mod component_host;
/// ADR-0023 step 7: the embedded lane's runner for a WASI 0.3 component.
pub use component_host::run_component;
/// The private `almide:process/spawn` capability (#2589, ADR-0025): the
/// `[permissions] proc` bound and the canonical-ABI import.
pub use host_process::{link_spawn_import, set_allowlist as set_proc_allowlist};
/// `[permissions] fs` (#4022): the directories the embedded host may touch,
/// and the preopens of its p3 component host.
pub use host_fs_scope::{scope as fs_scope, set_scope as set_fs_scope};
/// The stock-WASI transform (`to_wasi`, `P1_SERVED_OPS`, …): its own crate
/// since #2554 (it never needed the engine); the path stays.
pub use almide_wasi as wasi;
pub mod wasi_p2;
pub mod wasi_p3;

pub use host::{
    run_wasm, run_wasm_capped, run_wasm_real_stdin, run_wasm_real_stdin_args, run_wasm_unbounded,
    run_wasm_unbounded_args,
    run_wasm_with,
    AllocCount, RunResult,
};
