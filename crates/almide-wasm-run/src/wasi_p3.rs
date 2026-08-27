//! `to_p3` (#1628 stage 2, increment 1): rewrite an emitted almide module
//! into a WASI 0.3 COMPONENT — stdio over component-model streams. This
//! is the plumbing keystone for the fan lowering: guest-held streams,
//! sync stream/future builtins, and an async-lifted entry are exactly the
//! vocabulary fan arms will schedule on.
//!
//! Same doctrine and five-op host surface as `to_p2` (#1588/#1628 stage 1):
//! console out, exit codes, stdin (cursor + read-to-end), entropy, the wall
//! clock; fs/env/process keep the DEFINED refusal. The transform is a
//! post-pass — the emitter's verified envelope is untouched.
//!
//! The 0.3 shapes used (as wasmtime 46+ implements them; vendored WIT
//! under `crates/almide-wasm-run/wit/p3/`):
//!   - stdout/stderr `write-via-stream: func(stream<u8>) ->
//!     future<result<_, error-code>>` — a SYNC call handing the host the
//!     readable end and answering a completion future. The guest opens
//!     the stream ONCE, keeps the writable end, and feeds it with SYNC
//!     `stream.write` (each write rendezvous-blocks until the host
//!     consumed the bytes — program order per stream by construction);
//!   - stdin `read-via-stream: func() -> tuple<stream<u8>, future<...>>`
//!     — sync with a retptr; SYNC `stream.read` lands bytes straight in
//!     guest memory (no cabi_realloc hop, unlike p2's blocking-read);
//!   - the run export (`run: async func() -> result`) is lifted `async`
//!     with a CALLBACK, but the body runs to completion in the initial
//!     call — sync builtins may block inside a callback-lifted task (the
//!     sync-streams doctrine), so the callback itself is unreachable:
//!     main → close streams → read each completion future → task.return
//!     (ok) → EXIT;
//!   - `wasi:clocks/system-clock.now() -> instant` (s64 seconds, u32
//!     nanos) and `get-random-bytes` are plain sync lowers, as on p2.
//!
//! The finale's `future.read` on each output stream's completion future
//! is the DETERMINISTIC drain handshake: it blocks until the host
//! acknowledges the whole stream, so "the program exited" implies "every
//! byte reached the host" — the property the cross-target byte-identity
//! contract stands on. Sync `stream.write`/`stream.read`/`future.read`
//! are the 🚝 builtins: the runtime needs
//! `component-model-more-async-builtins` (wasmtime: `-W ...`, on by
//! default in current releases' `-S p3` stacks).

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use crate::wasi::{
    mem, mem8, parse_module, reencode_body, type_index, Parsed, Remap, DATA, MSG, PARK_SPAN,
    UNSUPPORTED_MSG,
};

// Import indices (18 imports replace the 5 almide.* ones).
const I_EXIT: u32 = 0;
const I_OUT_CALL: u32 = 1; // write-via-stream (stdout): (rx) -> future
const I_OUT_NEW: u32 = 2;
const I_OUT_WRITE: u32 = 3;
const I_OUT_DROP_TX: u32 = 4;
const I_OUT_FUT_READ: u32 = 5;
const I_ERR_CALL: u32 = 6;
const I_ERR_NEW: u32 = 7;
const I_ERR_WRITE: u32 = 8;
const I_ERR_DROP_TX: u32 = 9;
const I_ERR_FUT_READ: u32 = 10;
const I_STDIN_OPEN: u32 = 11; // read-via-stream (sync, retptr)
const I_STDIN_READ: u32 = 12;
const I_STDIN_DROP_RX: u32 = 13;
const I_STDIN_DROP_FUT: u32 = 14;
const I_CLOCK_NOW: u32 = 15;
const I_RANDOM: u32 = 16;
const I_TASK_RETURN: u32 = 17;
const IMPORTS: u32 = 18;
const SHIFT: u32 = IMPORTS - 5;

// Park offsets past the shared ones: retptr / future-payload scratch.
const RET: u64 = 32;

/// 8-byte MemArg.
fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}

pub fn to_p3(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let parsed = parse_module(bytes)?;
    let Parsed {
        mut types,
        func_types,
        tables,
        old_mem_min,
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
    // exit, fs_call, host_read), then cabi_realloc, run, callback.
    let f_realloc = shim_base + 5;
    let f_run = shim_base + 6;
    let f_callback = shim_base + 7;

    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    // Globals: originals, then plen, ppos, the stream state: stdout/stderr
    // writable ends + completion futures, stdin readable + its future.
    let (g_plen, g_ppos) = (global_count, global_count + 1);
    let (g_out_tx, g_out_fut) = (global_count + 2, global_count + 3);
    let (g_err_tx, g_err_fut) = (global_count + 4, global_count + 5);
    let (g_in_rx, g_in_fut) = (global_count + 6, global_count + 7);
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
    for _ in 0..6 {
        globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stream state
    }

    // Canonical-ABI core types.
    let t_exit = type_index(&mut types, &[ValType::I32], &[]);
    let t_call = type_index(&mut types, &[ValType::I32], &[ValType::I32]);
    let t_new = type_index(&mut types, &[], &[ValType::I64]);
    let t_rw = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I32],
        &[ValType::I32],
    );
    let t_fut_read = type_index(&mut types, &[ValType::I32, ValType::I32], &[ValType::I32]);
    let t_drop = type_index(&mut types, &[ValType::I32], &[]);
    let t_retptr = type_index(&mut types, &[ValType::I32], &[]);
    let t_random = type_index(&mut types, &[ValType::I64, ValType::I32], &[]);
    // Shim types (the almide.* signatures) + realloc + run + callback.
    let t_print = type_index(&mut types, &[ValType::I32, ValType::I32], &[]);
    let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
    let t_hread = type_index(&mut types, &[ValType::I32], &[]);
    let t_realloc = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_status = type_index(&mut types, &[], &[ValType::I32]);
    let t_callback = type_index(&mut types, &[ValType::I32; 3], &[ValType::I32]);

    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }

    let mut imports = ImportSection::new();
    imports.import("wasi:cli/exit@0.3.0", "exit", EntityType::Function(t_exit));
    for iface in ["wasi:cli/stdout@0.3.0", "wasi:cli/stderr@0.3.0"] {
        imports.import(iface, "write-via-stream", EntityType::Function(t_call));
        imports.import(iface, "[stream-new-0]write-via-stream", EntityType::Function(t_new));
        imports.import(iface, "[stream-write-0]write-via-stream", EntityType::Function(t_rw));
        imports.import(
            iface,
            "[stream-drop-writable-0]write-via-stream",
            EntityType::Function(t_drop),
        );
        imports.import(iface, "[future-read-1]write-via-stream", EntityType::Function(t_fut_read));
    }
    imports.import("wasi:cli/stdin@0.3.0", "read-via-stream", EntityType::Function(t_retptr));
    imports.import(
        "wasi:cli/stdin@0.3.0",
        "[stream-read-0]read-via-stream",
        EntityType::Function(t_rw),
    );
    imports.import(
        "wasi:cli/stdin@0.3.0",
        "[stream-drop-readable-0]read-via-stream",
        EntityType::Function(t_drop),
    );
    imports.import(
        "wasi:cli/stdin@0.3.0",
        "[future-drop-readable-1]read-via-stream",
        EntityType::Function(t_drop),
    );
    imports.import("wasi:clocks/system-clock@0.3.0", "now", EntityType::Function(t_retptr));
    imports.import("wasi:random/random@0.3.0", "get-random-bytes", EntityType::Function(t_random));
    imports.import(
        "[export]wasi:cli/run@0.3.0",
        "[task-return]run",
        EntityType::Function(t_exit),
    );

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    for ti in [t_print, t_print, t_exit, t_fs, t_hread, t_realloc, t_status, t_callback] {
        functions.function(ti);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + PARK_SPAN / 65536,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let exports = {
        let mut e = wasm_encoder::ExportSection::new();
        e.export("memory", ExportKind::Memory, 0);
        e.export("[async-lift]wasi:cli/run@0.3.0#run", ExportKind::Func, f_run);
        e.export(
            "[callback][async-lift]wasi:cli/run@0.3.0#run",
            ExportKind::Func,
            f_callback,
        );
        e.export("cabi_realloc", ExportKind::Func, f_realloc);
        e
    };

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift: SHIFT };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, I_EXIT)?);
    }
    code.function(&shim_print(g_out_tx, g_out_fut, I_OUT_CALL, I_OUT_NEW, I_OUT_WRITE, park, true));
    code.function(&shim_print(g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, I_ERR_WRITE, park, true));
    code.function(&shim_exit());
    code.function(&shim_fs_call(
        park, g_plen, g_ppos, g_in_rx, g_in_fut, g_out_tx, g_out_fut, g_err_tx, g_err_fut,
    ));
    code.function(&shim_host_read(g_plen, g_ppos));
    code.function(&shim_cabi_realloc(heap_global));
    code.function(&shim_run(
        main_index + SHIFT,
        park,
        g_out_tx,
        g_out_fut,
        g_err_tx,
        g_err_fut,
        g_in_rx,
        g_in_fut,
    ));
    code.function(&shim_callback());

    data.active(0, &ConstExpr::i32_const((park + MSG) as i32), UNSUPPORTED_MSG.iter().copied());

    let mut m = Module::new();
    m.section(&type_sec)
        .section(&imports)
        .section(&functions)
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&elements)
        .section(&code)
        .section(&data);
    let mut core = m.finish();
    wasmparser::validate(&core)?;

    let mut resolve = wit_parser::Resolve::default();
    for (name, text) in [
        ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
        ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
        ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
    ] {
        resolve
            .push_str(name, text)
            .map_err(|e| anyhow::anyhow!("wit {name}: {e}"))?;
    }
    let pkg = resolve
        .push_str("world.wit", include_str!("../wit/p3/world.wit"))
        .map_err(|e| anyhow::anyhow!("wit world: {e}"))?;
    let world = resolve
        .select_world(&[pkg], Some("p3-command"))
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

/// Lazy stream open: `if g_tx < 0 { (rx,tx) = stream.new; g_tx = tx;
/// g_fut = write-via-stream(rx) }`. The host's read side starts
/// concurrently; every later sync write rendezvous-blocks against it.
fn open_stream(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_tx: u32,
    g_fut: u32,
    call_import: u32,
    new_import: u32,
    scratch64: u32,
) {
    i.global_get(g_tx).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.call(new_import).local_set(scratch64);
    // tx = high half, rx = low half (the sync-streams.wast packing)
    i.local_get(scratch64).i64_const(32).i64_shr_u().i32_wrap_i64().global_set(g_tx);
    i.local_get(scratch64).i32_wrap_i64();
    i.call(call_import).global_set(g_fut);
    i.end();
}

/// Sync write loop: `while len > 0 { r = stream.write(tx, ptr, len);
/// n = r >> 4; if n == 0 { break } ptr += n; len -= n }` — a DROPPED
/// status answers n=0 and the loop exits (output sunk, as p2/POSIX).
fn write_all(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_tx: u32,
    write_import: u32,
    ptr: u32,
    len: u32,
    n: u32,
) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(len).i32_eqz().br_if(1);
    i.global_get(g_tx);
    i.local_get(ptr).local_get(len);
    i.call(write_import);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(ptr).local_get(n).i32_add().local_set(ptr);
    i.local_get(len).local_get(n).i32_sub().local_set(len);
    i.br(0).end().end();
}

/// `(ptr, len) -> ()`: open-once, sync-write-all, then the newline.
fn shim_print(
    g_tx: u32,
    g_fut: u32,
    call_import: u32,
    new_import: u32,
    write_import: u32,
    park: u64,
    newline: bool,
) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let n = 2u32;
    let s64 = 3u32;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    // locals: 0 ptr, 1 len (params), 2 n (i32), 3 scratch (i64)
    let mut i = f.instructions();
    open_stream(&mut i, g_tx, g_fut, call_import, new_import, s64);
    write_all(&mut i, g_tx, write_import, ptr, len, n);
    if newline {
        i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(0));
        i.i32_const(park as i32).local_set(ptr);
        i.i32_const(1).local_set(len);
        write_all(&mut i, g_tx, write_import, ptr, len, n);
    }
    i.end();
    f
}

/// `(code) -> ()`: exit(ok) for 0, exit(err) otherwise.
fn shim_exit() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).i32_const(0).i32_ne();
    i.call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// Lazy stdin open: `if g_rx < 0 { read-via-stream(retptr); g_rx =
/// mem[ret]; g_fut = mem[ret+4] }`.
fn open_stdin(i: &mut wasm_encoder::InstructionSink<'_>, g_rx: u32, g_fut: u32, park: u64) {
    i.global_get(g_rx).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32);
    i.call(I_STDIN_OPEN);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).global_set(g_rx);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_fut);
    i.end();
}

/// The five-op host contract over p3 (op codes shared with the embedded
/// host): 30 raw stdout, 31 stdin read-to-end, 35 stdin take-n, 32
/// entropy, 34 wall clock; anything else = the defined refusal.
#[allow(clippy::too_many_arguments)]
fn shim_fs_call(
    park: u64,
    g_plen: u32,
    g_ppos: u32,
    g_in_rx: u32,
    g_in_fut: u32,
    g_out_tx: u32,
    g_out_fut: u32,
    g_err_tx: u32,
    g_err_fut: u32,
) -> Function {
    let (op, _a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let total = 5u32;
    let n = 6u32;
    let s64 = 7u32;
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();

    // op 30: raw stdout append (no newline) — b carries the bytes.
    i.local_get(op).i32_const(30).i32_eq().if_(BlockType::Empty);
    open_stream(&mut i, g_out_tx, g_out_fut, I_OUT_CALL, I_OUT_NEW, s64);
    write_all(&mut i, g_out_tx, I_OUT_WRITE, b_ptr, b_len, n);
    i.i64_const(0).return_();
    i.end();

    // op 35: stdin take up to a_len bytes — ONE sync read, straight into
    // the park data span; a DROPPED status (writer closed) answers 0.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    open_stdin(&mut i, g_in_rx, g_in_fut, park);
    i.global_get(g_in_rx);
    i.i32_const((park + DATA) as i32);
    i.local_get(a_len);
    i.call(I_STDIN_READ);
    i.i32_const(4).i32_shr_u().global_set(g_plen);
    i.i32_const((park + DATA) as i32).global_set(g_ppos);
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 31: stdin read-to-end — sync reads into the park data span until
    // DROPPED/empty; overflow takes the refusal.
    i.local_get(op).i32_const(31).i32_eq().if_(BlockType::Empty);
    open_stdin(&mut i, g_in_rx, g_in_fut, park);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    // room check: DATA span is the park's tail.
    i.local_get(total).i32_const((PARK_SPAN - DATA) as i32).i32_ge_u();
    i.if_(BlockType::Empty);
    i.i32_const(1).call(I_EXIT).unreachable();
    i.end();
    i.global_get(g_in_rx);
    i.i32_const((park + DATA) as i32).local_get(total).i32_add();
    i.i32_const((PARK_SPAN - DATA) as i32).local_get(total).i32_sub();
    i.call(I_STDIN_READ);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
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
    open_stream(&mut i, g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, s64);
    i.i32_const((park + MSG) as i32).local_set(b_ptr);
    i.i32_const(UNSUPPORTED_MSG.len() as i32).local_set(b_len);
    write_all(&mut i, g_err_tx, I_ERR_WRITE, b_ptr, b_len, n);
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
/// module's own `__heap` frontier (8-aligned raw bytes, never freed),
/// growing memory on demand; a grow failure exits err.
fn shim_cabi_realloc(heap_global: u32) -> Function {
    let (old_ptr, old_size, _align, new_size) = (0u32, 1u32, 2u32, 3u32);
    let result = 4u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.global_get(heap_global).i32_const(7).i32_add().i32_const(-8).i32_and();
    i.local_set(result);
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

/// `() -> status`: the async-lifted `wasi:cli/run.run` — call main, close
/// both output streams and BLOCK on each completion future (the
/// deterministic drain handshake), drop the stdin readables, task.return
/// (ok), answer EXIT (0). The callback is never reached: everything
/// happened in the initial call.
#[allow(clippy::too_many_arguments)]
fn shim_run(
    main_index: u32,
    park: u64,
    g_out_tx: u32,
    g_out_fut: u32,
    g_err_tx: u32,
    g_err_fut: u32,
    g_in_rx: u32,
    g_in_fut: u32,
) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.call(main_index);

    // End both streams, then wait for the host's completion future: the
    // future.read blocks until every byte is drained host-side.
    for (g_tx, g_fut, drop_import, read_import) in [
        (g_out_tx, g_out_fut, I_OUT_DROP_TX, I_OUT_FUT_READ),
        (g_err_tx, g_err_fut, I_ERR_DROP_TX, I_ERR_FUT_READ),
    ] {
        i.global_get(g_tx).i32_const(0).i32_ge_s();
        i.if_(BlockType::Empty);
        i.global_get(g_tx).call(drop_import);
        i.global_get(g_fut).i32_const((park + RET) as i32).call(read_import);
        i.drop();
        i.end();
    }

    // Drop the stdin readables if they were opened.
    i.global_get(g_in_rx).i32_const(0).i32_ge_s();
    i.if_(BlockType::Empty);
    i.global_get(g_in_rx).call(I_STDIN_DROP_RX);
    i.global_get(g_in_fut).call(I_STDIN_DROP_FUT);
    i.end();

    // task.return(ok), then EXIT: the task is complete.
    i.i32_const(0).call(I_TASK_RETURN);
    i.i32_const(0);
    i.end();
    f
}

/// `(event, p1, p2) -> status`: never reached — the initial call runs to
/// completion (sync builtins block inside the task instead of yielding).
fn shim_callback() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.unreachable();
    i.end();
    f
}
