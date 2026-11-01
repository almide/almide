//! The header `cap` every `$alloc` path writes, checked by RUNNING the
//! shipped `$alloc` / `$free` bodies (not a Rust model of them) in a
//! minimal module: the two bodies at their fixed indices, stubs for the
//! two calls the OOM arm makes, the heap global and one memory.
//!
//! Every path writes the block's PHYSICAL payload capacity:
//! - bump, at or below the class ceiling: `(16 << class) - PAYLOAD`;
//! - free-list pop: the same value, re-derived from the class slot;
//! - bump above the ceiling: `want - PAYLOAD`, `want = round4(PAYLOAD + len)`;
//! - large-list take: `node size - PAYLOAD`, the remainder below SPLIT
//!   riding along.
//!
//! So `cap >= max(len, 4)` on every fresh block, and `$free`, which
//! re-derives the class from `cap`, files each block exactly where the next
//! request of its class looks, whichever path the block was born on.
//! Exhaustive over every request length up to the class ceiling.

use wasm_encoder::{
    CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType, MemorySection,
    MemoryType, Module, TypeSection, ValType,
};

const HEAP0: i32 = 4096;
const PAYLOAD: u32 = almide_layout::PAYLOAD;
/// The largest payload a classed block holds: `(16 << 12) - PAYLOAD`.
const CLASS_CEILING_LEN: u32 = (16 << (crate::FREELIST_CLASSES - 1)) - PAYLOAD;

fn module_bytes() -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], []); // 0: (i32) -> ()
    types.ty().function([ValType::I32], [ValType::I32]); // 1: (i32) -> i32
    types.ty().function([], []); // 2: () -> ()
    let n = crate::F_FREE.max(crate::F_ALLOC) + 1;
    let ty_of = |k: u32| match k {
        k if k == crate::F_ALLOC => 1,
        k if k == crate::F_FREE || k == crate::F_EXIT_IMPORT || k == crate::F_EPRINTLN_BLOCK => 0,
        _ => 2,
    };
    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    for k in 0..n {
        funcs.function(ty_of(k));
        let body = if k == crate::F_ALLOC {
            super::emit_alloc(0, None, false, false)
        } else if k == crate::F_FREE {
            super::emit_free(None, false)
        } else {
            let mut f = Function::new([]);
            if ty_of(k) == 0 {
                f.instructions().unreachable();
            }
            f.instructions().end();
            f
        };
        code.function(&body);
    }
    let mut mems = MemorySection::new();
    mems.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut globals = GlobalSection::new();
    for g in 0..=crate::G_HEAP {
        let v = if g == crate::G_HEAP { HEAP0 } else { 0 };
        globals.global(GlobalType { val_type: ValType::I32, mutable: true, shared: false }, &ConstExpr::i32_const(v));
    }
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("alloc", ExportKind::Func, crate::F_ALLOC);
    exports.export("free", ExportKind::Func, crate::F_FREE);
    exports.export("heap", ExportKind::Global, crate::G_HEAP);
    let mut m = Module::new();
    m.section(&types).section(&funcs).section(&mems).section(&globals).section(&exports).section(&code);
    m.finish()
}

struct Rt {
    store: wasmtime::Store<()>,
    alloc: wasmtime::TypedFunc<i32, i32>,
    free: wasmtime::TypedFunc<i32, ()>,
    mem: wasmtime::Memory,
    heap: wasmtime::Global,
}

impl Rt {
    fn new() -> Self {
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, module_bytes()).expect("the minimal module validates");
        let mut store = wasmtime::Store::new(&engine, ());
        let inst = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
        let alloc = inst.get_typed_func(&mut store, "alloc").expect("alloc");
        let free = inst.get_typed_func(&mut store, "free").expect("free");
        let mem = inst.get_memory(&mut store, "memory").expect("memory");
        let heap = inst.get_global(&mut store, "heap").expect("heap");
        Rt { store, alloc, free, mem, heap }
    }
    /// Empty every free list and put the frontier back at HEAP0.
    fn reset(&mut self) {
        let d = self.mem.data_mut(&mut self.store);
        d[crate::FREELIST_BASE as usize..crate::POOL_START as usize].fill(0);
        d[crate::runtime_large::LARGE_HEAD as usize..][..4].fill(0);
        self.heap.set(&mut self.store, wasmtime::Val::I32(HEAP0)).expect("set heap");
    }
    fn alloc(&mut self, len: u32) -> u32 {
        self.alloc.call(&mut self.store, len as i32).expect("alloc") as u32
    }
    fn free(&mut self, b: u32) {
        self.free.call(&mut self.store, b as i32).expect("free");
    }
    fn word(&self, a: u32) -> u32 {
        u32::from_le_bytes(self.mem.data(&self.store)[a as usize..][..4].try_into().unwrap())
    }
    fn heap(&mut self) -> u32 {
        self.heap.get(&mut self.store).unwrap_i32() as u32
    }
    /// The header of `b`, checked for rc 1 / len; returns cap.
    fn cap(&self, b: u32, len: u32) -> u32 {
        assert_eq!(self.word(b + almide_layout::RC.offset), 1, "rc of block {b} (len {len})");
        assert_eq!(self.word(b + almide_layout::LEN.offset), len, "len of block {b}");
        self.word(b + almide_layout::CAP.offset)
    }
}

/// `$alloc`'s request rounding and `$free`'s re-derivation from cap.
fn want(len: u32) -> u32 {
    ((PAYLOAD + len + 3) & !3).max(16)
}
fn class_of_total(total: u32) -> u32 {
    28 - (total - 1).leading_zeros()
}
fn free_class(cap: u32) -> u32 {
    class_of_total((cap + PAYLOAD + 3) & !3)
}
/// The smallest and largest request of class `c`.
fn class_range(c: u32) -> (u32, u32) {
    let hi = (16u32 << c) - PAYLOAD;
    (if c == 0 { 0 } else { (16u32 << (c - 1)) - PAYLOAD + 1 }, hi)
}

#[test]
fn every_path_writes_the_physical_class_cap_and_free_files_it_where_alloc_takes() {
    let mut rt = Rt::new();
    for len in 0..=CLASS_CEILING_LEN {
        let c = class_of_total(want(len));
        assert!(c < crate::FREELIST_CLASSES, "len {len} is classed");
        let phys = (16u32 << c) - PAYLOAD;
        rt.reset();
        // bump-born
        let b = rt.alloc(len);
        assert_eq!(b, HEAP0 as u32, "len {len}: lists empty, so the frontier");
        assert_eq!(rt.cap(b, len), phys, "len {len}: bump-born cap is the class's physical capacity");
        assert_eq!(rt.heap() - b, phys + PAYLOAD, "len {len}: the frontier advances by the whole class");
        assert!(phys >= len.max(4), "len {len}: cap >= max(len, 4)");
        assert_eq!(free_class(phys), c, "len {len}: free re-derives the class alloc took");
        // freelist-born, for the request itself and both ends of its class:
        // each pops the one block filed, with the same header cap.
        let (lo, hi) = class_range(c);
        for l in [len, lo, hi, len] {
            rt.free(b);
            assert_eq!(rt.alloc(l), b, "len {len}: a class-{c} request of {l} pops the filed block");
            assert_eq!(rt.cap(b, l), phys, "len {len}: freelist-born cap equals bump-born cap");
        }
    }
}

#[test]
fn above_the_ceiling_cap_is_the_exact_block_payload() {
    let mut rt = Rt::new();
    let c0 = CLASS_CEILING_LEN;
    for len in [c0 + 1, c0 + 2, c0 + 3, c0 + 4, c0 + 5, 100_000, 100_001, 1 << 20, (1 << 20) + 3] {
        assert!(class_of_total(want(len)) >= crate::FREELIST_CLASSES, "len {len} is above the ceiling");
        rt.reset();
        let b = rt.alloc(len);
        let total = want(len);
        assert_eq!(rt.cap(b, len), total - PAYLOAD, "len {len}: bump-born cap");
        assert_eq!(rt.heap() - b, total, "len {len}: the frontier advances by the exact want");
        assert!(total - PAYLOAD >= len.max(4), "len {len}: cap >= len");
        // large-list take of the same request: the exact node back
        rt.free(b);
        assert_eq!(rt.alloc(len), b, "len {len}: the large list hands the node back");
        assert_eq!(rt.cap(b, len), total - PAYLOAD, "len {len}: taken cap = node size - PAYLOAD");
        // a smaller huge request takes the node unsplit (remainder < SPLIT):
        // the slack rides in cap, which is still the node's physical payload
        let smaller = c0 + 1;
        rt.free(b);
        assert_eq!(rt.alloc(smaller), b, "len {len}: the node serves a smaller huge request");
        let cap = rt.cap(b, smaller);
        assert!(cap >= smaller, "len {len}: cap covers the smaller request");
        assert_eq!(cap, if total - want(smaller) >= 65536 { want(smaller) - PAYLOAD } else { total - PAYLOAD });
    }
}
