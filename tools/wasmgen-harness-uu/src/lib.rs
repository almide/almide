//! Browser-ABI determinism harness. Mirrors the playground's compile path —
//! the STRUCTURAL leg through the library route
//! (`almide::wasm_route::render_wasm_routed` with `force_structural`, the SAME
//! entry and bytes the native sibling `tools/wasmgen-harness` drives, #2753) —
//! built to wasm32-unknown-unknown so the gate exercises the exact target the
//! browser playground runs the compiler on, catching
//! wasm32-unknown-unknown-specific failures (e.g. unconditional std::time,
//! unsupported there) and host-pointer-width codegen divergence that
//! wasm32-wasip1 can mask. A WALL surfaces as a structured `Err` (a JS throw
//! the gate script reports), never a fabricated module and never an
//! incumbent module standing in for it.
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compile_source_to_wasm(source: &str) -> Result<Vec<u8>, String> {
    let bundled = almide_mir::pipeline::bundled_self_modules(source);
    let opts = almide::wasm_route::RouteOptions { force_structural: true, ..Default::default() };
    let modules = almide::wasm_route::ModuleSource::Provided(&bundled);
    let routed = almide::wasm_route::render_wasm_routed("in.almd", source, modules, opts)
        .map_err(|e| format!("wall: {e:?}"))?;
    if !routed.structural() {
        return Err("force_structural handed back an incumbent module".to_string());
    }
    // The structural module followed by its stock-WASI form when the
    // transform serves every host op — byte-for-byte what the native harness
    // writes.
    let mut bytes = routed.bytes.clone();
    if let Ok(wasi) = routed.stock_wasi() {
        bytes.extend_from_slice(&wasi);
    }
    Ok(bytes)
}
