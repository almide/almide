//! The http call shim's helpers against what native runs: Rust's
//! `String::from_utf8_lossy`, the call core's
//! `http_stream_incomplete_utf8_tail`, i64 formatting — the helpers
//! instantiated alone on wasmtime and fed a corpus of valid, broken and
//! random UTF-8 — and the call texts against the call core's own answers.
use super::*;
use wasm_encoder::{ExportSection, Module, TypeSection};

const SRC: usize = 1024;
const DST: usize = 65536;

struct H {
    store: wasmtime::Store<()>,
    mem: wasmtime::Memory,
    lossy: wasmtime::TypedFunc<(i32, i32, i32), i32>,
    dec: wasmtime::TypedFunc<(i32, i64), i32>,
    tail: wasmtime::TypedFunc<(i32, i32), i32>,
}

/// `lossy`, `dec` and a wrapper around the inline tail check, alone.
fn harness() -> H {
    use ValType::{I32, I64};
    let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    let t_lossy = type_index(&mut types, &[I32, I32, I32], &[I32]);
    let t_dec = type_index(&mut types, &[I32, I64], &[I32]);
    let t_tail = type_index(&mut types, &[I32, I32], &[I32]);
    let mut m = Module::new();
    let mut ts = TypeSection::new();
    for (p, r) in &types {
        ts.ty().function(p.iter().copied(), r.iter().copied());
    }
    m.section(&ts);
    let mut funcs = FunctionSection::new();
    for t in [t_lossy, t_dec, t_tail] {
        funcs.function(t);
    }
    m.section(&funcs);
    let mut mems = wasm_encoder::MemorySection::new();
    mems.memory(wasm_encoder::MemoryType { minimum: 4, maximum: None, memory64: false, shared: false, page_size_log2: None });
    m.section(&mems);
    let mut ex = ExportSection::new();
    ex.export("memory", wasm_encoder::ExportKind::Memory, 0);
    for (n, f) in [("lossy", 0), ("dec", 1), ("tail", 2)] {
        ex.export(n, wasm_encoder::ExportKind::Func, f);
    }
    m.section(&ex);
    let mut code = CodeSection::new();
    code.function(&hc_fn_lossy());
    code.function(&hc_fn_dec());
    let mut tail = Function::new([(3, ValType::I32)]);
    {
        let mut i = tail.instructions();
        hc_utf8_tail(&mut i, (0, 1), (2, 3, 4));
        i.local_get(2);
        i.end();
    }
    code.function(&tail);
    m.section(&code);
    let bytes = m.finish();
    wasmparser::validate(&bytes).expect("the helpers validate");
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, &bytes).expect("compile");
    let mut store = wasmtime::Store::new(&engine, ());
    let inst = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    H {
        mem: inst.get_memory(&mut store, "memory").expect("memory"),
        lossy: inst.get_typed_func(&mut store, "lossy").expect("lossy"),
        dec: inst.get_typed_func(&mut store, "dec").expect("dec"),
        tail: inst.get_typed_func(&mut store, "tail").expect("tail"),
        store,
    }
}

impl H {
    fn lossy(&mut self, b: &[u8]) -> Vec<u8> {
        self.mem.write(&mut self.store, SRC, b).expect("write");
        let end = self.lossy.call(&mut self.store, (DST as i32, SRC as i32, b.len() as i32)).expect("lossy") as usize;
        self.mem.data(&self.store)[DST..end].to_vec()
    }

    fn tail(&mut self, b: &[u8]) -> usize {
        self.mem.write(&mut self.store, SRC, b).expect("write");
        self.tail.call(&mut self.store, (SRC as i32, b.len() as i32)).expect("tail") as usize
    }

    fn dec(&mut self, v: i64) -> String {
        let end = self.dec.call(&mut self.store, (DST as i32, v)).expect("dec") as usize;
        String::from_utf8(self.mem.data(&self.store)[DST..end].to_vec()).expect("ascii")
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Valid text, every malformation class from_utf8_lossy distinguishes,
/// and random bytes biased toward the lead and continuation ranges.
fn corpus() -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = [
        "", "a", "café", "日本語", "🎉x", "a\u{10FFFF}b", "\u{7FF}\u{800}\u{FFFF}\u{10000}",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    for bad in [
        &[0x80][..],
        &[0xBF, 0x41],
        &[0xC0, 0x80],
        &[0xC1, 0xBF],
        &[0xC2],
        &[0xC2, 0x41],
        &[0xE0, 0x80, 0x80],
        &[0xE0, 0xA0],
        &[0xE0, 0xA0, 0x41],
        &[0xED, 0xA0, 0x80],
        &[0xED, 0x9F, 0xBF],
        &[0xEF, 0xBF],
        &[0xF0, 0x80, 0x80, 0x80],
        &[0xF0, 0x90, 0x80],
        &[0xF0, 0x90, 0x80, 0x41],
        &[0xF4, 0x8F, 0xBF, 0xBF],
        &[0xF4, 0x90, 0x80, 0x80],
        &[0xF5, 0x80],
        &[0xFF, 0xFE],
        &[0x63, 0x61, 0x66, 0xC3],
    ] {
        out.push(bad.to_vec());
    }
    let mut g = Lcg(0x4308);
    let pick = [0x00u8, 0x41, 0x7F, 0x80, 0x9F, 0xA0, 0xBF, 0xC0, 0xC2, 0xDF, 0xE0, 0xED, 0xEF, 0xF0, 0xF4, 0xF5, 0xFF];
    for _ in 0..4000 {
        let n = (g.next() % 12) as usize;
        let v: Vec<u8> = (0..n)
            .map(|_| if g.next().is_multiple_of(3) { (g.next() & 0xFF) as u8 } else { pick[(g.next() as usize) % pick.len()] })
            .collect();
        out.push(v);
    }
    out
}

#[test]
fn bodies_decode_as_from_utf8_lossy_decodes_them() {
    let mut h = harness();
    for b in corpus() {
        assert_eq!(h.lossy(&b), String::from_utf8_lossy(&b).as_bytes(), "input {b:02x?}");
    }
}

#[test]
fn read_new_holds_back_what_the_call_core_holds_back() {
    let mut h = harness();
    for b in corpus() {
        assert_eq!(h.tail(&b), almide_rt_core::http_client_core::http_stream_incomplete_utf8_tail(&b), "input {b:02x?}");
    }
}

#[test]
fn numbers_render_as_rust_renders_them() {
    let mut h = harness();
    for v in [0, 1, -1, 9, 10, -10, 404, 1000, 4000, i64::MAX, i64::MIN, 7_654_321, -7_654_321] {
        assert_eq!(h.dec(v), v.to_string());
    }
}

/// A loopback listener that accepts (the backlog does) and never answers.
fn silent() -> (std::net::TcpListener, String) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://127.0.0.1:{}/", l.local_addr().expect("addr").port());
    (l, url)
}

/// The texts the shim lays out are the ones native's call core answers.
#[test]
fn the_texts_are_the_call_cores() {
    use almide_rt_core::http_client_core as core;
    let invalid = core::http_call_spawn("GET", "http://127.0.0.1:1/", "", Vec::new(), -1, -2).err().expect("refused");
    assert_eq!(invalid, format!("{}-1{}-2", HC_TEXT_INVALID[0], HC_TEXT_INVALID[1]));
    let (_keep, url) = silent();
    let cancelled = core::http_call_spawn("GET", &url, "", Vec::new(), 0, 0).expect("start");
    cancelled.cancel();
    assert_eq!(core::http_call_poll(&cancelled).expect("ended").err().as_deref(), Some(HC_TEXT_CANCELLED));
    let total = core::http_call_spawn("GET", &url, "", Vec::new(), 50, 0).expect("start");
    assert_eq!(core::http_call_wait(&total).err(), Some(format!("{}50{}", HC_TEXT_TOTAL[0], HC_TEXT_TOTAL[1])));
    let idle = core::http_call_spawn("GET", &url, "", Vec::new(), 0, 50).expect("start");
    assert_eq!(core::http_call_wait(&idle).err(), Some(format!("{}50{}", HC_TEXT_IDLE[0], HC_TEXT_IDLE[1])));
    // The embedded host's text for an id it never issued.
    let calls = std::sync::Mutex::new(crate::http_call_host::HttpCalls::default());
    let (packed, bytes) = crate::http_call_host::by_id(&calls, OP_HC_STATE, 7);
    assert_eq!(packed >> 32, 1);
    assert_eq!(String::from_utf8(bytes).expect("utf8"), format!("{HC_TEXT_UNKNOWN}7"));
}
