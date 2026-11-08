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

#[test]
fn emitted_hash_matches_fnv_for_every_short_tail_and_page_edge() {
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
        let p = 65536 - n - almide_layout::PAYLOAD as usize;
        memory
            .write(&mut store, p + 4, &(n as u32).to_le_bytes())
            .expect("the write is inside the memory");
        memory
            .write(&mut store, p + almide_layout::PAYLOAD as usize, &bytes)
            .expect("the write is inside the memory");
        let h = bytes.iter().fold(0x811c9dc5_u32, |h, &b| {
            (h ^ u32::from(b)).wrapping_mul(0x01000193)
        });
        assert_eq!(
            hash.call(&mut store, p as i32).expect("the call does not trap") as u32,
            h ^ (h >> 15),
            "length {n}"
        );
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
