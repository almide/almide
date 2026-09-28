//! The p1 fs service (#2742): the `fs.*` host ops (and `env.os` /
//! `env.temp_dir` / `env.cwd`, which answer from the same environment) over
//! WASI preview 1, for a stock-runtime artifact.
//!
//! Before this, the p1 shim served none of them, so the build path's op
//! audit rerouted every fs program to the incumbent emitter — 25 programs of
//! the #2739 census. The service is written as WAT (`fs_service.wat`) and
//! SPLICED into the artifact by `to_wasi`:
//!
//!   - it ships only when the emitted op set reaches one of [`FS_SERVICE_OPS`]
//!     (the #1841 gate: a hello-world artifact keeps its five imports), and
//!     only the functions and imports reachable from the ops present;
//!   - its `wasi_snapshot_preview1` imports reuse the artifact's own where the
//!     name matches and append the rest;
//!   - it answers through the same `g_ppos` / `g_plen` staging `host_read`
//!     copies from, and takes the guest's free heap as its per-call arena.
//!
//! The reference is the embedded host's `fs_dispatch` (almide-wasm-run),
//! which answers what native `std::fs` answers; the error texts come from
//! `almide_base::fs_errno`, the table every leg spells from.

use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
use wasm_encoder::{CodeSection, ConstExpr, DataSection, GlobalSection, ValType};
use wasmparser::{Parser, Payload};

/// Every op the fs service answers: (op, the WAT function serving it, the op
/// whose call name its errors carry). The fan prefetch pair's await (41) is
/// `fs.read_text` under its own number (the host's `fs_dispatch(1, ..)`).
pub const FS_SERVICE_OPS: &[(i32, &str, i32)] = &[
    (1, "op_read_text", 1),
    (2, "op_write", 2),
    (3, "op_write_bytes", 3),
    (4, "op_exists", 4),
    (5, "op_is_dir", 5),
    (6, "op_is_file", 6),
    (7, "op_mkdir_p", 7),
    (8, "op_remove", 8),
    (9, "op_remove_all", 9),
    (10, "op_create_temp_dir", 10),
    (11, "op_list_dir", 11),
    (12, "op_read_lines", 12),
    (13, "op_read_text_if_exists", 13),
    (14, "op_read_bytes", 14),
    (15, "op_write", 15),
    (16, "op_append", 16),
    (17, "op_file_size", 17),
    (18, "op_modified_at", 18),
    (19, "op_copy", 19),
    (20, "op_rename", 20),
    (21, "op_create_temp_file", 21),
    (22, "op_is_symlink", 22),
    (23, "op_walk", 23),
    (24, "op_read_lines_if_exists", 24),
    (25, "op_read_bytes_if_exists", 25),
    (27, "op_os", 27),
    (28, "op_temp_dir", 28),
    (33, "op_cwd", 33),
    (38, "op_stat", 38),
    (39, "op_glob", 39),
    (40, "op_nop", 40),
    (41, "op_read_text", 1),
    (42, "op_nop", 42),
    (51, "op_read_lines", 51),
    (52, "op_read_lines", 52),
    // 61: fold_lines_range (and fold_lines_chunked's worker read) — op 1's
    // body under the range's name; the guest walks the text (#2744).
    // 62: fold_lines_chunked's size probe — op 17's body under its name.
    (61, "op_read_text", 61),
    (62, "op_file_size", 62),
    // 63/64: the Bytes-typed readers, ops 14/25's bodies under the writer's
    // call name (#2890 — sharing 14/25 said `fs.read_bytes` where native
    // says `fs.read_bytes_raw`).
    (63, "op_read_bytes", 63),
    (64, "op_read_bytes_if_exists", 64),
];

/// Whether the fs service answers `op`.
pub fn serves(op: i32) -> bool {
    FS_SERVICE_OPS.iter().any(|(o, _, _)| *o == op)
}

/// The Almide call an fs op came from, so an error names what the WRITER
/// wrote rather than the host primitive that served it (#2090). The embedded
/// host spells its messages from this table too.
const FS_OP_NAMES: &[(i32, &str)] = &[
    (1, "fs.read_text"),
    (2, "fs.write"),
    (3, "fs.write_bytes"),
    (7, "fs.mkdir_p"),
    (8, "fs.remove"),
    (9, "fs.remove_all"),
    (10, "fs.create_temp_dir"),
    (11, "fs.list_dir"),
    (12, "fs.read_lines"),
    (13, "fs.read_text_if_exists"),
    (14, "fs.read_bytes"),
    (15, "fs.write_bytes_raw"),
    (16, "fs.append"),
    (17, "fs.file_size"),
    (18, "fs.modified_at"),
    (19, "fs.copy"),
    (20, "fs.rename"),
    (21, "fs.create_temp_file"),
    (23, "fs.walk"),
    (24, "fs.read_lines_if_exists"),
    (25, "fs.read_bytes_if_exists"),
    (38, "fs.stat"),
    (39, "fs.glob"),
    (51, "fs.fold_lines"),
    (52, "fs.for_each_line"),
    (61, "fs.fold_lines_range"),
    (62, "fs.fold_lines_chunked"),
    (63, "fs.read_bytes_raw"),
    (64, "fs.read_bytes_raw_if_exists"),
];

/// The call name of an fs op (`"fs"` for one without its own row).
pub fn fs_op_name(op: i32) -> &'static str {
    FS_OP_NAMES.iter().find(|(o, _)| *o == op).map_or("fs", |(_, n)| n)
}

const TEMPLATE: &str = include_str!("fs_service.wat");
/// The statics' offset within the fs page (the layout in fs_service.wat).
const STATICS: u32 = 1024;
const STATICS_END: u32 = 8192;

/// The WAT of the service for the ops in `ops`, its page at `fsp`.
fn service_wat(fsp: u32, ops: &[i32]) -> String {
    let mut wat = TEMPLATE.replace("@FSP@", &fsp.to_string());
    let mut blob: Vec<u8> = Vec::new();
    let mut place = |bytes: &[u8]| -> (u32, u32) {
        let at = fsp + STATICS + blob.len() as u32;
        blob.extend_from_slice(bytes);
        (at, bytes.len() as u32)
    };
    let fixed: &[(&str, &[u8])] = &[
        ("oom", crate::OOM_MSG),
        ("errno_fallback", b"filesystem operation failed (wasi errno "),
        ("utf8", almide_base::fs_errno::INVALID_UTF8_TEXT.as_bytes()),
        ("write_zero", almide_base::fs_errno::WRITE_ZERO_TEXT.as_bytes()),
        ("almide_cwd", b"ALMIDE_CWD"),
        ("pwd", b"PWD"),
        ("tmpdir", b"TMPDIR"),
        ("slash_tmp", b"/tmp"),
        ("dot", b"."),
        ("slash", b"/"),
        ("wasi", b"wasi"),
    ];
    let mut tail = String::new();
    for (name, bytes) in fixed {
        let (at, len) = place(bytes);
        tail.push_str(&format!("  (func $s_{name} (result i32 i32) (i32.const {at}) (i32.const {len}))\n"));
    }
    tail.push_str("  (func $errno_text (param $e i32) (result i32 i32)\n");
    for row in almide_base::fs_errno::FS_ERRNOS {
        let (at, len) = place(row.text.as_bytes());
        tail.push_str(&format!(
            "    (if (i32.eq (local.get $e) (i32.const {})) (then (return (i32.const {at}) (i32.const {len}))))\n",
            row.wasi
        ));
    }
    tail.push_str("    (i32.const 0) (i32.const 0))\n");
    tail.push_str("  (func $op_name (param $op i32) (result i32 i32)\n");
    let mut named: Vec<i32> = FS_SERVICE_OPS.iter().map(|(_, _, n)| *n).collect();
    named.sort_unstable();
    named.dedup();
    for op in named.into_iter().filter(|op| fs_op_name(*op) != "fs") {
        let (at, len) = place(fs_op_name(op).as_bytes());
        tail.push_str(&format!(
            "    (if (i32.eq (local.get $op) (i32.const {op})) (then (return (i32.const {at}) (i32.const {len}))))\n"
        ));
    }
    let (at, len) = place(b"fs");
    tail.push_str(&format!("    (i32.const {at}) (i32.const {len}))\n"));
    assert!(STATICS + blob.len() as u32 <= STATICS_END, "fs service statics overflow their span");

    // The dispatcher: one arm per op the module reaches, so everything else
    // in the template stays unreachable and is not spliced.
    tail.push_str(
        "  (func $fs (export \"fs\") (param $op i32) (param $a i32) (param $al i32) (param $b i32) (param $bl i32) (result i64)\n    (call $prologue)\n",
    );
    for (op, func, name_op) in FS_SERVICE_OPS.iter().filter(|(o, _, _)| ops.contains(o)) {
        tail.push_str(&format!(
            "    (if (i32.eq (local.get $op) (i32.const {op})) (then (return (call ${func} (i32.const {name_op}) (local.get $a) (local.get $al) (local.get $b) (local.get $bl)))))\n"
        ));
    }
    tail.push_str("    (unreachable))\n");
    let escaped: String = blob.iter().map(|b| format!("\\{b:02x}")).collect();
    tail.push_str(&format!("  (data (i32.const {}) \"{escaped}\")\n)\n", fsp + STATICS));
    wat.push_str(&tail);
    wat
}


/// The bytes the fs service's page takes past the park.
pub const FS_PAGE: u64 = 65536;

/// The WASI imports of the artifact the service can share by name: the base
/// five, at their fixed indices.
const BASE_IMPORTS: [&str; 5] = ["fd_write", "proc_exit", "random_get", "clock_time_get", "fd_read"];

/// The parsed service module, before any splice decision.
#[derive(Default)]
struct Parsed {
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
    /// (name, type index) of each function import, in index order.
    imports: Vec<(String, u32)>,
    /// The `shim.*` global imports by name, in index order.
    global_imports: Vec<String>,
    globals: Vec<(wasm_encoder::GlobalType, ConstExpr)>,
    func_types: Vec<u32>,
    /// The direct callees of each defined function.
    calls: Vec<Vec<u32>>,
    entry: Option<u32>,
}

fn conv(ts: &[wasmparser::ValType]) -> Vec<ValType> {
    ts.iter().map(|t| RoundtripReencoder.val_type(*t).expect("valtype")).collect()
}

fn reenc<T, E: std::fmt::Debug>(r: Result<T, E>) -> anyhow::Result<T> {
    r.map_err(|e| anyhow::anyhow!("fs service reencode: {e:?}"))
}

impl Parsed {
    fn read(bytes: &[u8]) -> anyhow::Result<Self> {
        let mut m = Parsed::default();
        for payload in Parser::new(0).parse_all(bytes) {
            m.take(payload?)?;
        }
        Ok(m)
    }

    fn take(&mut self, payload: Payload<'_>) -> anyhow::Result<()> {
        match payload {
            Payload::TypeSection(r) => {
                for ty in r.into_iter_err_on_gc_types() {
                    let f = ty?;
                    self.types.push((conv(f.params()), conv(f.results())));
                }
            }
            Payload::ImportSection(r) => {
                for imp in r.into_imports() {
                    self.import(imp?);
                }
            }
            Payload::FunctionSection(r) => self.func_types = r.into_iter().collect::<Result<_, _>>()?,
            Payload::GlobalSection(r) => {
                for g in r {
                    let g = g?;
                    self.globals.push((reenc(RoundtripReencoder.global_type(g.ty))?, reenc(RoundtripReencoder.const_expr(g.init_expr))?));
                }
            }
            Payload::ExportSection(r) => {
                self.entry = r.into_iter().filter_map(Result::ok).find(|e| e.name == "fs").map(|e| e.index);
            }
            Payload::CodeSectionEntry(body) => self.calls.push(callees(&body)?),
            _ => {}
        }
        Ok(())
    }

    fn import(&mut self, imp: wasmparser::Import<'_>) {
        match imp.ty {
            wasmparser::TypeRef::Func(t) => self.imports.push((imp.name.to_string(), t)),
            wasmparser::TypeRef::Global(_) => self.global_imports.push(imp.name.to_string()),
            _ => {}
        }
    }

    /// Every function index reachable from `entry` over direct calls (the
    /// service has no tables, so a call is the only edge).
    fn reachable(&self, entry: u32) -> Vec<bool> {
        let n_imp = self.imports.len() as u32;
        let mut reached = vec![false; self.imports.len() + self.func_types.len()];
        let mut stack = vec![entry];
        while let Some(f) = stack.pop() {
            if std::mem::replace(&mut reached[f as usize], true) || f < n_imp {
                continue;
            }
            stack.extend(self.calls[(f - n_imp) as usize].iter().copied());
        }
        reached
    }
}

fn callees(body: &wasmparser::FunctionBody<'_>) -> anyhow::Result<Vec<u32>> {
    let mut out = Vec::new();
    for op in body.get_operators_reader()? {
        if let wasmparser::Operator::Call { function_index } = op? {
            out.push(function_index);
        }
    }
    Ok(out)
}

/// The type indices a splice needs, in the artifact's type list.
pub struct FsTypes {
    /// Every service type, by its module index (block types reference them).
    map: Vec<u32>,
}

/// Where the splice lands in the artifact: the index each service import,
/// `shim.*` global and own global maps to, and the first shipped function.
pub struct SpliceTargets {
    pub import_at: Vec<u32>,
    pub g_plen: u32,
    pub g_ppos: u32,
    pub heap: u32,
    pub first_global: u32,
    pub first_func: u32,
}

/// The fs service for one artifact: parsed for the ops it reaches, with the
/// reachable functions and WASI imports decided.
pub struct FsSplice {
    m: Parsed,
    bytes: Vec<u8>,
    ops: Vec<i32>,
    /// Per defined function: whether it ships.
    shipped: Vec<bool>,
    /// The reachable imports the artifact does not already have, by index
    /// into the service's imports.
    fresh: Vec<usize>,
    entry: u32,
}

impl FsSplice {
    /// The service for `host_ops` with its page at `fs_page`, or `None` when
    /// the op set reaches no fs op. `environ_shipped`: the env.get service
    /// already imports the environ pair.
    pub fn plan(host_ops: &[i32], fs_page: u32, environ_shipped: bool) -> anyhow::Result<Option<Self>> {
        let ops: Vec<i32> = host_ops.iter().copied().filter(|op| serves(*op)).collect();
        if ops.is_empty() {
            return Ok(None);
        }
        let bytes = wat::parse_str(service_wat(fs_page, &ops))?;
        let m = Parsed::read(&bytes)?;
        let n_imp = m.imports.len();
        let entry = m.entry.ok_or_else(|| anyhow::anyhow!("fs service: no `fs` export"))?;
        let reached = m.reachable(entry);
        let shared = |name: &str| {
            BASE_IMPORTS.contains(&name) || (environ_shipped && matches!(name, "environ_sizes_get" | "environ_get"))
        };
        let fresh = (0..n_imp).filter(|i| reached[*i] && !shared(&m.imports[*i].0)).collect();
        Ok(Some(Self {
            shipped: reached[n_imp..].to_vec(),
            entry: entry - n_imp as u32,
            m,
            bytes,
            ops,
            fresh,
        }))
    }

    /// The WASI imports the splice appends.
    pub fn fresh_imports(&self) -> u32 {
        self.fresh.len() as u32
    }

    /// Register every service type in the artifact's type list.
    pub fn register_types(&self, types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> FsTypes {
        FsTypes { map: self.m.types.iter().map(|(p, r)| crate::type_index(types, p, r)).collect() }
    }

    /// Append the fresh imports (from `next_import` on) and answer where
    /// every service import lands. `environ`: the env.get service's pair.
    pub fn import(
        &self,
        imports: &mut wasm_encoder::ImportSection,
        types: &FsTypes,
        next_import: &mut u32,
        environ: Option<(u32, u32)>,
    ) -> Vec<u32> {
        let mut at: Vec<u32> = self
            .m
            .imports
            .iter()
            .map(|(name, _)| match (name.as_str(), environ) {
                ("environ_sizes_get", Some((sizes, _))) => sizes,
                ("environ_get", Some((_, get))) => get,
                (name, _) => BASE_IMPORTS.iter().position(|b| *b == name).unwrap_or(0) as u32,
            })
            .collect();
        for &i in &self.fresh {
            let (name, ty) = &self.m.imports[i];
            imports.import("wasi_snapshot_preview1", name, wasm_encoder::EntityType::Function(types.map[*ty as usize]));
            at[i] = *next_import;
            *next_import += 1;
        }
        at
    }

    /// Declare the shipped functions from index `first` on; the dispatcher's
    /// index among them.
    pub fn declare(&self, functions: &mut wasm_encoder::FunctionSection, types: &FsTypes, first: u32) -> u32 {
        for (t, _) in self.m.func_types.iter().zip(&self.shipped).filter(|(_, s)| **s) {
            functions.function(types.map[*t as usize]);
        }
        first + self.shipped[..self.entry as usize].iter().filter(|s| **s).count() as u32
    }

    /// The `fs_call` forwarding rows: every fs op present, to the dispatcher.
    pub fn forward(&self, dispatcher: u32) -> Vec<(i32, u32)> {
        self.ops.iter().map(|op| (*op, dispatcher)).collect()
    }

    /// Append the service's own globals.
    pub fn emit_globals(&self, globals: &mut GlobalSection) {
        for (ty, init) in &self.m.globals {
            globals.global(*ty, init);
        }
    }

    /// Append the shipped bodies to `code` and the statics to `data`.
    pub fn emit(&self, code: &mut CodeSection, data: &mut DataSection, types: &FsTypes, to: &SpliceTargets) -> anyhow::Result<()> {
        let mut slot = to.first_func;
        let funcs: Vec<u32> = self
            .shipped
            .iter()
            .map(|s| {
                slot += u32::from(*s);
                slot - u32::from(*s)
            })
            .collect();
        let shim = |name: &str| match name {
            "plen" => to.g_plen,
            "ppos" => to.g_ppos,
            _ => to.heap,
        };
        let globals: Vec<u32> = self
            .m
            .global_imports
            .iter()
            .map(|n| shim(n))
            .chain((0..self.m.globals.len() as u32).map(|i| to.first_global + i))
            .collect();
        let mut remap = SpliceRemap { n_imp: self.m.imports.len() as u32, import_at: &to.import_at, funcs: &funcs, globals: &globals, types: &types.map };
        let mut shipped = self.shipped.iter();
        for payload in Parser::new(0).parse_all(&self.bytes) {
            match payload? {
                Payload::CodeSectionEntry(body) if *shipped.next().expect("one flag per body") => {
                    reenc(remap.parse_function_body(code, body))?;
                }
                Payload::DataSection(r) => {
                    for d in r {
                        let d = d?;
                        if let wasmparser::DataKind::Active { offset_expr, .. } = d.kind {
                            data.active(0, &reenc(RoundtripReencoder.const_expr(offset_expr))?, d.data.iter().copied());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

struct SpliceRemap<'a> {
    n_imp: u32,
    import_at: &'a [u32],
    funcs: &'a [u32],
    globals: &'a [u32],
    types: &'a [u32],
}

impl Reencode for SpliceRemap<'_> {
    type Error = std::convert::Infallible;
    fn function_index(&mut self, func: u32) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(if func < self.n_imp { self.import_at[func as usize] } else { self.funcs[(func - self.n_imp) as usize] })
    }
    fn global_index(&mut self, global: u32) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(self.globals[global as usize])
    }
    fn type_index(&mut self, ty: u32) -> Result<u32, wasm_encoder::reencode::Error<Self::Error>> {
        Ok(self.types[ty as usize])
    }
}
