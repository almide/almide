//! `to_p2` (#1628 stage 1): rewrite an emitted almide module into a
//! WASI 0.2 COMPONENT directly — the canonical-ABI core imports plus a
//! wit-component encode of the vendored minimal WIT world, **no preview1
//! adapter** (the stage-0 wrap carries ~25 KB of adapter; this path
//! carries none, and the adapter structurally cannot host a guest
//! scheduler, so stage 2's fan lowering builds on THIS shape).
//!
//! Same doctrine as `to_wasi` (#1588), same five-op host surface:
//! console out, exit codes, stdin (cursor + read-to-end), entropy, the
//! wall clock. fs/env/process ops keep the DEFINED refusal. The
//! transform is a post-pass — the emitter's verified envelope is
//! untouched; the p1 build remains the plain `--target wasm` artifact.
//!
//! Canonical-ABI shapes used (WASI 0.2.3, the vendored WIT under
//! `crates/almide-wasm-run/wit/`):
//!   - `get-stdin/get-stdout/get-stderr: func() -> own<stream>` → core
//!     `() -> i32` (handles cached in globals, fetched once);
//!   - `[method]output-stream.blocking-write-and-flush(list<u8>)` → core
//!     `(self, ptr, len, retptr)` — the spec caps one write at 4096
//!     bytes, so the print shims CHUNK;
//!   - `[method]input-stream.blocking-read(u64)` → core
//!     `(self, len, retptr)`; the returned list lands via our exported
//!     `cabi_realloc`; EOF is `err(closed)`;
//!   - `wall-clock.now() -> datetime` → core `(retptr)`,
//!     record{u64 seconds, u32 nanos};
//!   - `get-random-bytes(u64) -> list<u8>` → core `(len, retptr)`;
//!   - `exit(result)` → core `(i32)`; the component run world only
//!     carries ok/err, which spells exactly the corpus's 0/1 codes.
//!
//! `cabi_realloc` bumps the module's own `__heap` frontier (raw bytes,
//! never a block: the bump region is never freed, so the allocator's
//! free-list discipline is undisturbed), growing memory on demand.

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use crate::wasi::{
    mem, mem8, parse_module, reencode_body, type_index, Parsed, Remap, DATA, MSG, PARK_SPAN,
    UNSUPPORTED_MSG,
};

// Import indices (8 imports replace the 5 almide.* ones; original
// non-import indices shift by +3).
const I_GET_STDIN: u32 = 0;
const I_GET_STDOUT: u32 = 1;
const I_GET_STDERR: u32 = 2;
const I_EXIT: u32 = 3;
const I_BLOCKING_READ: u32 = 4;
const I_WRITE_FLUSH: u32 = 5;
const I_CLOCK_NOW: u32 = 6;
const I_RANDOM: u32 = 7;
const IMPORTS: u32 = 8;
const SHIFT: u32 = IMPORTS - 5;

// Park offsets past the shared ones: retptr scratch (8-aligned).
const RET: u64 = 32;

/// 8-byte MemArg.
fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}

pub fn to_p2(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let parsed = parse_module(bytes)?;
    let Parsed {
        mut types,
        func_types,
        tables,
        old_mem_min,
        old_mem_max,
        parsed_globals,
        global_count,
        heap_global,
        exports: _,
        main_index,
        elements,
        mut data,
        bodies,
    } = parsed;
    let main_index = main_index.ok_or_else(|| anyhow::anyhow!("no main export"))?;
    let heap_global = heap_global.ok_or_else(|| anyhow::anyhow!("no __heap export"))?;
    let n_funcs = func_types.len() as u32;
    let shim_base = IMPORTS + n_funcs;
    // Shim order mirrors the almide.* import order (println, eprintln,
    // exit, fs_call, host_read), then cabi_realloc and the run wrapper.
    let f_realloc = shim_base + 5;
    let f_run = shim_base + 6;

    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    // Globals: originals, then plen, ppos, stdin/stdout/stderr handles.
    let (g_plen, g_ppos) = (global_count, global_count + 1);
    let (g_stdin, g_stdout, g_stderr) = (global_count + 2, global_count + 3, global_count + 4);
    let mut globals = GlobalSection::new();
    for (idx, (gt, i32v, i64v, f64v)) in parsed_globals.iter().enumerate() {
        let init = if idx as u32 == heap_global {
            ConstExpr::i32_const((heap_init + PARK_SPAN) as i32)
        } else if let Some(v) = i32v {
            ConstExpr::i32_const(*v)
        } else if let Some(v) = i64v {
            ConstExpr::i64_const(*v)
        } else {
            ConstExpr::f64_const(f64::from_bits(f64v.expect("global init")).into())
        };
        globals.global(*gt, &init);
    }
    let mutable_i32 = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // plen
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // ppos
    globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stdin
    globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stdout
    globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stderr

    // Canonical-ABI core types.
    let t_get = type_index(&mut types, &[], &[ValType::I32]);
    let t_exit = type_index(&mut types, &[ValType::I32], &[]);
    let t_read =
        type_index(&mut types, &[ValType::I32, ValType::I64, ValType::I32], &[]);
    let t_write = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        &[],
    );
    let t_now = type_index(&mut types, &[ValType::I32], &[]);
    let t_random = type_index(&mut types, &[ValType::I64, ValType::I32], &[]);
    // Shim types (the almide.* signatures) + realloc + run.
    let t_print = type_index(&mut types, &[ValType::I32, ValType::I32], &[]);
    let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
    let t_hread = type_index(&mut types, &[ValType::I32], &[]);
    let t_realloc = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_run = type_index(&mut types, &[], &[ValType::I32]);

    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }

    let mut imports = ImportSection::new();
    imports.import("wasi:cli/stdin@0.2.3", "get-stdin", EntityType::Function(t_get));
    imports.import("wasi:cli/stdout@0.2.3", "get-stdout", EntityType::Function(t_get));
    imports.import("wasi:cli/stderr@0.2.3", "get-stderr", EntityType::Function(t_get));
    imports.import("wasi:cli/exit@0.2.3", "exit", EntityType::Function(t_exit));
    imports.import(
        "wasi:io/streams@0.2.3",
        "[method]input-stream.blocking-read",
        EntityType::Function(t_read),
    );
    imports.import(
        "wasi:io/streams@0.2.3",
        "[method]output-stream.blocking-write-and-flush",
        EntityType::Function(t_write),
    );
    imports.import("wasi:clocks/wall-clock@0.2.3", "now", EntityType::Function(t_now));
    imports.import("wasi:random/random@0.2.3", "get-random-bytes", EntityType::Function(t_random));

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    for ti in [t_print, t_print, t_exit, t_fs, t_hread, t_realloc, t_run] {
        functions.function(ti);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + PARK_SPAN / 65536,
        // #1729: carry the heap-cap maximum through, span-shifted.
        maximum: old_mem_max.map(|m| m.max(old_mem_min) + PARK_SPAN / 65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    // Exports: memory survives (the canonical ABI reads/writes it), the
    // run world's entry is the lifted wrapper, and cabi_realloc is the
    // host's landing zone for returned lists. main/__heap exports are
    // DROPPED — a component models its exports, and the world has none
    // for them.
    let exports = {
        let mut e = wasm_encoder::ExportSection::new();
        e.export("memory", ExportKind::Memory, 0);
        e.export("wasi:cli/run@0.2.3#run", ExportKind::Func, f_run);
        e.export("cabi_realloc", ExportKind::Func, f_realloc);
        e
    };

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift: SHIFT };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, I_EXIT)?);
    }
    code.function(&shim_print(g_stdout, I_GET_STDOUT, park, true));
    code.function(&shim_print(g_stderr, I_GET_STDERR, park, true));
    code.function(&shim_exit());
    code.function(&shim_fs_call(park, g_plen, g_ppos, g_stdin, g_stdout, g_stderr));
    code.function(&shim_host_read(g_plen, g_ppos));
    code.function(&shim_cabi_realloc(heap_global));
    code.function(&shim_run(main_index + SHIFT));

    // Elements re-encode through the Remap (#1716): the +3 import shift
    // must move funcref table entries too, or every closure retargets
    // three functions early — the #1688 silent class, live here until
    // this line.
    let mut element_sec = wasm_encoder::ElementSection::new();
    for e in elements {
        use wasm_encoder::reencode::Reencode as _;
        remap
            .parse_element(&mut element_sec, e)
            .map_err(|e| anyhow::anyhow!("element reencode: {e:?}"))?;
    }

    data.active(0, &ConstExpr::i32_const((park + MSG) as i32), UNSUPPORTED_MSG.iter().copied());

    let mut m = Module::new();
    m.section(&type_sec)
        .section(&imports)
        .section(&functions)
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&element_sec)
        .section(&code)
        .section(&data);
    let mut core = m.finish();
    wasmparser::validate(&core)?;

    // Embed the world's component-type metadata, then encode WITHOUT an
    // adapter — the imports above ARE the canonical ABI. The WIT text is
    // COMPILED IN (include_str!): an installed binary must not depend on
    // the build tree existing at run time.
    let mut resolve = wit_parser::Resolve::default();
    for (name, text) in [
        ("io.wit", include_str!("../wit/deps/io/package.wit")),
        ("clocks.wit", include_str!("../wit/deps/clocks/package.wit")),
        ("random.wit", include_str!("../wit/deps/random/package.wit")),
        ("cli.wit", include_str!("../wit/deps/cli/package.wit")),
    ] {
        resolve
            .push_str(name, text)
            .map_err(|e| anyhow::anyhow!("wit {name}: {e}"))?;
    }
    let pkg = resolve
        .push_str("world.wit", include_str!("../wit/world.wit"))
        .map_err(|e| anyhow::anyhow!("wit world: {e}"))?;
    let world = resolve
        .select_world(&[pkg], Some("p2-command"))
        .map_err(|e| anyhow::anyhow!("world: {e}"))?;
    wit_component::embed_component_metadata(
        &mut core,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .map_err(|e| anyhow::anyhow!("embed: {e}"))?;
    let component = wit_component::ComponentEncoder::default()
        .module(&core)
        .map_err(|e| anyhow::anyhow!("module: {e}"))?
        .encode()
        .map_err(|e| anyhow::anyhow!("encode: {e}"))?;
    Ok(component)
}

/// Fetch-and-cache a stream handle: `if g < 0 { g = get() } g`.
fn load_handle(i: &mut wasm_encoder::InstructionSink<'_>, g: u32, get_import: u32) {
    i.global_get(g).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.call(get_import).global_set(g);
    i.end();
    i.global_get(g);
}

/// `(ptr, len) -> ()`: chunked blocking-write-and-flush (the spec caps a
/// single write at 4096 bytes), then a newline write when `newline`.
fn shim_print(g_handle: u32, get_import: u32, park: u64, newline: bool) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let chunk = 2u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(len).i32_eqz().br_if(1);
    // chunk = min(len, 4096)
    i.local_get(len).i32_const(4096).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(len);
    i.else_();
    i.i32_const(4096);
    i.end();
    i.local_set(chunk);
    load_handle(&mut i, g_handle, get_import);
    i.local_get(ptr).local_get(chunk);
    i.i32_const((park + RET) as i32);
    i.call(I_WRITE_FLUSH);
    i.local_get(ptr).local_get(chunk).i32_add().local_set(ptr);
    i.local_get(len).local_get(chunk).i32_sub().local_set(len);
    i.br(0).end().end();
    if newline {
        i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(0));
        load_handle(&mut i, g_handle, get_import);
        i.i32_const(park as i32).i32_const(1);
        i.i32_const((park + RET) as i32);
        i.call(I_WRITE_FLUSH);
    }
    i.end();
    f
}

/// `(code) -> ()`: exit(ok) for 0, exit(err) otherwise — the run world's
/// two codes, which are the deterministic corpus's two codes.
fn shim_exit() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).i32_const(0).i32_ne();
    i.call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// The five-op host contract over p2 (op codes shared with the embedded
/// host): 30 raw stdout, 31 stdin read-to-end, 35 stdin take-n, 32
/// entropy, 34 wall clock; anything else = the defined refusal.
fn shim_fs_call(
    park: u64,
    g_plen: u32,
    g_ppos: u32,
    g_stdin: u32,
    g_stdout: u32,
    g_stderr: u32,
) -> Function {
    let (op, _a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let total = 5u32;
    let n = 6u32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();

    // op 30: raw stdout append (no newline) — b carries the bytes.
    i.local_get(op).i32_const(30).i32_eq().if_(BlockType::Empty);
    // Chunked write, inline (mirrors shim_print's loop without \n).
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(b_len).i32_eqz().br_if(1);
    i.local_get(b_len).i32_const(4096).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(b_len);
    i.else_();
    i.i32_const(4096);
    i.end();
    i.local_set(n);
    load_handle(&mut i, g_stdout, I_GET_STDOUT);
    i.local_get(b_ptr).local_get(n);
    i.i32_const((park + RET) as i32);
    i.call(I_WRITE_FLUSH);
    i.local_get(b_ptr).local_get(n).i32_add().local_set(b_ptr);
    i.local_get(b_len).local_get(n).i32_sub().local_set(b_len);
    i.br(0).end().end();
    i.i64_const(0).return_();
    i.end();

    // op 35: stdin take up to a_len bytes — ONE blocking-read; the list
    // lands via cabi_realloc; EOF (err) answers 0 bytes.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    load_handle(&mut i, g_stdin, I_GET_STDIN);
    i.local_get(a_len).i64_extend_i32_u();
    i.i32_const((park + RET) as i32);
    i.call(I_BLOCKING_READ);
    // result tag 0 = ok(list ptr,len) — err (closed/failed) = 0 bytes.
    i.i32_const((park + RET) as i32).i32_load(mem(0)).i32_eqz();
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_ppos);
    i.i32_const((park + RET) as i32).i32_load(mem(8)).global_set(g_plen);
    i.else_();
    i.i32_const(0).global_set(g_plen);
    i.end();
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 31: stdin read-to-end — loop take-4096 into the park data span
    // (contiguous, host_read's contract); overflow takes the refusal.
    i.local_get(op).i32_const(31).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    load_handle(&mut i, g_stdin, I_GET_STDIN);
    i.i64_const(4096);
    i.i32_const((park + RET) as i32);
    i.call(I_BLOCKING_READ);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).i32_eqz().i32_eqz().br_if(1); // err -> EOF
    i.i32_const((park + RET) as i32).i32_load(mem(8)).local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    // room check: DATA span is the park's tail.
    i.local_get(total).local_get(n).i32_add();
    i.i32_const((PARK_SPAN - DATA) as i32).i32_gt_u();
    i.if_(BlockType::Empty);
    i.i32_const(1).call(I_EXIT).unreachable();
    i.end();
    i.i32_const((park + DATA) as i32).local_get(total).i32_add();
    i.i32_const((park + RET) as i32).i32_load(mem(4));
    i.local_get(n);
    i.memory_copy(0, 0);
    i.local_get(total).local_get(n).i32_add().local_set(total);
    i.br(0).end().end();
    i.i32_const((park + DATA) as i32).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.end();

    // op 32: entropy — n rides b_len; the list lands via cabi_realloc.
    i.local_get(op).i32_const(32).i32_eq().if_(BlockType::Empty);
    i.local_get(b_len).i64_extend_i32_u();
    i.i32_const((park + RET) as i32);
    i.call(I_RANDOM);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).global_set(g_ppos);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_plen);
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 34: wall clock, raw nanos (seconds * 1e9 + nanos).
    i.local_get(op).i32_const(34).i32_eq().if_(BlockType::Empty);
    i.i32_const((park + RET) as i32);
    i.call(I_CLOCK_NOW);
    i.i32_const((park + RET) as i32).i64_load(mem64(0));
    i.i64_const(1_000_000_000).i64_mul();
    i.i32_const((park + RET) as i32).i64_load32_u(mem(8));
    i.i64_add().return_();
    i.end();

    // Everything else: the defined refusal — the message on stderr, exit 1.
    load_handle(&mut i, g_stderr, I_GET_STDERR);
    i.i32_const((park + MSG) as i32);
    i.i32_const(UNSUPPORTED_MSG.len() as i32);
    i.i32_const((park + RET) as i32);
    i.call(I_WRITE_FLUSH);
    i.i32_const(1).call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// `(dst) -> ()`: copy the parked payload to guest memory.
fn shim_host_read(g_plen: u32, g_ppos: u32) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0);
    i.global_get(g_ppos);
    i.global_get(g_plen);
    i.memory_copy(0, 0);
    i.end();
    f
}

/// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr`: bump the
/// module's own `__heap` frontier (8-aligned raw bytes, never freed —
/// the block allocator's free lists are undisturbed), growing memory on
/// demand; a grow failure exits err (the OOM discipline).
fn shim_cabi_realloc(heap_global: u32) -> Function {
    let (old_ptr, old_size, _align, new_size) = (0u32, 1u32, 2u32, 3u32);
    let result = 4u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // result = (heap + 7) & !7
    i.global_get(heap_global).i32_const(7).i32_add().i32_const(-8).i32_and();
    i.local_set(result);
    // grow if result + new_size overruns memory
    i.local_get(result).local_get(new_size).i32_add();
    i.memory_size(0).i32_const(16).i32_shl();
    i.i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(new_size).i32_const(0xFFFF).i32_add().i32_const(16).i32_shr_u();
    i.memory_grow(0);
    i.i32_const(-1).i32_eq();
    i.if_(BlockType::Empty);
    i.i32_const(1).call(I_EXIT).unreachable();
    i.end();
    i.end();
    i.local_get(result).local_get(new_size).i32_add().global_set(heap_global);
    // realloc semantics: copy min(old_size, new_size) from old_ptr.
    i.local_get(old_ptr).i32_eqz().i32_eqz();
    i.if_(BlockType::Empty);
    i.local_get(result);
    i.local_get(old_ptr);
    i.local_get(old_size).local_get(new_size).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(old_size);
    i.else_();
    i.local_get(new_size);
    i.end();
    i.memory_copy(0, 0);
    i.end();
    i.local_get(result);
    i.end();
    f
}

/// `() -> i32`: the lifted `wasi:cli/run.run` — call main, answer ok.
/// (An abort inside main already left through the exit import.)
fn shim_run(main_index: u32) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.call(main_index);
    i.i32_const(0);
    i.end();
    f
}
