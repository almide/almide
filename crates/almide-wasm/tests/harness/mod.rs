//! Shared wasmtime harness for unit-6 gates: every module passes the
//! wasmparser wall before instantiation; `almide.println` appends a line to
//! the captured output.

use std::sync::{Arc, Mutex};

struct Host {
    out: Arc<Mutex<String>>,
}

pub fn run_wasm(bytes: &[u8]) -> anyhow::Result<String> {
    wasmparser::validate(bytes)?; // the wall: never instantiate an invalid module
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let mut store = wasmtime::Store::new(&engine, Host { out: out.clone() });
    let mut linker = wasmtime::Linker::new(&engine);
    linker.func_wrap(
        "almide",
        "println",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let mut buf = vec![0u8; len as usize];
            mem.read(&caller, ptr as usize, &mut buf).expect("in-bounds read");
            let mut o = caller.data().out.lock().unwrap();
            o.push_str(&String::from_utf8_lossy(&buf));
            o.push('\n');
        },
    )?;
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    main.call(&mut store, ())?;
    let s = out.lock().unwrap().clone();
    Ok(s)
}

