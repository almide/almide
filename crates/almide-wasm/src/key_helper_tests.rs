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
    ((((h >> 32) ^ h).wrapping_mul(k)) >> 32) as u32
}

/// The index build over `keys` as the emitter does it (capacity the power of
/// two >= 2 * entries, at least 32; linear probing from `hash & mask`):
/// the mean and the longest walk of an insert.
fn probe_walks(keys: &[Vec<u8>]) -> (f64, u32) {
    let cap = (2 * keys.len()).next_power_of_two().max(32);
    let mut taken = vec![false; cap];
    let (mut total, mut longest) = (0u64, 0u32);
    for k in keys {
        let mut slot = reference_hash(k) as usize & (cap - 1);
        let mut steps = 0u32;
        while taken[slot] {
            slot = (slot + 1) & (cap - 1);
            steps += 1;
        }
        taken[slot] = true;
        total += u64::from(steps);
        longest = longest.max(steps);
    }
    (total as f64 / keys.len() as f64, longest)
}

/// Ordinary key families spread: no insert of 20 000 such keys walks near
/// the flood bound, and the mean walk is that of a uniform hash at load
/// 1/2 or under (about 0.25). The families are the ones a fold of the two
/// state halves alone sent to a few hundred starting slots (a counter in
/// the upper bytes of an eight-byte word: mean walk 60 to 100, longest 300
/// to 600, every one past the bound), next to ones it spread.
#[test]
fn ordinary_key_families_stay_far_from_the_flood_bound() {
    const N: usize = 20_000;
    let family = |f: &dyn Fn(usize) -> String| -> Vec<Vec<u8>> { (0..N).map(|i| f(i).into_bytes()).collect() };
    let families: [(&str, Vec<Vec<u8>>); 12] = [
        ("key_NNNN", family(&|i| format!("key_{i:04}"))),
        ("userNNNN", family(&|i| format!("user{i:04}"))),
        ("8 digits", family(&|i| format!("{i:08}"))),
        ("16 digits", family(&|i| format!("{i:016}"))),
        ("item-NNNNNN", family(&|i| format!("item-{i:06}"))),
        ("user:N:name", family(&|i| format!("user:{i}:name"))),
        ("16-byte prefix + 8 digits", family(&|i| format!("0123456789abcdef{i:08}"))),
        ("8 digits + 8-byte suffix", family(&|i| format!("{i:08}ABCDEFGH"))),
        ("digits in bytes 4..8 of 16", family(&|i| format!("xxxx{i:04}yyyyyyyy"))),
        ("kN", family(&|i| format!("k{i}"))),
        ("decimal", family(&|i| i.to_string())),
        ("path", family(&|i| format!("/a/b/file{i:05}.txt"))),
    ];
    for (name, keys) in &families {
        let (mean, longest) = probe_walks(keys);
        assert!(
            mean < 0.5 && longest < crate::map_ordered::FLOOD_PROBES as u32 / 4,
            "{name}: {} keys walk {mean:.2} slots on average, {longest} at most (the flood bound is {})",
            keys.len(),
            crate::map_ordered::FLOOD_PROBES
        );
    }
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
