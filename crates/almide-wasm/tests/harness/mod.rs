//! Shared wasmtime harness for unit-6 gates: every module passes the
//! wasmparser wall before instantiation; `almide.println` appends a line to
//! the captured stdout, `almide.eprintln` to a captured stderr, and
//! `almide.exit` records the process exit code and unwinds — so ABORT parity
//! (exit code + stdout-before-abort, the C-153 family) is a first-class
//! observable of every run, never an opaque Err.

use std::sync::{Arc, Mutex};

/// One wasm run's cross-target observables: stdout, stderr, exit code.
/// A trap WITHOUT a recorded `almide.exit` code is a runtime abort
/// (unreachable / div-by-zero / OOB) — exit 1, the native abort contract.
/// Not every gate reads every field (the run manifest hashes stdout only).
#[allow(dead_code)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit: i32,
}

struct Host {
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
    exit: Arc<Mutex<Option<i32>>>,
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
    let mut buf = vec![0u8; len as u32 as usize];
    if let Err(e) = mem.read(&caller, ptr as u32 as usize, &mut buf) {
        panic!("in-bounds read: {e:?} ptr={ptr} len={len} memsize={}", mem.data_size(&caller));
    }
    let mut o = sink(caller.data()).lock().expect("test harness invariant");
    o.push_str(&String::from_utf8_lossy(&buf));
    o.push('\n');
}

pub fn run_wasm(bytes: &[u8]) -> anyhow::Result<RunResult> {
    wasmparser::validate(bytes)?; // the wall: never instantiate an invalid module
    // Epoch deadline: a fixture (or a MUTANT under the gate) that
    // diverges must FAIL the run, never hang the suite. 30s of real time
    // is orders beyond any fixture; the deadline maps to a plain trap.
    let mut cfg = wasmtime::Config::new();
    cfg.epoch_interruption(true);
    let engine = wasmtime::Engine::new(&cfg)?;
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let err = Arc::new(Mutex::new(String::new()));
    let exit = Arc::new(Mutex::new(None));
    let mut store = wasmtime::Store::new(
        &engine,
        Host { out: out.clone(), err: err.clone(), exit: exit.clone() },
    );
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
    // A fn item (not a closure): a return-type-annotated closure is not
    // higher-ranked over the Caller lifetime and fails IntoFunc.
    fn exit_host(caller: wasmtime::Caller<'_, Host>, code: i32) -> wasmtime::Result<()> {
        *caller.data().exit.lock().expect("test harness invariant") = Some(code);
        // Unwind: the emitter guarantees an `unreachable` follows the
        // call, so returning an error here is the ONLY way out — no
        // instruction after `process.exit` ever executes.
        Err(wasmtime::Error::msg("almide.exit"))
    }
    linker.func_wrap("almide", "exit", exit_host)?;
    store.set_epoch_deadline(1);
    let eng = engine.clone();
    let ticker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        eng.increment_epoch();
    });
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    let call = main.call(&mut store, ());
    let recorded = exit.lock().expect("test harness invariant").take();
    let exit_code = match (&call, recorded) {
        (Ok(()), None) => 0,
        (Err(_), Some(code)) => code,
        (Err(e), None) => {
            if std::env::var("ALMIDE_DBG_TRAP").is_ok() {
                eprintln!("TRAP: {e:?}");
            }
            1 // genuine trap = runtime abort
        }
        (Ok(()), Some(_)) => {
            anyhow::bail!("almide.exit recorded a code but the run returned normally")
        }
    };
    drop(ticker);
    Ok(RunResult {
        stdout: out.lock().expect("test harness invariant").clone(),
        stderr: err.lock().expect("test harness invariant").clone(),
        exit: exit_code,
    })
}
