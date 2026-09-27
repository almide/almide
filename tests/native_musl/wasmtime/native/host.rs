// Compiles a module with cranelift, instantiates it and calls it, then builds
// a WASI context: the JIT, the runtime's signal handling and the WASI crate
// are all linked into the static musl binary and exercised once.
fn run() -> wasmtime::Result<i32> {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(
        &engine,
        r#"(module (func (export "answer") (result i32) i32.const 42))"#,
    )?;
    let wasi = wasmtime_wasi::WasiCtxBuilder::new().inherit_stdout().build();
    let mut store = wasmtime::Store::new(&engine, wasi);
    let instance = wasmtime::Instance::new(&mut store, &module, &[])?;
    let answer = instance.get_typed_func::<(), i32>(&mut store, "answer")?;
    answer.call(&mut store, ())
}

pub fn answer() -> String {
    match run() {
        Ok(n) => format!("wasm answer {n}"),
        Err(e) => format!("wasmtime failed: {e:#}"),
    }
}
