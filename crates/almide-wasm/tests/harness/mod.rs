//! Shared wasmtime harness for unit-6 gates: every module passes the
//! wasmparser wall before instantiation; `almide.println` appends a line to
//! the captured stdout, `almide.eprintln` to a captured stderr (the run
//! manifest's oracle hash covers stdout only — stderr is captured so a
//! future abort-parity slice can assert it, never silently dropped).

use std::sync::{Arc, Mutex};

struct Host {
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
}

fn append_line(
    caller: &mut wasmtime::Caller<'_, Host>,
    sink: fn(&Host) -> &Arc<Mutex<String>>,
    ptr: i32,
    len: i32,
) {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .expect("exported memory");
    let mut buf = vec![0u8; len as usize];
    mem.read(&caller, ptr as usize, &mut buf).expect("in-bounds read");
    let mut o = sink(caller.data()).lock().expect("test harness invariant");
    o.push_str(&String::from_utf8_lossy(&buf));
    o.push('\n');
}

pub fn run_wasm(bytes: &[u8]) -> anyhow::Result<String> {
    wasmparser::validate(bytes)?; // the wall: never instantiate an invalid module
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let err = Arc::new(Mutex::new(String::new()));
    let mut store = wasmtime::Store::new(&engine, Host { out: out.clone(), err: err.clone() });
    let mut linker = wasmtime::Linker::new(&engine);
    linker.func_wrap(
        "almide",
        "println",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.out, ptr, len);
        },
    )?;
    linker.func_wrap(
        "almide",
        "eprintln",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.err, ptr, len);
        },
    )?;
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    main.call(&mut store, ())?;
    let s = out.lock().expect("test harness invariant").clone();
    Ok(s)
}

