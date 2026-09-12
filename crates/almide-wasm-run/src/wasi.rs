//! `to_wasi` (#1588): rewrite an emitted almide module into a
//! self-sufficient WASI preview-1 command that runs on STOCK runtimes
//! (`wasmtime run mod.wasm`, wasmer, browsers with a p1 shim) — no
//! bespoke host.
//!
//! The transform is a POST-PASS, so the emitter and its verified
//! envelope stay untouched:
//!   - the 5 `almide.*` imports are replaced by 5 WASI imports
//!     (fd_write / proc_exit / random_get / clock_time_get / fd_read),
//!     plus the environ/args quartet ONLY when the module's emitted op
//!     set reaches it (below); every non-import index shifts by the
//!     import delta, and the element section re-encodes through the
//!     same Remap (#1716);
//!   - every call to an old import retargets to one of 5 appended SHIM
//!     functions implementing the almide host contract over WASI;
//!   - one PARK span is appended to linear memory for iovecs, the
//!     stdin/entropy buffer (grown on demand), the unsupported-op
//!     message and the env overlay log; globals carry the park length
//!     and capacity (and the overlay length, when an env service ships).
//!
//! Supported host surface (the non-host-variant corpus): console
//! output (println/eprintln/io.print/io.write), exit codes, stdin
//! read-to-end, entropy, the wall clock. fs/process ops take the
//! DEFINED refusal: a named message on stderr + exit 1 — never a
//! silent wrong answer (the target-availability doctrine, #1423).
//!
//! The env/args SERVICES are reachability-gated (#1841, the #1712
//! discipline applied to the transform): `env.get` (op 26) ships the
//! environ pair of imports + its scan shim, `env.set` (op 37) its
//! overlay-append shim, `env.args`/`process.args` (op 29) the args
//! pair of imports + its frames shim — each only when the emitted op
//! set names the op. A hello-world artifact carries none of them
//! (five imports, five shims), and an op that never reached the module
//! cannot be called, so the gate is a selection over the op table the
//! build path already audits against `P1_SERVED_OPS`, not an analysis.

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
pub const P1_SERVED_OPS: &[i32] = &[26, 29, 30, 32, 34, 35, 36, 37];

pub(crate) const UNSUPPORTED_MSG: &[u8] = b"Error: host op unsupported in the WASI build\n";
/// The env.set overlay log's own refusal. It used to borrow the line above,
/// which names an operation the build supports and had just performed — the
/// #2103 defect (an error that does not say what failed) in the shim.
pub(crate) const ENV_FULL_MSG: &[u8] = b"Error: env.set log full (64 KiB of names and values)\n";
/// C-197's line, for the shim-side stagings that ask the machine for pages
/// (#2120). The guest allocator prints the same words from its own path.
pub(crate) const OOM_MSG: &[u8] = b"Error: out of memory\n";
// Park-page layout (offsets from park base).
pub(crate) const IOV: u64 = 0; // two iovec entries (16 bytes)
pub(crate) const NREAD: u64 = 16;
pub(crate) const NL: u64 = 24;
pub(crate) const MSG: u64 = 64;
/// The second message slot, clear of MSG's text and below DATA.
pub(crate) const MSG2: u64 = 256;
/// The third: the shim-side out-of-memory line.
pub(crate) const MSG3: u64 = 384;
pub(crate) const DATA: u64 = 1024; // stdin/entropy bytes + op result staging
/// The env.set overlay log (#1716): [klen u32][vlen u32][key][val] entries,
/// append-only, scanned last-write-wins by op 26. Its page sits above the
/// staging span the other ops use.
pub(crate) const OVL: u64 = 4 * 65536;
/// The staging room the emitter refuses to overrun (#2118) and this layout
/// provides: one number, checked here rather than trusted.
const _: () = assert!((OVL - DATA) as i64 == almide_wasm::WASI_STAGING_ROOM);
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

/// The optional p1 services (#1841), selected from the module's emitted
/// op set — one flag per service, each with its own imports and shim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct P1Services {
    /// op 26 (`env.get`): environ_sizes_get + environ_get, the scan shim.
    pub env_get: bool,
    /// op 37 (`env.set`): the overlay-append shim (no import).
    pub env_set: bool,
    /// op 29 (`env.args` / `process.args`): args_sizes_get + args_get,
    /// the frames shim.
    pub args: bool,
}

impl P1Services {
    /// Which services `host_ops` (the emitter's op set) reaches.
    pub fn from_ops(host_ops: &[i32]) -> Self {
        Self {
            env_get: host_ops.contains(&26),
            env_set: host_ops.contains(&37),
            args: host_ops.contains(&29),
        }
    }

    /// The WASI imports this selection adds past the base five.
    pub fn extra_imports(self) -> u32 {
        2 * u32::from(self.env_get) + 2 * u32::from(self.args)
    }
}

/// Rewrite an emitted almide module into a stock-runtime p1 command.
/// `host_ops` is the emitter's op set for the module (the second half of
/// `almide_wasm::emit_program_with_ops`): the env/args services ship only
/// for the ops it names (#1841).
pub fn to_wasi(bytes: &[u8], host_ops: &[i32]) -> anyhow::Result<Vec<u8>> {
    let services = P1Services::from_ops(host_ops);
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
    // The base five WASI imports replace the five almide.* ones; the
    // environ/args pairs (#1716) are appended only for the services the
    // op set reaches (#1841), so every non-import index shifts by the
    // number of pairs shipped (0, 2 or 4).
    let imports_count: u32 = 5 + services.extra_imports();
    let shift: u32 = imports_count - 5;
    let shim_base = imports_count + func_types.len() as u32;
    // The park CANNOT live past the current memory end — the bump heap
    // grows there. It takes over the ORIGINAL heap base instead, and
    // the heap's initial pointer moves up by the span: nothing else
    // reads that global's init, and the rc heap-floor guards only ever
    // see block handles.
    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    let g_plen = global_count;
    // g_ppos exists only for the services that can stage outside the park
    // (#2120); a module without them keeps the fixed source and its bytes.
    let g_ppos = (services.env_get || services.args).then_some(global_count + 1);
    // g_ovl (the overlay log length) exists only when an env service
    // ships — nothing else reads or writes the log.
    let g_ovl = (services.env_get || services.env_set)
        .then_some(global_count + 1 + u32::from(g_ppos.is_some()));
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
    // The environ/args pairs (#1716) — all share the (ptr, ptr) -> errno
    // shape. Appended AFTER the base five so the shim bodies' literal
    // import indices 0..4 stay put; each pair is present only when its
    // service ships (#1841), and its shim takes the indices it landed on.
    let mut next_import = 5u32;
    let environ_imports = services.env_get.then(|| {
        imports.import(W, "environ_sizes_get", EntityType::Function(t_random));
        imports.import(W, "environ_get", EntityType::Function(t_random));
        next_import += 2;
        (next_import - 2, next_import - 1)
    });
    let args_imports = services.args.then(|| {
        imports.import(W, "args_sizes_get", EntityType::Function(t_random));
        imports.import(W, "args_get", EntityType::Function(t_random));
        next_import += 2;
        (next_import - 2, next_import - 1)
    });
    debug_assert_eq!(next_import, imports_count);

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    for ti in [t_print, t_print, t_exit, t_fs, t_read] {
        functions.function(ti);
    }
    // The optional service shims, in order behind the base five: their
    // function indices are handed to shim_fs_call's forwarding arms.
    let mut next_shim = shim_base + 5;
    let mut service_slot = |present: bool| {
        present.then(|| {
            functions.function(t_fs);
            next_shim += 1;
            next_shim - 1
        })
    };
    let f_env_get = service_slot(services.env_get);
    let f_env_set = service_slot(services.env_set);
    let f_args = service_slot(services.args);

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
    // g_ppos: where host_read copies FROM (#2120). The staging page is the
    // default; a service whose result outgrows it stages above the heap and
    // points this at those bytes instead of answering a wrong value.
    if g_ppos.is_some() {
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const((park + DATA) as i32),
        );
    }
    // g_ovl: bytes appended to the env overlay log so far.
    if g_ovl.is_some() {
        globals.global(
            GlobalType { val_type: ValType::I32, mutable: true, shared: false },
            &ConstExpr::i32_const(0),
        );
    }

    let mut exports = ExportSection::new();
    for (name, kind, idx) in &export_rows {
        let idx = if *kind == ExportKind::Func { *idx + shift } else { *idx };
        exports.export(name, *kind, idx);
    }
    exports.export("_start", ExportKind::Func, main_index + shift);

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, 1)?);
    }
    // Shims (their own calls target the NEW imports — no remap). Order:
    // println, eprintln, exit, fs_call, host_read, then whichever of
    // env_get, env_set, args the op set reached.
    code.function(&shim_print(1, park));
    code.function(&shim_print(2, park));
    code.function(&shim_exit());
    // #1962: a module whose emitted op set is EMPTY never calls `fs_call`
    // (and `host_read` only copies an op's result out), so both shims ship
    // as index-stable `unreachable` stubs — the fs_call dispatcher alone is
    // ~460 B, a quarter of a hello-world artifact. The op set is the same
    // audited one the build path routes on, so a stub is never reached.
    if host_ops.is_empty() {
        let mut stub = Function::new([]);
        stub.instructions().unreachable().end();
        code.function(&stub);
        code.function(&stub);
    } else {
        code.function(&shim_fs_call(park, g_plen, g_ppos, f_env_get, f_env_set, f_args));
        code.function(&shim_host_read(park, g_plen, g_ppos));
    }
    if f_env_get.is_some() {
        let (i_sizes, i_get) = environ_imports.expect("env_get service imports its pair");
        code.function(&shim_env_get(park, g_plen, g_ppos.expect("env.get stages"), g_ovl.expect("env service global"), i_sizes, i_get));
    }
    if f_env_set.is_some() {
        code.function(&shim_env_set(park, g_ovl.expect("env service global")));
    }
    if f_args.is_some() {
        let (i_sizes, i_get) = args_imports.expect("args service imports its pair");
        code.function(&shim_args(park, g_plen, g_ppos.expect("args stages"), i_sizes, i_get));
    }

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
    if f_env_set.is_some() {
        data.active(0, &ConstExpr::i32_const((park + MSG2) as i32), ENV_FULL_MSG.iter().copied());
    }
    // The high-staging services are the only shim-side callers of memory.grow.
    if f_env_get.is_some() || f_args.is_some() {
        data.active(0, &ConstExpr::i32_const((park + MSG3) as i32), OOM_MSG.iter().copied());
    }

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

// The p1 shims (print / exit / fs_call / host_read / env / args): wasi_shims.rs.
include!("wasi_shims.rs");
