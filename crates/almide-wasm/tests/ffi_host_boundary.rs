//! ADR-0031 amendment (A1, A4): Region buffers at the wasm host boundary,
//! zero-copy in both directions.
//!
//! - A1 / A4 push: an `@export(wasm)` fn returning a Region crosses as
//!   `(ptr: i32, len: i32)` through the multi-value return. The host holds
//!   one share of the block until it gives `ptr` back to `__region_release`,
//!   so the bytes stay put across later calls; the host writes straight into
//!   an `ffi.alloc` Region and the next export reads what it wrote.
//! - A1c (#4519): an export declared `returns: address` crosses as the
//!   address alone, one i32, with every other A1 rule kept.
//! - A1d: such an export holds one host share, a lease its next call ends,
//!   so a host that never releases runs in bounded memory.
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

@export(wasm, "inbox_tail")
effect fn inbox_tail() -> Region = ffi.slice(inbox, 1, 2)!

@export(wasm, "inbox_at", returns: address)
effect fn inbox_at(n: Float) -> Region = {
  inbox = ffi.alloc(int.from_saturating(n))!
  inbox
}

@export(wasm, "greeting_at", returns: address)
fn greeting_at() -> Region = ffi.region(bytes.from_string("hello"))

@export(wasm, "empty_at", returns: address)
fn empty_at() -> Region = ffi.region(bytes.new())

@export(wasm, "inbox_again", returns: address)
fn inbox_again() -> Region = inbox

@export(wasm, "tail_at", returns: address)
effect fn tail_at() -> Region = ffi.slice(inbox, 1, 2)!

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
                c.data().lock().expect("wasmtime").out.push_str(&t);
            })
            .expect("wasmtime");
        linker
            .func_wrap("almide", "eprintln", |mut c: Caller<'_, Arc<Mutex<Host>>>, p: i32, n: i32| {
                let t = text(&mut c, p, n);
                c.data().lock().expect("wasmtime").err.push_str(&t);
            })
            .expect("wasmtime");
        // A `panic` writes its line through the raw-stderr host op (73):
        // `fs_call(op, 0, 0, ptr, len)`. Every other op traps.
        linker
            .func_wrap("almide", "fs_call", |mut c: Caller<'_, Arc<Mutex<Host>>>, op: i32, _a: i32, _b: i32, p: i32, n: i32| -> wasmtime::Result<i64> {
                if op != 73 {
                    return Err(wasmtime::Error::msg(format!("fs_call op {op} is not served here")));
                }
                let t = text(&mut c, p, n);
                c.data().lock().expect("wasmtime").err.push_str(&t);
                Ok(0)
            })
            .expect("wasmtime");
        linker
            .func_wrap("almide", "exit", |_c: Caller<'_, Arc<Mutex<Host>>>, code: i32| -> wasmtime::Result<()> {
                Err(wasmtime::Error::msg(format!("exit {code}")))
            })
            .expect("wasmtime");
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
                c.data().lock().expect("wasmtime").fills.push((base, len));
                Ok(())
            })
            .expect("wasmtime");
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
            .expect("wasmtime");
        linker.define_unknown_imports_as_traps(&module).expect("wasmtime");
        let mut store = Store::new(&engine, host.clone());
        let inst = linker.instantiate(&mut store, &module).expect("instantiate");
        let main = inst.get_typed_func::<(), ()>(&mut store, "main").expect("main");
        main.call(&mut store, ()).expect("main runs");
        assert_eq!(host.lock().expect("wasmtime").out.trim(), "ready");
        Run { store, inst, host }
    }

    fn region0(&mut self, name: &str) -> wasmtime::Result<(i32, i32)> {
        let f = self.inst.get_typed_func::<(), (i32, i32)>(&mut self.store, name).expect(name);
        f.call(&mut self.store, ())
    }

    fn mem(&mut self, p: i32, n: i32) -> Vec<u8> {
        let m = self.inst.get_memory(&mut self.store, "memory").expect("wasmtime");
        m.data(&self.store)[p as usize..(p + n) as usize].to_vec()
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
    let ty = r.inst.get_func(&mut r.store, "inbox_open").expect("wasmtime").ty(&r.store);
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
    let open = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "inbox_open").expect("wasmtime");
    let (p, n) = open.call(&mut r.store, 4).expect("inbox_open");
    assert!(p != 0 && n == 4, "({p}, {n})");
    let m = r.inst.get_memory(&mut r.store, "memory").expect("wasmtime");
    m.data_mut(&mut r.store)[p as usize..(p + 4) as usize].copy_from_slice(&[1, 2, 3, 40]);
    // A scalar `effect fn` export answers its ok value itself (#4376).
    let sum = r.inst.get_typed_func::<(), i64>(&mut r.store, "inbox_sum").expect("wasmtime");
    assert_eq!(sum.call(&mut r.store, ()).expect("wasmtime"), 46);
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
    // The program's own copy (the `inbox` var) still holds the block.
    assert_eq!(sum.call(&mut r.store, ()).expect("wasmtime"), 46);
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
    let churn = r.inst.get_typed_func::<i64, i64>(&mut r.store, "churn").expect("wasmtime");
    churn.call(&mut r.store, 300).expect("wasmtime");
    assert_eq!(r.mem(p, n), b"hello".to_vec(), "the bytes moved under the host");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
}

/// An empty Region crosses as (0, 0) and its release is a no-op.
#[test]
fn an_empty_region_crosses_as_zero_zero() {
    let mut r = Run::new();
    assert_eq!(r.region0("empty").expect("wasmtime"), (0, 0));
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, 0).expect("wasmtime");
}

/// A1: a sub-region crosses as its own address inside its parent's block,
/// with no copy. The host's share keeps the parent alive even after the
/// program dropped it, `__region_release` gives exactly that share back, and
/// releasing an address twice (or one never handed out) traps.
#[test]
fn an_exported_slice_holds_its_parent_until_released() {
    let mut r = Run::new();
    let (p, n) = r.region0("tail_of").expect("tail_of");
    assert_eq!((n, r.mem(p, n)), (4, b"ello".to_vec()));
    // `tail_of`'s parent is a temporary: only the host's share holds it.
    assert_eq!(r.rc(p - 1), 1);
    let churn = r.inst.get_typed_func::<i64, i64>(&mut r.store, "churn").expect("wasmtime");
    churn.call(&mut r.store, 300).expect("wasmtime");
    assert_eq!(r.mem(p, n), b"ello".to_vec(), "the parent moved under the host");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
    assert!(release.call(&mut r.store, p).is_err(), "a second release of one address did not trap");
}

/// The share accounting on a parent the program keeps: the export of a slice
/// of the `inbox` var adds one share, and its release takes exactly that one
/// back; an address the module never handed out traps.
#[test]
fn releasing_an_exported_slice_gives_back_one_share() {
    let mut r = Run::new();
    let open = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "inbox_open").expect("wasmtime");
    let (p, _) = open.call(&mut r.store, 4).expect("wasmtime");
    let m = r.inst.get_memory(&mut r.store, "memory").expect("wasmtime");
    m.data_mut(&mut r.store)[p as usize..(p + 4) as usize].copy_from_slice(&[1, 2, 3, 40]);
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
    assert_eq!(r.rc(p), 1, "the inbox var's own share");
    let (q, n) = r.region0("inbox_tail").expect("inbox_tail");
    assert_eq!((q, n, r.mem(q, n)), (p + 1, 2, vec![2, 3]));
    assert_eq!(r.rc(p), 2, "the var's share and the host's");
    release.call(&mut r.store, q).expect("wasmtime");
    assert_eq!(r.rc(p), 1, "the release took back the host's share alone");
    assert!(release.call(&mut r.store, q).is_err(), "a released address was released again");
    assert!(release.call(&mut r.store, p + 2).is_err(), "an address never handed out was released");
}

/// An `effect fn` export's err has no slot in `(ptr, len)`: it traps with the
/// reason on stderr.
#[test]
fn an_err_traps_with_its_reason() {
    let mut r = Run::new();
    let open = r.inst.get_typed_func::<i64, (i32, i32)>(&mut r.store, "inbox_open").expect("wasmtime");
    assert!(open.call(&mut r.store, -1).is_err());
    assert!(r.host.lock().expect("wasmtime").err.contains("ffi.alloc: negative length -1"), "{}", r.host.lock().expect("wasmtime").err);
}

/// A4 pull: the import is handed the lent Region's base and the length it
/// holds, and fills it in place.
#[test]
fn an_import_fills_a_lent_alloc_region_in_place() {
    let mut r = Run::new();
    let pull = r.inst.get_typed_func::<i64, i64>(&mut r.store, "pull").expect("wasmtime");
    assert_eq!(pull.call(&mut r.store, 5).expect("wasmtime"), 35);
    let fills = r.host.lock().expect("wasmtime").fills.clone();
    assert_eq!(fills.len(), 1);
    assert!(fills[0].0 != 0 && fills[0].1 == 5, "{fills:?}");
}

/// A3 at the boundary: a slice handed to an import crosses as its own
/// (base, len) — the sub-range of the parent, with no copy.
#[test]
fn a_slice_crosses_an_import_as_its_sub_range() {
    let mut r = Run::new();
    let f = r.inst.get_typed_func::<(), i64>(&mut r.store, "sum_tail").expect("wasmtime");
    assert_eq!(f.call(&mut r.store, ()).expect("wasmtime"), 43);
}

/// A Region crosses an import in the target memory's address width, as an
/// export returns one: `(i32, i32)` on wasm32, the byte argument staying the
/// `Int` scalar.
#[test]
fn a_region_crosses_an_import_as_two_i32s_on_wasm32() {
    let ir = almide_spine::s5::lower_to_ir("ffi_host_boundary.almd", SRC).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).expect("module");
    let ty = |name: &str| {
        let f = module.imports().find(|i| i.module() == "host" && i.name() == name).expect("wasmtime").ty().func().cloned().expect("wasmtime");
        (f.params().map(|v| format!("{v}")).collect::<Vec<_>>(), f.results().map(|v| format!("{v}")).collect::<Vec<_>>())
    };
    assert_eq!(ty("fill"), (vec!["i32".into(), "i32".into(), "i64".into()], vec![]));
    assert_eq!(ty("sum"), (vec!["i32".into(), "i32".into()], vec!["i64".into()]));
}

/// #4519 (A1c): `returns: address` makes the export `(params…) -> i32`, the
/// view's data address alone; a `Float` parameter stays an f64, so a host
/// passes a plain number.
#[test]
fn an_address_only_region_export_returns_one_i32() {
    let mut r = Run::new();
    let ty = r.inst.get_func(&mut r.store, "inbox_at").expect("wasmtime").ty(&r.store);
    assert!(matches!(ty.params().collect::<Vec<_>>()[..], [ValType::F64]), "{ty:?}");
    assert!(matches!(ty.results().collect::<Vec<_>>()[..], [ValType::I32]), "{ty:?}");
    let ty = r.inst.get_func(&mut r.store, "greeting_at").expect("wasmtime").ty(&r.store);
    assert!(ty.params().len() == 0 && matches!(ty.results().collect::<Vec<_>>()[..], [ValType::I32]), "{ty:?}");
    // The pair form beside it is unchanged.
    let ty = r.inst.get_func(&mut r.store, "inbox_open").expect("wasmtime").ty(&r.store);
    assert!(matches!(ty.results().collect::<Vec<_>>()[..], [ValType::I32, ValType::I32]), "{ty:?}");
}

/// A1c push: the host writes at the address the export answered, for the
/// length it asked for, and the next export reads those bytes; the address
/// is the one the pair form would hand out, so the host's share and
/// `__region_release` work as for A1.
#[test]
fn the_host_writes_at_an_address_only_export_and_the_next_export_reads_it() {
    let mut r = Run::new();
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    let p = at.call(&mut r.store, 4.0).expect("inbox_at");
    assert!(p != 0, "{p}");
    let m = r.inst.get_memory(&mut r.store, "memory").expect("wasmtime");
    m.data_mut(&mut r.store)[p as usize..(p + 4) as usize].copy_from_slice(&[1, 2, 3, 40]);
    let sum = r.inst.get_typed_func::<(), i64>(&mut r.store, "inbox_sum").expect("wasmtime");
    assert_eq!(sum.call(&mut r.store, ()).expect("wasmtime"), 46);
    assert_eq!(r.rc(p), 2, "the inbox var's share and the host's");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
    assert_eq!(r.rc(p), 1, "the release took back the host's share alone");
    assert_eq!(sum.call(&mut r.store, ()).expect("wasmtime"), 46);
    assert!(release.call(&mut r.store, p).is_err(), "a second release of one address did not trap");
}

/// A1c ownership: a Region only the host's share holds survives later
/// allocation until released; a slice crosses as its own address inside its
/// parent; an empty Region crosses as 0, whose release is a no-op.
#[test]
fn an_address_only_export_keeps_every_other_a1_rule() {
    let mut r = Run::new();
    let call0 = |r: &mut Run, name: &str| {
        let f = r.inst.get_typed_func::<(), i32>(&mut r.store, name).expect(name);
        f.call(&mut r.store, ())
    };
    let p = call0(&mut r, "greeting_at").expect("greeting_at");
    assert_eq!(r.mem(p, 5), b"hello".to_vec());
    assert_eq!(r.rc(p), 1, "only the host's share holds the block");
    let churn = r.inst.get_typed_func::<i64, i64>(&mut r.store, "churn").expect("wasmtime");
    churn.call(&mut r.store, 300).expect("wasmtime");
    assert_eq!(r.mem(p, 5), b"hello".to_vec(), "the bytes moved under the host");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    release.call(&mut r.store, p).expect("wasmtime");
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    let base = at.call(&mut r.store, 4.0).expect("inbox_at");
    let q = call0(&mut r, "tail_at").expect("tail_at");
    assert_eq!(q, base + 1, "a slice's address is its parent's plus its offset");
    release.call(&mut r.store, q).expect("wasmtime");
    release.call(&mut r.store, base).expect("wasmtime");
    assert_eq!(call0(&mut r, "empty_at").expect("empty_at"), 0);
    release.call(&mut r.store, 0).expect("wasmtime");
}

/// A1c: an `effect fn` export's err traps with its reason on stderr, as the
/// pair form's does.
#[test]
fn an_address_only_export_err_traps_with_its_reason() {
    let mut r = Run::new();
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    assert!(at.call(&mut r.store, -1.0).is_err());
    assert!(r.host.lock().expect("wasmtime").err.contains("ffi.alloc: negative length -1"), "{}", r.host.lock().expect("wasmtime").err);
}

/// A1d: an address-only export holds ONE host share. Its next call ends the
/// previous lease, so the superseded address is the module's again and
/// releasing it traps; the current lease is released like any address.
#[test]
fn an_address_only_export_holds_one_lease_that_its_next_call_ends() {
    let mut r = Run::new();
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    let p1 = at.call(&mut r.store, 4.0).expect("inbox_at");
    assert_eq!(r.rc(p1), 2, "the inbox var's share and the lease");
    let p2 = at.call(&mut r.store, 4.0).expect("inbox_at");
    assert_ne!(p1, p2);
    assert_eq!(r.rc(p2), 2, "the new block: the var's share and the lease, nothing kept for p1");
    let release = r.inst.get_typed_func::<i32, ()>(&mut r.store, "__region_release").expect("wasmtime");
    assert!(release.call(&mut r.store, p1).is_err(), "a superseded address was still the host's");
}

/// A1d: the same address in the same block, handed out again, holds the
/// lease's one share — however many times, from one export.
#[test]
fn an_address_handed_out_again_holds_one_share() {
    let mut r = Run::new();
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    let p = at.call(&mut r.store, 4.0).expect("inbox_at");
    let again = r.inst.get_typed_func::<(), i32>(&mut r.store, "inbox_again").expect("wasmtime");
    for _ in 0..5 {
        assert_eq!(again.call(&mut r.store, ()).expect("inbox_again"), p);
    }
    assert_eq!(r.rc(p), 3, "the var's share and one lease per export, not one per call");
}

/// A1d: a host that never calls `__region_release` runs in bounded memory:
/// after a warm-up, thousands more 4 KB results do not grow the memory.
#[test]
fn an_unreleasing_host_runs_in_bounded_memory() {
    let mut r = Run::new();
    let at = r.inst.get_typed_func::<f64, i32>(&mut r.store, "inbox_at").expect("wasmtime");
    for _ in 0..200 {
        at.call(&mut r.store, 4096.0).expect("inbox_at");
    }
    let m = r.inst.get_memory(&mut r.store, "memory").expect("wasmtime");
    let settled = m.data_size(&r.store);
    for _ in 0..5000 {
        at.call(&mut r.store, 4096.0).expect("inbox_at");
    }
    assert_eq!(m.data_size(&r.store), settled, "an unreleased address kept its block");
}
