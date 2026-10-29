//! ADR-0031 amendment (A1, A4): Region buffers at the wasm host boundary,
//! zero-copy in both directions.
//!
//! - A1 / A4 push: an `@export(wasm)` fn returning a Region crosses as
//!   `(ptr: i32, len: i32)` through the multi-value return. The host holds
//!   one share of the block until it gives `ptr` back to `__region_release`,
//!   so the bytes stay put across later calls; the host writes straight into
//!   an `ffi.alloc` Region and the next export reads what it wrote.
//! - A4 pull: a writable `ffi.alloc` Region lent to an `@extern(wasm)`
//!   import arrives as the (base, len) pair, the length the Region holds, and
//!   the host fills it in place.
//!
//! The module is driven by wasmtime directly, with the five `almide.*`
//! imports served here, so the boundary the test sees is the module's own.

use std::sync::{Arc, Mutex};

use wasmtime::{Caller, Engine, Extern, Linker, Module, Store, Val, ValType};

const SRC: &str = r#"import ffi

var inbox: Region = ffi.region(bytes.new())

@export(wasm, "inbox_open")
effect fn inbox_open(n: Int) -> Region = {
  inbox = ffi.alloc(n)!
  inbox
}

@export(wasm, "inbox_sum")
effect fn inbox_sum() -> Int = {
  let b = ffi.read(inbox, 0, ffi.len(inbox))!
  bytes.to_list(b) |> list.fold(0, (a, x) => a + x)
}

@export(wasm, "greeting")
fn greeting() -> Region = ffi.region(bytes.from_string("hello"))

@export(wasm, "empty")
fn empty() -> Region = ffi.region(bytes.new())

@export(wasm, "tail_of")
effect fn tail_of() -> Region = ffi.slice(ffi.region(bytes.from_string("hello")), 1, 4)!

@export(wasm, "churn")
fn churn(n: Int) -> Int = list.range(0, n) |> list.map((i) => bytes.len(bytes.from_string("filler-${i}"))) |> list.fold(0, (a, x) => a + x)

@extern(wasm, "host", "fill")
fn host_fill(buf: Region, byte: Int) -> Unit

@export(wasm, "pull")
effect fn pull(n: Int) -> Int = {
  let r = ffi.alloc(n)!
  host_fill(r, 7)
  let b = ffi.read(r, 0, ffi.len(r))!
  bytes.to_list(b) |> list.fold(0, (a, x) => a + x)
}

@extern(wasm, "host", "sum")
fn host_sum(buf: Region) -> Int

@export(wasm, "sum_tail")
fn sum_tail() -> Int = host_sum(ffi.slice(ffi.region(bytes.from_list([1, 2, 3, 40])), 2, 2) ?? ffi.region(bytes.new()))

effect fn main() -> Unit = println("ready")
"#;

#[derive(Default)]
struct Host {
    out: String,
    err: String,
    /// (base, len) of each `host.fill` call.
    fills: Vec<(i64, i64)>,
}

fn text(caller: &mut Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32) -> String {
    let mem = caller.get_export("memory").and_then(Extern::into_memory).expect("memory");
    String::from_utf8_lossy(&mem.data(&*caller)[p as usize..(p + n) as usize]).into_owned()
}

fn as_i64(v: &Val) -> i64 {
    match v {
        Val::I32(x) => i64::from(*x),
        Val::I64(x) => *x,
        other => panic!("not an integer: {other:?}"),
    }
}

struct Run {
    store: Store<Arc<Mutex<Host>>>,
    inst: wasmtime::Instance,
    host: Arc<Mutex<Host>>,
}

impl Run {
    fn new() -> Self {
        let ir = almide_spine::s5::lower_to_ir("ffi_host_boundary.almd", SRC).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("emit");
        let engine = Engine::default();
        let module = Module::new(&engine, &bytes).expect("module");
        let host = Arc::new(Mutex::new(Host::default()));
        let mut linker: Linker<Arc<Mutex<Host>>> = Linker::new(&engine);
        linker
            .func_wrap("almide", "println", |mut c: Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32| {
                let t = text(&mut c, p, n);
                c.data().lock().unwrap().out.push_str(&t);
            })
            .unwrap();
        linker
            .func_wrap("almide", "eprintln", |mut c: Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32| {
                let t = text(&mut c, p, n);
                c.data().lock().unwrap().err.push_str(&t);
            })
            .unwrap();
        // A `panic` writes its line through the raw-stderr host op (73):
        // `fs_call(op, 0, 0, ptr, len)`. Every other op traps.
        linker
            .func_wrap("almide", "fs_call", |mut c: Caller<'_, Arc<Mutex<Host>>>, op: i32, _a: i32, _b: i32, p: i32, n: i32| -> wasmtime::Result<i64> {
                if op != 73 {
                    return Err(wasmtime::Error::msg(format!("fs_call op {op} is not served here")));
                }
                let t = text(&mut c, p, n);
                c.data().lock().unwrap().err.push_str(&t);
                Ok(0)
            })
            .unwrap();
        linker
            .func_wrap("almide", "exit", |_c: Caller<'_, Arc<Mutex<Host>>>, code: i32| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg(format!("exit {code}")))
            })
            .unwrap();
        // The pull import: fill the lent region with `byte`, using only the
        // (base, len) the module passed.
        let fill = module
            .imports()
            .find(|i| i.module() == "host" && i.name() == "fill")
            .and_then(|i| i.ty().func().cloned())
            .expect("the module imports host.fill");
        linker
            .func_new("host", "fill", fill, |mut c, args, _rets| {
                let (base, len, byte) = (as_i64(&args[0]), as_i64(&args[1]), as_i64(&args[2]));
                let mem = c.get_export("memory").and_then(Extern::into_memory).expect("memory");
                for b in &mut mem.data_mut(&mut c)[base as usize..(base + len) as usize] {
                    *b = byte as u8;
                }
                c.data().lock().unwrap().fills.push((base, len));
                Ok(())
            })
            .unwrap();
        // A lent read: the sum of the (base, len) bytes.
        let sum = module
            .imports()
            .find(|i| i.module() == "host" && i.name() == "sum")
            .and_then(|i| i.ty().func().cloned())
            .expect("the module imports host.sum");
        linker
            .func_new("host", "sum", sum, |mut c, args, rets| {
                let (base, len) = (as_i64(&args[0]), as_i64(&args[1]));
                let mem = c.get_export("memory").and_then(Extern::into_memory).expect("memory");
                let total: i64 = mem.data(&c)[base as usize..(base + len) as usize].iter().map(|b| i64::from(*b)).sum();
                rets[0] = Val::I64(total);
                Ok(())
            })
            .unwrap();
        linker.define_unknown_imports_as_traps(&module).unwrap();
        let mut store = Store::new(&engine, host.clone());
        let inst = linker.instantiate(&mut store, &module).expect("instantiate");
        let main = inst.get_typed_func::<(), ()>(&mut store, "main").expect("main");
        main.call(&mut store, ()).expect("main runs");
        assert_eq!(host.lock().unwrap().out.trim(), "ready");
        Run { store, inst, host }
    }

    fn region0(&mut self, name: &str) -> wasmtime::Result<(i32, i32)> {
        let f = self.inst.get_typed_func::<(), (i32, i32)>(&mut self.store, name).expect(name);
        f.call(&mut self.store, ())
    }

    fn mem(&mut self, p: i32, n: i32) -> Vec<u8> {
        let m = self.inst.get_memory(&mut self.store, "memory").unwrap();
        m.data(&self.store)[p as usize..(p + n) as usize].to_vec()
    }

    /// An `effect fn` export answers a Result block: the ok Int, or the test fails.
    fn ok_int(&mut self, block: i32) -> i64 {
        let tag = self.mem(block + (almide_layout::PAYLOAD + almide_layout::SUM_TAG) as i32, 4);
        assert_eq!(tag, [0, 0, 0, 0], "the export answered err");
        let v = self.mem(block + (almide_layout::PAYLOAD + almide_layout::SUM_FIELD) as i32, 8);
        i64::from_le_bytes(v.try_into().unwrap())
    }

    fn rc(&mut self, p: i32) -> u32 {
        let b = self.mem(p - almide_layout::PAYLOAD as i32, 4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}

/// The export's signature is `(params…) -> (i32, i32)`, and `__region_release`
/// is `(i32) -> ()`.
#[test]
fn a_region_export_returns_ptr_and_len_as_two_i32s() {
    let mut r = Run::new();
    let ty = r.inst.get_func(&mut r.store, "inbox_open").unwrap().ty(&r.store);
    assert!(matches!(ty.params().collect::<Vec<_>>()[..], [ValType::I64]), "{ty:?}");
    assert!(matches!(ty.results().collect::<Vec<_>>()[..], [ValType::I32, ValType::I32]), "{ty:?}");
    let rel = r.inst.get_func(&mut r.store, "__region_release").expect("the release export").ty(&r.store);
    assert!(matches!(rel.params().collect::<Vec<_>>()[..], [ValType::I32]) && rel.results().len() == 0, "{rel:?}");
}

/// A4 push: the host writes into the writable Region the export returned,
/// and the next export reads the bytes the host wrote — no Bytes copy at the
/// boundary.
#[test]
fn the_host_writes_into_an_exported_alloc_region_and_the_next_export_reads_it() {
    let mut r = Run::new();
    let open = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "inbox_open").unwrap();
    let (p, n) = open.call(&mut r.store, 4).expect("inbox_open");
    assert!(p != 0 && n == 4, "({p}, {n})");
    let m = r.inst.get_memory(&mut r.store, "memory").unwrap();
    m.data_mut(&mut r.store)[p as usize..(p + 4) as usize].copy_from_slice(&[1, 2, 3, 40]);
    let sum = r.inst.get_typed_func::<(), i32>(&mut r.store, "inbox_sum").unwrap();
    let b = sum.call(&mut r.store, ()).unwrap();
    assert_eq!(r.ok_int(b), 46);
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").unwrap();
    release.call(&mut r.store, p).unwrap();
    // The program's own copy (the `inbox` var) still holds the block.
    let b = sum.call(&mut r.store, ()).unwrap();
    assert_eq!(r.ok_int(b), 46);
}

/// A1 ownership: a Region the program dropped when the export returned
/// stays the host's — its block is held by exactly the host's share, and its
/// bytes survive later allocation — until the host releases it.
#[test]
fn an_exported_region_lives_until_the_host_releases_it() {
    let mut r = Run::new();
    let (p, n) = r.region0("greeting").expect("greeting");
    assert_eq!((n, r.mem(p, n)), (5, b"hello".to_vec()));
    assert_eq!(r.rc(p), 1, "only the host's share holds the block");
    let churn = r.inst.get_typed_func::<i64, i64>(&mut r.store, "churn").unwrap();
    churn.call(&mut r.store, 300).unwrap();
    assert_eq!(r.mem(p, n), b"hello".to_vec(), "the bytes moved under the host");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").unwrap();
    release.call(&mut r.store, p).unwrap();
}

/// An empty Region crosses as (0, 0) and its release is a no-op.
#[test]
fn an_empty_region_crosses_as_zero_zero() {
    let mut r = Run::new();
    assert_eq!(r.region0("empty").unwrap(), (0, 0));
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").unwrap();
    release.call(&mut r.store, 0).unwrap();
}

/// A sub-region has no block of its own to hand out, and an `effect fn`
/// export's err has no slot in `(ptr, len)`: both trap with the reason on
/// stderr, never an address the host could not give back.
#[test]
fn a_sub_region_or_an_err_traps_with_its_reason() {
    let mut r = Run::new();
    assert!(r.region0("tail_of").is_err(), "a sub-region crossed");
    assert!(r.host.lock().unwrap().err.contains("returned a sub-region of 4 bytes"), "{}", r.host.lock().unwrap().err);
    let open = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "inbox_open").unwrap();
    assert!(open.call(&mut r.store, -1).is_err());
    assert!(r.host.lock().unwrap().err.contains("ffi.alloc: negative length -1"), "{}", r.host.lock().unwrap().err);
}

/// A4 pull: the import is handed the lent Region's base and the length it
/// holds, and fills it in place.
#[test]
fn an_import_fills_a_lent_alloc_region_in_place() {
    let mut r = Run::new();
    let pull = r.inst.get_typed_func::<i64, i32>(&mut r.store, "pull").unwrap();
    let b = pull.call(&mut r.store, 5).unwrap();
    assert_eq!(r.ok_int(b), 35);
    let fills = r.host.lock().unwrap().fills.clone();
    assert_eq!(fills.len(), 1);
    assert!(fills[0].0 != 0 && fills[0].1 == 5, "{fills:?}");
}

/// A3 at the boundary: a slice handed to an import crosses as its own
/// (base, len) — the sub-range of the parent, with no copy.
#[test]
fn a_slice_crosses_an_import_as_its_sub_range() {
    let mut r = Run::new();
    let f = r.inst.get_typed_func::<(), i64>(&mut r.store, "sum_tail").unwrap();
    assert_eq!(f.call(&mut r.store, ()).unwrap(), 43);
}
