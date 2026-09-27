//! Emit WASM bytes for one `.almd` file via the STRUCTURAL leg — the renderer
//! `--target wasm` ships by default (#2753, #1696 step 5).
//! Usage: wasmgen-harness <input.almd> <output.wasm>
//!
//! Run natively and on wasm32-wasip1; the two outputs must match byte-for-byte
//! (host-architecture codegen determinism). See scripts/check-host-determinism.sh.
//!
//! The entry is the library route (`almide::wasm_route::render_wasm_routed`,
//! the one the CLI's `--target wasm` and the playground call) with
//! `force_structural`, so the incumbent renderer is never consulted: a
//! structural wall is an exit-3 WALL (a tracked skip — the fixture is not
//! host-nondeterministic, the default leg simply declines it), never an
//! incumbent module standing in for it. Both hosts hitting the SAME wall is
//! fine; only an emitted fixture is byte-compared.
//!
//! The compared bytes are the structural module (importing `almide.*`)
//! followed by its stock-WASI form (`almide_wasi::to_wasi`, what a stock
//! runtime and the playground's runner load) when the transform serves every
//! emitted host op — so a host-dependent transform diverges here too.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inp = args.get(1).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let outp = args.get(2).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let source = std::fs::read_to_string(inp).expect("read input");
    // No filesystem resolver on the browser twin: both harnesses hand the
    // route the bundled modules a single entry imports, pre-parsed (the
    // playground's ModuleSource::Provided form).
    let bundled = almide_mir::pipeline::bundled_self_modules(&source);
    let opts = almide::wasm_route::RouteOptions { force_structural: true, ..Default::default() };
    let modules = almide::wasm_route::ModuleSource::Provided(&bundled);
    match almide::wasm_route::render_wasm_routed("in.almd", &source, modules, opts) {
        Ok(routed) => {
            assert!(routed.structural(), "force_structural must never hand back an incumbent module");
            let mut bytes = routed.bytes.clone();
            if let Ok(wasi) = routed.stock_wasi() {
                bytes.extend_from_slice(&wasi);
            }
            std::fs::write(outp, &bytes).expect("write output");
        }
        Err(e) => {
            eprintln!("WALL: {e:?}");
            std::process::exit(3);
        }
    }
}
