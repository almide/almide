//! #4376 (C-451): an `@export(wasm)` `effect fn` whose declared return is a
//! scalar crosses as the value itself. The raw export's one result is the ok
//! `Int` (i64), `Float` (f64) or `Bool` (i32), and a `Unit` export has none,
//! where it used to be the address of a `Result` block (#3352) that only the
//! `--host js` glue unwrapped. An `err` writes its message to stderr and
//! traps, as a Region export's err does (C-446).
//!
//! The module is spec/wasm_host_js/effect_export_scalar.almd, driven by
//! wasmtime directly with the `almide.*` imports served here, so the
//! boundary the test sees is the module's own: what a Rust or C host gets.

use std::sync::{Arc, Mutex};

use wasmtime::{Caller, Engine, Extern, Linker, Module, Store, ValType};

const SRC: &str = include_str!("../../../spec/wasm_host_js/effect_export_scalar.almd");

#[derive(Default)]
struct Host {
    out: String,
    err: String,
}

fn text(caller: &mut Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32) -> String {
    let mem = caller.get_export("memory").and_then(Extern::into_memory).expect("memory");
    String::from_utf8_lossy(&mem.data(&*caller)[p as usize..(p + n) as usize]).into_owned()
}

fn module_bytes(src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("effect_export_scalar.almd", src).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}

struct Run {
    store: Store<Arc<Mutex<Host>>>,
    inst: wasmtime::Instance,
    host: Arc<Mutex<Host>>,
}

impl Run {
    fn new() -> Self {
        let engine = Engine::default();
        let module = Module::new(&engine, module_bytes(SRC)).expect("module");
        let host = Arc::new(Mutex::new(Host::default()));
        let mut linker: Linker<Arc<Mutex<Host>>> = Linker::new(&engine);
        linker
            .func_wrap("almide", "println", |mut c: Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32| {
                // The import is handed the line without its newline.
                let t = text(&mut c, p, n);
                let mut h = c.data().lock().expect("host");
                h.out.push_str(&t);
                h.out.push('\n');
            })
            .expect("wasmtime");
        linker
            .func_wrap("almide", "eprintln", |mut c: Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32| {
                let t = text(&mut c, p, n);
                c.data().lock().expect("host").err.push_str(&t);
            })
            .expect("wasmtime");
        // A `panic` writes its line through the raw-stderr host op (73).
        linker
            .func_wrap("almide", "fs_call", |mut c: Caller<'_, Arc<Mutex<Host>>>, op: i32, _a: i32, _b: i32, p: i32, n: i32| -> wasmtime::Result<i64> {
                if op != 73 {
                    return Err(wasmtime::Error::msg(format!("fs_call op {op} is not served here")));
                }
                let t = text(&mut c, p, n);
                c.data().lock().expect("host").err.push_str(&t);
                Ok(0)
            })
            .expect("wasmtime");
        linker
            .func_wrap("almide", "exit", |_c: Caller<'_, Arc<Mutex<Host>>>, code: i32| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg(format!("exit {code}")))
            })
            .expect("wasmtime");
        linker.define_unknown_imports_as_traps(&module).expect("wasmtime");
        let mut store = Store::new(&engine, host.clone());
        let inst = linker.instantiate(&mut store, &module).expect("instantiate");
        let main = inst.get_typed_func::<(), ()>(&mut store, "main").expect("main");
        main.call(&mut store, ()).expect("main runs");
        assert_eq!(host.lock().expect("host").out, "scaled 6\nhalf 2.5\neven true\nnote 7\n");
        Run { store, inst, host }
    }

    fn sig(&mut self, name: &str) -> (Vec<String>, Vec<String>) {
        let ty = self.inst.get_func(&mut self.store, name).expect(name).ty(&self.store);
        (ty.params().map(|v| format!("{v}")).collect(), ty.results().map(|v| format!("{v}")).collect())
    }

    fn memory_pages(&mut self) -> u64 {
        self.inst.get_memory(&mut self.store, "memory").expect("memory").size(&self.store)
    }
}

/// The raw export's results are the scalar's own wasm type: i64 for an
/// `Int`, f64 for a `Float`, i32 for a `Bool`, none for `Unit` — never the
/// i32 address of a Result block. A lent Region parameter still crosses as
/// two i32 (C-446).
#[test]
fn a_scalar_effect_export_returns_the_scalars_own_type() {
    let mut r = Run::new();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(r.sig("first"), (s(&[]), s(&["i64"])));
    assert_eq!(r.sig("scaled"), (s(&["i64"]), s(&["i64"])));
    assert_eq!(r.sig("half"), (s(&["i64"]), s(&["f64"])));
    assert_eq!(r.sig("even"), (s(&["i64"]), s(&["i32"])));
    assert_eq!(r.sig("note"), (s(&["i64"]), s(&[])));
    assert_eq!(r.sig("total"), (s(&["i32", "i32"]), s(&["i64"])));
    let ty = r.inst.get_func(&mut r.store, "put").expect("put").ty(&r.store);
    assert!(matches!(ty.results().collect::<Vec<_>>()[..], [ValType::I32, ValType::I32]), "{ty:?}");
}

/// The issue's repro: the host writes 42 at the address `put` handed out,
/// and the raw `first()` answers 42 (it answered a block address, 393776).
#[test]
fn the_host_reads_back_the_byte_it_wrote_as_the_raw_result() {
    let mut r = Run::new();
    let put = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "put").expect("put");
    let (p, n) = put.call(&mut r.store, 4).expect("put");
    assert_eq!(n, 4);
    let m = r.inst.get_memory(&mut r.store, "memory").expect("memory");
    m.data_mut(&mut r.store)[p as usize..(p + 4) as usize].copy_from_slice(&[42, 1, 2, 3]);
    let first = r.inst.get_typed_func::<(), i64>(&mut r.store, "first").expect("first: () -> i64");
    assert_eq!(first.call(&mut r.store, ()).expect("first"), 42);
    let total = r.inst.get_typed_func::<(i32, i32), i64>(&mut r.store, "total").expect("total");
    assert_eq!(total.call(&mut r.store, (p, n)).expect("total"), 48);
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("release");
    release.call(&mut r.store, p).expect("release");
}

/// Each scalar crosses as its value, and the Unit export runs for its effect.
#[test]
fn every_scalar_crosses_as_its_value() {
    let mut r = Run::new();
    let scaled = r.inst.get_typed_func::<i64, i64>(&mut r.store, "scaled").expect("scaled");
    assert_eq!(scaled.call(&mut r.store, 5).expect("scaled"), 15);
    let half = r.inst.get_typed_func::<i64, f64>(&mut r.store, "half").expect("half");
    assert_eq!(half.call(&mut r.store, 5).expect("half"), 2.5);
    let even = r.inst.get_typed_func::<i64, i32>(&mut r.store, "even").expect("even");
    assert_eq!((even.call(&mut r.store, 4).expect("even"), even.call(&mut r.store, 3).expect("even")), (1, 0));
    let note = r.inst.get_typed_func::<i64, ()>(&mut r.store, "note").expect("note");
    note.call(&mut r.store, 3).expect("note");
    assert!(r.host.lock().expect("host").out.ends_with("note 7\nnote 3\n"));
}

/// The export frees the Result block it unwrapped: many calls leave the
/// memory where a few left it.
#[test]
fn a_scalar_export_frees_its_result_block() {
    let mut r = Run::new();
    let scaled = r.inst.get_typed_func::<i64, i64>(&mut r.store, "scaled").expect("scaled");
    let half = r.inst.get_typed_func::<i64, f64>(&mut r.store, "half").expect("half");
    let churn = |r: &mut Run, n: i64| {
        for i in 0..n {
            scaled.call(&mut r.store, i % 100).expect("scaled");
            half.call(&mut r.store, 7).expect("half");
        }
    };
    churn(&mut r, 500);
    let settled = r.memory_pages();
    churn(&mut r, 20_000);
    assert_eq!(r.memory_pages(), settled, "memory grew: a scalar export leaks its Result block");
}

/// An `err` has no slot in a scalar result: the export writes the message
/// to stderr and traps, for every scalar type.
#[test]
fn an_err_traps_with_its_message() {
    for (name, arg) in [("scaled", 101), ("half", 102), ("even", 103), ("note", 104)] {
        let mut r = Run::new();
        let f = r.inst.get_func(&mut r.store, name).expect(name);
        let mut out = vec![wasmtime::Val::I32(0); f.ty(&r.store).results().len()];
        assert!(f.call(&mut r.store, &[wasmtime::Val::I64(arg)], &mut out).is_err(), "{name}({arg}) answered instead of trapping");
        let err = r.host.lock().expect("host").err.clone();
        assert!(err.contains(&format!("over 100: {arg}")), "{name}: stderr was {err:?}");
    }
}

/// Scope: a pure fn export, a declared `effect fn` returning a heap value or
/// a declared `Result`, and an effect fn exported only by being `pub` keep
/// the ABI they had.
#[test]
fn only_a_declared_scalar_effect_export_changes() {
    let src = "local fn check(n: Int) -> Result[Int, String] = if n > 1 then err(\"big\") else ok(n)\n\n\
               @export(wasm, \"plain\")\nfn plain(n: Int) -> Int = n + 1\n\n\
               @export(wasm, \"text\")\neffect fn text(n: Int) -> String = int.to_string(check(n)!)\n\n\
               @export(wasm, \"declared\")\nfn declared(n: Int) -> Result[Int, String] = check(n)\n\n\
               pub effect fn implicit(n: Int) -> Int = check(n)! + 1\n\n\
               effect fn main() -> Unit = println(\"m\")\n";
    let engine = Engine::default();
    let module = Module::new(&engine, module_bytes(src)).expect("module");
    let sig = |name: &str| {
        let f = module.exports().find(|e| e.name() == name).expect(name).ty().func().cloned().expect(name);
        f.results().map(|v| format!("{v}")).collect::<Vec<_>>()
    };
    assert_eq!(sig("plain"), vec!["i64"]);
    assert_eq!(sig("text"), vec!["i32"], "a String result is a block");
    assert_eq!(sig("declared"), vec!["i32"], "a declared Result is a block");
    assert_eq!(sig("implicit"), vec!["i32"], "an implicit pub export keeps the Result block");
}
