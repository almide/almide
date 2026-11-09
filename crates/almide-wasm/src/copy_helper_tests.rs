//! Hold the emitted `$copy` helper to Rust's `copy_within` (memmove) for
//! every length on either side of its 16-byte inline cutoff, every source
//! and destination alignment, an overlapping move toward lower addresses,
//! and blocks that end at the last byte of linear memory.
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, FunctionSection, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

const PAGE: usize = 65536;

fn module() -> Vec<u8> {
    let mut m = Module::new();
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32; 3], Vec::<ValType>::new());
    m.section(&types);
    let mut funcs = FunctionSection::new();
    funcs.function(0);
    m.section(&funcs);
    let mut mem = MemorySection::new();
    mem.memory(MemoryType {
        minimum: 1,
        maximum: Some(1),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    m.section(&mem);
    let mut exports = ExportSection::new();
    exports
        .export("memory", ExportKind::Memory, 0)
        .export("copy", ExportKind::Func, 0);
    m.section(&exports);
    let mut code = CodeSection::new();
    code.function(&crate::runtime::emit_copy());
    m.section(&code);
    m.finish()
}

struct Probe {
    store: wasmtime::Store<()>,
    memory: wasmtime::Memory,
    copy: wasmtime::TypedFunc<(i32, i32, i32), ()>,
}

impl Probe {
    fn new() -> Self {
        let engine = wasmtime::Engine::default();
        let mut store = wasmtime::Store::new(&engine, ());
        let m = wasmtime::Module::new(&engine, module()).expect("the module compiles");
        let inst = wasmtime::Instance::new(&mut store, &m, &[]).expect("the module instantiates");
        let memory = inst.get_memory(&mut store, "memory").expect("the module exports its memory");
        let copy = inst
            .get_typed_func::<(i32, i32, i32), ()>(&mut store, "copy")
            .expect("the export has the expected signature");
        Probe { store, memory, copy }
    }

    /// Fill `[lo, lo + span)` with a pattern no two cases share, run
    /// `$copy(dst, src, len)`, and compare the whole window with the same
    /// move done by `copy_within` — a byte written outside the destination
    /// fails the case as surely as a byte missing from it.
    fn check(&mut self, lo: usize, span: usize, dst: usize, src: usize, len: usize, seed: usize) {
        let mut expected: Vec<u8> = (0..span).map(|i| ((i * 151 + seed * 37 + 11) % 251) as u8).collect();
        self.memory.write(&mut self.store, lo, &expected).expect("the window is inside the memory");
        self.copy
            .call(&mut self.store, (dst as i32, src as i32, len as i32))
            .expect("the copy does not trap");
        expected.copy_within(src - lo..src - lo + len, dst - lo);
        let mut got = vec![0u8; span];
        self.memory.read(&self.store, lo, &mut got).expect("the window is inside the memory");
        assert_eq!(got, expected, "dst {dst} src {src} len {len}");
    }
}

#[test]
fn emitted_copy_matches_memmove_for_disjoint_blocks_at_every_alignment() {
    let mut p = Probe::new();
    let mut seed = 0;
    for len in 0..=40 {
        for s in 0..8 {
            for d in 0..8 {
                seed += 1;
                // destination below the source and above it
                p.check(1024, 2048, 1024 + 8 + d, 2048 + s, len, seed);
                p.check(1024, 2048, 2048 + d, 1024 + 8 + s, len, seed);
            }
        }
    }
}

#[test]
fn emitted_copy_is_exact_for_an_overlapping_move_toward_lower_addresses() {
    let mut p = Probe::new();
    let mut seed = 0;
    for len in 0..=40 {
        for gap in 0..=17 {
            for d in 0..8 {
                seed += 1;
                let dst = 4096 + d;
                p.check(4096, 128, dst, dst + gap, len, seed);
            }
        }
    }
}

#[test]
fn emitted_copy_reads_and_writes_no_byte_past_the_end_of_memory() {
    let mut p = Probe::new();
    for len in 0..=40 {
        let lo = PAGE - 256;
        // source ends at the last byte of memory
        p.check(lo, 256, lo + 8, PAGE - len, len, len);
        // destination ends at the last byte of memory
        p.check(lo, 256, PAGE - len, lo + 8, len, len + 100);
    }
}
