//! C-292: deep (incl. mutual) tail recursion must run in CONSTANT stack —
//! `return_call` on tail-position direct calls. The stage-8 CI caught the
//! regression this guards: the fixture passed on a laptop's generous stack
//! and overflowed on the runner. This referee pins a SMALL stack so depth
//! bugs cannot hide behind the host environment.

use std::sync::{Arc, Mutex};

fn run_small_stack(bytes: &[u8]) -> anyhow::Result<String> {
    wasmparser::validate(bytes)?;
    let mut cfg = wasmtime::Config::new();
    cfg.max_wasm_stack(64 * 1024); // deliberately tiny: depth must be O(1)
    let engine = wasmtime::Engine::new(&cfg)?;
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let out2 = out.clone();
    let mut store = wasmtime::Store::new(&engine, ());
    let mut linker = wasmtime::Linker::new(&engine);
    let mem_out = out2.clone();
    linker.func_wrap(
        "almide",
        "println",
        move |mut caller: wasmtime::Caller<'_, ()>, ptr: i32, len: i32| {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let mut buf = vec![0u8; len as usize];
            mem.read(&caller, ptr as usize, &mut buf).expect("in-bounds read");
            let mut o = mem_out.lock().expect("lock");
            o.push_str(&String::from_utf8_lossy(&buf));
            o.push('\n');
        },
    )?;
    linker.func_wrap("almide", "eprintln", |_: wasmtime::Caller<'_, ()>, _: i32, _: i32| {})?;
    fn exit_host(_: wasmtime::Caller<'_, ()>, code: i32) -> wasmtime::Result<()> {
        Err(wasmtime::Error::msg(format!("almide.exit({code}) in a tail-call fixture")))
    }
    linker.func_wrap("almide", "exit", exit_host)?;
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    main.call(&mut store, ())?;
    let s = out.lock().expect("lock").clone();
    Ok(s)
}

#[test]
fn deep_tail_recursion_is_constant_stack() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("root");
    let rel = "spec/wasm_cross/ref_gleam_tail_deep.almd";
    let text = std::fs::read_to_string(root.join(rel)).expect("fixture");
    let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let wasm_out = run_small_stack(&bytes).expect("must run in a tiny stack (return_call)");
    let interp = almide_spine::s5::run_file(rel, &text).expect("interp");
    assert_eq!(interp.exit, 0);
    assert_eq!(wasm_out, interp.stdout);
}
