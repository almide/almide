//! `almide build app.almd --target wasm --host js` (#2265): the JS host the
//! module needs to run in a page or under node, written by the compiler
//! itself next to the `.wasm` — `<mod>.js` (a dependency-free ES module) and
//! `<mod>.d.ts` — so the artifact is page-runnable without a hand-written
//! WASI stub, import object or String marshalling.
//!
//! What the host does:
//! - the `wasi_snapshot_preview1` imports the SHIPPED module names get a
//!   shim (`fd_write` → stdout/stderr, `proc_exit` → an [`AlmideExit`]
//!   throw, the clock/random/read floor); nothing else is linked, so nothing
//!   else is emitted (#2276) — the host is derived from the bytes after
//!   `--wasm-opt`, never from the pre-opt module;
//! - every `@extern(wasm, "js", "name")` import is wired to
//!   `init(source, { js: { name } })`, with `String` args decoded from the
//!   block header before the user function runs and its return encoded;
//! - every `pub fn` becomes a wrapper marshalling `Int` ↔ `number` (range
//!   checked at ±2^53), `Float`, `Bool`, `String`, `Unit`, and the block
//!   shapes `Bytes`, `List[T]`, `Option[T]` and records (#3354, planned in
//!   `js_host_exports.rs` from the layout the emitter records); any other
//!   type is a compile-time refusal naming the function and the type;
//! - an `effect fn` (or a declared `Result[T, String]`) returns a Result
//!   block: its wrapper unwraps ok into `T` and throws `AlmideError` with the
//!   err message (#3352). The wrapper follows the return ABI the emitter
//!   records per export (`host_exports::export_rets`), and a record that
//!   disagrees with the source type is a refusal, never a misread;
//! - `main` is exposed as `run()`; `_start` is not called by `init`.
//!
//! The marshalling reads the module's OWN signatures (the wasm type section)
//! for the valtype of each slot, and the block layout `almide-layout` owns
//! (rc @0, len @4, cap @8, payload @12). `String` blocks the
//! host builds go through the module's exported allocator, and blocks the
//! host takes out are released through its exported release, so the
//! guest's free lists see every block they own. Those two exports and the
//! glue's string helpers ship only when some signature carries a `String`
//! (#2276): a scalar-only surface leaves the `.wasm` byte-identical to a
//! build without `--host js`.

use std::collections::BTreeMap;
use almide_ir::IrProgram;
use almide_lang::types::Ty;
use almide_wasm::host_exports::{AbiShape, ExportRet};

#[path = "js_host_exports.rs"]
mod exports;
#[path = "js_host_async.rs"]
mod jspi;
#[path = "js_host_imports.rs"]
mod imports;

/// One function on the host boundary: an exported `pub fn` or an extern.
#[derive(Debug, Clone)]
pub(crate) struct HostFn {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    /// An `effect fn`: its wasm value is a `Result` block whatever `ret`
    /// says (#3352).
    pub is_effect: bool,
}

/// One `@extern(wasm, module, name)` import.
#[derive(Debug, Clone)]
pub(crate) struct HostExtern {
    pub module: String,
    pub import: String,
    pub sig: HostFn,
}

/// The host-visible surface of a program, read from the IR before routing.
#[derive(Debug, Clone, Default)]
pub(crate) struct HostSurface {
    pub exports: Vec<HostFn>,
    pub externs: Vec<HostExtern>,
    pub has_main: bool,
    /// The extern import names whose hook returns a Promise: an
    /// `@extern(wasm, "js", name, returns: promise)` (#3371; #3353 took
    /// them from a build flag).
    pub async_imports: Vec<String>,
}

impl HostSurface {
    /// Does any marshalled signature carry a `String` (#2276)? Only then does
    /// the host build or take a block, so only then do the module's
    /// allocator/release exports and the glue's string helpers ship.
    pub(crate) fn needs_string_abi(&self) -> bool {
        // A fallible import (#3356) answers with a Result block, and its err
        // carries a String.
        let externs_string = self.externs.iter().any(|e| {
            returns_result(&e.sig) || e.sig.params.iter().map(|(_, t)| t).chain(std::iter::once(&e.sig.ret)).any(|t| matches!(t, Ty::String))
        });
        // An export builds or takes a block for anything but a scalar: a
        // String, Bytes, List, Option or record (#3354), or an unwrapped
        // Result (#3352), whose err message is a String.
        let exports_block = self.exports.iter().any(|f| {
            returns_result(f) || !exports::scalar_only(visible_ret(f)) || f.params.iter().any(|(_, t)| !exports::scalar_only(t))
        });
        externs_string || exports_block
    }

    pub(crate) fn of(program: &IrProgram) -> Self {
        let mut s = HostSurface::default();
        for f in &program.functions {
            let name = f.name.as_str();
            let sig = || HostFn {
                name: name.to_string(),
                params: f.params.iter().map(|p| (p.name.as_str().to_string(), p.ty.clone())).collect(),
                ret: f.ret_ty.clone(),
                is_effect: f.is_effect,
            };
            if let Some(a) = f.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") {
                s.mark_async(a);
                s.externs.push(HostExtern {
                    module: a.module.as_str().to_string(),
                    import: a.function.as_str().to_string(),
                    sig: sig(),
                });
                continue;
            }
            if name == "main" {
                s.has_main = true;
                continue;
            }
            // The same set the structural emitter exports (#457): entry-program
            // pub fns, monomorphic, not tests, not compiler-internal.
            if name.starts_with("__")
                || f.is_test
                || f.generics.as_ref().is_some_and(|g| !g.is_empty())
                || !matches!(f.visibility, almide_ir::IrVisibility::Public)
            {
                continue;
            }
            // `@export(wasm, "sym")` (#2752) renames the module's export,
            // and the wrapper is the host's name for it.
            let mut exported = sig();
            if let Some(a) = f.export_attrs.iter().find(|a| a.target.as_str() == "wasm") {
                exported.name = a.symbol.to_string();
            }
            s.exports.push(exported);
        }
        // An extern declared in another module of the program is the same
        // host binding (#2876): the structural leg imports it wherever it is
        // declared, so the host serves it wherever it is declared. A module
        // fn exports only by declaring `@export(wasm, "sym")` (#3281), the
        // same set the structural emitter exports.
        for f in program.modules.iter().flat_map(|m| &m.functions) {
            if let Some(a) = f.export_attrs.iter().find(|a| a.target.as_str() == "wasm")
                && !f.is_test
                && !f.name.as_str().starts_with("__")
                && !f.generics.as_ref().is_some_and(|g| !g.is_empty())
            {
                s.exports.push(HostFn {
                    name: a.symbol.to_string(),
                    params: f.params.iter().map(|p| (p.name.as_str().to_string(), p.ty.clone())).collect(),
                    ret: f.ret_ty.clone(),
                    is_effect: f.is_effect,
                });
            }
            let Some(a) = f.extern_attrs.iter().find(|a| a.target.as_str() == "wasm") else { continue };
            s.mark_async(a);
            let (module, import) = (a.module.as_str().to_string(), a.function.as_str().to_string());
            if s.externs.iter().any(|e| e.module == module && e.import == import) {
                continue;
            }
            let sig = HostFn {
                name: f.name.as_str().to_string(),
                params: f.params.iter().map(|p| (p.name.as_str().to_string(), p.ty.clone())).collect(),
                ret: f.ret_ty.clone(),
                is_effect: f.is_effect,
            };
            s.externs.push(HostExtern { module, import, sig });
        }
        s
    }

    /// `returns: promise` (#3371): the parser admits it only on
    /// `@extern(wasm, "js", ...)`, so its import is a JS hook to suspend on.
    fn mark_async(&mut self, a: &almide_lang::ast::ExternAttr) {
        let import = a.function.as_str();
        if a.returns_promise && !self.async_imports.iter().any(|n| n == import) {
            self.async_imports.push(import.to_string());
        }
    }
}

/// The marshalled kinds — the five the host converts today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marshal {
    Int,
    Float,
    Bool,
    Str,
    Unit,
}

fn marshal_of(ty: &Ty) -> Option<Marshal> {
    match ty {
        Ty::Int => Some(Marshal::Int),
        Ty::Float => Some(Marshal::Float),
        Ty::Bool => Some(Marshal::Bool),
        Ty::String => Some(Marshal::Str),
        Ty::Unit => Some(Marshal::Unit),
        _ => None,
    }
}

/// `Result[T, String]` → `T`'s type, for a declared-Result return.
fn result_ok_ty(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(c, args) if *c == almide_lang::types::constructor::TypeConstructorId::Result && args.len() == 2 && matches!(args[1], Ty::String) => Some(&args[0]),
        _ => None,
    }
}

/// Does the export's module function return a `Result` block the wrapper
/// unwraps (#3352): an `effect fn` (always), or a declared `Result[T, String]`?
fn returns_result(f: &HostFn) -> bool {
    f.is_effect || result_ok_ty(&f.ret).is_some()
}

/// The JS-visible return type of an export: `T` of an unwrapped Result.
fn visible_ret(f: &HostFn) -> &Ty {
    result_ok_ty(&f.ret).unwrap_or(&f.ret)
}

fn ts_of(m: Marshal) -> &'static str {
    match m {
        Marshal::Int | Marshal::Float => "number",
        Marshal::Bool => "boolean",
        Marshal::Str => "string",
        Marshal::Unit => "void",
    }
}

/// A wasm valtype the boundary can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Val {
    I32,
    I64,
    F64,
}

struct WasmSig {
    params: Vec<Val>,
    results: Vec<Val>,
}

/// Function signatures of the emitted module: its function imports (by
/// module and name) and its function exports (by name).
struct WasmSigs {
    imports: Vec<(String, String, WasmSig)>,
    exports: BTreeMap<String, WasmSig>,
}

fn val_of(v: wasmparser::ValType) -> Result<Val, String> {
    match v {
        wasmparser::ValType::I32 => Ok(Val::I32),
        wasmparser::ValType::I64 => Ok(Val::I64),
        wasmparser::ValType::F64 => Ok(Val::F64),
        other => Err(format!("wasm valtype {other:?} has no host marshalling")),
    }
}

/// The function type a type-section entry declares (a non-function entry
/// keeps the index space aligned with an empty signature).
fn func_sig_of(sub: &wasmparser::SubType) -> Result<WasmSig, String> {
    match &sub.composite_type.inner {
        wasmparser::CompositeInnerType::Func(ft) => Ok(WasmSig {
            params: ft.params().iter().map(|v| val_of(*v)).collect::<Result<_, _>>()?,
            results: ft.results().iter().map(|v| val_of(*v)).collect::<Result<_, _>>()?,
        }),
        _ => Ok(WasmSig { params: Vec::new(), results: Vec::new() }),
    }
}

/// The raw sections a signature table is built from.
#[derive(Default)]
struct Sections {
    types: Vec<WasmSig>,
    /// The type index of every function in index order (imports first).
    func_types: Vec<u32>,
    imports: Vec<(String, String, u32)>,
    exports: Vec<(String, u32)>,
}

fn collect_sections(bytes: &[u8]) -> Result<Sections, String> {
    use wasmparser::{ExternalKind, Parser, Payload, TypeRef};
    let mut s = Sections::default();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            Payload::TypeSection(reader) => {
                for group in reader {
                    for sub in group.map_err(|e| e.to_string())?.into_types() {
                        s.types.push(func_sig_of(&sub)?);
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for (_, imp) in reader.into_iter().flatten().flat_map(|g| g.into_iter().flatten()) {
                    if let TypeRef::Func(t) = imp.ty {
                        s.func_types.push(t);
                        s.imports.push((imp.module.to_string(), imp.name.to_string(), t));
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for t in reader {
                    s.func_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            Payload::ExportSection(reader) => {
                for e in reader {
                    let e = e.map_err(|e| e.to_string())?;
                    if e.kind == ExternalKind::Func {
                        s.exports.push((e.name.to_string(), e.index));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(s)
}

fn wasm_sigs(bytes: &[u8]) -> Result<WasmSigs, String> {
    let s = collect_sections(bytes)?;
    let sig_at = |t: u32| -> Result<WasmSig, String> {
        let sig = s.types.get(t as usize).ok_or_else(|| format!("type index {t} out of range"))?;
        Ok(WasmSig { params: sig.params.clone(), results: sig.results.clone() })
    };
    let imports = s.imports.iter().map(|(m, n, t)| sig_at(*t).map(|sig| (m.clone(), n.clone(), sig))).collect::<Result<Vec<_>, _>>()?;
    let mut exports = BTreeMap::new();
    for (name, idx) in &s.exports {
        let t = *s.func_types.get(*idx as usize).ok_or_else(|| format!("export `{name}` names function {idx} outside the index space"))?;
        exports.insert(name.clone(), sig_at(t)?);
    }
    Ok(WasmSigs { imports, exports })
}

/// What an extern's signature may carry.
const EXTERN_SET: &str = "an @extern(wasm, ...) import carries Int, Float, Bool, String and Unit (#2265), and an `effect fn` (or `Result[T, String]`) one of those returns a hook's throw as an err (#3356)";
/// What an export's signature may carry (#3354).
const EXPORT_SET: &str = "an export carries Int, Float, Bool, String, Unit, Bytes, List[T], Option[T] and records (#3354); variants, maps, sets, tuples and functions are not marshalled";

/// The refusal for a type the host cannot marshal. The type is printed as
/// it is spelled in source. An unmarked `fn` is public, so the fix for a
/// helper is `local fn`, not dropping a `pub` it may not have (#3360).
fn refuse(fn_name: &str, what: &str, ty: &Ty, set: &str) -> String {
    let ty = ty.display();
    let hint = if set == EXTERN_SET {
        format!("pass `{fn_name}`'s value across the boundary as one of those types (a String carrying JSON, for example)")
    } else {
        format!("an unmarked or `pub` fn is public and becomes a JS export; write `local fn {fn_name}` (or `mod fn`) to keep it internal, or wrap it in a pub fn over the marshalled types")
    };
    format!("error: --host js cannot marshal {what} of `{fn_name}`: `{ty}` — {set}\n  hint: {hint}")
}

/// The JS expression converting `expr` (a JS value) into the wasm slot `v`
/// for marshal kind `m`.
fn to_wasm(m: Marshal, v: Val, expr: &str, what: &str) -> String {
    match (m, v) {
        (Marshal::Int, Val::I64) => format!("toI64({expr}, \"{what}\")"),
        (Marshal::Float, Val::F64) => format!("Number({expr})"),
        (Marshal::Bool, Val::I32) => format!("({expr} ? 1 : 0)"),
        (Marshal::Bool, Val::I64) => format!("({expr} ? 1n : 0n)"),
        (Marshal::Str, Val::I32) => format!("allocString({expr})"),
        _ => format!("/* unmarshallable {m:?} as {v:?} */ {expr}"),
    }
}

/// The JS expression converting the wasm slot value `expr` of valtype `v`
/// into the JS value for marshal kind `m`. `take` releases a String block.
fn from_wasm(m: Marshal, v: Val, expr: &str, what: &str, take: bool) -> String {
    match (m, v) {
        (Marshal::Int, Val::I64) => format!("fromI64({expr}, \"{what}\")"),
        (Marshal::Float, Val::F64) => expr.to_string(),
        (Marshal::Bool, Val::I32) => format!("({expr} !== 0)"),
        (Marshal::Bool, Val::I64) => format!("({expr} !== 0n)"),
        (Marshal::Str, Val::I32) if take => format!("takeString({expr})"),
        (Marshal::Str, Val::I32) => format!("readString({expr})"),
        _ => format!("/* unmarshallable {m:?} from {v:?} */ {expr}"),
    }
}

/// The WASI shims the host knows, by import name. Only the ones the shipped
/// module names are emitted (#2276); an import outside this table is a
/// build-time refusal rather than a `LinkError` in the page.
const WASI_SHIMS: &[(&str, &str)] = &[
    ("fd_write", r#"  fd_write(fd, iovs, iovsLen, nwritten) {
    const v = view();
    let total = 0;
    const parts = [];
    for (let i = 0; i < iovsLen; i++) {
      const ptr = v.getUint32(iovs + 8 * i, true);
      const len = v.getUint32(iovs + 8 * i + 4, true);
      parts.push(bytes().slice(ptr, ptr + len));
      total += len;
    }
    if (fd !== 1 && fd !== 2) return 8; // EBADF
    const chunk = new Uint8Array(total);
    let off = 0;
    for (const p of parts) { chunk.set(p, off); off += p.length; }
    emit(fd, chunk);
    view().setUint32(nwritten, total, true);
    return 0;
  }"#),
    ("proc_exit", r#"  proc_exit(code) { flush(); throw new AlmideExit(code); }"#),
    ("fd_read", r#"  fd_read(fd, iovs, iovsLen, nread) { view().setUint32(nread, 0, true); return 0; }"#),
    ("random_get", r#"  random_get(ptr, len) { globalThis.crypto.getRandomValues(bytes().subarray(ptr, ptr + len)); return 0; }"#),
    ("clock_time_get", r#"  clock_time_get(id, precision, out) { view().setBigUint64(out, BigInt(Date.now()) * 1000000n, true); return 0; }"#),
    ("args_sizes_get", r#"  args_sizes_get(argc, bufSize) { const v = view(); v.setUint32(argc, 0, true); v.setUint32(bufSize, 0, true); return 0; }"#),
    ("args_get", r#"  args_get() { return 0; }"#),
    ("environ_sizes_get", r#"  environ_sizes_get(count, bufSize) { const v = view(); v.setUint32(count, 0, true); v.setUint32(bufSize, 0, true); return 0; }"#),
    ("environ_get", r#"  environ_get() { return 0; }"#),
    ("fd_close", r#"  fd_close() { return 0; }"#),
    ("fd_fdstat_get", r#"  fd_fdstat_get() { return 8; }"#),
    ("fd_seek", r#"  fd_seek() { return 8; }"#),
    ("fd_prestat_get", r#"  fd_prestat_get() { return 8; }"#),
    ("fd_prestat_dir_name", r#"  fd_prestat_dir_name() { return 8; }"#),
    ("fd_filestat_get", r#"  fd_filestat_get() { return 8; }"#),
    ("fd_readdir", r#"  fd_readdir() { return 8; }"#),
    ("path_open", r#"  // ENOENT: the host has no filesystem
  path_open() { return 44; }"#),
    ("path_filestat_get", r#"  path_filestat_get() { return 44; }"#),
    ("path_create_directory", r#"  path_create_directory() { return 44; }"#),
    ("path_remove_directory", r#"  path_remove_directory() { return 44; }"#),
    ("path_unlink_file", r#"  path_unlink_file() { return 44; }"#),
    ("sched_yield", r#"  sched_yield() { return 0; }"#),
    ("poll_oneoff", r#"  // ENOSYS
  poll_oneoff() { return 52; }"#),
];

/// Every type on the boundary is marshallable, or the build refuses: an
/// extern's by the scalar-and-String set, an export's by the wider set its
/// wrapper carries (#3354) — the module's record then decides the layout.
fn check_marshallable(surface: &HostSurface) -> Result<(), String> {
    for e in &surface.externs {
        let f = &e.sig;
        for (p, ty) in &f.params {
            if marshal_of(ty).is_none() {
                return Err(refuse(&f.name, &format!("parameter `{p}`"), ty, EXTERN_SET));
            }
        }
        // A fallible extern (#3356) returns its `T` to the hook's caller.
        if marshal_of(visible_ret(f)).is_none() {
            return Err(refuse(&f.name, "the return type", &f.ret, EXTERN_SET));
        }
    }
    for f in &surface.exports {
        for (p, ty) in &f.params {
            if !exports::expressible(ty) {
                return Err(refuse(&f.name, &format!("parameter `{p}`"), ty, EXPORT_SET));
            }
        }
        if !exports::expressible(visible_ret(f)) {
            return Err(refuse(&f.name, "the return type", &f.ret, EXPORT_SET));
        }
    }
    Ok(())
}

/// The `wasi` object: one shim per `wasi_snapshot_preview1` import the
/// shipped module names, in table order, and nothing for the rest (#2276).
fn wasi_object_js(sigs: &WasmSigs) -> String {
    let mut js = String::from("const wasi = {\n");
    for (name, body) in WASI_SHIMS {
        if sigs.imports.iter().any(|(m, n, _)| m == "wasi_snapshot_preview1" && n == name) {
            js.push_str(body);
            js.push_str(",\n");
        }
    }
    js.push_str("};\n");
    js
}

/// Every import the module names has a shim or a declared extern.
fn check_imports_served(sigs: &WasmSigs, surface: &HostSurface) -> Result<(), String> {
    for (module, name, _) in &sigs.imports {
        let known = (module == "wasi_snapshot_preview1" && WASI_SHIMS.iter().any(|(n, _)| n == name))
            || surface.externs.iter().any(|e| &e.module == module && &e.import == name);
        if !known {
            return Err(format!("error: --host js has no shim for the import `{module}.{name}` the module names — declare it with @extern(wasm, \"{module}\", \"{name}\") or file an issue naming the program shape"));
        }
    }
    Ok(())
}

/// The `imports()` function: the WASI shims the module names and one
/// marshalling closure per extern import.

fn signature_dts(f: &HostFn) -> String {
    let params: Vec<String> = f.params.iter().map(|(p, ty)| format!("{p}: {}", ts_of(marshal_of(ty).expect("checked")))).collect();
    format!("({}) => {}", params.join(", "), ts_of(marshal_of(visible_ret(f)).expect("checked")))
}

/// The `Hooks` interface's `js` member: one entry per extern import.
fn hooks_dts(surface: &HostSurface) -> String {
    if surface.externs.is_empty() {
        return "  /** The program declares no `@extern(wasm, ...)` import. */\n  js?: Record<string, never>;\n}\n\n".to_string();
    }
    let rows: Vec<String> = surface
        .externs
        .iter()
        .map(|e| {
            let sig = signature_dts(&e.sig);
            // An async import's hook may return the value or a Promise of it (#3353).
            let sig = match sig.rsplit_once(" => ") {
                Some((params, ret)) if surface.async_imports.contains(&e.import) => format!("{params} => {ret} | Promise<{ret}>"),
                _ => sig,
            };
            format!("    {}: {};", e.import, sig)
        })
        .collect();
    format!("  /** The `@extern(wasm, \"js\", ...)` imports the program declares. */\n  js: {{\n{}\n  }};\n}}\n\n", rows.join("\n"))
}

const RUN_JS: &str = "\n/** Run `main` (the module's `_start`); a non-zero exit throws AlmideExit. */\nexport function run() {\n  ready();\n  try {\n    instance.exports._start();\n  } catch (e) {\n    if (e instanceof AlmideExit && e.code === 0) return;\n    throw e;\n  } finally {\n    flush();\n  }\n}\n";

/// What the emitter recorded about each export (#2265, #3352, #3354), keyed
/// by export name: which params the callee owns, and the boundary layout of
/// its params and return.
pub(crate) struct ExportNotes {
    pub owned: BTreeMap<String, Vec<bool>>,
    pub params: BTreeMap<String, Vec<AbiShape>>,
    pub rets: BTreeMap<String, ExportRet>,
    /// The return ABI of each declared import, by (module, name) (#3356).
    pub imports: ImportRets,
}

pub(crate) type ImportRets = BTreeMap<(String, String), ExportRet>;

/// The generated host: `(js, d_ts)`.
pub(crate) fn generate(
    wasm_name: &str,
    source_file: &str,
    bytes: &[u8],
    surface: &HostSurface,
    notes: &ExportNotes,
) -> Result<(String, String), String> {
    let ExportNotes { owned: export_param_owned, params: export_params, rets: export_rets, imports: import_rets } = notes;
    let sigs = wasm_sigs(bytes)?;
    check_marshallable(surface)?;
    check_imports_served(&sigs, surface)?;
    let wrapped = surface.exports.iter().map(|f| f.name.clone()).chain(std::iter::once("_start".to_string())).collect();
    let suspension = jspi::analyse(bytes, &surface.async_imports, &wrapped)?;

    let version = env!("CARGO_PKG_VERSION");
    let alloc_body = "  const h = instance.exports.__alloc(b.length); // the module sets rc = 1, len, cap";
    let banner = format!("// Generated by `almide build {source_file} --target wasm --host js` (almide {version}, structural leg).\n// Do not edit: change the .almd and rebuild.\n");

    let mut js = banner.clone();
    js.push_str(&format!("const WASM_URL = new URL(\"./{wasm_name}\", import.meta.url);\n"));
    // Every wrapper is planned first: whether the value helpers ship
    // depends on whether any wrapper calls them.
    let mut wrappers = String::new();
    let mut export_dts = String::new();
    let mut needs_values = false;
    for f in &surface.exports {
        // The structural leg exports a pub fn only when its whole call
        // closure lowers; a fn the module does not export gets no wrapper.
        let Some(sig) = sigs.exports.get(&f.name) else { continue };
        let owned = export_param_owned.get(&f.name).map(Vec::as_slice).unwrap_or(&[]);
        let plan = exports::plan_export(f, export_params.get(&f.name).map(Vec::as_slice), export_rets.get(&f.name))?;
        needs_values |= plan.needs_values();
        let entry = suspension.entry(&f.name);
        wrappers.push_str(&exports::wrapper_js(f, sig, owned, &plan, entry)?);
        export_dts.push_str(&exports::export_dts(f, &plan, entry));
    }
    // A fallible import (#3356) builds its Result block with the value helpers.
    needs_values |= surface.externs.iter().any(|e| returns_result(&e.sig));
    let mut string_helpers = if surface.needs_string_abi() { JS_STRING_HELPERS.replace("{ALLOC_BODY}", alloc_body) } else { String::new() };
    if !surface.externs.is_empty() {
        string_helpers.push_str(JS_ABANDON);
    }
    if surface.externs.iter().any(|e| !suspension.imports.contains(&e.import)) {
        string_helpers.push_str(JS_SYNC);
    }
    if needs_values {
        string_helpers.push_str(JS_VALUE_HELPERS);
    }
    if suspension.active() {
        string_helpers.push_str(jspi::JS_ASYNC_RUNTIME);
    }
    let runtime = JS_RUNTIME
        .replace("{STRING_HELPERS}", &string_helpers)
        .replace("{WASI_OBJECT}", &wasi_object_js(&sigs))
        .replace("{JSPI_CHECK}", &jspi::init_check(&suspension))
        .replace("{JSPI_PROMISED}", &jspi::init_promised(&suspension));
    js.push_str(&runtime);
    js.push_str(&imports::import_object_js(&sigs, surface, &suspension, import_rets)?);

    let mut dts = format!("// Generated by `almide build {source_file} --target wasm --host js` (almide {version}).\n\n");
    dts.push_str(DTS_RUNTIME);
    if needs_values {
        dts.push_str(DTS_VALUES);
    }
    dts.push_str(&hooks_dts(surface));
    dts.push_str("export function init(source?: WasmSource, hooks?: Hooks): Promise<void>;\n");
    if surface.has_main && sigs.exports.contains_key("_start") {
        if suspension.exports.contains("_start") {
            js.push_str(jspi::RUN_ASYNC_JS);
            dts.push_str("/** Run `main`, which awaits async imports; a non-zero exit code rejects with `AlmideExit`. */\nexport function run(): Promise<void>;\n");
        } else {
            // With async imports present, a synchronous entry refuses while
            // an async call is suspended (#3353).
            let guard = if suspension.active() { "  ready();\n  idle(\"run\");\n" } else { "  ready();\n" };
            js.push_str(&RUN_JS.replacen("  ready();\n", guard, 1));
            dts.push_str("/** Run `main`; a non-zero exit code throws `AlmideExit`. */\nexport function run(): void;\n");
        }
    }
    js.push_str(&wrappers);
    dts.push_str(&export_dts);
    Ok((js, dts))
}

const JS_RUNTIME: &str = r#"const INT_LIMIT = 9007199254740992n;

/** Thrown by `proc_exit`: the program ended with `code`. */
export class AlmideExit extends Error {
  constructor(code) {
    super(`almide: exit ${code}`);
    this.name = "AlmideExit";
    this.code = code;
  }
}

let instance = null;
let memory = null;
let hooks = {};
const lineBuf = { 1: "", 2: "" };
const decoder = new TextDecoder("utf-8");
const encoder = new TextEncoder();

function bytes() { return new Uint8Array(memory.buffer); }
function view() { return new DataView(memory.buffer); }
let abandoned = null;
function ready() {
  if (instance === null) throw new Error("almide: call init() before using the module");
  if (abandoned !== null) throw new Error(abandoned);
}

{STRING_HELPERS}function toI64(x, what) {
  if (typeof x === "bigint") return x;
  if (!Number.isSafeInteger(x)) throw new RangeError(`almide: ${what} takes an Int (an integer within ±2^53), got ${x}`);
  return BigInt(x);
}
function fromI64(b, what) {
  if (b > INT_LIMIT || b < -INT_LIMIT) throw new RangeError(`almide: ${what} produced an Int outside ±2^53: ${b}n — pass a BigInt-aware hook to keep it exact`);
  return Number(b);
}
function hook(module, name) {
  const table = hooks[module] || {};
  const f = table[name];
  if (typeof f !== "function") throw new Error(`almide: init() needs hooks.${module}.${name} for @extern(wasm, "${module}", "${name}")`);
  return f;
}
function emit(fd, chunk) {
  const custom = fd === 2 ? hooks.stderr : hooks.stdout;
  if (custom) { custom(chunk); return; }
  if (typeof process !== "undefined" && process.stdout && typeof process.stdout.write === "function") {
    (fd === 2 ? process.stderr : process.stdout).write(chunk);
    return;
  }
  lineBuf[fd] += decoder.decode(chunk, { stream: true });
  let nl;
  while ((nl = lineBuf[fd].indexOf("\n")) >= 0) {
    (fd === 2 ? console.error : console.log)(lineBuf[fd].slice(0, nl));
    lineBuf[fd] = lineBuf[fd].slice(nl + 1);
  }
}
function flush() {
  for (const fd of [1, 2]) {
    if (lineBuf[fd] !== "") { (fd === 2 ? console.error : console.log)(lineBuf[fd]); lineBuf[fd] = ""; }
  }
}
{WASI_OBJECT}
/**
 * Compile and instantiate the module. `source` may be omitted (the .wasm
 * next to this file is loaded: `fs.readFile` under node, `fetch` in a page),
 * or be bytes, a `Response`, a promise of either, or a compiled
 * `WebAssembly.Module`. `hooks.js` carries the `@extern(wasm, "js", ...)`
 * functions; `hooks.stdout` / `hooks.stderr` receive raw `Uint8Array`
 * chunks (default: `process.stdout` under node, `console.log` per line in a page).
 */
export async function init(source, h = {}) {
{JSPI_CHECK}  hooks = h;
  abandoned = null;
  let module;
  if (source instanceof WebAssembly.Module) {
    module = source;
  } else {
    let data = await source;
    if (data == null) {
      if (typeof process !== "undefined" && process.versions && process.versions.node) {
        const fs = await import("node:fs/promises");
        data = await fs.readFile(WASM_URL);
      } else {
        data = await fetch(WASM_URL);
      }
    }
    if (typeof Response !== "undefined" && data instanceof Response) data = await data.arrayBuffer();
    module = await WebAssembly.compile(data);
  }
  instance = await WebAssembly.instantiate(module, imports());
  memory = instance.exports.memory;
{JSPI_PROMISED}}
"#;

/// The String marshalling helpers: shipped only for a surface that
/// marshals a String (#2276).
const JS_STRING_HELPERS: &str = r#"const PAYLOAD = 12;
function readString(h) {
  const len = view().getUint32(h + 4, true);
  return decoder.decode(bytes().subarray(h + PAYLOAD, h + PAYLOAD + len));
}
function takeString(h) {
  const s = readString(h);
  instance.exports.__release(h);
  return s;
}
function allocString(s) {
  const b = encoder.encode(String(s));
{ALLOC_BODY}
  bytes().set(b, h + PAYLOAD);
  return h;
}
"#;

/// A throw out of an infallible hook (#3356): the instance is abandoned —
/// its unwound frames kept their blocks, so no later call may run on it —
/// and the thrown error names the import and the fix. Shipped when the
/// program declares an extern.
///
/// `UnmarkedPromise` is the refusal [`JS_SYNC`] throws, passed through here.
const JS_ABANDON: &str = r#"class UnmarkedPromise extends Error {}
function abandon(module, name, e) {
  if (e instanceof UnmarkedPromise) return e;
  abandoned = `almide: the instance was abandoned after hooks.${module}.${name} threw — call init() again`;
  return new Error(`almide: hooks.${module}.${name} threw, but its @extern is infallible, so the call cannot return an err and the instance is abandoned; declare it \`effect fn\` (or returning Result[T, String]) to receive a throw as an err: ${e instanceof Error ? e.message : String(e)}`, { cause: e });
}
"#;

/// `sync()` (#3371): the one check a SYNC hook's answer goes through. A
/// thenable means the hook is async but its @extern is not marked
/// `returns: promise`, so the call cannot wait for it. The refusal names the
/// fix and abandons the instance (the throw unwinds the module's frames as
/// any hook throw does); the dropped promise gets a no-op rejection handler
/// so its own failure is not a second, unhandled one. Shipped when some
/// extern is unmarked.
const JS_SYNC: &str = r#"function sync(module, name, r) {
  if (typeof r?.then !== "function") return r;
  Promise.resolve(r).catch(() => {});
  abandoned = `almide: the instance was abandoned after hooks.${module}.${name} returned a Promise — call init() again`;
  throw new UnmarkedPromise(`almide: hooks.${module}.${name} returned a Promise; mark its @extern with returns: promise`);
}
"#;

/// The boundary value helpers (#3352, #3354): `AlmideError`, the Result
/// unwrapping, and the block readers/builders every non-scalar export shape
/// goes through. Shipped only when some wrapper calls them.
const JS_VALUE_HELPERS: &str = include_str!("js_host_values.js");

const DTS_VALUES: &str = r#"/** Thrown by an exported effect fn that returned err: `message` is its error. */
export class AlmideError extends Error {}
/** The module's linear memory size in bytes (stays flat when nothing leaks). */
export function memoryBytes(): number;

"#;

const DTS_RUNTIME: &str = r#"export type WasmSource =
  | BufferSource
  | Response
  | WebAssembly.Module
  | Promise<BufferSource | Response>;

/** Thrown by the program's `proc_exit`: `code` is the exit code. */
export class AlmideExit extends Error {
  code: number;
}

export interface Hooks {
  /** Raw stdout chunks; default: `process.stdout` under node, `console.log` per line in a page. */
  stdout?: (chunk: Uint8Array) => void;
  /** Raw stderr chunks; default: `process.stderr` / `console.error`. */
  stderr?: (chunk: Uint8Array) => void;
"#;

#[cfg(test)]
#[path = "js_host_tests.rs"]
mod tests;
