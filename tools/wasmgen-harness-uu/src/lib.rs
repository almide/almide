//! Browser-ABI determinism harness. Mirrors the playground's compile path —
//! #782: the v0 emitter is retired, so that path is the v1 trust-spine renderer
//! (`almide_mir::pipeline::try_render_wasm_source`, the SAME entry the native
//! sibling `tools/wasmgen-harness` drives) — built to wasm32-unknown-unknown so
//! the gate exercises the exact target the browser playground runs the compiler
//! on, catching wasm32-unknown-unknown-specific failures (e.g. unconditional
//! std::time, unsupported there) and host-pointer-width codegen divergence that
//! wasm32-wasip1 can mask. A WALL surfaces as a structured `Err` (a JS throw
//! the gate script reports), never a fabricated module.
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_source_to_wasm(source: &str) -> Result<Vec<u8>, String> {
    let wat_text = almide_mir::pipeline::try_render_wasm_source(source, &[], false)
        .map_err(|e| format!("wall: {e:?}"))?;
    wat::parse_str(&wat_text).map_err(|e| format!("wat: {e}"))
}
