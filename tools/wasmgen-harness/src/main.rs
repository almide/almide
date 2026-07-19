//! Emit WASM bytes for one `.almd` file via the v1 trust-spine renderer.
//! Usage: wasmgen-harness <input.almd> <output.wasm>
//!
//! Run natively and on wasm32-wasip1; the two outputs must match byte-for-byte
//! (host-architecture codegen determinism). See scripts/check-host-determinism.sh.
//! #782: the v0 emitter this harness used to drive is retired — the v1 renderer
//! (almide-mir) is the only wasm path, so IT is what must be host-deterministic.
//! A WALL exits 3 (a tracked skip — the fixture is not host-nondeterministic,
//! it is simply not yet renderable) so the gate NEVER pretends a walled fixture
//! agreed. Both hosts hitting the SAME wall is fine; only an emitted fixture is
//! byte-compared.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let inp = args.get(1).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let outp = args.get(2).expect("usage: wasmgen-harness <in.almd> <out.wasm>");
    let source = std::fs::read_to_string(inp).expect("read input");
    match almide_mir::pipeline::try_render_wasm_source(&source, &[], false) {
        Ok(wat_text) => {
            let bytes = wat::parse_str(&wat_text).expect("v1 WAT must parse");
            std::fs::write(outp, &bytes).expect("write output");
        }
        Err(e) => {
            eprintln!("WALL: {e:?}");
            std::process::exit(3);
        }
    }
}
