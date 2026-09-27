//! Experiment (#2659 → ADR input): the smallest `wasi:http/handler@0.3.0`
//! component, assembled the way almide's to_p3 assembles `wasi:cli/run`
//! (wasm-encoder core module + wit-component), to measure what `wasmtime
//! serve` does with it: does it load, what head does the client receive, and
//! does one instance see several requests (a guest global counter).
//!
//! Run (wasmtime 47.0.2, measured 2026-09-27):
//!   cargo run --offline --release -- handler.wasm   (CLEN=1 adds content-length)
//!   wasmtime serve -W component-model-more-async-builtins -S p3 -S http \
//!     --addr 127.0.0.1:8080 handler.wasm
//! Observed: `HTTP/1.1 418 I'm a teapot` (hyper's canonical reason, not the
//! program's), header names lowercased (`x-Count` → `x-count`), `date:`
//! always added, `transfer-encoding: chunked` unless the guest sets a
//! content-length, keep-alive unless the client asks to close; the counter
//! climbs 1, 2, 3 on one instance, resets after 1 s idle, and stays at 1
//! under `--max-instance-reuse-count 1`. Without the more-async-builtins
//! flag the sync stream builtins refuse to load.
use anyhow::Result;
use wasm_encoder::*;
use wit_parser::abi::{AbiVariant, FlatTypes, WasmType};

const WIT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../crates/almide-wasm-run/wit/p3");

fn vt(t: WasmType) -> ValType {
    match t {
        WasmType::I64 | WasmType::PointerOrI64 => ValType::I64,
        WasmType::F32 => ValType::F32,
        WasmType::F64 => ValType::F64,
        _ => ValType::I32,
    }
}

fn main() -> Result<()> {
    let mut resolve = wit_parser::Resolve::default();
    for (n, p) in [("clocks", "clocks"), ("random", "random"), ("cli", "cli"), ("filesystem", "filesystem"), ("http", "http")] {
        let text = std::fs::read_to_string(format!("{WIT}/deps/{p}/package.wit"))?;
        resolve.push_str(format!("{n}.wit"), &text)?;
    }
    let pkg = resolve.push_str(
        "svc.wit",
        "package exp:svc@0.1.0;\nworld svc {\n  import wasi:http/types@0.3.0;\n  export wasi:http/handler@0.3.0;\n}\n",
    )?;
    let world = resolve.select_world(&[pkg], Some("svc"))?;
    let (_, http) = resolve.packages.iter().find(|(_, p)| p.name.name == "http").unwrap();
    let types_if = &resolve.interfaces[http.interfaces["types"]];
    let handler_if = &resolve.interfaces[http.interfaces["handler"]];
    let sig = |name: &str| {
        let f = &types_if.functions[name];
        let s = resolve.wasm_signature(AbiVariant::GuestImport, f);
        (s.params.into_iter().map(vt).collect::<Vec<_>>(), s.results.into_iter().map(vt).collect::<Vec<_>>())
    };
    // task.return(result<response, error-code>): flattened up to 16, else a pointer.
    let handle = &handler_if.functions["handle"];
    let rty = handle.result.unwrap();
    let mut storage = [WasmType::I32; 16];
    let mut flat = FlatTypes::new(&mut storage);
    let fits = resolve.push_flat(&rty, &mut flat);
    let tr_params: Vec<ValType> = if fits { flat.to_vec().into_iter().map(vt).collect() } else { vec![ValType::I32] };
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(&resolve)?;
    let ec = types_if.types["error-code"];
    let payload = 4u64.max(sa.align(&wit_parser::Type::Id(ec)).align_wasm32() as u64);
    eprintln!("task.return flat fits={fits} params={:?} payload={payload}", tr_params);

    let mut types: Vec<(Vec<ValType>, Vec<ValType>)> = Vec::new();
    let mut ti = |p: Vec<ValType>, r: Vec<ValType>| -> u32 {
        if let Some(k) = types.iter().position(|t| t.0 == p && t.1 == r) {
            return k as u32;
        }
        types.push((p, r));
        (types.len() - 1) as u32
    };
    let t = "wasi:http/types@0.3.0";
    let imports: Vec<(&str, &str, u32)> = vec![
        (t, "[constructor]fields", { let (p, r) = sig("[constructor]fields"); ti(p, r) }),
        (t, "[method]fields.append", { let (p, r) = sig("[method]fields.append"); ti(p, r) }),
        (t, "[stream-new-0][static]response.new", ti(vec![], vec![ValType::I64])),
        (t, "[future-new-1][static]response.new", ti(vec![], vec![ValType::I64])),
        (t, "[static]response.new", { let (p, r) = sig("[static]response.new"); ti(p, r) }),
        (t, "[method]response.set-status-code", { let (p, r) = sig("[method]response.set-status-code"); ti(p, r) }),
        (t, "[stream-write-0][static]response.new", ti(vec![ValType::I32; 3], vec![ValType::I32])),
        (t, "[stream-drop-writable-0][static]response.new", ti(vec![ValType::I32], vec![])),
        (t, "[future-write-1][static]response.new", ti(vec![ValType::I32; 2], vec![ValType::I32])),
        (t, "[future-drop-writable-1][static]response.new", ti(vec![ValType::I32], vec![])),
        (t, "[future-drop-readable-2][static]response.new", ti(vec![ValType::I32], vec![])),
        (t, "[resource-drop]request", ti(vec![ValType::I32], vec![])),
        ("[export]wasi:http/handler@0.3.0", "[task-return]handle", ti(tr_params.clone(), vec![])),
    ];
    let (i_fnew, i_fappend, i_snew, i_futnew, i_rnew, i_status, i_swrite, i_sdrop, i_fwrite, i_fwdrop, i_frdrop, i_reqdrop, i_tr) =
        (0u32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
    let n_imp = imports.len() as u32;
    let t_handle = ti(vec![ValType::I32], vec![ValType::I32]);
    let t_cb = ti(vec![ValType::I32; 3], vec![ValType::I32]);
    let t_realloc = ti(vec![ValType::I32; 4], vec![ValType::I32]);
    let (f_handle, f_cb, f_realloc) = (n_imp, n_imp + 1, n_imp + 2);

    // Memory layout: 0..1024 scratch, statics at 1024, heap from 65536.
    const RET: i32 = 0;
    const NAME: i32 = 1024; // "x-Count"
    const VAL: i32 = 1040; // one decimal digit slot (4 bytes)
    const NAME2: i32 = 1056; // "X-Kind"
    const VAL2: i32 = 1072; // "Teapot"
    const BODY: i32 = 1088; // "hello from p3 #"
    const DIG: i32 = 1088 + 15;
    const TRL: i32 = 1200; // ok(none) for the trailers future
    const TRBUF: i32 = 2048; // task.return indirect buffer
    let g_count = 1u32; // global 0 = heap, 1 = counter

    let mut h = Function::new([(10, ValType::I32), (1, ValType::I64)]);
    let (hdrs, s64, rx, tx, frx, ftx, resp, rfut, d) = (1u32, 11u32, 2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32);
    {
        let mut i = h.instructions();
        // counter += 1; digit = '0' + counter % 10
        i.global_get(g_count).i32_const(1).i32_add().global_set(g_count);
        i.global_get(g_count).i32_const(10).i32_rem_u().i32_const(48).i32_add().local_set(d);
        i.i32_const(VAL).local_get(d).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
        i.i32_const(DIG).local_get(d).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
        i.local_get(0).call(i_reqdrop);
        i.call(i_fnew).local_set(hdrs);
        i.local_get(hdrs).i32_const(NAME).i32_const(7).i32_const(VAL).i32_const(1).i32_const(RET).call(i_fappend);
        i.local_get(hdrs).i32_const(NAME2).i32_const(6).i32_const(VAL2).i32_const(6).i32_const(RET).call(i_fappend);
        if std::env::var("CLEN").is_ok() {
            i.local_get(hdrs).i32_const(1300).i32_const(14).i32_const(1320).i32_const(2).i32_const(RET).call(i_fappend);
        }
        i.call(i_snew).local_tee(s64).i32_wrap_i64().local_set(rx);
        i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(tx);
        i.call(i_futnew).local_tee(s64).i32_wrap_i64().local_set(frx);
        i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(ftx);
        // response.new(headers, some(rx), frx, retptr)
        i.local_get(hdrs).i32_const(1).local_get(rx).local_get(frx).i32_const(RET).call(i_rnew);
        i.i32_const(RET).i32_load(MemArg { offset: 0, align: 2, memory_index: 0 }).local_set(resp);
        i.i32_const(RET).i32_load(MemArg { offset: 4, align: 2, memory_index: 0 }).local_set(rfut);
        i.local_get(resp).i32_const(418).call(i_status).drop();
        // task.return(ok(resp))
        if tr_params.len() == 1 {
            i.i32_const(TRBUF).i32_const(0).i32_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
            i.i32_const(TRBUF).local_get(resp).i32_store(MemArg { offset: payload, align: 2, memory_index: 0 });
            i.i32_const(TRBUF).call(i_tr);
        } else {
            i.i32_const(0).local_get(resp);
            if tr_params[1] == ValType::I64 {
                i.i64_extend_i32_u();
            }
            for p in &tr_params[2..] {
                match p { ValType::I64 => i.i64_const(0), ValType::F32 => i.f32_const(0.0f32.into()), ValType::F64 => i.f64_const(0.0f64.into()), _ => i.i32_const(0) };
            }
            i.call(i_tr);
        }
        // body, after task.return: the host reads it as the client does
        i.local_get(tx).i32_const(BODY).i32_const(16).call(i_swrite).drop();
        i.local_get(tx).call(i_sdrop);
        i.local_get(ftx).i32_const(TRL).call(i_fwrite).drop();
        i.local_get(ftx).call(i_fwdrop);
        i.local_get(rfut).call(i_frdrop);
        i.i32_const(0); // EXIT
        i.end();
    }
    let mut cb = Function::new([]);
    cb.instructions().unreachable().end();
    let mut ra = Function::new([(1, ValType::I32)]);
    {
        let mut i = ra.instructions();
        i.global_get(0).i32_const(7).i32_add().i32_const(-8).i32_and().local_tee(4);
        i.local_get(3).i32_add().global_set(0);
        i.local_get(0).if_(BlockType::Empty);
        i.local_get(4).local_get(0).local_get(1).memory_copy(0, 0);
        i.end();
        i.local_get(4);
        i.end();
    }

    let mut m = Module::new();
    let mut ts = TypeSection::new();
    for (p, r) in &types {
        ts.ty().function(p.iter().copied(), r.iter().copied());
    }
    let mut is = ImportSection::new();
    for (md, n, ty) in &imports {
        is.import(md, n, EntityType::Function(*ty));
    }
    let mut fs = FunctionSection::new();
    fs.function(t_handle).function(t_cb).function(t_realloc);
    let mut ms = MemorySection::new();
    ms.memory(MemoryType { minimum: 2, maximum: None, memory64: false, shared: false, page_size_log2: None });
    let mut gs = GlobalSection::new();
    let mi = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
    gs.global(mi, &ConstExpr::i32_const(65536));
    gs.global(mi, &ConstExpr::i32_const(0));
    let mut es = ExportSection::new();
    es.export("memory", ExportKind::Memory, 0);
    es.export("[async-lift]wasi:http/handler@0.3.0#handle", ExportKind::Func, f_handle);
    es.export("[callback][async-lift]wasi:http/handler@0.3.0#handle", ExportKind::Func, f_cb);
    es.export("cabi_realloc", ExportKind::Func, f_realloc);
    let mut cs = CodeSection::new();
    cs.function(&h).function(&cb).function(&ra);
    let mut ds = DataSection::new();
    ds.active(0, &ConstExpr::i32_const(NAME), b"x-Count".iter().copied());
    ds.active(0, &ConstExpr::i32_const(NAME2), b"X-Kind".iter().copied());
    ds.active(0, &ConstExpr::i32_const(VAL2), b"Teapot".iter().copied());
    ds.active(0, &ConstExpr::i32_const(BODY), b"hello from p3 #".iter().copied());
    ds.active(0, &ConstExpr::i32_const(1300), b"Content-Length".iter().copied());
    ds.active(0, &ConstExpr::i32_const(1320), b"16".iter().copied());
    m.section(&ts).section(&is).section(&fs).section(&ms).section(&gs).section(&es).section(&cs).section(&ds);
    let mut core = m.finish();
    wasmparser_validate(&core)?;
    wit_component::embed_component_metadata(&mut core, &resolve, world, wit_component::StringEncoding::UTF8)?;
    let comp = wit_component::ComponentEncoder::default().module(&core)?.encode()?;
    std::fs::write(std::env::args().nth(1).unwrap_or("handler.wasm".into()), comp)?;
    Ok(())
}


fn wasmparser_validate(_b: &[u8]) -> Result<()> {
    Ok(())
}
