//! Compare emitted byte helpers with independent Rust operations, including
//! tails placed at the final byte of linear memory (no padding to read).
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, FunctionSection, MemorySection, MemoryType, Module,
    TypeSection, ValType,
};

fn module(function: wasm_encoder::Function, arity: usize) -> Vec<u8> {
    let mut m = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .function(vec![ValType::I32; arity], [ValType::I32]);
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
        .export("probe", ExportKind::Func, 0);
    m.section(&exports);
    let mut code = CodeSection::new();
    code.function(&function);
    m.section(&code);
    m.finish()
}

/// The emitted hash, written independently over the payload bytes alone.
fn reference_hash(bytes: &[u8]) -> u32 {
    let k = crate::map_index::HASH_STR_MUL;
    let mut h = bytes.len() as u64;
    let mut rest = bytes;
    while rest.len() > 8 {
        h = (h ^ u64::from_le_bytes(rest[..8].try_into().expect("eight bytes"))).wrapping_mul(k);
        rest = &rest[8..];
    }
    if !rest.is_empty() {
        let mut w = [0u8; 8];
        w[..rest.len()].copy_from_slice(rest);
        h = (h ^ u64::from_le_bytes(w)).wrapping_mul(k);
    }
    ((h >> 32) ^ h) as u32
}

#[test]
fn emitted_hash_reads_only_the_payload_for_every_short_tail_and_page_edge() {
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let m = wasmtime::Module::new(&engine, module(crate::map_index::emit_hash_str(), 1)).expect("the module compiles");
    let inst = wasmtime::Instance::new(&mut store, &m, &[]).expect("the module instantiates");
    let memory = inst.get_memory(&mut store, "memory").expect("the module exports its memory");
    let hash = inst
        .get_typed_func::<i32, i32>(&mut store, "probe")
        .expect("the export has the expected signature");
    for n in (0..=256).chain([511, 512, 513, 4095, 4096, 4097]) {
        let bytes: Vec<u8> = (0..n).map(|i| ((i * 73 + n * 19) % 256) as u8).collect();
        // The payload ends at the last byte of memory: nothing to read past it.
        let p = 65536 - n - almide_layout::PAYLOAD as usize;
        // Two header fills: the rc and cap words must not reach the hash.
        for fill in [0x00u8, 0xA5] {
            memory.write(&mut store, p, &[fill; 12]).expect("the write is inside the memory");
            memory
                .write(&mut store, p + 4, &(n as u32).to_le_bytes())
                .expect("the write is inside the memory");
            memory
                .write(&mut store, p + almide_layout::PAYLOAD as usize, &bytes)
                .expect("the write is inside the memory");
            assert_eq!(
                hash.call(&mut store, p as i32).expect("the call does not trap") as u32,
                reference_hash(&bytes),
                "length {n}, header fill {fill:#x}"
            );
        }
    }
}

#[test]
fn emitted_string_equality_preserves_aliases_lengths_and_tail_bytes() {
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let m = wasmtime::Module::new(&engine, module(crate::runtime::emit_str_eq(), 2)).expect("the module compiles");
    let inst = wasmtime::Instance::new(&mut store, &m, &[]).expect("the module instantiates");
    let memory = inst.get_memory(&mut store, "memory").expect("the module exports its memory");
    let eq = inst
        .get_typed_func::<(i32, i32), i32>(&mut store, "probe")
        .expect("the export has the expected signature");
    for n in 0..=65 {
        let bytes: Vec<u8> = (0..n).map(|i| (i * 13) as u8).collect();
        let a = 64usize;
        let b = 65536 - n - almide_layout::PAYLOAD as usize;
        for p in [a, b] {
            memory
                .write(&mut store, p + 4, &(n as u32).to_le_bytes())
                .expect("the write is inside the memory");
            memory
                .write(&mut store, p + almide_layout::PAYLOAD as usize, &bytes)
                .expect("the write is inside the memory");
        }
        assert_eq!(eq.call(&mut store, (a as i32, a as i32)).expect("the call does not trap"), 1);
        assert_eq!(eq.call(&mut store, (a as i32, b as i32)).expect("the call does not trap"), 1);
        if n > 0 {
            memory
                .write(&mut store, 65535, &[bytes[n - 1] ^ 1])
                .expect("the write is inside the memory");
            assert_eq!(eq.call(&mut store, (a as i32, b as i32)).expect("the call does not trap"), 0);
            memory
                .write(&mut store, b + 4, &((n - 1) as u32).to_le_bytes())
                .expect("the write is inside the memory");
            assert_eq!(eq.call(&mut store, (a as i32, b as i32)).expect("the call does not trap"), 0);
        }
    }
}
