//! `to_wasi` (#1588): rewrite an emitted almide module into a
//! self-sufficient WASI preview-1 command that runs on STOCK runtimes
//! (`wasmtime run mod.wasm`, wasmer, browsers with a p1 shim) — no
//! bespoke host.
//!
//! The transform is a POST-PASS, so the emitter and its verified
//! envelope stay untouched:
//!   - the 5 `almide.*` imports are replaced by 5 WASI imports (SAME
//!     count, so every other function index is preserved verbatim);
//!   - every call to an old import retargets to one of 5 appended SHIM
//!     functions implementing the almide host contract over WASI
//!     (fd_write / proc_exit / random_get / clock_time_get / fd_read);
//!   - one PARK page is appended to linear memory for iovecs, the
//!     stdin/entropy buffer (grown on demand), and the unsupported-op
//!     message; two globals carry the park length and capacity.
//!
//! Supported host surface (the non-host-variant corpus): console
//! output (println/eprintln/io.print/io.write), exit codes, stdin
//! read-to-end, entropy, the wall clock. fs/env/process ops take the
//! DEFINED refusal: a named message on stderr + exit 1 — never a
//! silent wrong answer (the target-availability doctrine, #1423).

use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ElementSection, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg,
    MemorySection, MemoryType, Module, TableSection, TypeSection, ValType,
};
use wasmparser::{Parser, Payload};

/// The host ops `shim_fs_call` SERVES on a stock WASI runtime. The build
/// path audits an artifact's emitted op set against this before shipping
/// (an unserved op = a runtime refusal on a runtime the developer never
/// ran — the env.set lesson): extend the shim and this list TOGETHER.
pub const P1_SERVED_OPS: &[i32] = &[26, 29, 30, 31, 32, 34, 35, 36, 37];

pub(crate) const UNSUPPORTED_MSG: &[u8] = b"Error: host op unsupported in the WASI build\n";
// Park-page layout (offsets from park base).
pub(crate) const IOV: u64 = 0; // two iovec entries (16 bytes)
pub(crate) const NREAD: u64 = 16;
pub(crate) const NL: u64 = 24;
pub(crate) const MSG: u64 = 64;
pub(crate) const DATA: u64 = 1024; // stdin/entropy bytes + op result staging
/// The env.set overlay log (#1716): [klen u32][vlen u32][key][val] entries,
/// append-only, scanned last-write-wins by op 26. Its page sits ABOVE the
/// stdin ceiling (g_pcap inits to park+OVL), so read-to-end can never run
/// into it.
pub(crate) const OVL: u64 = 4 * 65536;
/// The park span: five pages carved out at the original heap base — four
/// for iovecs/messages/stdin, one for the env overlay log.
pub(crate) const PARK_SPAN: u64 = 5 * 65536;

pub(crate) struct Remap {
    pub(crate) shim_base: u32,
    /// How far NON-import function indices move (0 for the p1 build — it
    /// keeps the import count at five; the p2 build imports eight, so
    /// every original index >= 5 shifts by three).
    pub(crate) shift: u32,
}

impl Reencode for Remap {
    type Error = std::convert::Infallible;
    fn function_index(&mut self, func: u32) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(if func < 5 { self.shim_base + func } else { func + self.shift })
    }
}

pub(crate) fn mem(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

pub(crate) fn mem8(offset: u64) -> MemArg {
    MemArg { offset, align: 0, memory_index: 0 }
}

/// Find a function type's index, or append it.
pub(crate) fn type_index(
    types: &mut Vec<(Vec<ValType>, Vec<ValType>)>,
    params: &[ValType],
    results: &[ValType],
) -> u32 {
    if let Some(i) = types.iter().position(|(p, r)| p == params && r == results) {
        return i as u32;
    }
    types.push((params.to_vec(), results.to_vec()));
    types.len() as u32 - 1
}

/// Everything `to_wasi` needs out of the source module, in one parse pass.
pub(crate) struct Parsed<'a> {
    pub(crate) types: Vec<(Vec<ValType>, Vec<ValType>)>,
    pub(crate) func_types: Vec<u32>,
    pub(crate) tables: TableSection,
    pub(crate) old_mem_min: u64,
    /// The source module's declared memory MAXIMUM (the #1729 heap-cap,
    /// baked by the structural emitter) — carried through the transform,
    /// widened by the PARK_SPAN pages this shim appends.
    pub(crate) old_mem_max: Option<u64>,
    pub(crate) parsed_globals: Vec<(GlobalType, Option<i32>, Option<i64>, Option<u64>)>,
    pub(crate) global_count: u32,
    pub(crate) heap_global: Option<u32>,
    /// Raw export rows — Func indices are ORIGINAL and must be shifted by
    /// the transform's import delta when rebuilt (#1716).
    pub(crate) exports: Vec<(String, ExportKind, u32)>,
    pub(crate) main_index: Option<u32>,
    /// Raw element segments: each transform re-encodes them through its
    /// own `Remap`, so funcref table entries shift with the import count
    /// (the #1688 silent-corruption class — a verbatim roundtrip under a
    /// nonzero shift retargets every closure).
    pub(crate) elements: Vec<wasmparser::Element<'a>>,
    pub(crate) data: DataSection,
    pub(crate) bodies: Vec<wasmparser::FunctionBody<'a>>,
}

/// One global's type and const-init operands (i32/i64/f64 — the only forms
/// the emitter produces).
fn parse_global(
    g: wasmparser::Global<'_>,
) -> anyhow::Result<(GlobalType, Option<i32>, Option<i64>, Option<u64>)> {
    let mut init_i32: Option<i32> = None;
    let mut init_i64: Option<i64> = None;
    let mut init_f64: Option<u64> = None;
    for opr in g.init_expr.get_operators_reader() {
        match opr? {
            wasmparser::Operator::I32Const { value } => init_i32 = Some(value),
            wasmparser::Operator::I64Const { value } => init_i64 = Some(value),
            wasmparser::Operator::F64Const { value } => init_f64 = Some(value.bits()),
            wasmparser::Operator::End => {}
            other => anyhow::bail!("non-const global init {other:?}"),
        }
    }
    let gt = GlobalType {
        val_type: RoundtripReencoder.val_type(g.ty.content_type).expect("valtype"),
        mutable: g.ty.mutable,
        shared: g.ty.shared,
    };
    Ok((gt, init_i32, init_i64, init_f64))
}

/// One export row, noting the two the transform anchors on (`main`, `__heap`).
fn parse_export(e: wasmparser::Export<'_>, p: &mut Parsed<'_>) -> anyhow::Result<()> {
    let kind = match e.kind {
        wasmparser::ExternalKind::Func => {
            if e.name == "main" {
                p.main_index = Some(e.index);
            }
            ExportKind::Func
        }
        wasmparser::ExternalKind::Memory => ExportKind::Memory,
        wasmparser::ExternalKind::Global => {
            if e.name == "__heap" {
                p.heap_global = Some(e.index);
            }
            ExportKind::Global
        }
        wasmparser::ExternalKind::Table => ExportKind::Table,
        wasmparser::ExternalKind::Tag => ExportKind::Tag,
        other => anyhow::bail!("unexpected export kind {other:?}"),
    };
    p.exports.push((e.name.to_string(), kind, e.index));
    Ok(())
}

pub(crate) fn parse_module(bytes: &[u8]) -> anyhow::Result<Parsed<'_>> {
    let mut p = Parsed {
        types: Vec::new(),
        func_types: Vec::new(),
        tables: TableSection::new(),
        old_mem_min: 0,
        old_mem_max: None,
        parsed_globals: Vec::new(),
        global_count: 0,
        heap_global: None,
        exports: Vec::new(),
        main_index: None,
        elements: Vec::new(),
        data: DataSection::new(),
        bodies: Vec::new(),
    };
    for payload in Parser::new(0).parse_all(bytes) {
        match payload? {
            Payload::TypeSection(r) => {
                for t in r.into_iter_err_on_gc_types() {
                    let ft = t?;
                    let conv = |v: &[wasmparser::ValType]| -> Vec<ValType> {
                        v.iter()
                            .map(|x| RoundtripReencoder.val_type(*x).expect("core valtype"))
                            .collect()
                    };
                    p.types.push((conv(ft.params()), conv(ft.results())));
                }
            }
            Payload::FunctionSection(r) => {
                for ti in r {
                    p.func_types.push(ti?);
                }
            }
            Payload::TableSection(r) => {
                for t in r {
                    let t = t?;
                    p.tables.table(RoundtripReencoder.table_type(t.ty).expect("table type"));
                }
            }
            Payload::MemorySection(r) => {
                for m in r {
                    let m = m?;
                    p.old_mem_min = m.initial;
                    p.old_mem_max = m.maximum;
                }
            }
            Payload::GlobalSection(r) => {
                for g in r {
                    p.parsed_globals.push(parse_global(g?)?);
                    p.global_count += 1;
                }
            }
            Payload::ExportSection(r) => {
                for e in r {
                    parse_export(e?, &mut p)?;
                }
            }
            Payload::ElementSection(r) => {
                for e in r {
                    p.elements.push(e?);
                }
            }
            Payload::DataSection(r) => {
                let mut re = RoundtripReencoder;
                for d in r {
                    re.parse_data(&mut p.data, d?).expect("data");
                }
            }
            Payload::CodeSectionEntry(b) => p.bodies.push(b),
            _ => {}
        }
    }
    Ok(p)
}

pub fn to_wasi(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
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
        exports: export_rows,
        main_index,
        elements,
        mut data,
        bodies,
    } = parsed;
    let main_index = main_index.ok_or_else(|| anyhow::anyhow!("no main export"))?;
    let heap_global = heap_global.ok_or_else(|| anyhow::anyhow!("no __heap export"))?;
    // 9 WASI imports replace the 5 almide.* ones (#1716 added the
    // environ/args quartet), so every non-import index shifts by 4.
    const IMPORTS: u32 = 9;
    const SHIFT: u32 = IMPORTS - 5;
    let shim_base = IMPORTS + func_types.len() as u32;
    // The park CANNOT live past the current memory end — the bump heap
    // grows there. It takes over the ORIGINAL heap base instead, and
    // the heap's initial pointer moves up by the span: nothing else
    // reads that global's init, and the rc heap-floor guards only ever
    // see block handles.
    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    let (g_plen, g_pcap, g_ovl) = (global_count, global_count + 1, global_count + 2);
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

    // WASI import types.
    let t_fd_rw = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        &[ValType::I32],
    );
    let t_exit = type_index(&mut types, &[ValType::I32], &[]);
    let t_random = type_index(&mut types, &[ValType::I32, ValType::I32], &[ValType::I32]);
    let t_clock =
        type_index(&mut types, &[ValType::I32, ValType::I64, ValType::I32], &[ValType::I32]);
    // Shim types mirror the almide.* signatures.
    let t_print = type_index(&mut types, &[ValType::I32, ValType::I32], &[]);
    let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
    let t_read = type_index(&mut types, &[ValType::I32], &[]);

    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }

    let mut imports = ImportSection::new();
    const W: &str = "wasi_snapshot_preview1";
    imports.import(W, "fd_write", EntityType::Function(t_fd_rw)); // 0
    imports.import(W, "proc_exit", EntityType::Function(t_exit)); // 1
    imports.import(W, "random_get", EntityType::Function(t_random)); // 2
    imports.import(W, "clock_time_get", EntityType::Function(t_clock)); // 3
    imports.import(W, "fd_read", EntityType::Function(t_fd_rw)); // 4
    // The environ/args quartet (#1716) — all share the (ptr, ptr) -> errno
    // shape. Appended AFTER the original five so the shim bodies' literal
    // import indices 0..4 stay put.
    imports.import(W, "environ_sizes_get", EntityType::Function(t_random)); // 5
    imports.import(W, "environ_get", EntityType::Function(t_random)); // 6
    imports.import(W, "args_sizes_get", EntityType::Function(t_random)); // 7
    imports.import(W, "args_get", EntityType::Function(t_random)); // 8

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    for ti in [t_print, t_print, t_exit, t_fs, t_read, t_fs, t_fs, t_fs] {
        functions.function(ti);
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + PARK_SPAN / 65536,
        // Preserve the heap-cap maximum (#1729), shifted by the same span
        // the minimum gained — dropping it silently un-capped every
        // `--heap-cap` structural artifact.
        maximum: old_mem_max.map(|m| m.max(old_mem_min) + PARK_SPAN / 65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(0),
    );
    // g_pcap: the stdin read-to-end ceiling — the overlay page above it
    // is the env log's, never stdin's.
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const((park + OVL) as i32),
    );
    // g_ovl: bytes appended to the env overlay log so far.
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(0),
    );

    let mut exports = ExportSection::new();
    for (name, kind, idx) in &export_rows {
        let idx = if *kind == ExportKind::Func { *idx + SHIFT } else { *idx };
        exports.export(name, *kind, idx);
    }
    exports.export("_start", ExportKind::Func, main_index + SHIFT);

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift: SHIFT };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, 1)?);
    }
    // Shims (their own calls target the NEW imports — no remap). Order:
    // println, eprintln, exit, fs_call, host_read, env_get, env_set, args.
    let (f_env_get, f_env_set, f_args) = (shim_base + 5, shim_base + 6, shim_base + 7);
    code.function(&shim_print(1, park));
    code.function(&shim_print(2, park));
    code.function(&shim_exit());
    code.function(&shim_fs_call(park, g_plen, g_pcap, f_env_get, f_env_set, f_args));
    code.function(&shim_host_read(park, g_plen));
    code.function(&shim_env_get(park, g_plen, g_ovl));
    code.function(&shim_env_set(park, g_ovl));
    code.function(&shim_args(park, g_plen));

    let mut element_sec = ElementSection::new();
    for e in elements {
        remap
            .parse_element(&mut element_sec, e)
            .map_err(|e| anyhow::anyhow!("element reencode: {e:?}"))?;
    }

    data.active(
        0,
        &ConstExpr::i32_const((park + MSG) as i32),
        UNSUPPORTED_MSG.iter().copied(),
    );

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
    let out = m.finish();
    wasmparser::validate(&out)?;
    Ok(out)
}

/// Hand-rolled body reencode: call indices remap through the shims, and
/// every `unreachable` gets a `proc_exit(1)` injected in front — the
/// almide host CONTRACT maps any trap to exit 1 (W-7's semantic-abort/
/// engine-fault split), and a stock WASI runtime would otherwise surface
/// 128+SIGABRT. The trailing `unreachable` stays for stack-polymorphic
/// validity.
pub(crate) fn reencode_body(b: &wasmparser::FunctionBody<'_>, remap: &mut Remap, exit_fn: u32) -> anyhow::Result<Function> {
    let locals: Vec<(u32, ValType)> = b
        .get_locals_reader()?
        .into_iter()
        .map(|l| {
            let (n, ty) = l.expect("local");
            (n, RoundtripReencoder.val_type(ty).expect("valtype"))
        })
        .collect();
    let mut f = Function::new(locals);
    for op in b.get_operators_reader()? {
        let op = op?;
        if matches!(op, wasmparser::Operator::Unreachable) {
            // A trap becomes the DEFINED failure exit (p1: proc_exit(1);
            // p2: wasi:cli/exit exit(err)) — both spell "exit code 1".
            f.instructions().i32_const(1).call(exit_fn);
        }
        let inst = remap.instruction(op).expect("instruction reencode");
        f.instruction(&inst);
    }
    Ok(f)
}

/// `(ptr, len) -> ()`: fd_write(fd, [(ptr,len),("\n",1)]).
fn shim_print(fd: i32, park: u64) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.i32_const(park as i32).local_get(ptr).i32_store(mem(IOV));
    i.i32_const(park as i32).local_get(len).i32_store(mem(IOV + 4));
    i.i32_const(fd);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0); // fd_write: the payload
    i.drop();
    i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(NL));
    i.i32_const(park as i32).i32_const((park + NL) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(1).i32_store(mem(IOV + 4));
    i.i32_const(fd);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0); // fd_write: the newline
    i.drop();
    i.end();
    f
}

/// `(code) -> ()`: proc_exit never returns.
fn shim_exit() -> Function {
    let mut f = Function::new([]);
    f.instructions().local_get(0).call(1).unreachable().end();
    f
}

/// The almide `fs_call` contract over WASI: ops 26/29/30/31/32/34/35/36/37
/// supported (the environ/args trio routes to its own shims, #1716),
/// everything else takes the defined refusal (stderr + exit 1).
fn shim_fs_call(
    park: u64,
    g_plen: u32,
    g_pcap: u32,
    f_env_get: u32,
    f_env_set: u32,
    f_args: u32,
) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len; locals: 5=total 6=nread
    // 7=deadline (i64, op 36)
    let (op, a_len, b_ptr, b_len, total, nread) = (0u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let deadline = 7u32;
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();

    // ops 26/37/29: env.get / env.set / args — forwarded whole.
    for (code, target) in [(26, f_env_get), (37, f_env_set), (29, f_args)] {
        i.local_get(op).i32_const(code).i32_eq().if_(BlockType::Empty);
        for p in 0..5u32 {
            i.local_get(p);
        }
        i.call(target).return_();
        i.end();
    }

    // op 30: raw stdout append.
    i.local_get(op).i32_const(30).i32_eq().if_(BlockType::Empty);
    i.i32_const(park as i32).local_get(b_ptr).i32_store(mem(IOV));
    i.i32_const(park as i32).local_get(b_len).i32_store(mem(IOV + 4));
    i.i32_const(1);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i64_const(0).return_();
    i.end();

    // op 35: incremental stdin — ONE fd_read of up to min(a_len, 4096)
    // bytes into the park data region (the count rides in a_len, op 32's
    // b_len convention). Short reads are the contract ("up to n"): the
    // guest's read_line/read_byte loops ask byte-at-a-time, so one
    // fd_read per call is exactly the incumbent leg's cadence. An errno
    // or EOF answers 0 bytes.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    i.i32_const(park as i32).i32_const((park + DATA) as i32).i32_store(mem(IOV));
    // len = clamp(a_len, 0..=4096) — unsigned min folds a negative count
    // into the 4096 arm, and 4096 stays inside the fixed park span.
    i.local_get(a_len).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.i32_const(0).local_set(a_len);
    i.end();
    i.local_get(a_len).i32_const(4096).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(a_len);
    i.else_();
    i.i32_const(4096);
    i.end();
    i.local_set(nread);
    i.i32_const(park as i32).local_get(nread).i32_store(mem(IOV + 4));
    i.i32_const(0);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(4); // fd_read
    i.if_(BlockType::Empty); // errno != 0 → 0 bytes
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(nread);
    i.local_get(nread).global_set(g_plen);
    i.local_get(nread).i64_extend_i32_u().return_();
    i.end();

    // op 31: stdin read-to-end into the park data region (grown on demand).
    i.local_get(op).i32_const(31).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    // Room: the park span is FIXED (the heap owns everything above) —
    // a stdin larger than it takes the defined refusal, never a
    // truncation.
    i.i32_const((park + DATA + 4096) as i32).local_get(total).i32_add();
    i.global_get(g_pcap).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(park as i32).i32_const((park + MSG) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(UNSUPPORTED_MSG.len() as i32).i32_store(mem(IOV + 4));
    i.i32_const(2);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i32_const(1).call(1);
    i.unreachable();
    i.end();
    // iovec = (park+DATA+total, 4096)
    i.i32_const(park as i32);
    i.i32_const((park + DATA) as i32).local_get(total).i32_add();
    i.i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(4096).i32_store(mem(IOV + 4));
    i.i32_const(0);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(4); // fd_read
    i.br_if(1); // errno != 0 → done with what we have
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(nread);
    i.local_get(nread).i32_eqz().br_if(1); // EOF
    i.local_get(total).local_get(nread).i32_add().local_set(total);
    i.br(0).end().end();
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.end();

    // op 32: entropy into the park data region (count rides in b_len).
    i.local_get(op).i32_const(32).i32_eq().if_(BlockType::Empty);
    i.i32_const((park + DATA) as i32).local_get(b_len).call(2).drop();
    i.local_get(b_len).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();

    // op 34: the wall clock, raw nanos.
    i.local_get(op).i32_const(34).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0)).return_();
    i.end();

    // op 36: env.sleep_ms — a MONOTONIC busy-wait over clock_time_get
    // (the ms count rides a_len, the op-35 scalar convention). WASI p1
    // has no sleep primitive short of poll_oneoff; the import-count
    // objection died with #1716 (elements re-encode through the Remap
    // now), so the spin survives only until someone wires poll_oneoff.
    // The embedded host and native sleep properly; the CPU burn is
    // confined to stock-runtime artifacts.
    i.local_get(op).i32_const(36).i32_eq().if_(BlockType::Empty);
    i.local_get(a_len).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.i32_const(0).local_set(a_len);
    i.end();
    i.i32_const(1).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0));
    i.local_get(a_len).i64_extend_i32_u().i64_const(1_000_000).i64_mul();
    i.i64_add().local_set(deadline);
    i.loop_(BlockType::Empty);
    i.i32_const(1).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0));
    i.local_get(deadline).i64_lt_u().br_if(0);
    i.end();
    i.i64_const(0).return_();
    i.end();

    // Everything else: the defined refusal.
    i.i32_const(park as i32).i32_const((park + MSG) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(UNSUPPORTED_MSG.len() as i32).i32_store(mem(IOV + 4));
    i.i32_const(2);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i32_const(1).call(1); // proc_exit(1)
    i.unreachable();
    i.end();
    f
}

/// `(dst) -> ()`: copy the parked bytes into guest memory.
fn shim_host_read(park: u64, g_plen: u32) -> Function {
    let dst = 0u32;
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(dst);
    i.i32_const((park + DATA) as i32);
    i.global_get(g_plen);
    i.memory_copy(0, 0);
    i.end();
    f
}

/// op 37 (env.set): append `[klen u32][vlen u32][key][val]` to the overlay
/// log page. The log is append-only; op 26 scans it last-write-wins, so a
/// re-set key needs no in-place edit. A full page takes the defined
/// refusal — never a silent drop.
fn shim_env_set(park: u64, g_ovl: u32) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len; locals: 5=at
    let (a_ptr, a_len, b_ptr, b_len, at) = (1u32, 2u32, 3u32, 4u32, 5u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const((park + OVL) as i32).global_get(g_ovl).i32_add().local_set(at);
    // Room check: entry must fit under the park end.
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.i32_const((park + PARK_SPAN) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(park as i32).i32_const((park + MSG) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(UNSUPPORTED_MSG.len() as i32).i32_store(mem(IOV + 4));
    i.i32_const(2);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i32_const(1).call(1);
    i.unreachable();
    i.end();
    i.local_get(at).local_get(a_len).i32_store(mem(0));
    i.local_get(at).local_get(b_len).i32_store(mem(4));
    i.local_get(at).i32_const(8).i32_add().local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add();
    i.local_get(b_ptr).local_get(b_len).memory_copy(0, 0);
    i.global_get(g_ovl).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.global_set(g_ovl);
    i.i64_const(0).return_();
    i.unreachable();
    i.end();
    f
}

/// op 26 (env.get): scan the overlay log for the LAST entry whose key
/// matches (env.set wins over the process environ), else walk the real
/// environ via `environ_sizes_get`/`environ_get` ("KEY=VALUE\0" strings).
/// Found: value bytes stage at park+DATA (host_read's landing zone),
/// answer `pack(0, len)`. Absent: `pack(2, 0)` — the ok-none tag.
fn shim_env_get(park: u64, g_plen: u32, g_ovl: u32) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len
    // locals: 5=p 6=end 7=klen 8=vlen 9=best 10=j 11=s 12=count
    let (a_ptr, a_len) = (1u32, 2u32);
    let (p, endp, klen, vlen, best, j, s, count) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();

    // ── overlay scan, last match wins ──
    i.i32_const(0).local_set(best);
    i.i32_const((park + OVL) as i32).local_set(p);
    i.i32_const((park + OVL) as i32).global_get(g_ovl).i32_add().local_set(endp);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(endp).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(mem(0)).local_set(klen);
    i.local_get(p).i32_load(mem(4)).local_set(vlen);
    i.local_get(klen).local_get(a_len).i32_eq().if_(BlockType::Empty);
    // byte compare key at p+8 vs a_ptr
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(klen).i32_ge_u().if_(BlockType::Empty);
    i.local_get(p).local_set(best); // full match
    i.br(2);
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(j).i32_add().i32_load8_u(mem8(0));
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(klen).i32_add().local_get(vlen).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(best).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(best).i32_load(mem(4)).local_set(vlen);
    i.i32_const((park + DATA) as i32);
    i.local_get(best).i32_const(8).i32_add().local_get(best).i32_load(mem(0)).i32_add();
    i.local_get(vlen);
    i.memory_copy(0, 0);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();

    // ── real environ fallthrough ──
    i.i32_const((park + NREAD) as i32).i32_const((park + NREAD + 4) as i32).call(5); // environ_sizes_get
    i.if_(BlockType::Empty); // errno → none
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(count);
    // Oversized environ (ptrs + buf past the overlay page's start) → none:
    // the staging area is DATA..OVL.
    i.i32_const((park + DATA) as i32).local_get(count).i32_const(4).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_add();
    i.i32_const((park + OVL) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    i.i32_const((park + DATA) as i32);
    i.i32_const((park + DATA) as i32).local_get(count).i32_const(4).i32_mul().i32_add();
    i.call(6); // environ_get(ptrs, buf)
    i.if_(BlockType::Empty);
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    // walk entries: match "KEY=" prefix.
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(count).i32_ge_u().br_if(1);
    i.i32_const((park + DATA) as i32).local_get(p).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).local_set(s);
    // key bytes equal AND s[a_len] == '='
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(a_len).i32_ge_u().if_(BlockType::Empty);
    i.local_get(s).local_get(a_len).i32_add().i32_load8_u(mem8(0)).i32_const(61).i32_ne().br_if(2);
    // value = s+a_len+1 .. NUL
    i.local_get(s).local_get(a_len).i32_add().i32_const(1).i32_add().local_set(s);
    i.i32_const(0).local_set(vlen);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(s).local_get(vlen).i32_add().i32_load8_u(mem8(0)).i32_eqz().br_if(1);
    i.local_get(vlen).i32_const(1).i32_add().local_set(vlen);
    i.br(0).end().end();
    i.i32_const((park + DATA) as i32).local_get(s).local_get(vlen).memory_copy(0, 0);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();
    i.local_get(s).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(0).end().end();
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.unreachable();
    i.end();
    f
}

/// op 29 (args): every argv entry from `args_sizes_get`/`args_get`
/// (argv0 included, matching the embedded host's non-empty-argv0
/// contract), re-framed as `[len u32][bytes]` per entry — the almide
/// frames encoding — staged at park+DATA. Answer `pack(0, total)`.
fn shim_args(park: u64, g_plen: u32) -> Function {
    // params 0..4 unused beyond the ABI; locals: 5=argc 6=i 7=s 8=n 9=out
    // 10=frames_base
    let (argc, idx, s, n, out, frames_base) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const((park + NREAD) as i32).i32_const((park + NREAD + 4) as i32).call(7); // args_sizes_get
    i.if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(argc);
    // Staging: raw ptrs+buf at DATA, frames rebuilt behind them. The raw
    // area is argc*4 + bufsize; frames need at most bufsize + 4*argc more.
    // Both must sit under the overlay page.
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(8).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_const(2).i32_mul().i32_add();
    i.i32_const((park + OVL) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const((park + DATA) as i32);
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(4).i32_mul().i32_add();
    i.call(8); // args_get(ptrs, buf)
    i.if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    // Build frames after the raw area: out = DATA + argc*4 + bufsize.
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(4).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_add();
    i.local_tee(out).local_set(frames_base);
    i.i32_const(0).local_set(idx);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(idx).local_get(argc).i32_ge_u().br_if(1);
    i.i32_const((park + DATA) as i32).local_get(idx).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).local_set(s);
    // n = strlen(s)
    i.i32_const(0).local_set(n);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(s).local_get(n).i32_add().i32_load8_u(mem8(0)).i32_eqz().br_if(1);
    i.local_get(n).i32_const(1).i32_add().local_set(n);
    i.br(0).end().end();
    i.local_get(out).local_get(n).i32_store(mem(0));
    i.local_get(out).i32_const(4).i32_add().local_get(s).local_get(n).memory_copy(0, 0);
    i.local_get(out).i32_const(4).i32_add().local_get(n).i32_add().local_set(out);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0).end().end();
    // Slide the frames down to DATA (memmove semantics) and answer.
    i.local_get(out).local_get(frames_base).i32_sub().local_set(n); // total
    i.i32_const((park + DATA) as i32).local_get(frames_base).local_get(n).memory_copy(0, 0);
    i.local_get(n).global_set(g_plen);
    i.local_get(n).i64_extend_i32_u().return_();
    i.unreachable();
    i.end();
    f
}
